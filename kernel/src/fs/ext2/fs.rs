use crate::sync::YieldMutex;
#[allow(dead_code)]
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::mem::size_of;


use alloc::collections::{BTreeMap, BTreeSet};

use crate::fs::disk;
use crate::fs::ext2::structs::{BlockGroupDescriptor, Inode, Superblock};

#[derive(Debug)]
pub struct Ext2 {
    disk_id: u8,
    base_lba: u64,
    pub superblock: Superblock,
    block_size: u64,
    inodes_per_group: u32,
    inode_size: u16,
    sector_cache: BTreeMap<u64, [u8; 512]>,
    dirty_sectors: BTreeSet<u64>,
    pub lock: YieldMutex<()>,
}

impl Ext2 {
    pub fn new(disk_id: u8, base_lba: u64) -> Result<Box<Self>, String> {
        let mut superblock = unsafe { core::mem::zeroed::<Superblock>() };
        let mut buf = [0u8; 1024];

        crate::debugln!("Ext2: Reading superblock...");
        disk::read(base_lba + 2, disk_id, &mut buf[0..512]);
        disk::read(base_lba + 3, disk_id, &mut buf[512..1024]);
        crate::debugln!("Ext2: Superblock read.");

        unsafe {
            core::ptr::copy_nonoverlapping(buf.as_ptr(), &mut superblock as *mut _ as *mut u8, size_of::<Superblock>());
        }

        let magic = superblock.s_magic;
        crate::debugln!("Ext2: Magic: {:#x}", magic);

        if magic != 0xEF53 {
            return Err(alloc::format!("Invalid Ext2 Magic: {:#x} (Expected 0xEF53).", magic));
        }

        let block_size = 1024 << superblock.log_block_size;
        let inode_size = if superblock.rev_level >= 1 { superblock.inode_size } else { 128 };
        crate::debugln!("Ext2: Mounted. Block Size: {}, Inode Size: {}", block_size, inode_size);

        Ok(Box::new(Ext2 {
            disk_id,
            base_lba,
            superblock,
            block_size: block_size as u64,
            inodes_per_group: superblock.inodes_per_group,
            inode_size,
            sector_cache: BTreeMap::new(),
            dirty_sectors: BTreeSet::new(),
            lock: YieldMutex::new(()),
        }))
    }
}

unsafe impl Send for Ext2 {}
unsafe impl Sync for Ext2 {}

impl Ext2 {
    fn read_disk_data(&mut self, offset: u64, buffer: &mut [u8]) {
        let abs_offset = offset + (self.base_lba * 512);
        let start_lba = abs_offset / 512;
        let offset_in_sector = (abs_offset % 512) as usize;

        // Fast path for aligned, multi-sector reads
        if offset_in_sector == 0 && (buffer.len() % 512) == 0 && buffer.len() >= 512 {
            // Read everything from disk in one go
            disk::read(start_lba, self.disk_id, buffer);
            
            // Patch in any cached sectors
            let num_sectors = (buffer.len() / 512) as u64;
            for i in 0..num_sectors {
                let lba = start_lba + i;
                if let Some(cached) = self.sector_cache.get(&lba) {
                    let buf_start = (i * 512) as usize;
                    buffer[buf_start..buf_start + 512].copy_from_slice(cached);
                }
            }
            return;
        }

        let mut current_lba = start_lba;
        let mut bytes_read = 0;
        let total_bytes = buffer.len();

        while bytes_read < total_bytes {
            let mut temp_buf = [0u8; 512];
            if let Some(cached) = self.sector_cache.get(&current_lba) {
                temp_buf.copy_from_slice(cached);
            } else {
                disk::read(current_lba, self.disk_id, &mut temp_buf);
                if self.sector_cache.len() < 8192 {
                    self.sector_cache.insert(current_lba, temp_buf);
                }
            }

            let start_index = if current_lba == start_lba { offset_in_sector } else { 0 };
            let remaining_in_sector = 512 - start_index;
            let to_copy = core::cmp::min(total_bytes - bytes_read, remaining_in_sector);

            buffer[bytes_read..bytes_read + to_copy].copy_from_slice(&temp_buf[start_index..start_index + to_copy]);

            bytes_read += to_copy;
            current_lba += 1;
        }
    }

    fn write_disk_data(&mut self, offset: u64, buffer: &[u8]) {
        let abs_offset = offset + (self.base_lba * 512);
        let start_lba = abs_offset / 512;
        let offset_in_sector = (abs_offset % 512) as usize;

        let mut current_lba = start_lba;
        let mut bytes_written = 0;
        let total_bytes = buffer.len();

        while bytes_written < total_bytes {
            let mut temp_buf = [0u8; 512];
            let start_index = if current_lba == start_lba { offset_in_sector } else { 0 };
            let remaining_in_sector = 512 - start_index;
            let to_copy = core::cmp::min(total_bytes - bytes_written, remaining_in_sector);

            if to_copy < 512 {
                if let Some(cached) = self.sector_cache.get(&current_lba) {
                    temp_buf.copy_from_slice(cached);
                } else {
                    disk::read(current_lba, self.disk_id, &mut temp_buf);
                }
            }

            temp_buf[start_index..start_index + to_copy].copy_from_slice(&buffer[bytes_written..bytes_written + to_copy]);

            self.sector_cache.insert(current_lba, temp_buf);
            self.dirty_sectors.insert(current_lba);

            if self.dirty_sectors.len() > 8192 {
                self.flush();
            }

            bytes_written += to_copy;
            current_lba += 1;
        }
    }

    pub fn flush(&mut self) {
        if self.dirty_sectors.is_empty() { return; }
        let mut lbas: alloc::vec::Vec<u64> = self.dirty_sectors.iter().copied().collect();
        lbas.sort_unstable();
        
        let mut current_start = lbas[0];
        let mut consecutive_data = alloc::vec::Vec::new();
        
        for &lba in &lbas {
            if lba == current_start + (consecutive_data.len() / 512) as u64 {
                consecutive_data.extend_from_slice(self.sector_cache.get(&lba).unwrap());
            } else {
                disk::write(current_start, self.disk_id, &consecutive_data);
                current_start = lba;
                consecutive_data.clear();
                consecutive_data.extend_from_slice(self.sector_cache.get(&lba).unwrap());
            }
        }
        if !consecutive_data.is_empty() {
            disk::write(current_start, self.disk_id, &consecutive_data);
        }
        self.dirty_sectors.clear();
        
        if self.sector_cache.len() > 16384 {
            self.sector_cache.clear();
        }
    }

    pub fn read_block_group_descriptor(&mut self, group_idx: u32) -> BlockGroupDescriptor {
        let bgdt_start_block = if self.block_size == 1024 { 2 } else { 1 };
        let desc_size = size_of::<BlockGroupDescriptor>() as u64;

        let offset = (bgdt_start_block as u64 * self.block_size) + (group_idx as u64 * desc_size);

        let mut buf = [0u8; size_of::<BlockGroupDescriptor>()];
        self.read_disk_data(offset, &mut buf);

        let mut desc = unsafe { core::mem::zeroed::<BlockGroupDescriptor>() };
        unsafe {
            core::ptr::copy_nonoverlapping(buf.as_ptr(), &mut desc as *mut _ as *mut u8, size_of::<BlockGroupDescriptor>());
        }
        desc
    }

    pub fn write_block_group_descriptor(&mut self, group_idx: u32, desc: &BlockGroupDescriptor) {
        let bgdt_start_block = if self.block_size == 1024 { 2 } else { 1 };
        let desc_size = size_of::<BlockGroupDescriptor>() as u64;
        let offset = (bgdt_start_block as u64 * self.block_size) + (group_idx as u64 * desc_size);

        let ptr = desc as *const BlockGroupDescriptor as *const u8;
        let slice = unsafe { core::slice::from_raw_parts(ptr, size_of::<BlockGroupDescriptor>()) };
        self.write_disk_data(offset, slice);
    }

    pub fn write_superblock(&mut self) {
        let offset = 1024;
        let ptr = &self.superblock as *const Superblock as *const u8;
        let slice = unsafe { core::slice::from_raw_parts(ptr, size_of::<Superblock>()) };
        self.write_disk_data(offset, slice);
    }

    pub fn read_inode(&mut self, inode_idx: u32) -> Inode {
        let group = (inode_idx - 1) / self.inodes_per_group;
        let index_in_group = (inode_idx - 1) % self.inodes_per_group;

        let bg_desc = self.read_block_group_descriptor(group);

        let inode_table_offset = bg_desc.inode_table as u64 * self.block_size;

        let inode_size = self.inode_size;

        let inode_offset = inode_table_offset + (index_in_group as u64 * inode_size as u64);

        let mut buf = [0u8; size_of::<Inode>()];
        self.read_disk_data(inode_offset, &mut buf);

        let mut inode = unsafe { core::mem::zeroed::<Inode>() };
        unsafe {
            core::ptr::copy_nonoverlapping(buf.as_ptr(), &mut inode as *mut _ as *mut u8, size_of::<Inode>());
        }
        inode
    }

    pub fn write_inode(&mut self, inode_idx: u32, inode: &Inode) {
        let group = (inode_idx - 1) / self.inodes_per_group;
        let index_in_group = (inode_idx - 1) % self.inodes_per_group;
        let bg_desc = self.read_block_group_descriptor(group);
        let inode_table_offset = bg_desc.inode_table as u64 * self.block_size;
        let inode_size = self.inode_size;
        let inode_offset = inode_table_offset + (index_in_group as u64 * inode_size as u64);

        let ptr = inode as *const Inode as *const u8;
        let slice = unsafe { core::slice::from_raw_parts(ptr, size_of::<Inode>()) };
        self.write_disk_data(inode_offset, slice);
    }

    pub fn get_block_address(&mut self, inode: &Inode, logical_block: u32) -> u32 {
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
            if first_block == 0 { return 0; }
            return self.read_indirect_pointer(first_block, second_idx);
        }
        indirect_idx -= (ptrs_per_block * ptrs_per_block) as u32;

        let _p3 = ptrs_per_block * ptrs_per_block * ptrs_per_block;
        let first_idx = indirect_idx / (ptrs_per_block * ptrs_per_block) as u32;
        let rem = indirect_idx % (ptrs_per_block * ptrs_per_block) as u32;
        let second_idx = rem / ptrs_per_block as u32;
        let third_idx = rem % ptrs_per_block as u32;

        let first_block = self.read_indirect_pointer(inode.block[14], first_idx);
        if first_block == 0 { return 0; }
        let second_block = self.read_indirect_pointer(first_block, second_idx);
        if second_block == 0 { return 0; }
        return self.read_indirect_pointer(second_block, third_idx);
    }

    pub fn set_block_address(&mut self, inode: &mut Inode, logical_block: u32, phys: u32) -> Result<(), String> {
        let ptrs_per_block = self.block_size / 4;

        if logical_block < 12 {
            inode.block[logical_block as usize] = phys;
            return Ok(());
        }

        let mut indirect_idx = logical_block - 12;

        if indirect_idx < ptrs_per_block as u32 {
            if inode.block[12] == 0 {
                let new_block = self.alloc_block();
                if new_block == 0 { return Err(String::from("No space for indirect block")); }
                inode.block[12] = new_block;

                let zero = alloc::vec![0u8; self.block_size as usize];
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
                if new_block == 0 { return Err(String::from("No space for dbl-indirect block")); }
                inode.block[13] = new_block;
                let zero = alloc::vec![0u8; self.block_size as usize];
                self.write_disk_data(new_block as u64 * self.block_size, &zero);
                inode.blocks += self.block_size as u32 / 512;
            }

            let first_block = inode.block[13];
            let mut second_block = self.read_indirect_pointer(first_block, first_idx);

            if second_block == 0 {
                second_block = self.alloc_block();
                if second_block == 0 { return Err(String::from("No space for dbl-indirect L2")); }
                self.write_indirect_pointer(first_block, first_idx, second_block);
                let zero = alloc::vec![0u8; self.block_size as usize];
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
            if new_block == 0 { return Err(String::from("No space for triple-indirect L1")); }
            inode.block[14] = new_block;
            let zero = alloc::vec![0u8; self.block_size as usize];
            self.write_disk_data(new_block as u64 * self.block_size, &zero);
            inode.blocks += self.block_size as u32 / 512;
        }

        let first_block = inode.block[14];
        let mut second_block = self.read_indirect_pointer(first_block, first_idx);

        if second_block == 0 {
            second_block = self.alloc_block();
            if second_block == 0 { return Err(String::from("No space for triple-indirect L2")); }
            self.write_indirect_pointer(first_block, first_idx, second_block);
            let zero = alloc::vec![0u8; self.block_size as usize];
            self.write_disk_data(second_block as u64 * self.block_size, &zero);
            inode.blocks += self.block_size as u32 / 512;
        }

        let mut third_block = self.read_indirect_pointer(second_block, second_idx);

        if third_block == 0 {
            third_block = self.alloc_block();
            if third_block == 0 { return Err(String::from("No space for triple-indirect L3")); }
            self.write_indirect_pointer(second_block, second_idx, third_block);
            let zero = alloc::vec![0u8; self.block_size as usize];
            self.write_disk_data(third_block as u64 * self.block_size, &zero);
            inode.blocks += self.block_size as u32 / 512;
        }

        self.write_indirect_pointer(third_block, third_idx, phys);
        Ok(())
    }

    fn read_indirect_pointer(&mut self, block_addr: u32, offset: u32) -> u32 {
        if block_addr == 0 { return 0; }

        let read_offset = (block_addr as u64 * self.block_size) + (offset as u64 * 4);
        let mut bytes = [0u8; 4];
        self.read_disk_data(read_offset, &mut bytes);
        u32::from_le_bytes(bytes)
    }

    fn write_indirect_pointer(&mut self, block_addr: u32, offset: u32, val: u32) {
        let write_offset = (block_addr as u64 * self.block_size) + (offset as u64 * 4);
        self.write_disk_data(write_offset, &val.to_le_bytes());
    }

    fn alloc_block(&mut self) -> u32 {
        let alloc_start = unsafe { crate::task::SYSTEM_TICKS };
        let groups = self.superblock.blocks_count / self.superblock.blocks_per_group;
        let block_size_usize = self.block_size as usize;
        
        for i in 0..=groups {
            let mut bg = self.read_block_group_descriptor(i);
            if bg.free_blocks_count > 0 {
                let bitmap_block = bg.block_bitmap;
                let mut bitmap = [0u8; 4096];
                
                let read_start = unsafe { crate::task::SYSTEM_TICKS };
                self.read_disk_data(bitmap_block as u64 * self.block_size, &mut bitmap[..block_size_usize]);
                let read_time = unsafe { crate::task::SYSTEM_TICKS } - read_start;

                for byte_idx in 0..block_size_usize {
                    if bitmap[byte_idx] != 0xFF {
                        for bit_idx in 0..8 {
                            if (bitmap[byte_idx] & (1 << bit_idx)) == 0 {
                                bitmap[byte_idx] |= 1 << bit_idx;
                                
                                let write_start = unsafe { crate::task::SYSTEM_TICKS };
                                self.write_disk_data(bitmap_block as u64 * self.block_size, &bitmap[..block_size_usize]);

                                bg.free_blocks_count -= 1;
                                self.write_block_group_descriptor(i, &bg);

                                self.superblock.free_blocks_count -= 1;
                                self.write_superblock();
                                let write_time = unsafe { crate::task::SYSTEM_TICKS } - write_start;

                                let block_id = (i * self.superblock.blocks_per_group) + (byte_idx as u32 * 8) + bit_idx as u32 + self.superblock.first_data_block;
                                
                                let total_time = unsafe { crate::task::SYSTEM_TICKS } - alloc_start;
                                if total_time > 10 {
                                    crate::debugln!("Ext2::alloc_block SLOW: {} ticks (read bitmap: {}, write metadata: {})", total_time, read_time, write_time);
                                }
                                
                                return block_id;
                            }
                        }
                    }
                }
            }
        }
        0
    }

    fn alloc_inode(&mut self) -> u32 {
        let groups = self.superblock.inodes_count / self.superblock.inodes_per_group;
        let block_size_usize = self.block_size as usize;
        
        for i in 0..=groups {
            let mut bg = self.read_block_group_descriptor(i);
            if bg.free_inodes_count > 0 {
                let bitmap_block = bg.inode_bitmap;
                let mut bitmap = [0u8; 4096];
                self.read_disk_data(bitmap_block as u64 * self.block_size, &mut bitmap[..block_size_usize]);

                for byte_idx in 0..block_size_usize {
                    if bitmap[byte_idx] != 0xFF {
                        for bit_idx in 0..8 {
                            if (bitmap[byte_idx] & (1 << bit_idx)) == 0 {
                                bitmap[byte_idx] |= 1 << bit_idx;
                                self.write_disk_data(bitmap_block as u64 * self.block_size, &bitmap[..block_size_usize]);

                                bg.free_inodes_count -= 1;
                                self.write_block_group_descriptor(i, &bg);

                                self.superblock.free_inodes_count -= 1;
                                self.write_superblock();

                                let inode_id = (i * self.superblock.inodes_per_group) + (byte_idx as u32 * 8) + bit_idx as u32 + 1;
                                return inode_id;
                            }
                        }
                    }
                }
            }
        }
        0
    }

    fn free_block(&mut self, block_id: u32) {
        if block_id == 0 { return; }

        let block_idx = block_id - self.superblock.first_data_block;
        let group = block_idx / self.superblock.blocks_per_group;
        let index_in_group = block_idx % self.superblock.blocks_per_group;

        let mut bg = self.read_block_group_descriptor(group);
        let bitmap_block = bg.block_bitmap;

        let block_size_usize = self.block_size as usize;
        let mut bitmap = [0u8; 4096];
        self.read_disk_data(bitmap_block as u64 * self.block_size, &mut bitmap[..block_size_usize]);

        let byte_idx = (index_in_group / 8) as usize;
        let bit_idx = index_in_group % 8;

        if (bitmap[byte_idx] & (1 << bit_idx)) != 0 {
            bitmap[byte_idx] &= !(1 << bit_idx);
            self.write_disk_data(bitmap_block as u64 * self.block_size, &bitmap[..block_size_usize]);

            bg.free_blocks_count += 1;
            self.write_block_group_descriptor(group, &bg);

            self.superblock.free_blocks_count += 1;
            self.write_superblock();
        }
    }

    fn free_inode(&mut self, inode_id: u32) {
        if inode_id == 0 { return; }

        let inode_idx = inode_id - 1;
        let group = inode_idx / self.superblock.inodes_per_group;
        let index_in_group = inode_idx % self.superblock.inodes_per_group;

        let mut bg = self.read_block_group_descriptor(group);
        let bitmap_block = bg.inode_bitmap;

        let block_size_usize = self.block_size as usize;
        let mut bitmap = [0u8; 4096];
        self.read_disk_data(bitmap_block as u64 * self.block_size, &mut bitmap[..block_size_usize]);

        let byte_idx = (index_in_group / 8) as usize;
        let bit_idx = index_in_group % 8;

        if (bitmap[byte_idx] & (1 << bit_idx)) != 0 {
            bitmap[byte_idx] &= !(1 << bit_idx);
            self.write_disk_data(bitmap_block as u64 * self.block_size, &bitmap[..block_size_usize]);

            bg.free_inodes_count += 1;
            self.write_block_group_descriptor(group, &bg);

            self.superblock.free_inodes_count += 1;
            self.write_superblock();
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
        let self_ptr = self as *mut Ext2;
        let inode = {
            let _lock = self.lock.lock();
            unsafe { (*self_ptr).read_inode(2) }
        };
        Ok(Box::new(Ext2Node {
            fs: self_ptr,
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
        self.inode.size as u64
    }

    fn kind(&self) -> FileType {
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
        crate::fs::vfs::Stat {
            dev: 1,
            ino: self.inode_idx as u64,
            mode: self.inode.mode as u32,
            uid: self.inode.uid as u32,
            gid: self.inode.gid as u32,
            nlink: self.inode.links_count as u32,
            size: self.inode.size as u64,
            atime: self.inode.atime as u64,
            mtime: self.inode.mtime as u64,
            ctime: self.inode.ctime as u64,
        }
    }

    fn read(&mut self, offset: u64, buffer: &mut [u8]) -> Result<usize, String> {
        let fs = unsafe { &mut *self.fs };
        let fs_ptr = fs as *mut Ext2;

        let total_size = self.size();
        if offset >= total_size {
            return Ok(0);
        }

        // Handle fast symlinks (size < 60 bytes and is a symlink)
        if self.kind() == FileType::Symlink && total_size < 60 {
            let mut data = [0u8; 60];
            let inode_block = self.inode.block;

            unsafe {
                core::ptr::copy_nonoverlapping(inode_block.as_ptr() as *const u8, data.as_mut_ptr(), 60);
            }
            let to_copy = core::cmp::min(buffer.len(), (total_size - offset) as usize);
            buffer[..to_copy].copy_from_slice(&data[offset as usize..offset as usize + to_copy]);
            return Ok(to_copy);
        }

        let mut bytes_read = 0;
        let mut current_offset = offset;
        let len = core::cmp::min(buffer.len() as u64, total_size - offset) as usize;

        let block_size = fs.block_size as u64;

        let start_ticks = unsafe { crate::task::SYSTEM_TICKS };
        let mut loop_count = 0;

        while bytes_read < len {
            loop_count += 1;
            let block_idx = (current_offset / block_size) as u32;
            let block_offset = (current_offset % block_size) as usize;

            // Fast path for large, block-aligned reads
            if block_offset == 0 && (len - bytes_read) >= block_size as usize {
                let mut blocks_to_read = 1;
                let max_blocks = ((len - bytes_read) / block_size as usize) as u32;
                
                let start_phys = {
                    let _lock = fs.lock.lock();
                    unsafe { (*fs_ptr).get_block_address(&self.inode, block_idx) }
                };

                if start_phys != 0 {
                    while blocks_to_read < max_blocks {
                        let next_phys = {
                            let _lock = fs.lock.lock();
                            unsafe { (*fs_ptr).get_block_address(&self.inode, block_idx + blocks_to_read) }
                        };
                        if next_phys == start_phys + blocks_to_read {
                            blocks_to_read += 1;
                        } else {
                            break;
                        }
                    }

                    let read_len = (blocks_to_read * block_size as u32) as usize;
                    let target_slice = &mut buffer[bytes_read..bytes_read + read_len];
                    
                    {
                        let _lock = fs.lock.lock();
                        unsafe {
                            (*fs_ptr).read_disk_data(start_phys as u64 * block_size, target_slice);
                        }
                    }

                    bytes_read += read_len;
                    current_offset += read_len as u64;
                    continue;
                }
            }

            let phys = {
                let _lock = fs.lock.lock();
                unsafe { (*fs_ptr).get_block_address(&self.inode, block_idx) }
            };

            let cache_phys = {
                let mut cache = crate::fs::cache::GLOBAL_PAGE_CACHE.lock();
                cache.get_or_load(fs.disk_id, self.inode_idx as u64, block_idx, |dest| {
                    if phys != 0 {
                        let _lock = fs.lock.lock();
                        unsafe { (*fs_ptr).read_disk_data(phys as u64 * block_size, dest) };
                    } else {
                        dest.fill(0);
                    }
                })
            };

            let cache_virt = cache_phys + crate::memory::paging::HHDM_OFFSET;
            let cache_slice = unsafe { core::slice::from_raw_parts(cache_virt as *const u8, block_size as usize) };

            let to_copy = core::cmp::min(len - bytes_read, block_size as usize - block_offset);
            buffer[bytes_read..bytes_read + to_copy].copy_from_slice(&cache_slice[block_offset..block_offset + to_copy]);

            bytes_read += to_copy;
            current_offset += to_copy as u64;
        }

        let end_ticks = unsafe { crate::task::SYSTEM_TICKS };
        if end_ticks - start_ticks > 10 || len > 1024 * 1024 {
            crate::debugln!("Ext2Node::read of {} bytes took {} ticks over {} loops", len, end_ticks - start_ticks, loop_count);
        }

        Ok(bytes_read)
    }

    fn write(&mut self, offset: u64, buffer: &[u8]) -> Result<usize, String> {
        let fs = unsafe { &mut *self.fs };
        let fs_ptr = fs as *mut Ext2;
        let block_size = fs.block_size as u64;

        let mut bytes_written = 0;
        let mut current_offset = offset;
        let mut buf_offset = 0;
        let len = buffer.len();

        let mut bounce_buf = alloc::vec![0u8; fs.block_size as usize];

        let write_start_time = unsafe { crate::task::SYSTEM_TICKS };
        let mut total_alloc_time = 0;
        let mut total_disk_write_time = 0;
        let mut total_blocks_allocated = 0;
        let mut loop_count = 0;

        let mut total_get_block_addr_time = 0;
        let mut total_cache_inval_time = 0;

        crate::debugln!("Ext2Node::write: START len={}", len);

        while bytes_written < len {
            let loop_start = unsafe { crate::task::SYSTEM_TICKS };
            
            let block_idx = (current_offset / block_size) as u32;
            let block_offset = (current_offset % block_size) as usize;

            let get_block_start = unsafe { crate::task::SYSTEM_TICKS };
            let mut phys = {
                let _lock = fs.lock.lock();
                unsafe { (*fs_ptr).get_block_address(&self.inode, block_idx) }
            };
            total_get_block_addr_time += unsafe { crate::task::SYSTEM_TICKS } - get_block_start;

            let mut newly_allocated = false;

            if phys == 0 {
                let alloc_start = unsafe { crate::task::SYSTEM_TICKS };
                phys = {
                    let _lock = fs.lock.lock();
                    unsafe {
                        let p = (*fs_ptr).alloc_block();
                        if p != 0 {
                            if let Ok(_) = (*fs_ptr).set_block_address(&mut self.inode, block_idx, p) {
                                self.inode.blocks += (block_size / 512) as u32;
                                newly_allocated = true;
                            } else {
                                return Err(String::from("Failed to allocate and set block"));
                            }
                        }
                        p
                    }
                };
                total_alloc_time += unsafe { crate::task::SYSTEM_TICKS } - alloc_start;
                total_blocks_allocated += 1;
                if phys == 0 { return Err(String::from("Failed to allocate block")); }
            }

            let io_start = unsafe { crate::task::SYSTEM_TICKS };

            if block_offset != 0 || (len - bytes_written) < block_size as usize {
                let to_copy = core::cmp::min(len - bytes_written, (block_size as usize) - block_offset);
                
                if !newly_allocated {
                    let _lock = fs.lock.lock();
                    unsafe {
                        (*fs_ptr).read_disk_data(phys as u64 * block_size, &mut bounce_buf);
                    }
                } else {
                    bounce_buf.fill(0);
                }
                
                bounce_buf[block_offset..block_offset + to_copy].copy_from_slice(&buffer[buf_offset..buf_offset + to_copy]);
                
                {
                    let _lock = fs.lock.lock();
                    unsafe {
                        (*fs_ptr).write_disk_data(phys as u64 * block_size, &bounce_buf);
                    }
                }

                bytes_written += to_copy;
                current_offset += to_copy as u64;
                buf_offset += to_copy;
            } else {
                let to_copy = block_size as usize;
                {
                    let _lock = fs.lock.lock();
                    unsafe { (*fs_ptr).write_disk_data(phys as u64 * block_size, &buffer[buf_offset..buf_offset + to_copy]) };
                }

                bytes_written += to_copy;
                current_offset += to_copy as u64;
                buf_offset += to_copy;
            }

            total_disk_write_time += unsafe { crate::task::SYSTEM_TICKS } - io_start;
            
            let inval_start = unsafe { crate::task::SYSTEM_TICKS };
            crate::fs::cache::GLOBAL_PAGE_CACHE.lock().invalidate(fs.disk_id, self.inode_idx as u64, block_idx);
            total_cache_inval_time += unsafe { crate::task::SYSTEM_TICKS } - inval_start;

            loop_count += 1;
            if loop_count % 100 == 0 {
                crate::debugln!(
                    "Ext2Node::write: Progress {} / {} bytes... get_block={} alloc={} io={} inval={}", 
                    bytes_written, len, total_get_block_addr_time, total_alloc_time, total_disk_write_time, total_cache_inval_time
                );
            }
        }

        crate::debugln!("Ext2Node::write: Loop done. Ticks so far: {}", unsafe { crate::task::SYSTEM_TICKS } - write_start_time);

        let total_time = unsafe { crate::task::SYSTEM_TICKS } - write_start_time;
        if len >= 1024 {
            crate::debugln!("Ext2 write of {} bytes took {} ticks (alloc_time: {} ticks for {} blocks, io_time: {} ticks)", len, total_time, total_alloc_time, total_blocks_allocated, total_disk_write_time);
        }

        let need_size_update = current_offset > self.inode.size as u64;
        // Always update mtime on write; update size if it grew
        {
            let _lock = fs.lock.lock();
            self.inode.mtime = crate::drivers::rtc::unix_timestamp();
            if need_size_update {
                self.inode.size = current_offset as u32;
            }
            unsafe { (*fs_ptr).write_inode(self.inode_idx, &self.inode) };
            crate::debugln!("Ext2Node::write: Inode updated, about to flush...");
            unsafe { (*fs_ptr).flush() };
            crate::debugln!("Ext2Node::write: Flush done.");
        }

        Ok(bytes_written)
    }

    fn children(&mut self) -> Result<Vec<Box<dyn VfsNode>>, String> {
        if self.kind() != FileType::Directory {
            return Err(String::from("Not a directory"));
        }


        let fs = unsafe { &mut *self.fs };
        let fs_ptr = fs as *mut Ext2;

        let block_size = fs.block_size as usize;

        let mut entries = Vec::new();

        let mut offset = 0;

        let total_size = self.size();


        while offset < total_size {
            let block_idx = (offset / block_size as u64) as u32;

            let phys = {
                let _lock = fs.lock.lock();
                unsafe { (*fs_ptr).get_block_address(&self.inode, block_idx) }
            };


            if phys != 0 {
                let cache_phys = {
                    let mut cache = crate::fs::cache::GLOBAL_PAGE_CACHE.lock();
                    cache.get_or_load(fs.disk_id, self.inode_idx as u64, block_idx, |dest| {
                        if phys != 0 {
                            let _lock = fs.lock.lock();
                            unsafe { (*fs_ptr).read_disk_data(phys as u64 * block_size as u64, dest) };
                        } else {
                            dest.fill(0);
                        }
                    })
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

                        if block_pos + 8 + name_len > block_size { break; }


                        let name_slice = unsafe { core::slice::from_raw_parts(name_ptr, name_len) };

                        let name = String::from_utf8_lossy(name_slice).into_owned();


                        let child_inode = {
                            let _lock = fs.lock.lock();
                            unsafe { (*fs_ptr).read_inode(entry.inode) }
                        };

                        entries.push(Box::new(Ext2Node {
                            fs: self.fs,

                            inode_idx: entry.inode,

                            inode: child_inode,

                            name,

                        }) as Box<dyn VfsNode>);
                    }

                    block_pos += entry.rec_len as usize;
                }
            }

            offset += block_size as u64;
        }

        Ok(entries)
    }


    fn find(&mut self, name: &str) -> Result<Box<dyn VfsNode>, String> {
        if self.kind() != FileType::Directory {
            return Err(String::from("Not a directory"));
        }

        let fs = unsafe { &mut *self.fs };
        let fs_ptr = fs as *mut Ext2;

        // Force reload the inode to get latest size/blocks
        {
            let _lock = fs.lock.lock();
            self.inode = unsafe { (*fs_ptr).read_inode(self.inode_idx) };
        }

        let block_size = fs.block_size as usize;
        let mut offset = 0;
        let total_size = self.size();
        let name_bytes = name.as_bytes();

        while offset < total_size {
            let block_idx = (offset / block_size as u64) as u32;
            let phys = {
                let _lock = fs.lock.lock();
                unsafe { (*fs_ptr).get_block_address(&self.inode, block_idx) }
            };

            if phys != 0 {
                let cache_phys = {
                    let mut cache = crate::fs::cache::GLOBAL_PAGE_CACHE.lock();
                    cache.get_or_load(fs.disk_id, self.inode_idx as u64, block_idx, |dest| {
                        if phys != 0 {
                            let _lock = fs.lock.lock();
                            unsafe { (*fs_ptr).read_disk_data(phys as u64 * block_size as u64, dest) };
                        } else {
                            dest.fill(0);
                        }
                    })
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
                                let child_inode = {
                                    let _lock = fs.lock.lock();
                                    unsafe { (*fs_ptr).read_inode(entry.inode) }
                                };

                                // crate::debugln!("Ext2Node::find: '{}' in '{}'       : V", name, self.name);
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

        // crate::debugln!("Ext2Node::find: '{}' in '{}'       : X", name, self.name);
        Err(String::from("File not found"))
    }


    fn create_file(&mut self, name: &str) -> Result<Box<dyn VfsNode>, String> {
        self.create_node(name, 0x81B4)
    }

    fn create_dir(&mut self, name: &str) -> Result<Box<dyn VfsNode>, String> {
        self.create_node(name, 0x41ED)
    }

    fn remove(&mut self, name: &str) -> Result<(), String> {
        let fs = unsafe { &mut *self.fs };
        let fs_ptr = fs as *mut Ext2;

        let mut buf = alloc::vec![0u8; fs.block_size as usize];
        let mut offset = 0;
        let total_size = self.size();

        while offset < total_size {
            let block_off = offset - (offset % fs.block_size as u64);
            let block_addr = {
                let _lock = fs.lock.lock();
                unsafe { (*fs_ptr).get_block_address(&self.inode, (block_off / fs.block_size as u64) as u32) }
            };
            let read_off = block_addr as u64 * fs.block_size as u64;

            {
                let _lock = fs.lock.lock();
                unsafe { (*fs_ptr).read_disk_data(read_off, &mut buf) };
            }

            let mut block_pos = 0;
            let mut prev_rec_len = 0;
            let mut prev_pos = 0;

            while block_pos < fs.block_size as usize {
                let ptr = unsafe { buf.as_ptr().add(block_pos) };
                let entry = unsafe { &mut *(ptr as *mut DirectoryEntry) };

                if entry.rec_len == 0 { break; }

                // Skip deleted entries
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
                        } else {
                            entry.inode = 0;
                        }


                        {
                            let _lock = fs.lock.lock();
                            unsafe { (*fs_ptr).write_disk_data(read_off, &buf) };
                        }


                        let mut target_inode = {
                            let _lock = fs.lock.lock();
                            unsafe { (*fs_ptr).read_inode(inode_to_free) }
                        };

                        let is_dir = (target_inode.mode & 0xF000) == 0x4000;
                        if is_dir {
                            let mut check_buf = alloc::vec![0u8; fs.block_size as usize];


                            if target_inode.block[0] != 0 {
                                {
                                    let _lock = fs.lock.lock();
                                    unsafe { (*fs_ptr).read_disk_data(target_inode.block[0] as u64 * fs.block_size as u64, &mut check_buf) };
                                }
                                let mut check_pos = 0;
                                let mut entries_count = 0;
                                while check_pos < fs.block_size as usize {
                                    let c_ptr = unsafe { check_buf.as_ptr().add(check_pos) };
                                    let c_entry = unsafe { &*(c_ptr as *const DirectoryEntry) };
                                    if c_entry.rec_len == 0 { break; }
                                    if c_entry.inode != 0 {
                                        entries_count += 1;
                                    }
                                    check_pos += c_entry.rec_len as usize;
                                }

                                if entries_count > 2 {
                                    return Err(String::from("Directory not empty"));
                                }
                            }
                        }

                        if target_inode.links_count > 0 {
                            target_inode.links_count -= 1;
                            if target_inode.links_count == 0 {
                                {
                                    let _lock = fs.lock.lock();
                                    unsafe {
                                        for i in 0..12 {
                                            if target_inode.block[i] != 0 {
                                                (*fs_ptr).free_block(target_inode.block[i]);
                                                target_inode.block[i] = 0;
                                            }
                                        }


                                        (*fs_ptr).write_inode(inode_to_free, &target_inode);
                                        (*fs_ptr).free_inode(inode_to_free);
                                    }
                                }
                            } else {
                                let _lock = fs.lock.lock();
                                unsafe { (*fs_ptr).write_inode(inode_to_free, &target_inode) };
                            }
                        }

                        return Ok(());
                    }
                }

                prev_pos = block_pos;
                prev_rec_len = entry.rec_len;
                block_pos += entry.rec_len as usize;
            }
            offset += fs.block_size as u64;
        }
        Err(String::from("File not found"))
    }

    fn read_dir(&mut self, start_index: u64, buffer: &mut [u8]) -> Result<(usize, usize), String> {
        let fs = unsafe { &mut *self.fs };
        let fs_ptr = fs as *mut Ext2;
        let block_size = fs.block_size as usize;

        let mut bytes_written = 0;
        let mut count_read = 0;
        let mut entry_index: u64 = 0;
        let mut offset = 0;
        let total_size = self.size();

        while offset < total_size {
            let block_idx = (offset / block_size as u64) as u32;
            let phys = {
                let _lock = fs.lock.lock();
                unsafe { (*fs_ptr).get_block_address(&self.inode, block_idx) }
            };

            if phys != 0 {
                let cache_phys = {
                    let mut cache = crate::fs::cache::GLOBAL_PAGE_CACHE.lock();
                    cache.get_or_load(fs.disk_id, self.inode_idx as u64, block_idx, |dest| {
                        if phys != 0 {
                            let _lock = fs.lock.lock();
                            unsafe { (*fs_ptr).read_disk_data(phys as u64 * block_size as u64, dest) };
                        } else {
                            dest.fill(0);
                        }
                    })
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


                            if bytes_written + 2 + name_len > buffer.len() {
                                return Ok((bytes_written, count_read));
                            }

                            let child_inode = {
                                let _lock = fs.lock.lock();
                                unsafe { (*fs_ptr).read_inode(entry.inode) }
                            };

                            let mapped_type = if (child_inode.mode & 0xF000) == 0x4000 {
                                2
                            } else if (child_inode.mode & 0xF000) == 0x8000 {
                                1
                            } else {
                                0
                            };


                            buffer[bytes_written] = mapped_type;


                            buffer[bytes_written + 1] = name_len as u8;

                            let name_ptr = unsafe { ptr.add(8) };
                            unsafe {
                                core::ptr::copy_nonoverlapping(name_ptr, buffer.as_mut_ptr().add(bytes_written + 2), name_len);
                            }

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
        let fs = unsafe { &mut *self.fs };
        let fs_ptr = fs as *mut Ext2;

        let _child = self.find_internal(old_name)?;


        let mut buf = alloc::vec![0u8; fs.block_size as usize];
        let mut offset = 0;
        let total_size = self.size();
        let mut target_inode = 0;
        let mut file_type = 0;


        while offset < total_size {
            let block_off = offset - (offset % fs.block_size as u64);
            let block_addr = {
                let _lock = fs.lock.lock();
                unsafe { (*fs_ptr).get_block_address(&self.inode, (block_off / fs.block_size as u64) as u32) }
            };
            let read_off = block_addr as u64 * fs.block_size as u64;

            {
                let _lock = fs.lock.lock();
                unsafe { (*fs_ptr).read_disk_data(read_off, &mut buf) };
            }
            let mut block_pos = 0;
            while block_pos < fs.block_size as usize {
                let ptr = unsafe { buf.as_ptr().add(block_pos) };
                let entry = unsafe { &*(ptr as *const DirectoryEntry) };
                if entry.rec_len == 0 { break; }
                let name_len = entry.name_len as usize;
                let name_ptr = unsafe { ptr.add(8) };
                let entry_name = unsafe { core::slice::from_raw_parts(name_ptr, name_len) };

                if entry_name == old_name.as_bytes() {
                    target_inode = entry.inode;
                    file_type = entry.file_type;
                    break;
                }
                block_pos += entry.rec_len as usize;
            }
            if target_inode != 0 { break; }
            offset += fs.block_size as u64;
        }

        if target_inode == 0 {
            return Err(String::from("Old file not found"));
        }


        self.add_directory_entry(target_inode, new_name, file_type)?;


        let mut inode = {
            let _lock = fs.lock.lock();
            unsafe { (*fs_ptr).read_inode(target_inode) }
        };
        inode.links_count += 1;
        {
            let _lock = fs.lock.lock();
            unsafe { (*fs_ptr).write_inode(target_inode, &inode) };
        }

        self.remove_internal(old_name)?;

        Ok(())
    }

    fn truncate(&mut self, size: u64) -> Result<(), String> {
        let fs = unsafe { &mut *self.fs };
        let fs_ptr = fs as *mut Ext2;
        let _lock = fs.lock.lock();

        if size > 0xFFFFFFFF {
            return Err(String::from("File too large for Ext2 (32-bit size)"));
        }

        if size == 0 {
            // Support simple truncation to 0 for now
            for i in 0..15 {
                let block = self.inode.block[i];
                if block != 0 {
                    unsafe { (*fs_ptr).free_block(block) };
                    self.inode.block[i] = 0;
                    crate::fs::cache::GLOBAL_PAGE_CACHE.lock().invalidate(fs.disk_id, self.inode_idx as u64, i as u32);
                }
            }
            self.inode.size = 0;
            self.inode.blocks = 0;
            unsafe { (*fs_ptr).write_inode(self.inode_idx, &self.inode) };
            Ok(())
        } else {
            // Partial truncate: for now, just update the size.
            // In a production FS, we would free blocks beyond the new size.
            // Updating size is enough to satisfy metadata checks.
            self.inode.size = size as u32;
            unsafe { (*fs_ptr).write_inode(self.inode_idx, &self.inode) };
            Ok(())
        }
    }

    fn mmap(&mut self, offset: u64, _len: usize) -> Result<u64, String> {
        let fs = unsafe { &mut *self.fs };
        let fs_ptr = fs as *mut Ext2;
        let block_size = fs.block_size as u64;

        let block_idx = (offset / block_size) as u32;
        let block_offset = (offset % block_size) as usize;

        let phys = {
            let _lock = fs.lock.lock();
            unsafe { (*fs_ptr).get_block_address(&self.inode, block_idx) }
        };

        let cache_phys = {
            let mut cache = crate::fs::cache::GLOBAL_PAGE_CACHE.lock();
            cache.get_or_load(fs.disk_id, self.inode_idx as u64, block_idx, |dest| {
                if phys != 0 {
                    let _lock = fs.lock.lock();
                    unsafe { (*fs_ptr).read_disk_data(phys as u64 * block_size, dest) };
                } else {
                    dest.fill(0);
                }
            })
        };

        Ok(cache_phys + crate::memory::paging::HHDM_OFFSET + block_offset as u64)
    }

    fn link(&mut self, name: &str, src: &mut dyn VfsNode) -> Result<(), String> {
        let inode_num = src.inode() as u32;
        let fs = unsafe { &mut *self.fs };
        let fs_ptr = fs as *mut Ext2;

        let mut src_inode = {
            let _lock = fs.lock.lock();
            unsafe { (*fs_ptr).read_inode(inode_num) }
        };

        src_inode.links_count += 1;
        {
            let _lock = fs.lock.lock();
            unsafe { (*fs_ptr).write_inode(inode_num, &src_inode) };
        }

        let file_type = if (src_inode.mode & 0xF000) == 0x4000 { 2 } else { 1 };
        self.add_directory_entry(inode_num, name, file_type)?;

        Ok(())
    }

    fn symlink(&mut self, name: &str, target: &str) -> Result<(), String> {
        let mut node = self.create_node(name, 0xA1FF)?;
        node.write(0, target.as_bytes())?;
        Ok(())
    }

    fn set_times(&mut self, atime: u64, mtime: u64) -> Result<(), String> {
        let fs = unsafe { &mut *self.fs };
        let fs_ptr = fs as *mut Ext2;
        let _lock = fs.lock.lock();

        self.inode.atime = atime as u32;
        self.inode.mtime = mtime as u32;
        unsafe { (*fs_ptr).write_inode(self.inode_idx, &self.inode) };
        Ok(())
    }

    fn readlink(&mut self) -> Result<String, String> {
        if self.kind() != FileType::Symlink {
            return Err(String::from("Not a symlink"));
        }
        let size = self.size() as usize;
        let mut buf = alloc::vec![0u8; size];
        let n = self.read(0, &mut buf)?;
        Ok(String::from_utf8_lossy(&buf[..n]).into_owned())
    }
}

impl Ext2Node {
    fn find_internal(&mut self, name: &str) -> Result<Box<dyn VfsNode>, String> {
        if self.kind() != FileType::Directory {
            return Err(String::from("Not a directory"));
        }

        let fs = unsafe { &mut *self.fs };
        let fs_ptr = fs as *mut Ext2;
        let block_size = fs.block_size as usize;
        let mut offset = 0;
        let total_size = self.size();
        let name_bytes = name.as_bytes();

        while offset < total_size {
            let block_idx = (offset / block_size as u64) as u32;
            let phys = fs.get_block_address(&self.inode, block_idx);

            if phys != 0 {
                let cache_phys = {
                    let mut cache = crate::fs::cache::GLOBAL_PAGE_CACHE.lock();
                    cache.get_or_load(fs.disk_id, self.inode_idx as u64, block_idx, |dest| {
                        if phys != 0 {
                            let _lock = fs.lock.lock();
                            unsafe { (*fs_ptr).read_disk_data(phys as u64 * block_size as u64, dest) };
                        } else {
                            dest.fill(0);
                        }
                    })
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

    fn remove_internal(&mut self, name: &str) -> Result<(), String> {
        let fs = unsafe { &mut *self.fs };
        let fs_ptr = fs as *mut Ext2;

        let mut buf = alloc::vec![0u8; fs.block_size as usize];
        let mut offset = 0;
        let total_size = self.size();

        while offset < total_size {
            let block_off = offset - (offset % fs.block_size as u64);
            let block_addr = {
                let _lock = fs.lock.lock();
                unsafe { (*fs_ptr).get_block_address(&self.inode, (block_off / fs.block_size as u64) as u32) }
            };
            let read_off = block_addr as u64 * fs.block_size as u64;

            {
                let _lock = fs.lock.lock();
                unsafe { (*fs_ptr).read_disk_data(read_off, &mut buf) };
            }

            let mut block_pos = 0;
            let mut prev_rec_len = 0;
            let mut prev_pos = 0;

            while block_pos < fs.block_size as usize {
                let ptr = unsafe { buf.as_ptr().add(block_pos) };
                let entry = unsafe { &mut *(ptr as *mut DirectoryEntry) };

                if entry.rec_len == 0 { break; }

                let name_len = entry.name_len as usize;
                let name_ptr = unsafe { ptr.add(8) };
                let entry_name = unsafe { core::slice::from_raw_parts(name_ptr, name_len) };

                if entry_name == name.as_bytes() {
                    let inode_to_free = entry.inode;

                    if prev_rec_len > 0 {
                        let prev_ptr = unsafe { buf.as_mut_ptr().add(prev_pos) };
                        let prev_entry = unsafe { &mut *(prev_ptr as *mut DirectoryEntry) };
                        prev_entry.rec_len += entry.rec_len;
                    } else {
                        entry.inode = 0;
                    }


                    {
                        let _lock = fs.lock.lock();
                        unsafe { (*fs_ptr).write_disk_data(read_off, &buf) };
                    }

                    // Invalidate cache for this directory block
                    let block_idx = (block_off / fs.block_size as u64) as u32;
                    crate::fs::cache::GLOBAL_PAGE_CACHE.lock().invalidate(fs.disk_id, self.inode_idx as u64, block_idx);


                    let mut target_inode = {
                        let _lock = fs.lock.lock();
                        unsafe { (*fs_ptr).read_inode(inode_to_free) }
                    };

                    let is_dir = (target_inode.mode & 0xF000) == 0x4000;
                    if is_dir {
                        let mut check_buf = alloc::vec![0u8; fs.block_size as usize];
                        if target_inode.block[0] != 0 {
                            {
                                let _lock = fs.lock.lock();
                                unsafe { (*fs_ptr).read_disk_data(target_inode.block[0] as u64 * fs.block_size as u64, &mut check_buf) };
                            }
                            let mut check_pos = 0;
                            let mut entries_count = 0;
                            while check_pos < fs.block_size as usize {
                                let c_ptr = unsafe { check_buf.as_ptr().add(check_pos) };
                                let c_entry = unsafe { &*(c_ptr as *const DirectoryEntry) };
                                if c_entry.rec_len == 0 { break; }
                                if c_entry.inode != 0 {
                                    entries_count += 1;
                                }
                                check_pos += c_entry.rec_len as usize;
                            }

                            if entries_count > 2 {
                                return Err(String::from("Directory not empty"));
                            }
                        }
                    }

                    if target_inode.links_count > 0 {
                        target_inode.links_count -= 1;
                        if target_inode.links_count == 0 {
                            {
                                let _lock = fs.lock.lock();
                                unsafe {
                                    for i in 0..12 {
                                        if target_inode.block[i] != 0 {
                                            (*fs_ptr).free_block(target_inode.block[i]);
                                            target_inode.block[i] = 0;
                                        }
                                    }


                                    (*fs_ptr).write_inode(inode_to_free, &target_inode);
                                    (*fs_ptr).free_inode(inode_to_free);
                                }
                            }
                        } else {
                            let _lock = fs.lock.lock();
                            unsafe { (*fs_ptr).write_inode(inode_to_free, &target_inode) };
                        }
                    }

                    return Ok(());
                }

                prev_pos = block_pos;
                prev_rec_len = entry.rec_len;
                block_pos += entry.rec_len as usize;
            }
            offset += fs.block_size as u64;
        }
        Err(String::from("File not found"))
    }

    fn create_node(&mut self, name: &str, mode: u16) -> Result<Box<dyn VfsNode>, String> {
        if let Ok(_) = self.find_internal(name) {
            return Err(String::from("File already exists"));
        }

        let fs = unsafe { &mut *self.fs };
        let fs_ptr = fs as *mut Ext2;


        let inode_id = {
            let _lock = fs.lock.lock();
            unsafe { (*fs_ptr).alloc_inode() }
        };
        if inode_id == 0 { return Err(String::from("No free inodes")); }

        let current_time = crate::drivers::rtc::unix_timestamp();

        let new_inode = Inode {
            mode,
            uid: 0,
            size: 0,
            atime: current_time,
            ctime: current_time,
            mtime: current_time,
            dtime: 0,
            gid: 0,
            links_count: 1,
            blocks: 0,
            flags: 0,
            osd1: 0,
            block: [0; 15],
            generation: 0,
            file_acl: 0,
            dir_acl: 0,
            faddr: 0,
            osd2: [0; 3],
        };

        {
            let _lock = fs.lock.lock();
            unsafe { (*fs_ptr).write_inode(inode_id, &new_inode) };
        }


        if let Err(e) = self.add_directory_entry(inode_id, name, if (mode & 0xF000) == 0x4000 { 2 } else { 1 }) {
            {
                let _lock = fs.lock.lock();
                unsafe { (*fs_ptr).free_inode(inode_id) };
            }
            return Err(e);
        }

        Ok(Box::new(Ext2Node {
            fs: self.fs,
            inode_idx: inode_id,
            inode: new_inode,
            name: String::from(name),
        }))
    }

    fn add_directory_entry(&mut self, inode_id: u32, name: &str, file_type: u8) -> Result<(), String> {
        let fs = unsafe { &mut *self.fs };
        let fs_ptr = fs as *mut Ext2;
        let name_len = name.len();
        if name_len > 255 { return Err(String::from("Name too long")); }

        let mut needed_len = 8 + name_len;
        needed_len = (needed_len + 3) & !3;

        let mut buf = alloc::vec![0u8; fs.block_size as usize];
        let mut offset = 0;
        let total_size = self.size();

        // crate::debugln!("Ext2: add_dir_entry('{}', ino={}) into '{}' (size={})", name, inode_id, self.name, total_size);

        while offset < total_size {
            let block_off = offset - (offset % fs.block_size as u64);

            let block_addr = {
                let _lock = fs.lock.lock();
                unsafe { (*fs_ptr).get_block_address(&self.inode, (block_off / fs.block_size as u64) as u32) }
            };
            if block_addr == 0 {
                offset += fs.block_size as u64;
                continue;
            }

            let read_off = block_addr as u64 * fs.block_size as u64;

            {
                let _lock = fs.lock.lock();
                unsafe { (*fs_ptr).read_disk_data(read_off, &mut buf) };
            }

            let mut block_pos = 0;
            while block_pos < fs.block_size as usize {
                let ptr = unsafe { buf.as_ptr().add(block_pos) };
                let entry = unsafe { &mut *(ptr as *mut DirectoryEntry) };

                if entry.rec_len == 0 {
                    // crate::debugln!("Ext2: Zero rec_len at pos {}, stopping block scan", block_pos);
                    break;
                }

                let used_len = 8 + entry.name_len as usize;
                let used_aligned = (used_len + 3) & !3;

                let available = entry.rec_len as usize - used_aligned;

                if available >= needed_len {
                    // crate::debugln!("Ext2: Found space in existing block {} at pos {} (avail={})", block_addr, block_pos, available);
                    let old_rec_len = entry.rec_len;
                    entry.rec_len = used_aligned as u16;

                    let next_ptr = unsafe { buf.as_mut_ptr().add(block_pos + used_aligned) };
                    let next_entry = unsafe { &mut *(next_ptr as *mut DirectoryEntry) };

                    next_entry.inode = inode_id;
                    next_entry.rec_len = (old_rec_len as usize - used_aligned) as u16;
                    next_entry.name_len = name_len as u8;
                    next_entry.file_type = file_type;

                    let name_dest = unsafe { next_ptr.add(8) };
                    unsafe {
                        core::ptr::copy_nonoverlapping(name.as_ptr(), name_dest, name_len);
                    }

                    {
                        let _lock = fs.lock.lock();
                        unsafe { (*fs_ptr).write_disk_data(read_off, &buf) };
                    }

                    // Invalidate the cache for this directory block
                    let block_idx = (block_off / fs.block_size as u64) as u32;
                    crate::fs::cache::GLOBAL_PAGE_CACHE.lock().invalidate(fs.disk_id, self.inode_idx as u64, block_idx);

                    // Reload our own inode to ensure size/blocks are current
                    {
                        let _lock = fs.lock.lock();
                        self.inode = unsafe { (*fs_ptr).read_inode(self.inode_idx) };
                    }

                    return Ok(());
                }

                block_pos += entry.rec_len as usize;
            }

            offset += fs.block_size as u64;
        }

        // crate::debugln!("Ext2: No space in existing blocks, allocating new block...");
        let new_block = {
            let _lock = fs.lock.lock();
            unsafe { (*fs_ptr).alloc_block() }
        };
        if new_block == 0 { return Err(String::from("No space for dir entry")); }


        let block_idx = self.inode.blocks / (fs.block_size as u32 / 512);
        if block_idx < 12 {
            {
                let _lock = fs.lock.lock();
                self.inode.block[block_idx as usize] = new_block;
                self.inode.blocks += fs.block_size as u32 / 512;
                self.inode.size += fs.block_size as u32;
                unsafe { (*fs_ptr).write_inode(self.inode_idx, &self.inode) };
            }
        } else {
            return Err(String::from("Dir too large"));
        }


        buf.fill(0);
        let entry = unsafe { &mut *(buf.as_mut_ptr() as *mut DirectoryEntry) };
        entry.inode = inode_id;
        entry.rec_len = fs.block_size as u16;
        entry.name_len = name_len as u8;
        entry.file_type = file_type;

        let name_dest = unsafe { buf.as_mut_ptr().add(8) };
        unsafe {
            core::ptr::copy_nonoverlapping(name.as_ptr(), name_dest, name_len);
        }

        {
            let _lock = fs.lock.lock();
            unsafe { (*fs_ptr).write_disk_data(new_block as u64 * fs.block_size as u64, &buf) };
        }

        // Invalidate cache for the new block (although it shouldn't be in cache yet, safe to be sure)
        // But more importantly, if we updated the inode (which we did), future 'get_block_address' calls are fine.
        // If we previously tried to read this logical block index and it returned 0, it might be cached as all-zeros?
        // GLOBAL_PAGE_CACHE uses (inode, block_idx) as key. If we tried to read block_idx N before it was allocated,
        // Ext2Node::read would have filled it with zeros. We need to invalidate that.
        crate::fs::cache::GLOBAL_PAGE_CACHE.lock().invalidate(fs.disk_id, self.inode_idx as u64, block_idx);

        Ok(())
    }
}