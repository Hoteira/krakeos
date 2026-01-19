use crate::sync::Mutex;
use alloc::vec::Vec;

#[derive(Debug, Clone, Copy)]
pub struct VmaRegion {
    pub start: u64,
    pub size: u64,
    pub pid: u64,
}

pub struct VmaAllocator {
    regions: Vec<VmaRegion>,
}

impl VmaAllocator {
    pub const fn new() -> Self {
        Self { regions: Vec::new() }
    }

    pub fn track(&mut self, start: u64, size: u64, pid: u64) {
        // Simple overlap check
        let end = start + size;
        for r in &self.regions {
            let r_end = r.start + r.size;
            if (start >= r.start && start < r_end) || (end > r.start && end <= r_end) {
                crate::debugln!("VMA WARNING: Overlap detected! New [PID {} {:#x}-{:#x}], Existing [PID {} {:#x}-{:#x}]", 
                    pid, start, end, r.pid, r.start, r_end);
            }
        }
        self.regions.push(VmaRegion { start, size, pid });
    }

    pub fn is_mapped(&self, addr: u64) -> bool {
        for r in &self.regions {
            if addr >= r.start && addr < r.start + r.size {
                return true;
            }
        }
        false
    }

    pub fn dump(&self) {
        crate::debugln!("--- VMA Dump ---");
        for r in &self.regions {
            crate::debugln!("PID {}: {:#x} - {:#x} ({} MB)", r.pid, r.start, r.start + r.size, r.size / 1024 / 1024);
        }
        crate::debugln!("----------------");
    }
}

pub static GLOBAL_VMA: Mutex<VmaAllocator> = Mutex::new(VmaAllocator::new());
