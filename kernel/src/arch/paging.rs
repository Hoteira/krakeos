#![allow(unsafe_op_in_unsafe_fn)]
use core::alloc::Layout;
use core::ptr;

pub const PAGE_SIZE: usize = 4096;

pub const PTE_V: u64 = 1 << 0;
pub const PTE_R: u64 = 1 << 1;
pub const PTE_W: u64 = 1 << 2;
pub const PTE_X: u64 = 1 << 3;
pub const PTE_U: u64 = 1 << 4;
pub const PTE_G: u64 = 1 << 5;
pub const PTE_A: u64 = 1 << 6;
pub const PTE_D: u64 = 1 << 7;

#[repr(C, align(4096))]
pub struct PageTable {
    pub entries: [u64; 512],
}

pub unsafe fn map_page(
    root: *mut PageTable,
    vaddr: usize,
    paddr: usize,
    flags: u64,
) {
    let vpn2 = (vaddr >> 30) & 0x1FF;
    let vpn1 = (vaddr >> 21) & 0x1FF;
    let vpn0 = (vaddr >> 12) & 0x1FF;

    let vpns = [vpn2, vpn1, vpn0];
    let mut table = root;

    for level in 0..2 {
        let index = vpns[level];
        let mut pte = (*table).entries[index];

        if (pte & PTE_V) == 0 {
            // Allocate new page table
            let layout = Layout::from_size_align(PAGE_SIZE, PAGE_SIZE).unwrap();
            let new_table = alloc::alloc::alloc_zeroed(layout) as *mut PageTable;
            
            let new_ppn = (new_table as u64) >> 12;
            pte = (new_ppn << 10) | PTE_V;
            (*table).entries[index] = pte;
        }

        let next_ppn = (pte >> 10) & ((1 << 44) - 1);
        table = (next_ppn << 12) as *mut PageTable;
    }

    // Level 0 (Leaf)
    let ppn = (paddr as u64) >> 12;
    (*table).entries[vpn0] = (ppn << 10) | flags | PTE_V | PTE_A | PTE_D;
}

pub unsafe fn map_range(
    root: *mut PageTable,
    vaddr_start: usize,
    paddr_start: usize,
    size: usize,
    flags: u64,
) {
    let mut offset = 0;
    while offset < size {
        map_page(
            root,
            vaddr_start + offset,
            paddr_start + offset,
            flags,
        );
        offset += PAGE_SIZE;
    }
}
