pub mod heap;
use self::heap::{align_up, Heap, MAX_HEAP_REGIONS};
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
        let mut rflags: u64 = 0;
        #[cfg(not(target_arch = "wasm32"))]
        unsafe {
            core::arch::asm!("pushfq; pop {}", out(reg) rflags);
            core::arch::asm!("cli");
        }

        #[cfg(target_arch = "wasm32")]
        {
            let heap = unsafe { &*self.heap.get() };
            if heap.magic != heap::HEAP_MAGIC {
                self.lock.store(false, Ordering::Release);
            }
        }

        while self
            .lock
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            #[cfg(target_arch = "wasm32")]
            {
                // In single-threaded WASM, if we are spinning, it's a re-entrant deadlock (e.g. from debug_print).
                // We force-unlock to break the deadlock.
                self.lock.store(false, Ordering::Release);
                continue;
            }
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
        // Kernel mode: allocate physical frames from PMM and use HHDM mapping
        let pages = (min_size + 4095) / 4096;
        unsafe extern "C" {
            fn pmm_allocate_frames(count: usize, owner: u64) -> u64;
        }
        let phys = unsafe { pmm_allocate_frames(pages, 0) };
        if phys == 0 || phys == u64::MAX {
            return None;
        }
        const HHDM_OFFSET: usize = 0xFFFF_8000_0000_0000;
        let start = phys as usize + HHDM_OFFSET;
        let end = start + pages * 4096;
        return Some((start, end));
    }

    // Userland: grow using sys::alloc_pages (brk/mmap)
    #[cfg(feature = "userland")]
    {
        // Grow in 1MB chunks to reduce syscall overhead
        let growth_size = if min_size < 1024 * 1024 { 1024 * 1024 } else { min_size };
        let ptr = crate::sys::alloc_pages(growth_size);
        if ptr.is_null() { return None; }

        let start = ptr as usize;
        let actual_size = if cfg!(target_arch = "wasm32") {
            (growth_size + 65535) & !65535
        } else {
            (growth_size + 4095) & !4095
        };

        Some((start, start + actual_size))
    }
}

unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let flags = self.lock();
        let heap = &mut *self.heap.get();
        let mut ptr = heap.alloc(layout, |_| None); // Don't grow inside lock
        self.unlock(flags);

        if ptr.is_null() {
            // Request enough for layout + overhead (16 byte header + 8 byte footer + alignment)
            if let Some((start, end)) = grow_handler(layout.size() + 64) {
                let flags = self.lock();
                let heap = &mut *self.heap.get();
                heap.add_memory(start, end);
                ptr = heap.alloc(layout, |_| None);
                self.unlock(flags);
            }
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let flags = self.lock();
        let heap = &mut *self.heap.get();
        heap.dealloc(ptr, layout);
        self.unlock(flags);
    }
}

#[cfg(feature = "global_allocator")]
#[cfg_attr(not(test), global_allocator)]
pub static ALLOCATOR: Allocator = Allocator::new();

#[cfg(not(feature = "global_allocator"))]
pub static ALLOCATOR: Allocator = Allocator::new();

pub fn init_heap(base: *mut u8, size: usize) {
    let flags = ALLOCATOR.lock();
    unsafe {
        let heap = &mut *ALLOCATOR.heap.get();
        heap.init(base, size);
    }
    ALLOCATOR.unlock(flags);
}

pub fn debug_allocator() {
    // Stubs to prevent deadlock-prone debug prints
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

