use crate::memory::{pmm, paging};
use core::arch::asm;
use crate::debugln;

pub fn init() {
    unsafe {
        let pml4_phys = (*(&raw const crate::boot::BOOT_INFO)).pml4;
        debugln!("[VMM] PML4 at {:#x}", pml4_phys);
    }
}

pub fn map_page(virt: u64, phys: u64, flags: u64) {
    unsafe {
        // std::println!("[VMM] Mapping v={:#x} to p={:#x}", virt, phys);
        let pml4 = paging::active_level_4_table();
        
        let p4_idx = (virt >> 39) & 0x1FF;
        let p3_idx = (virt >> 30) & 0x1FF;
        let p2_idx = (virt >> 21) & 0x1FF;
        let p1_idx = (virt >> 12) & 0x1FF;

        let mut p3_entry = pml4.entries[p4_idx as usize];
        if p3_entry & paging::PAGE_PRESENT == 0 {
            let frame = pmm::allocate_frame().expect("VMM: OOM for PDPT");
            // Ensure intermediate tables allow User access if we want user pages below them
            p3_entry = frame | paging::PAGE_PRESENT | paging::PAGE_WRITABLE | paging::PAGE_USER;
            pml4.entries[p4_idx as usize] = p3_entry;
            paging::get_table(p3_entry).unwrap().zero();
        }
        let p3 = paging::get_table(p3_entry).expect("VMM: Failed to get L3 table");

        let mut p2_entry = p3.entries[p3_idx as usize];
        if p2_entry & paging::PAGE_PRESENT == 0 {
            let frame = pmm::allocate_frame().expect("VMM: OOM for PD");
            p2_entry = frame | paging::PAGE_PRESENT | paging::PAGE_WRITABLE | paging::PAGE_USER;
            p3.entries[p3_idx as usize] = p2_entry;
            paging::get_table(p2_entry).unwrap().zero();
        }
        if p2_entry & paging::PAGE_HUGE != 0 {
            panic!("VMM: Huge page collision at L3 level for virt {:#x}", virt);
        }
        let p2 = paging::get_table(p2_entry).expect("VMM: Failed to get L2 table");

        let mut p1_entry = p2.entries[p2_idx as usize];
        if p1_entry & paging::PAGE_PRESENT == 0 {
            let frame = pmm::allocate_frame().expect("VMM: OOM for PT");
            p1_entry = frame | paging::PAGE_PRESENT | paging::PAGE_WRITABLE | paging::PAGE_USER;
            p2.entries[p2_idx as usize] = p1_entry;
            paging::get_table(p1_entry).unwrap().zero();
        }
        if p1_entry & paging::PAGE_HUGE != 0 {
            panic!("VMM: Huge page collision at L2 level for virt {:#x}", virt);
        }
        let p1 = paging::get_table(p1_entry).expect("VMM: Failed to get L1 table");

        p1.entries[p1_idx as usize] = phys | flags;
        
        asm!("invlpg [{}]", in(reg) virt);
    }
}

pub fn unmap_page(virt: u64) {
    unsafe {
        let pml4 = paging::active_level_4_table();
        let p4_idx = (virt >> 39) & 0x1FF;
        let p3_entry = pml4.entries[p4_idx as usize];
        
        if let Some(p3) = paging::get_table(p3_entry) {
            let p3_idx = (virt >> 30) & 0x1FF;
            let p2_entry = p3.entries[p3_idx as usize];
            
            if let Some(p2) = paging::get_table(p2_entry) {
                 let p2_idx = (virt >> 21) & 0x1FF;
                 let p1_entry = p2.entries[p2_idx as usize];
                 
                 if let Some(p1) = paging::get_table(p1_entry) {
                     let p1_idx = (virt >> 12) & 0x1FF;
                     p1.entries[p1_idx as usize] = 0;
                     asm!("invlpg [{}]", in(reg) virt);
                 }
            }
        }
    }
}

pub fn map_user_stack(phys_frame: u64) -> u64 {
    // Pick a high virtual address for the user stack.
    // Let's use 0x0000_7000_0000_0000 + phys_frame (just to keep it unique per frame for now)
    // A real OS would use a VMA allocator.
    let virt_addr = 0x0000_7000_0000_0000 + phys_frame; 
    
    map_page(
        virt_addr, 
        phys_frame, 
        paging::PAGE_PRESENT | paging::PAGE_WRITABLE | paging::PAGE_USER
    );
    
    virt_addr
}