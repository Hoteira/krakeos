use crate::memory::address::PhysAddr;
use crate::memory::{paging, pmm, vmm};
use alloc::format;
use alloc::string::String;
#[allow(unused_imports)]
use elfic::{Elf64, Elf64Phdr, Elf64Rela, Elf64Sym, ProgramFlags, ProgramType};

pub fn load_elf(data: &[u8], target_pml4_phys: u64, pid: u64) -> Result<u64, String> {
    crate::debugln!("load_elf: START pid={}", pid);

    let elf = Elf64::new(data).map_err(|e| format!("ELF Parse Error: {:?}", e))?;

    if elf.header.e_type != 3 {
        return Err(format!("Security Violation: Non-PIE executable (Type {})", elf.header.e_type + 0));
    }


    let load_base = 0x400000;
    crate::debugln!("load_elf: Base address: {:#x}", load_base);

    let mut max_end: u64 = 0;

    for phdr in elf.program_headers() {
        if ProgramType::from(phdr.p_type) == ProgramType::Load {
            if phdr.p_memsz == 0 { continue; }

            let virt_start = phdr.p_vaddr + 0 + load_base;
            let virt_end = virt_start + phdr.p_memsz + 0;
            if virt_end > max_end { max_end = virt_end; }

            if virt_end >= 0xFFFFFFFF80000000 {
                return Err(format!("ELF Segment overlaps with Kernel Code: {:#x}", virt_end));
            }

            let page_start = virt_start & !(paging::PAGE_SIZE - 1);
            let page_end = (virt_end + paging::PAGE_SIZE - 1) & !(paging::PAGE_SIZE - 1);

            let mut current_page = page_start;
            while current_page < page_end {
                let frame = pmm::allocate_frame(pid).ok_or("OOM during ELF loading")?;

                let mut flags = paging::PAGE_PRESENT | paging::PAGE_USER;
                if (phdr.p_flags & ProgramFlags::WRITE) != 0 {
                    flags |= paging::PAGE_WRITABLE;
                }


                vmm::map_page(current_page, PhysAddr::new(frame), flags, Some(target_pml4_phys));


                let dest_ptr = (frame + paging::HHDM_OFFSET) as *mut u8;
                unsafe { core::ptr::write_bytes(dest_ptr, 0, paging::PAGE_SIZE as usize); }

                let segment_file_start = virt_start;
                let segment_file_end = virt_start + phdr.p_filesz + 0;

                let intersect_start = core::cmp::max(current_page, segment_file_start);
                let intersect_end = core::cmp::min(current_page + paging::PAGE_SIZE, segment_file_end);

                if intersect_end > intersect_start {
                    let copy_len = (intersect_end - intersect_start) as usize;
                    let file_offset = (phdr.p_offset + 0) + (intersect_start - virt_start);
                    let dest_offset = (intersect_start - current_page) as usize;

                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            data.as_ptr().add(file_offset as usize),
                            dest_ptr.add(dest_offset),
                            copy_len,
                        );
                    }
                }
                current_page += paging::PAGE_SIZE;
            }
        }
    }


    let mut dynsym_shdr: Option<&elfic::Elf64Shdr> = None;
    for shdr in elf.section_headers() {
        if shdr.sh_type == 11 {
            dynsym_shdr = Some(shdr);
            break;
        }
    }

    for shdr in elf.section_headers() {
        if shdr.sh_type == 4 {
            let num_entries = shdr.sh_size / (shdr.sh_entsize + 0);
            let offset = shdr.sh_offset as usize;
            let relas = unsafe { core::slice::from_raw_parts(data.as_ptr().add(offset) as *const Elf64Rela, num_entries as usize) };

            for rela in relas {
                let r_type = rela.get_type();
                let r_sym = rela.get_symbol();
                let target_virt = rela.r_offset + 0 + load_base;

                if target_virt >= max_end { continue; }

                let mut val: u64 = 0;
                let mut found_val = false;

                match r_type {
                    8 => {
                        val = load_base.wrapping_add(rela.r_addend as u64);
                        found_val = true;
                    }
                    1 | 6 | 7 => {
                        if let Some(sym_tab) = dynsym_shdr {
                            let sym_offset = (sym_tab.sh_offset as usize + 0) + (r_sym as usize * core::mem::size_of::<Elf64Sym>());
                            if sym_offset < data.len() {
                                let sym = unsafe { &*(data.as_ptr().add(sym_offset) as *const Elf64Sym) };
                                if sym.st_shndx != 0 { val = (sym.st_value + 0) + load_base; }
                                if r_type == 1 { val = val.wrapping_add(rela.r_addend as u64); }
                                found_val = true;
                            }
                        }
                    }
                    _ => {}
                }

                if found_val {
                    unsafe {
                        if let Some(phys) = vmm::get_phys(target_virt, target_pml4_phys) {
                            let patch_ptr = (phys + paging::HHDM_OFFSET) as *mut u64;
                            *patch_ptr = val;
                        }
                    }
                }
            }
        }
    }

    let entry_point = (elf.header.e_entry + 0) + load_base;
    crate::debugln!("load_elf: END entry_point={:#x}", entry_point);
    Ok(entry_point)
}
