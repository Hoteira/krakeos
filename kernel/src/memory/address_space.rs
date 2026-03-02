use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

pub const MAX_SLOTS: u32         = 1024;
pub const CODE_REGION_BASE: u64  = 0x0000_0001_0000_0000; // 4 GiB
pub const CODE_SLOT_SIZE: u64    = 1 * 1024 * 1024 * 1024; // 1 GiB per slot
pub const SHM_REGION_BASE: u64   = 0x0000_0040_0000_0000; // 256 GiB
pub const HEAP_REGION_BASE: u64  = 0x0000_0080_0000_0000; // 512 GiB
pub const HEAP_SLOT_SIZE: u64    = 4 * 1024 * 1024 * 1024; // 4 GiB per slot
pub const STACK_REGION_TOP: u64  = 0x0000_7FFF_FFFF_0000; // ~128 TiB
pub const STACK_SLOT_SIZE: u64   = 16 * 1024 * 1024;       // 16 MiB per slot

static NEXT_SLOT_ID: AtomicU32 = AtomicU32::new(0);

pub fn get_next_slot_id() -> u32 {
    let id = NEXT_SLOT_ID.fetch_add(1, Ordering::SeqCst);
    if id >= MAX_SLOTS {
        panic!("SAS: Out of process slots!");
    }
    id
}

pub fn allocate_code(_size: u64, _pid: u64, slot_id: u32) -> u64 {
    CODE_REGION_BASE + (slot_id as u64) * CODE_SLOT_SIZE
}

pub fn allocate_heap(_size: u64, _pid: u64, slot_id: u32) -> u64 {
    HEAP_REGION_BASE + (slot_id as u64) * HEAP_SLOT_SIZE
}

pub fn allocate_stack(_size: u64, _pid: u64, slot_id: u32) -> u64 {
    STACK_REGION_TOP - (slot_id as u64) * STACK_SLOT_SIZE
}

pub fn allocate_shm(size: u64) -> u64 {
    static NEXT_SHM_ADDR: AtomicU64 = AtomicU64::new(SHM_REGION_BASE);
    let aligned_size = (size + 0x1FFFFF) & !0x1FFFFF;
    NEXT_SHM_ADDR.fetch_add(aligned_size, Ordering::SeqCst)
}
