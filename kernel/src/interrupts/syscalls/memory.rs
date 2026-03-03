use crate::interrupts::task::CPUState;
use crate::memory::address::PhysAddr;
use crate::memory::{paging, pmm, vmm};

pub fn handle_brk(context: &mut CPUState) {
    let new_brk = context.rdi;
    let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
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

        if new_brk < proc.heap_start || new_brk > proc.heap_limit {
            context.rax = u64::MAX;
            return;
        }

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
                        vmm::map_page(virt, PhysAddr::new(phys), flags, None);
                    }
                } else {
                    context.rax = current_brk;
                    return;
                }
            }
        }

        *heap_end = new_brk;
        context.rax = new_brk;
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

    crate::debugln!("[MMAP] addr={:#x} len={:#x}", addr, len);

    if len == 0 {
        crate::debugln!("[MMAP] REJECTED: len=0");
        context.rax = u64::MAX;
        return;
    }

    let (pid, heap_limit, mem_base) = {
        let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
        let current_idx = tm.current_task;
        if current_idx < 0 {
            crate::debugln!("[MMAP] REJECTED: no current task");
            context.rax = u64::MAX;
            return;
        }
        if let Some(thread) = tm.tasks[current_idx as usize].as_ref() {
            let proc = thread.process.as_ref().expect("Thread has no process");
            (proc.pid, proc.heap_limit, proc.linear_memory_base)
        } else {
            crate::debugln!("[MMAP] REJECTED: no thread");
            context.rax = u64::MAX;
            return;
        }
    };

    crate::debugln!("[MMAP] pid={} mem_base={:#x} heap_limit={:#x}", pid, mem_base, heap_limit);

    let target_addr = if addr == 0 {
        let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
        let current_idx = tm.current_task;
        if let Some(thread) = tm.tasks[current_idx as usize].as_ref() {
            let proc = thread.process.as_ref().expect("Thread has no process");
            let mut heap_end = proc.heap_end.lock();
            let old_brk = *heap_end;
            let new_brk = old_brk + len;

            if new_brk > heap_limit {
                context.rax = u64::MAX;
                return;
            }

            let aligned_new = (new_brk + 0xFFF) & !0xFFF;
            *heap_end = aligned_new;
            old_brk
        } else {
            context.rax = u64::MAX;
            return;
        }
    } else {
        // Check if within SAS slot
        use crate::memory::address_space::LINEAR_MEMORY_SLOT_SIZE;
        if addr < mem_base || addr + len > mem_base + LINEAR_MEMORY_SLOT_SIZE {
            crate::debugln!("[MMAP] REJECTED: out of SAS bounds addr={:#x} mem_base={:#x} slot_size={:#x}", addr, mem_base, LINEAR_MEMORY_SLOT_SIZE);
            context.rax = u64::MAX;
            return;
        }
        addr
    };

    let start_page = target_addr & !0xFFF;
    let end_page = (target_addr + len + 0xFFF) & !0xFFF;
    let pages = (end_page - start_page) / 4096;

    crate::debugln!("[MMAP] Mapping {} pages at {:#x}..{:#x}", pages, start_page, end_page);

    for i in 0..pages {
        let virt = start_page + (i * 4096);
        if let Some(phys) = pmm::allocate_frame(pid) {
            let flags = paging::PAGE_PRESENT | paging::PAGE_WRITABLE | paging::PAGE_USER;
            unsafe {
                vmm::map_page(virt, PhysAddr::new(phys), flags, None);
            }
        } else {
            crate::debugln!("[MMAP] FAILED: OOM at page {}", i);
            context.rax = u64::MAX;
            return;
        }
    }

    crate::debugln!("[MMAP] OK: mapped at {:#x}", target_addr);
    context.rax = target_addr;
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

pub fn handle_shm_get(context: &mut CPUState) {
    let name_ptr = context.rdi as *const u8;
    let name_len = context.rsi as usize;
    let size = context.rdx as u64;

    let name = crate::interrupts::syscalls::fs::copy_string_from_user(name_ptr, name_len);
    crate::debugln!("[Syscall] SHM_GET: name='{}', size={}", name, size);

    let mut shm = crate::memory::shm::GLOBAL_SHM.lock();
    match shm.get_or_create(&name, size) {
        Ok(addr) => {
            crate::debugln!("[Syscall] SHM_GET: Found/Created '{}' at {:#x}", name, addr);
            context.rax = addr;
        }
        Err(e) => {
            crate::debugln!("[Syscall] SHM_GET: FAILED for '{}': {}", name, e);
            context.rax = u64::MAX;
        }
    }
}

pub fn handle_shm_map(context: &mut CPUState) {
    let name_ptr = context.rdi as *const u8;
    let name_len = context.rsi as usize;
    let target_addr = context.rdx as u64;

    let name = crate::interrupts::syscalls::fs::copy_string_from_user(name_ptr, name_len);
    crate::debugln!("[Syscall] SHM_MAP: name='{}', target_addr={:#x}", name, target_addr);

    // Get current process info first, before taking SHM lock
    let (pid, mem_base) = {
        let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
        if let Some(current) = tm.current_task_idx() {
            if let Some(thread) = tm.tasks[current].as_ref() {
                let proc = thread.process.as_ref().expect("Thread has no process");
                (proc.pid, proc.linear_memory_base)
            } else {
                context.rax = u64::MAX;
                return;
            }
        } else {
            context.rax = u64::MAX;
            return;
        }
    };

    let shm = crate::memory::shm::GLOBAL_SHM.lock();
    if let Some(seg) = shm.get(&name) {
        // Bounds check
        use crate::memory::address_space::LINEAR_MEMORY_SLOT_SIZE;
        if target_addr < mem_base || target_addr + seg.size > mem_base + LINEAR_MEMORY_SLOT_SIZE {
            context.rax = u64::MAX;
            return;
        }

        let page_count = seg.frames.len();
        for i in 0..page_count {
            let phys = seg.frames[i];
            let flags = paging::PAGE_PRESENT | paging::PAGE_WRITABLE | paging::PAGE_USER;
            unsafe {
                vmm::map_page(target_addr + (i as u64 * 4096), PhysAddr::new(phys), flags, None);
            }
        }
        context.rax = 0;
    } else {
        context.rax = u64::MAX;
    }
}
