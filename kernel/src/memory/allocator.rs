use core::{
    alloc::{GlobalAlloc, Layout},
    sync::atomic::{AtomicBool, Ordering},
    cell::UnsafeCell,
};
use common::allocator::Heap;

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

    fn lock(&self) -> u64 {
        unsafe {
            let rflags: u64;
            core::arch::asm!("pushfq; pop {}", out(reg) rflags);
            core::arch::asm!("cli");
            while self
                .lock
                .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_err()
            {
                core::hint::spin_loop();
            }
            rflags
        }
    }

    fn unlock(&self, rflags: u64) {
        self.lock.store(false, Ordering::Release);
        unsafe {
            if (rflags & 0x200) != 0 {
                core::arch::asm!("sti");
            }
        }
    }
}

unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let flags = self.lock();
        let heap = &mut *self.heap.get();
        let ptr = heap.alloc(layout, |_| None);
        self.unlock(flags);
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let flags = self.lock();
        let heap = &mut *self.heap.get();
        heap.dealloc(ptr, layout);
        self.unlock(flags);
    }
}

#[global_allocator]
static ALLOCATOR: Allocator = Allocator::new();

pub fn init_heap(base: *mut u8, size: usize) {
    let flags = ALLOCATOR.lock();
    unsafe {
        let heap = &mut *ALLOCATOR.heap.get();
        heap.init(base, size);
    }
    ALLOCATOR.unlock(flags);
}