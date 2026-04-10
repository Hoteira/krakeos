use super::address::PhysAddr;
use crate::boot::BOOT_INFO;
use crate::debugln;
use crate::sync::Mutex;

use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

pub struct LockFreeMagazine {
    head: AtomicU64,
    pub count: AtomicUsize,
}

impl LockFreeMagazine {
    pub const fn new() -> Self {
        Self {
            head: AtomicU64::new(0),
            count: AtomicUsize::new(0),
        }
    }

    pub fn push(&self, phys: u64) {
        let node_ptr = (phys + crate::memory::paging::HHDM_OFFSET) as *mut u64;
        let mut head_val = self.head.load(Ordering::Acquire);
        loop {
            unsafe { *node_ptr = head_val; }
            let tag = (head_val >> 48).wrapping_add(1);
            let new_head = (tag << 48) | (phys & 0x0000FFFFFFFFF000);
            match self.head.compare_exchange_weak(head_val, new_head, Ordering::Release, Ordering::Relaxed) {
                Ok(_) => break,
                Err(v) => head_val = v,
            }
        }
        self.count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn pop(&self) -> Option<u64> {
        let mut head_val = self.head.load(Ordering::Acquire);
        loop {
            let phys = head_val & 0x0000FFFFFFFFF000;
            if phys == 0 { return None; }

            let node_ptr = (phys + crate::memory::paging::HHDM_OFFSET) as *const u64;
            let next_val = unsafe { *node_ptr };

            match self.head.compare_exchange_weak(head_val, next_val, Ordering::Release, Ordering::Relaxed) {
                Ok(_) => {
                    self.count.fetch_sub(1, Ordering::Relaxed);
                    return Some(phys);
                }
                Err(v) => head_val = v,
            }
        }
    }
}

pub static PER_CPU_MAGAZINES: [LockFreeMagazine; 64] = [const { LockFreeMagazine::new() }; 64];
static mut PAGE_MAP: *mut PageDescriptor = core::ptr::null_mut();
static mut PAGE_MAP_ENTRIES: usize = 0;


pub const PAGE_SIZE: u64 = 4096;
pub const MAX_ORDER: usize = 18; // Max block size: 2^18 pages = 1GB

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameError {
    #[allow(dead_code)]
    NoMemory,
    #[allow(dead_code)]
    IndexOutOfBounds,
}

/// Metadata for every physical page in the system.
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct PageDescriptor {
        order: u8,     // Buddy order (if head of a block)
    flags: u8,     // Bit 0: Is Allocated, Bit 1: Is Head
}

/// A node in the free list, stored at the beginning of free pages.
#[repr(C)]
struct FreeBlockNode {
    next: *mut FreeBlockNode,
    prev: *mut FreeBlockNode,
}

pub struct BuddyAllocator {
    // Free lists for each order (0 to MAX_ORDER)
    free_lists: [*mut FreeBlockNode; MAX_ORDER + 1],
    // Pointer to the global page metadata map
    page_map: *mut PageDescriptor,
    page_map_entries: usize,
    
    total_pages: usize,
    used_pages: usize,
}

unsafe impl Send for BuddyAllocator {}
unsafe impl Sync for BuddyAllocator {}

// Mutex allows safe internal mutability and handles the lock bit for us.
static PMM: Mutex<BuddyAllocator> = Mutex::new(BuddyAllocator {
    free_lists: [core::ptr::null_mut(); MAX_ORDER + 1],
    page_map: core::ptr::null_mut(),
    page_map_entries: 0,
    total_pages: 0,
    used_pages: 0,
});

pub fn init() {
    let mut allocator = PMM.lock();
    unsafe {
        let mmap = (*(&raw mut BOOT_INFO)).mmap;
        let mut max_addr: u64 = 0;

        for i in 0..32 {
            let entry = mmap.entries[i];
            if entry.memory_type == 1 && entry.length > 0 {
                let end = entry.base + entry.length;
                if end > max_addr { max_addr = end; }
            }
        }

        let total_pages = (max_addr / PAGE_SIZE) as usize;
        let page_map_size = total_pages * core::mem::size_of::<PageDescriptor>();
        let page_map_pages = (page_map_size + PAGE_SIZE as usize - 1) / PAGE_SIZE as usize;

        debugln!("PMM: Detected {}MB RAM. Metadata needs {}KB.", max_addr / 1024 / 1024, page_map_size / 1024);

        // 2. Find a contiguous block for the Page Map
        let mut map_phys: u64 = 0;
        for i in 0..32 {
            let entry = mmap.entries[i];
            let base = entry.base;
            let length = entry.length;
            if entry.memory_type == 1 && length >= page_map_size as u64 {
                let candidate = base.max(0x1000000); // Start at 16MB
                if candidate + page_map_size as u64 <= base + length && candidate < 0x80000000 {
                    map_phys = candidate;
                    debugln!("PMM: Selected map_phys={:#x} from entry base={:#x} len={:#x}", map_phys, base, length);
                    break;
                }
            }
        }

        if map_phys == 0 {
            panic!("PMM: Could not find memory for Page Map in lower 2GB!");
        }

        let map_virt = (map_phys + crate::memory::paging::HHDM_OFFSET) as *mut PageDescriptor;
        debugln!("PMM: Page Map Virtual Address: {:p}. Zeroing...", map_virt);
        
        let num_descriptors = page_map_size / core::mem::size_of::<PageDescriptor>();
        for i in 0..num_descriptors {
            unsafe {
                core::ptr::write_bytes(map_virt.add(i) as *mut u8, 0, core::mem::size_of::<PageDescriptor>());
            }
        }
        debugln!("PMM: Page Map zeroed successfully ({} descriptors).", num_descriptors);

        allocator.page_map = map_virt;
        
        allocator.page_map_entries = total_pages;
        PAGE_MAP = allocator.page_map;
        PAGE_MAP_ENTRIES = allocator.page_map_entries;

        allocator.total_pages = total_pages;

        debugln!("PMM: Starting to add free regions...");
        for i in 0..32 {
            let entry = mmap.entries[i];
            let base = entry.base;
            let length = entry.length;
            
            if entry.memory_type == 1 && length > 0 {
                let mut start = base;
                let end = base + length;
                debugln!("PMM: Processing mmap entry {}: {:#x} -> {:#x}", i, start, end);

                // PROTECT KERNEL: Skip everything below 16MB (0x1000000)
                if start < 0x1000000 {
                    start = 0x1000000;
                }

                if start >= end { 
                    debugln!("PMM: Skipping entry {} (fully below 16MB or invalid)", i);
                    continue; 
                }

                // Skip the region used by the Page Map itself
                if start <= map_phys && end > map_phys {
                    let map_end = map_phys + (page_map_pages as u64 * PAGE_SIZE);
                    debugln!("PMM: Entry overlaps Page Map. Splitting. Map is {:#x} -> {:#x}", map_phys, map_end);
                    
                    if map_phys > start {
                        debugln!("PMM: Adding region before map: {:#x} -> {:#x}", start, map_phys);
                        add_free_region(&mut allocator, start, map_phys);
                    }
                    
                    start = map_end;
                    debugln!("PMM: Remaining region after map: {:#x} -> {:#x}", start, end);
                }

                if end > start {
                    // CAP TO 3GB: Reserve 0xC0000000 - 0xFFFFFFFF for PCI MMIO hole.
                    // This prevents collisions between RAM and BAR allocations.
                    let safe_end = end.min(0xC0000000); 
                    if safe_end > start {
                        debugln!("PMM: Adding free region: {:#x} -> {:#x}", start, safe_end);
                        add_free_region(&mut allocator, start, safe_end);
                    } else {
                        debugln!("PMM: Region {:#x} -> {:#x} is in PCI hole, skipping.", start, end);
                    }
                }
            }
        }
        
        debugln!("PMM: Buddy Allocator initialized. Free: {}MB", (allocator.total_pages - allocator.used_pages) * PAGE_SIZE as usize / 1024 / 1024);
    }
}

/// Discovers all physical RAM entries, including those above 4GB.
/// This must only be called AFTER vmm::init() has mapped the entire RAM range.
pub fn discover_all_memory() {
    let mut allocator = PMM.lock();
    unsafe {
        let mmap = (*(&raw mut BOOT_INFO)).mmap;
        let map_phys = allocator.page_map as u64 - crate::memory::paging::HHDM_OFFSET;
        let map_pages = (allocator.page_map_entries * core::mem::size_of::<PageDescriptor>() + PAGE_SIZE as usize - 1) / PAGE_SIZE as usize;

        debugln!("PMM: Expanding discovery to all available RAM...");

        for i in 0..32 {
            let entry = mmap.entries[i];
            let base = entry.base;
            let length = entry.length;
            
            if entry.memory_type == 1 && length > 0 {
                let mut start = base;
                let end = base + length;

                // PROTECT KERNEL
                if start < 0x1000000 { start = 0x1000000; }
                if start >= end { continue; }

                // We only care about memory ABOVE 4GB now, or memory we partially skipped
                // Memory below 4GB was already added in init().
                // However, adding it again is safe because Buddy push_free_block 
                // will just update metadata. But to be clean, let's only add new stuff.
                if end <= 0x100000000 && start >= 0x1000000 {
                    // This was likely already handled, but let's check if it overlaps map
                    continue;
                }

                if start < 0x100000000 && end > 0x100000000 {
                    start = 0x100000000; // Only add the new part
                } else if start < 0x100000000 {
                    continue; // fully below 4GB, skip
                }

                // Skip the region used by the Page Map itself (unlikely above 4GB but possible)
                if start <= map_phys && end > map_phys {
                    let map_end = map_phys + (map_pages as u64 * PAGE_SIZE);
                    if map_phys > start {
                        add_free_region(&mut allocator, start, map_phys);
                    }
                    start = map_end;
                }

                if end > start {
                    debugln!("PMM: Adding extended free region: {:#x} -> {:#x}", start, end);
                    add_free_region(&mut allocator, start, end);
                }
            }
        }
        debugln!("PMM: Expansion complete. Free: {}MB", (allocator.total_pages - allocator.used_pages) * PAGE_SIZE as usize / 1024 / 1024);
    }
}

unsafe fn add_free_region(alloc: &mut BuddyAllocator, start: u64, end: u64) {
    let mut current = (start + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
    let aligned_end = end & !(PAGE_SIZE - 1);

    while current < aligned_end {
        let mut order = MAX_ORDER;
        while order > 0 {
            let size = (1u64 << order) * PAGE_SIZE;
            if current + size <= aligned_end && (current % size) == 0 {
                break;
            }
            order -= 1;
        }

        push_free_block(alloc, current, order);
        current += (1u64 << order) * PAGE_SIZE;
    }
}

unsafe fn push_free_block(alloc: &mut BuddyAllocator, phys: u64, order: usize) {
    let virt = (phys + crate::memory::paging::HHDM_OFFSET) as *mut FreeBlockNode;
    
    let node = &mut *virt;
    node.next = alloc.free_lists[order];
    node.prev = core::ptr::null_mut();
    
    if !node.next.is_null() {
        (*node.next).prev = virt;
    }
    alloc.free_lists[order] = virt;

    let page_idx = (phys / PAGE_SIZE) as usize;
    let desc = &mut (*alloc.page_map.add(page_idx));
    desc.order = order as u8;
    desc.flags = 0x02; // Is Head, Not Allocated
}

unsafe fn pop_free_block(alloc: &mut BuddyAllocator, order: usize) -> Option<u64> {
    let head = alloc.free_lists[order];
    if head.is_null() { return None; }

    alloc.free_lists[order] = (*head).next;
    if !alloc.free_lists[order].is_null() {
        (*alloc.free_lists[order]).prev = core::ptr::null_mut();
    }

    let virt = head as u64;
    let phys = virt - crate::memory::paging::HHDM_OFFSET;
    
    let page_idx = (phys / PAGE_SIZE) as usize;
    (*alloc.page_map.add(page_idx)).flags = 0x03; // Head + Allocated
    
    Some(phys)
}

unsafe fn remove_free_block(alloc: &mut BuddyAllocator, phys: u64, order: usize) {
    let virt = (phys + crate::memory::paging::HHDM_OFFSET) as *mut FreeBlockNode;
    let node = &mut *virt;

    if !node.prev.is_null() {
        (*node.prev).next = node.next;
    } else {
        alloc.free_lists[order] = node.next;
    }

    if !node.next.is_null() {
        (*node.next).prev = node.prev;
    }
}

pub fn allocate_frames(count: usize) -> Option<u64> {
    if count == 0 { return None; }
    
    // --- MAGAZINE FAST PATH (interrupts disabled to prevent re-entrancy) ---
    if count == 1 {
        let cpu_id = crate::task::cpu::get_cpu_id() as usize;
        if cpu_id < 64 {
            let flags: u64;
            unsafe { core::arch::asm!("pushfq; pop {}; cli", out(reg) flags); }
            let result = PER_CPU_MAGAZINES[cpu_id].pop();
            unsafe { core::arch::asm!("push {}; popfq", in(reg) flags); }
            if let Some(phys) = result {
                unsafe {
                    core::ptr::write_bytes((phys + crate::memory::paging::HHDM_OFFSET) as *mut u8, 0, PAGE_SIZE as usize);
                }
                return Some(phys);
            }
        }
    }
    // ---------------------------
    
    let mut order = 0;
    while (1 << order) < count {
        order += 1;
    }

    if order > MAX_ORDER { return None; }

    let mut alloc = PMM.int_lock();
    
    let mut current_order = order;
    while current_order <= MAX_ORDER && alloc.free_lists[current_order].is_null() {
        current_order += 1;
    }

    if current_order > MAX_ORDER {
        return None;
    }

    unsafe {
        let mut phys = pop_free_block(&mut alloc, current_order).unwrap();

        while current_order > order {
            current_order -= 1;
            let buddy_phys = phys + (1u64 << current_order) * PAGE_SIZE;
            push_free_block(&mut alloc, buddy_phys, current_order);
            
            let head_desc = &mut (*alloc.page_map.add((phys / PAGE_SIZE) as usize));
            head_desc.order = current_order as u8;
        }

        let page_idx = (phys / PAGE_SIZE) as usize;
        let num_pages = 1 << order;
        for i in 0..num_pages {
            let desc = &mut (*alloc.page_map.add(page_idx + i));
                        desc.flags |= 0x01; // Allocated
        }
        
        alloc.used_pages += num_pages;
        
        let virt_ptr = (phys + crate::memory::paging::HHDM_OFFSET) as *mut u8;
        core::ptr::write_bytes(virt_ptr, 0, num_pages * PAGE_SIZE as usize);

        Some(phys)
    }
}

pub fn free_frame(addr: u64) {
    if addr % PAGE_SIZE != 0 { return; }
    
    // --- MAGAZINE FAST PATH (interrupts disabled to prevent re-entrancy) ---
    let page_idx = (addr / PAGE_SIZE) as usize;
    unsafe {
        if page_idx < PAGE_MAP_ENTRIES {
            let desc = &*PAGE_MAP.add(page_idx);
            if (desc.flags & 0x01) != 0 && desc.order == 0 {
                let cpu_id = crate::task::cpu::get_cpu_id() as usize;
                if cpu_id < 64 && PER_CPU_MAGAZINES[cpu_id].count.load(Ordering::Relaxed) < 4096 {
                    let flags: u64;
                    core::arch::asm!("pushfq; pop {}; cli", out(reg) flags);
                    PER_CPU_MAGAZINES[cpu_id].push(addr);
                    core::arch::asm!("push {}; popfq", in(reg) flags);
                    return;
                }
            }
        }
    }
    // ---------------------------
    
    let mut alloc = PMM.int_lock();
    unsafe {
        let mut current_phys = addr;
        let mut page_idx = (addr / PAGE_SIZE) as usize;
        
        if page_idx >= alloc.page_map_entries {
            return;
        }

        let desc = &mut (*alloc.page_map.add(page_idx));
        if (desc.flags & 0x01) == 0 {
            return;
        }

        let mut order = desc.order as usize;
        alloc.used_pages -= 1 << order;

        while order < MAX_ORDER {
            let buddy_phys = current_phys ^ ((1u64 << order) * PAGE_SIZE);
            let buddy_idx = (buddy_phys / PAGE_SIZE) as usize;
            
            if buddy_idx >= alloc.page_map_entries { break; }
            
            let buddy_desc = &mut (*alloc.page_map.add(buddy_idx));
            
            if (buddy_desc.flags & 0x01) == 0 && (buddy_desc.flags & 0x02) != 0 && buddy_desc.order as usize == order {
                remove_free_block(&mut alloc, buddy_phys, order);
                
                buddy_desc.flags = 0;
                buddy_desc.order = 0;
                
                if buddy_phys < current_phys {
                    let old_head = &mut (*alloc.page_map.add(page_idx));
                    old_head.flags = 0;
                    old_head.order = 0;
                    current_phys = buddy_phys;
                    page_idx = buddy_idx;
                } else {
                    let old_buddy_head = &mut (*alloc.page_map.add(buddy_idx));
                    old_buddy_head.flags = 0;
                    old_buddy_head.order = 0;
                }
                
                order += 1;
            } else {
                break;
            }
        }

        push_free_block(&mut alloc, current_phys, order);
    }
}



pub fn reserve_frame(_addr: u64) -> bool {
    true 
}

pub fn get_used_memory() -> usize {
    let base_used = PMM.lock().used_pages * PAGE_SIZE as usize;
    let cpu_id = crate::task::cpu::get_cpu_id() as usize;
    let cached = if cpu_id < 64 { PER_CPU_MAGAZINES[cpu_id].count.load(Ordering::Relaxed) * PAGE_SIZE as usize } else { 0 };
    base_used.saturating_sub(cached)
}

pub fn get_total_memory() -> usize {
    PMM.lock().total_pages * PAGE_SIZE as usize
}

pub fn get_free_memory() -> usize {
    let alloc = PMM.lock();
    let base_free = (alloc.total_pages - alloc.used_pages) * PAGE_SIZE as usize;
    let cpu_id = crate::task::cpu::get_cpu_id() as usize;
    let cached = if cpu_id < 64 { PER_CPU_MAGAZINES[cpu_id].count.load(Ordering::Relaxed) * PAGE_SIZE as usize } else { 0 };
    base_free + cached
}



#[unsafe(no_mangle)]
pub extern "C" fn pmm_allocate_frames(count: usize, owner: u64) -> u64 {
    allocate_frames(count).unwrap_or(0)
}

pub fn allocate_frame() -> Option<u64> {
    allocate_frames(1)
}

pub fn allocate_memory(bytes: usize) -> Option<u64> {
    let pages = (bytes + PAGE_SIZE as usize - 1) / PAGE_SIZE as usize;
    allocate_frames(pages)
}

pub fn allocate_aligned_memory(bytes: usize, alignment: usize) -> Option<u64> {
    let mut pages = (bytes + PAGE_SIZE as usize - 1) / PAGE_SIZE as usize;
    let align_pages = alignment / PAGE_SIZE as usize;
    if align_pages > pages { pages = align_pages; }
    allocate_frames(pages)
}
