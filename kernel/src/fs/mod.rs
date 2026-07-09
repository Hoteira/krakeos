use crate::virtio;
use alloc::vec::Vec;
use alloc::string::String;

pub mod ramfs;

pub const BLOCK_SIZE: usize = 1024 * 1024; // 1MB
pub const MAX_DESCRIPTORS: usize = 1024;
pub const MAX_BLOCKS: usize = 1024;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Descriptor {
    pub filename: [u8; 64],
    pub size: u32,
    pub is_dir: u8,
    pub _padding1: [u8; 3],
    pub blocks: [u16; 1024],
    pub _padding2: [u8; 1976],
}

impl Descriptor {
    pub fn empty() -> Self {
        Self {
            filename: [0; 64],
            size: 0,
            is_dir: 0,
            _padding1: [0; 3],
            blocks: [0; 1024],
            _padding2: [0; 1976],
        }
    }
}

pub struct KrakeFS {
    pub descriptors: [Descriptor; MAX_DESCRIPTORS],
    pub block_bitmap: [u64; MAX_BLOCKS / 64],
    pub desc_bitmap: [u64; MAX_DESCRIPTORS / 64],
}

static mut FS: KrakeFS = KrakeFS {
    descriptors: [Descriptor {
        filename: [0; 64],
        size: 0,
        is_dir: 0,
        _padding1: [0; 3],
        blocks: [0; 1024],
        _padding2: [0; 1976],
    }; MAX_DESCRIPTORS],
    block_bitmap: [0; MAX_BLOCKS / 64],
    desc_bitmap: [0; MAX_DESCRIPTORS / 64],
};

static mut P2_OFFSET: u64 = 0;

pub fn mount() -> bool {
    crate::println!("Mounting FatSquid (MBR Hybrid)...");
    
    let mut buf = [0u8; 512];
    
    // Read MBR
    unsafe {
        virtio::read_sector(0, buf.as_mut_ptr(), 1);
    }
    
    if buf[510] != 0x55 || buf[511] != 0xAA {
        crate::println!("FatSquid: Invalid MBR signature.");
        return false;
    }
    
    // Read Partition 2
    let p2_type = buf[462 + 4];
    if p2_type != 0x83 {
        crate::println!("FatSquid: Partition 2 is not type 0x83.");
        return false;
    }
    
    let lba_p2 = u32::from_le_bytes([buf[470], buf[471], buf[472], buf[473]]);
    unsafe {
        P2_OFFSET = lba_p2 as u64;
    }
    
    // Read FatSquid Boot Sector (Block 0 of Partition 2)
    unsafe {
        virtio::read_sector(P2_OFFSET, buf.as_mut_ptr(), 1);
    }
    
    let magic = &buf[4..12];
    if magic != b"FTSQUID1" {
        crate::println!("FatSquid: Invalid magic {:?}", core::str::from_utf8(magic).unwrap_or(""));
        return false;
    }
    
    let descriptors = unsafe { &mut *core::ptr::addr_of_mut!(FS.descriptors) };

    // Read the Descriptor Table (Block 1 of Partition 2)
    let desc_ptr = descriptors.as_mut_ptr() as *mut u8;
    
    for i in 0..4 {
        let block_idx = 1 + i;
        let sector = unsafe { P2_OFFSET } + block_idx * 2048;
        let offset = i as usize * BLOCK_SIZE;
        unsafe {
            virtio::read_sector(sector, desc_ptr.add(offset), 2048);
        }
    }
    
    // Check if the root directory is valid
    let root = &descriptors[0];
    if root.filename[0] != b'/' {
        crate::println!("FatSquid format invalid (root directory not found).");
        return false;
    }
    
    // Mark Block 0 (Boot) and Block 1-4 (Desc Table) as used
    unsafe {
        for i in 0..5 {
            mark_block_used(i);
        }
    }

    // Populate bitmaps based on active descriptors
    for i in 0..MAX_DESCRIPTORS {
        let desc = &descriptors[i];
        if desc.filename[0] != 0 {
            unsafe { mark_desc_used(i) };
            
            let mut num_blocks = (desc.size as usize + BLOCK_SIZE - 1) / BLOCK_SIZE;
            if desc.is_dir == 1 {
                if desc.size > 0 {
                    num_blocks = 1;
                } else {
                    num_blocks = 0;
                }
            }

            for b in 0..num_blocks {
                let block_id = desc.blocks[b];
                if block_id != 0 {
                    unsafe { mark_block_used(block_id as usize) };
                }
            }
        }
    }
    
    crate::println!("FatSquid mounted successfully from Partition 2.");
    true
}

pub fn find_file(name: &str) -> Option<usize> {
    if ramfs::is_ram_file(name) {
        return ramfs::find_file(name);
    }
    let descriptors = unsafe { &*core::ptr::addr_of!(FS.descriptors) };
    for i in 0..MAX_DESCRIPTORS {
        let desc = &descriptors[i];
        if desc.filename[0] == 0 { continue; }
        
        let len = name.len().min(64);
        let mut match_ = true;
        for j in 0..len {
            if desc.filename[j] != name.as_bytes()[j] {
                match_ = false;
                break;
            }
        }
        if match_ && (len == 64 || desc.filename[len] == 0) {
            return Some(i);
        }
    }
    None
}

pub fn get_file_size(desc_idx: usize) -> usize {
    if desc_idx >= ramfs::RAMFS_DESC_OFFSET {
        return ramfs::get_file_size(desc_idx);
    }
    let descriptors = unsafe { &*core::ptr::addr_of!(FS.descriptors) };
    descriptors[desc_idx].size as usize
}

fn alloc_block() -> Option<u16> {
    let bitmap = unsafe { &mut *core::ptr::addr_of_mut!(FS.block_bitmap) };
    for i in 0..MAX_BLOCKS {
        let word = i / 64;
        let bit = i % 64;
        if (bitmap[word] & (1 << bit)) == 0 {
            bitmap[word] |= 1 << bit;
            return Some(i as u16);
        }
    }
    None
}

fn alloc_desc() -> Option<usize> {
    let bitmap = unsafe { &mut *core::ptr::addr_of_mut!(FS.desc_bitmap) };
    for i in 0..MAX_DESCRIPTORS {
        let word = i / 64;
        let bit = i % 64;
        if (bitmap[word] & (1 << bit)) == 0 {
            bitmap[word] |= 1 << bit;
            return Some(i);
        }
    }
    None
}

fn sync_desc(idx: usize) {
    let descriptors = unsafe { &mut *core::ptr::addr_of_mut!(FS.descriptors) };
    let desc_ptr = descriptors.as_mut_ptr() as *mut u8;
    let sector = unsafe { P2_OFFSET } + 2048 + (idx as u64 * 8);
    let offset = idx * 4096;
    unsafe {
        virtio::write_sector(sector, desc_ptr.add(offset), 8);
    }
}

pub fn create_file(name: &str) -> Option<usize> {
    if ramfs::is_ram_file(name) {
        return ramfs::create_file(name);
    }
    if find_file(name).is_some() { return None; }
    
    let desc_idx = alloc_desc()?;
    let descriptors = unsafe { &mut *core::ptr::addr_of_mut!(FS.descriptors) };
    let desc = &mut descriptors[desc_idx];
    
    desc.size = 0;
    desc.is_dir = 0;
    for i in 0..1024 { desc.blocks[i] = 0; }
    
    for i in 0..64 { desc.filename[i] = 0; }
    let len = name.len().min(64);
    desc.filename[..len].copy_from_slice(&name.as_bytes()[..len]);
    
    sync_desc(desc_idx);
    Some(desc_idx)
}

pub fn read_file(desc_idx: usize, offset: usize, buf: &mut [u8]) -> usize {
    if desc_idx >= ramfs::RAMFS_DESC_OFFSET {
        return ramfs::read_file(desc_idx, offset, buf);
    }
    let descriptors = unsafe { &*core::ptr::addr_of!(FS.descriptors) };
    let desc = &descriptors[desc_idx];
    if offset >= desc.size as usize { return 0; }
    
    let mut to_read = buf.len();
    if offset + to_read > desc.size as usize {
        to_read = desc.size as usize - offset;
    }
    
    let mut bytes_read = 0;
    while bytes_read < to_read {
        let current_offset = offset + bytes_read;
        let block_idx = current_offset / BLOCK_SIZE;
        let offset_in_block = current_offset % BLOCK_SIZE;
        let block_id = desc.blocks[block_idx];
        if block_id == 0 { break; } // Hole in file
        
        let chunk_size = (BLOCK_SIZE - offset_in_block).min(to_read - bytes_read);
        
        let sector = unsafe { P2_OFFSET } + (block_id as u64) * 2048 + (offset_in_block as u64 / 512);
        let sector_offset = offset_in_block % 512;
        let num_sectors = (chunk_size + sector_offset + 511) / 512;
        
        unsafe {
            virtio::read_sector(sector, core::ptr::addr_of_mut!(IO_BUF) as *mut u8, num_sectors);
            let io_buf = &*core::ptr::addr_of!(IO_BUF);
            buf[bytes_read..bytes_read + chunk_size].copy_from_slice(&io_buf[sector_offset..sector_offset + chunk_size]);
        }
        
        bytes_read += chunk_size;
    }
    
    bytes_read
}

static mut IO_BUF: [u8; BLOCK_SIZE] = [0; BLOCK_SIZE];

pub fn write_file(desc_idx: usize, offset: usize, buf: &[u8]) -> usize {
    if desc_idx >= ramfs::RAMFS_DESC_OFFSET {
        return ramfs::write_file(desc_idx, offset, buf);
    }
    let descriptors = unsafe { &mut *core::ptr::addr_of_mut!(FS.descriptors) };
    let desc = &mut descriptors[desc_idx];
    
    let mut bytes_written = 0;
    let to_write = buf.len();
    
    while bytes_written < to_write {
        let current_offset = offset + bytes_written;
        let block_idx = current_offset / BLOCK_SIZE;
        let offset_in_block = current_offset % BLOCK_SIZE;
        
        if block_idx >= 1024 { break; } // Max file size reached
        
        let mut block_id = desc.blocks[block_idx];
        if block_id == 0 {
            if let Some(new_block) = alloc_block() {
                block_id = new_block;
                desc.blocks[block_idx] = new_block;
                sync_desc(desc_idx);
            } else {
                break; // Disk full
            }
        }
        
        let chunk_size = (BLOCK_SIZE - offset_in_block).min(to_write - bytes_written);
        
        let sector = unsafe { P2_OFFSET } + (block_id as u64) * 2048 + (offset_in_block as u64 / 512);
        let sector_offset = offset_in_block % 512;
        let num_sectors = (chunk_size + sector_offset + 511) / 512;
        
        unsafe {
            // If partial sector write, read first
            if sector_offset != 0 || chunk_size % 512 != 0 {
                virtio::read_sector(sector, core::ptr::addr_of_mut!(IO_BUF) as *mut u8, num_sectors);
            }
            
            let io_buf_mut = &mut *core::ptr::addr_of_mut!(IO_BUF);
            io_buf_mut[sector_offset..sector_offset + chunk_size].copy_from_slice(&buf[bytes_written..bytes_written + chunk_size]);
            virtio::write_sector(sector, core::ptr::addr_of!(IO_BUF) as *const u8, num_sectors);
        }
        
        bytes_written += chunk_size;
    }
    
    if offset + bytes_written > desc.size as usize {
        desc.size = (offset + bytes_written) as u32;
        sync_desc(desc_idx);
    }
    
    bytes_written
}

unsafe fn mark_block_used(idx: usize) {
    let word = idx / 64;
    let bit = idx % 64;
    let bitmap = core::ptr::addr_of_mut!(FS.block_bitmap);
    (*bitmap)[word] |= 1 << bit;
}

unsafe fn mark_desc_used(idx: usize) {
    let word = idx / 64;
    let bit = idx % 64;
    let bitmap = core::ptr::addr_of_mut!(FS.desc_bitmap);
    (*bitmap)[word] |= 1 << bit;
}
