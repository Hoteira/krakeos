use crate::task::noop_waker;
use core::future::Future;
use core::task::{Context, Poll};

pub fn block_on<F: Future>(mut future: F) -> F::Output {
    let mut future = unsafe { core::pin::Pin::new_unchecked(&mut future) };
    let waker = noop_waker();
    let mut cx = Context::from_waker(&waker);

    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(val) => return val,
            Poll::Pending => {
                // In a real OS, we'd sleep until the waker is called.
                // For now, we just yield to the OS.
                crate::os::yield_task();
            }
        }
    }
}
