pub mod heap;
pub mod mmio;

use crate::os::syscall;

pub fn malloc(size: usize) -> usize {
    unsafe { syscall(5, size as u64, 0, 0) as usize }
}

pub fn free(base: usize, pid: u64) {
    let main_pid = (pid >> 32);
    let child_pid = pid & 0xFFFFFFFF;
    unsafe { syscall(6, base as u64, main_pid, child_pid); }
}
