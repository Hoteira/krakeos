#[cfg(not(target_arch = "wasm32"))]
use crate::os::syscall;
use core::cell::UnsafeCell;
use alloc::alloc::{alloc, dealloc, Layout};
use alloc::boxed::Box;

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
    let stack_size = 512 * 1024; // 512KB stack
    let stack_layout = Layout::from_size_align(stack_size, 16).unwrap();
    let stack = unsafe { alloc(stack_layout) };
    let stack_ptr = unsafe { (stack.add(stack_size) as usize) - 8 };

    let packet = Box::new(Packet {
        result: UnsafeCell::new(None),
    });
    let packet_ptr = Box::into_raw(packet);

    let args = Box::new(ThreadArgs {
        f,
        packet: packet_ptr,
    });
    let args_ptr = Box::into_raw(args);

    #[cfg(not(target_arch = "wasm32"))]
    let tid = unsafe {
        syscall(112, thread_start::<F, T> as usize as u64, stack_ptr as u64, args_ptr as u64)
    } as usize;
    #[cfg(target_arch = "wasm32")]
    let tid = 0; // Threads not supported in pure WASM yet

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
        #[cfg(not(target_arch = "wasm32"))]
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
            #[cfg(not(target_arch = "wasm32"))]
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
