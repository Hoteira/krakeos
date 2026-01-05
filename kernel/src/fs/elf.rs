#[allow(unused_imports)]
use elfic::{Elf64, Elf64Phdr, ProgramType, ProgramFlags, Elf64Rela, Elf64Sym};
use crate::memory::{pmm, paging};

unsafe fn virt_to_phys(_pml4_phys: u64, virt: u64) -> Option<u64> {
    Some(virt)
}

pub fn load_elf(data: &[u8], target_pml4_phys: u64, pid: u64) -> Result<u64, alloc::string::String> {

    let elf = Elf64::new(data).map_err(|e| alloc::format!("ELF Parse Error: {:?}", e))?;
    let header_e_type = elf.header.e_type;

    if header_e_type != 3 {
        panic!("Security Violation: Attempted to load non-PIE executable (Type {})! All user programs must be Position Independent.", header_e_type);
    }

    let mut max_end: u64 = 0;
    let load_base = {
        for phdr in elf.program_headers() {
            if ProgramType::from(phdr.p_type) == ProgramType::Load {
                let end = phdr.p_vaddr + phdr.p_memsz;
                if end > max_end { max_end = end; }
            }
        }

        let pages = (max_end + 4095) / 4096;
        if pages == 0 {
            panic!("ELF LOAD ERROR: ELF segment exceeds 4GB!");
        } else {
             match pmm::allocate_frames(pages as usize, pid) {
                 Some(addr) => addr,
                 None => return Err(alloc::format!("OOM: Failed to allocate {} pages for ELF", pages)),
             }
        }
    };

    for (_i, phdr) in elf.program_headers().into_iter().enumerate() {
        let _phdr_p_type = phdr.p_type;
        let _phdr_p_vaddr = phdr.p_vaddr;
        let _phdr_p_memsz = phdr.p_memsz;
        let phdr_p_filesz = phdr.p_filesz;
        let _phdr_p_flags = phdr.p_flags;
        let phdr_p_offset = phdr.p_offset;


        if ProgramType::from(phdr.p_type) == ProgramType::Load {

            if phdr.p_memsz == 0 {

                continue;
            }

            let virt_start = phdr.p_vaddr + load_base;
            let virt_end = virt_start + phdr.p_memsz;

            if virt_end >= 0xFFFF_8000_0000_0000 {
                return Err(alloc::format!("ELF Segment overlaps with Kernel memory: {:#x}", virt_end));
            }



            let entry_point = elf.header.e_entry + load_base;

            if entry_point >= virt_start && entry_point < virt_end {
                let entry_offset_in_segment = entry_point - virt_start;
                if entry_offset_in_segment < phdr_p_filesz {
                    let file_offset = phdr_p_offset + entry_offset_in_segment;
                    unsafe {
                        let _ptr = data.as_ptr().add(file_offset as usize);

                    }
                } else {

                }
            }

            let page_start = virt_start & !(paging::PAGE_SIZE - 1);
            let page_end = (virt_end + paging::PAGE_SIZE - 1) & !(paging::PAGE_SIZE - 1);

            let pages = (page_end - page_start) / paging::PAGE_SIZE;


            let mut flags = paging::PAGE_PRESENT | paging::PAGE_USER;
            if (phdr.p_flags & ProgramFlags::WRITE) != 0 {
                flags |= paging::PAGE_WRITABLE;
            }

            let file_size = phdr.p_filesz as usize;
            if file_size > 0 {

                let segment_data = &data[phdr.p_offset as usize..(phdr.p_offset as usize + file_size)];

                let mut remaining = file_size;
                let mut src_offset = 0;
                let mut current_virt = virt_start;

                while remaining > 0 {
                    let phys_addr = current_virt;

                    let page_offset = (current_virt % paging::PAGE_SIZE) as usize;
                    let to_copy = core::cmp::min(remaining, (paging::PAGE_SIZE - page_offset as u64) as usize);

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

        }
    }




    let mut dynsym_shdr: Option<&elfic::Elf64Shdr> = None;
    for shdr in elf.section_headers() {
        if shdr.sh_type == 11 { 
            let _shdr_sh_offset = shdr.sh_offset;

            dynsym_shdr = Some(shdr);
            break;
        }
    }

    if load_base > 0 {
        for shdr in elf.section_headers() {
            if shdr.sh_type == 4 { 
                let _shdr_sh_offset_rela = shdr.sh_offset;

                let num_entries = shdr.sh_size / shdr.sh_entsize;
                let offset = shdr.sh_offset as usize;



                let relas = unsafe {
                    core::slice::from_raw_parts(
                        data.as_ptr().add(offset) as *const Elf64Rela,
                        num_entries as usize
                    )
                };

                for (_k, rela) in relas.iter().enumerate() {
                    let r_type = rela.get_type();
                    let r_sym = rela.get_symbol();
                    let target_virt = rela.r_offset + load_base;

                    if rela.r_offset >= max_end {
                        // crate::debugln!("Relocation out of bounds: {:#x} >= {:#x}", rela.r_offset, max_end);
                        continue;
                    }

                    let _rela_r_offset = rela.r_offset;

                    let mut val: u64 = 0;
                    let mut found_val = false;

                    match r_type {
                        8 => { 
                            val = load_base.wrapping_add(rela.r_addend as u64);
                            found_val = true;
                        }
                        1 | 6 | 7 => { 
                            if let Some(sym_tab) = dynsym_shdr {
                                let sym_offset = sym_tab.sh_offset as usize + (r_sym as usize * core::mem::size_of::<Elf64Sym>());
                                if sym_offset < data.len() {
                                    let sym = unsafe { &*(data.as_ptr().add(sym_offset) as *const Elf64Sym) };

                                    if sym.st_shndx != 0 {
                                        val = sym.st_value + load_base;
                                        let _sym_st_value = sym.st_value;
                                    } else {
                                    }

                                    if r_type == 1 {
                                        val = val.wrapping_add(rela.r_addend as u64);
                                    }
                                    found_val = true;
                                }
                            } else {

                            }
                        }
                        _ => {

                        }
                    }

                    if found_val {
                        if let Some(phys) = unsafe { virt_to_phys(target_pml4_phys, target_virt) } {
                            unsafe {
                                *(phys as *mut u64) = val;
                            }
                        } else {

                        }
                    }
                }
            }
        }
    }



    let entry_point = elf.header.e_entry + load_base;


    unsafe {
        if let Some(phys) = virt_to_phys(target_pml4_phys, entry_point) {
            let _code_ptr = phys as *const u8;
        } else {

        }
    }

    Ok(entry_point)
}