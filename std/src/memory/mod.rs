pub use crate::alloc as heap;
pub mod mmio;

pub fn malloc(size: usize) -> usize {
    unsafe {
        let layout = core::alloc::Layout::from_size_align(size, 8).unwrap();
        crate::rust_alloc::alloc::alloc(layout) as usize
    }
}

pub fn free(ptr: usize, _size: usize) {
    unsafe {
        let layout = core::alloc::Layout::from_size_align(_size, 8).unwrap();
        crate::rust_alloc::alloc::dealloc(ptr as *mut u8, layout);
    }
}

pub fn shm_get(name: &str, size: u64) -> Option<u64> {
    let res = unsafe {
        crate::os::syscall(120, name.as_ptr() as u64, name.len() as u64, size)
    };
    if res == u64::MAX { None } else { Some(res) }
}
