pub use core::task::*;

use core::task::{RawWaker, RawWakerVTable, Waker};

pub fn noop_waker() -> Waker {
    unsafe { Waker::from_raw(noop_raw_waker()) }
}

fn noop_raw_waker() -> RawWaker {
    fn noop(_: *const ()) {}
    fn clone(_: *const ()) -> RawWaker {
        noop_raw_waker()
    }

    let vtable = &RawWakerVTable::new(clone, noop, noop, noop);
    RawWaker::new(core::ptr::null(), vtable)
}
