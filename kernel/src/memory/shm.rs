use crate::memory::address::PhysAddr;
use crate::memory::{pmm, vmm, paging};
use crate::sync::Mutex;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

#[derive(Debug, Clone)]
pub struct ShmSegment {
    pub frames: Vec<u64>,
    pub size: u64,
}

pub struct ShmManager {
    segments: BTreeMap<String, ShmSegment>,
}

pub static GLOBAL_SHM: Mutex<ShmManager> = Mutex::new(ShmManager {
    segments: BTreeMap::new(),
});

impl ShmManager {
    pub fn get_or_create(&mut self, name: &str, size: u64) -> Result<u64, String> {
        if let Some(_) = self.segments.get(name) {
            // Return a pseudo-handle (just a hash or incrementing ID)
            // For now, let's use the index in the map as a handle, or just return 0 on success
            // Actually, the syscall expect a u64 that can be passed to shm_map.
            // We can use the address of the segment in our map if it was stable, but it's not.
            // Let's just return 1 for now as a "success" handle if name exists.
            return Ok(1); 
        }

        let page_count = (size + 4095) / 4096;
        let mut frames = Vec::with_capacity(page_count as usize);
        for _ in 0..page_count {
            let phys = pmm::allocate_frame(0).ok_or("Out of memory for SHM")?;
            frames.push(phys);
        }

        self.segments.insert(String::from(name), ShmSegment {
            frames,
            size,
        });

        Ok(1)
    }

    pub fn get(&self, name: &str) -> Option<ShmSegment> {
        self.segments.get(name).cloned()
    }
}
