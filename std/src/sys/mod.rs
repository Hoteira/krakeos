#[cfg(not(target_arch = "wasm32"))]
pub use crate::wasi::krakeos::{
    krakeos_syscall as syscall,
    krakeos_syscall5 as syscall4,
    krakeos_syscall6 as syscall5,
    krakeos_syscall7 as syscall6,
    yield_task,
    hlt_loop,
    alloc_pages,
};

#[cfg(target_arch = "wasm32")]
pub use crate::wasi::krakeos::{
    yield_task,
    hlt_loop,
    alloc_pages,
};

// Syscalls removed for WASM target
#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn syscall1(num: u64, a1: u64) -> u64 { syscall(num, a1, 0, 0) }
#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn syscall2(num: u64, a1: u64, a2: u64) -> u64 { syscall(num, a1, a2, 0) }
#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn syscall3(num: u64, a1: u64, a2: u64, a3: u64) -> u64 { syscall(num, a1, a2, a3) }
