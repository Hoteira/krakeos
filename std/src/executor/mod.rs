use crate::os::{PollFd, POLLIN, POLLOUT, REACTOR};
use crate::rust_alloc::sync::Arc;
use crate::rust_alloc::task::Wake;
use crate::rust_alloc::vec::Vec;
use crate::task::{noop_waker, waker_ref, Task, RUN_QUEUE};
use core::future::Future;
use core::task::{Context, Poll, Waker};

pub struct Executor;

impl Executor {
    pub fn new() -> Self {
        Executor
    }

    pub fn spawn<F>(&mut self, future: F)
    where
        F: Future<Output=()> + Send + 'static,
    {
        let task = Arc::new(Task::new(future));
        RUN_QUEUE.lock().push_back(task);
    }

    pub fn run(&mut self) {
        let pid = crate::process::get_pid();
        loop {
            let task = RUN_QUEUE.lock().pop_front();
            if let Some(task) = task {
                let waker = waker_ref(&task);
                let mut context = Context::from_waker(&waker);
                match task.poll(&mut context) {
                    Poll::Ready(()) => {} // Task finished
                    Poll::Pending => {
                        // Task still pending, will be re-queued by waker
                    }
                }
            } else {
                // If we have nothing to do, wait for an event OR poll the reactor
                let mut poll_fds = Vec::new();
                {
                    let reactor = REACTOR.lock();
                    for &fd in reactor.read_waiters.keys() {
                        poll_fds.push(PollFd { fd, events: POLLIN, revents: 0 });
                    }
                    for &fd in reactor.write_waiters.keys() {
                        poll_fds.push(PollFd { fd, events: POLLOUT, revents: 0 });
                    }
                }

                if poll_fds.is_empty() {

                    // No I/O to wait for, just wait for generic signal

                    unsafe { crate::os::syscall(130, 0, pid, 0); }
                } else {

                    // Wait for I/O or generic signal

                    let n = crate::os::poll(&mut poll_fds, -1);


                    if n > 0 {

                        // Process ready FDs

                        let mut reactor = REACTOR.lock();

                        for pfd in poll_fds {
                            if (pfd.revents & POLLIN) != 0 {
                                if let Some(waker) = reactor.read_waiters.remove(&pfd.fd) {
                                    waker.wake();
                                }
                            }

                            if (pfd.revents & POLLOUT) != 0 {
                                if let Some(waker) = reactor.write_waiters.remove(&pfd.fd) {
                                    waker.wake();
                                }
                            }
                        }
                    } else if n == 0 {

                        // Generic wakeup occurred (from waker.wake() -> signal_event)

                        // The next loop iteration will pick up the task from RUN_QUEUE

                    }
                }
            }
        }
    }
}

pub fn block_on<F: Future>(mut future: F) -> F::Output {
    let mut future = unsafe { core::pin::Pin::new_unchecked(&mut future) };
    let task = Arc::new(Task::new(async move {})); // Dummy task for waker creation context if needed, but we use a real waker

    // Actually, block_on needs its own task loop
    let waker = noop_waker(); // Standard block_on uses a waker that re-runs the loop
    // But since we want to handle Reactor, we need a better block_on.

    // Let's implement a proper local loop for block_on
    let pid = crate::process::get_pid();
    loop {
        // We need a waker that knows about THIS specific block_on call? 
        // Simple: use a waker that signals the PID.

        struct BlockOnWaker(u64);
        impl Wake for BlockOnWaker {
            fn wake(self: Arc<Self>) {
                unsafe { crate::os::syscall(132, 0, self.0, 0); }
            }
        }

        let waker = Waker::from(Arc::new(BlockOnWaker(pid)));
        let mut cx = Context::from_waker(&waker);

        match future.as_mut().poll(&mut cx) {
            Poll::Ready(val) => return val,
            Poll::Pending => {
                // Same Reactor logic as Executor::run
                let mut poll_fds = Vec::new();
                {
                    let reactor = REACTOR.lock();
                    for &fd in reactor.read_waiters.keys() {
                        poll_fds.push(PollFd { fd, events: POLLIN, revents: 0 });
                    }
                    for &fd in reactor.write_waiters.keys() {
                        poll_fds.push(PollFd { fd, events: POLLOUT, revents: 0 });
                    }
                }

                if poll_fds.is_empty() {
                    unsafe { crate::os::syscall(130, 0, pid, 0); }
                } else {
                    let n = crate::os::poll(&mut poll_fds, -1);
                    if n > 0 {
                        let mut reactor = REACTOR.lock();
                        for pfd in poll_fds {
                            if (pfd.revents & POLLIN) != 0 {
                                if let Some(w) = reactor.read_waiters.remove(&pfd.fd) { w.wake(); }
                            }
                            if (pfd.revents & POLLOUT) != 0 {
                                if let Some(w) = reactor.write_waiters.remove(&pfd.fd) { w.wake(); }
                            }
                        }
                    }
                }
            }
        }
    }
}
