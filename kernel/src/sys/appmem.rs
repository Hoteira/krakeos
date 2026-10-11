// On-demand linear memory for wasm apps. A slot's base never moves, so AOT code can keep
// it in a register; pages are backed only as the memory grows.

use core::alloc::Layout;

use crate::arch::paging::{self, PAGE_SIZE, PTE_R, PTE_U, PTE_W};

// Above RAM's identity map, in the user half of Sv39.
const REGION_BASE: usize = 0x20_0000_0000;
pub const SLOT_SIZE: usize = 1 << 30;
const MAX_SLOTS: usize = 64;
// Commits fail once the kernel heap would drop below this, so apps can't starve the kernel.
const KERNEL_RESERVE: usize = 16 * 1024 * 1024;

#[derive(Copy, Clone)]
struct Slot {
    in_use: bool,
    owner: usize,
    // Bytes from the slot base backed by pages (page-aligned).
    committed: usize,
}

static mut SLOTS: [Slot; MAX_SLOTS] = [Slot { in_use: false, owner: 0, committed: 0 }; MAX_SLOTS];

fn slot_base(i: usize) -> usize {
    REGION_BASE + i * SLOT_SIZE
}

unsafe fn find(base: usize, tid: usize) -> Option<usize> {
    if base < REGION_BASE {
        return None;
    }
    let i = (base - REGION_BASE) / SLOT_SIZE;
    let slots = &*core::ptr::addr_of!(SLOTS);
    if i < MAX_SLOTS && slot_base(i) == base && slots[i].in_use && slots[i].owner == tid {
        Some(i)
    } else {
        None
    }
}

pub fn reserve(tid: usize) -> Option<usize> {
    unsafe {
        let slots = &mut *core::ptr::addr_of_mut!(SLOTS);
        let i = slots.iter().position(|s| !s.in_use)?;
        slots[i] = Slot { in_use: true, owner: tid, committed: 0 };
        Some(slot_base(i))
    }
}

// On failure nothing changes.
pub fn commit(base: usize, size: usize, tid: usize) -> bool {
    unsafe {
        let Some(i) = find(base, tid) else { return false };
        if size > SLOT_SIZE {
            return false;
        }
        let slots = &mut *core::ptr::addr_of_mut!(SLOTS);
        let old = slots[i].committed;
        let new = (size + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
        if new <= old {
            return true;
        }
        if crate::ALLOCATOR.lock().free() < (new - old) + KERNEL_RESERVE {
            return false;
        }
        let layout = Layout::from_size_align(PAGE_SIZE, PAGE_SIZE).unwrap();
        let mut off = old;
        while off < new {
            let page = alloc::alloc::alloc_zeroed(layout) as usize;
            if page == 0 {
                unmap_range(base, old, off);
                return false;
            }
            paging::map_page(crate::ROOT_PAGE_TABLE, base + off, page, PTE_R | PTE_W | PTE_U);
            off += PAGE_SIZE;
        }
        crate::csr::sfence_vma();
        slots[i].committed = new;
        true
    }
}

// Unmap and free the pages in [base+from, base+to).
unsafe fn unmap_range(base: usize, from: usize, to: usize) {
    let layout = Layout::from_size_align(PAGE_SIZE, PAGE_SIZE).unwrap();
    let mut off = from;
    while off < to {
        if let Some(page) = paging::unmap_page(crate::ROOT_PAGE_TABLE, base + off) {
            alloc::alloc::dealloc(page as *mut u8, layout);
        }
        off += PAGE_SIZE;
    }
    crate::csr::sfence_vma();
}

unsafe fn free_slot(i: usize) {
    let slots = &mut *core::ptr::addr_of_mut!(SLOTS);
    unmap_range(slot_base(i), 0, slots[i].committed);
    slots[i] = Slot { in_use: false, owner: 0, committed: 0 };
}

pub fn release(base: usize, tid: usize) -> bool {
    unsafe {
        match find(base, tid) {
            Some(i) => {
                free_slot(i);
                true
            }
            None => false,
        }
    }
}

pub fn release_all_of(tid: usize) {
    unsafe {
        for i in 0..MAX_SLOTS {
            let s = (*core::ptr::addr_of!(SLOTS))[i];
            if s.in_use && s.owner == tid {
                free_slot(i);
            }
        }
    }
}

pub fn committed_by(tid: usize) -> usize {
    unsafe {
        (*core::ptr::addr_of!(SLOTS))
            .iter()
            .filter(|s| s.in_use && s.owner == tid)
            .map(|s| s.committed)
            .sum()
    }
}
