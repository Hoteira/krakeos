use core::alloc::Layout;
use core::ptr::{self, write_bytes, NonNull};

pub const BIN_COUNT: usize = 32;
pub const MIN_BLOCK_SIZE: usize = 32;
const MAX_HEAP_REGIONS: usize = 64;
const MAGIC_USED: u32 = 0xDEAD_BEEF;
const FLAG_FREE: usize = 1;

#[repr(C, align(8))]
pub struct Free {
    pub header: usize, // size | FLAG_FREE
    pub next: *mut Free,
    pub prev: *mut Free,
}

#[repr(C, align(8))]
struct Used {
    pub header: usize, // size
    pub magic: u32,
    pub padding: u32,
}

#[repr(C, align(8))]
struct Footer {
    pub header: usize,
}

impl Free {
    #[inline(always)]
    fn size(&self) -> usize {
        self.header & !FLAG_FREE
    }

    #[inline(always)]
    fn footer(&self) -> *mut Footer {
        unsafe {
            (self as *const Free as *mut u8).add(self.size() - 8) as *mut Footer
        }
    }

    #[inline(always)]
    fn set_free(&mut self, size: usize) {
        self.header = size | FLAG_FREE;
        unsafe {
            (*self.footer()).header = self.header;
        }
    }
}

impl Used {
    #[inline(always)]
    fn size(&self) -> usize {
        self.header & !FLAG_FREE
    }

    #[inline(always)]
    fn footer(&self) -> *mut Footer {
        unsafe {
            (self as *const Used as *mut u8).add(self.size() - 8) as *mut Footer
        }
    }

    #[inline(always)]
    fn set_used(&mut self, size: usize) {
        self.header = size & !FLAG_FREE;
        unsafe {
            (*self.footer()).header = self.header;
        }
    }
}

#[derive(Copy, Clone)]
struct HeapRegion {
    start: usize,
    end: usize,
}

pub struct Heap {
    bins: [*mut Free; BIN_COUNT],
    bin_mask: u32,
    regions: [HeapRegion; MAX_HEAP_REGIONS],
    region_count: usize,
}

unsafe impl Send for Heap {}

impl Heap {
    pub const fn new() -> Self {
        Self {
            bins: [ptr::null_mut(); BIN_COUNT],
            bin_mask: 0,
            regions: [HeapRegion { start: 0, end: 0 }; MAX_HEAP_REGIONS],
            region_count: 0,
        }
    }

    #[inline]
    fn is_in_region(&self, ptr: *const u8) -> bool {
        if ptr.is_null() { return false; }
        let p = ptr as usize;
        for i in 0..self.region_count {
            if p >= self.regions[i].start && p < self.regions[i].end {
                return true;
            }
        }
        false
    }

    fn is_in_same_region(&self, a: *const u8, b: *const u8) -> bool {
        let addr_a = a as usize;
        let addr_b = b as usize;
        for i in 0..self.region_count {
            let r = &self.regions[i];
            if addr_a >= r.start && addr_a < r.end && addr_b >= r.start && addr_b < r.end {
                return true;
            }
        }
        false
    }

    unsafe fn list_remove(&mut self, idx: usize, block: *mut Free) {
        let prev = (*block).prev;
        let next = (*block).next;

        if !prev.is_null() {
            (*prev).next = next;
        } else {
            self.bins[idx] = next;
            if next.is_null() {
                self.bin_mask &= !(1 << idx);
            }
        }

        if !next.is_null() {
            (*next).prev = prev;
        }
    }

    unsafe fn list_push(&mut self, idx: usize, block: *mut Free) {
        let head = self.bins[idx];

        (*block).next = head;
        (*block).prev = ptr::null_mut();

        if !head.is_null() {
            (*head).prev = block;
        }

        self.bins[idx] = block;
        self.bin_mask |= 1 << idx;
    }

    pub fn init(&mut self, base: *mut u8, size: usize) {
        let base_usize = base as usize;
        let aligned_base = align_up(base_usize, 8);
        let adjustment = aligned_base - base_usize;

        if adjustment >= size { return; }
        let block_size = size - adjustment;
        if block_size < MIN_BLOCK_SIZE { return; }

        self.regions[0] = HeapRegion { start: aligned_base, end: base_usize + size };
        self.region_count = 1;

        let seg = aligned_base as *mut Free;
        unsafe {
            (*seg).set_free(block_size);
            self.list_push(get_bin_index(block_size), seg);
        }
    }

    pub unsafe fn add_memory(&mut self, start: usize, end: usize) {
        let size = end - start;
        if size < MIN_BLOCK_SIZE { return; }

        if self.region_count > 0 && self.regions[self.region_count - 1].end == start {
            self.regions[self.region_count - 1].end = end;
        } else {
            if self.region_count >= MAX_HEAP_REGIONS { return; }
            self.regions[self.region_count] = HeapRegion { start, end };
            self.region_count += 1;
        }

        let seg = start as *mut Free;
        (*seg).set_free(size);
        self.coalesce_and_push(seg);
    }

    pub fn in_heap_bounds(&self, ptr: *const u8) -> bool {
        self.is_in_region(ptr)
    }

    pub unsafe fn alloc(&mut self, layout: Layout, mut grow_fn: impl FnMut(usize) -> Option<(usize, usize)>) -> *mut u8 {
        if layout.size() == 0 {
            return NonNull::<u8>::dangling().as_ptr();
        }

        let new_size = align_up(layout.size(), 8);
        let needed_total = 16 + new_size + 8; // overhead + payload + footer
        let needed_total = if needed_total < MIN_BLOCK_SIZE { MIN_BLOCK_SIZE } else { needed_total };

        loop {
            let start_idx = get_bin_index(needed_total);

            // Search bitmask for first non-empty bin >= start_idx
            let mask = self.bin_mask & !((1 << start_idx) - 1);
            if mask != 0 {
                let mut idx = mask.trailing_zeros() as usize;
                while idx < BIN_COUNT {
                    if (self.bin_mask & (1 << idx)) != 0 {
                        let mut cur = self.bins[idx];
                        while !cur.is_null() {
                            if (*cur).size() >= needed_total {
                                self.list_remove(idx, cur);
                                return self.prep_return(cur, needed_total);
                            }
                            cur = (*cur).next;
                        }
                    }
                    idx += 1;
                }
            }

            match grow_fn(needed_total) {
                Some((start, end)) => {
                    self.add_memory(start, end);
                }
                None => break,
            }
        }

        ptr::null_mut()
    }

    unsafe fn prep_return(&mut self, block: *mut Free, needed: usize) -> *mut u8 {
        let total_available = (*block).size();
        if total_available >= needed + MIN_BLOCK_SIZE {
            let used = block as *mut Used;
            (*used).set_used(needed);
            (*used).magic = MAGIC_USED;

            let payload_ptr = (used as *mut u8).add(16);
            write_bytes(payload_ptr, 0, needed - 16 - 8);

            let remaining = (block as *mut u8).add(needed) as *mut Free;
            (*remaining).set_free(total_available - needed);
            self.list_push(get_bin_index(total_available - needed), remaining);

            payload_ptr
        } else {
            let used = block as *mut Used;
            (*used).set_used(total_available);
            (*used).magic = MAGIC_USED;

            let payload_ptr = (used as *mut u8).add(16);
            write_bytes(payload_ptr, 0, total_available - 16 - 8);

            payload_ptr
        }
    }

    pub unsafe fn dealloc(&mut self, ptr: *mut u8, _layout: Layout) {
        if ptr.is_null() { return; }
        let used = (ptr as *mut u8).offset(-16) as *mut Used;
        if !self.is_in_region(used as *const u8) { return; }
        if (*used).magic != MAGIC_USED { return; }

        (*used).magic = 0;
        self.coalesce_and_push(used as *mut Free);
    }

    unsafe fn coalesce_and_push(&mut self, mut block: *mut Free) {
        let mut size = (*block).size();

        // Coalesce Next
        let next_hdr_ptr = (block as *mut u8).add(size) as *mut usize;
        if self.is_in_region(next_hdr_ptr as *const u8) {
            let header_val = *next_hdr_ptr;
            if (header_val & FLAG_FREE) != 0 {
                let next_free = next_hdr_ptr as *mut Free;
                let next_size = header_val & !FLAG_FREE;
                self.list_remove(get_bin_index(next_size), next_free);
                size += next_size;
            }
        }

        // Coalesce Prev
        let prev_ftr_ptr = (block as *mut u8).offset(-8) as *mut usize;
        if self.is_in_region(prev_ftr_ptr as *const u8) {
            let footer_val = *prev_ftr_ptr;
            if (footer_val & FLAG_FREE) != 0 {
                let prev_size = footer_val & !FLAG_FREE;
                let prev_free = (block as *mut u8).offset(-(prev_size as isize)) as *mut Free;
                if self.is_in_same_region(block as *const u8, prev_free as *const u8) {
                    self.list_remove(get_bin_index(prev_size), prev_free);
                    size += prev_size;
                    block = prev_free;
                }
            }
        }

        (*block).set_free(size);
        self.list_push(get_bin_index(size), block);
    }
}

/// Segregated Bin Mapping:
/// Bins 0-11: Linear (8 bytes) -> 32, 40, ..., 120
/// Bins 12-31: Logarithmic -> 128, 256, ..., 2^26
pub fn get_bin_index(size: usize) -> usize {
    if size < 128 {
        ((size.saturating_sub(MIN_BLOCK_SIZE)) / 8).min(11)
    } else {
        let log = (usize::BITS - size.leading_zeros()) as usize - 1; // Log2
        (12 + (log.saturating_sub(7))).min(31)
    }
}

#[inline(always)]
pub fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}