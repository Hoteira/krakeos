use crate::boot::BOOT_INFO;
use crate::debugln;
use core::sync::atomic::{AtomicBool, Ordering};
use super::address::PhysAddr;

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
    pub start: PhysAddr,
    pub count: usize,
    pub used: bool,
}

const MAX_ALLOCS: usize = 4096;

pub struct StructPmm {
    allocations: [FrameAllocation; MAX_ALLOCS],
    total_ram: PhysAddr,
    lock: AtomicBool,
}

static mut PMM: StructPmm = StructPmm {
    allocations: [FrameAllocation { pid: 0, start: PhysAddr::new(0), count: 0, used: false }; MAX_ALLOCS],
    total_ram: PhysAddr::new(0),
    lock: AtomicBool::new(false),
};

pub fn init() {
    unsafe {
        let mmap = (*(&raw mut BOOT_INFO)).mmap;

        debugln!("--- PMM Init: Memory Map ---");
        let mut max_addr: u64 = 0;
        for i in 0..32 {
            let entry = mmap.entries[i];

            if entry.length > 0 {
                let base = entry.base;
                let len = entry.length;
                let type_ = entry.memory_type;
                debugln!("  [{}] Base: {:#x}, Len: {:#x}, Type: {}", i, base, len, type_);
                let end = base + len;
                if end > max_addr { max_addr = end; }
            }
        }
        debugln!("----------------------------");

        let pmm_ptr = &raw mut PMM;
        (*pmm_ptr).total_ram = PhysAddr::new(max_addr);

        let pages = (0xA00000 / PAGE_SIZE) as usize;
        add_allocation(0, PhysAddr::new(0), pages);
    }
}

unsafe fn add_allocation(pid: u64, start: PhysAddr, count: usize) -> bool {
    unsafe {
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
                (*pmm_ptr).allocations[i + 1] = (*pmm_ptr).allocations[i];
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
}

unsafe fn remove_allocation(start: PhysAddr) {
    unsafe {
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
            let start_addr = (*pmm_ptr).allocations[found_idx].start;
            let size = (*pmm_ptr).allocations[found_idx].count as u64 * PAGE_SIZE;
            
            // Note: We still write bytes here using physical address as pointer
            // This works because of identity mapping.
            core::ptr::write_bytes(start_addr.as_u64() as *mut u8, 0, size as usize);

            for i in found_idx..(count_used - 1) {
                (*pmm_ptr).allocations[i] = (*pmm_ptr).allocations[i + 1];
            }
            (*pmm_ptr).allocations[count_used - 1].used = false;
        }
    }
}

unsafe fn is_overlap(start: PhysAddr, count: usize) -> bool {
    unsafe {
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
}

unsafe fn is_valid_ram(start: PhysAddr, count: usize) -> bool {
    unsafe {
        let end = start + (count as u64 * PAGE_SIZE);
        let mmap = (*(&raw mut BOOT_INFO)).mmap;

        for i in 0..32 {
            let entry = mmap.entries[i];
            if entry.memory_type == 1 && entry.length > 0 {
                let entry_base = PhysAddr::new(entry.base);
                let entry_end = entry_base + entry.length;
                if start >= entry_base && end <= entry_end {
                    return true;
                }
            }
        }
        false
    }
}

unsafe fn lock_pmm() {
    unsafe {
        let pmm_ptr = &raw mut PMM;
        while (*pmm_ptr).lock.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            core::hint::spin_loop();
        }
    }
}

unsafe fn unlock_pmm() {
    unsafe {
        let pmm_ptr = &raw mut PMM;
        (*pmm_ptr).lock.store(false, Ordering::Release);
    }
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

        let mut found_addr = PhysAddr::new(0);
        let mut found = false;

        let mut prev_end = PhysAddr::new(0);

        if count_used > 0 {
            prev_end = (*pmm_ptr).allocations[0].start + ((*pmm_ptr).allocations[0].count as u64 * PAGE_SIZE);
        } else {
            prev_end = PhysAddr::new(0xA00000);
        }

        if prev_end.as_u64() < 0xA00000 {
            prev_end = PhysAddr::new(0xA00000);
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
            let mmap = (*(&raw mut BOOT_INFO)).mmap;
            let req_size = pages as u64 * PAGE_SIZE;

            for i in 0..32 {
                let entry = mmap.entries[i];
                if entry.memory_type == 1 && entry.length > 0 {
                    let entry_base = PhysAddr::new(entry.base);
                    let entry_end = entry_base + entry.length;
                    
                    // If the region ends before our search start point, skip it.
                    if entry_end <= prev_end { continue; }

                    // We can start at the beginning of the region, or at prev_end, whichever is higher.
                    let mut candidate_start = if entry_base > prev_end { entry_base } else { prev_end };
                    
                    // Ensure page alignment (4KB)
                    if !candidate_start.is_aligned(PAGE_SIZE) {
                        candidate_start = candidate_start.align_up(PAGE_SIZE);
                    }

                    if candidate_start < entry_end {
                        let available = entry_end - candidate_start;
                        if available >= req_size {
                            found_addr = candidate_start;
                            found = true;
                            break;
                        }
                    }
                }
            }
        }

        if found {
            if add_allocation(pid, found_addr, pages) {
                // ZERO THE ALLOCATED MEMORY
                core::ptr::write_bytes(found_addr.as_u64() as *mut u8, 0, pages * PAGE_SIZE as usize);

                unlock_pmm();
                return Some(found_addr.as_u64());
            }
        }

        unlock_pmm();
        debugln!("PMM: FAILED to allocate {} bytes for PID {}. prev_end={:#x}, count_used={}", bytes, pid, prev_end.as_u64(), count_used);
        None
    }
}

pub fn reserve_frame(addr: u64) -> bool {
    reserve_frames(addr, 1)
}

pub fn reserve_frames(addr: u64, count: usize) -> bool {
    unsafe {
        lock_pmm();
        if is_overlap(PhysAddr::new(addr), count) {
            unlock_pmm();
            return false;
        }
        let res = add_allocation(0, PhysAddr::new(addr), count);
        if res {
            core::ptr::write_bytes(addr as *mut u8, 0, count * PAGE_SIZE as usize);
        }
        unlock_pmm();
        res
    }
}

pub fn free_frame(addr: u64) {
    unsafe {
        lock_pmm();
        remove_allocation(PhysAddr::new(addr));
        unlock_pmm();
    }
}

pub fn free_frames_by_pid(pid: u64) {
    unsafe {
        lock_pmm();
        let pmm_ptr = &raw mut PMM;

        let target_main = pid >> 32;
        let target_child = pid & 0xFFFFFFFF;

        let mut i = 0;
        while i < MAX_ALLOCS {
            if (*pmm_ptr).allocations[i].used {
                let alloc_pid = (*pmm_ptr).allocations[i].pid;
                let alloc_main = alloc_pid >> 32;


                let should_free = if target_child == 0 {
                    alloc_main == target_main
                } else {
                    alloc_pid == pid
                };

                if should_free {
                    let start_addr = (*pmm_ptr).allocations[i].start;
                    let size = (*pmm_ptr).allocations[i].count as u64 * PAGE_SIZE;
                    core::ptr::write_bytes(start_addr.as_u64() as *mut u8, 0, size as usize);

                    let count_used = {
                        let mut c = 0;
                        for k in 0..MAX_ALLOCS {
                            if (*pmm_ptr).allocations[k].used { c += 1; }
                        }
                        c
                    };

                    for k in i..(count_used - 1) {
                        (*pmm_ptr).allocations[k] = (*pmm_ptr).allocations[k + 1];
                    }
                    (*pmm_ptr).allocations[count_used - 1].used = false;


                    continue;
                }
            } else {
                break;
            }
            i += 1;
        }

        unlock_pmm();
    }
}

pub fn print_allocations() {
    unsafe {
        lock_pmm();
        let pmm_ptr = &raw mut PMM;

        debugln!("--- PMM Allocations ---");

        let mut count_used = 0;
        for i in 0..MAX_ALLOCS {
            if (*pmm_ptr).allocations[i].used {
                count_used += 1;
            } else {
                break;
            }
        }

        for i in 0..count_used {
            let alloc = (*pmm_ptr).allocations[i];
            let start = alloc.start;
            let end = start + (alloc.count as u64 * PAGE_SIZE);

            debugln!("PID {}: {:#x} -> {:#x} ({} pages)", alloc.pid, start.as_u64(), end.as_u64(), alloc.count);

            if i > 0 {
                let prev = (*pmm_ptr).allocations[i - 1];
                let prev_end = prev.start + (prev.count as u64 * PAGE_SIZE);
                if start < prev_end {
                    debugln!("!!! COLLISION DETECTED with previous allocation !!!");
                }
            }
        }
        debugln!("--- End of Allocations ---");

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
    unsafe { (*(&raw mut PMM)).total_ram.as_u64() as usize }
}

pub fn get_memory_usage_by_pid(pid: u64) -> usize {
    unsafe {
        let pmm_ptr = &raw mut PMM;
        let mut total_pages = 0;

        for i in 0..MAX_ALLOCS {
            if (*pmm_ptr).allocations[i].used {
                if (*pmm_ptr).allocations[i].pid == pid {
                    total_pages += (*pmm_ptr).allocations[i].count;
                }
            } else {
                break;
            }
        }

        total_pages * PAGE_SIZE as usize
    }
}