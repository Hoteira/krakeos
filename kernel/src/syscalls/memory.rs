use crate::task::CPUState;
use crate::memory::address::PhysAddr;
use crate::memory::{paging, pmm, vmm};

pub fn handle_brk(context: &mut CPUState) {
    let new_brk = context.rdi;
    let mut tm = crate::task::TASK_MANAGER.int_lock();
    let current_idx = tm.current_task;

    if current_idx < 0 {
        context.rax = 0;
        return;
    }

    if let Some(thread) = tm.tasks.get_mut(&(current_idx as usize)) {
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
                if let Some(phys) = pmm::allocate_frame() {
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

pub fn handle_memory_size(context: &mut CPUState) {
    if let Some(proc) = super::get_current_process() {
        context.rax = (*proc.linear_memory_size.lock() / 65536) as u64;
    } else {
        context.rax = u64::MAX;
    }
}

pub fn handle_memory_grow(context: &mut CPUState) {
    let n_pages = context.rdi as u32;
    if let Some(proc) = super::get_current_process() {
        let mut mem_size = proc.linear_memory_size.lock();
        let old_pages = (*mem_size / 65536) as u32;

        use crate::memory::address_space::LINEAR_MEMORY_SLOT_SIZE;
        let new_size = (*mem_size as u64) + (n_pages as u64 * 65536);
        if new_size > LINEAR_MEMORY_SLOT_SIZE {
            context.rax = u64::MAX;
            return;
        }

        // Map physical pages for the grown region
        let mem_base = proc.linear_memory_base;
        let grow_start = mem_base + *mem_size as u64;
        let grow_bytes = n_pages as u64 * 65536;

        let mut current_virt = grow_start;
        let end_virt = grow_start + grow_bytes;

        while current_virt < end_virt {
            if let Some(phys) = pmm::allocate_frame() {
                let flags = paging::PAGE_PRESENT | paging::PAGE_WRITABLE | paging::PAGE_USER;
                unsafe {
                    vmm::map_page(current_virt, PhysAddr::new(phys), flags, None);
                    core::ptr::write_bytes(current_virt as *mut u8, 0, 4096);
                }
                current_virt += 4096;
            } else {
                // OOM - don't update size, return failure
                context.rax = u64::MAX;
                return;
            }
        }

        *mem_size = new_size as usize;
        context.rax = old_pages as u64;
    } else {
        context.rax = u64::MAX;
    }
}

pub fn handle_mmap(context: &mut CPUState) {
    let addr = context.rdi;
    let len = context.rsi;
    let _prot = context.rdx;
    let _flags = context.r10;
    let _fd = context.r8;
    let _offset = context.r9;

    if len == 0 {
        context.rax = u64::MAX;
        return;
    }

    let (pid, heap_limit, mem_base) = {
        let tm = crate::task::TASK_MANAGER.int_lock();
        let current_idx = tm.current_task;
        if current_idx < 0 {
            crate::debugln!("[MMAP] REJECTED: no current task");
            context.rax = u64::MAX;
            return;
        }
        if let Some(thread) = tm.tasks.get(&(current_idx as usize)) {
            let proc = thread.process.as_ref().expect("Thread has no process");
            (proc.pid, proc.heap_limit, proc.linear_memory_base)
        } else {
            crate::debugln!("[MMAP] REJECTED: no thread");
            context.rax = u64::MAX;
            return;
        }
    };

    let target_addr = if addr == 0 {
        let tm = crate::task::TASK_MANAGER.int_lock();
        let current_idx = tm.current_task;
        if let Some(thread) = tm.tasks.get(&(current_idx as usize)) {
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
        // SAS Check
        use crate::memory::address_space::{LINEAR_MEMORY_BASE, LINEAR_MEMORY_SLOT_SIZE, STACK_REGION_BASE};
        
        // If kernel context (PID 0 or CS==0 fake context from kernel-mode syscall), allow mapping anywhere
        if pid == 0 || context.cs == 0 {
             if addr < 0x1000 { // Protect null
                 crate::debugln!("[MMAP] REJECTED: kernel tried to map null page");
                 context.rax = u64::MAX;
                 return;
             }
             // For kernel, we trust the caller (std::wasm) to provide a valid SAS address for some process.
             addr
        } else {
            // Check if within current process SAS slot
            if addr < mem_base || addr + len > mem_base + LINEAR_MEMORY_SLOT_SIZE {
                crate::debugln!("[MMAP] REJECTED: out of SAS bounds addr={:#x} mem_base={:#x} slot_size={:#x}", addr, mem_base, LINEAR_MEMORY_SLOT_SIZE);
                context.rax = u64::MAX;
                return;
            }
            addr
        }
    };

    let start_page = target_addr & !0xFFF;
    let end_page = (target_addr + len + 0xFFF) & !0xFFF;

    let mut current_virt = start_page;
    let mut remaining_bytes = (end_page - start_page) as usize;

    while remaining_bytes > 0 {
        let is_2mb_aligned = (current_virt % 0x200000 == 0) && (remaining_bytes >= 0x200000);
        
        if is_2mb_aligned {
            // Attempt 2MB huge page allocation
            if let Some(phys) = pmm::allocate_aligned_memory(0x200000, 0x200000) {
                let flags = paging::PAGE_PRESENT | paging::PAGE_WRITABLE | paging::PAGE_USER;
                vmm::map_huge_page(current_virt, PhysAddr::new(phys), flags, None);
                
                // Zeroing 2MB is faster in one go
                unsafe {
                    core::ptr::write_bytes((current_virt) as *mut u8, 0, 0x200000);
                }
                
                current_virt += 0x200000;
                remaining_bytes -= 0x200000;
                continue;
            }
        }

        // Fallback to 4KB page
        if let Some(phys) = pmm::allocate_frame() {
            let flags = paging::PAGE_PRESENT | paging::PAGE_WRITABLE | paging::PAGE_USER;
            unsafe {
                vmm::map_page(current_virt, PhysAddr::new(phys), flags, None);
                core::ptr::write_bytes(current_virt as *mut u8, 0, 4096);
            }
            current_virt += 4096;
            remaining_bytes -= 4096;
        } else {
            crate::debugln!("[MMAP] FAILED: OOM during allocation");
            context.rax = u64::MAX;
            return;
        }
    }

    context.rax = target_addr;
}

pub fn handle_munmap(context: &mut CPUState) {
    let _addr = context.rdi;
    let _len = context.rsi;

    context.rax = 0;
}

pub fn handle_get_process_mem(context: &mut CPUState) {
    let pid = context.rdi as u64;
    context.rax = crate::memory::vma::GLOBAL_VMA.lock().get_usage_by_pid(pid) as u64;
}

pub fn handle_shm_get(context: &mut CPUState) {
    let name_ptr = context.rdi as *const u8;
    let name_len = context.rsi as usize;
    let size = context.rdx as u64;

    if !super::validate_user_buf(context, name_ptr as u64, name_len as u64) { return; }
    let name = crate::syscalls::fs::copy_string_from_user(name_ptr, name_len);
    crate::debugln!("[Syscall] SHM_GET: name='{}', size={}", name, size);

    let mut shm = crate::memory::shm::GLOBAL_SHM.lock();
    match shm.get_or_create(&name, size) {
        Ok(addr) => {
            if addr == 1 {
                panic!("Syscall SHM_GET returned 1 for segment '{}'!", name);
            }
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

    if !super::validate_user_buf(context, name_ptr as u64, name_len as u64) { return; }
    let name = crate::syscalls::fs::copy_string_from_user(name_ptr, name_len);
    crate::debugln!("[Syscall] SHM_MAP: name='{}', target_addr={:#x}", name, target_addr);

    // Get current process info first, before taking SHM lock
    let (pid, mem_base) = {
        let tm = crate::task::TASK_MANAGER.int_lock();
        if let Some(current) = tm.current_task_idx() {
            if let Some(thread) = tm.tasks.get(&(current)) {
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
