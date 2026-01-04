use core::{
    alloc::{GlobalAlloc, Layout},
    ptr::NonNull,
    sync::atomic::{AtomicBool, AtomicPtr, Ordering},
};
use core::ptr::write_bytes;
use super::malloc;

const MAGIC_USED: u32 = 0xDEAD_BEEF;

#[repr(C, align(8))]
pub struct Free {
    next: *mut Free,
    size: usize,
}

#[repr(C, align(8))]
struct Used {
    magic: u32,
    size: usize,
}

impl Free {
    fn start(&self) -> *mut u8 {
        unsafe { (self as *const Free).add(1) as *mut u8 }
    }

    fn end(&self) -> *mut u8 {
        unsafe { self.start().add(self.size) }
    }

    fn set_end(&mut self, end: *mut u8) {

        self.size = unsafe { end.offset_from(self.start()) as usize };
    }
}



impl Used {
    #[allow(dead_code)]
    fn start(&self) -> *mut u8 {
        unsafe { (self as *const Used).add(1) as *mut u8 }
    }

    #[allow(dead_code)]
    fn total_size(&self) -> usize {
        size_of::<Used>() + self.size
    }
}

#[derive(Copy, Clone)]
struct Region {
    start: usize,
    end: usize,
}

const MAX_REGIONS: usize = 64;
static mut HEAP_REGIONS: [Region; MAX_REGIONS] = [Region { start: 0, end: 0 }; MAX_REGIONS];
static mut REGION_COUNT: usize = 0;

const BIN_COUNT: usize = 8;
const MIN_BLOCK_SIZE: usize = 32;
const MAX_BIN_SIZE: usize = 4096;

pub struct Allocator {
    first_free: AtomicPtr<Free>,
    bins: [AtomicPtr<Free>; BIN_COUNT],
    lock: AtomicBool,
}

impl Allocator {
    pub const fn new() -> Self {
        Self {
            first_free: AtomicPtr::new(core::ptr::null_mut()),
            bins: [
                AtomicPtr::new(core::ptr::null_mut()),
                AtomicPtr::new(core::ptr::null_mut()),
                AtomicPtr::new(core::ptr::null_mut()),
                AtomicPtr::new(core::ptr::null_mut()),
                AtomicPtr::new(core::ptr::null_mut()),
                AtomicPtr::new(core::ptr::null_mut()),
                AtomicPtr::new(core::ptr::null_mut()),
                AtomicPtr::new(core::ptr::null_mut()),
            ],
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

    unsafe fn grow_heap(&self, required_size: usize) -> bool {
        let mut total_size = 0;
        for i in 0..REGION_COUNT {
            total_size += HEAP_REGIONS[i].end - HEAP_REGIONS[i].start;
        }
        
        let mut new_size = total_size; 
        if new_size < required_size {
            new_size = required_size.next_power_of_two();
        }
        // Minimal allocation
        if new_size < 4096 { new_size = 4096; }

        crate::println!("\x1B[93m[Allocator] Growing heap by {} bytes...\x1B[0m", new_size);

        let ptr = malloc(new_size);
        if ptr == 0 { return false; } // OOM

        // Add to regions
        if REGION_COUNT >= MAX_REGIONS { return false; } // Too many regions
        
        // Zero the new memory (malloc syscall usually does this, but to be safe/consistent)
        write_bytes(ptr as *mut u8, 0, new_size);

        HEAP_REGIONS[REGION_COUNT] = Region { start: ptr, end: ptr + new_size };
        REGION_COUNT += 1;

        // Construct Free header
        // We assume malloc returns aligned pointer (page aligned)
        let region_align = align_of::<Free>().max(align_of::<Used>()).max(8);
        let ptr_usize = ptr as usize;
        let aligned_ptr_usize = align_up(ptr_usize, region_align);
        let adjustment = aligned_ptr_usize - ptr_usize;
        
        if adjustment + size_of::<Free>() >= new_size {
             // Should not happen with reasonable sizes
             return false;
        }

        let new_free_ptr = aligned_ptr_usize as *mut Free;
        (*new_free_ptr).size = new_size - adjustment - size_of::<Free>();
        (*new_free_ptr).next = core::ptr::null_mut();

        // Insert into sorted free list
        let mut prev: *mut Free = core::ptr::null_mut();
        let mut current = self.first_free.load(Ordering::Acquire);
        
        while !current.is_null() && (current as usize) < (new_free_ptr as usize) {
            prev = current;
            current = (*current).next;
        }
        
        (*new_free_ptr).next = current;
        if prev.is_null() {
            self.first_free.store(new_free_ptr, Ordering::Release);
        } else {
            (*prev).next = new_free_ptr;
        }
        
        true
    }
}

fn get_bin_index(total_size: usize) -> Option<usize> {
    if total_size < MIN_BLOCK_SIZE || total_size > MAX_BIN_SIZE {
        return None;
    }
    if (total_size & (total_size - 1)) != 0 {
        return None;
    }

    let idx = (usize::BITS - total_size.leading_zeros()) as usize - 1;
    if idx < 5 { return None; }
    Some(idx - 5)
}

#[inline(always)]
fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}

#[inline(always)]
fn align_down(addr: usize, align: usize) -> usize {
    addr & !(align - 1)
}

pub fn init(base: *mut u8, size: usize) {
    assert_eq!(size_of::<Used>(), size_of::<Free>());

    unsafe { write_bytes(base, 0, size); }

    let region_align = align_of::<Free>()
        .max(align_of::<Used>())
        .max(8);

    let base_usize = base as usize;
    let aligned_base_usize = align_up(base_usize, region_align);
    let adjustment = aligned_base_usize - base_usize;

    if adjustment >= size {
        panic!("Heap region too small after alignment");
    }

    let heap_start_ptr = aligned_base_usize as *mut u8;
    
    // Register the first region
    unsafe {
        if REGION_COUNT < MAX_REGIONS {
            HEAP_REGIONS[REGION_COUNT] = Region {
                start: aligned_base_usize,
                end: base_usize + size,
            };
            REGION_COUNT += 1;
        }
    }

    let seg = heap_start_ptr as *mut Free;
    unsafe {
        (*seg).size = size - adjustment - size_of::<Free>();
        (*seg).next = core::ptr::null_mut();
    }
    ALLOCATOR.first_free.store(seg, Ordering::SeqCst);
}

fn get_used_header(ptr: *mut u8) -> *mut Used {
    (ptr as usize - size_of::<Used>()) as *mut Used
}

fn in_heap_bounds(ptr: *const u8) -> bool {
    let p = ptr as usize;
    unsafe {
        for i in 0..REGION_COUNT {
            if p >= HEAP_REGIONS[i].start && p < HEAP_REGIONS[i].end {
                return true;
            }
        }
    }
    false
}

fn find_header_for_allocation(seg: &Free, layout: &Layout) -> Option<*mut u8> {
    let seg_start = seg.start() as usize;
    let seg_end = seg.end() as usize;

    if layout.size() == 0 {
        return Some(NonNull::<u8>::dangling().as_ptr());
    }

    let req_align = layout.align();
    if req_align == 0 || !req_align.is_power_of_two() {
        return None;
    }

    let header_size = size_of::<Used>();
    let payload_size = layout.size();
    let total_needed = header_size + payload_size;

    if seg_end < seg_start || seg_end - seg_start < total_needed {
        return None;
    }

    let min_payload_addr = seg_start + header_size;

    let payload_candidate = align_up(min_payload_addr, req_align);

    if payload_candidate + payload_size <= seg_end {
        let used_align = align_of::<Used>();
        let mut p = payload_candidate;
        while p + payload_size <= seg_end {
            let h = p - header_size;
            if h >= seg_start && h % used_align == 0 {
                return Some(p as *mut u8);
            }
            p += req_align;
        }
    }

    None
}

unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if layout.size() == 0 {
            return NonNull::<u8>::dangling().as_ptr();
        }

        if layout.align() == 0 || !layout.align().is_power_of_two() {
            return core::ptr::null_mut();
        }

        self.lock();

        let needed_total = size_of::<Used>() + layout.size();
        let aligned_total = if needed_total < MIN_BLOCK_SIZE {
            MIN_BLOCK_SIZE
        } else {
            needed_total.next_power_of_two()
        };

        if layout.align() <= 16 && aligned_total <= MAX_BIN_SIZE {
            if let Some(idx) = get_bin_index(aligned_total) {
                let bin_head = self.bins[idx].load(Ordering::Relaxed);
                if !bin_head.is_null() {
                    let next = (*bin_head).next;
                    self.bins[idx].store(next, Ordering::Relaxed);

                    let used = bin_head as *mut Used;
                    (*used).magic = MAGIC_USED;
                    (*used).size = aligned_total - size_of::<Used>();

                    self.unlock();
                    return (used as *mut u8).add(size_of::<Used>());
                }
            }
        }

        let alloc_layout = if layout.align() <= 16 && aligned_total <= MAX_BIN_SIZE {
            Layout::from_size_align(aligned_total - size_of::<Used>(), layout.align()).unwrap()
        } else {
            layout
        };

        loop {
            let mut prev_ptr: *mut Free = core::ptr::null_mut();
            let mut cur_ptr = self.first_free.load(Ordering::Acquire);

            while !cur_ptr.is_null() {
                let cur = &mut *cur_ptr;
                if let Some(payload_ptr) = find_header_for_allocation(cur, &alloc_layout) {
                    let header_ptr = get_used_header(payload_ptr);
                    let old_end = cur.end();


                    cur.set_end(header_ptr as *mut u8);

                    (*header_ptr).magic = MAGIC_USED;
                    (*header_ptr).size = alloc_layout.size();

                    let allocated_end = payload_ptr.add(alloc_layout.size());
                    let allocated_end_addr = allocated_end as usize;
                    let aligned_end_addr = (allocated_end_addr + align_of::<Free>() - 1) & !(align_of::<Free>() - 1);
                    let allocated_end = aligned_end_addr as *mut u8;

                    if (allocated_end as usize) < old_end as usize {
                        let remaining = old_end as usize - allocated_end as usize;
                        if remaining >= size_of::<Free>() {
                            let new_free = allocated_end as *mut Free;
                            (*new_free).size = remaining - size_of::<Free>();
                            (*new_free).next = cur.next;
                            cur.next = new_free;
                        }
                    }

                    if cur.size < size_of::<Free>() {
                        if prev_ptr.is_null() {
                            self.first_free.store(cur.next, Ordering::Release);
                        } else {
                            (*prev_ptr).next = cur.next;
                        }
                    }

                    self.unlock();
                    return payload_ptr;
                }

                prev_ptr = cur_ptr;
                cur_ptr = (*cur_ptr).next;
            }
            
            // If allocation failed, try to grow
            if !self.grow_heap(needed_total) {
                break; // OOM
            }
        }

        self.unlock();
        core::ptr::null_mut()
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        if ptr.is_null() {
            return;
        }

        self.lock();

        if !in_heap_bounds(ptr as *const u8) {
            self.unlock();
            panic!("dealloc: pointer outside heap bounds");
        }

        let hdr = get_used_header(ptr);

        if !in_heap_bounds(hdr as *const u8) || (hdr as usize) % align_of::<Used>() != 0  {
            self.unlock();
            panic!("dealloc: invalid header location");
        }

        unsafe {
            if (*hdr).magic != MAGIC_USED {
                self.unlock();
                panic!("dealloc: magic mismatch (double free or corruption?)");
            }

            (*hdr).magic = 0;

            let total_size = (*hdr).size + size_of::<Used>();
            let free_block = hdr as *mut Free;
            (*free_block).size = (*hdr).size;

            if let Some(idx) = get_bin_index(total_size) {
                let current_head = self.bins[idx].load(Ordering::Relaxed);
                (*free_block).next = current_head;
                self.bins[idx].store(free_block, Ordering::Relaxed);
                self.unlock();
                return;
            }

            (*free_block).next = core::ptr::null_mut();

            let mut prev: *mut Free = core::ptr::null_mut();
            let mut current = self.first_free.load(Ordering::Acquire);

            while !current.is_null() && current < free_block {
                prev = current;
                current = (*current).next;
            }

            (*free_block).next = current;
            if prev.is_null() {
                self.first_free.store(free_block, Ordering::Release);
            } else {
                (*prev).next = free_block;
            }

            if !(*free_block).next.is_null() {
                let next_block = (*free_block).next;
                let free_end = (*free_block).end();
                if free_end == next_block as *mut u8 {
                    (*free_block).size += (*next_block).size + size_of::<Free>();
                    (*free_block).next = (*next_block).next;
                }
            }

            if !prev.is_null() {
                let prev_end = (*prev).end();
                if prev_end == free_block as *mut u8 {
                    (*prev).size += (*free_block).size + size_of::<Free>();
                    (*prev).next = (*free_block).next;
                }
            }
        }

        self.unlock();
    }
}

#[global_allocator]
static ALLOCATOR: Allocator = Allocator::new();

pub fn init_heap(base: *mut u8, size: usize) {
    init(base, size);
}