pub mod heap;
use self::heap::{align_up, Heap};
use core::{
    alloc::{GlobalAlloc, Layout},
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, Ordering},
};

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
        let rflags: u64;
        #[cfg(not(feature = "userland"))]
        unsafe {
            core::arch::asm!("pushfq; pop {}", out(reg) rflags);
            core::arch::asm!("cli");
        }
        #[cfg(feature = "userland")]
        {
            rflags = 0;
        }

        while self
            .lock
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
        rflags
    }

    fn unlock(&self, rflags: u64) {
        self.lock.store(false, Ordering::Release);
        #[cfg(not(feature = "userland"))]
        unsafe {
            if (rflags & 0x200) != 0 {
                core::arch::asm!("sti");
            }
        }
    }

    pub fn init(&self, base: *mut u8, size: usize) {
        let flags = self.lock();
        unsafe {
            let heap = &mut *self.heap.get();
            heap.init(base, size);
        }
        self.unlock(flags);
    }
}

unsafe fn grow_handler(min_size: usize) -> Option<(usize, usize)> {
    #[cfg(not(feature = "userland"))]
    {
        return None;
    }

    // Unified growth using sys::alloc_pages
    let ptr = crate::sys::alloc_pages(min_size);
    if ptr.is_null() { return None; }
    
    let start = ptr as usize;
    // We assume alloc_pages returns exactly the size requested (or aligned up)
    // To be safe, we calculate the end based on min_size aligned to page boundaries
    let actual_size = if cfg!(target_arch = "wasm32") {
        (min_size + 65535) & !65535
    } else {
        (min_size + 4095) & !4095
    };
    
    Some((start, start + actual_size))
}

unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let flags = self.lock();
        let heap = &mut *self.heap.get();
        let ptr = heap.alloc(layout, |min| grow_handler(min));
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

#[cfg(feature = "userland")]
#[cfg_attr(not(test), global_allocator)]
pub static ALLOCATOR: Allocator = Allocator::new();

#[cfg(not(feature = "userland"))]
pub static ALLOCATOR: Allocator = Allocator::new();

pub fn init_heap(base: *mut u8, size: usize) {
    let flags = ALLOCATOR.lock();
    unsafe {
        let heap = &mut *ALLOCATOR.heap.get();
        heap.init(base, size);
    }
    ALLOCATOR.unlock(flags);
}

#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cabi_realloc(ptr: *mut u8, old_size: usize, align: usize, new_size: usize) -> *mut u8 {
    use core::alloc::Layout;
    let align = if align == 0 { 1 } else { align };
    
    if ptr.is_null() {
        if new_size == 0 { return align as *mut u8; }
        let layout = Layout::from_size_align(new_size, align).unwrap();
        crate::alloc::alloc::alloc(layout)
    } else {
        if new_size == 0 {
            let layout = Layout::from_size_align(old_size, align).unwrap();
            crate::alloc::alloc::dealloc(ptr, layout);
            return core::ptr::null_mut();
        }
        let layout = Layout::from_size_align(old_size, align).unwrap();
        let new_ptr = crate::alloc::alloc::realloc(ptr, layout, new_size);
        new_ptr
    }
}
