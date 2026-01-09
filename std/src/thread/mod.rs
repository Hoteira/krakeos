pub use crate::time::sleep;

pub fn yield_now() {
    crate::os::yield_task();
}
