use crate::memory::address::PhysAddr;
use crate::memory::paging::PageTableFlags;
use crate::memory::{paging, pmm};
use core::arch::asm;
use crate::debugln;

pub static mut KERNEL_PML4: u64 = 0;

pub fn init() {
    unsafe {
        let new_pml4_phys = pmm::allocate_frame().expect("VMM: Failed to allocate initial kernel PML4");
        let new_pml4_addr = PhysAddr::new(new_pml4_phys);
        KERNEL_PML4 = new_pml4_phys;


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
            let pdpt_frame = pmm::allocate_frame().expect("VMM: OOM for Shared Kernel PDPT");
            let mut entry = paging::PageTableEntry::new();
            entry.set_addr(PhysAddr::new(pdpt_frame),
                           paging::PageTableFlags::PRESENT | paging::PageTableFlags::WRITABLE);
            pml4[p4_idx as usize] = entry;

            let pdpt_virt = paging::phys_to_virt(PhysAddr::new(pdpt_frame));
            core::ptr::write_bytes(pdpt_virt.as_mut_ptr::<u8>(), 0, 4096);
        }

        // The bootloader shares one PDPT between PML4[0] (user SAS), PML4[256]
        // (HHDM) and PML4[511].  map_physical_memory() will fill the shared PDPT
        // with kernel-only huge pages, which poisons user-space code/stack/linear
        // memory regions that live under PML4[0].  Give PML4[0] its own empty PDPT
        // so the two address ranges are independent.
        {
            let user_pdpt_frame = pmm::allocate_frame().expect("VMM: OOM for user PDPT");
            let user_pdpt_virt = paging::phys_to_virt(PhysAddr::new(user_pdpt_frame));
            core::ptr::write_bytes(user_pdpt_virt.as_mut_ptr::<u8>(), 0, 4096);

            let mut entry = paging::PageTableEntry::new();
            entry.set_addr(PhysAddr::new(user_pdpt_frame),
                           paging::PageTableFlags::PRESENT
                               | paging::PageTableFlags::WRITABLE
                               | paging::PageTableFlags::USER_ACCESSIBLE);
            pml4[0] = entry;
        }

        asm!("mov cr3, {}", in(reg) new_pml4_phys);


        (*(&raw mut crate::boot::BOOT_INFO)).pml4 = new_pml4_phys;

        map_physical_memory(new_pml4_phys);
    }
}

pub fn map_physical_memory(pml4_phys: u64) {
    unsafe {
        let mmap = (*(&raw mut crate::boot::BOOT_INFO)).mmap;

        for i in 0..32 {
            let entry = mmap.entries[i];
            if entry.memory_type == 1 && entry.length > 0 {
                let start = entry.base;
                let end = entry.base + entry.length;
                
                // Align to 2MB for huge page mapping
                let mut current = start & !0x1FFFFF;
                let aligned_end = (end + 0x1FFFFF) & !0x1FFFFF;

                while current < aligned_end {
                    // SKIP PCI HOLE: Do not map 3GB-4GB into the Cached HHDM.
                    // This prevents cache-type alias conflicts with PCI BARs.
                    if current >= 0xC0000000 && current < 0x100000000 {
                        current = 0x100000000;
                        continue;
                    }

                    let virt = current + paging::HHDM_OFFSET;
                    let flags = paging::PAGE_PRESENT | paging::PAGE_WRITABLE;
                    
                    // Map using 2MB huge pages to minimize page table overhead
                    map_huge_page(virt, PhysAddr::new(current), flags, Some(pml4_phys));
                    current += 0x200000; // 2MB
                }
            }
        }
    }
}

pub fn map_huge_page(virt: u64, phys: PhysAddr, flags_raw: u64, target_pml4_phys: Option<u64>) {
    unsafe {
        let flags = flags_raw | paging::PAGE_HUGE;
        let pml4_table = if let Some(pml4_addr) = target_pml4_phys {
            paging::get_table_from_phys(pml4_addr).expect("VMM: Invalid target PML4")
        } else {
            paging::active_level_4_table()
        };

        let p4_idx = (virt >> 39) & 0x1FF;
        let p3_idx = (virt >> 30) & 0x1FF;
        let p2_idx = (virt >> 21) & 0x1FF;

        let is_user = (flags & paging::PAGE_USER) != 0;

        // Level 4 -> Level 3
        let mut p3_entry = pml4_table[p4_idx as usize];
        if p3_entry.is_unused() {
            let frame = pmm::allocate_frame().expect("VMM: OOM for PDPT");
            let mut new_entry = paging::PageTableEntry::new();
            let table_flags = paging::PageTableFlags::from_bits_truncate(paging::PAGE_PRESENT | paging::PAGE_WRITABLE | paging::PAGE_USER);
            new_entry.set_addr(PhysAddr::new(frame), table_flags);
            pml4_table[p4_idx as usize] = new_entry;
            paging::get_table_from_phys(frame).unwrap().zero();
            p3_entry = new_entry;
        } else {
            let mut f = p3_entry.flags();
            let mut changed = false;
            if is_user && !f.contains(PageTableFlags::USER_ACCESSIBLE) {
                f |= PageTableFlags::USER_ACCESSIBLE;
                changed = true;
            }
            if (flags_raw & paging::PAGE_WRITABLE) != 0 && !f.contains(PageTableFlags::WRITABLE) {
                f |= PageTableFlags::WRITABLE;
                changed = true;
            }
            if changed {
                p3_entry.set_flags(f);
                pml4_table[p4_idx as usize] = p3_entry;
            }
        }

        let p3 = paging::get_table_from_phys(p3_entry.addr().as_u64()).expect("VMM: Failed to get L3 table");

        // Level 3 -> Level 2
        let mut p2_table_entry = p3[p3_idx as usize];
        if p2_table_entry.is_unused() {
            let frame = pmm::allocate_frame().expect("VMM: OOM for PD");
            let mut new_entry = paging::PageTableEntry::new();
            let table_flags = paging::PageTableFlags::from_bits_truncate(paging::PAGE_PRESENT | paging::PAGE_WRITABLE | paging::PAGE_USER);
            new_entry.set_addr(PhysAddr::new(frame), table_flags);
            p3[p3_idx as usize] = new_entry;
            paging::get_table_from_phys(frame).unwrap().zero();
            p2_table_entry = new_entry;
        } else {
            // Ensure PD has enough permissions
            let mut f = p2_table_entry.flags();
            let mut changed = false;
            if is_user && !f.contains(PageTableFlags::USER_ACCESSIBLE) {
                f |= PageTableFlags::USER_ACCESSIBLE;
                changed = true;
            }
            if (flags_raw & paging::PAGE_WRITABLE) != 0 && !f.contains(PageTableFlags::WRITABLE) {
                f |= PageTableFlags::WRITABLE;
                changed = true;
            }
            if changed {
                p2_table_entry.set_flags(f);
                p3[p3_idx as usize] = p2_table_entry;
            }
        }

        let p2 = paging::get_table_from_phys(p2_table_entry.addr().as_u64()).expect("VMM: Failed to get L2 table");

        // Set Level 2 entry as HUGE page
        let mut p2_entry = paging::PageTableEntry::new();
        *(&mut p2_entry as *mut _ as *mut u64) = phys.as_u64() | flags;
        p2[p2_idx as usize] = p2_entry;

        let current_cr3: u64;
        asm!("mov {}, cr3", out(reg) current_cr3);
        let current_pml4 = current_cr3 & 0x000F_FFFF_FFFF_F000;

        if target_pml4_phys.is_none() || target_pml4_phys == Some(current_pml4) {
            asm!("invlpg [{}]", in(reg) virt);
        }
    }
}

pub fn map_page(virt: u64, phys: PhysAddr, flags_raw: u64, target_pml4_phys: Option<u64>) {
    unsafe {
        let flags = flags_raw;
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
            let frame = pmm::allocate_frame().expect("VMM: OOM for PDPT");
            let mut new_entry = paging::PageTableEntry::new();
            let table_flags = paging::PageTableFlags::from_bits_truncate(paging::PAGE_PRESENT | paging::PAGE_WRITABLE | paging::PAGE_USER);
            new_entry.set_addr(PhysAddr::new(frame), table_flags);
            pml4_table[p4_idx as usize] = new_entry;
            paging::get_table_from_phys(frame).unwrap().zero();
            p3_entry = new_entry;
        } else {
            let mut flags = p3_entry.flags();
            let mut changed = false;
            if is_user && !flags.contains(PageTableFlags::USER_ACCESSIBLE) {
                flags |= PageTableFlags::USER_ACCESSIBLE;
                changed = true;
            }
            if (flags_raw & paging::PAGE_WRITABLE) != 0 && !flags.contains(PageTableFlags::WRITABLE) {
                flags |= PageTableFlags::WRITABLE;
                changed = true;
            }
            if changed {
                p3_entry.set_flags(flags);
                pml4_table[p4_idx as usize] = p3_entry;
            }
        }

        let p3 = paging::get_table_from_phys(p3_entry.addr().as_u64()).expect("VMM: Failed to get L3 table");


        let mut p2_entry = p3[p3_idx as usize];
        if p2_entry.is_unused() {
            let frame = pmm::allocate_frame().expect("VMM: OOM for PD");
            let mut new_entry = paging::PageTableEntry::new();
            let table_flags = paging::PageTableFlags::from_bits_truncate(paging::PAGE_PRESENT | paging::PAGE_WRITABLE | paging::PAGE_USER);
            new_entry.set_addr(PhysAddr::new(frame), table_flags);
            p3[p3_idx as usize] = new_entry;
            paging::get_table_from_phys(frame).unwrap().zero();
            p2_entry = new_entry;
        } else if (p2_entry.as_u64() & paging::PAGE_HUGE) != 0 {
            let frame = pmm::allocate_frame().expect("VMM: OOM for L2 shattering");
            let new_table = paging::get_table_from_phys(frame).unwrap();
            let base_phys = p2_entry.addr().as_u64();
            let huge_flags = paging::PageTableFlags::from_bits_truncate(p2_entry.as_u64() & 0xFFF);
            for i in 0..512 {
                let mut e = paging::PageTableEntry::new();
                e.set_addr(PhysAddr::new(base_phys + (i as u64 * 0x40000000 / 512)), huge_flags);
                new_table[i] = e;
            }
            p2_entry.set_addr(PhysAddr::new(frame), (huge_flags & !PageTableFlags::HUGE_PAGE) | PageTableFlags::PRESENT);
            p3[p3_idx as usize] = p2_entry;
        } else {
            let mut flags = p2_entry.flags();
            let mut changed = false;
            if is_user && !flags.contains(PageTableFlags::USER_ACCESSIBLE) {
                flags |= PageTableFlags::USER_ACCESSIBLE;
                changed = true;
            }
            if (flags_raw & paging::PAGE_WRITABLE) != 0 && !flags.contains(PageTableFlags::WRITABLE) {
                flags |= PageTableFlags::WRITABLE;
                changed = true;
            }
            if changed {
                p2_entry.set_flags(flags);
                p3[p3_idx as usize] = p2_entry;
            }
        }

        let p2 = paging::get_table_from_phys(p2_entry.addr().as_u64()).expect("VMM: Failed to get L2 table");


        let mut p1_entry = p2[p2_idx as usize];
        if p1_entry.is_unused() {
            let frame = pmm::allocate_frame().expect("VMM: OOM for PT");
            let mut new_entry = paging::PageTableEntry::new();
            let table_flags = paging::PageTableFlags::from_bits_truncate(paging::PAGE_PRESENT | paging::PAGE_WRITABLE | paging::PAGE_USER);
            new_entry.set_addr(PhysAddr::new(frame), table_flags);
            p2[p2_idx as usize] = new_entry;
            paging::get_table_from_phys(frame).unwrap().zero();
            p1_entry = new_entry;
        } else if (p1_entry.as_u64() & paging::PAGE_HUGE) != 0 {
            let frame = pmm::allocate_frame().expect("VMM: OOM for L1 shattering");
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
        } else {
            let mut flags = p1_entry.flags();
            let mut changed = false;
            if is_user && !flags.contains(PageTableFlags::USER_ACCESSIBLE) {
                flags |= PageTableFlags::USER_ACCESSIBLE;
                changed = true;
            }
            if (flags_raw & paging::PAGE_WRITABLE) != 0 && !flags.contains(PageTableFlags::WRITABLE) {
                flags |= PageTableFlags::WRITABLE;
                changed = true;
            }
            if changed {
                p1_entry.set_flags(flags);
                p2[p2_idx as usize] = p1_entry;
            }
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

const MMIO_BASE: u64 = 0xFFFF_A000_0000_0000;
const MMIO_SIZE: u64 = 1024 * 1024 * 1024 * 1024; // 1 TB
const MMIO_CHUNK: u64 = 2 * 1024 * 1024; // 2 MB
const MMIO_BITMAP_SIZE: usize = (MMIO_SIZE / MMIO_CHUNK / 64) as usize;

static mut MMIO_BITMAP: [u64; MMIO_BITMAP_SIZE] = [0; MMIO_BITMAP_SIZE];
static mut KERNEL_MAPPING_HEAD: u64 = 0xFFFF_FA00_0000_0000;

pub fn map_mmio(phys: u64, size: usize) -> u64 {
    unsafe {
        let chunks_needed = (size as u64 + MMIO_CHUNK - 1) / MMIO_CHUNK;
        let mut start_chunk = 0;
        let mut found = false;

        // Simple first-fit search in the bitmap
        'outer: for i in 0..MMIO_BITMAP_SIZE {
            if MMIO_BITMAP[i] == u64::MAX { continue; }
            for bit in 0..64 {
                let chunk_idx = i * 64 + bit;
                let mut possible = true;
                for j in 0..chunks_needed {
                    let c = chunk_idx + j as usize;
                    let word = c / 64;
                    let b = c % 64;
                    if word >= MMIO_BITMAP_SIZE || (MMIO_BITMAP[word] & (1 << b)) != 0 {
                        possible = false;
                        break;
                    }
                }
                if possible {
                    start_chunk = chunk_idx;
                    found = true;
                    break 'outer;
                }
            }
        }

        if !found {
            panic!("VMM: Out of virtual MMIO space!");
        }

        // Mark as used
        for j in 0..chunks_needed {
            let c = start_chunk + j as usize;
            MMIO_BITMAP[c / 64] |= 1 << (c % 64);
        }

        let start_virt = MMIO_BASE + (start_chunk as u64 * MMIO_CHUNK);
        let pages = (size + 4095) / 4096;
        for i in 0..pages {
            let offset = i as u64 * 4096;
            // Use both NO_CACHE (PCD) and WRITE_THROUGH (PWT) to ensure Strong Uncacheable behavior.
            map_page(start_virt + offset, PhysAddr::new(phys + offset),
                     paging::PAGE_PRESENT | paging::PAGE_WRITABLE | paging::PAGE_NO_CACHE | paging::PAGE_WRITE_THROUGH, None);
        }
        start_virt
    }
}

pub fn unmap_mmio(virt: u64, size: usize) {
    if virt < MMIO_BASE || virt >= MMIO_BASE + MMIO_SIZE { return; }
    unsafe {
        let start_chunk = (virt - MMIO_BASE) / MMIO_CHUNK;
        let chunks = (size as u64 + MMIO_CHUNK - 1) / MMIO_CHUNK;
        for j in 0..chunks {
            let c = (start_chunk + j) as usize;
            MMIO_BITMAP[c / 64] &= !(1 << (c % 64));
        }
        // In a production OS, we would also unmap from page tables here.
        // For now, we just reclaim the virtual address space.
    }
}


pub fn unmap_and_free_range(virt_start: u64, size: u64) {
    let mut current = virt_start & !0xFFF;
    let end = (virt_start + size + 0xFFF) & !0xFFF;

    unsafe {
        let pml4 = paging::active_level_4_table();

        while current < end {
            let p4_idx = ((current >> 39) & 0x1FF) as usize;
            let p3_idx = ((current >> 30) & 0x1FF) as usize;
            let p2_idx = ((current >> 21) & 0x1FF) as usize;
            let p1_idx = ((current >> 12) & 0x1FF) as usize;

            let mut p3_entry = pml4[p4_idx];
            if p3_entry.is_unused() {
                current = (current + 0x8000000000) & !0x7FFFFFFFFF;
                continue;
            }

            let p3 = paging::get_table_from_phys(p3_entry.addr().as_u64()).unwrap();
            let mut p2_entry = p3[p3_idx];
            if p2_entry.is_unused() {
                current = (current + 0x40000000) & !0x3FFFFFFF;
                continue;
            }

            if (p2_entry.as_u64() & paging::PAGE_HUGE) != 0 {
                pmm::free_frame(p2_entry.addr().as_u64());
                p2_entry.set_unused();
                p3[p3_idx] = p2_entry;
                current = (current + 0x200000) & !0x1FFFFF;
                continue;
            }

            let p2 = paging::get_table_from_phys(p2_entry.addr().as_u64()).unwrap();
            let mut p1_entry = p2[p2_idx];
            if p1_entry.is_unused() {
                current = (current + 0x200000) & !0x1FFFFF;
                continue;
            }

            let p1 = paging::get_table_from_phys(p1_entry.addr().as_u64()).unwrap();
            let mut final_entry = p1[p1_idx];
            if !final_entry.is_unused() {
                pmm::free_frame(final_entry.addr().as_u64());
                final_entry.set_unused();
                p1[p1_idx] = final_entry;
            }
            
            current += 4096;
        }
    }
}

/// Vector reserved for TLB shootdown IPIs.
/// Must match the IDT handler that calls `tlb_shootdown_handler()`.
pub const TLB_SHOOTDOWN_VECTOR: u8 = 0x50;

/// Flush the local TLB and acknowledge TLB shootdown (called from IDT handler).
pub fn tlb_shootdown_handler() {
    unsafe {
        let cr3: u64;
        core::arch::asm!("mov {}, cr3", out(reg) cr3);
        core::arch::asm!("mov cr3, {}", in(reg) cr3);
    }
    crate::arch::x86_64::apic::eoi();
}

/// Invalidate TLBs on all CPUs by broadcasting a shootdown IPI.
/// Call this after modifying kernel page table entries that are visible
/// to other CPUs (e.g., `map_page`, `unmap_and_free_range`).
pub fn tlb_shootdown_all() {
    // Flush our own TLB first
    unsafe {
        let cr3: u64;
        core::arch::asm!("mov {}, cr3", out(reg) cr3);
        core::arch::asm!("mov cr3, {}", in(reg) cr3);
    }
    // Then kick all other CPUs
    crate::arch::x86_64::apic::broadcast_ipi_except_self(TLB_SHOOTDOWN_VECTOR);
}
