use core::{
    alloc::{GlobalAlloc, Layout},
    sync::atomic::{AtomicBool, Ordering},
    cell::UnsafeCell,
};
use common::allocator::{Heap, align_up};

pub struct Allocator {
    heap: UnsafeCell<Heap>,
    lock: AtomicBool,
}

unsafe impl Sync for Allocator {}

impl Allocator {
    pub const fn new() -> Self {
        Self {
            heap: UnsafeCell::new(Heap::new()),
            lock: AtomicBool::new(false),
        }
    }

    fn lock(&self) {
        while self
            .lock
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
    }

    fn unlock(&self) {
        self.lock.store(false, Ordering::Release);
    }
}

unsafe fn grow_handler(min_size: usize) -> Option<(usize, usize)> {
    #[cfg(not(feature = "userland"))]
    {
        return None;
    }

    #[cfg(feature = "userland")]
    {
        #[cfg(target_arch = "wasm32")]
        {
            let pages = (min_size + 65535) / 65536;
            let ptr = crate::sys::alloc_pages(min_size);
            if ptr.is_null() { return None; }
            let size = pages * 65536;
            let start = ptr as usize;
            let end = start + size;
            Some((start, end))
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut size = 4096 * 1024; // Default 4MB chunks
            if min_size > size {
                size = min_size.next_power_of_two();
            }

            // Get current break
            let current_brk = crate::os::brk(0);
            if current_brk == 0 {
                return None;
            }

            // Align requested size to page boundary (4096)
            let new_brk_req = align_up(current_brk + size, 4096);
            
            // Request extension
            let new_brk = crate::os::brk(new_brk_req);

            if new_brk < new_brk_req {
                return None;
            }

            Some((current_brk, new_brk))
        }
    }
}

unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.lock();
        let heap = &mut *self.heap.get();
        let ptr = heap.alloc(layout, |min| grow_handler(min));
        self.unlock();
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.lock();
        let heap = &mut *self.heap.get();
        heap.dealloc(ptr, layout);
        self.unlock();
    }
}

#[global_allocator]
pub static ALLOCATOR: Allocator = Allocator::new();

pub fn init_heap(base: *mut u8, size: usize) {
    ALLOCATOR.lock();
    unsafe {
        let heap = &mut *ALLOCATOR.heap.get();
        heap.init(base, size);
    }
    ALLOCATOR.unlock();
}