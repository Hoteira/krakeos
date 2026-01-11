use crate::os::syscall;
use core::cell::UnsafeCell;
use rust_alloc::alloc::{alloc, dealloc, Layout};
use rust_alloc::boxed::Box;

pub struct JoinHandle<T> {
    id: usize,
    stack: *mut u8,
    stack_layout: Layout,
    packet: *mut Packet<T>,
}

unsafe impl<T: Send> Send for JoinHandle<T> {}
unsafe impl<T: Send> Sync for JoinHandle<T> {}

struct Packet<T> {
    result: UnsafeCell<Option<T>>,
}

struct ThreadArgs<F, T> {
    f: F,
    packet: *mut Packet<T>,
}

pub fn spawn<F, T>(f: F) -> JoinHandle<T>
where
    F: FnOnce() -> T,
    F: Send + 'static,
    T: Send + 'static,
{
    let stack_size = 4096 * 4; // 16KB stack
    let stack_layout = Layout::from_size_align(stack_size, 16).unwrap();
    let stack = unsafe { alloc(stack_layout) };
    let stack_ptr = unsafe { stack.add(stack_size) as usize };

    let packet = Box::new(Packet {
        result: UnsafeCell::new(None),
    });
    let packet_ptr = Box::into_raw(packet);

    let args = Box::new(ThreadArgs {
        f,
        packet: packet_ptr,
    });
    let args_ptr = Box::into_raw(args);

    let tid = unsafe {
        syscall(112, thread_start::<F, T> as usize as u64, stack_ptr as u64, args_ptr as u64)
    } as usize;

    JoinHandle {
        id: tid,
        stack,
        stack_layout,
        packet: packet_ptr,
    }
}

extern "C" fn thread_start<F, T>(args_ptr: *mut ThreadArgs<F, T>)
where
    F: FnOnce() -> T,
    F: Send + 'static,
    T: Send + 'static,
{
    unsafe {
        let args = Box::from_raw(args_ptr);
        let res = (args.f)();

        // Write result to packet
        *(*args.packet).result.get() = Some(res);

        // Exit thread
        crate::os::syscall(113, 0, 0, 0);
    }
}

impl<T> JoinHandle<T> {
    pub fn join(self) -> Result<T, ()> {
        let id = self.id;
        let stack = self.stack;
        let layout = self.stack_layout;
        let packet_ptr = self.packet;

        // Ensure we don't run Drop which would (if it did anything) be bad
        core::mem::forget(self);

        unsafe {
            // Wait for thread to exit
            loop {
                let res = crate::os::syscall(61, id as u64, 0, 0); // SYS_WAIT4
                if res != u64::MAX {
                    break;
                }
                crate::os::yield_task();
            }

            // Read result
            let packet = Box::from_raw(packet_ptr);
            let res = (*packet.result.get()).take().ok_or(())?;

            // Now it's safe to deallocate the stack
            dealloc(stack, layout);

            Ok(res)
        }
    }

    pub fn thread_id(&self) -> usize {
        self.id
    }

    pub fn detach(self) {
        core::mem::forget(self);
    }
}

pub use crate::time::sleep;

pub fn yield_now() {
    crate::os::yield_task();
}
