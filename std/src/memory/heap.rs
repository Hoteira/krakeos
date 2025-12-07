use core::{
    alloc::{GlobalAlloc, Layout},
    ptr::NonNull,
    sync::atomic::{AtomicBool, AtomicPtr, Ordering},
};

const MAGIC_USED: u32 = 0xDEAD_BEEF;

#[repr(C, align(8))]
pub struct Free {
    next: *mut Free,
    size: usize,  // Size of free space (excluding header)
}

#[repr(C, align(8))]
struct Used {
    magic: u32,
    size: usize,  // Size of payload (excluding header)
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

static HEAP_START: AtomicPtr<u8> = AtomicPtr::new(core::ptr::null_mut());
static HEAP_END: AtomicPtr<u8> = AtomicPtr::new(core::ptr::null_mut());

pub struct Allocator {
    first_free: AtomicPtr<Free>,
    lock: AtomicBool,
}

impl Allocator {
    pub const fn new() -> Self {
        Self {
            first_free: AtomicPtr::new(core::ptr::null_mut()),
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

    unsafe { base.write_bytes(0xCC, size) };

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
    let heap_end_ptr = unsafe { base.add(size) };
    HEAP_START.store(heap_start_ptr, Ordering::SeqCst);
    HEAP_END.store(heap_end_ptr, Ordering::SeqCst);

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
    let start = HEAP_START.load(Ordering::SeqCst) as usize;
    let end = HEAP_END.load(Ordering::SeqCst) as usize;
    let p = ptr as usize;
    p >= start && p < end
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

    let max_payload = seg_end - payload_size;
    let min_payload = seg_start + header_size;

    if max_payload < min_payload {
        return None;
    }

    let mut payload_candidate = align_down(max_payload, req_align);

    while payload_candidate >= min_payload {
        let header_addr = payload_candidate - header_size;
        if header_addr % align_of::<Used>() == 0 {
            if payload_candidate + payload_size <= seg_end {
                return Some(payload_candidate as *mut u8);
            }
        }

        if payload_candidate < req_align {
            break;
        }
        payload_candidate -= req_align;
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
        let mut prev_ptr: *mut Free = core::ptr::null_mut();
        let mut cur_ptr = self.first_free.load(Ordering::Acquire);

        unsafe {
            while !cur_ptr.is_null() {
                let cur = &mut *cur_ptr;
                if let Some(payload_ptr) = find_header_for_allocation(cur, &layout) {
                    let header_ptr = get_used_header(payload_ptr);
                    let old_end = cur.end();

                    
                    cur.set_end(header_ptr as *mut u8);

                    (*header_ptr).magic = MAGIC_USED;
                    (*header_ptr).size = layout.size();
                    
                    let allocated_end = payload_ptr.add(layout.size());
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

            let free_block = hdr as *mut Free;
            (*free_block).size = (*hdr).size;
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