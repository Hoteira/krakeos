use crate::memory::address::PhysAddr;
use crate::memory::paging::PageTableFlags;
use crate::memory::{paging, pmm};
use core::arch::asm;

pub fn init() {
    unsafe {
        let new_pml4_phys = pmm::allocate_frame(0).expect("VMM: Failed to allocate initial kernel PML4");
        let new_pml4_addr = PhysAddr::new(new_pml4_phys);


        let old_pml4_phys = (*(&raw const crate::boot::BOOT_INFO)).pml4;


        core::ptr::copy_nonoverlapping(
            old_pml4_phys as *const u8,
            new_pml4_addr.as_u64() as *mut u8,
            4096,
        );


        let pml4_virt = paging::phys_to_virt(new_pml4_addr);
        let pml4 = unsafe { &mut *(pml4_virt.as_mut_ptr() as *mut paging::PageTable) };
        let p4_idx = (KERNEL_MAPPING_HEAD >> 39) & 0x1FF;

        if pml4[p4_idx as usize].is_unused() {
            let pdpt_frame = pmm::allocate_frame(0).expect("VMM: OOM for Shared Kernel PDPT");
            let mut entry = paging::PageTableEntry::new();
            entry.set_addr(PhysAddr::new(pdpt_frame),
                           paging::PageTableFlags::PRESENT | paging::PageTableFlags::WRITABLE);
            pml4[p4_idx as usize] = entry;

            let pdpt_virt = paging::phys_to_virt(PhysAddr::new(pdpt_frame));
            core::ptr::write_bytes(pdpt_virt.as_mut_ptr::<u8>(), 0, 4096);
        }


        asm!("mov cr3, {}", in(reg) new_pml4_phys);


        (*(&raw mut crate::boot::BOOT_INFO)).pml4 = new_pml4_phys;

        crate::debugln!("VMM: Relocated PML4 from {:#x} to {:#x}", old_pml4_phys, new_pml4_phys);
    }
}

pub fn map_page(virt: u64, phys: PhysAddr, flags: u64, target_pml4_phys: Option<u64>) {
    unsafe {
        let pml4_table = if let Some(pml4_addr) = target_pml4_phys {
            paging::get_table_from_phys(pml4_addr).expect("VMM: Invalid target PML4")
        } else {
            paging::active_level_4_table()
        };

        let p4_idx = (virt >> 39) & 0x1FF;
        let p3_idx = (virt >> 30) & 0x1FF;
        let p2_idx = (virt >> 21) & 0x1FF;
        let p1_idx = (virt >> 12) & 0x1FF;

        let is_user = (flags & paging::PAGE_USER) != 0;


        let mut p3_entry = pml4_table[p4_idx as usize];
        if p3_entry.is_unused() {
            let frame = pmm::allocate_frame(0).expect("VMM: OOM for PDPT");
            let mut new_entry = paging::PageTableEntry::new();
            let table_flags = paging::PageTableFlags::from_bits_truncate(paging::PAGE_PRESENT | paging::PAGE_WRITABLE | paging::PAGE_USER);
            new_entry.set_addr(PhysAddr::new(frame), table_flags);
            pml4_table[p4_idx as usize] = new_entry;
            paging::get_table_from_phys(frame).unwrap().zero();
            p3_entry = new_entry;
        } else if is_user && !p3_entry.flags().contains(PageTableFlags::USER_ACCESSIBLE) {
            p3_entry.set_flags(p3_entry.flags() | PageTableFlags::USER_ACCESSIBLE);
            pml4_table[p4_idx as usize] = p3_entry;
        }

        let p3 = paging::get_table_from_phys(p3_entry.addr().as_u64()).expect("VMM: Failed to get L3 table");


        let mut p2_entry = p3[p3_idx as usize];
        if p2_entry.is_unused() {
            let frame = pmm::allocate_frame(0).expect("VMM: OOM for PD");
            let mut new_entry = paging::PageTableEntry::new();
            let table_flags = paging::PageTableFlags::from_bits_truncate(paging::PAGE_PRESENT | paging::PAGE_WRITABLE | paging::PAGE_USER);
            new_entry.set_addr(PhysAddr::new(frame), table_flags);
            p3[p3_idx as usize] = new_entry;
            paging::get_table_from_phys(frame).unwrap().zero();
            p2_entry = new_entry;
        } else if (p2_entry.as_u64() & paging::PAGE_HUGE) != 0 {
            let frame = pmm::allocate_frame(0).expect("VMM: OOM for L2 shattering");
            let new_table = paging::get_table_from_phys(frame).unwrap();
            let base_phys = p2_entry.addr().as_u64();
            let huge_flags = paging::PageTableFlags::from_bits_truncate(p2_entry.as_u64() & 0xFFF);
            for i in 0..512 {
                let mut e = paging::PageTableEntry::new();
                e.set_addr(PhysAddr::new(base_phys + (i as u64 * 0x40000000 / 512)), huge_flags);
                new_table[i] = e;
            }
            p2_entry.set_addr(PhysAddr::new(frame), huge_flags & !PageTableFlags::HUGE_PAGE);
            p3[p3_idx as usize] = p2_entry;
        } else if is_user && !p2_entry.flags().contains(PageTableFlags::USER_ACCESSIBLE) {
            p2_entry.set_flags(p2_entry.flags() | PageTableFlags::USER_ACCESSIBLE);
            p3[p3_idx as usize] = p2_entry;
        }

        let p2 = paging::get_table_from_phys(p2_entry.addr().as_u64()).expect("VMM: Failed to get L2 table");


        let mut p1_entry = p2[p2_idx as usize];
        if p1_entry.is_unused() {
            let frame = pmm::allocate_frame(0).expect("VMM: OOM for PT");
            let mut new_entry = paging::PageTableEntry::new();
            let table_flags = paging::PageTableFlags::from_bits_truncate(paging::PAGE_PRESENT | paging::PAGE_WRITABLE | paging::PAGE_USER);
            new_entry.set_addr(PhysAddr::new(frame), table_flags);
            p2[p2_idx as usize] = new_entry;
            paging::get_table_from_phys(frame).unwrap().zero();
            p1_entry = new_entry;
        } else if (p1_entry.as_u64() & paging::PAGE_HUGE) != 0 {
            let frame = pmm::allocate_frame(0).expect("VMM: OOM for L1 shattering");
            let new_table = paging::get_table_from_phys(frame).unwrap();
            let base_phys = p1_entry.addr().as_u64();
            let huge_flags = paging::PageTableFlags::from_bits_truncate(p1_entry.as_u64() & 0xFFF);
            for i in 0..512 {
                let mut e = paging::PageTableEntry::new();
                e.set_addr(PhysAddr::new(base_phys + (i as u64 * 4096)), huge_flags & !PageTableFlags::HUGE_PAGE);
                new_table[i] = e;
            }
            p1_entry.set_addr(PhysAddr::new(frame), (huge_flags & !PageTableFlags::HUGE_PAGE) | PageTableFlags::PRESENT);
            p2[p2_idx as usize] = p1_entry;
        } else if is_user && !p1_entry.flags().contains(PageTableFlags::USER_ACCESSIBLE) {
            p1_entry.set_flags(p1_entry.flags() | PageTableFlags::USER_ACCESSIBLE);
            p2[p2_idx as usize] = p1_entry;
        }

        let p1 = paging::get_table_from_phys(p1_entry.addr().as_u64()).expect("VMM: Failed to get L1 table");


        let mut final_entry = paging::PageTableEntry::new();
        *(&mut final_entry as *mut _ as *mut u64) = phys.as_u64() | flags;
        p1[p1_idx as usize] = final_entry;

        let current_cr3: u64;
        asm!("mov {}, cr3", out(reg) current_cr3);
        let current_pml4 = current_cr3 & 0x000F_FFFF_FFFF_F000;

        if target_pml4_phys.is_none() || target_pml4_phys == Some(current_pml4) {
            asm!("invlpg [{}]", in(reg) virt);
        }
    }
}

pub unsafe fn create_user_pml4() -> Option<u64> {
    let pml4_phys = pmm::allocate_frame(0)?;
    let pml4_virt_addr = paging::phys_to_virt(PhysAddr::new(pml4_phys));
    let pml4 = &mut *(pml4_virt_addr.as_mut_ptr() as *mut paging::PageTable);

    pml4.zero();


    let kernel_pml4 = paging::active_level_4_table();
    for i in 256..512 {
        pml4[i] = kernel_pml4[i];
    }

    Some(pml4_phys)
}

pub unsafe fn get_phys(virt: u64, pml4_phys: u64) -> Option<u64> {
    let pml4_virt = paging::phys_to_virt(PhysAddr::new(pml4_phys));
    let pml4 = &*(pml4_virt.as_ptr() as *const paging::PageTable);

    let p4_idx = (virt >> 39) & 0x1FF;
    let p3_idx = (virt >> 30) & 0x1FF;
    let p2_idx = (virt >> 21) & 0x1FF;
    let p1_idx = (virt >> 12) & 0x1FF;

    let p3_entry = pml4[p4_idx as usize];
    if p3_entry.is_unused() { return None; }
    let p3 = &*(paging::phys_to_virt(p3_entry.addr()).as_ptr() as *const paging::PageTable);

    let p2_entry = p3[p3_idx as usize];
    if p2_entry.is_unused() { return None; }
    if (p2_entry.as_u64() & paging::PAGE_HUGE) != 0 {
        return Some(p2_entry.addr().as_u64() + (virt & 0x3FFFFFFF));
    }
    let p2 = &*(paging::phys_to_virt(p2_entry.addr()).as_ptr() as *const paging::PageTable);

    let p1_entry = p2[p2_idx as usize];
    if p1_entry.is_unused() { return None; }
    if (p1_entry.as_u64() & paging::PAGE_HUGE) != 0 {
        return Some(p1_entry.addr().as_u64() + (virt & 0x1FFFFF));
    }
    let p1 = &*(paging::phys_to_virt(p1_entry.addr()).as_ptr() as *const paging::PageTable);

    let final_entry = p1[p1_idx as usize];
    if final_entry.is_unused() { return None; }
    Some(final_entry.addr().as_u64() + (virt & 0xFFF))
}

static mut MMIO_VIRT_HEAD: u64 = 0xFFFF_A000_0000_0000;
static mut KERNEL_MAPPING_HEAD: u64 = 0xFFFF_FA00_0000_0000;

pub fn map_mmio(phys: u64, size: usize) -> u64 {
    unsafe {
        let start_virt = MMIO_VIRT_HEAD;
        let pages = (size + 4095) / 4096;
        for i in 0..pages {
            let offset = i as u64 * 4096;
            map_page(start_virt + offset, PhysAddr::new(phys + offset),
                     paging::PAGE_PRESENT | paging::PAGE_WRITABLE | paging::PAGE_NO_CACHE, None);
        }
        MMIO_VIRT_HEAD += (pages as u64 * 4096) + 4096;
        start_virt
    }
}

pub fn map_user_memory_into_kernel(user_virt: u64, size: usize, user_pml4: u64) -> Option<u64> {
    unsafe {
        let offset_in_page = user_virt & 0xFFF;
        let pages = (offset_in_page as usize + size + 4095) / 4096;
        let start_virt = KERNEL_MAPPING_HEAD;

        let user_virt_aligned = user_virt & !0xFFF;

        let mut mapped = true;
        for i in 0..pages {
            let offset = i as u64 * 4096;
            if let Some(phys) = get_phys(user_virt_aligned + offset, user_pml4) {
                map_page(start_virt + offset, PhysAddr::new(phys & !0xFFF),
                         paging::PAGE_PRESENT | paging::PAGE_WRITABLE, None);
            } else {
                mapped = false;
                break;
            }
        }

        if mapped {
            KERNEL_MAPPING_HEAD += (pages as u64 * 4096) + 4096;
            Some(start_virt + offset_in_page)
        } else {
            None
        }
    }
}

pub unsafe fn new_user_pml4() -> u64 {
    create_user_pml4().expect("Failed to create user PML4")
}
