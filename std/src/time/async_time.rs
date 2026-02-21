use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};
use core::time::Duration;

pub struct SleepFuture {
    deadline_ms: u64,
}

impl SleepFuture {
    pub fn new(duration: Duration) -> Self {
        let now = crate::os::get_system_ticks();
        SleepFuture {
            deadline_ms: now + duration.as_millis() as u64,
        }
    }
}

impl Future for SleepFuture {
    type Output = ();

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let now = crate::os::get_system_ticks();
        if now >= self.deadline_ms {
            Poll::Ready(())
        } else {
            // Register timer with kernel if we want to be efficient
            // For now, executor's yield/wait loop will wake us
            // SYS_REGISTER_EVENT(Timer, deadline_ms)
            #[cfg(not(target_arch = "wasm32"))]
            unsafe { crate::os::syscall(131, 2, self.deadline_ms, 0); }
            #[cfg(target_arch = "wasm32")]
            { /* stub or use wasi poll */ }
            Poll::Pending
        }
    }
}

pub fn sleep(duration: Duration) -> SleepFuture {
    SleepFuture::new(duration)
}
