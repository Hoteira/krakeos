use crate::sync::Mutex;

pub const MAX_SLOTS: u16 = 4096;

// --- AOT code ---
pub const CODE_REGION_BASE: u64  = 0x0000_0001_0000_0000; // 4 GiB
pub const CODE_SLOT_SIZE: u64    = 64 * 1024 * 1024; // 64 MiB
// Ends at: 260 GiB (0x41_0000_0000)

// --- User stack ---
pub const STACK_REGION_BASE: u64 = 0x0000_0041_0000_0000; // 260 GiB
pub const STACK_SLOT_SIZE: u64   = 16 * 1024 * 1024; // 16 MiB
// Ends at: 324 GiB (0x51_0000_0000)

// --- Kernel stack ---
pub const KERNEL_STACK_REGION_BASE: u64 = 0x0000_0051_0000_0000; // 324 GiB
pub const KERNEL_STACK_SLOT_SIZE: u64   = 128 * 1024; // 128 KiB
// 128 KiB × 4096 slots = 512 MiB
// Ends at: 324.5 GiB (0x51_2000_0000)

// --- Linear memory ---
pub const LINEAR_MEMORY_BASE: u64      = 0x0000_0051_2000_0000; // 324.5 GiB
pub const LINEAR_MEMORY_SLOT_SIZE: u64 = 31 * 1024 * 1024 * 1024; // 31 GiB
// Ends at: ~124 TiB (fits in 128 TiB canonical)

static SLOT_BITMAP: Mutex<[u64; 64]> = Mutex::new([u64::MAX; 64]);

pub fn allocate_slot() -> Option<u16> {
    let mut bitmap = SLOT_BITMAP.lock();
    for i in 0..64 {
        if bitmap[i] != 0 {
            let bit = bitmap[i].trailing_zeros() as u16;
            bitmap[i] &= !(1 << bit);
            return Some(i as u16 * 64 + bit);
        }
    }
    None
}

pub fn free_slot(id: u16) {
    let mut bitmap = SLOT_BITMAP.lock();
    let idx = (id / 64) as usize;
    let bit = id % 64;
    bitmap[idx] |= 1 << bit;
}

pub fn allocate_code(pid: u64, slot_id: u16) -> u64 {
    let addr = CODE_REGION_BASE + (slot_id as u64) * CODE_SLOT_SIZE;
    crate::memory::vma::GLOBAL_VMA.lock().track(addr, CODE_SLOT_SIZE, pid);
    addr
}

pub fn allocate_linear_memory(pid: u64, slot_id: u16) -> u64 {
    let addr = LINEAR_MEMORY_BASE + (slot_id as u64) * LINEAR_MEMORY_SLOT_SIZE;
    crate::memory::vma::GLOBAL_VMA.lock().track(addr, LINEAR_MEMORY_SLOT_SIZE, pid);
    addr
}

pub fn allocate_stack(pid: u64, slot_id: u16) -> u64 {
    let base = STACK_REGION_BASE + (slot_id as u64) * STACK_SLOT_SIZE;
    let top = base + STACK_SLOT_SIZE;
    crate::memory::vma::GLOBAL_VMA.lock().track(base, STACK_SLOT_SIZE, pid);
    top
}
