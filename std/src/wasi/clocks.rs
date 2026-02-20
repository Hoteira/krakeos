#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "wasi:clocks/monotonic-clock@0.2.0")]
unsafe extern "C" {
    #[link_name = "now"]
    pub fn monotonic_clock_now() -> u64;
    #[link_name = "resolution"]
    pub fn monotonic_clock_resolution() -> u64;
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn monotonic_clock_now() -> u64 {
    crate::sys::syscall(109, 0, 0, 0) * 1_000_000 // Convert ticks to ns
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn monotonic_clock_resolution() -> u64 {
    1_000_000 // 1ms
}

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "wasi:clocks/wall-clock@0.2.0")]
unsafe extern "C" {
    #[link_name = "now"]
    pub fn wall_clock_now(result_ptr: *mut u8);
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn wall_clock_now(result_ptr: *mut u8) {
    let (d, m, y) = crate::os::get_date();
    let (h, min, s) = crate::os::get_time();
    let yrs = if y >= 1970 { (y - 1970) as u64 } else { 0 };
    let secs = yrs * 31_536_000
        + (m as u64).saturating_sub(1) * 2_592_000
        + (d as u64).saturating_sub(1) * 86_400
        + (h as u64) * 3600
        + (min as u64) * 60
        + s as u64;
    core::ptr::write_unaligned(result_ptr as *mut u64, secs);
    core::ptr::write_unaligned(result_ptr.add(8) as *mut u32, 0); // nanoseconds
}

pub unsafe fn sleep(ms: u64) {
    #[cfg(target_arch = "wasm32")]
    {
        crate::sys::syscall(35, ms, 0, 0);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        crate::sys::syscall(35, ms, 0, 0);
        crate::sys::yield_task();
    }
}
