use core::sync::atomic::{AtomicU64, Ordering};

// Start Code at 4GB
static NEXT_CODE_ADDR: AtomicU64 = AtomicU64::new(0x0000_0001_0000_0000);

// Start Heap at 512GB
static NEXT_HEAP_ADDR: AtomicU64 = AtomicU64::new(0x0000_0080_0000_0000);

// Start SHM at 256GB
static NEXT_SHM_ADDR: AtomicU64 = AtomicU64::new(0x0000_0040_0000_0000);

// Start Stack at 112TB (Top of user space roughly)
static NEXT_STACK_ADDR: AtomicU64 = AtomicU64::new(0x0000_7FFF_FFFF_0000);

pub fn allocate_code(size: u64, pid: u64) -> u64 {
    // Each process gets a 1GB region for code/static data
    let region_size = 1024 * 1024 * 1024;
    let addr = NEXT_CODE_ADDR.fetch_add(region_size, Ordering::Relaxed);

    // We still track the actual requested size for reference
    let tracked_size = (size + 0xFFF) & !0xFFF;
    crate::memory::vma::GLOBAL_VMA.lock().track(addr, tracked_size, pid);
    addr
}

pub fn allocate_shm(size: u64) -> u64 {
    // SHM regions are allocated as requested, aligned to 2MB
    let aligned_size = (size + 0x1FFFFF) & !0x1FFFFF;
    let addr = NEXT_SHM_ADDR.fetch_add(aligned_size, Ordering::Relaxed);
    crate::memory::vma::GLOBAL_VMA.lock().track(addr, aligned_size, 0);
    addr
}

pub fn allocate_heap(_size: u64, pid: u64) -> u64 {
    // Each process gets a 4GB region for heap
    let region_size = 4 * 1024 * 1024 * 1024;
    let addr = NEXT_HEAP_ADDR.fetch_add(region_size, Ordering::Relaxed);
    crate::memory::vma::GLOBAL_VMA.lock().track(addr, region_size, pid);
    addr
}

pub fn allocate_stack(size: u64, pid: u64) -> u64 {
    // Allocate downwards. Return the TOP of the stack (high address).
    let total_size = size + 4096;
    let aligned_size = (total_size + 0xFFFFF) & !0xFFFFF; // 1MB alignment

    let old_top = NEXT_STACK_ADDR.fetch_sub(aligned_size, Ordering::Relaxed);
    let base = old_top - aligned_size;
    crate::memory::vma::GLOBAL_VMA.lock().track(base, aligned_size, pid);
    old_top
}