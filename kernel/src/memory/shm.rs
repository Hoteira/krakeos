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
        if let Some(seg) = self.segments.get(name) {
            let h = seg.frames[0];
            crate::debugln!("[SHM] Found existing segment '{}' with handle {:#x}", name, h);
            if h == 1 {
                panic!("SHM BUG: Found segment with handle 1!");
            }
            return Ok(h); 
        }

        let page_count = (size + 4095) / 4096;
        let mut frames = Vec::with_capacity(page_count as usize);
        for _ in 0..page_count {
            let phys = pmm::allocate_frame(0).ok_or("Out of memory for SHM")?;
            frames.push(phys);
        }

        let handle = frames[0];
        crate::debugln!("[SHM] Created new segment '{}' at {:#x}", name, handle);
        if handle == 1 {
            panic!("SHM BUG: Created segment with handle 1!");
        }
        
        self.segments.insert(String::from(name), ShmSegment {
            frames,
            size,
        });

        Ok(handle)
    }

    pub fn get(&self, name: &str) -> Option<ShmSegment> {
        self.segments.get(name).cloned()
    }
}
