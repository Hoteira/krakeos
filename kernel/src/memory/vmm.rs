use crate::memory::{pmm, paging};
use core::arch::asm;

pub fn init() {
    unsafe {
        let _pml4_phys = (*(&raw const crate::boot::BOOT_INFO)).pml4;
    }
}

pub fn map_page(virt: u64, phys: u64, flags: u64, target_pml4_phys: Option<u64>) {
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

        let mut p3_entry = pml4_table.entries[p4_idx as usize];
        if p3_entry & paging::PAGE_PRESENT == 0 {
            let frame = pmm::allocate_frame(0).expect("VMM: OOM for PDPT");
            
            p3_entry = frame | paging::PAGE_PRESENT | paging::PAGE_WRITABLE | paging::PAGE_USER;
            pml4_table.entries[p4_idx as usize] = p3_entry;
            let p3_temp_table = paging::get_table_from_phys(p3_entry & 0x000FFFFFFFFFF000).expect("VMM: Cannot get P3 table from phys for zeroing");
            p3_temp_table.zero();
        }
        let p3 = paging::get_table_from_phys(p3_entry & 0x000FFFFFFFFFF000).expect("VMM: Failed to get L3 table from phys");

        let mut p2_entry = p3.entries[p3_idx as usize];
        
        if p2_entry & paging::PAGE_HUGE != 0 {
             let huge_page_base = p2_entry & 0x000FFFFFC0000000;
             let virt_offset = virt & 0x3FFFFFFF;
             let expected_phys = huge_page_base + virt_offset;
             
             if expected_phys == phys {
                 return;
             }
            panic!("VMM: Huge page collision at L3 level for virt {:#x}. Existing maps to {:#x}, requested {:#x}", virt, expected_phys, phys);
        }

        if p2_entry & paging::PAGE_PRESENT == 0 {
            let frame = pmm::allocate_frame(0).expect("VMM: OOM for PD");

            p2_entry = frame | paging::PAGE_PRESENT | paging::PAGE_WRITABLE | paging::PAGE_USER;
            p3.entries[p3_idx as usize] = p2_entry;
            let p2_temp_table = paging::get_table_from_phys(p2_entry & 0x000FFFFFFFFFF000).expect("VMM: Cannot get P2 table from phys for zeroing");
            p2_temp_table.zero();
        }
        
        if p2_entry & paging::PAGE_HUGE != 0 {
             let huge_page_base = p2_entry & 0x000FFFFFFFE00000;
             let virt_offset = virt & 0x1FFFFF;
             let expected_phys = huge_page_base + virt_offset;
             
             if expected_phys == phys {
                 return;
             }
            panic!("VMM: Huge page collision at L2 level for virt {:#x}. Existing maps to {:#x}, requested {:#x}", virt, expected_phys, phys);
        }

        let p2 = paging::get_table_from_phys(p2_entry & 0x000FFFFFFFFFF000).expect("VMM: Failed to get L2 table from phys");

        let mut p1_entry = p2.entries[p2_idx as usize];
        if p1_entry & paging::PAGE_PRESENT == 0 {
            let frame = pmm::allocate_frame(0).expect("VMM: OOM for PT");

            p1_entry = frame | paging::PAGE_PRESENT | paging::PAGE_WRITABLE | paging::PAGE_USER;
            p2.entries[p2_idx as usize] = p1_entry;
            let p1_temp_table = paging::get_table_from_phys(p1_entry & 0x000FFFFFFFFFF000).expect("VMM: Cannot get P1 table from phys for zeroing");
            p1_temp_table.zero();
        }
        
        let p1 = paging::get_table_from_phys(p1_entry & 0x000FFFFFFFFFF000).expect("VMM: Failed to get L1 table from phys");

        p1.entries[p1_idx as usize] = phys | flags;
        
        if target_pml4_phys.is_none() || target_pml4_phys == Some(pml4_table as *const _ as u64) {
            asm!("invlpg [{}]", in(reg) virt);
        } else {
        }
    }
}

pub unsafe fn new_user_pml4() -> u64 { unsafe {

    (*(&raw const crate::boot::BOOT_INFO)).pml4
}}