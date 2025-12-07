
#[allow(unused_imports)]
use elfic::{Elf64, Elf64Phdr, ProgramType, ProgramFlags, Elf64Rela, Elf64Sym};
use crate::memory::{pmm, vmm, paging};
use crate::debugln;

unsafe fn virt_to_phys(pml4_phys: u64, virt: u64) -> Option<u64> {
    let pml4 = &*(pml4_phys as *const paging::PageTable);
    let p4_idx = (virt >> 39) & 0x1FF;
    let p3_entry = pml4.entries[p4_idx as usize];
    if p3_entry & paging::PAGE_PRESENT == 0 { return None; }
    
    let p3 = &*((p3_entry & 0x000FFFFFFFFFF000) as *const paging::PageTable);
    let p3_idx = (virt >> 30) & 0x1FF;
    let p2_entry = p3.entries[p3_idx as usize];
    if p2_entry & paging::PAGE_PRESENT == 0 { return None; }
    
    // Check for 1GB Huge Page (L3 Entry)
    if p2_entry & paging::PAGE_HUGE != 0 {
        let offset = virt & 0x3FFFFFFF;
        return Some((p2_entry & 0x000FFFFFC0000000) + offset);
    }
    
    let p2 = &*((p2_entry & 0x000FFFFFFFFFF000) as *const paging::PageTable);
    let p2_idx = (virt >> 21) & 0x1FF;
    let p1_entry = p2.entries[p2_idx as usize];
    if p1_entry & paging::PAGE_PRESENT == 0 { return None; }
    
    // Check for 2MB Huge Page (L2 Entry)
    if p1_entry & paging::PAGE_HUGE != 0 {
        let offset = virt & 0x1FFFFF;
        return Some((p1_entry & 0x000FFFFFFFE00000) + offset);
    }
    
    let p1 = &*((p1_entry & 0x000FFFFFFFFFF000) as *const paging::PageTable);
    let p1_idx = (virt >> 12) & 0x1FF;
    let page_entry = p1.entries[p1_idx as usize];
    if page_entry & paging::PAGE_PRESENT == 0 { return None; }
    
    Some((page_entry & 0x000FFFFFFFFFF000) + (virt & 0xFFF))
}

pub fn load_elf(data: &[u8], target_pml4_phys: u64) -> Result<u64, alloc::string::String> {
    debugln!("[ELF] Starting load_elf function.");
    let elf = Elf64::new(data).map_err(|e| alloc::format!("ELF Parse Error: {:?}", e))?;
    let header_e_type = elf.header.e_type;
    debugln!("[ELF] Elf64 parsed successfully. Header e_type: {:#x}", header_e_type);


    let load_base: u64 = if elf.header.e_type == 3 { 0x04000000 } else { 0 };
    debugln!("[ELF] Load Base set to: {:#x}", load_base);
    
    if load_base > 0 {
        debugln!("[ELF] Detected PIE Executable. Relocating to base {:#x}", load_base);
    } else {
        debugln!("[ELF] Not a PIE Executable.");
    }

    debugln!("[ELF] Starting segment loading phase. Total PHDRs: {}", elf.header.e_phnum + 0);
    // 1. Load Segments
    for (i, phdr) in elf.program_headers().into_iter().enumerate() {
        let phdr_p_type = phdr.p_type;
        let phdr_p_vaddr = phdr.p_vaddr;
        let phdr_p_memsz = phdr.p_memsz;
        let phdr_p_filesz = phdr.p_filesz;
        let phdr_p_flags = phdr.p_flags;
        let phdr_p_offset = phdr.p_offset;
        
        debugln!("[ELF] PHDR {}: type={:#x}, offset={:#x}, vaddr={:#x}, memsz={:#x}, filesz={:#x}, flags={:#x}", 
            i, phdr_p_type, phdr_p_offset, phdr_p_vaddr, phdr_p_memsz, phdr_p_filesz, phdr_p_flags);

        if ProgramType::from(phdr.p_type) == ProgramType::Load {
            debugln!("[ELF]   - Found LOAD segment.");
            if phdr.p_memsz == 0 { 
                debugln!("[ELF]   - Segment has zero memsz, skipping.");
                continue; 
            }

            let virt_start = phdr.p_vaddr + load_base;
            let virt_end = virt_start + phdr.p_memsz;
            
            // Check for Kernel Overlap
            if virt_end >= 0xFFFF_8000_0000_0000 {
                return Err(alloc::format!("ELF Segment overlaps with Kernel memory: {:#x}", virt_end));
            }

            debugln!("[ELF]   - Segment: virt_start={:#x}, virt_end={:#x}, filesz={:#x}", virt_start, virt_end, phdr_p_filesz);

            // Check if Entry Point is in this segment and verify file data
            let entry_point = elf.header.e_entry + load_base;
            if entry_point >= virt_start && entry_point < virt_end {
                let entry_offset_in_segment = entry_point - virt_start;
                if entry_offset_in_segment < phdr_p_filesz {
                    let file_offset = phdr_p_offset + entry_offset_in_segment;
                    unsafe {
                        let ptr = data.as_ptr().add(file_offset as usize);
                        debugln!("[ELF] Entry Point found in this segment. File Offset: {:#x}", file_offset);
                        debugln!("[ELF] DATA AT ENTRY POINT (FILE): {:02x} {:02x} {:02x} {:02x} ...", 
                            *ptr, *ptr.add(1), *ptr.add(2), *ptr.add(3));
                    }
                } else {
                    debugln!("[ELF] Entry Point in this segment but BEYOND file size (BSS/Zeroed)?");
                }
            }

            let page_start = virt_start & !(paging::PAGE_SIZE - 1);
            let page_end = (virt_end + paging::PAGE_SIZE - 1) & !(paging::PAGE_SIZE - 1);
            
            let pages = (page_end - page_start) / paging::PAGE_SIZE;
            debugln!("[ELF]   - Mapping {} pages from {:#x} to {:#x}", pages, page_start, page_end);

            let mut flags = paging::PAGE_PRESENT | paging::PAGE_USER;
            if (phdr.p_flags & ProgramFlags::WRITE) != 0 {
                flags |= paging::PAGE_WRITABLE;
            }
            debugln!("[ELF]   - Page flags: {:#b}", flags);

            for j in 0..pages {
                let virt = page_start + (j * paging::PAGE_SIZE);
                let frame: u64;

                if virt < 0x1_0000_0000 {
                    // Identity Mapped Region (0-4GiB)
                    // We don't allocate a new frame, we use the one corresponding to the virtual address.
                    // But we must mark it as used in PMM so we don't double-alloc later.
                    let phys = virt;
                    if pmm::reserve_frame(phys) {
                        // Success
                        frame = phys;
                    } else {
                        // Already used. 
                        frame = phys;
                    }
                    
                    // We rely on the global identity map (0-32GB) established in vmm::init
                    // which sets User+RW flags. No need to remap.

                } else {
                    // High Memory (> 4GB)
                    frame = pmm::allocate_frame().expect("OOM loading ELF");
                    
                    // Map to User Space
                    vmm::map_page(virt, frame, flags, Some(target_pml4_phys));
                    
                    // Map to Kernel Space (Identity) for access
                    vmm::map_page(frame, frame, paging::PAGE_PRESENT | paging::PAGE_WRITABLE, None);
                }
                
                unsafe {
                    core::ptr::write_bytes(frame as *mut u8, 0, paging::PAGE_SIZE as usize);
                }
            }

            let file_size = phdr.p_filesz as usize;
            if file_size > 0 {
                debugln!("[ELF]   - Copying {} bytes of segment data.", file_size);
                let segment_data = &data[phdr.p_offset as usize..(phdr.p_offset as usize + file_size)];
                
                let mut remaining = file_size;
                let mut src_offset = 0;
                let mut current_virt = virt_start;
                
                while remaining > 0 {
                    // Calculate physical address
                    let phys_addr = if current_virt < 0x1_0000_0000 {
                        // Identity Mapped Region (0-4GiB)
                        // We know phys == virt because we set it up that way in new_user_pml4
                        current_virt
                    } else {
                        // High Memory
                        unsafe { virt_to_phys(target_pml4_phys, current_virt)
                            .expect("Translation failed during load") }
                    };

                    let page_offset = (current_virt % paging::PAGE_SIZE) as usize;
                    let to_copy = core::cmp::min(remaining, (paging::PAGE_SIZE - page_offset as u64) as usize);
                    
                    // DEBUG: Print first few bytes of source data
                    if src_offset == 0 {
                         let ptr = segment_data.as_ptr();
                         unsafe {
                             debugln!("[ELF] Segment Data Start: {:02x} {:02x} {:02x} {:02x} ...", 
                                 *ptr, *ptr.add(1), *ptr.add(2), *ptr.add(3));
                         }
                    }

                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            segment_data.as_ptr().add(src_offset),
                            phys_addr as *mut u8,
                            to_copy
                        );
                    }
                    
                    remaining -= to_copy;
                    src_offset += to_copy;
                    current_virt += to_copy as u64;
                }
            }
            debugln!("[ELF]   - Finished mapping segment.");
        }
    }

    debugln!("[ELF] Segment loading phase completed.");

    debugln!("[ELF] Starting relocation phase.");
    // 2. Find Symbol Table (DYNSYM) for relocations
    let mut dynsym_shdr: Option<&elfic::Elf64Shdr> = None;
    for shdr in elf.section_headers() {
        if shdr.sh_type == 11 { // SHT_DYNSYM
            let shdr_sh_offset = shdr.sh_offset;
            debugln!("[ELF]   - Found DYNSYM section at offset {:#x}", shdr_sh_offset);
            dynsym_shdr = Some(shdr);
            break;
        }
    }

    // 3. Perform Relocations
    if load_base > 0 {
        for shdr in elf.section_headers() {
            if shdr.sh_type == 4 { // SHT_RELA
                 let shdr_sh_offset_rela = shdr.sh_offset;
                 debugln!("[ELF]   - Found RELA section at offset {:#x}", shdr_sh_offset_rela);
                 let num_entries = shdr.sh_size / shdr.sh_entsize;
                 let offset = shdr.sh_offset as usize;
                 
                 debugln!("[ELF]   - Processing Relocation Section: entries={}", num_entries);

                 let relas = unsafe {
                     core::slice::from_raw_parts(
                         data.as_ptr().add(offset) as *const Elf64Rela,
                         num_entries as usize
                     )
                 };

                 for (k, rela) in relas.iter().enumerate() {
                     let r_type = rela.get_type();
                     let r_sym = rela.get_symbol();
                     let target_virt = rela.r_offset + load_base;
                     
                     let rela_r_offset = rela.r_offset;
                     //debugln!("[ELF]     - Rela {}: type={}, sym={}, offset={:#x}", k, r_type, r_sym, rela_r_offset);

                     let mut val: u64 = 0;
                     let mut found_val = false;

                     match r_type {
                         8 => { // R_X86_64_RELATIVE
                             // B + A
                             val = load_base.wrapping_add(rela.r_addend as u64);
                             found_val = true;
                             //debugln!("[ELF]       - RELATIVE: val={:#x}", val);
                         }
                         1 | 6 | 7 => { // R_X86_64_64 (1), GLOB_DAT (6), JUMP_SLOT (7)
                             // S + A (64) or S (GLOB_DAT/JUMP_SLOT)
                             // We need to lookup the symbol value
                             if let Some(sym_tab) = dynsym_shdr {
                                 let sym_offset = sym_tab.sh_offset as usize + (r_sym as usize * core::mem::size_of::<Elf64Sym>());
                                 if sym_offset < data.len() {
                                     let sym = unsafe { &*(data.as_ptr().add(sym_offset) as *const Elf64Sym) };
                                     
                                     if sym.st_shndx != 0 { // If defined in ELF
                                         val = sym.st_value + load_base;
                                         let sym_st_value = sym.st_value;
                                         //debugln!("[ELF]       - SYMBOL (defined): sym_val={:#x}, final_val={:#x}", sym_st_value, val);
                                     } else {
                                         // Undefined symbol (Imported).
                                         // In a real OS, we would look this up in Kernel exports or Shared Libs.
                                         // For now, we leave it as 0 (weak) or fail.
                                         //debugln!("[ELF]       - Warning: Undefined symbol index {} (leaving as 0).", r_sym);
                                     }
                                     
                                     if r_type == 1 { // R_X86_64_64 adds addend
                                         val = val.wrapping_add(rela.r_addend as u64);
                                         //debugln!("[ELF]       - R_X86_64_64: val after addend={:#x}", val);
                                     }
                                     found_val = true;
                                 }
                             } else {
                                 debugln!("[ELF]       - Warning: No DYNSYM found for symbol lookup.");
                             }
                         }
                         _ => {
                             debugln!("[ELF]       - Unhandled relocation type: {}", r_type);
                             // Ignore others for now (NONE, COPY, etc)
                         }
                     }

                     if found_val {
                         //debugln!("[ELF]     - Applying relocation to virt {:#x} with value {:#x}", target_virt, val);
                         if let Some(phys) = unsafe { virt_to_phys(target_pml4_phys, target_virt) } {
                             unsafe {
                                 //debugln!("[ELF]       - Translated target_virt {:#x} to phys {:#x}", target_virt, phys);
                                 *(phys as *mut u64) = val;
                                 //debugln!("[ELF]       - Relocation applied.");
                             }
                         } else {
                             debugln!("[ELF]       - ERROR: Failed to translate target_virt {:#x} for relocation!", target_virt);
                         }
                     }
                 }
            }
        }
    }
    debugln!("[ELF] Relocation phase completed.");


    let entry_point = elf.header.e_entry + load_base;
    debugln!("[ELF] Load_elf completed. Entry point: {:#x}", entry_point);
    
    // DEBUG: Inspect code at entry point
    unsafe {
        if let Some(phys) = virt_to_phys(target_pml4_phys, entry_point) {
             let code_ptr = phys as *const u8;
             debugln!("[ELF] Code at entry point: {:02x} {:02x} {:02x} {:02x} ...",
                 *code_ptr, *code_ptr.add(1), *code_ptr.add(2), *code_ptr.add(3));
        } else {
             debugln!("[ELF] WARNING: Entry point not mapped?");
        }
    }

    Ok(entry_point)
}
