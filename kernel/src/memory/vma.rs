use crate::sync::Mutex;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;

#[derive(Debug, Clone, Copy)]
pub struct VmaRegion {
    pub start: u64,
    pub size: u64,
    pub pid: u64,
}

pub struct VmaAllocator {
    regions: BTreeMap<u64, VmaRegion>,
}

impl VmaAllocator {
    pub const fn new() -> Self {
        Self { regions: BTreeMap::new() }
    }

    pub fn track(&mut self, start: u64, size: u64, pid: u64) {
        crate::debugln!("[VMA] Track PID {} region {:#x}..{:#x}", pid, start, start + size);
        let end = start + size;
        for r in self.regions.values() {
            let r_end = r.start + r.size;
            if (start >= r.start && start < r_end) || (end > r.start && end <= r_end) {
                crate::debugln!("VMA WARNING: Overlap detected! New [PID {} {:#x}-{:#x}], Existing [PID {} {:#x}-{:#x}]", 
                    pid, start, end, r.pid, r.start, r_end);
            }
        }
        crate::debugln!("[VMA] Pushing to BTreeMap...");
        self.regions.insert(start, VmaRegion { start, size, pid });
        crate::debugln!("[VMA] Push successful!");
    }

    pub fn is_mapped(&self, addr: u64) -> bool {
        if let Some((_, r)) = self.regions.range(..=addr).next_back() {
            if addr < r.start + r.size {
                return true;
            }
        }
        false
    }

    pub fn remove_by_pid(&mut self, pid: u64) {
        let mut to_remove = Vec::new();
        for r in self.regions.values() {
            if r.pid == pid {
                crate::memory::vmm::unmap_and_free_range(r.start, r.size);
                to_remove.push(r.start);
            }
        }
        for start in to_remove {
            self.regions.remove(&start);
        }
    }

    pub fn get_usage_by_pid(&self, pid: u64) -> usize {
        let mut total = 0;
        for r in self.regions.values() {
            if r.pid == pid {
                total += r.size as usize;
            }
        }
        total
    }

    pub fn get_regions(&self) -> alloc::collections::btree_map::Values<'_, u64, VmaRegion> {
        self.regions.values()
    }

    pub fn dump(&self) {
        let mut buf = [0u8; 4096];
        let len = self.dump_to_buffer(&mut buf);
        crate::debugln!("{}", core::str::from_utf8(&buf[..len]).unwrap_or("VMA Dump Error"));
    }

    pub fn dump_to_buffer(&self, buf: &mut [u8]) -> usize {
        use core::fmt::Write;
        struct BufWriter<'a> {
            buf: &'a mut [u8],
            pos: usize,
        }
        impl Write for BufWriter<'_> {
            fn write_str(&mut self, s: &str) -> core::fmt::Result {
                let len = s.len();
                if self.pos + len > self.buf.len() {
                    return Err(core::fmt::Error);
                }
                self.buf[self.pos..self.pos + len].copy_from_slice(s.as_bytes());
                self.pos += len;
                Ok(())
            }
        }

        let mut writer = BufWriter { buf, pos: 0 };
        let _ = writeln!(writer, "\n[SAS DUMP]");
        
        let mut seen_pids = Vec::new();
        let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
        
        for task in tm.get_tasks() {
            if true {
                if let Some(proc) = &task.process {
                    let pid = proc.pid;
                    if seen_pids.contains(&pid) { continue; }
                    seen_pids.push(pid);
                    
                    let slot = proc.slot_id;
                    let mut code = (0, 0);
                    let mut heap = (0, 0);
                    let mut stack = (0, 0);
                    
                    use crate::memory::address_space::*;
                    for r in self.regions.values() {
                        if r.pid == pid {
                            if r.start >= STACK_REGION_BASE && r.start < LINEAR_MEMORY_BASE {
                                stack = (r.start, r.start + r.size - 1);
                            } else if r.start >= LINEAR_MEMORY_BASE {
                                heap = (r.start, r.start + r.size - 1);
                            } else if r.start >= CODE_REGION_BASE && r.start < STACK_REGION_BASE {
                                code = (r.start, r.start + r.size - 1);
                            }
                        }
                    }
                    let _ = writeln!(writer, "Slot {:>3} (PID {}): Code {:#x}..{:#x}  LinMem {:#x}..{:#x}  Stack {:#x}..{:#x}", 
                        slot, pid, code.0, code.1, heap.0, heap.1, stack.0, stack.1);
                }
            }
        }

        let used = crate::memory::pmm::get_used_memory() / 1024 / 1024;
        let total = crate::memory::pmm::get_total_memory() / 1024 / 1024;
        let _ = writeln!(writer, "Physical: {} MiB used / {} MiB total", used, total);
        let _ = writeln!(writer, "----------------");
        writer.pos
    }
}

pub static GLOBAL_VMA: Mutex<VmaAllocator> = Mutex::new(VmaAllocator::new());
