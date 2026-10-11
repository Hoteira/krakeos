use crate::virtio;

pub mod ramfs;

pub const BLOCK_SIZE: usize = 1024 * 1024;
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
    
    virtio::read_sector(0, buf.as_mut_ptr(), 1);

    if buf[510] != 0x55 || buf[511] != 0xAA {
        crate::println!("FatSquid: Invalid MBR signature.");
        return false;
    }
    
    // Partition 2 entry: type at +4, start LBA at +8.
    let p2_type = buf[462 + 4];
    if p2_type != 0x83 {
        crate::println!("FatSquid: Partition 2 is not type 0x83.");
        return false;
    }
    
    let lba_p2 = u32::from_le_bytes([buf[470], buf[471], buf[472], buf[473]]);
    unsafe {
        P2_OFFSET = lba_p2 as u64;
    }
    
    // FatSquid boot sector is block 0 of partition 2.
    unsafe {
        virtio::read_sector(P2_OFFSET, buf.as_mut_ptr(), 1);
    }
    
    let magic = &buf[4..12];
    if magic != b"FTSQUID1" {
        crate::println!("FatSquid: Invalid magic {:?}", core::str::from_utf8(magic).unwrap_or(""));
        return false;
    }
    
    let descriptors = unsafe { &mut *core::ptr::addr_of_mut!(FS.descriptors) };

    // Descriptor table is blocks 1 to 4 of partition 2.
    let desc_ptr = descriptors.as_mut_ptr() as *mut u8;
    
    for i in 0..4 {
        let block_idx = 1 + i;
        let sector = unsafe { P2_OFFSET } + block_idx * 2048;
        let offset = i as usize * BLOCK_SIZE;
        unsafe {
            virtio::read_sector(sector, desc_ptr.add(offset), 2048);
        }
    }
    
    let root = &descriptors[0];
    if root.filename[0] != b'/' {
        crate::println!("FatSquid format invalid (root directory not found).");
        return false;
    }
    
    unsafe {
        for i in 0..5 {
            mark_block_used(i);
        }
    }

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
    
    // Root directory is always at index 0.
    let mut current_dir = 0;
    
    let path = if name.starts_with('/') { &name[1..] } else { name };
    
    let mut parts = path.split('/');
    let mut current_part = parts.next();
    
    while let Some(part) = current_part {
        if part.is_empty() {
            current_part = parts.next();
            continue;
        }
        
        let desc = &descriptors[current_dir];
        if desc.is_dir == 0 {
            return None;
        }
        
        let mut found_child = None;
        
        // A directory's blocks array holds its children's descriptor indices.
        for &child_idx in desc.blocks.iter() {
            if child_idx == 0 { continue; }
            let child_idx = child_idx as usize;
            if child_idx >= MAX_DESCRIPTORS { continue; }
            
            let child = &descriptors[child_idx];
            if child.filename[0] == 0 { continue; }
            
            let len = part.len().min(64);
            let mut match_ = true;
            for j in 0..len {
                if child.filename[j] != part.as_bytes()[j] {
                    match_ = false;
                    break;
                }
            }
            if match_ && (len == 64 || child.filename[len] == 0) {
                found_child = Some(child_idx);
                break;
            }
        }
        
        if let Some(child) = found_child {
            current_part = parts.next();
            if current_part.is_none() {
                return Some(child);
            }
            current_dir = child;
        } else {
            return None;
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

// Descriptor indices double as fds, so skip stdio, the WASI preopen and device fds (100..=127).
pub fn reserved_fd(i: usize) -> bool {
    i <= 3 || (100..=127).contains(&i)
}

fn alloc_desc() -> Option<usize> {
    let bitmap = unsafe { &mut *core::ptr::addr_of_mut!(FS.desc_bitmap) };
    for i in 0..MAX_DESCRIPTORS {
        if reserved_fd(i) {
            continue;
        }
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

pub fn truncate_file(desc_idx: usize, length: usize) {
    if desc_idx >= ramfs::RAMFS_DESC_OFFSET {
        ramfs::truncate_file(desc_idx, length);
        return;
    }
    if desc_idx == 0 || desc_idx >= MAX_DESCRIPTORS {
        return;
    }
    let descriptors = unsafe { &mut *core::ptr::addr_of_mut!(FS.descriptors) };
    let desc = &mut descriptors[desc_idx];
    if desc.is_dir == 0 && (length as u32) < desc.size {
        // mount() only counts blocks within the size, so free the rest now.
        let keep = (length + BLOCK_SIZE - 1) / BLOCK_SIZE;
        for b in desc.blocks[keep..].iter_mut() {
            if *b != 0 {
                free_block(*b as usize);
                *b = 0;
            }
        }
        desc.size = length as u32;
        sync_desc(desc_idx);
    }
}

fn free_block(idx: usize) {
    if idx < 5 || idx >= MAX_BLOCKS {
        return;
    }
    let bitmap = unsafe { &mut *core::ptr::addr_of_mut!(FS.block_bitmap) };
    bitmap[idx / 64] &= !(1 << (idx % 64));
}

fn free_desc(idx: usize) {
    let bitmap = unsafe { &mut *core::ptr::addr_of_mut!(FS.desc_bitmap) };
    bitmap[idx / 64] &= !(1 << (idx % 64));
}

// WASI errno values, returned as-is by the path operations below.
pub const E_EXIST: usize = 20;
pub const E_INVAL: usize = 28;
pub const E_ISDIR: usize = 31;
pub const E_NOENT: usize = 44;
pub const E_NOSPC: usize = 51;
pub const E_NOTDIR: usize = 54;
pub const E_NOTEMPTY: usize = 55;
pub const E_NOTSUP: usize = 58;

fn trim_path(name: &str) -> &str {
    name.trim_start_matches('/').trim_end_matches('/')
}

fn split_parent(name: &str) -> Result<(usize, &str), usize> {
    let path = trim_path(name);
    let (parent_path, base) = match path.rfind('/') {
        Some(i) => (&path[..i], &path[i + 1..]),
        None => ("", path),
    };
    if base.is_empty() || base.len() > 63 || base == "." || base == ".." {
        return Err(E_INVAL);
    }
    let parent = if parent_path.is_empty() { 0 } else { find_file(parent_path).ok_or(E_NOENT)? };
    let descriptors = unsafe { &*core::ptr::addr_of!(FS.descriptors) };
    if parent >= MAX_DESCRIPTORS || descriptors[parent].is_dir == 0 {
        return Err(E_NOTDIR);
    }
    Ok((parent, base))
}

fn link(parent: usize, child: usize) -> Result<(), usize> {
    let descriptors = unsafe { &mut *core::ptr::addr_of_mut!(FS.descriptors) };
    let dir = &mut descriptors[parent];
    let slot = dir.blocks.iter().position(|&b| b == 0).ok_or(E_NOSPC)?;
    dir.blocks[slot] = child as u16;
    dir.size += 1; // a directory's size is its child count
    sync_desc(parent);
    Ok(())
}

fn unlink(parent: usize, child: usize) {
    let descriptors = unsafe { &mut *core::ptr::addr_of_mut!(FS.descriptors) };
    let dir = &mut descriptors[parent];
    if let Some(slot) = dir.blocks.iter().position(|&b| b as usize == child) {
        dir.blocks[slot] = 0;
        dir.size = dir.size.saturating_sub(1);
        sync_desc(parent);
    }
}

fn set_name(desc: &mut Descriptor, base: &str) {
    desc.filename = [0; 64];
    desc.filename[..base.len()].copy_from_slice(base.as_bytes());
}

// For WASI path_filestat_get.
pub fn stat(name: &str) -> Option<(usize, bool)> {
    if trim_path(name).is_empty() {
        return Some((0, true));
    }
    if ramfs::is_ram_file(name) {
        let idx = ramfs::find_file(name)?;
        return Some((ramfs::get_file_size(idx), false));
    }
    let idx = find_file(name)?;
    let descriptors = unsafe { &*core::ptr::addr_of!(FS.descriptors) };
    let d = &descriptors[idx];
    Some((d.size as usize, d.is_dir != 0))
}

pub fn make_dir(name: &str) -> Result<usize, usize> {
    if ramfs::is_ram_file(name) {
        return Err(E_NOTSUP);
    }
    let (parent, base) = split_parent(name)?;
    if find_file(name).is_some() {
        return Err(E_EXIST);
    }
    let idx = alloc_desc().ok_or(E_NOSPC)?;
    let descriptors = unsafe { &mut *core::ptr::addr_of_mut!(FS.descriptors) };
    let desc = &mut descriptors[idx];
    *desc = Descriptor::empty();
    desc.is_dir = 1;
    set_name(desc, base);
    sync_desc(idx);
    if let Err(e) = link(parent, idx) {
        desc.filename = [0; 64];
        sync_desc(idx);
        free_desc(idx);
        return Err(e);
    }
    Ok(idx)
}

pub fn remove(name: &str, dir: bool) -> Result<(), usize> {
    if ramfs::is_ram_file(name) {
        return Err(E_NOTSUP);
    }
    let (parent, _) = split_parent(name)?;
    let idx = find_file(name).ok_or(E_NOENT)?;
    let descriptors = unsafe { &mut *core::ptr::addr_of_mut!(FS.descriptors) };
    let desc = &mut descriptors[idx];
    if (desc.is_dir != 0) != dir {
        return Err(if dir { E_NOTDIR } else { E_ISDIR });
    }
    if dir {
        if desc.blocks.iter().any(|&b| b != 0) {
            return Err(E_NOTEMPTY);
        }
    } else {
        let used = (desc.size as usize + BLOCK_SIZE - 1) / BLOCK_SIZE;
        for (i, b) in desc.blocks.iter().enumerate() {
            // Blocks past the size may belong to another file already.
            if *b != 0 && i < used {
                free_block(*b as usize);
            }
        }
    }
    unlink(parent, idx);
    *desc = Descriptor::empty();
    sync_desc(idx);
    free_desc(idx);
    Ok(())
}

// An existing file at the target is replaced; an existing directory is not.
pub fn rename(from: &str, to: &str) -> Result<(), usize> {
    if ramfs::is_ram_file(from) || ramfs::is_ram_file(to) {
        return Err(E_NOTSUP);
    }
    let (from_parent, _) = split_parent(from)?;
    let idx = find_file(from).ok_or(E_NOENT)?;
    let (to_parent, base) = split_parent(to)?;
    let (a, b) = (trim_path(from), trim_path(to));
    if a == b {
        return Ok(());
    }
    // A directory can't move inside itself.
    if b.len() > a.len() && b.starts_with(a) && b.as_bytes()[a.len()] == b'/' {
        return Err(E_INVAL);
    }
    if let Some(existing) = find_file(to) {
        let descriptors = unsafe { &*core::ptr::addr_of!(FS.descriptors) };
        if descriptors[existing].is_dir != 0 {
            return Err(E_EXIST);
        }
        remove(to, false)?;
    }
    if from_parent != to_parent {
        link(to_parent, idx)?;
        unlink(from_parent, idx);
    }
    let descriptors = unsafe { &mut *core::ptr::addr_of_mut!(FS.descriptors) };
    set_name(&mut descriptors[idx], base);
    sync_desc(idx);
    Ok(())
}

// The parent must exist; the descriptor stores only the last path component.
pub fn create_file(name: &str) -> Option<usize> {
    if ramfs::is_ram_file(name) {
        return ramfs::create_file(name);
    }
    if find_file(name).is_some() { return None; }
    let (parent, base) = split_parent(name).ok()?;
    let descriptors = unsafe { &mut *core::ptr::addr_of_mut!(FS.descriptors) };
    if !descriptors[parent].blocks.iter().any(|&b| b == 0) {
        return None;
    }

    let desc_idx = alloc_desc()?;
    let desc = &mut descriptors[desc_idx];
    *desc = Descriptor::empty();
    set_name(desc, base);
    sync_desc(desc_idx);
    link(parent, desc_idx).ok()?;
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

        // DMA into IO_BUF, since buf may be a non-identity-mapped user buffer.
        let first_sector = offset_in_block / 512;
        let end_sector = (offset_in_block + chunk_size + 511) / 512;
        let sector = unsafe { P2_OFFSET } + (block_id as u64) * 2048 + first_sector as u64;
        let start_in_io = offset_in_block - first_sector * 512;

        unsafe {
            virtio::read_sector(sector, core::ptr::addr_of_mut!(IO_BUF) as *mut u8, end_sector - first_sector);
            let io_buf = &*core::ptr::addr_of!(IO_BUF);
            buf[bytes_read..bytes_read + chunk_size]
                .copy_from_slice(&io_buf[start_in_io..start_in_io + chunk_size]);
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

        // Read-modify-write when the chunk doesn't fully cover its edge sectors.
        let first_sector = offset_in_block / 512;
        let end_sector = (offset_in_block + chunk_size + 511) / 512;
        let sector_count = end_sector - first_sector;
        let sector = unsafe { P2_OFFSET } + (block_id as u64) * 2048 + first_sector as u64;
        let start_in_io = offset_in_block - first_sector * 512;

        unsafe {
            if start_in_io != 0 || start_in_io + chunk_size != sector_count * 512 {
                virtio::read_sector(sector, core::ptr::addr_of_mut!(IO_BUF) as *mut u8, sector_count);
            }

            let io_buf = &mut *core::ptr::addr_of_mut!(IO_BUF);
            io_buf[start_in_io..start_in_io + chunk_size]
                .copy_from_slice(&buf[bytes_written..bytes_written + chunk_size]);

            virtio::write_sector(sector, core::ptr::addr_of!(IO_BUF) as *const u8, sector_count);
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

pub fn list_fs() -> alloc::string::String {
    use alloc::string::String;
    let mut out = String::new();
    
    unsafe {
        for i in 0..ramfs::MAX_RAM_FILES {
            if let Some(ref file) = ramfs::RAM_FS.files[i] {
                out.push_str(&alloc::format!("RAMFS: {} ({} bytes)\n", file.name, file.data.len()));
            }
        }
    }
    
    unsafe {
        let bitmap = &*core::ptr::addr_of!(FS.desc_bitmap);
        for i in 0..MAX_DESCRIPTORS {
            let word = i / 64;
            let bit = i % 64;
            if (bitmap[word] & (1 << bit)) != 0 {
                let desc = &FS.descriptors[i];
                let mut len = 0;
                while len < 64 && desc.filename[len] != 0 { len += 1; }
                if let Ok(name) = core::str::from_utf8(&desc.filename[..len]) {
                    out.push_str(&alloc::format!("DISK: {} ({} bytes) [Dir: {}]\n", name, desc.size, desc.is_dir));
                }
            }
        }
    }
    
    out
}

// Writes [next_cookie: u32][size: u32][desc: u32][is_dir: u8][name_len: u8][name...].
// Returns its length, 0 at the end, or None if not a directory or buf is too small.
pub fn read_dir_entry(dir_idx: usize, cookie: usize, buf: &mut [u8]) -> Option<usize> {
    if dir_idx >= MAX_DESCRIPTORS {
        return None;
    }
    let descriptors = unsafe { &*core::ptr::addr_of!(FS.descriptors) };
    let dir = &descriptors[dir_idx];
    if dir.is_dir == 0 {
        return None;
    }
    for slot in cookie..dir.blocks.len() {
        let child_idx = dir.blocks[slot] as usize;
        if child_idx == 0 || child_idx >= MAX_DESCRIPTORS {
            continue;
        }
        let child = &descriptors[child_idx];
        let name_len = child.filename.iter().position(|&b| b == 0).unwrap_or(64);
        if name_len == 0 {
            continue;
        }
        let total = 14 + name_len;
        if buf.len() < total {
            return None;
        }
        buf[0..4].copy_from_slice(&((slot + 1) as u32).to_le_bytes());
        buf[4..8].copy_from_slice(&child.size.to_le_bytes());
        buf[8..12].copy_from_slice(&(child_idx as u32).to_le_bytes());
        buf[12] = (child.is_dir != 0) as u8;
        buf[13] = name_len as u8;
        buf[14..total].copy_from_slice(&child.filename[..name_len]);
        return Some(total);
    }
    Some(0)
}
