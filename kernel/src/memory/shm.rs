use crate::memory::address::PhysAddr;
use crate::memory::{address_space, pmm, vmm};
use crate::sync::Mutex;
use alloc::collections::BTreeMap;
use alloc::string::String;

#[derive(Debug, Clone)]
pub struct ShmSegment {
    pub addr: u64,
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
        if let Some(seg) = self.segments.get(name) {
            return Ok(seg.addr);
        }

        // Create new
        let addr = address_space::allocate_shm(size);
        crate::memory::vma::GLOBAL_VMA.lock().track(addr, size, 0); // PID 0 for global SHM

        // Commiting memory immediately for SHM (easier for now)
        let page_count = (size + 4095) / 4096;
        for i in 0..page_count {
            let phys = pmm::allocate_frame(0).ok_or("Out of memory for SHM")?;
            vmm::map_page(addr + (i * 4096), PhysAddr::new(phys), 0x7, None); // User + Writable + Present
        }

        self.segments.insert(String::from(name), ShmSegment {
            addr,
            size,
        });

        crate::debugln!("SHM: Created '{}' at {:#x} ({} bytes)", name, addr, size);
        Ok(addr)
    }

    pub fn get(&self, name: &str) -> Option<u64> {
        self.segments.get(name).map(|s| s.addr)
    }
}