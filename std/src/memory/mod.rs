pub mod mmio;

use core::sync::atomic::{AtomicU64, Ordering};

static SAS_HEAP_BASE: AtomicU64 = AtomicU64::new(0);
static SAS_HEAP_TOP: AtomicU64 = AtomicU64::new(0);

pub fn init_sas_manager(base: u64) {
    SAS_HEAP_BASE.store(base, Ordering::SeqCst);
    // Reserve the first 128MB for the native std heap (malloc)
    SAS_HEAP_TOP.store(base + 128 * 1024 * 1024, Ordering::SeqCst);
}

pub fn allocate_sas_region(size: u64) -> Option<u64> {
    let top = SAS_HEAP_TOP.fetch_add(size, Ordering::SeqCst);
    if top + size > SAS_HEAP_BASE.load(Ordering::SeqCst) + 4 * 1024 * 1024 * 1024 {
        None
    } else {
        Some(top)
    }
}

pub fn malloc(size: usize) -> usize {
    unsafe {
        let layout = core::alloc::Layout::from_size_align(size, 8).unwrap();
        crate::alloc::alloc::alloc(layout) as usize
    }
}

pub fn free(ptr: usize, _size: usize) {
    unsafe {
        let layout = core::alloc::Layout::from_size_align(_size, 8).unwrap();
        crate::alloc::alloc::dealloc(ptr as *mut u8, layout);
    }
}

pub fn shm_get(name: &str, size: u64) -> Option<u64> {
    let res = unsafe {
        crate::os::krakeos::shm_get_raw(name.as_ptr(), name.len(), size as usize)
    };
    if res == u64::MAX { None } else { Some(res) }
}
