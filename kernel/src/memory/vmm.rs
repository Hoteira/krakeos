use crate::memory::{paging, pmm};
use crate::memory::address::PhysAddr;
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

        let mut p3_entry = pml4_table[p4_idx as usize];
        if p3_entry.is_unused() {
            let frame = pmm::allocate_frame(0).expect("VMM: OOM for PDPT");
            
            // Create a temporary entry to write back
            let mut new_entry = paging::PageTableEntry::new();
            // We use raw flags here because map_page takes raw u64 flags
            // This is temporary until map_page signature is updated
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

        // Construct the final entry using raw u64 manipulation to preserve caller flags
        // Access inner entry via unsafe transmute or just set it if we expose a setter?
        // PageTableEntry is transparent, so we can just pointer cast if needed, but let's add a raw setter to paging.rs if needed.
        // Actually I added set_addr which takes flags.
        
        // p1.entries[p1_idx as usize] = phys | flags; <--- Old
        let mut final_entry = paging::PageTableEntry::new();
        // Manually constructing because we don't have a way to set raw u64 flags yet
        // Let's rely on the fact that PageTableEntry is repr(transparent)
        *( &mut final_entry as *mut _ as *mut u64 ) = phys | flags;
        
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