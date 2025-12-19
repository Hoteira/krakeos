#[allow(dead_code)]
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::mem::size_of;


use crate::fs::disk;
use crate::fs::ext2::structs::{Superblock, BlockGroupDescriptor, Inode};

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

        // Optimization: If aligned and reading whole sectors, read directly (bypass cache for large reads)
        if offset_in_sector == 0 && (buffer.len() % 512) == 0 && buffer.len() >= 512 {
            // crate::debugln!("Ext2: Direct Read LBA {}, sectors {}", start_lba, buffer.len()/512);
            disk::read(start_lba, self.disk_id, buffer);
            return;
        }
        
        let mut current_lba = start_lba;
        let mut bytes_read = 0;
        let total_bytes = buffer.len();
        
        while bytes_read < total_bytes {
            // Check cache
            if self.cache_lba != Some(current_lba) {
                // crate::debugln!("Ext2: Cache Miss LBA {}", current_lba);
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

    pub fn read_inode(&mut self, inode_idx: u32) -> Inode {
        // crate::debugln!("Reading Inode {}", inode_idx);
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

    pub fn get_block_address(&mut self, inode: &Inode, logical_block: u32) -> u32 {
        let ptrs_per_block = self.block_size / 4; 

        if logical_block < 12 {
            return inode.block[logical_block as usize];
        }

        let mut indirect_idx = logical_block - 12;

        if indirect_idx < ptrs_per_block as u32 {
            // crate::debugln!("Ext2: Indirect Access LogicBlock: {}", logical_block);
            return self.read_indirect_pointer(inode.block[12], indirect_idx);
        }
        indirect_idx -= ptrs_per_block as u32;

        if indirect_idx < (ptrs_per_block * ptrs_per_block) as u32 {
            //crate::debugln!("Ext2: Dbl-Indirect Access LogicBlock: {}", logical_block);
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

    fn read_indirect_pointer(&mut self, block_addr: u32, offset: u32) -> u32 {
        if block_addr == 0 { return 0; }
        
        let read_offset = (block_addr as u64 * self.block_size) + (offset as u64 * 4);
        let mut bytes = [0u8; 4];
        self.read_disk_data(read_offset, &mut bytes);
        u32::from_le_bytes(bytes)
    }
}

use crate::fs::vfs::{FileSystem, VfsNode, FileType};
use crate::fs::ext2::structs::DirectoryEntry;

pub struct Ext2Node {
    fs: *mut Ext2, 
    inode_idx: u32,
    inode: Inode,
    name: String,
}

impl FileSystem for Ext2 {
    fn root(&mut self) -> Result<Box<dyn VfsNode>, String> {
        crate::debugln!("Ext2::root called");
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

        crate::debugln!("Ext2Node::read: Off={}, BufLen={}", offset, buffer.len());

        let mut bytes_read = 0;
        let mut current_offset = offset;
        let mut buf_offset = 0;
        let len = core::cmp::min(buffer.len() as u64, total_size - offset) as usize;
        
        let block_size = fs.block_size as u64;
        let mut bounce_buf = alloc::vec![0u8; fs.block_size as usize];

        // 1. Handle Start (Unaligned)
        let start_block_offset = (current_offset % block_size) as usize;
        if start_block_offset != 0 {
            crate::debugln!("  Start unaligned read");
            let block_idx = (current_offset / block_size) as u32;
            let phys = fs.get_block_address(&self.inode, block_idx);
            
            if phys != 0 {
                fs.read_disk_data(phys as u64 * block_size, &mut bounce_buf);
            } else {
                bounce_buf.fill(0);
            }
            
            let to_copy = core::cmp::min(len, (block_size as usize) - start_block_offset);
            buffer[0..to_copy].copy_from_slice(&bounce_buf[start_block_offset..start_block_offset+to_copy]);
            
            bytes_read += to_copy;
            current_offset += to_copy as u64;
            buf_offset += to_copy;
        }

        // 2. Handle Middle (Aligned Full Blocks) - Coalesced
        while (len - bytes_read) >= block_size as usize {
            let start_block_idx = (current_offset / block_size) as u32;
            
            // Check logic - maybe redundant reads here
            let start_phys = fs.get_block_address(&self.inode, start_block_idx);
            
            // Check for contiguous run
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
                
                //crate::debugln!("  Mid Aligned: Block {}, Phys {}, Count {}", start_block_idx, start_phys, count);

                let chunk_size = count * block_size as usize;
                let dest_slice = &mut buffer[buf_offset .. buf_offset + chunk_size];
                fs.read_disk_data(start_phys as u64 * block_size, dest_slice);
                
                bytes_read += chunk_size;
                current_offset += chunk_size as u64;
                buf_offset += chunk_size;
            } else {
                crate::debugln!("  Mid Aligned: Sparse Block {}", start_block_idx);
                let chunk_size = block_size as usize;
                let dest_slice = &mut buffer[buf_offset .. buf_offset + chunk_size];
                dest_slice.fill(0);
                
                bytes_read += chunk_size;
                current_offset += chunk_size as u64;
                buf_offset += chunk_size;
            }
        }

        // 3. Handle End (Unaligned)
        if bytes_read < len {
            crate::debugln!("  End unaligned read");
            let block_idx = (current_offset / block_size) as u32;
            let phys = fs.get_block_address(&self.inode, block_idx);
            
            if phys != 0 {
                fs.read_disk_data(phys as u64 * block_size, &mut bounce_buf);
            } else {
                bounce_buf.fill(0);
            }
            
            let to_copy = len - bytes_read;
            buffer[buf_offset..buf_offset+to_copy].copy_from_slice(&bounce_buf[0..to_copy]);
            
            bytes_read += to_copy;
        }

        crate::debugln!("Ext2Node::read done. Bytes read: {}", bytes_read);
        Ok(bytes_read)
    }

    fn write(&mut self, _offset: u64, _buffer: &[u8]) -> Result<usize, String> {
        Err(String::from("Read-only"))
    }

    fn children(&mut self) -> Result<Vec<Box<dyn VfsNode>>, String> {
        crate::debugln!("Listing children of inode {}", self.inode_idx);
        if self.kind() != FileType::Directory {
            return Err(String::from("Not a directory"));
        }
        
        let fs = unsafe { &mut *self.fs };
        let mut entries = Vec::new();
        let mut buf = alloc::vec![0u8; self.size() as usize];
        
        self.read(0, &mut buf)?;
        
        let mut offset = 0;
        while offset < buf.len() {
            if offset + size_of::<DirectoryEntry>() > buf.len() { break; }

            let ptr = unsafe { buf.as_ptr().add(offset) };
            let entry = unsafe { &*(ptr as *const DirectoryEntry) };
            
            if entry.rec_len == 0 { break; } 

            if entry.inode != 0 {
                let name_len = entry.name_len as usize;
                let name_ptr = unsafe { ptr.add(8) }; 
                
                if offset + 8 + name_len > buf.len() { break; }

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
            
            offset += entry.rec_len as usize;
        }
        
        Ok(entries)
    }

    fn find(&mut self, name: &str) -> Result<Box<dyn VfsNode>, String> {
        crate::debugln!("Finding file: {}", name);
        let children = self.children()?;
        for child in children {
            if child.name() == name {
                return Ok(child);
            }
        }
        Err(String::from("File not found"))
    }
}