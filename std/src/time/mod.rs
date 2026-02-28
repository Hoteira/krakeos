pub use core::time::Duration;

pub mod host;
pub mod async_time;
#[cfg(feature = "userland")]
pub mod wasi;

use host as clocks;

// --- Clock method_export! bindings (from wasi/clocks.rs) ---

method_export!("wasi:clocks/monotonic-clock@0.2.0", "now",
    pub unsafe fn monotonic_clock_now() -> u64 {
        crate::sys::syscall(109, 0, 0, 0) * 1_000_000 // Convert ticks to ns
    }
);

method_export!("wasi:clocks/monotonic-clock@0.2.0", "resolution",
    pub unsafe fn monotonic_clock_resolution() -> u64 {
        1_000_000 // 1ms
    }
);

method_export!("wasi:clocks/monotonic-clock@0.2.0", "subscribe-duration",
    pub unsafe fn monotonic_clock_subscribe_duration(_duration: u64) -> i32 {
        0 // stub
    }
);

method_export!("wasi:clocks/wall-clock@0.2.0", "now",
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
);

// --- Public API ---

pub fn sleep(duration: Duration) {
    let ms = duration.as_millis() as u64;
    #[cfg(target_arch = "wasm32")]
    unsafe {
        let pollable = monotonic_clock_subscribe_duration(ms * 1_000_000);
        crate::io::streams::poll_block(pollable);
        crate::io::streams::pollable_drop(pollable);
    }
    #[cfg(not(target_arch = "wasm32"))]
    unsafe {
        crate::sys::syscall(35, ms, 0, 0);
        crate::sys::yield_task();
    }
}

pub fn monotonic_now() -> Duration {
    let ns = unsafe { monotonic_clock_now() };
    Duration::from_nanos(ns)
}

pub fn wall_now() -> (u64, u32) {
    let mut result = [0u8; 12];
    unsafe {
        wall_clock_now(result.as_mut_ptr());
        let secs = core::ptr::read_unaligned(result.as_ptr() as *const u64);
        let nsecs = core::ptr::read_unaligned(result.as_ptr().add(8) as *const u32);
        (secs, nsecs)
    }
}
