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
        crate::debugln!("\n[SAS DUMP]");
        
        let mut seen_pids = Vec::new();
        let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
        
        // Use a local copy of regions to avoid long lock hold if necessary, 
        // though regions is likely small enough.
        
        for task_opt in tm.get_tasks() {
            if let Some(task) = task_opt {
                if let Some(proc) = &task.process {
                    let pid = proc.pid;
                    if seen_pids.contains(&pid) { continue; }
                    seen_pids.push(pid);
                    
                    let slot = proc.slot_id;
                    
                    // Filter regions for this PID
                    let mut code = (0, 0);
                    let mut heap = (0, 0);
                    let mut stack = (0, 0);
                    
                    use crate::memory::address_space::*;
                    
                    for r in &self.regions {
                        if r.pid == pid {
                            if r.start >= 0x7000_0000_0000 {
                                stack = (r.start, r.start + r.size - 1);
                            } else if r.start >= HEAP_REGION_BASE {
                                heap = (r.start, r.start + r.size - 1);
                            } else if r.start >= CODE_REGION_BASE && r.start < SHM_REGION_BASE {
                                code = (r.start, r.start + r.size - 1);
                            }
                        }
                    }
                    
                    crate::debugln!("Slot {:>3} (PID {}): Code {:#x}..{:#x}  Heap {:#x}..{:#x}  Stack {:#x}..{:#x}", 
                        slot, pid, code.0, code.1, heap.0, heap.1, stack.0, stack.1);
                }
            }
        }

        // Print SHM regions by address range (strictly between SHM base and Heap base)
        for r in &self.regions {
            if r.start >= crate::memory::address_space::SHM_REGION_BASE && r.start < crate::memory::address_space::HEAP_REGION_BASE {
                // Ensure it's not actually a code region (which are < SHM base)
                crate::debugln!("Global SHM:  {:#x}..{:#x} ({:>8} KiB)", r.start, r.start + r.size - 1, r.size / 1024);
            }
        }

        let used = crate::memory::pmm::get_used_memory() / 1024 / 1024;
        let total = crate::memory::pmm::get_total_memory() / 1024 / 1024;
        crate::debugln!("Physical: {} MiB used / {} MiB total", used, total);
        crate::debugln!("----------------");
    }
}

pub static GLOBAL_VMA: Mutex<VmaAllocator> = Mutex::new(VmaAllocator::new());
