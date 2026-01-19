pub use crate::alloc as heap;
pub mod mmio;

pub fn malloc(size: usize) -> usize {
    unsafe {
        let layout = core::alloc::Layout::from_size_align(size, 8).unwrap();
        rust_alloc::alloc::alloc(layout) as usize
    }
}

pub fn free(ptr: usize, _pid: u64) {
    unsafe {
        let layout = core::alloc::Layout::from_size_align(0, 8).unwrap();
        rust_alloc::alloc::dealloc(ptr as *mut u8, layout);
    }
}

pub fn shm_get(name: &str, size: u64) -> u64 {
    unsafe {
        crate::os::syscall(120, name.as_ptr() as u64, name.len() as u64, size)
    }
}
