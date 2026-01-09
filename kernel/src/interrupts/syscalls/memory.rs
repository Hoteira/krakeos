use crate::interrupts::task::CPUState;
use crate::memory::{paging, pmm, vmm};
use crate::memory::address::PhysAddr;

pub fn handle_brk(context: &mut CPUState) {
    let new_brk = context.rdi;
    let mut tm = crate::interrupts::task::TASK_MANAGER.lock();
    let current_idx = tm.current_task;

    if current_idx < 0 {
        context.rax = 0;
        return;
    }

    if let Some(thread) = tm.tasks[current_idx as usize].as_mut() {
        let proc = thread.process.as_ref().expect("Thread has no process");
        let mut heap_end = proc.heap_end.lock();
        let current_brk = *heap_end;

        
        if new_brk == 0 {
            context.rax = current_brk;
            return;
        }

        let pml4_phys = proc.pml4_phys;
        let pid = proc.pid;

        
        let aligned_new = (new_brk + 0xFFF) & !0xFFF;
        let aligned_current = (current_brk + 0xFFF) & !0xFFF;

        if aligned_new > aligned_current {
            
            let size = aligned_new - aligned_current;
            let pages = size / 4096;
            
            
            for i in 0..pages {
                let virt = aligned_current + (i * 4096);
                if let Some(phys) = pmm::allocate_frame(pid) {
                    
                    let flags = paging::PAGE_PRESENT | paging::PAGE_WRITABLE | paging::PAGE_USER;
                    unsafe {
                        vmm::map_page(virt, PhysAddr::new(phys), flags, Some(pml4_phys));
                    }
                } else {
                    
                    context.rax = current_brk;
                    return;
                }
            }
            
            *heap_end = new_brk;
            context.rax = new_brk;
        } else if aligned_new < aligned_current {
            
            
            *heap_end = new_brk;
            context.rax = new_brk;
        } else {
            *heap_end = new_brk;
            context.rax = new_brk;
        }
    } else {
        context.rax = 0;
    }
}

pub fn handle_mmap(context: &mut CPUState) {
    let addr = context.rdi;
    let len = context.rsi;
    let _prot = context.rdx;
    let _flags = context.r10;
    let _fd = context.r8;
    let _offset = context.r9;

    let mut tm = crate::interrupts::task::TASK_MANAGER.lock();
    let current_idx = tm.current_task;
    
    if current_idx < 0 || len == 0 {
        context.rax = u64::MAX; 
        return;
    }

    if let Some(thread) = tm.tasks[current_idx as usize].as_mut() {
        let proc = thread.process.as_ref().expect("Thread has no process");
        let pml4_phys = proc.pml4_phys;
        let pid = proc.pid;

        
        
        
        
        
        
        
        
        
        
        
        
        let target_addr = if addr == 0 {
            
            
            let mut heap_end = proc.heap_end.lock();
            let old_brk = *heap_end;
            let new_brk = old_brk + len;
            
            let aligned_new = (new_brk + 0xFFF) & !0xFFF;
            *heap_end = aligned_new; 
            old_brk 
        } else {
            addr
        };

        let start_page = target_addr & !0xFFF;
        let end_page = (target_addr + len + 0xFFF) & !0xFFF;
        let pages = (end_page - start_page) / 4096;

        for i in 0..pages {
            let virt = start_page + (i * 4096);
            if let Some(phys) = pmm::allocate_frame(pid) {
                let flags = paging::PAGE_PRESENT | paging::PAGE_WRITABLE | paging::PAGE_USER;
                unsafe {
                    vmm::map_page(virt, PhysAddr::new(phys), flags, Some(pml4_phys));
                }
            } else {
                context.rax = u64::MAX;
                return;
            }
        }

        context.rax = target_addr;
    } else {
        context.rax = u64::MAX;
    }
}

pub fn handle_munmap(context: &mut CPUState) {
    let _addr = context.rdi;
    let _len = context.rsi;
    
    context.rax = 0;
}

pub fn handle_get_process_mem(context: &mut CPUState) {
    let pid = context.rdi as u64;
    context.rax = crate::memory::pmm::get_memory_usage_by_pid(pid) as u64;
}
