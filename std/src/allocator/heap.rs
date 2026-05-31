use core::alloc::Layout;
use core::ptr::{self, NonNull};
use talc::{Span, Talc, ErrOnOom};

pub const MAX_HEAP_REGIONS: usize = 1024;
pub const HEAP_MAGIC: u64 = 0x5452_41_4B_45_4F_53; // "KRAKEOS"

#[derive(Copy, Clone)]
struct HeapRegion {
    start: usize,
    end: usize,
}

pub struct Heap {
    pub magic: u64,
    talc: Talc<ErrOnOom>,
    regions: [HeapRegion; MAX_HEAP_REGIONS],
    pub region_count: usize,
}

unsafe impl Send for Heap {}

impl Heap {
    pub const fn new() -> Self {
        Self {
            magic: 0,
            talc: Talc::new(ErrOnOom),
            regions: [HeapRegion { start: 0, end: 0 }; MAX_HEAP_REGIONS],
            region_count: 0,
        }
    }

    pub fn reset(&mut self) {
        self.magic = HEAP_MAGIC;
        self.talc = Talc::new(ErrOnOom);
        self.region_count = 0;
    }

    pub fn init(&mut self, base: *mut u8, size: usize) {
        self.reset();
        let base_usize = base as usize;
        let aligned_base = align_up(base_usize, 16);
        let adjustment = aligned_base - base_usize;

        if adjustment >= size { return; }
        let block_size = (size - adjustment) & !15;
        if block_size < 128 { return; } // Talc needs some minimum space

        self.regions[0] = HeapRegion { start: aligned_base, end: aligned_base + block_size };
        self.region_count = 1;

        unsafe {
            let span = Span::from_base_size(aligned_base as *mut u8, block_size);
            self.talc.claim(span).expect("Talc: Failed to claim initial span");
        }
    }

    pub unsafe fn add_memory(&mut self, start: usize, end: usize) {
        let aligned_start = align_up(start, 16);
        if aligned_start >= end { return; }
        let size = (end - aligned_start) & !15;
        if size < 128 { return; }

        if self.magic != HEAP_MAGIC {
            self.reset();
        }

        if self.region_count >= MAX_HEAP_REGIONS { return; }
        self.regions[self.region_count] = HeapRegion { start: aligned_start, end: aligned_start + size };
        self.region_count += 1;

        let span = Span::from_base_size(aligned_start as *mut u8, size);
        self.talc.claim(span).expect("Talc: Failed to claim new span");
    }

    pub fn in_heap_bounds(&self, ptr: *const u8) -> bool {
        let p = ptr as usize;
        for i in 0..self.region_count {
            if p >= self.regions[i].start && p < self.regions[i].end {
                return true;
            }
        }
        false
    }

    pub unsafe fn alloc(&mut self, layout: Layout, mut _grow_fn: impl FnMut(usize) -> Option<(usize, usize)>) -> *mut u8 {
        if layout.size() == 0 {
            return NonNull::<u8>::dangling().as_ptr();
        }

        if self.magic != HEAP_MAGIC {
            self.reset();
        }

        match self.talc.malloc(layout) {
            Ok(ptr) => ptr.as_ptr(),
            Err(_) => ptr::null_mut(),
        }
    }

    pub unsafe fn dealloc(&mut self, ptr: *mut u8, layout: Layout) {
        if ptr.is_null() { return; }
        if self.magic != HEAP_MAGIC { return; }

        // Safety: check if in bounds
        if !self.in_heap_bounds(ptr) { return; }

        self.talc.free(NonNull::new_unchecked(ptr), layout);
    }
}

#[inline(always)]
pub fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}
