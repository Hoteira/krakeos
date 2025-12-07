use core::sync::atomic::{AtomicBool, Ordering};
use crate::boot::{BOOT_INFO, MemoryMapEntry};
#[allow(unused_imports)]
use crate::debugln;

pub const PAGE_SIZE: u64 = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum FrameError {
    NoMemory,
    IndexOutOfBounds,
}

pub struct BitmapPmm {
    bitmap: *mut u8,
    total_frames: usize,
    used_frames: usize,
    bitmap_size: usize,
    lock: AtomicBool,
}

static mut PMM: BitmapPmm = BitmapPmm {
    bitmap: 0 as *mut u8,
    total_frames: 0,
    used_frames: 0,
    bitmap_size: 0,
    lock: AtomicBool::new(false),
};

pub fn init() {
    unsafe {
        debugln!("[PMM] Starting initialization...");
        
        // Access BOOT_INFO safely
        let mmap = &(*(&raw mut BOOT_INFO)).mmap;
        
        // 1. Calculate total memory size
        let mut max_addr: u64 = 0;
        
        for i in 0..32 {
            let entry = mmap.entries[i];
            if entry.length == 0 { continue; }
            
            debugln!("[PMM] MMap Entry {}: Base={:#x}, Len={:#x}, Type={}", i, entry.base + 0, entry.length + 0, entry.memory_type + 0);

            let end = entry.base + entry.length;
            if end > max_addr {
                max_addr = end;
            }
        }
        
        if max_addr == 0 {
            debugln!("[PMM] Error: No memory found in map.");
            return;
        }

        let total_frames = (max_addr / PAGE_SIZE) as usize;
        let bitmap_size = (total_frames + 7) / 8;

        debugln!("[PMM] Total Frames: {}, Bitmap Size: {} bytes", total_frames, bitmap_size);

        // 2. Find a place for the bitmap
        // We explicitly look for memory ABOVE 10MB (0xA00000) to avoid:
        // - The first 1MB (BIOS/VGA)
        // - The Kernel (loaded somewhere low)
                    // - The Hardcoded Heap (0x300000 - 0x400000)
                    let safe_threshold = 0xA00000;
                                let mut bitmap_addr: u64 = 0;
        let mut found = false;

        for i in 0..32 {
            let entry = mmap.entries[i];
            // Type 1 = Usable
            if entry.memory_type == 1 {
                // Check if this block can hold the bitmap starting above safe_threshold
                let mut candidate_base = entry.base;
                if candidate_base < safe_threshold {
                    // If the block starts below threshold, see if it extends above it
                    if entry.base + entry.length > safe_threshold {
                        candidate_base = safe_threshold;
                    } else {
                        continue; // Block is entirely below threshold
                    }
                }

                // Align to page
                if candidate_base % PAGE_SIZE != 0 {
                    candidate_base = (candidate_base + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
                }
                
                // Check if enough space remains in this block
                let block_end = entry.base + entry.length;
                if candidate_base + (bitmap_size as u64) <= block_end {
                    bitmap_addr = candidate_base;
                    found = true;
                    break;
                }
            }
        }

        if !found {
            panic!("PMM: Could not find safe memory (above 4MB) for bitmap!");
        }
        
        debugln!("[PMM] Bitmap placed at {:#x}", bitmap_addr);

        let pmm_ptr = &raw mut PMM;
        (*pmm_ptr).bitmap = bitmap_addr as *mut u8;
        (*pmm_ptr).total_frames = total_frames;
        (*pmm_ptr).bitmap_size = bitmap_size;
        (*pmm_ptr).used_frames = total_frames; 

        // 3. Clear bitmap (mark all used initially)
        // This write is critical. If bitmap_addr is not mapped, we crash here.
        core::ptr::write_bytes((*pmm_ptr).bitmap, 0xFF, bitmap_size);

        // 4. Iterate mmap and free usable regions
        debugln!("[PMM] Before mmap processing: is_bit_set(0x0/4096) = {}", is_bit_set(0x0));
        debugln!("[PMM] Before mmap processing: is_bit_set(0x4000/4096) = {}", is_bit_set(0x4000 / PAGE_SIZE as usize));
        for i in 0..32 {
            let entry = mmap.entries[i];
            if entry.memory_type == 1 { // Usable
                let start_frame = entry.base / PAGE_SIZE;
                let num_frames = entry.length / PAGE_SIZE;
                
                for f in 0..num_frames {
                    let frame_idx = (start_frame + f) as usize;
                    if frame_idx < total_frames {
                        if is_bit_set(frame_idx) {
                            unset_bit(frame_idx);
                            (*pmm_ptr).used_frames -= 1;
                        }
                    }
                }
            }
        }
        debugln!("[PMM] After mmap processing: is_bit_set(0x4000/4096) = {}", is_bit_set(0x4000 / PAGE_SIZE as usize));

        // 5. Mark the bitmap memory itself as used
        let bitmap_start_frame = bitmap_addr / PAGE_SIZE;
        let bitmap_frames = (bitmap_size as u64 + PAGE_SIZE - 1) / PAGE_SIZE;
        
        for f in 0..bitmap_frames {
            let frame_idx = (bitmap_start_frame + f) as usize;
            if frame_idx < total_frames {
                if !is_bit_set(frame_idx) {
                    set_bit(frame_idx);
                    (*pmm_ptr).used_frames += 1;
                }
            }
        }
        debugln!("[PMM] After bitmap marking: is_bit_set(0x4000/4096) = {}", is_bit_set(0x4000 / PAGE_SIZE as usize));

// 6. Mark 0-4MB as used (Legacy + Kernel + Heap)
        let frames_reserved = safe_threshold / PAGE_SIZE; // 1024 frames (0x0 to 0x3FF000)
        debugln!("[PMM] Marking 0-4MB as used. frames_reserved={}", frames_reserved);
        debugln!("[PMM] Before 0-4MB marking: is_bit_set(0x4000/4096) = {}", is_bit_set(0x4000 / PAGE_SIZE as usize));
        for f in 0..frames_reserved { // f goes from 0 to 1023.
             if f < total_frames as u64 {
                 // Forcefully set the bit.
                 // We need to re-evaluate used_frames increment/decrement.
                 // The safest is to re-calculate used_frames at the end, or only increment if it was actually free.
                 // For now, let's just make sure the bit is SET.
                 if !is_bit_set(f as usize) {
                    set_bit(f as usize);
                    // No used_frames increment here yet, we will re-calculate total later
                 }
             }
        }
        // Recalculate used_frames based on actual set bits after all marking is done.
        (*pmm_ptr).used_frames = 0;
        for i in 0..total_frames {
            if is_bit_set(i) {
                (*pmm_ptr).used_frames += 1;
            }
        }
        debugln!("[PMM] After 0-4MB marking: is_bit_set(0x4000/4096) = {}", is_bit_set(0x4000 / PAGE_SIZE as usize));

        debugln!("[PMM] Initialized. Used: {} KB, Free: {} KB", 
            ((*pmm_ptr).used_frames * 4), 
            (total_frames - (*pmm_ptr).used_frames) * 4
        );    }
}

// Minimum frame index from which user physical memory can be allocated.
// This is based on the safe_threshold used in pmm::init (0xA00000 = 10MB).
const MIN_ALLOC_FRAME_IDX: usize = (0xA00000 / PAGE_SIZE) as usize;

unsafe fn set_bit(idx: usize) {
    let byte_idx = idx / 8;
    let bit_idx = idx % 8;
    let pmm_ptr = &raw mut PMM;
    let ptr = unsafe { (*pmm_ptr).bitmap.add(byte_idx) };
    unsafe { *ptr |= 1 << bit_idx };
}

unsafe fn unset_bit(idx: usize) {
    let byte_idx = idx / 8;
    let bit_idx = idx % 8;
    let pmm_ptr = &raw mut PMM;
    let ptr = unsafe { (*pmm_ptr).bitmap.add(byte_idx) };
    unsafe { *ptr &= !(1 << bit_idx) };
}

unsafe fn is_bit_set(idx: usize) -> bool {
    let byte_idx = idx / 8;
    let bit_idx = idx % 8;
    let pmm_ptr = &raw mut PMM;
    let ptr = unsafe { (*pmm_ptr).bitmap.add(byte_idx) };
    unsafe { (*ptr & (1 << bit_idx)) != 0 }
}

unsafe fn lock_pmm() {
    let pmm_ptr = &raw mut PMM;
    while unsafe { (*pmm_ptr).lock.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() } {
        core::hint::spin_loop();
    }
}

unsafe fn unlock_pmm() {
    let pmm_ptr = &raw mut PMM;
    unsafe { (*pmm_ptr).lock.store(false, Ordering::Release) };
}

pub fn allocate_frame() -> Option<u64> {
    allocate_frames(1)
}

pub fn allocate_frames(count: usize) -> Option<u64> {
    unsafe {
        lock_pmm();
        let pmm_ptr = &raw mut PMM;
        
        if (*pmm_ptr).total_frames < count {
            unlock_pmm();
            return None;
        }

        // Start searching for free frames from MIN_ALLOC_FRAME_IDX
        for i in MIN_ALLOC_FRAME_IDX..((*pmm_ptr).total_frames - count + 1) {
            let mut all_free = true;
            for j in 0..count {
                if is_bit_set(i + j) {
                    all_free = false;
                    break;
                }
            }

            if all_free {
                for j in 0..count {
                    set_bit(i + j);
                }
                (*pmm_ptr).used_frames += count;
                unlock_pmm();
                return Some(i as u64 * PAGE_SIZE);
            }
        }
        unlock_pmm();
        None
    }
}

pub fn reserve_frame(addr: u64) -> bool {
    let frame_idx = (addr / PAGE_SIZE) as usize;
    unsafe {
        lock_pmm();
        let pmm_ptr = &raw mut PMM;
        if frame_idx >= (*pmm_ptr).total_frames {
            unlock_pmm();
            return false;
        }
        
        if is_bit_set(frame_idx) {
            unlock_pmm();
            return false; // Already allocated
        }
        
        set_bit(frame_idx);
        (*pmm_ptr).used_frames += 1;
        unlock_pmm();
        true
    }
}

pub fn free_frame(addr: u64) {
    let frame_idx = (addr / PAGE_SIZE) as usize;
    unsafe {
        lock_pmm();
        let pmm_ptr = &raw mut PMM;
        if frame_idx < (*pmm_ptr).total_frames {
            if is_bit_set(frame_idx) {
                 unset_bit(frame_idx);
                 (*pmm_ptr).used_frames -= 1;
            }
        }
        unlock_pmm();
    }
}

#[allow(dead_code)]
pub fn get_used_memory() -> usize {
    unsafe { (*(&raw mut PMM)).used_frames * PAGE_SIZE as usize }
}

#[allow(dead_code)]
pub fn get_total_memory() -> usize {
    unsafe { (*(&raw mut PMM)).total_frames * PAGE_SIZE as usize }
}
