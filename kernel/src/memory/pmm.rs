use core::sync::atomic::{AtomicBool, Ordering};
use crate::boot::{BOOT_INFO, MemoryMapEntry};


pub const PAGE_SIZE: u64 = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum FrameError {
    NoMemory,
    IndexOutOfBounds,
}

#[derive(Debug, Clone, Copy)]
pub struct FrameAllocation {
    pub pid: u64,
    pub start: u64,
    pub count: usize,
    pub used: bool,
}

const MAX_ALLOCS: usize = 4096;

pub struct StructPmm {
    allocations: [FrameAllocation; MAX_ALLOCS],
    total_ram: u64,
    lock: AtomicBool,
}

static mut PMM: StructPmm = StructPmm {
    allocations: [FrameAllocation { pid: 0, start: 0, count: 0, used: false }; MAX_ALLOCS],
    total_ram: 0,
    lock: AtomicBool::new(false),
};

pub fn init() {
    unsafe {
        let mmap = (*(&raw mut BOOT_INFO)).mmap;
        
        let mut max_addr: u64 = 0;
        for i in 0..32 {
            let entry = mmap.entries[i];
            
            if entry.length > 0 {
                let end = entry.base + entry.length;
                if end > max_addr { max_addr = end; }
            }
        }
        
        let pmm_ptr = &raw mut PMM;
        (*pmm_ptr).total_ram = max_addr;

        let pages = (0xA00000 / PAGE_SIZE) as usize;
        add_allocation(0, 0, pages);
    }
}

unsafe fn add_allocation(pid: u64, start: u64, count: usize) -> bool {
    let pmm_ptr = &raw mut PMM;
    
    let mut count_used = 0;
    for i in 0..MAX_ALLOCS {
        if (*pmm_ptr).allocations[i].used {
            count_used += 1;
        }
    }
    
    if count_used >= MAX_ALLOCS {
        return false;
    }

    let mut idx = 0;
    while idx < count_used {
        if (*pmm_ptr).allocations[idx].start > start {
            break;
        }
        idx += 1;
    }

    if idx < count_used {
        for i in (idx..count_used).rev() {
            (*pmm_ptr).allocations[i+1] = (*pmm_ptr).allocations[i];
        }
    }
    
    (*pmm_ptr).allocations[idx] = FrameAllocation {
        pid,
        start,
        count,
        used: true,
    };
    
    true
}

unsafe fn remove_allocation(start: u64) {
    let pmm_ptr = &raw mut PMM;
    let mut found_idx = MAX_ALLOCS;
    let mut count_used = 0;
    
    for i in 0..MAX_ALLOCS {
        if (*pmm_ptr).allocations[i].used {
            count_used += 1;
            if (*pmm_ptr).allocations[i].start == start {
                found_idx = i;
            }
        } else {
            break; 
        }
    }

    if found_idx != MAX_ALLOCS {
        for i in found_idx..(count_used - 1) {
            (*pmm_ptr).allocations[i] = (*pmm_ptr).allocations[i+1];
        }
        (*pmm_ptr).allocations[count_used - 1].used = false;
    }
}

unsafe fn is_overlap(start: u64, count: usize) -> bool {
    let end = start + (count as u64 * PAGE_SIZE);
    let pmm_ptr = &raw mut PMM;
    
    for i in 0..MAX_ALLOCS {
        let alloc = &(*pmm_ptr).allocations[i];
        if alloc.used {
            let alloc_end = alloc.start + (alloc.count as u64 * PAGE_SIZE);
            if start < alloc_end && end > alloc.start {
                return true;
            }
        } else {
            break; 
        }
    }
    false
}

unsafe fn is_valid_ram(start: u64, count: usize) -> bool {
    let end = start + (count as u64 * PAGE_SIZE);
    let mmap = (*(&raw mut BOOT_INFO)).mmap;

    for i in 0..32 {
        let entry = mmap.entries[i];
        if entry.memory_type == 1 && entry.length > 0 {
            let entry_end = entry.base + entry.length;
            if start >= entry.base && end <= entry_end {
                return true;
            }
        }
    }
    false
}

unsafe fn lock_pmm() {
    let pmm_ptr = &raw mut PMM;
    while (*pmm_ptr).lock.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }
}

unsafe fn unlock_pmm() {
    let pmm_ptr = &raw mut PMM;
    (*pmm_ptr).lock.store(false, Ordering::Release);
}

pub fn allocate_frame(pid: u64) -> Option<u64> {
    allocate_frames(1, pid)
}

pub fn allocate_frames(count: usize, pid: u64) -> Option<u64> {
    allocate_memory(count * PAGE_SIZE as usize, pid)
}

pub fn allocate_memory(bytes: usize, pid: u64) -> Option<u64> {
    let pages = (bytes + PAGE_SIZE as usize - 1) / PAGE_SIZE as usize;
    if pages == 0 { return None; }

    unsafe {
        lock_pmm();
        let pmm_ptr = &raw mut PMM;
        
        let mut count_used = 0;
        for i in 0..MAX_ALLOCS {
            if (*pmm_ptr).allocations[i].used {
                count_used += 1;
            } else {
                break;
            }
        }

        let mut found_addr = 0;
        let mut found = false;

        let mut prev_end = 0;
        
        if count_used > 0 {
            prev_end = (*pmm_ptr).allocations[0].start + ((*pmm_ptr).allocations[0].count as u64 * PAGE_SIZE);
        } else {
            prev_end = 0xA00000;
        }

        if prev_end < 0xA00000 {
            prev_end = 0xA00000;
        }

        for i in 0..count_used {
            let current = (*pmm_ptr).allocations[i];
            
            if current.start > prev_end {
                let gap_size = current.start - prev_end;
                if gap_size >= (pages as u64 * PAGE_SIZE) {
                    if is_valid_ram(prev_end, pages) {
                        found_addr = prev_end;
                        found = true;
                        break;
                    }
                }
            }
            
            let current_end = current.start + (current.count as u64 * PAGE_SIZE);
            if current_end > prev_end {
                prev_end = current_end;
            }
        }

        if !found {
            if prev_end + (pages as u64 * PAGE_SIZE) <= (*pmm_ptr).total_ram {
                if is_valid_ram(prev_end, pages) {
                    found_addr = prev_end;
                    found = true;
                }
            }
        }

        if found {
            if add_allocation(pid, found_addr, pages) {
                unlock_pmm();
                return Some(found_addr);
            }
        }
        
        unlock_pmm();
        None
    }
}

pub unsafe fn realloc_memory(old_addr: u64, old_bytes: usize, new_bytes: usize, pid: u64) -> Option<u64> {
    if new_bytes <= old_bytes { return Some(old_addr); }
    
    let old_pages = (old_bytes + PAGE_SIZE as usize - 1) / PAGE_SIZE as usize;
    let new_pages = (new_bytes + PAGE_SIZE as usize - 1) / PAGE_SIZE as usize;
    
    if old_pages == new_pages { return Some(old_addr); }

    lock_pmm();
    let pmm_ptr = &raw mut PMM;
    
    let mut alloc_idx = MAX_ALLOCS;
    for i in 0..MAX_ALLOCS {
        if (*pmm_ptr).allocations[i].used && (*pmm_ptr).allocations[i].start == old_addr {
            alloc_idx = i;
            break;
        }
    }
    
    if alloc_idx == MAX_ALLOCS {
        unlock_pmm();
        return None;
    }
    
    let current_alloc = (*pmm_ptr).allocations[alloc_idx];
    let extension_start = current_alloc.start + (current_alloc.count as u64 * PAGE_SIZE);
    let needed_extra = new_pages - current_alloc.count;
    
    if is_valid_ram(extension_start, needed_extra) {
         if !is_overlap(extension_start, needed_extra) {
             (*pmm_ptr).allocations[alloc_idx].count = new_pages;
             unlock_pmm();
             return Some(old_addr);
         }
    }
    
    unlock_pmm();
    
    if let Some(new_addr) = allocate_memory(new_bytes, pid) {
        core::ptr::copy_nonoverlapping(old_addr as *const u8, new_addr as *mut u8, old_bytes);
        free_frame(old_addr);
        return Some(new_addr);
    }

    None
}

pub fn reserve_frame(addr: u64) -> bool {
    unsafe {
        lock_pmm();
        if is_overlap(addr, 1) {
            unlock_pmm();
            return false;
        }
        let res = add_allocation(0, addr, 1);
        unlock_pmm();
        res
    }
}

pub fn free_frame(addr: u64) {
    unsafe {
        lock_pmm();
        remove_allocation(addr);
        unlock_pmm();
    }
}

#[allow(dead_code)]
pub fn get_used_memory() -> usize {
    unsafe {
        let pmm_ptr = &raw mut PMM;
        let mut total = 0;
        for i in 0..MAX_ALLOCS {
            if (*pmm_ptr).allocations[i].used {
                total += (*pmm_ptr).allocations[i].count;
            }
        }
        total * PAGE_SIZE as usize
    }
}

#[allow(dead_code)]
pub fn get_total_memory() -> usize {
    unsafe { (*(&raw mut PMM)).total_ram as usize }
}