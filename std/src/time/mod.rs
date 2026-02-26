pub use core::time::Duration;
use crate::wasi::clocks;

pub mod async_time;
#[cfg(feature = "userland")]
pub mod wasi;

pub fn sleep(duration: Duration) {
    let ms = duration.as_millis() as u64;
    unsafe { clocks::sleep(ms) };
}

pub fn monotonic_now() -> Duration {
    let ns = unsafe { clocks::monotonic_clock_now() };
    Duration::from_nanos(ns)
}

pub fn wall_now() -> (u64, u32) {
    let mut result = [0u8; 12];
    unsafe {
        clocks::wall_clock_now(result.as_mut_ptr());
        let secs = core::ptr::read_unaligned(result.as_ptr() as *const u64);
        let nsecs = core::ptr::read_unaligned(result.as_ptr().add(8) as *const u32);
        (secs, nsecs)
    }
}
