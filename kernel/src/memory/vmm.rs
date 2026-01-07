use crate::memory::{paging, pmm};
use crate::memory::address::PhysAddr;
use core::arch::asm;

pub fn init() {
    unsafe {
        // Allocate a new frame for the Kernel PML4
        let new_pml4_phys = pmm::allocate_frame(0).expect("VMM: Failed to allocate initial kernel PML4");
        let new_pml4_addr = PhysAddr::new(new_pml4_phys);

        // Get the current (Bootloader) PML4
        let old_pml4_phys = (*(&raw const crate::boot::BOOT_INFO)).pml4;

        // Copy the mappings (Identity Map) from the old table to the new one
        core::ptr::copy_nonoverlapping(
            old_pml4_phys as *const u8,
            new_pml4_addr.as_u64() as *mut u8,
            4096 // paging::PAGE_SIZE
        );

        // Switch CR3 to the new PML4
        asm!("mov cr3, {}", in(reg) new_pml4_phys);

        // Update global tracking so new processes inherit this table
        (*(&raw mut crate::boot::BOOT_INFO)).pml4 = new_pml4_phys;

        crate::debugln!("VMM: Relocated PML4 from {:#x} to {:#x}", old_pml4_phys, new_pml4_phys);
    }
}

pub fn map_page(virt: u64, phys: PhysAddr, flags: u64, target_pml4_phys: Option<u64>) {
    if (flags & paging::PAGE_USER) != 0 {
        if virt >= 0xFFFF_8000_0000_0000 {
            panic!("VMM: Attempt to map user page at kernel address {:#x}", virt);
        }
    }

    unsafe {
        let pml4_table = if let Some(pml4_addr) = target_pml4_phys {
            &mut *(pml4_addr as *mut paging::PageTable)
        } else {
            paging::active_level_4_table()
        };

        let p4_idx = (virt >> 39) & 0x1FF;
        let p3_idx = (virt >> 30) & 0x1FF;
        let p2_idx = (virt >> 21) & 0x1FF;
        let p1_idx = (virt >> 12) & 0x1FF;

        let mut p3_entry = pml4_table[p4_idx as usize];
        if p3_entry.is_unused() {
            let frame = pmm::allocate_frame(0).expect("VMM: OOM for PDPT");
            
            let mut new_entry = paging::PageTableEntry::new();
            let flags = paging::PageTableFlags::from_bits_truncate(paging::PAGE_PRESENT | paging::PAGE_WRITABLE | paging::PAGE_USER);
            new_entry.set_addr(PhysAddr::new(frame), flags);
            pml4_table[p4_idx as usize] = new_entry;
            
            let p3_temp_table = paging::get_table_from_phys(frame).expect("VMM: Cannot get P3 table from phys for zeroing");
            p3_temp_table.zero();
            
            p3_entry = new_entry;
        }
        
        let p3 = paging::get_table_from_phys(p3_entry.addr().as_u64()).expect("VMM: Failed to get L3 table from phys");

        let mut p2_entry = p3[p3_idx as usize];

        if (p2_entry.as_u64() & paging::PAGE_HUGE) != 0 {
            panic!("VMM: Huge page collision at L3 level");
        }

        if p2_entry.is_unused() {
            let frame = pmm::allocate_frame(0).expect("VMM: OOM for PD");

            let mut new_entry = paging::PageTableEntry::new();
            let flags = paging::PageTableFlags::from_bits_truncate(paging::PAGE_PRESENT | paging::PAGE_WRITABLE | paging::PAGE_USER);
            new_entry.set_addr(PhysAddr::new(frame), flags);
            p3[p3_idx as usize] = new_entry;

            let p2_temp_table = paging::get_table_from_phys(frame).expect("VMM: Cannot get P2 table from phys for zeroing");
            p2_temp_table.zero();
            
            p2_entry = new_entry;
        }

        if (p2_entry.as_u64() & paging::PAGE_HUGE) != 0 {
             panic!("VMM: Huge page collision at L2 level");
        }

        let p2 = paging::get_table_from_phys(p2_entry.addr().as_u64()).expect("VMM: Failed to get L2 table from phys");

        let mut p1_entry = p2[p2_idx as usize];
        if p1_entry.is_unused() {
            let frame = pmm::allocate_frame(0).expect("VMM: OOM for PT");

            let mut new_entry = paging::PageTableEntry::new();
            let flags = paging::PageTableFlags::from_bits_truncate(paging::PAGE_PRESENT | paging::PAGE_WRITABLE | paging::PAGE_USER);
            new_entry.set_addr(PhysAddr::new(frame), flags);
            p2[p2_idx as usize] = new_entry;

            let p1_temp_table = paging::get_table_from_phys(frame).expect("VMM: Cannot get P1 table from phys for zeroing");
            p1_temp_table.zero();
            
            p1_entry = new_entry;
        }

        let p1 = paging::get_table_from_phys(p1_entry.addr().as_u64()).expect("VMM: Failed to get L1 table from phys");

        let mut final_entry = paging::PageTableEntry::new();
        // Construct the final entry using raw u64 manipulation to preserve caller flags (which are u64)
        // Ideally we would take PageTableFlags in map_page too, but that is a bigger change.
        *( &mut final_entry as *mut _ as *mut u64 ) = phys.as_u64() | flags;
        
        p1[p1_idx as usize] = final_entry;

        if target_pml4_phys.is_none() || target_pml4_phys == Some(pml4_table as *const _ as u64) {
            asm!("invlpg [{}]", in(reg) virt);
        }
    }
}

pub unsafe fn new_user_pml4() -> u64 {
    unsafe {
        (*(&raw const crate::boot::BOOT_INFO)).pml4
    }
}