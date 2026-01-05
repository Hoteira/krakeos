#[allow(dead_code)]
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::mem::size_of;


use crate::fs::disk;
use crate::fs::ext2::structs::{BlockGroupDescriptor, Inode, Superblock};

#[derive(Debug, Clone)]
pub struct Ext2 {
    disk_id: u8,
    base_lba: u64,
    pub superblock: Superblock,
    block_size: u64,
    inodes_per_group: u32,
    cache_lba: Option<u64>,
    cache_data: [u8; 512],
}

impl Ext2 {
    pub fn new(disk_id: u8, base_lba: u64) -> Result<Box<Self>, String> {
        let mut superblock = unsafe { core::mem::zeroed::<Superblock>() };
        let mut buf = [0u8; 1024];

        disk::read(base_lba + 2, disk_id, &mut buf[0..512]);
        disk::read(base_lba + 3, disk_id, &mut buf[512..1024]);

        unsafe {
            core::ptr::copy_nonoverlapping(buf.as_ptr(), &mut superblock as *mut _ as *mut u8, size_of::<Superblock>());
        }

        let magic = unsafe { *(buf.as_ptr().add(56) as *const u16) };

        if magic != 0xEF53 {
            return Err(alloc::format!("Invalid Ext2 Magic: {:#x} (Expected 0xEF53).", magic));
        }

        let block_size = 1024 << superblock.log_block_size;
        crate::debugln!("Ext2: Mounted. Block Size: {}", block_size);

        Ok(Box::new(Ext2 {
            disk_id,
            base_lba,
            superblock,
            block_size: block_size as u64,
            inodes_per_group: superblock.inodes_per_group,
            cache_lba: None,
            cache_data: [0; 512],
        }))
    }

    fn read_disk_data(&mut self, offset: u64, buffer: &mut [u8]) {
        let abs_offset = offset + (self.base_lba * 512);
        let start_lba = abs_offset / 512;
        let offset_in_sector = (abs_offset % 512) as usize;


        if offset_in_sector == 0 && (buffer.len() % 512) == 0 && buffer.len() >= 512 {
            disk::read(start_lba, self.disk_id, buffer);
            return;
        }

        let mut current_lba = start_lba;
        let mut bytes_read = 0;
        let total_bytes = buffer.len();

        while bytes_read < total_bytes {
            if self.cache_lba != Some(current_lba) {
                disk::read(current_lba, self.disk_id, &mut self.cache_data);
                self.cache_lba = Some(current_lba);
            }

            let start_index = if current_lba == start_lba { offset_in_sector } else { 0 };
            let remaining_in_sector = 512 - start_index;
            let to_copy = core::cmp::min(total_bytes - bytes_read, remaining_in_sector);

            buffer[bytes_read..bytes_read + to_copy].copy_from_slice(&self.cache_data[start_index..start_index + to_copy]);

            bytes_read += to_copy;
            current_lba += 1;
        }
    }

    fn write_disk_data(&mut self, offset: u64, buffer: &[u8]) {
        let abs_offset = offset + (self.base_lba * 512);
        let start_lba = abs_offset / 512;
        let offset_in_sector = (abs_offset % 512) as usize;


        if offset_in_sector == 0 && (buffer.len() % 512) == 0 && buffer.len() >= 512 {
            disk::write(start_lba, self.disk_id, buffer);
            return;
        }

        let mut current_lba = start_lba;
        let mut bytes_written = 0;
        let total_bytes = buffer.len();

        while bytes_written < total_bytes {
            if self.cache_lba != Some(current_lba) {
                disk::read(current_lba, self.disk_id, &mut self.cache_data);
                self.cache_lba = Some(current_lba);
            }

            let start_index = if current_lba == start_lba { offset_in_sector } else { 0 };
            let remaining_in_sector = 512 - start_index;
            let to_copy = core::cmp::min(total_bytes - bytes_written, remaining_in_sector);


            self.cache_data[start_index..start_index + to_copy].copy_from_slice(&buffer[bytes_written..bytes_written + to_copy]);


            disk::write(current_lba, self.disk_id, &self.cache_data);

            bytes_written += to_copy;
            current_lba += 1;
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

        let inode_size = if self.superblock.rev_level >= 1 {
            128
        } else {
            128
        };

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
        let inode_size = 128;
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
        let groups = self.superblock.blocks_count / self.superblock.blocks_per_group;
        for i in 0..=groups {
            let mut bg = self.read_block_group_descriptor(i);
            if bg.free_blocks_count > 0 {
                let bitmap_block = bg.block_bitmap;
                let mut bitmap = alloc::vec![0u8; self.block_size as usize];
                self.read_disk_data(bitmap_block as u64 * self.block_size, &mut bitmap);

                for byte_idx in 0..self.block_size as usize {
                    if bitmap[byte_idx] != 0xFF {
                        for bit_idx in 0..8 {
                            if (bitmap[byte_idx] & (1 << bit_idx)) == 0 {
                                bitmap[byte_idx] |= 1 << bit_idx;
                                self.write_disk_data(bitmap_block as u64 * self.block_size, &bitmap);

                                bg.free_blocks_count -= 1;
                                self.write_block_group_descriptor(i, &bg);

                                self.superblock.free_blocks_count -= 1;
                                self.write_superblock();

                                let block_id = (i * self.superblock.blocks_per_group) + (byte_idx as u32 * 8) + bit_idx as u32 + self.superblock.first_data_block;
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
        for i in 0..=groups {
            let mut bg = self.read_block_group_descriptor(i);
            if bg.free_inodes_count > 0 {
                let bitmap_block = bg.inode_bitmap;
                let mut bitmap = alloc::vec![0u8; self.block_size as usize];
                self.read_disk_data(bitmap_block as u64 * self.block_size, &mut bitmap);

                for byte_idx in 0..self.block_size as usize {
                    if bitmap[byte_idx] != 0xFF {
                        for bit_idx in 0..8 {
                            if (bitmap[byte_idx] & (1 << bit_idx)) == 0 {
                                bitmap[byte_idx] |= 1 << bit_idx;
                                self.write_disk_data(bitmap_block as u64 * self.block_size, &bitmap);

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

        let mut bitmap = alloc::vec![0u8; self.block_size as usize];
        self.read_disk_data(bitmap_block as u64 * self.block_size, &mut bitmap);

        let byte_idx = (index_in_group / 8) as usize;
        let bit_idx = index_in_group % 8;

        if (bitmap[byte_idx] & (1 << bit_idx)) != 0 {
            bitmap[byte_idx] &= !(1 << bit_idx);
            self.write_disk_data(bitmap_block as u64 * self.block_size, &bitmap);

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

        let mut bitmap = alloc::vec![0u8; self.block_size as usize];
        self.read_disk_data(bitmap_block as u64 * self.block_size, &mut bitmap);

        let byte_idx = (index_in_group / 8) as usize;
        let bit_idx = index_in_group % 8;

        if (bitmap[byte_idx] & (1 << bit_idx)) != 0 {
            bitmap[byte_idx] &= !(1 << bit_idx);
            self.write_disk_data(bitmap_block as u64 * self.block_size, &bitmap);

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

impl FileSystem for Ext2 {
    fn root(&mut self) -> Result<Box<dyn VfsNode>, String> {
        let inode = self.read_inode(2);
        Ok(Box::new(Ext2Node {
            fs: self as *mut _,
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
        } else {
            FileType::Unknown
        }
    }

    fn read(&mut self, offset: u64, buffer: &mut [u8]) -> Result<usize, String> {
        let fs = unsafe { &mut *self.fs };
        let total_size = self.size();
        if offset >= total_size { return Ok(0); }

        let mut bytes_read = 0;
        let mut current_offset = offset;
        let mut buf_offset = 0;
        let len = core::cmp::min(buffer.len() as u64, total_size - offset) as usize;

        let block_size = fs.block_size as u64;
        let mut bounce_buf = alloc::vec![0u8; fs.block_size as usize];


        let start_block_offset = (current_offset % block_size) as usize;
        if start_block_offset != 0 {
            let block_idx = (current_offset / block_size) as u32;
            let phys = fs.get_block_address(&self.inode, block_idx);

            if phys != 0 {
                fs.read_disk_data(phys as u64 * block_size, &mut bounce_buf);
            } else {
                bounce_buf.fill(0);
            }

            let to_copy = core::cmp::min(len, (block_size as usize) - start_block_offset);
            buffer[0..to_copy].copy_from_slice(&bounce_buf[start_block_offset..start_block_offset + to_copy]);

            bytes_read += to_copy;
            current_offset += to_copy as u64;
            buf_offset += to_copy;
        }


        while (len - bytes_read) >= block_size as usize {
            let start_block_idx = (current_offset / block_size) as u32;

            let start_phys = fs.get_block_address(&self.inode, start_block_idx);

            let mut count = 1;
            let max_blocks = core::cmp::min(32, (len - bytes_read) / block_size as usize);

            if start_phys != 0 {
                while count < max_blocks {
                    let next_phys = fs.get_block_address(&self.inode, start_block_idx + count as u32);
                    if next_phys == start_phys + count as u32 {
                        count += 1;
                    } else {
                        break;
                    }
                }

                let chunk_size = count * block_size as usize;
                let dest_slice = &mut buffer[buf_offset..buf_offset + chunk_size];
                fs.read_disk_data(start_phys as u64 * block_size, dest_slice);

                bytes_read += chunk_size;
                current_offset += chunk_size as u64;
                buf_offset += chunk_size;
            } else {
                let chunk_size = block_size as usize;
                let dest_slice = &mut buffer[buf_offset..buf_offset + chunk_size];
                dest_slice.fill(0);

                bytes_read += chunk_size;
                current_offset += chunk_size as u64;
                buf_offset += chunk_size;
            }
        }


        if bytes_read < len {
            let block_idx = (current_offset / block_size) as u32;
            let phys = fs.get_block_address(&self.inode, block_idx);

            if phys != 0 {
                fs.read_disk_data(phys as u64 * block_size, &mut bounce_buf);
            } else {
                bounce_buf.fill(0);
            }

            let to_copy = len - bytes_read;
            buffer[buf_offset..buf_offset + to_copy].copy_from_slice(&bounce_buf[0..to_copy]);

            bytes_read += to_copy;
        }

        Ok(bytes_read)
    }

    fn write(&mut self, offset: u64, buffer: &[u8]) -> Result<usize, String> {
        let fs = unsafe { &mut *self.fs };
        let block_size = fs.block_size as u64;

        let mut bytes_written = 0;
        let mut current_offset = offset;
        let mut buf_offset = 0;
        let len = buffer.len();

        let mut bounce_buf = alloc::vec![0u8; fs.block_size as usize];

        while bytes_written < len {
            let block_idx = (current_offset / block_size) as u32;
            let block_offset = (current_offset % block_size) as usize;

            let mut phys = fs.get_block_address(&self.inode, block_idx);


            if phys == 0 {
                phys = fs.alloc_block();
                if phys == 0 { return Err(String::from("No free blocks")); }


                if let Err(e) = fs.set_block_address(&mut self.inode, block_idx, phys) {
                    return Err(e);
                }

                self.inode.blocks += (block_size / 512) as u32;
                fs.write_inode(self.inode_idx, &self.inode);


                bounce_buf.fill(0);
                fs.write_disk_data(phys as u64 * block_size, &bounce_buf);
            }


            if block_offset != 0 || (len - bytes_written) < block_size as usize {
                fs.read_disk_data(phys as u64 * block_size, &mut bounce_buf);

                let to_copy = core::cmp::min(len - bytes_written, (block_size as usize) - block_offset);
                bounce_buf[block_offset..block_offset + to_copy].copy_from_slice(&buffer[buf_offset..buf_offset + to_copy]);

                fs.write_disk_data(phys as u64 * block_size, &bounce_buf);

                bytes_written += to_copy;
                current_offset += to_copy as u64;
                buf_offset += to_copy;
            } else {
                let to_copy = block_size as usize;
                fs.write_disk_data(phys as u64 * block_size, &buffer[buf_offset..buf_offset + to_copy]);

                bytes_written += to_copy;
                current_offset += to_copy as u64;
                buf_offset += to_copy;
            }
        }


        if current_offset > self.inode.size as u64 {
            self.inode.size = current_offset as u32;
            fs.write_inode(self.inode_idx, &self.inode);
        }

        Ok(bytes_written)
    }

    fn children(&mut self) -> Result<Vec<Box<dyn VfsNode>>, String> {
        if self.kind() != FileType::Directory {
            return Err(String::from("Not a directory"));
        }


        let fs = unsafe { &mut *self.fs };

        let block_size = fs.block_size as usize;

        let mut entries = Vec::new();

        let mut buf = alloc::vec![0u8; block_size];


        let mut offset = 0;

        let total_size = self.size();


        while offset < total_size {

            // Determine physical block for this offset

            let block_idx = (offset / block_size as u64) as u32;

            let phys = fs.get_block_address(&self.inode, block_idx);


            if phys != 0 {
                fs.read_disk_data(phys as u64 * block_size as u64, &mut buf);


                let mut block_pos = 0;

                while block_pos < block_size {
                    let ptr = unsafe { buf.as_ptr().add(block_pos) };

                    let entry = unsafe { &*(ptr as *const DirectoryEntry) };


                    if entry.rec_len == 0 { break; }


                    if entry.inode != 0 {
                        let name_len = entry.name_len as usize;

                        let name_ptr = unsafe { ptr.add(8) };

                        if block_pos + 8 + name_len > block_size { break; }


                        let name_slice = unsafe { core::slice::from_raw_parts(name_ptr, name_len) };

                        let name = String::from_utf8_lossy(name_slice).into_owned();


                        // Only read the inode if we need to return full nodes. 

                        // Note: Reading inodes for *every* child is still heavy, but memory usage is lower per loop iteration.

                        // Optimization: We could lazy-load inodes, but VfsNode requires size/kind immediately.

                        let child_inode = fs.read_inode(entry.inode);

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

        let block_size = fs.block_size as usize;

        let mut buf = alloc::vec![0u8; block_size];


        let mut offset = 0;

        let total_size = self.size();

        let name_bytes = name.as_bytes();


        while offset < total_size {
            let block_idx = (offset / block_size as u64) as u32;

            let phys = fs.get_block_address(&self.inode, block_idx);


            if phys != 0 {
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
        self.create_node(name, 0x81B4)
    }

    fn create_dir(&mut self, name: &str) -> Result<Box<dyn VfsNode>, String> {
        self.create_node(name, 0x41ED)
    }

    fn remove(&mut self, name: &str) -> Result<(), String> {
        let fs = unsafe { &mut *self.fs };

        let mut buf = alloc::vec![0u8; fs.block_size as usize];
        let mut offset = 0;
        let total_size = self.size();

        while offset < total_size {
            let block_off = offset - (offset % fs.block_size as u64);
            let block_addr = fs.get_block_address(&self.inode, (block_off / fs.block_size as u64) as u32);
            let read_off = block_addr as u64 * fs.block_size as u64;

            fs.read_disk_data(read_off, &mut buf);

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


                    fs.write_disk_data(read_off, &buf);


                    let mut target_inode = fs.read_inode(inode_to_free);

                    let is_dir = (target_inode.mode & 0xF000) == 0x4000;
                    if is_dir {
                        let mut check_buf = alloc::vec![0u8; fs.block_size as usize];
                        let _has_entries = false;


                        if target_inode.block[0] != 0 {
                            fs.read_disk_data(target_inode.block[0] as u64 * fs.block_size as u64, &mut check_buf);
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
                            for i in 0..12 {
                                if target_inode.block[i] != 0 {
                                    fs.free_block(target_inode.block[i]);
                                    target_inode.block[i] = 0;
                                }
                            }


                            fs.write_inode(inode_to_free, &target_inode);
                            fs.free_inode(inode_to_free);
                        } else {
                            fs.write_inode(inode_to_free, &target_inode);
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

    fn read_dir(&mut self, start_index: u64, buffer: &mut [u8]) -> Result<(usize, usize), String> {
        let fs = unsafe { &mut *self.fs };
        let block_size = fs.block_size as usize;

        let mut bytes_written = 0;
        let mut count_read = 0;
        let mut entry_index: u64 = 0;
        let mut offset = 0;
        let total_size = self.size();

        let mut block_buf = alloc::vec![0u8; block_size];

        while offset < total_size {
            let block_idx = (offset / block_size as u64) as u32;
            let phys = fs.get_block_address(&self.inode, block_idx);

            if phys != 0 {
                fs.read_disk_data(phys as u64 * block_size as u64, &mut block_buf);

                let mut block_pos = 0;
                while block_pos < block_size {
                    let ptr = unsafe { block_buf.as_ptr().add(block_pos) };
                    let entry = unsafe { &*(ptr as *const DirectoryEntry) };

                    if entry.rec_len == 0 { break; }

                    if entry.inode != 0 {
                        if entry_index >= start_index {
                            let name_len = entry.name_len as usize;

                            // Check if entry fits in remaining buffer
                            if bytes_written + 2 + name_len > buffer.len() {
                                return Ok((bytes_written, count_read));
                            }

                            let child_inode = fs.read_inode(entry.inode);

                            let mapped_type = if (child_inode.mode & 0xF000) == 0x4000 {
                                2 // Directory

                            } else if (child_inode.mode & 0xF000) == 0x8000 {
                                1 // File

                            } else {
                                0 // Unknown

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
        let _child = self.find(old_name)?;


        let fs = unsafe { &mut *self.fs };
        let mut buf = alloc::vec![0u8; fs.block_size as usize];
        let mut offset = 0;
        let total_size = self.size();
        let mut target_inode = 0;
        let mut file_type = 0;


        while offset < total_size {
            let block_off = offset - (offset % fs.block_size as u64);
            let block_addr = fs.get_block_address(&self.inode, (block_off / fs.block_size as u64) as u32);
            let read_off = block_addr as u64 * fs.block_size as u64;

            fs.read_disk_data(read_off, &mut buf);
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

        if target_inode == 0 { return Err(String::from("Old file not found")); }


        self.add_directory_entry(target_inode, new_name, file_type)?;


        let mut inode = fs.read_inode(target_inode);
        inode.links_count += 1;
        fs.write_inode(target_inode, &inode);

        self.remove(old_name)?;

        Ok(())
    }
}

impl Ext2Node {
    fn create_node(&mut self, name: &str, mode: u16) -> Result<Box<dyn VfsNode>, String> {
        let fs = unsafe { &mut *self.fs };


        let inode_id = fs.alloc_inode();
        if inode_id == 0 { return Err(String::from("No free inodes")); }

        let current_time = 0;

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

        fs.write_inode(inode_id, &new_inode);


        if let Err(e) = self.add_directory_entry(inode_id, name, if (mode & 0xF000) == 0x4000 { 2 } else { 1 }) {
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

    fn add_directory_entry(&mut self, inode_id: u32, name: &str, file_type: u8) -> Result<(), String> {
        let fs = unsafe { &mut *self.fs };
        let name_len = name.len();
        if name_len > 255 { return Err(String::from("Name too long")); }

        let mut needed_len = 8 + name_len;
        needed_len = (needed_len + 3) & !3;

        let mut buf = alloc::vec![0u8; fs.block_size as usize];
        let mut offset = 0;
        let total_size = self.size();


        while offset < total_size {
            let block_off = offset - (offset % fs.block_size as u64);

            let block_addr = fs.get_block_address(&self.inode, (block_off / fs.block_size as u64) as u32);
            let read_off = block_addr as u64 * fs.block_size as u64;

            fs.read_disk_data(read_off, &mut buf);

            let mut block_pos = 0;
            while block_pos < fs.block_size as usize {
                let ptr = unsafe { buf.as_ptr().add(block_pos) };
                let entry = unsafe { &mut *(ptr as *mut DirectoryEntry) };

                if entry.rec_len == 0 { break; }

                let used_len = 8 + entry.name_len as usize;
                let used_aligned = (used_len + 3) & !3;

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

                    let name_dest = unsafe { next_ptr.add(8) };
                    unsafe {
                        core::ptr::copy_nonoverlapping(name.as_ptr(), name_dest, name_len);
                    }


                    let write_addr = fs.get_block_address(&self.inode, (block_off / fs.block_size as u64) as u32);
                    let write_off = write_addr as u64 * fs.block_size as u64;
                    fs.write_disk_data(write_off, &buf);

                    return Ok(());
                }

                block_pos += entry.rec_len as usize;
            }

            offset += fs.block_size as u64;
        }


        let new_block = fs.alloc_block();
        if new_block == 0 { return Err(String::from("No space for dir entry")); }


        let block_idx = self.inode.blocks / (fs.block_size as u32 / 512);
        if block_idx < 12 {
            self.inode.block[block_idx as usize] = new_block;
            self.inode.blocks += fs.block_size as u32 / 512;
            self.inode.size += fs.block_size as u32;
            fs.write_inode(self.inode_idx, &self.inode);
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

        fs.write_disk_data(new_block as u64 * fs.block_size as u64, &buf);

        Ok(())
    }
}