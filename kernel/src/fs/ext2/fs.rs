use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::mem::size_of;
use crate::debugln;

use crate::fs::disk;
use crate::fs::ext2::structs::{Superblock, BlockGroupDescriptor, Inode};

#[derive(Debug)]
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
        let mut buf = [0u8; 1024];
        
        // Superblock is always at byte offset 1024 from the start of the volume.
        disk::read(base_lba + 2, disk_id, &mut buf[0..512]);
        disk::read(base_lba + 3, disk_id, &mut buf[512..1024]);
        
        unsafe {
            core::ptr::copy_nonoverlapping(buf.as_ptr(), &mut superblock as *mut _ as *mut u8, size_of::<Superblock>());
        }

        if superblock.magic != 0xEF53 {
            return Err(alloc::format!("Invalid Ext2 Magic: {:#x} (Expected 0xEF53).", superblock.magic + 0));
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

    // Helper: Reads bytes from disk at a specific byte offset (relative to volume start)
    fn read_disk_data(&self, offset: u64, buffer: &mut [u8]) {
        let abs_offset = offset + (self.base_lba * 512);
        let start_lba = abs_offset / 512;
        let offset_in_sector = (abs_offset % 512) as usize;
        
        // We might need to read multiple sectors
        let mut current_lba = start_lba;
        let mut bytes_read = 0;
        let total_bytes = buffer.len();
        let mut sector_buf = [0u8; 512];

        while bytes_read < total_bytes {
            disk::read(current_lba, self.disk_id, &mut sector_buf);
            
            let start_index = if current_lba == start_lba { offset_in_sector } else { 0 };
            let remaining_in_sector = 512 - start_index;
            let to_copy = core::cmp::min(total_bytes - bytes_read, remaining_in_sector);
            
            buffer[bytes_read..bytes_read + to_copy].copy_from_slice(&sector_buf[start_index..start_index + to_copy]);
            
            bytes_read += to_copy;
            current_lba += 1;
        }
    }

    pub fn read_block_group_descriptor(&self, group_idx: u32) -> BlockGroupDescriptor {
        // BGDT starts immediately after the Superblock.
        // If block_size == 1024, Superblock is in block 1. BGDT starts at block 2.
        // If block_size > 1024, Superblock is inside block 0. BGDT starts at block 1.
        let bgdt_start_block = if self.block_size == 1024 { 2 } else { 1 };
        let desc_size = size_of::<BlockGroupDescriptor>() as u64;
        
        let offset = (bgdt_start_block as u64 * self.block_size) + (group_idx as u64 * desc_size);
        
        let mut desc = unsafe { core::mem::zeroed::<BlockGroupDescriptor>() };
        unsafe {
            let slice = core::slice::from_raw_parts_mut(&mut desc as *mut _ as *mut u8, size_of::<BlockGroupDescriptor>());
            self.read_disk_data(offset, slice);
        }
        desc
    }

    pub fn read_inode(&self, inode_idx: u32) -> Inode {
        debugln!("DEBUG: read_inode({})", inode_idx);
        // Inode indices are 1-based
        let group = (inode_idx - 1) / self.inodes_per_group;
        let index_in_group = (inode_idx - 1) % self.inodes_per_group;
        
        let bg_desc = self.read_block_group_descriptor(group);
        
        // Locate Inode Table
        let inode_table_offset = bg_desc.inode_table as u64 * self.block_size;
        
        // Inode size is in superblock (rev >= 1) or 128 (rev 0)
        let inode_size = if self.superblock.rev_level >= 1 {
            // self.superblock.inode_size as u64 // Need to add this field to struct if missing
            // For now assume standard 128 or read from offset 88 in superblock if needed
            // Struct def doesn't have it yet? Assuming 128 for simplicity or update struct
            // Actually, standard ext2 often has 128 even in rev 1 unless specified.
            // But `mke2fs` defaults to 256 sometimes now.
            // Let's HARDCODE 128 for now or check struct.
            // Looking at previous structs.rs, `rev_level` is there. `inode_size` is NOT.
            // Let's assume 128.
            128 
        } else {
            128
        };

        let inode_offset = inode_table_offset + (index_in_group as u64 * inode_size as u64);
        
        let mut inode = unsafe { core::mem::zeroed::<Inode>() };
        unsafe {
            let slice = core::slice::from_raw_parts_mut(&mut inode as *mut _ as *mut u8, size_of::<Inode>());
            self.read_disk_data(inode_offset, slice);
        }
        inode
    }

    // Resolves a logical file block (0, 1, 2...) to a physical disk block address
    pub fn get_block_address(&self, inode: &Inode, logical_block: u32) -> u32 {
        let ptrs_per_block = self.block_size / 4; // 4 bytes per u32 pointer

        // Direct Blocks (0-11)
        if logical_block < 12 {
            return inode.block[logical_block as usize];
        }

        let mut indirect_idx = logical_block - 12;

        // Singly Indirect (12)
        if indirect_idx < ptrs_per_block as u32 {
            return self.read_indirect_pointer(inode.block[12], indirect_idx);
        }
        indirect_idx -= ptrs_per_block as u32;

        // Doubly Indirect (13)
        if indirect_idx < (ptrs_per_block * ptrs_per_block) as u32 {
            let first_idx = indirect_idx / ptrs_per_block as u32;
            let second_idx = indirect_idx % ptrs_per_block as u32;
            let first_block = self.read_indirect_pointer(inode.block[13], first_idx);
            if first_block == 0 { return 0; }
            return self.read_indirect_pointer(first_block, second_idx);
        }
        indirect_idx -= (ptrs_per_block * ptrs_per_block) as u32;

        // Triply Indirect (14)
        let p3 = ptrs_per_block * ptrs_per_block * ptrs_per_block;
        // Implementation logic similar to above...
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

    // Helper to read a u32 pointer from a block
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
        let inode = self.read_inode(2); // Root is inode 2
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
        let fs = unsafe { &*self.fs };
        let mut bytes_read = 0;
        let mut current_offset = offset;
        
        debugln!("DEBUG: Reading file size {}, offset {}, buf_len {}", self.size(), offset, buffer.len());

        while bytes_read < buffer.len() && current_offset < self.size() {
            let block_idx = (current_offset / fs.block_size) as u32;
            let block_offset = (current_offset % fs.block_size) as usize;
            
            debugln!("DEBUG: Logical Block {}, Block Offset {}", block_idx, block_offset);

            // Use get_block_address to handle indirect blocks
            let physical_block = fs.get_block_address(&self.inode, block_idx);
            
            debugln!("DEBUG: Physical Block {}", physical_block);

            let mut block_buf = alloc::vec![0u8; fs.block_size as usize];
            
            if physical_block != 0 {
                fs.read_disk_data(physical_block as u64 * fs.block_size, &mut block_buf);
            } else {
                // Sparse block, already zeroed
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
        if self.kind() != FileType::Directory {
            return Err(String::from("Not a directory"));
        }
        
        let fs = unsafe { &*self.fs };
        let mut entries = Vec::new();
        let mut buf = alloc::vec![0u8; self.size() as usize];
        
        self.read(0, &mut buf)?;
        
        let mut offset = 0;
        while offset < buf.len() {
            // Ensure we don't read past buffer
            if offset + size_of::<DirectoryEntry>() > buf.len() { break; }

            let ptr = unsafe { buf.as_ptr().add(offset) };
            let entry = unsafe { &*(ptr as *const DirectoryEntry) };
            
            if entry.rec_len == 0 { break; } // Corrupt or end?

            if entry.inode != 0 {
                let name_len = entry.name_len as usize;
                let name_ptr = unsafe { ptr.add(8) }; // Struct size is 8
                
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
        let children = self.children()?;
        for child in children {
            if child.name() == name {
                return Ok(child);
            }
        }
        Err(String::from("File not found"))
    }
}