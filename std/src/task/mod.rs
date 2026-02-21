use crate::rust_alloc::boxed::Box;
use crate::rust_alloc::collections::VecDeque;
use crate::rust_alloc::sync::Arc;
use crate::sync::Mutex;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

pub type BoxFuture = Pin<Box<dyn Future<Output=()> + Send + 'static>>;

pub struct Task {
    future: Mutex<BoxFuture>,
    pid: u64,
}

impl Task {
    pub fn new(future: impl Future<Output=()> + Send + 'static) -> Self {
        Task {
            future: Mutex::new(Box::pin(future)),
            pid: crate::process::get_pid(),
        }
    }

    pub fn poll(&self, context: &mut Context) -> Poll<()> {
        self.future.lock().as_mut().poll(context)
    }
}

pub static RUN_QUEUE: Mutex<VecDeque<Arc<Task>>> = Mutex::new(VecDeque::new());

pub fn spawn(future: impl Future<Output=()> + Send + 'static) {
    let task = Arc::new(Task::new(future));
    RUN_QUEUE.lock().push_back(task);
}

// Waker implementation
fn task_waker_raw(task: *const Task) -> RawWaker {
    RawWaker::new(task as *const (), &VTABLE)
}

fn task_waker_clone(data: *const ()) -> RawWaker {
    unsafe {
        Arc::increment_strong_count(data as *const Task);
    }
    task_waker_raw(data as *const Task)
}

fn task_waker_wake(data: *const ()) {
    let task = unsafe { Arc::from_raw(data as *const Task) };
    let pid = task.pid;
    RUN_QUEUE.lock().push_back(task);
    // Wake up the thread by signaling a generic event with its PID
    #[cfg(not(target_arch = "wasm32"))]
    unsafe { crate::os::syscall(132, 0, pid, 0); }
    #[cfg(target_arch = "wasm32")]
    { /* stub */ }
}

fn task_waker_wake_by_ref(data: *const ()) {
    let task = unsafe { Arc::from_raw(data as *const Task) };
    let task_clone = task.clone();
    let pid = task.pid;
    core::mem::forget(task); // Don't drop the original
    RUN_QUEUE.lock().push_back(task_clone);
    #[cfg(not(target_arch = "wasm32"))]
    unsafe { crate::os::syscall(132, 0, pid, 0); }
    #[cfg(target_arch = "wasm32")]
    { /* stub */ }
}

fn task_waker_drop(data: *const ()) {
    unsafe {
        Arc::decrement_strong_count(data as *const Task);
    }
}

static VTABLE: RawWakerVTable = RawWakerVTable::new(
    task_waker_clone,
    task_waker_wake,
    task_waker_wake_by_ref,
    task_waker_drop,
);

pub fn waker_ref(task: &Arc<Task>) -> Waker {
    let ptr = Arc::as_ptr(task);
    unsafe {
        Arc::increment_strong_count(ptr);
        Waker::from_raw(task_waker_raw(ptr))
    }
}

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