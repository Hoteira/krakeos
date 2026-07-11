use std::fs::{OpenOptions, self};
use std::io::{Write, Seek, SeekFrom};
use std::path::Path;

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

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        println!("Usage: fatsquid_fmt <image> <format|add|pack> [args...]");
        return;
    }

    let image_path = &args[1];
    let cmd = &args[2];

    let lba_p1: u32 = 2048;
    let sec_p1: u32 = 131072; // 64MB
    let lba_p2: u32 = lba_p1 + sec_p1;
    let size_1gb: u64 = 1024 * 1024 * 1024;
    let sec_p2: u32 = (size_1gb / 512) as u32 - lba_p2;
    let p2_offset = (lba_p2 as u64) * 512;

    if cmd == "format" {
        println!("Formatting FatSquid Image (MBR Hybrid)...");
        let mut file = OpenOptions::new().read(true).write(true).create(true).truncate(true).open(image_path).expect("Failed to create fs.img");
        file.set_len(size_1gb).expect("Failed to set file length");

        let mut mbr = [0u8; 512];
        mbr[446 + 4] = 0x0C;
        mbr[446 + 8..446 + 12].copy_from_slice(&lba_p1.to_le_bytes());
        mbr[446 + 12..446 + 16].copy_from_slice(&sec_p1.to_le_bytes());

        mbr[462 + 4] = 0x83;
        mbr[462 + 8..462 + 12].copy_from_slice(&lba_p2.to_le_bytes());
        mbr[462 + 12..462 + 16].copy_from_slice(&sec_p2.to_le_bytes());

        mbr[510] = 0x55;
        mbr[511] = 0xAA;

        file.seek(SeekFrom::Start(0)).unwrap();
        file.write_all(&mbr).unwrap();

        println!("Formatting Partition 1 as FAT32...");
        {
            let mut part1 = fscommon::StreamSlice::new(file.try_clone().unwrap(), (lba_p1 as u64) * 512, (sec_p1 as u64) * 512).unwrap();
            fatfs::format_volume(&mut part1, fatfs::FormatVolumeOptions::new()).expect("Failed to format FAT32");
        }

        println!("Formatting Partition 2 as FatSquid...");
        file.seek(SeekFrom::Start(p2_offset)).unwrap();
        let jump_inst: u32 = 0x0000006f; 
        let magic: &[u8; 8] = b"FTSQUID1";
        file.write_all(&jump_inst.to_le_bytes()).unwrap();
        file.write_all(magic).unwrap();

        let mut descriptors = vec![Descriptor::empty(); 1024];
        descriptors[0].filename[0] = b'/';
        descriptors[0].is_dir = 1;

        file.seek(SeekFrom::Start(p2_offset + 1024 * 1024)).unwrap();
        let descriptors_bytes = unsafe {
            std::slice::from_raw_parts(
                descriptors.as_ptr() as *const u8,
                descriptors.len() * std::mem::size_of::<Descriptor>(),
            )
        };
        file.write_all(descriptors_bytes).unwrap();
        file.sync_all().unwrap();
        println!("FatSquid fs.img formatted successfully with MBR!");
    } else if cmd == "add" {
        if args.len() < 5 {
            println!("Usage: fatsquid_fmt <image> add <host_path> <squid_name>");
            return;
        }
        let host_path = &args[3];
        let squid_name = &args[4];
        
        let mut file = OpenOptions::new().read(true).write(true).open(image_path).unwrap();
        
        // Read descriptors
        let mut descriptors = vec![Descriptor::empty(); 1024];
        file.seek(SeekFrom::Start(p2_offset + 1024 * 1024)).unwrap();
        let mut desc_bytes = vec![0u8; 1024 * std::mem::size_of::<Descriptor>()];
        std::io::Read::read_exact(&mut file, &mut desc_bytes).unwrap();
        
        unsafe {
            std::ptr::copy_nonoverlapping(
                desc_bytes.as_ptr(),
                descriptors.as_mut_ptr() as *mut u8,
                desc_bytes.len()
            );
        }

        // Find empty descriptor and next free block
        let mut empty_idx = 0;
        let mut highest_block = 4; // Blocks 0-4 reserved
        for (i, desc) in descriptors.iter().enumerate() {
            if i > 0 && desc.size == 0 && desc.is_dir == 0 && desc.filename[0] == 0 {
                if empty_idx == 0 {
                    empty_idx = i;
                }
            } else {
                for &b in desc.blocks.iter() {
                    if b > highest_block {
                        highest_block = b;
                    }
                }
            }
        }
        
        if empty_idx == 0 {
            println!("No empty descriptors found!");
            return;
        }

        let wasm_bytes = std::fs::read(host_path).unwrap();
        let name_bytes = squid_name.as_bytes();
        descriptors[empty_idx].filename[..name_bytes.len()].copy_from_slice(name_bytes);
        descriptors[empty_idx].size = wasm_bytes.len() as u32;
        descriptors[empty_idx].is_dir = 0;
        
        let mut block_idx = 0;
        let mut bytes_written = 0;
        let block_size = 1024 * 1024;
        let start_block = highest_block + 1;
        
        while bytes_written < wasm_bytes.len() {
            let chunk_size = std::cmp::min(block_size, wasm_bytes.len() - bytes_written);
            let block_id = start_block + block_idx as u16;
            descriptors[empty_idx].blocks[block_idx] = block_id;
            
            file.seek(SeekFrom::Start(p2_offset + (block_id as u64) * (block_size as u64))).unwrap();
            file.write_all(&wasm_bytes[bytes_written..bytes_written + chunk_size]).unwrap();
            
            bytes_written += chunk_size;
            block_idx += 1;
        }
        
        // Write descriptors back
        file.seek(SeekFrom::Start(p2_offset + 1024 * 1024)).unwrap();
        let descriptors_bytes = unsafe {
            std::slice::from_raw_parts(
                descriptors.as_ptr() as *const u8,
                descriptors.len() * std::mem::size_of::<Descriptor>(),
            )
        };
        file.write_all(descriptors_bytes).unwrap();
        file.sync_all().unwrap();
        println!("Embedded {} ({} bytes in {} blocks) at index {}", squid_name, wasm_bytes.len(), block_idx, empty_idx);
    } else if cmd == "pack" {
        if args.len() < 4 {
            println!("Usage: fatsquid_fmt <image> pack <host_dir>");
            return;
        }
        let host_dir = &args[3];
        
        let mut file = OpenOptions::new().read(true).write(true).open(image_path).unwrap();
        
        // Wipe existing descriptors (re-init)
        let mut descriptors = vec![Descriptor::empty(); 1024];
        descriptors[0].filename[0] = b'/';
        descriptors[0].is_dir = 1;
        
        let mut next_desc_idx = 1;
        let mut highest_block = 4; // Blocks 0-4 reserved
        let block_size = 1024 * 1024;
        
        fn pack_dir(
            host_path: &Path, 
            parent_desc_idx: usize, 
            descriptors: &mut Vec<Descriptor>, 
            next_desc_idx: &mut usize,
            highest_block: &mut u16,
            file: &mut std::fs::File,
            p2_offset: u64,
            block_size: u64
        ) {
            let entries = match fs::read_dir(host_path) {
                Ok(e) => e,
                Err(_) => return,
            };
            
            for entry in entries {
                if let Ok(entry) = entry {
                    let path = entry.path();
                    let name = entry.file_name().into_string().unwrap();
                    let is_dir = path.is_dir();
                    
                    let desc_idx = *next_desc_idx;
                    if desc_idx >= 1024 {
                        println!("ERROR: Max descriptors reached!");
                        return;
                    }
                    *next_desc_idx += 1;
                    
                    // Add this child to parent's block list
                    let mut added = false;
                    for b in descriptors[parent_desc_idx].blocks.iter_mut() {
                        if *b == 0 {
                            *b = desc_idx as u16;
                            descriptors[parent_desc_idx].size += 1; // Size of a directory is the number of children
                            added = true;
                            break;
                        }
                    }
                    if !added {
                        println!("ERROR: Parent directory is full!");
                        return;
                    }
                    
                    let name_bytes = name.as_bytes();
                    descriptors[desc_idx].filename[..name_bytes.len()].copy_from_slice(name_bytes);
                    
                    if is_dir {
                        descriptors[desc_idx].is_dir = 1;
                        pack_dir(&path, desc_idx, descriptors, next_desc_idx, highest_block, file, p2_offset, block_size);
                    } else {
                        descriptors[desc_idx].is_dir = 0;
                        let wasm_bytes = std::fs::read(&path).unwrap();
                        descriptors[desc_idx].size = wasm_bytes.len() as u32;
                        
                        let mut block_idx = 0;
                        let mut bytes_written = 0;
                        let start_block = *highest_block + 1;
                        
                        while bytes_written < wasm_bytes.len() {
                            let chunk_size = std::cmp::min(block_size as usize, wasm_bytes.len() - bytes_written);
                            let block_id = start_block + block_idx as u16;
                            descriptors[desc_idx].blocks[block_idx] = block_id;
                            *highest_block = std::cmp::max(*highest_block, block_id);
                            
                            file.seek(SeekFrom::Start(p2_offset + (block_id as u64) * block_size)).unwrap();
                            file.write_all(&wasm_bytes[bytes_written..bytes_written + chunk_size]).unwrap();
                            
                            bytes_written += chunk_size;
                            block_idx += 1;
                        }
                        println!("Packed file {} at index {} ({} blocks)", name, desc_idx, block_idx);
                    }
                }
            }
        }
        
        let root_path = Path::new(host_dir);
        pack_dir(&root_path, 0, &mut descriptors, &mut next_desc_idx, &mut highest_block, &mut file, p2_offset, block_size as u64);
        
        // Write descriptors back
        file.seek(SeekFrom::Start(p2_offset + 1024 * 1024)).unwrap();
        let descriptors_bytes = unsafe {
            std::slice::from_raw_parts(
                descriptors.as_ptr() as *const u8,
                descriptors.len() * std::mem::size_of::<Descriptor>(),
            )
        };
        file.write_all(descriptors_bytes).unwrap();
        file.sync_all().unwrap();
        println!("Successfully packed {} into FatSquid image!", host_dir);
    }
}
