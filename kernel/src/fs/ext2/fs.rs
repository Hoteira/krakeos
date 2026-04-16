use crate::sync::{YieldMutex, YieldRwLock};
use alloc::sync::Arc;
use alloc::collections::BTreeMap;
use alloc::vec;
#[allow(dead_code)]
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::mem::size_of;

use crate::fs::disk;
use crate::fs::ext2::structs::{BlockGroupDescriptor, Inode, Superblock};

// 4-way set-associative sector cache.
// 1024 sets × 4 ways = 4096 entries × 512 bytes = 2MB capacity.
// O(4) lookup and O(1) eviction vs O(log n) for BTreeMap.
// Significant speedup for large sequential reads (WASM AOT compilation).
const CACHE_SETS: usize = 1024;
const CACHE_WAYS: usize = 4;

struct SectorHashCache {
    tags: [[u64; CACHE_WAYS]; CACHE_SETS],      // LBA per way (32 KB)
    flags: [[u8; CACHE_WAYS]; CACHE_SETS],      // bit0=valid, bit1=dirty (4 KB)
    age:   [[u8; CACHE_WAYS]; CACHE_SETS],      // LRU age: higher = older (4 KB)
    data:  alloc::vec::Vec<u8>,                 // 4096 × 512 bytes, heap-allocated
}

impl SectorHashCache {
    fn new() -> Self {
        let mut data = alloc::vec::Vec::new();
        data.resize(CACHE_SETS * CACHE_WAYS * 512, 0u8);
        SectorHashCache {
            tags: [[0u64; CACHE_WAYS]; CACHE_SETS],
            flags: [[0u8; CACHE_WAYS]; CACHE_SETS],
            age:   [[0u8; CACHE_WAYS]; CACHE_SETS],
            data,
        }
    }

    #[inline(always)]
    fn set_idx(lba: u64) -> usize { (lba as usize) & (CACHE_SETS - 1) }

    #[inline(always)]
    fn data_range(set: usize, way: usize) -> core::ops::Range<usize> {
        let base = (set * CACHE_WAYS + way) * 512;
        base..base + 512
    }

    fn get(&self, lba: u64) -> Option<&[u8]> {
        let s = Self::set_idx(lba);
        for w in 0..CACHE_WAYS {
            if self.flags[s][w] & 1 != 0 && self.tags[s][w] == lba {
                return Some(&self.data[Self::data_range(s, w)]);
            }
        }
        None
    }

    /// Insert a sector. Returns evicted (lba, 512-byte copy) only when the evicted
    /// way was dirty — the caller must write it back to disk.
    fn insert(&mut self, lba: u64, new_data: &[u8], mark_dirty: bool) -> Option<(u64, [u8; 512])> {
        let s = Self::set_idx(lba);

        // Check if already present — just update.
        for w in 0..CACHE_WAYS {
            if self.flags[s][w] & 1 != 0 && self.tags[s][w] == lba {
                let r = Self::data_range(s, w);
                self.data[r].copy_from_slice(new_data);
                if mark_dirty { self.flags[s][w] |= 2; }
                self.touch(s, w);
                return None;
            }
        }

        // Find an empty way first.
        for w in 0..CACHE_WAYS {
            if self.flags[s][w] & 1 == 0 {
                self.tags[s][w] = lba;
                self.flags[s][w] = 1 | if mark_dirty { 2 } else { 0 };
                let r = Self::data_range(s, w);
                self.data[r].copy_from_slice(new_data);
                self.touch(s, w);
                return None;
            }
        }

        // All ways occupied — evict LRU.
        let victim = self.find_lru(s);
        let evicted_lba = self.tags[s][victim];
        let evicted_dirty = self.flags[s][victim] & 2 != 0;
        let eviction = if evicted_dirty {
            let mut buf = [0u8; 512];
            buf.copy_from_slice(&self.data[Self::data_range(s, victim)]);
            Some((evicted_lba, buf))
        } else {
            None
        };

        self.tags[s][victim] = lba;
        self.flags[s][victim] = 1 | if mark_dirty { 2 } else { 0 };
        let r = Self::data_range(s, victim);
        self.data[r].copy_from_slice(new_data);
        self.touch(s, victim);
        eviction
    }

    /// Collect all dirty entries and clear dirty bits; returns (lba, data) pairs.
    fn drain_dirty(&mut self) -> alloc::vec::Vec<(u64, [u8; 512])> {
        let mut out = alloc::vec::Vec::new();
        for s in 0..CACHE_SETS {
            for w in 0..CACHE_WAYS {
                if self.flags[s][w] & 3 == 3 {  // valid AND dirty
                    let mut buf = [0u8; 512];
                    buf.copy_from_slice(&self.data[Self::data_range(s, w)]);
                    out.push((self.tags[s][w], buf));
                    self.flags[s][w] &= !2;  // clear dirty
                }
            }
        }
        out
    }

    fn is_dirty_empty(&self) -> bool {
        for s in 0..CACHE_SETS {
            for w in 0..CACHE_WAYS {
                if self.flags[s][w] & 3 == 3 { return false; }
            }
        }
        true
    }

    fn touch(&mut self, s: usize, used: usize) {
        for w in 0..CACHE_WAYS {
            if self.flags[s][w] & 1 != 0 {
                self.age[s][w] = self.age[s][w].saturating_add(1);
            }
        }
        self.age[s][used] = 0;
    }

    fn find_lru(&self, s: usize) -> usize {
        let (mut max_age, mut victim) = (0u8, 0usize);
        for w in 0..CACHE_WAYS {
            if self.age[s][w] >= max_age {
                max_age = self.age[s][w];
                victim = w;
            }
        }
        victim
    }
}

pub struct Ext2 {
    disk_id: u8,
    base_lba: u64,
    pub superblock: YieldMutex<Superblock>,
    block_size: u64,
    inodes_per_group: u32,
    inode_size: u16,
    cache: YieldMutex<SectorHashCache>,
    pub metadata_lock: YieldMutex<()>,
    inode_locks: YieldMutex<BTreeMap<u32, Arc<YieldRwLock<()>>>>,
    pub bg_descriptors: YieldRwLock<Vec<BlockGroupDescriptor>>,
}

impl Ext2 {
    pub fn get_inode_lock(&self, inode: u32) -> Arc<YieldRwLock<()>> {
        let mut locks = self.inode_locks.lock();
        if let Some(lock) = locks.get(&inode) {
            lock.clone()
        } else {
            let lock = Arc::new(YieldRwLock::new(()));
            locks.insert(inode, lock.clone());
            lock
        }
    }

    pub fn new(disk_id: u8, base_lba: u64) -> Result<Box<Self>, String> {
        let mut superblock = unsafe { core::mem::zeroed::<Superblock>() };
        let mut buf = [0u8; 1024];

        crate::debugln!("Ext2: Reading superblock...");
        disk::read(base_lba + 2, disk_id, &mut buf[0..512]);
        disk::read(base_lba + 3, disk_id, &mut buf[512..1024]);
        crate::debugln!("Ext2: Superblock read.");

        unsafe {
            core::ptr::copy_nonoverlapping(
                buf.as_ptr(),
                &mut superblock as *mut _ as *mut u8,
                size_of::<Superblock>(),
            );
        }

        let magic = superblock.s_magic;
        crate::debugln!("Ext2: Magic: {:#x}", magic);

        if magic != 0xEF53 {
            return Err(alloc::format!(
                "Invalid Ext2 Magic: {:#x} (Expected 0xEF53).",
                magic
            ));
        }

        let block_size = 1024 << superblock.log_block_size;
        let inode_size = if superblock.rev_level >= 1 {
            superblock.inode_size
        } else {
            128
        };

        let blocks_count = superblock.blocks_count;
        let blocks_per_group = superblock.blocks_per_group;
        let groups_count = (blocks_count + blocks_per_group - 1) / blocks_per_group;
        let mut descriptors = Vec::with_capacity(groups_count as usize);

        let bgdt_start_block = if block_size == 1024 { 2 } else { 1 };
        
        for i in 0..groups_count {
            let block = bgdt_start_block + (i as u64 * size_of::<BlockGroupDescriptor>() as u64) / block_size as u64;
            let offset_in_block = (i as u64 * size_of::<BlockGroupDescriptor>() as u64) % block_size as u64;
            
            let mut b = vec![0u8; block_size as usize];
            disk::read(base_lba + (block * (block_size as u64 / 512)), disk_id, &mut b[0..512]);
            if block_size > 512 {
                disk::read(base_lba + (block * (block_size as u64 / 512)) + 1, disk_id, &mut b[512..1024]);
            }

            let mut desc = unsafe { core::mem::zeroed::<BlockGroupDescriptor>() };
            unsafe {
                core::ptr::copy_nonoverlapping(
                    b.as_ptr().add(offset_in_block as usize),
                    &mut desc as *mut _ as *mut u8,
                    size_of::<BlockGroupDescriptor>(),
                );
            }
            descriptors.push(desc);
        }

        Ok(Box::new(Ext2 {
            disk_id,
            base_lba,
            superblock: YieldMutex::new(superblock),
            block_size: block_size as u64,
            inodes_per_group: superblock.inodes_per_group,
            inode_size,
            cache: YieldMutex::new(SectorHashCache::new()),
            metadata_lock: YieldMutex::new(()),
            inode_locks: YieldMutex::new(BTreeMap::new()),
            bg_descriptors: YieldRwLock::new(descriptors),
        }))
    }
}

unsafe impl Send for Ext2 {}
unsafe impl Sync for Ext2 {}

impl Ext2 {
    fn read_disk_data(&self, offset: u64, buffer: &mut [u8]) {
        let abs_offset = offset + (self.base_lba * 512);
        let start_lba = abs_offset / 512;
        let offset_in_sector = (abs_offset % 512) as usize;

        if offset_in_sector == 0 && (buffer.len() % 512) == 0 && buffer.len() >= 512 {
            disk::read(start_lba, self.disk_id, buffer);
            let num_sectors = (buffer.len() / 512) as u64;
            let mut cache = self.cache.lock();
            for i in 0..num_sectors {
                let lba = start_lba + i;
                let buf_start = (i * 512) as usize;
                if cache.get(lba).is_none() {
                    cache.insert(lba, &buffer[buf_start..buf_start + 512], false);
                }
            }
            return;
        }

        let mut current_lba = start_lba;
        let mut bytes_read = 0;
        let total_bytes = buffer.len();

        while bytes_read < total_bytes {
            let start_index = if current_lba == start_lba { offset_in_sector } else { 0 };
            let remaining = 512 - start_index;
            let to_copy = core::cmp::min(total_bytes - bytes_read, remaining);

            let hit = {
                let cache = self.cache.lock();
                if let Some(cached) = cache.get(current_lba) {
                    buffer[bytes_read..bytes_read + to_copy]
                        .copy_from_slice(&cached[start_index..start_index + to_copy]);
                    true
                } else {
                    false
                }
            };

            if !hit {
                let mut temp_buf = [0u8; 512];
                disk::read(current_lba, self.disk_id, &mut temp_buf);
                let eviction = {
                    let mut cache = self.cache.lock();
                    cache.insert(current_lba, &temp_buf, false)
                };
                if let Some((evict_lba, evict_data)) = eviction {
                    disk::write(evict_lba, self.disk_id, &evict_data);
                }
                buffer[bytes_read..bytes_read + to_copy]
                    .copy_from_slice(&temp_buf[start_index..start_index + to_copy]);
            }

            bytes_read += to_copy;
            current_lba += 1;
        }
    }

    fn write_disk_data(&self, offset: u64, buffer: &[u8]) {
        let abs_offset = offset + (self.base_lba * 512);
        let start_lba = abs_offset / 512;
        let offset_in_sector = (abs_offset % 512) as usize;

        let mut current_lba = start_lba;
        let mut bytes_written = 0;
        let total_bytes = buffer.len();

        while bytes_written < total_bytes {
            let mut temp_buf = [0u8; 512];
            let start_index = if current_lba == start_lba { offset_in_sector } else { 0 };
            let remaining = 512 - start_index;
            let to_copy = core::cmp::min(total_bytes - bytes_written, remaining);

            if to_copy < 512 {
                let hit = {
                    let cache = self.cache.lock();
                    if let Some(cached) = cache.get(current_lba) {
                        temp_buf.copy_from_slice(cached);
                        true
                    } else {
                        false
                    }
                };
                if !hit {
                    disk::read(current_lba, self.disk_id, &mut temp_buf);
                }
            }

            temp_buf[start_index..start_index + to_copy]
                .copy_from_slice(&buffer[bytes_written..bytes_written + to_copy]);

            let eviction = {
                let mut cache = self.cache.lock();
                cache.insert(current_lba, &temp_buf, true)
            };

            if let Some((evict_lba, evict_data)) = eviction {
                disk::write(evict_lba, self.disk_id, &evict_data);
            }

            bytes_written += to_copy;
            current_lba += 1;
        }
    }

    pub fn flush(&self) {
        let mut dirty = {
            let mut cache = self.cache.lock();
            if cache.is_dirty_empty() { return; }
            cache.drain_dirty()
        };
        
        dirty.sort_unstable_by_key(|(lba, _)| *lba);

        if dirty.is_empty() { return; }

        let mut run_start = dirty[0].0;
        let mut run_data: Vec<u8> = Vec::new();
        run_data.extend_from_slice(&dirty[0].1);

        for i in 1..dirty.len() {
            let (lba, ref data) = dirty[i];
            if lba == run_start + (run_data.len() / 512) as u64 {
                run_data.extend_from_slice(data);
            } else {
                disk::write(run_start, self.disk_id, &run_data);
                run_start = lba;
                run_data.clear();
                run_data.extend_from_slice(data);
            }
        }
        if !run_data.is_empty() {
            disk::write(run_start, self.disk_id, &run_data);
        }
    }

    pub fn read_block_group_descriptor(&self, group_idx: u32) -> BlockGroupDescriptor {
        self.bg_descriptors.read()[group_idx as usize]
    }

    pub fn write_block_group_descriptor(&self, group_idx: u32, desc: &BlockGroupDescriptor) {
        {
            let mut descriptors = self.bg_descriptors.write();
            descriptors[group_idx as usize] = *desc;
        }

        let bgdt_start_block = if self.block_size == 1024 { 2 } else { 1 };
        let desc_size = size_of::<BlockGroupDescriptor>() as u64;
        let offset = (bgdt_start_block as u64 * self.block_size) + (group_idx as u64 * desc_size);

        let ptr = desc as *const BlockGroupDescriptor as *const u8;
        let slice = unsafe { core::slice::from_raw_parts(ptr, size_of::<BlockGroupDescriptor>()) };
        self.write_disk_data(offset, slice);
    }

    pub fn write_superblock(&self) {
        let sb_val = {
            let _lock = self.metadata_lock.lock();
            let sb = self.superblock.lock();
            *sb
        };
        let offset = 1024;
        let ptr = &sb_val as *const Superblock as *const u8;
        let slice = unsafe { core::slice::from_raw_parts(ptr, size_of::<Superblock>()) };
        self.write_disk_data(offset, slice);
    }

    pub fn read_inode(&self, inode_idx: u32) -> Inode {
        let (inode_offset, inode_size) = {
            let group = (inode_idx - 1) / self.inodes_per_group;
            let index_in_group = (inode_idx - 1) % self.inodes_per_group;
            let bg_desc = self.read_block_group_descriptor(group);
            let inode_table_offset = bg_desc.inode_table as u64 * self.block_size;
            (inode_table_offset + (index_in_group as u64 * self.inode_size as u64), self.inode_size)
        };

        let mut buf = [0u8; size_of::<Inode>()];
        self.read_disk_data(inode_offset, &mut buf);

        let mut inode = unsafe { core::mem::zeroed::<Inode>() };
        unsafe {
            core::ptr::copy_nonoverlapping(
                buf.as_ptr(),
                &mut inode as *mut _ as *mut u8,
                size_of::<Inode>(),
            );
        }
        inode
    }

    pub fn write_inode(&self, inode_idx: u32, inode: &Inode) {
        let inode_offset = {
            let group = (inode_idx - 1) / self.inodes_per_group;
            let index_in_group = (inode_idx - 1) % self.inodes_per_group;
            let bg_desc = self.read_block_group_descriptor(group);
            let inode_table_offset = bg_desc.inode_table as u64 * self.block_size;
            inode_table_offset + (index_in_group as u64 * self.inode_size as u64)
        };

        let ptr = inode as *const Inode as *const u8;
        let slice = unsafe { core::slice::from_raw_parts(ptr, size_of::<Inode>()) };
        self.write_disk_data(inode_offset, slice);
    }

    pub fn get_block_address(&self, inode: &Inode, logical_block: u32) -> u32 {
        let ptrs_per_block = self.block_size / 4;

        if logical_block < 12 {
            return inode.block[logical_block as usize];
        }

        let mut indirect_idx = logical_block - 12;

        if indirect_idx < ptrs_per_block as u32 {
            return self.read_indirect_pointer(inode.block[12], indirect_idx);
        }
        indirect_idx -= ptrs_per_block as u32;

        if indirect_idx < (ptrs_per_block * ptrs_per_block) as u32 {
            let first_idx = indirect_idx / ptrs_per_block as u32;
            let second_idx = indirect_idx % ptrs_per_block as u32;
            let first_block = self.read_indirect_pointer(inode.block[13], first_idx);
            if first_block == 0 {
                return 0;
            }
            return self.read_indirect_pointer(first_block, second_idx);
        }
        indirect_idx -= (ptrs_per_block * ptrs_per_block) as u32;

        let first_idx = indirect_idx / (ptrs_per_block * ptrs_per_block) as u32;
        let rem = indirect_idx % (ptrs_per_block * ptrs_per_block) as u32;
        let second_idx = rem / ptrs_per_block as u32;
        let third_idx = rem % ptrs_per_block as u32;

        let first_block = self.read_indirect_pointer(inode.block[14], first_idx);
        if first_block == 0 {
            return 0;
        }
        let second_block = self.read_indirect_pointer(first_block, second_idx);
        if second_block == 0 {
            return 0;
        }
        return self.read_indirect_pointer(second_block, third_idx);
    }

    pub fn set_block_address(
        &self,
        inode: &mut Inode,
        logical_block: u32,
        phys: u32,
    ) -> Result<(), String> {
        let ptrs_per_block = self.block_size / 4;

        if logical_block < 12 {
            inode.block[logical_block as usize] = phys;
            return Ok(());
        }

        let mut indirect_idx = logical_block - 12;

        if indirect_idx < ptrs_per_block as u32 {
            if inode.block[12] == 0 {
                let new_block = self.alloc_block();
                if new_block == 0 {
                    return Err(String::from("No space for indirect block"));
                }
                inode.block[12] = new_block;

                let zero = vec![0u8; self.block_size as usize];
                self.write_disk_data(new_block as u64 * self.block_size, &zero);
                inode.blocks += self.block_size as u32 / 512;
            }
            self.write_indirect_pointer(inode.block[12], indirect_idx, phys);
            return Ok(());
        }
        indirect_idx -= ptrs_per_block as u32;

        if indirect_idx < (ptrs_per_block * ptrs_per_block) as u32 {
            let first_idx = indirect_idx / ptrs_per_block as u32;
            let second_idx = indirect_idx % ptrs_per_block as u32;

            if inode.block[13] == 0 {
                let new_block = self.alloc_block();
                if new_block == 0 {
                    return Err(String::from("No space for dbl-indirect block"));
                }
                inode.block[13] = new_block;
                let zero = vec![0u8; self.block_size as usize];
                self.write_disk_data(new_block as u64 * self.block_size, &zero);
                inode.blocks += self.block_size as u32 / 512;
            }

            let first_block = inode.block[13];
            let mut second_block = self.read_indirect_pointer(first_block, first_idx);

            if second_block == 0 {
                second_block = self.alloc_block();
                if second_block == 0 {
                    return Err(String::from("No space for dbl-indirect L2"));
                }
                self.write_indirect_pointer(first_block, first_idx, second_block);
                let zero = vec![0u8; self.block_size as usize];
                self.write_disk_data(second_block as u64 * self.block_size, &zero);

                inode.blocks += self.block_size as u32 / 512;
            }

            self.write_indirect_pointer(second_block, second_idx, phys);
            return Ok(());
        }
        indirect_idx -= (ptrs_per_block * ptrs_per_block) as u32;

        let first_idx = indirect_idx / (ptrs_per_block * ptrs_per_block) as u32;
        let rem = indirect_idx % (ptrs_per_block * ptrs_per_block) as u32;
        let second_idx = rem / ptrs_per_block as u32;
        let third_idx = rem % ptrs_per_block as u32;

        if inode.block[14] == 0 {
            let new_block = self.alloc_block();
            if new_block == 0 {
                return Err(String::from("No space for triple-indirect L1"));
            }
            inode.block[14] = new_block;
            let zero = vec![0u8; self.block_size as usize];
            self.write_disk_data(new_block as u64 * self.block_size, &zero);
            inode.blocks += self.block_size as u32 / 512;
        }

        let first_block = inode.block[14];
        let mut second_block = self.read_indirect_pointer(first_block, first_idx);

        if second_block == 0 {
            second_block = self.alloc_block();
            if second_block == 0 {
                return Err(String::from("No space for triple-indirect L2"));
            }
            self.write_indirect_pointer(first_block, first_idx, second_block);
            let zero = vec![0u8; self.block_size as usize];
            self.write_disk_data(second_block as u64 * self.block_size, &zero);
            inode.blocks += self.block_size as u32 / 512;
        }

        let mut third_block = self.read_indirect_pointer(second_block, second_idx);

        if third_block == 0 {
            third_block = self.alloc_block();
            if third_block == 0 {
                return Err(String::from("No space for triple-indirect L3"));
            }
            self.write_indirect_pointer(second_block, second_idx, third_block);
            let zero = vec![0u8; self.block_size as usize];
            self.write_disk_data(third_block as u64 * self.block_size, &zero);
            inode.blocks += self.block_size as u32 / 512;
        }

        self.write_indirect_pointer(third_block, third_idx, phys);
        Ok(())
    }

    fn read_indirect_pointer(&self, block_addr: u32, offset: u32) -> u32 {
        if block_addr == 0 {
            return 0;
        }

        let read_offset = (block_addr as u64 * self.block_size) + (offset as u64 * 4);
        let mut bytes = [0u8; 4];
        self.read_disk_data(read_offset, &mut bytes);
        u32::from_le_bytes(bytes)
    }

    fn write_indirect_pointer(&self, block_addr: u32, offset: u32, val: u32) {
        let write_offset = (block_addr as u64 * self.block_size) + (offset as u64 * 4);
        self.write_disk_data(write_offset, &val.to_le_bytes());
    }

    fn alloc_block(&self) -> u32 {
        let (groups, blocks_per_group, first_data_block) = {
            let sb = self.superblock.lock();
            (sb.blocks_count / sb.blocks_per_group, sb.blocks_per_group, sb.first_data_block)
        };
        let block_size = self.block_size;

        for i in 0..=groups {
            let mut bg = self.read_block_group_descriptor(i);

            if bg.free_blocks_count > 0 {
                // Pre-read bitmap into cache before taking the lock
                let bitmap_block = bg.block_bitmap;
                let mut dummy = vec![0u8; block_size as usize];
                self.read_disk_data(bitmap_block as u64 * block_size, &mut dummy);

                let mut found_bit = None;
                let mut bitmap_to_write = None;
                let mut bg_to_write = None;
                let mut sb_to_write = None;
                let mut bit_val = 0;

                {
                    let _lock = self.metadata_lock.lock();
                    bg = self.read_block_group_descriptor(i);
                    if bg.free_blocks_count > 0 {
                        let mut bitmap = vec![0u8; block_size as usize];
                        self.read_disk_data(bitmap_block as u64 * block_size, &mut bitmap);

                        for byte_idx in 0..block_size as usize {
                            if bitmap[byte_idx] != 0xFF {
                                for bit_idx in 0..8 {
                                    if (bitmap[byte_idx] & (1 << bit_idx)) == 0 {
                                        bitmap[byte_idx] |= 1 << bit_idx;
                                        bit_val = (i * blocks_per_group) + (byte_idx as u32 * 8) + bit_idx as u32 + first_data_block;
                                        found_bit = Some(bit_val);
                                        
                                        bitmap_to_write = Some(bitmap.clone());
                                        
                                        bg.free_blocks_count -= 1;
                                        bg_to_write = Some(bg);

                                        {
                                            let mut descriptors = self.bg_descriptors.write();
                                            descriptors[i as usize] = bg;
                                        }

                                        {
                                            let mut sb = self.superblock.lock();
                                            sb.free_blocks_count -= 1;
                                            sb_to_write = Some(*sb);
                                        }
                                        break;
                                    }
                                }
                            }
                            if found_bit.is_some() { break; }
                        }
                    }
                }

                if let Some(val) = found_bit {
                    if let Some(bitmap) = bitmap_to_write {
                        self.write_disk_data(bitmap_block as u64 * block_size, &bitmap);
                    }
                    if let Some(bg_val) = bg_to_write {
                        let bgdt_start_block = if block_size == 1024 { 2 } else { 1 };
                        let offset = (bgdt_start_block as u64 * block_size) + (i as u64 * size_of::<BlockGroupDescriptor>() as u64);
                        let ptr = &bg_val as *const BlockGroupDescriptor as *const u8;
                        let slice = unsafe { core::slice::from_raw_parts(ptr, size_of::<BlockGroupDescriptor>()) };
                        self.write_disk_data(offset, slice);
                    }
                    if let Some(sb_val) = sb_to_write {
                        let sb_ptr = &sb_val as *const Superblock as *const u8;
                        let sb_slice = unsafe { core::slice::from_raw_parts(sb_ptr, size_of::<Superblock>()) };
                        self.write_disk_data(1024, sb_slice);
                    }
                    return val;
                }
            }
        }
        0
    }

    fn alloc_inode(&self) -> u32 {
        let (groups, inodes_per_group) = {
            let sb = self.superblock.lock();
            (sb.inodes_count / sb.inodes_per_group, sb.inodes_per_group)
        };
        let block_size = self.block_size;

        for i in 0..=groups {
            let mut bg = self.read_block_group_descriptor(i);

            if bg.free_inodes_count > 0 {
                let bitmap_block = bg.inode_bitmap;
                let mut bitmap = vec![0u8; block_size as usize];
                self.read_disk_data(bitmap_block as u64 * block_size, &mut bitmap);

                let mut found_bit = None;
                let mut bitmap_to_write = None;
                let mut bg_to_write = None;
                let mut sb_to_write = None;
                let mut bit_val = 0;

                {
                    let _lock = self.metadata_lock.lock();
                    bg = self.read_block_group_descriptor(i);
                    if bg.free_inodes_count > 0 {
                        let mut bitmap = vec![0u8; block_size as usize];
                        self.read_disk_data(bitmap_block as u64 * block_size, &mut bitmap);

                        for byte_idx in 0..block_size as usize {
                            if bitmap[byte_idx] != 0xFF {
                                for bit_idx in 0..8 {
                                    if (bitmap[byte_idx] & (1 << bit_idx)) == 0 {
                                        bitmap[byte_idx] |= 1 << bit_idx;
                                        bit_val = (i * inodes_per_group) + (byte_idx as u32 * 8) + bit_idx as u32 + 1;
                                        found_bit = Some(bit_val);
                                        
                                        bitmap_to_write = Some(bitmap.clone());
                                        
                                        bg.free_inodes_count -= 1;
                                        bg_to_write = Some(bg);

                                        {
                                            let mut descriptors = self.bg_descriptors.write();
                                            descriptors[i as usize] = bg;
                                        }

                                        {
                                            let mut sb = self.superblock.lock();
                                            sb.free_inodes_count -= 1;
                                            sb_to_write = Some(*sb);
                                        }
                                        break;
                                    }
                                }
                            }
                            if found_bit.is_some() { break; }
                        }
                    }
                }

                if let Some(val) = found_bit {
                    if let Some(bitmap) = bitmap_to_write {
                        self.write_disk_data(bitmap_block as u64 * block_size, &bitmap);
                    }
                    if let Some(bg_val) = bg_to_write {
                        let bgdt_start_block = if block_size == 1024 { 2 } else { 1 };
                        let offset = (bgdt_start_block as u64 * block_size) + (i as u64 * size_of::<BlockGroupDescriptor>() as u64);
                        let ptr = &bg_val as *const BlockGroupDescriptor as *const u8;
                        let slice = unsafe { core::slice::from_raw_parts(ptr, size_of::<BlockGroupDescriptor>()) };
                        self.write_disk_data(offset, slice);
                    }
                    if let Some(sb_val) = sb_to_write {
                        let sb_ptr = &sb_val as *const Superblock as *const u8;
                        let sb_slice = unsafe { core::slice::from_raw_parts(sb_ptr, size_of::<Superblock>()) };
                        self.write_disk_data(1024, sb_slice);
                    }
                    return val;
                }
            }
        }
        0
    }

    fn free_block(&self, block_id: u32) {
        if block_id == 0 { return; }
        let (blocks_per_group, first_data_block) = {
            let sb = self.superblock.lock();
            (sb.blocks_per_group, sb.first_data_block)
        };
        let block_idx = block_id - first_data_block;
        let group = block_idx / blocks_per_group;
        let index_in_group = block_idx % blocks_per_group;

        let _lock = self.metadata_lock.lock();
        let mut bg = self.read_block_group_descriptor(group);
        let bitmap_block = bg.block_bitmap;
        let mut bitmap = vec![0u8; self.block_size as usize];
        self.read_disk_data(bitmap_block as u64 * self.block_size, &mut bitmap);

        let byte_idx = (index_in_group / 8) as usize;
        let bit_idx = index_in_group % 8;

        if (bitmap[byte_idx] & (1 << bit_idx)) != 0 {
            bitmap[byte_idx] &= !(1 << bit_idx);
            self.write_disk_data(bitmap_block as u64 * self.block_size, &bitmap);

            let mut free_blocks = bg.free_blocks_count;
            free_blocks += 1;
            bg.free_blocks_count = free_blocks;

            {
                let mut descriptors = self.bg_descriptors.write();
                descriptors[group as usize] = bg;
            }

            let bgdt_start_block = if self.block_size == 1024 { 2 } else { 1 };
            let offset = (bgdt_start_block as u64 * self.block_size) + (group as u64 * size_of::<BlockGroupDescriptor>() as u64);
            let ptr = &bg as *const BlockGroupDescriptor as *const u8;
            let slice = unsafe { core::slice::from_raw_parts(ptr, size_of::<BlockGroupDescriptor>()) };
            self.write_disk_data(offset, slice);

            {
                let mut sb = self.superblock.lock();
                let mut global_free = sb.free_blocks_count;
                global_free += 1;
                sb.free_blocks_count = global_free;
                let sb_ptr = &*sb as *const Superblock as *const u8;
                let sb_slice = unsafe { core::slice::from_raw_parts(sb_ptr, size_of::<Superblock>()) };
                self.write_disk_data(1024, sb_slice);
            }
        }
    }

    fn free_inode(&self, inode_id: u32) {
        if inode_id == 0 { return; }
        let inode_idx = inode_id - 1;
        let group = inode_idx / self.inodes_per_group;
        let index_in_group = inode_idx % self.inodes_per_group;

        let _lock = self.metadata_lock.lock();
        let mut bg = self.read_block_group_descriptor(group);
        let bitmap_block = bg.inode_bitmap;
        let mut bitmap = vec![0u8; self.block_size as usize];
        self.read_disk_data(bitmap_block as u64 * self.block_size, &mut bitmap);

        let byte_idx = (index_in_group / 8) as usize;
        let bit_idx = index_in_group % 8;

        if (bitmap[byte_idx] & (1 << bit_idx)) != 0 {
            bitmap[byte_idx] &= !(1 << bit_idx);
            self.write_disk_data(bitmap_block as u64 * self.block_size, &bitmap);

            let mut free_inodes = bg.free_inodes_count;
            free_inodes += 1;
            bg.free_inodes_count = free_inodes;

            {
                let mut descriptors = self.bg_descriptors.write();
                descriptors[group as usize] = bg;
            }

            let bgdt_start_block = if self.block_size == 1024 { 2 } else { 1 };
            let offset = (bgdt_start_block as u64 * self.block_size) + (group as u64 * size_of::<BlockGroupDescriptor>() as u64);
            let ptr = &bg as *const BlockGroupDescriptor as *const u8;
            let slice = unsafe { core::slice::from_raw_parts(ptr, size_of::<BlockGroupDescriptor>()) };
            self.write_disk_data(offset, slice);

            {
                let mut sb = self.superblock.lock();
                let mut global_free = sb.free_inodes_count;
                global_free += 1;
                sb.free_inodes_count = global_free;
                let sb_ptr = &*sb as *const Superblock as *const u8;
                let sb_slice = unsafe { core::slice::from_raw_parts(sb_ptr, size_of::<Superblock>()) };
                self.write_disk_data(1024, sb_slice);
            }
        }
    }
}

use crate::fs::ext2::structs::DirectoryEntry;
use crate::fs::vfs::{FileSystem, FileType, VfsNode};

pub struct Ext2Node {
    fs: *mut Ext2,
    inode_idx: u32,
    inode: Inode,
    name: String,
}

unsafe impl Send for Ext2Node {}
unsafe impl Sync for Ext2Node {}

impl FileSystem for Ext2 {
    fn root(&mut self) -> Result<Box<dyn VfsNode>, String> {
        let inode = self.read_inode(2);
        Ok(Box::new(Ext2Node {
            fs: self as *mut Ext2,
            inode_idx: 2,
            inode,
            name: String::from("/"),
        }))
    }
}

impl VfsNode for Ext2Node {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn size(&self) -> u64 {
        let fs = unsafe { &*self.fs };
        let _lock_arc = fs.get_inode_lock(self.inode_idx);
        let _lock = _lock_arc.read();
        self.inode.size as u64
    }

    fn kind(&self) -> FileType {
        let fs = unsafe { &*self.fs };
        let _lock_arc = fs.get_inode_lock(self.inode_idx);
        let _lock = _lock_arc.read();
        if (self.inode.mode & 0xF000) == 0x4000 {
            FileType::Directory
        } else if (self.inode.mode & 0xF000) == 0x8000 {
            FileType::File
        } else if (self.inode.mode & 0xF000) == 0xA000 {
            FileType::Symlink
        } else {
            FileType::Unknown
        }
    }

    fn inode(&self) -> u64 {
        self.inode_idx as u64
    }

    fn stat(&self) -> crate::fs::vfs::Stat {
        let fs = unsafe { &*self.fs };
        let _lock_arc = fs.get_inode_lock(self.inode_idx);
        let _lock = _lock_arc.read();
        crate::fs::vfs::Stat {
            dev: 1,
            ino: self.inode_idx as u64,
            mode: self.inode.mode as u32,
            uid: self.inode.uid as u32,
            gid: self.inode.gid as u32,
            nlink: self.inode.links_count as u32,
            size: self.inode.size as u64,
            atime: self.inode.atime as u32 as u64,
            mtime: self.inode.mtime as u32 as u64,
            ctime: self.inode.ctime as u32 as u64,
        }
    }

    fn read(&mut self, offset: u64, buffer: &mut [u8]) -> Result<usize, String> {
        let fs = unsafe { &*self.fs };
        let _lock_arc = fs.get_inode_lock(self.inode_idx);
        let _lock = _lock_arc.read();

        let total_size = self.inode.size as u64;
        if offset >= total_size {
            return Ok(0);
        }

        if (self.inode.mode & 0xF000) == 0xA000 && total_size < 60 {
            let mut data = [0u8; 60];
            let inode_block = self.inode.block;
            unsafe {
                core::ptr::copy_nonoverlapping(
                    inode_block.as_ptr() as *const u8,
                    data.as_mut_ptr(),
                    60,
                );
            }
            let to_copy = core::cmp::min(buffer.len(), (total_size - offset) as usize);
            buffer[..to_copy].copy_from_slice(&data[offset as usize..offset as usize + to_copy]);
            return Ok(to_copy);
        }

        let mut bytes_read = 0;
        let mut current_offset = offset;
        let len = core::cmp::min(buffer.len() as u64, total_size - offset) as usize;
        let block_size = fs.block_size;

        while bytes_read < len {
            let block_idx = (current_offset / block_size) as u32;
            let block_offset = (current_offset % block_size) as usize;

            if block_offset == 0 && (len - bytes_read) >= block_size as usize {
                let mut blocks_to_read = 1;
                let max_blocks = ((len - bytes_read) / block_size as usize) as u32;
                let start_phys = fs.get_block_address(&self.inode, block_idx);

                if start_phys != 0 {
                    while blocks_to_read < max_blocks {
                        let next_phys = fs.get_block_address(&self.inode, block_idx + blocks_to_read);
                        if next_phys == start_phys + blocks_to_read {
                            blocks_to_read += 1;
                        } else {
                            break;
                        }
                    }

                    let read_len = (blocks_to_read * block_size as u32) as usize;
                    let target_slice = &mut buffer[bytes_read..bytes_read + read_len];
                    fs.read_disk_data(start_phys as u64 * block_size, target_slice);

                    bytes_read += read_len;
                    current_offset += read_len as u64;
                    continue;
                }
            }

            let phys = fs.get_block_address(&self.inode, block_idx);
            let cache_phys = {
                let cache_hit = crate::fs::cache::GLOBAL_PAGE_CACHE.lock().get(fs.disk_id, self.inode_idx as u64, block_idx);
                if let Some(p) = cache_hit {
                    p
                } else {
                    let new_frame = crate::memory::pmm::allocate_frame().expect("OOM for Page Cache");
                    let virt = new_frame + crate::memory::paging::HHDM_OFFSET;
                    let dest = unsafe { core::slice::from_raw_parts_mut(virt as *mut u8, 4096) };
                    if phys != 0 {
                        fs.read_disk_data(phys as u64 * block_size, dest);
                    } else {
                        dest.fill(0);
                    }
                    
                    let mut cache = crate::fs::cache::GLOBAL_PAGE_CACHE.lock();
                    if let Some(existing) = cache.get(fs.disk_id, self.inode_idx as u64, block_idx) {
                        crate::memory::pmm::free_frame(new_frame);
                        existing
                    } else {
                        cache.insert(fs.disk_id, self.inode_idx as u64, block_idx, new_frame);
                        new_frame
                    }
                }
            };

            let cache_virt = cache_phys + crate::memory::paging::HHDM_OFFSET;
            let cache_slice = unsafe { core::slice::from_raw_parts(cache_virt as *const u8, block_size as usize) };
            let to_copy = core::cmp::min(len - bytes_read, block_size as usize - block_offset);
            buffer[bytes_read..bytes_read + to_copy].copy_from_slice(&cache_slice[block_offset..block_offset + to_copy]);

            bytes_read += to_copy;
            current_offset += to_copy as u64;
        }

        Ok(bytes_read)
    }

    fn write(&mut self, offset: u64, buffer: &[u8]) -> Result<usize, String> {
        let fs = unsafe { &*self.fs };
        let _lock_arc = fs.get_inode_lock(self.inode_idx);
        
        let block_size = fs.block_size;
        let mut bytes_written = 0;
        let mut current_offset = offset;
        let mut buf_offset = 0;
        let len = buffer.len();

        let mut bounce_buf = alloc::vec![0u8; block_size as usize];

        while bytes_written < len {
            let block_idx = (current_offset / block_size) as u32;
            let block_offset = (current_offset % block_size) as usize;

            let mut phys = 0;
            let mut newly_allocated = false;

            {
                let _lock = _lock_arc.write();
                self.inode = fs.read_inode(self.inode_idx);
                phys = fs.get_block_address(&self.inode, block_idx);

                if phys == 0 {
                    phys = fs.alloc_block();
                    if phys == 0 { return Err(String::from("Failed to allocate block")); }
                    fs.set_block_address(&mut self.inode, block_idx, phys)?;
                    self.inode.blocks += (block_size / 512) as u32;
                    newly_allocated = true;
                    fs.write_inode(self.inode_idx, &self.inode);
                }
            }

            let to_copy = core::cmp::min(len - bytes_written, (block_size as usize) - block_offset);
            if block_offset != 0 || to_copy < block_size as usize {
                if !newly_allocated {
                    fs.read_disk_data(phys as u64 * block_size, &mut bounce_buf);
                } else {
                    bounce_buf.fill(0);
                }
                bounce_buf[block_offset..block_offset + to_copy].copy_from_slice(&buffer[buf_offset..buf_offset + to_copy]);
                fs.write_disk_data(phys as u64 * block_size, &bounce_buf);
            } else {
                fs.write_disk_data(phys as u64 * block_size, &buffer[buf_offset..buf_offset + to_copy]);
            }

            bytes_written += to_copy;
            current_offset += to_copy as u64;
            buf_offset += to_copy;

            crate::fs::cache::GLOBAL_PAGE_CACHE.lock().invalidate(fs.disk_id, self.inode_idx as u64, block_idx);
        }

        {
            let _lock = _lock_arc.write();
            self.inode = fs.read_inode(self.inode_idx);
            let need_size_update = current_offset > self.inode.size as u64;
            self.inode.mtime = crate::drivers::rtc::unix_timestamp();
            if need_size_update {
                self.inode.size = current_offset as u32;
            }
            fs.write_inode(self.inode_idx, &self.inode);
        }
        
        fs.flush();

        Ok(bytes_written)
    }

    fn children(&mut self) -> Result<Vec<Box<dyn VfsNode>>, String> {
        let fs = unsafe { &*self.fs };
        let _lock_arc = fs.get_inode_lock(self.inode_idx);
        let _lock = _lock_arc.read();
        
        if (self.inode.mode & 0xF000) != 0x4000 {
            return Err(String::from("Not a directory"));
        }

        let block_size = fs.block_size as usize;
        let mut entries = Vec::new();
        let mut offset = 0;
        let total_size = self.inode.size as u64;

        while offset < total_size {
            let block_idx = (offset / block_size as u64) as u32;
            let phys = fs.get_block_address(&self.inode, block_idx);

            if phys != 0 {
                let cache_phys = {
                    let cache_hit = crate::fs::cache::GLOBAL_PAGE_CACHE.lock().get(fs.disk_id, self.inode_idx as u64, block_idx);
                    if let Some(p) = cache_hit {
                        p
                    } else {
                        let new_frame = crate::memory::pmm::allocate_frame().expect("OOM for Page Cache");
                        let virt = new_frame + crate::memory::paging::HHDM_OFFSET;
                        let dest = unsafe { core::slice::from_raw_parts_mut(virt as *mut u8, 4096) };
                        fs.read_disk_data(phys as u64 * block_size as u64, dest);
                        
                        let mut cache = crate::fs::cache::GLOBAL_PAGE_CACHE.lock();
                        if let Some(existing) = cache.get(fs.disk_id, self.inode_idx as u64, block_idx) {
                            crate::memory::pmm::free_frame(new_frame);
                            existing
                        } else {
                            cache.insert(fs.disk_id, self.inode_idx as u64, block_idx, new_frame);
                            new_frame
                        }
                    }
                };

                let cache_virt = cache_phys + crate::memory::paging::HHDM_OFFSET;
                let cache_slice = unsafe { core::slice::from_raw_parts(cache_virt as *const u8, block_size) };

                let mut block_pos = 0;
                while block_pos < block_size {
                    let ptr = unsafe { cache_slice.as_ptr().add(block_pos) };
                    let entry = unsafe { &*(ptr as *const DirectoryEntry) };
                    if entry.rec_len == 0 { break; }
                    if entry.inode != 0 {
                        let name_len = entry.name_len as usize;
                        let name_ptr = unsafe { ptr.add(8) };
                        if block_pos + 8 + name_len <= block_size {
                            let name_slice = unsafe { core::slice::from_raw_parts(name_ptr, name_len) };
                            let name = String::from_utf8_lossy(name_slice).into_owned();
                            let child_inode = fs.read_inode(entry.inode);
                            entries.push(Box::new(Ext2Node {
                                fs: self.fs,
                                inode_idx: entry.inode,
                                inode: child_inode,
                                name,
                            }) as Box<dyn VfsNode>);
                        }
                    }
                    block_pos += entry.rec_len as usize;
                }
            }
            offset += block_size as u64;
        }
        Ok(entries)
    }

    fn find(&mut self, name: &str) -> Result<Box<dyn VfsNode>, String> {
        let fs = unsafe { &*self.fs };
        let _lock_arc = fs.get_inode_lock(self.inode_idx);
        let _lock = _lock_arc.read();
        
        if (self.inode.mode & 0xF000) != 0x4000 {
            return Err(String::from("Not a directory"));
        }

        let block_size = fs.block_size as usize;
        let mut offset = 0;
        let total_size = self.inode.size as u64;
        let name_bytes = name.as_bytes();

        while offset < total_size {
            let block_idx = (offset / block_size as u64) as u32;
            let phys = fs.get_block_address(&self.inode, block_idx);
            if phys != 0 {
                let cache_phys = {
                    let cache_hit = crate::fs::cache::GLOBAL_PAGE_CACHE.lock().get(fs.disk_id, self.inode_idx as u64, block_idx);
                    if let Some(p) = cache_hit {
                        p
                    } else {
                        let new_frame = crate::memory::pmm::allocate_frame().expect("OOM for Page Cache");
                        let virt = new_frame + crate::memory::paging::HHDM_OFFSET;
                        let dest = unsafe { core::slice::from_raw_parts_mut(virt as *mut u8, 4096) };
                        fs.read_disk_data(phys as u64 * block_size as u64, dest);
                        
                        let mut cache = crate::fs::cache::GLOBAL_PAGE_CACHE.lock();
                        if let Some(existing) = cache.get(fs.disk_id, self.inode_idx as u64, block_idx) {
                            crate::memory::pmm::free_frame(new_frame);
                            existing
                        } else {
                            cache.insert(fs.disk_id, self.inode_idx as u64, block_idx, new_frame);
                            new_frame
                        }
                    }
                };

                let cache_virt = cache_phys + crate::memory::paging::HHDM_OFFSET;
                let cache_slice = unsafe { core::slice::from_raw_parts(cache_virt as *const u8, block_size) };
                let mut block_pos = 0;
                while block_pos < block_size {
                    let ptr = unsafe { cache_slice.as_ptr().add(block_pos) };
                    let entry = unsafe { &*(ptr as *const DirectoryEntry) };
                    if entry.rec_len == 0 { break; }
                    if entry.inode != 0 {
                        let name_len = entry.name_len as usize;
                        if block_pos + 8 + name_len <= block_size {
                            let entry_name_ptr = unsafe { ptr.add(8) };
                            let entry_name = unsafe { core::slice::from_raw_parts(entry_name_ptr, name_len) };
                            if entry_name == name_bytes {
                                let child_inode = fs.read_inode(entry.inode);
                                return Ok(Box::new(Ext2Node {
                                    fs: self.fs,
                                    inode_idx: entry.inode,
                                    inode: child_inode,
                                    name: String::from(name),
                                }));
                            }
                        }
                    }
                    block_pos += entry.rec_len as usize;
                }
            }
            offset += block_size as u64;
        }
        Err(String::from("File not found"))
    }

    fn create_file(&mut self, name: &str) -> Result<Box<dyn VfsNode>, String> {
        let fs = unsafe { &*self.fs };
        let _lock_arc = fs.get_inode_lock(self.inode_idx);
        let _lock = _lock_arc.write();
        self.inode = fs.read_inode(self.inode_idx);
        self.create_node_unlocked(name, 0x81B4)
    }

    fn create_dir(&mut self, name: &str) -> Result<Box<dyn VfsNode>, String> {
        let fs = unsafe { &*self.fs };
        let _lock_arc = fs.get_inode_lock(self.inode_idx);
        let _lock = _lock_arc.write();
        self.inode = fs.read_inode(self.inode_idx);
        self.create_node_unlocked(name, 0x41ED)
    }

    fn remove(&mut self, name: &str) -> Result<(), String> {
        let fs = unsafe { &*self.fs };
        let _lock_arc = fs.get_inode_lock(self.inode_idx);
        let _lock = _lock_arc.write();
        self.inode = fs.read_inode(self.inode_idx);
        self.remove_internal_unlocked(name)
    }

    fn read_dir(&mut self, start_index: u64, buffer: &mut [u8]) -> Result<(usize, usize), String> {
        let fs = unsafe { &*self.fs };
        let _lock_arc = fs.get_inode_lock(self.inode_idx);
        let _lock = _lock_arc.read();
        
        let block_size = fs.block_size as usize;
        let mut bytes_written = 0;
        let mut count_read = 0;
        let mut entry_index: u64 = 0;
        let mut offset = 0;
        let total_size = self.inode.size as u64;

        while offset < total_size {
            let block_idx = (offset / block_size as u64) as u32;
            let phys = fs.get_block_address(&self.inode, block_idx);
            if phys != 0 {
                let cache_phys = {
                    let cache_hit = crate::fs::cache::GLOBAL_PAGE_CACHE.lock().get(fs.disk_id, self.inode_idx as u64, block_idx);
                    if let Some(p) = cache_hit {
                        p
                    } else {
                        let new_frame = crate::memory::pmm::allocate_frame().expect("OOM for Page Cache");
                        let virt = new_frame + crate::memory::paging::HHDM_OFFSET;
                        let dest = unsafe { core::slice::from_raw_parts_mut(virt as *mut u8, 4096) };
                        fs.read_disk_data(phys as u64 * block_size as u64, dest);
                        let mut cache = crate::fs::cache::GLOBAL_PAGE_CACHE.lock();
                        if let Some(existing) = cache.get(fs.disk_id, self.inode_idx as u64, block_idx) {
                            crate::memory::pmm::free_frame(new_frame);
                            existing
                        } else {
                            cache.insert(fs.disk_id, self.inode_idx as u64, block_idx, new_frame);
                            new_frame
                        }
                    }
                };
                let cache_virt = cache_phys + crate::memory::paging::HHDM_OFFSET;
                let cache_slice = unsafe { core::slice::from_raw_parts(cache_virt as *const u8, block_size) };
                let mut block_pos = 0;
                while block_pos < block_size {
                    let ptr = unsafe { cache_slice.as_ptr().add(block_pos) };
                    let entry = unsafe { &*(ptr as *const DirectoryEntry) };
                    if entry.rec_len == 0 { break; }
                    if entry.inode != 0 {
                        if entry_index >= start_index {
                            let name_len = entry.name_len as usize;
                            if bytes_written + 2 + name_len > buffer.len() { return Ok((bytes_written, count_read)); }
                            let child_inode = fs.read_inode(entry.inode);
                            let mapped_type = if (child_inode.mode & 0xF000) == 0x4000 { 2 } else if (child_inode.mode & 0xF000) == 0x8000 { 1 } else { 0 };
                            buffer[bytes_written] = mapped_type;
                            buffer[bytes_written + 1] = name_len as u8;
                            let name_ptr = unsafe { ptr.add(8) };
                            unsafe { core::ptr::copy_nonoverlapping(name_ptr, buffer.as_mut_ptr().add(bytes_written + 2), name_len); }
                            bytes_written += 2 + name_len;
                            count_read += 1;
                        }
                        entry_index += 1;
                    }
                    block_pos += entry.rec_len as usize;
                }
            }
            offset += block_size as u64;
        }
        Ok((bytes_written, count_read))
    }

    fn rename(&mut self, old_name: &str, new_name: &str) -> Result<(), String> {
        let fs = unsafe { &*self.fs };
        let _lock_arc = fs.get_inode_lock(self.inode_idx);
        let _lock = _lock_arc.write();
        self.inode = fs.read_inode(self.inode_idx);

        let mut target_inode_idx = 0;
        let mut file_type = 0;
        let block_size = fs.block_size as usize;
        let mut offset = 0;
        let total_size = self.inode.size as u64;

        while offset < total_size {
            let block_idx = (offset / block_size as u64) as u32;
            let phys = fs.get_block_address(&self.inode, block_idx);
            if phys != 0 {
                let mut buf = vec![0u8; block_size];
                fs.read_disk_data(phys as u64 * block_size as u64, &mut buf);
                let mut block_pos = 0;
                while block_pos < block_size {
                    let ptr = unsafe { buf.as_ptr().add(block_pos) };
                    let entry = unsafe { &*(ptr as *const DirectoryEntry) };
                    if entry.rec_len == 0 { break; }
                    let name_len = entry.name_len as usize;
                    let name_ptr = unsafe { ptr.add(8) };
                    let entry_name = unsafe { core::slice::from_raw_parts(name_ptr, name_len) };
                    if entry_name == old_name.as_bytes() {
                        target_inode_idx = entry.inode;
                        file_type = entry.file_type;
                        break;
                    }
                    block_pos += entry.rec_len as usize;
                }
            }
            if target_inode_idx != 0 { break; }
            offset += block_size as u64;
        }
        if target_inode_idx == 0 { return Err(String::from("Old file not found")); }
        self.add_directory_entry_unlocked(target_inode_idx, new_name, file_type)?;
        let mut target_inode = fs.read_inode(target_inode_idx);
        target_inode.links_count += 1;
        fs.write_inode(target_inode_idx, &target_inode);
        self.remove_internal_unlocked(old_name)
    }

    fn truncate(&mut self, size: u64) -> Result<(), String> {
        let fs = unsafe { &*self.fs };
        let _lock_arc = fs.get_inode_lock(self.inode_idx);
        let _lock = _lock_arc.write();
        self.inode = fs.read_inode(self.inode_idx);
        if size > 0xFFFFFFFF { return Err(String::from("File too large")); }
        if size == 0 {
            for i in 0..15 {
                let block = self.inode.block[i];
                if block != 0 {
                    fs.free_block(block);
                    self.inode.block[i] = 0;
                    crate::fs::cache::GLOBAL_PAGE_CACHE.lock().invalidate(fs.disk_id, self.inode_idx as u64, i as u32);
                }
            }
            self.inode.size = 0;
            self.inode.blocks = 0;
        } else {
            self.inode.size = size as u32;
        }
        fs.write_inode(self.inode_idx, &self.inode);
        Ok(())
    }

    fn mmap(&mut self, offset: u64, _len: usize) -> Result<u64, String> {
        let fs = unsafe { &*self.fs };
        let _lock_arc = fs.get_inode_lock(self.inode_idx);
        let _lock = _lock_arc.read();
        let block_size = fs.block_size;
        let block_idx = (offset / block_size) as u32;
        let block_offset = (offset % block_size) as usize;
        let phys = fs.get_block_address(&self.inode, block_idx);
        let cache_phys = {
            let cache_hit = crate::fs::cache::GLOBAL_PAGE_CACHE.lock().get(fs.disk_id, self.inode_idx as u64, block_idx);
            if let Some(p) = cache_hit { p } else {
                let new_frame = crate::memory::pmm::allocate_frame().expect("OOM");
                let virt = new_frame + crate::memory::paging::HHDM_OFFSET;
                let dest = unsafe { core::slice::from_raw_parts_mut(virt as *mut u8, 4096) };
                if phys != 0 { fs.read_disk_data(phys as u64 * block_size, dest); } else { dest.fill(0); }
                let mut cache = crate::fs::cache::GLOBAL_PAGE_CACHE.lock();
                if let Some(existing) = cache.get(fs.disk_id, self.inode_idx as u64, block_idx) {
                    crate::memory::pmm::free_frame(new_frame);
                    existing
                } else {
                    cache.insert(fs.disk_id, self.inode_idx as u64, block_idx, new_frame);
                    new_frame
                }
            }
        };
        Ok(cache_phys + crate::memory::paging::HHDM_OFFSET + block_offset as u64)
    }

    fn link(&mut self, name: &str, src: &mut dyn VfsNode) -> Result<(), String> {
        let fs = unsafe { &*self.fs };
        let _lock_arc = fs.get_inode_lock(self.inode_idx);
        let _lock = _lock_arc.write();
        self.inode = fs.read_inode(self.inode_idx);
        let src_inode_idx = src.inode() as u32;
        let _src_lock_arc = fs.get_inode_lock(src_inode_idx);
        let _src_lock = _src_lock_arc.write();
        let mut src_inode = fs.read_inode(src_inode_idx);
        src_inode.links_count += 1;
        fs.write_inode(src_inode_idx, &src_inode);
        let file_type = if (src_inode.mode & 0xF000) == 0x4000 { 2 } else { 1 };
        self.add_directory_entry_unlocked(src_inode_idx, name, file_type)
    }

    fn symlink(&mut self, name: &str, target: &str) -> Result<(), String> {
        let mut node = self.create_file(name)?;
        node.write(0, target.as_bytes())?;
        Ok(())
    }

    fn set_times(&mut self, atime: u64, mtime: u64) -> Result<(), String> {
        let fs = unsafe { &*self.fs };
        let _lock_arc = fs.get_inode_lock(self.inode_idx);
        let _lock = _lock_arc.write();
        self.inode = fs.read_inode(self.inode_idx);
        self.inode.atime = atime as u32;
        self.inode.mtime = mtime as u32;
        fs.write_inode(self.inode_idx, &self.inode);
        Ok(())
    }

    fn readlink(&mut self) -> Result<String, String> {
        let fs = unsafe { &*self.fs };
        let _lock_arc = fs.get_inode_lock(self.inode_idx);
        let _lock = _lock_arc.read();
        if (self.inode.mode & 0xF000) != 0xA000 { return Err(String::from("Not a symlink")); }
        let size = self.inode.size as usize;
        let mut buf = vec![0u8; size];
        let n = self.read(0, &mut buf)?;
        Ok(String::from_utf8_lossy(&buf[..n]).into_owned())
    }
}

impl Ext2Node {
    fn find_internal_unlocked(&self, name: &str) -> Result<u32, String> {
        let fs = unsafe { &*self.fs };
        let block_size = fs.block_size as usize;
        let mut offset = 0;
        let total_size = self.inode.size as u64;
        let name_bytes = name.as_bytes();
        while offset < total_size {
            let block_idx = (offset / block_size as u64) as u32;
            let phys = fs.get_block_address(&self.inode, block_idx);
            if phys != 0 {
                let mut buf = vec![0u8; block_size];
                fs.read_disk_data(phys as u64 * block_size as u64, &mut buf);
                let mut block_pos = 0;
                while block_pos < block_size {
                    let ptr = unsafe { buf.as_ptr().add(block_pos) };
                    let entry = unsafe { &*(ptr as *const DirectoryEntry) };
                    if entry.rec_len == 0 { break; }
                    if entry.inode != 0 {
                        let name_len = entry.name_len as usize;
                        if block_pos + 8 + name_len <= block_size {
                            let entry_name_ptr = unsafe { ptr.add(8) };
                            let entry_name = unsafe { core::slice::from_raw_parts(entry_name_ptr, name_len) };
                            if entry_name == name_bytes { return Ok(entry.inode); }
                        }
                    }
                    block_pos += entry.rec_len as usize;
                }
            }
            offset += block_size as u64;
        }
        Err(String::from("File not found"))
    }

    fn remove_internal_unlocked(&mut self, name: &str) -> Result<(), String> {
        let fs = unsafe { &*self.fs };
        let block_size = fs.block_size;
        let mut offset = 0;
        let total_size = self.inode.size as u64;
        while offset < total_size {
            let block_idx = (offset / block_size) as u32;
            let phys = fs.get_block_address(&self.inode, block_idx);
            if phys != 0 {
                let mut buf = vec![0u8; block_size as usize];
                fs.read_disk_data(phys as u64 * block_size, &mut buf);
                let mut block_pos = 0;
                let mut prev_pos = 0;
                let mut prev_rec_len = 0;
                while block_pos < block_size as usize {
                    let ptr = unsafe { buf.as_ptr().add(block_pos) };
                    let entry = unsafe { &mut *(ptr as *mut DirectoryEntry) };
                    if entry.rec_len == 0 { break; }
                    if entry.inode != 0 {
                        let name_len = entry.name_len as usize;
                        let name_ptr = unsafe { ptr.add(8) };
                        let entry_name = unsafe { core::slice::from_raw_parts(name_ptr, name_len) };
                        if entry_name == name.as_bytes() {
                            let inode_to_free = entry.inode;
                            if prev_rec_len > 0 {
                                let prev_ptr = unsafe { buf.as_mut_ptr().add(prev_pos) };
                                let prev_entry = unsafe { &mut *(prev_ptr as *mut DirectoryEntry) };
                                prev_entry.rec_len += entry.rec_len;
                            } else { entry.inode = 0; }
                            fs.write_disk_data(phys as u64 * block_size, &buf);
                            crate::fs::cache::GLOBAL_PAGE_CACHE.lock().invalidate(fs.disk_id, self.inode_idx as u64, block_idx);
                            let mut target_inode = fs.read_inode(inode_to_free);
                            if (target_inode.mode & 0xF000) == 0x4000 {
                                let mut check_buf = vec![0u8; block_size as usize];
                                if target_inode.block[0] != 0 {
                                    fs.read_disk_data(target_inode.block[0] as u64 * block_size, &mut check_buf);
                                    let mut check_pos = 0;
                                    let mut entries_count = 0;
                                    while check_pos < block_size as usize {
                                        let c_ptr = unsafe { check_buf.as_ptr().add(check_pos) };
                                        let c_entry = unsafe { &*(c_ptr as *const DirectoryEntry) };
                                        if c_entry.rec_len == 0 { break; }
                                        if c_entry.inode != 0 { entries_count += 1; }
                                        check_pos += c_entry.rec_len as usize;
                                    }
                                    if entries_count > 2 { return Err(String::from("Directory not empty")); }
                                }
                            }
                            if target_inode.links_count > 0 {
                                target_inode.links_count -= 1;
                                if target_inode.links_count == 0 {
                                    for i in 0..12 {
                                        if target_inode.block[i] != 0 {
                                            fs.free_block(target_inode.block[i]);
                                            target_inode.block[i] = 0;
                                        }
                                    }
                                    fs.write_inode(inode_to_free, &target_inode);
                                    fs.free_inode(inode_to_free);
                                } else { fs.write_inode(inode_to_free, &target_inode); }
                            }
                            return Ok(());
                        }
                    }
                    prev_pos = block_pos;
                    prev_rec_len = entry.rec_len;
                    block_pos += entry.rec_len as usize;
                }
            }
            offset += block_size;
        }
        Err(String::from("File not found"))
    }

    fn create_node_unlocked(&mut self, name: &str, mode: u16) -> Result<Box<dyn VfsNode>, String> {
        if self.find_internal_unlocked(name).is_ok() { return Err(String::from("File already exists")); }
        let fs = unsafe { &*self.fs };
        let inode_id = fs.alloc_inode();
        if inode_id == 0 { return Err(String::from("No free inodes")); }
        let current_time = crate::drivers::rtc::unix_timestamp();
        let new_inode = Inode {
            mode, uid: 0, size: 0, atime: current_time, ctime: current_time, mtime: current_time,
            dtime: 0, gid: 0, links_count: 1, blocks: 0, flags: 0, osd1: 0, block: [0; 15],
            generation: 0, file_acl: 0, dir_acl: 0, faddr: 0, osd2: [0; 3],
        };
        fs.write_inode(inode_id, &new_inode);
        if let Err(e) = self.add_directory_entry_unlocked(inode_id, name, if (mode & 0xF000) == 0x4000 { 2 } else { 1 }) {
            fs.free_inode(inode_id);
            return Err(e);
        }
        Ok(Box::new(Ext2Node {
            fs: self.fs,
            inode_idx: inode_id,
            inode: new_inode,
            name: String::from(name),
        }))
    }

    fn add_directory_entry_unlocked(&mut self, inode_id: u32, name: &str, file_type: u8) -> Result<(), String> {
        let fs = unsafe { &*self.fs };
        let name_len = name.len();
        if name_len > 255 { return Err(String::from("Name too long")); }
        let mut needed_len = 8 + name_len;
        needed_len = (needed_len + 3) & !3;
        let block_size = fs.block_size;
        let mut offset = 0;
        let total_size = self.inode.size as u64;
        while offset < total_size {
            let block_idx = (offset / block_size) as u32;
            let phys = fs.get_block_address(&self.inode, block_idx);
            if phys != 0 {
                let mut buf = vec![0u8; block_size as usize];
                fs.read_disk_data(phys as u64 * block_size, &mut buf);
                let mut block_pos = 0;
                while block_pos < block_size as usize {
                    let ptr = unsafe { buf.as_ptr().add(block_pos) };
                    let entry = unsafe { &mut *(ptr as *mut DirectoryEntry) };
                    if entry.rec_len == 0 { break; }
                    let used_aligned = (8 + entry.name_len as usize + 3) & !3;
                    let available = entry.rec_len as usize - used_aligned;
                    if available >= needed_len {
                        let old_rec_len = entry.rec_len;
                        entry.rec_len = used_aligned as u16;
                        let next_ptr = unsafe { buf.as_mut_ptr().add(block_pos + used_aligned) };
                        let next_entry = unsafe { &mut *(next_ptr as *mut DirectoryEntry) };
                        next_entry.inode = inode_id;
                        next_entry.rec_len = (old_rec_len as usize - used_aligned) as u16;
                        next_entry.name_len = name_len as u8;
                        next_entry.file_type = file_type;
                        unsafe { core::ptr::copy_nonoverlapping(name.as_ptr(), next_ptr.add(8), name_len); }
                        fs.write_disk_data(phys as u64 * block_size, &buf);
                        crate::fs::cache::GLOBAL_PAGE_CACHE.lock().invalidate(fs.disk_id, self.inode_idx as u64, block_idx);
                        return Ok(());
                    }
                    block_pos += entry.rec_len as usize;
                }
            }
            offset += block_size;
        }
        let new_block = fs.alloc_block();
        if new_block == 0 { return Err(String::from("No space")); }
        let block_idx = self.inode.blocks / (block_size as u32 / 512);
        if block_idx < 12 {
            self.inode.block[block_idx as usize] = new_block;
            self.inode.blocks += block_size as u32 / 512;
            self.inode.size += block_size as u32;
            fs.write_inode(self.inode_idx, &self.inode);
        } else { return Err(String::from("Dir too large")); }
        let mut buf = vec![0u8; block_size as usize];
        let entry = unsafe { &mut *(buf.as_mut_ptr() as *mut DirectoryEntry) };
        entry.inode = inode_id;
        entry.rec_len = block_size as u16;
        entry.name_len = name_len as u8;
        entry.file_type = file_type;
        unsafe { core::ptr::copy_nonoverlapping(name.as_ptr(), buf.as_mut_ptr().add(8), name_len); }
        fs.write_disk_data(new_block as u64 * block_size, &buf);
        crate::fs::cache::GLOBAL_PAGE_CACHE.lock().invalidate(fs.disk_id, self.inode_idx as u64, block_idx);
        Ok(())
    }
}
