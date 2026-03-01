// wasi:clocks host functions

method_export!("wasi:clocks/monotonic-clock@0.2.0", "now",
    pub unsafe fn monotonic_clock_now() -> u64 {
        crate::sys::syscall(109, 0, 0, 0) * 1_000_000
    }
);

method_export!("wasi:clocks/monotonic-clock@0.2.0", "resolution",
    pub unsafe fn monotonic_clock_resolution() -> u64 {
        1_000_000
    }
);

method_export!("wasi:clocks/monotonic-clock@0.2.0", "subscribe-duration",
    pub unsafe fn monotonic_clock_subscribe_duration(duration: u64) -> i32 {
        let ms = duration / 1_000_000;
        ms as i32
    }
);

method_export!("wasi:clocks/wall-clock@0.2.0", "now",
    pub unsafe fn wall_clock_now(result_ptr: *mut u8) {
        let res_date = crate::sys::syscall(115, 0, 0, 0);
        let y = (res_date >> 16) as u16;
        let m = (res_date >> 8) as u8;
        let d = res_date as u8;

        let res_time = crate::sys::syscall(108, 0, 0, 0);
        let h = (res_time >> 16) as u8;
        let min = (res_time >> 8) as u8;
        let s = res_time as u8;

        let yrs = if y >= 1970 { (y - 1970) as u64 } else { 0 };
        let secs = yrs * 31_536_000
            + (m as u64).saturating_sub(1) * 2_592_000
            + (d as u64).saturating_sub(1) * 86_400
            + (h as u64) * 3600
            + (min as u64) * 60
            + s as u64;

        core::ptr::write_unaligned(result_ptr as *mut u64, secs);
        core::ptr::write_unaligned(result_ptr.add(8) as *mut u32, 0);
    }
);

pub unsafe fn sleep(ms: u64) {
    let pollable = monotonic_clock_subscribe_duration(ms * 1_000_000);
    crate::io::host::poll_block(pollable);
    crate::io::host::pollable_drop(pollable);
}
