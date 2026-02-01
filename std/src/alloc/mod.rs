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
        crate::rust_alloc::alloc::alloc(layout)
    } else {
        if new_size == 0 {
            let layout = Layout::from_size_align(old_size, align).unwrap();
            crate::rust_alloc::alloc::dealloc(ptr, layout);
            return core::ptr::null_mut();
        }
        let layout = Layout::from_size_align(old_size, align).unwrap();
        let new_ptr = crate::rust_alloc::alloc::realloc(ptr, layout, new_size);
        new_ptr
    }
}