use core::sync::atomic::{AtomicU64, Ordering};

pub fn allocate_shm(size: u64) -> u64 {
    static NEXT_SHM_ADDR: AtomicU64 = AtomicU64::new(0x0000_0040_0000_0000);
    let aligned_size = (size + 0x1FFFFF) & !0x1FFFFF;
    let addr = NEXT_SHM_ADDR.fetch_add(aligned_size, Ordering::Relaxed);
    addr
}
