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
}

impl Ext2 {
    pub fn new(disk_id: u8, base_lba: u64) -> Result<Box<Self>, String> {
        let mut superblock = unsafe { core::mem::zeroed::<Superblock>() };
        let mut buf = alloc::vec![0u8; 1024];
        
        disk::read(base_lba + 2, disk_id, &mut buf[0..512]);
        disk::read(base_lba + 3, disk_id, &mut buf[512..1024]);
        
        unsafe {
            core::ptr::copy_nonoverlapping(buf.as_ptr(), &mut superblock as *mut _ as *mut u8, size_of::<Superblock>());
        }
        
        let magic = unsafe { *(buf.as_ptr().add(56) as *const u16) };

        core::mem::forget(buf); // Leak the buffer to prevent DMA from corrupting other data if it writes late

        if magic != 0xEF53 {
            return Err(alloc::format!("Invalid Ext2 Magic: {:#x} (Expected 0xEF53).", magic));
        }

        let block_size = 1024 << superblock.log_block_size;

        Ok(Box::new(Ext2 {
            disk_id,
            base_lba,
            superblock,
            block_size: block_size as u64,
            inodes_per_group: superblock.inodes_per_group,
        }))
    }

    fn read_disk_data(&self, offset: u64, buffer: &mut [u8]) {
        let abs_offset = offset + (self.base_lba * 512);
        let start_lba = abs_offset / 512;
        let offset_in_sector = (abs_offset % 512) as usize;
        
        let mut current_lba = start_lba;
        let mut bytes_read = 0;
        let total_bytes = buffer.len();
        let mut sector_buf = alloc::vec![0u8; 512]; // Heap allocated
        
        while bytes_read < total_bytes {
            disk::read(current_lba, self.disk_id, &mut sector_buf); // DMA writes to this heap buffer
            
            let start_index = if current_lba == start_lba { offset_in_sector } else { 0 };
            let remaining_in_sector = 512 - start_index;
            let to_copy = core::cmp::min(total_bytes - bytes_read, remaining_in_sector);
            
            buffer[bytes_read..bytes_read + to_copy].copy_from_slice(&sector_buf[start_index..start_index + to_copy]);
            
            bytes_read += to_copy;
            current_lba += 1;
        }
        core::mem::forget(sector_buf); // Leak the buffer
    }

    pub fn read_block_group_descriptor(&self, group_idx: u32) -> BlockGroupDescriptor {
        let bgdt_start_block = if self.block_size == 1024 { 2 } else { 1 };
        let desc_size = size_of::<BlockGroupDescriptor>() as u64;
        
        let offset = (bgdt_start_block as u64 * self.block_size) + (group_idx as u64 * desc_size);
        
        let mut buf = alloc::vec![0u8; size_of::<BlockGroupDescriptor>()]; // Heap allocated
        self.read_disk_data(offset, &mut buf);

        let mut desc = unsafe { core::mem::zeroed::<BlockGroupDescriptor>() };
        unsafe {
            core::ptr::copy_nonoverlapping(buf.as_ptr(), &mut desc as *mut _ as *mut u8, size_of::<BlockGroupDescriptor>());
        }
        core::mem::forget(buf); // Leak the buffer
        desc
    }

    pub fn read_inode(&self, inode_idx: u32) -> Inode {
        crate::debugln!("Reading Inode {}", inode_idx);
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
        
        let mut buf = alloc::vec![0u8; size_of::<Inode>()]; // Heap allocated
        self.read_disk_data(inode_offset, &mut buf);

        let mut inode = unsafe { core::mem::zeroed::<Inode>() };
        unsafe {
            core::ptr::copy_nonoverlapping(buf.as_ptr(), &mut inode as *mut _ as *mut u8, size_of::<Inode>());
        }
        core::mem::forget(buf); // Leak the buffer
        inode
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

    fn read_indirect_pointer(&self, block_addr: u32, offset: u32) -> u32 {
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
        crate::debugln!("Reading file content: offset={}, size={}", offset, buffer.len());
        let fs = unsafe { &*self.fs };
        let mut bytes_read = 0;
        let mut current_offset = offset;
        


        while bytes_read < buffer.len() && current_offset < self.size() {
            let block_idx = (current_offset / fs.block_size) as u32;
            let block_offset = (current_offset % fs.block_size) as usize;
            


            let physical_block = fs.get_block_address(&self.inode, block_idx);
            


            let mut block_buf = alloc::vec![0u8; fs.block_size as usize];
            
            if physical_block != 0 {
                fs.read_disk_data(physical_block as u64 * fs.block_size, &mut block_buf);
            } else {
            }
            
            let available = fs.block_size as usize - block_offset;
            let to_copy = core::cmp::min(buffer.len() - bytes_read, available);
            let to_copy = core::cmp::min(to_copy, (self.size() - current_offset) as usize);
            
            buffer[bytes_read..bytes_read+to_copy].copy_from_slice(&block_buf[block_offset..block_offset+to_copy]);
            
            bytes_read += to_copy;
            current_offset += to_copy as u64;
        }
        
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
        
        let fs = unsafe { &*self.fs };
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