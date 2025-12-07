use crate::memory::{pmm, paging};
use core::arch::asm;
use crate::debugln;

pub fn init() {
    unsafe {
        let pml4_phys = (*(&raw const crate::boot::BOOT_INFO)).pml4;
        debugln!("[VMM] PML4 at {:#x}", pml4_phys);
        
        let pml4 = paging::active_level_4_table();
        
        // Bootloader has already mapped 0-4GB (Entries 0-3) with User+RW.
        // We only need to extend the identity map to cover more RAM if needed (e.g. up to 32GB).
        
        let pdpt = paging::get_table_from_phys(pml4.entries[0] & 0x000FFFFFFFFFF000).unwrap();
        
        // Map 4GiB-32GiB using 2MB Huge Pages (L2)
        // Start from index 4 (4GB), preserving bootloader's 0-3.
        for i in 4..32 {
            let pd_frame = pmm::allocate_frame().expect("VMM: OOM for Identity PD");
            let pd_ptr = pd_frame as *mut paging::PageTable;
            (*pd_ptr).zero();
            
            for j in 0..512 {
                let phys_addr = (i as u64 * 1024 * 1024 * 1024) + (j as u64 * 2 * 1024 * 1024);
                (*pd_ptr).entries[j] = phys_addr | paging::PAGE_PRESENT | paging::PAGE_WRITABLE | paging::PAGE_USER | paging::PAGE_HUGE;
            }
            
            pdpt.entries[i] = pd_frame | paging::PAGE_PRESENT | paging::PAGE_WRITABLE | paging::PAGE_USER;
        }
        
        debugln!("[VMM] Extended Identity Map (4-32GiB). 0-4GiB preserved from Bootloader.");
    }
}

pub fn map_page(virt: u64, phys: u64, flags: u64, target_pml4_phys: Option<u64>) {
    // ... (Keep existing map_page implementation, it handles huge page collisions now)
    debugln!("[VMM] map_page: Mapping virt {:#x} to phys {:#x} with flags {:#b} for PML4 {:#x?}", virt, phys, flags, target_pml4_phys);
    
    // Safety Check: Prevent User pages in Kernel Space
    if (flags & paging::PAGE_USER) != 0 {
        if virt >= 0xFFFF_8000_0000_0000 {
            panic!("VMM: Attempt to map user page at kernel address {:#x}", virt);
        }
    }

    unsafe {
        let pml4_table = if let Some(pml4_addr) = target_pml4_phys {
            // Use the provided PML4 physical address
            // debugln!("[VMM] map_page: Using provided PML4 physical address {:#x}", pml4_addr);
            &mut *(pml4_addr as *mut paging::PageTable) // Direct access assuming identity map for page tables
        } else {
            // Use the currently active PML4 from CR3
            // debugln!("[VMM] map_page: Using active PML4 from CR3.");
            paging::active_level_4_table()
        };
        
        let p4_idx = (virt >> 39) & 0x1FF;
        let p3_idx = (virt >> 30) & 0x1FF;
        let p2_idx = (virt >> 21) & 0x1FF;
        let p1_idx = (virt >> 12) & 0x1FF;

        // debugln!("[VMM] map_page: P4_idx={}, P3_idx={}, P2_idx={}, P1_idx={}", p4_idx, p3_idx, p2_idx, p1_idx);

        let mut p3_entry = pml4_table.entries[p4_idx as usize]; // Use pml4_table
        // debugln!("[VMM] map_page: P3_entry for P4_idx {} is {:#x}", p4_idx, p3_entry);
        if p3_entry & paging::PAGE_PRESENT == 0 {
            // debugln!("[VMM] map_page: P3 table not present, allocating frame for PDPT...");
            let frame = pmm::allocate_frame().expect("VMM: OOM for PDPT");
            // debugln!("[VMM] map_page: Allocated frame {:#x} for PDPT", frame);
            
            // Ensure this frame is mapped in the kernel so we can write to it
            // Recursive call might hit huge page collision check? 
            // If we identity map, it might hit the 1GB/2MB pages we just set up.
            // But that's fine, map_page handles collisions now.
            map_page(frame, frame, paging::PAGE_PRESENT | paging::PAGE_WRITABLE, None);
            
            // Ensure intermediate tables allow User access if we want user pages below them
            p3_entry = frame | paging::PAGE_PRESENT | paging::PAGE_WRITABLE | paging::PAGE_USER;
            pml4_table.entries[p4_idx as usize] = p3_entry; // Update pml4_table
            // debugln!("[VMM] map_page: P4 entry updated with new P3 table {:#x}. Zeroing PDPT...", p3_entry);
            // Assuming page tables are identity mapped or zeroable directly.
            let p3_temp_table = paging::get_table_from_phys(p3_entry & 0x000FFFFFFFFFF000).expect("VMM: Cannot get P3 table from phys for zeroing");
            p3_temp_table.zero();
            // debugln!("[VMM] map_page: PDPT zeroed.");
        }
        let p3 = paging::get_table_from_phys(p3_entry & 0x000FFFFFFFFFF000).expect("VMM: Failed to get L3 table from phys");
        // debugln!("[VMM] map_page: Got p3 at {:#x}", p3 as *const _ as u64);

        let mut p2_entry = p3.entries[p3_idx as usize];
        // debugln!("[VMM] map_page: P2_entry for P3_idx {} is {:#x}", p3_idx, p2_entry);
        
        // Check for Huge Page at L3 (1GB) level (unlikely but possible)
        if p2_entry & paging::PAGE_HUGE != 0 {
             // 1GB Page collision check
             // If we are trying to identity map X to X, and X is already covered by this huge page, we are fine.
             let huge_page_base = p2_entry & 0x000FFFFFC0000000; // 1GB mask
             let virt_offset = virt & 0x3FFFFFFF;
             let expected_phys = huge_page_base + virt_offset;
             
             if expected_phys == phys {
                 // debugln!("[VMM] map_page: Virt {:#x} already mapped to Phys {:#x} via 1GB Huge Page. Skipping.", virt, phys);
                 return;
             }
            panic!("VMM: Huge page collision at L3 level for virt {:#x}. Existing maps to {:#x}, requested {:#x}", virt, expected_phys, phys);
        }

        if p2_entry & paging::PAGE_PRESENT == 0 {
            // debugln!("[VMM] map_page: P2 table not present, allocating frame for PD...");
            let frame = pmm::allocate_frame().expect("VMM: OOM for PD");
            // debugln!("[VMM] map_page: Allocated frame {:#x} for PD", frame);
            
            map_page(frame, frame, paging::PAGE_PRESENT | paging::PAGE_WRITABLE, None);

            p2_entry = frame | paging::PAGE_PRESENT | paging::PAGE_WRITABLE | paging::PAGE_USER;
            p3.entries[p3_idx as usize] = p2_entry;
            // debugln!("[VMM] map_page: P3 entry updated with new P2 table {:#x}. Zeroing PD...", p2_entry);
            let p2_temp_table = paging::get_table_from_phys(p2_entry & 0x000FFFFFFFFFF000).expect("VMM: Cannot get P2 table from phys for zeroing");
            p2_temp_table.zero();
            // debugln!("[VMM] map_page: PD zeroed.");
        }
        
        // Check for Huge Page at L2 (2MB) level - This was the source of the panic
        if p2_entry & paging::PAGE_HUGE != 0 {
             // 2MB Page collision check
             let huge_page_base = p2_entry & 0x000FFFFFFFE00000; // 2MB mask (Bits 21-51)
             let virt_offset = virt & 0x1FFFFF;
             let expected_phys = huge_page_base + virt_offset;
             
             if expected_phys == phys {
                 // debugln!("[VMM] map_page: Virt {:#x} already mapped to Phys {:#x} via 2MB Huge Page. Skipping.", virt, phys);
                 return;
             }
            panic!("VMM: Huge page collision at L2 level for virt {:#x}. Existing maps to {:#x}, requested {:#x}", virt, expected_phys, phys);
        }

        // debugln!("[VMM] map_page: Getting P2 table...");
        let p2 = paging::get_table_from_phys(p2_entry & 0x000FFFFFFFFFF000).expect("VMM: Failed to get L2 table from phys");
        // debugln!("[VMM] map_page: Got p2 at {:#x}", p2 as *const _ as u64);

        let mut p1_entry = p2.entries[p2_idx as usize];
        // debugln!("[VMM] map_page: P1_entry for P2_idx {} is {:#x}", p2_idx, p1_entry);
        if p1_entry & paging::PAGE_PRESENT == 0 {
            // debugln!("[VMM] map_page: P1 table not present, allocating frame for PT...");
            let frame = pmm::allocate_frame().expect("VMM: OOM for PT");
            // debugln!("[VMM] map_page: Allocated frame {:#x} for PT", frame);
            
            map_page(frame, frame, paging::PAGE_PRESENT | paging::PAGE_WRITABLE, None);

            p1_entry = frame | paging::PAGE_PRESENT | paging::PAGE_WRITABLE | paging::PAGE_USER;
            p2.entries[p2_idx as usize] = p1_entry;
            // debugln!("[VMM] map_page: P2 entry updated with new P1 table {:#x}. Zeroing PT...", p1_entry);
            let p1_temp_table = paging::get_table_from_phys(p1_entry & 0x000FFFFFFFFFF000).expect("VMM: Cannot get P1 table from phys for zeroing");
            p1_temp_table.zero();
            // debugln!("[VMM] map_page: PT zeroed.");
        }
        // Removed incorrect PAGE_HUGE check for L1 (PT) level. Bit 7 is PAT here.
        
        // debugln!("[VMM] map_page: Getting P1 table...");
        let p1 = paging::get_table_from_phys(p1_entry & 0x000FFFFFFFFFF000).expect("VMM: Failed to get L1 table from phys");
        // debugln!("[VMM] map_page: Got p1 at {:#x}", p1 as *const _ as u64);

        // debugln!("[VMM] map_page: Setting P1 entry {:#x} to phys {:#x} with flags {:#b}", p1_idx, phys, flags);
        p1.entries[p1_idx as usize] = phys | flags;
        
        // Invalidate TLB only if we're modifying the currently active page table
        if target_pml4_phys.is_none() || target_pml4_phys == Some(pml4_table as *const _ as u64) {
            // debugln!("[VMM] map_page: Invalidating TLB for virt {:#x}", virt);
            asm!("invlpg [{}]", in(reg) virt);
            // debugln!("[VMM] map_page: TLB invalidated.");
        } else {
            // debugln!("[VMM] map_page: TLB not invalidated, as mapping a non-active PML4.");
        }
    }
    // debugln!("[VMM] map_page: Mapping completed successfully.");
}

pub unsafe fn new_user_pml4() -> u64 {
    // Return the Boot PML4 directly.
    // This effectively shares the kernel's address space with the user.
    // Since we mapped 0-32GiB as User+RW in init(), user tasks can run freely.
    (*(&raw const crate::boot::BOOT_INFO)).pml4
}

pub fn map_user_stack(phys_frame: u64, target_pml4_phys: u64) -> u64 {
    // Pick a high virtual address for the user stack.
    // Let's use 0x0000_7000_0000_0000 + phys_frame (just to keep it unique per frame for now)
    // A real OS would use a VMA allocator.
    let virt_addr = 0x0000_7000_0000_0000 + phys_frame; 
    
    map_page(
        virt_addr, 
        phys_frame, 
        paging::PAGE_PRESENT | paging::PAGE_WRITABLE | paging::PAGE_USER,
        Some(target_pml4_phys) // Pass the target PML4
    );
    
    virt_addr
}