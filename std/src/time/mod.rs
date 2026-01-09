pub use core::time::Duration;

pub fn sleep(duration: Duration) {
    let ms = duration.as_millis() as u64;
    crate::os::sleep(ms);
}
