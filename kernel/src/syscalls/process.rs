use crate::debugln;
use crate::syscalls::fs::resolve_path;
use crate::task::CPUState;
use crate::memory::paging;
use alloc::string::String;
use alloc::vec::Vec;
use alloc::boxed::Box;

pub fn spawn_process(path: &str, args: Option<&[&str]>, fd_inheritance: Option<&[(u8, u8)]>, parent_pid: Option<u64>) -> Result<u64, String> {
    crate::debugln!("[spawn_process] path='{}'", path);
    let cwd_str = {
        let tm = crate::task::TASK_MANAGER.int_lock();
        if tm.current_task >= 0 {
            if let Some(thread) = tm.tasks.get(&(tm.current_task as usize)) {
                let proc = thread.process.as_ref().expect("Thread has no process");
                let cwd = proc.cwd.lock();
                let cwd_len = cwd.iter().position(|&c| c == 0).unwrap_or(cwd.len());
                String::from_utf8_lossy(&cwd[..cwd_len]).into_owned()
            } else {
                String::from("@0xE0/")
            }
        } else {
            String::from("@0xE0/")
        }
    };

    let resolved = resolve_path(&cwd_str, path);

    let path_parts: Vec<&str> = resolved.split('/').collect();
    if path_parts.len() < 1 || !path_parts[0].starts_with('@') {
        crate::debugln!("[spawn_process] Error: Invalid path format"); return Err(String::from("Invalid path format"));
    }

    let disk_part = &path_parts[0][1..];
    let disk_id = if disk_part.starts_with("0x") || disk_part.starts_with("0X") {
        u8::from_str_radix(&disk_part[2..], 16).unwrap_or(0xFF)
    } else {
        disk_part.parse::<u8>().unwrap_or_else(|_| u8::from_str_radix(disk_part, 16).unwrap_or(0xFF))
    };

    let actual_path = if path_parts.len() > 1 { path_parts[1..].join("/") } else { String::from("") };

    let process_name_str = if let Some(last_slash) = actual_path.rfind('/') {
        &actual_path[last_slash + 1..]
    } else {
        &actual_path
    };
    let process_name_bytes = process_name_str.as_bytes();


    let mut file_buf = Vec::new();
    if let Ok(mut node) = crate::fs::vfs::open(disk_id, &actual_path) {
        let size = node.size();
        if size > 0 {
            file_buf.resize(size as usize, 0);
            if let Err(_) = node.read(0, &mut file_buf) {
                return Err(String::from("Failed to read file"));
            }
        } else {
            return Err(String::from("File empty"));
        }
    } else {
        crate::debugln!("[spawn_process] Error: File not found"); return Err(String::from("File not found"));
    }

    // WASM Detection and Execution
    if file_buf.len() > 4 && &file_buf[0..4] == b"\0asm" {
        let pid_idx = crate::task::TASK_MANAGER.int_lock().reserve_pid().map_err(|_| String::from("No free process slots"))?;
        let pid = pid_idx as u64;

        let (new_fd_table, term_size) = {
            let tm = crate::task::TASK_MANAGER.int_lock();
            let mut fds = alloc::vec![-1i16; 16];
            let mut size = (80u16, 25u16);
            if tm.current_task >= 0 {
                if let Some(thread) = tm.tasks.get(&(tm.current_task as usize)) {
                    let proc = thread.process.as_ref().expect("Thread has no process");
                    fds = proc.fd_table.lock().clone();
                    size = (*proc.terminal_width.lock(), *proc.terminal_height.lock());

                    if let Some(map) = fd_inheritance {
                        let mut custom_fds = alloc::vec![-1i16; fds.len()];
                        for &(child_fd, parent_fd) in map {
                            if (parent_fd as usize) < fds.len() {
                                if (child_fd as usize) >= custom_fds.len() {
                                    custom_fds.resize((child_fd as usize) + 1, -1);
                                }
                                custom_fds[child_fd as usize] = fds[parent_fd as usize];
                            }
                        }
                        fds = custom_fds;
                    }
                }
            }
            (fds, size)
        };

        for &g_fd in new_fd_table.iter() {
            if g_fd != -1 {
                crate::fs::vfs::increment_ref(g_fd as usize);
            }
        }

        // We need a slot_id to do AOT compilation
        // init_user_task creates the Process and assigns slot_id
        {
            let mut tm = crate::task::TASK_MANAGER.int_lock();
            tm.init_user_task(pid_idx, 0, 0, 0, args, Some(new_fd_table), process_name_bytes, term_size, parent_pid).map_err(|_| String::from("Failed to init task"))?;
        }

        let slot_id = {
            let tm = crate::task::TASK_MANAGER.int_lock();
            tm.tasks.get(&pid_idx).unwrap().process.as_ref().unwrap().slot_id
        };

        let wasm_args_vec: Vec<String> = args.unwrap_or(&[]).iter().map(|s| String::from(*s)).collect();

        // Perform Synchronous AOT Compilation
        let res = std::wasm::runner::run_with_buffer(
            path,
            &file_buf,
            wasm_args_vec,
            &cwd_str,
            &[],
            alloc::vec::Vec::new(),
            true, // AOT
            pid,
            slot_id,
        );

        match res {
            std::wasm::runner::WasmRunResult::AotReady(info) => {
                crate::debugln!("[spawn_process] AOT ready for PID {}. Entry={:#x} ctx={:#x}", pid, info.entry_addr, info.ctx_ptr);
                // Now re-initialize the task with the real entry point and context!
                let mut tm = crate::task::TASK_MANAGER.int_lock();
                // We don't want to re-create the process object, just update the task state.
                if let Some(task) = tm.tasks.get_mut(&pid_idx) {
                    unsafe {
                        let state = &mut *(task.cpu_state_ptr as *mut crate::task::CPUState);
                        state.rip = info.entry_addr;
                        state.rdi = info.ctx_ptr;
                        state.cs = 0x23; // User code
                        state.ss = 0x1B; // User data
                        state.rflags = 0x202; // IF=1

                        // Initialize Ring3Context stack fields
                        let ctx = &mut *(info.ctx_ptr as *mut ::std::wasm::aot::runtime::Ring3Context);
                        ctx.stack_base = info.stack_base as *mut u128;
                        ctx.stack_limit = info.stack_limit as usize;
                        ctx.locals_base = (info.stack_base - 1024 * 1024) as *mut u128;

                        // Push exit stub as return address so the entry function can return cleanly.
                        // The exit stub is at jump_table[1023] in the blob.
                        let jump_table = ctx.blob_base as *const u64;
                        let exit_fn_addr = *jump_table.add(1023);
                        let rsp = (info.stack_base & !15) - 8;
                        *(rsp as *mut u64) = exit_fn_addr;
                        state.rsp = rsp;

                        // Initialize linear_memory_size from Ring3Context
                        let proc = task.process.as_ref().expect("Thread has no process");
                        *proc.linear_memory_size.lock() = ctx.memory_size;
                        crate::debugln!("[spawn_process] Set linear_memory_size={} ({} pages)", ctx.memory_size, ctx.memory_size / 65536);
                    }
                    task.state = crate::task::ThreadState::Ready;
                    tm.push_to_run_queue(pid_idx);
                }
                return Ok(pid);
            }
            std::wasm::runner::WasmRunResult::Finished(exit_code) => {
                crate::debugln!("[spawn_process] WASM finished immediately with code {}", exit_code);
                // Task is already in map, but we might want to kill it or let it be.
                // If it finished during instantiation, it's effectively dead.
                crate::task::TASK_MANAGER.int_lock().kill_process(pid);
                return Ok(pid);
            }
        }
    }

    let pid_idx_elf = crate::task::TASK_MANAGER.int_lock().reserve_pid().map_err(|_| String::from("No free process slots"))?;
    let pid_elf = pid_idx_elf as u64;


    let (new_fd_table_elf, term_size_elf) = {
        let tm = crate::task::TASK_MANAGER.int_lock();
        let mut fds = alloc::vec![-1i16; 16];
        let mut size = (80u16, 25u16);
        if tm.current_task >= 0 {
            if let Some(thread) = tm.tasks.get(&(tm.current_task as usize)) {
                let proc = thread.process.as_ref().expect("Thread has no process");
                fds = proc.fd_table.lock().clone();
                size = (*proc.terminal_width.lock(), *proc.terminal_height.lock());

                if let Some(map) = fd_inheritance {
                    let mut custom_fds = alloc::vec![-1i16; fds.len()];
                    for &(child_fd, parent_fd) in map {
                        if (parent_fd as usize) < fds.len() {
                            if (child_fd as usize) >= custom_fds.len() {
                                custom_fds.resize((child_fd as usize) + 1, -1);
                            }
                            custom_fds[child_fd as usize] = fds[parent_fd as usize];
                        }
                    }
                    fds = custom_fds;
                }
            }
        }
        (fds, size)
    };

    for &g_fd in new_fd_table_elf.iter() {
        if g_fd != -1 {
            crate::fs::vfs::increment_ref(g_fd as usize);
        }
    }


    {
        let mut tm = crate::task::TASK_MANAGER.int_lock();

        tm.init_user_task(pid_idx_elf, 0, 0, 0, args, Some(new_fd_table_elf), process_name_bytes, term_size_elf, parent_pid).map_err(|_| String::from("Failed to init task"))?;
    }


    #[cfg(feature = "elf_support")]
    {
        match crate::fs::elf::load_elf(&file_buf, pid_elf) {
            Ok(entry_point) => {
                let mut tm = crate::task::TASK_MANAGER.int_lock();
                let task = tm.tasks.get_mut(&(pid_idx_elf)).unwrap();


                unsafe {
                    let cpu_state = &mut *(task.cpu_state_ptr as *mut crate::task::CPUState);
                    cpu_state.rip = entry_point;
                }

                Ok(pid_elf)
            }
            Err(e) => {
                crate::task::TASK_MANAGER.int_lock().kill_process(pid_elf);
                Err(e)
            }
        }
    }

    #[cfg(not(feature = "elf_support"))]
    {
        crate::task::TASK_MANAGER.int_lock().kill_process(pid_elf);
        Err(String::from("ELF support is disabled"))
    }
}

pub fn spawn_ext_process(name: &str, state: CPUState, parent_pid: Option<u64>) -> Result<u64, String> {
    let pid_idx = crate::task::TASK_MANAGER.int_lock().reserve_pid().map_err(|_| String::from("No free process slots"))?;
    let pid = pid_idx as u64;

    let process_name_bytes = name.as_bytes();
    let term_size = (80u16, 25u16); // Default

    {
        let mut tm = crate::task::TASK_MANAGER.int_lock();
        tm.init_user_task(pid_idx, 0, 0, 0, None, None, process_name_bytes, term_size, parent_pid).map_err(|_| String::from("Failed to init task"))?;
        
        let task = tm.tasks.get_mut(&(pid_idx)).unwrap();
        unsafe {
            core::ptr::write(task.cpu_state_ptr as *mut CPUState, state);
        }
    }

    Ok(pid)
}

pub fn handle_spawn_ext(context: &mut CPUState) {
    let name_ptr = context.rdi as *const u8;
    let name_len = context.rsi as usize;
    let state_ptr = context.rdx as *const CPUState;

    if name_ptr.is_null() || name_len == 0 || state_ptr.is_null() {
        context.rax = u64::MAX;
        return;
    }
    if !super::validate_user_buf(context, name_ptr as u64, name_len as u64) { return; }
    if !super::validate_user_buf(context, state_ptr as u64, core::mem::size_of::<CPUState>() as u64) { return; }

    let name_slice = unsafe { core::slice::from_raw_parts(name_ptr, name_len) };
    let name = String::from_utf8_lossy(name_slice);
    let state = unsafe { *state_ptr };

    let parent_pid = {
        let tm = crate::task::TASK_MANAGER.int_lock();
        let current = tm.current_task;
        if current >= 0 {
            tm.tasks.get(&(current as usize)).and_then(|t| t.process.as_ref()).map(|p| p.pid)
        } else {
            None
        }
    };

    match spawn_ext_process(&name, state, parent_pid) {
        Ok(pid) => context.rax = pid,
        Err(_) => context.rax = u64::MAX,
    }
}

pub fn handle_exit(context: &mut CPUState) {
    let exit_code = context.rdi;
    debugln!("[Syscall] Process exited with code {}", exit_code);
    {
        use crate::window_manager::composer::COMPOSER;

        let mut tm = crate::task::TASK_MANAGER.int_lock();
        let current = tm.current_task;
        if current >= 0 {
            if let Some(thread) = tm.tasks.get_mut(&(current as usize)) {
                thread.exit_code = exit_code;
                thread.state = crate::task::ThreadState::Zombie;

                unsafe {
                    (*(&raw mut COMPOSER)).remove_windows_by_pid(current as u64);
                }

                let proc = thread.process.as_ref().expect("Thread has no process");
                let mut fd_table = proc.fd_table.lock();
                for i in 0..fd_table.len() {
                    let global = fd_table[i];
                    if global != -1 {
                        crate::fs::vfs::close_file(global as usize);
                        fd_table[i] = -1;
                    }
                }
            }
        }
    }

    unsafe {
        core::arch::asm!("sti");
        loop { core::arch::asm!("hlt"); }
    }
}

pub fn handle_spawn(context: &mut CPUState) {
    let path_ptr = context.rdi as *const u8;
    let path_len = context.rsi as usize;
    let args_ptr = context.rdx as *const *const u8;
    let args_len = context.r10 as usize;
    let fd_map_ptr = context.r8 as *const (u8, u8);
    let fd_map_len = context.r9 as usize;
    crate::debugln!("[Syscall] handle_spawn called! path_len={}", path_len);

    if path_ptr.is_null() || path_len == 0 {
        context.rax = u64::MAX;
        return;
    }
    if !super::validate_user_buf(context, path_ptr as u64, path_len as u64) { return; }

    let path_slice = unsafe { core::slice::from_raw_parts(path_ptr, path_len) };
    let path_str = String::from_utf8_lossy(path_slice);


    let mut args_vec = Vec::new();
    if !args_ptr.is_null() && args_len > 0 {
        if !super::validate_user_buf(context, args_ptr as u64, (args_len * core::mem::size_of::<*const u8>()) as u64) { return; }
        let args_ptrs = unsafe { core::slice::from_raw_parts(args_ptr, args_len) };
        for &ptr in args_ptrs {
            if !ptr.is_null() {
                let s = unsafe { core::ffi::CStr::from_ptr(ptr as *const i8).to_string_lossy() };
                args_vec.push(s);
            }
        }
    }

    let args_refs: Vec<&str> = args_vec.iter().map(|s| s.as_str()).collect();
    let args_opt = if args_refs.is_empty() { None } else { Some(args_refs.as_slice()) };

    let fd_map = if !fd_map_ptr.is_null() && fd_map_len > 0 {
        if !super::validate_user_buf(context, fd_map_ptr as u64, (fd_map_len * core::mem::size_of::<(u8, u8)>()) as u64) { return; }
        unsafe { Some(core::slice::from_raw_parts(fd_map_ptr, fd_map_len)) }
    } else {
        None
    };

    let parent_pid = {
        let tm = crate::task::TASK_MANAGER.int_lock();
        let current = tm.current_task;
        if current >= 0 {
            tm.tasks.get(&(current as usize)).and_then(|t| t.process.as_ref()).map(|p| p.pid)
        } else {
            None
        }
    };

    match spawn_process(&path_str, args_opt, fd_map, parent_pid) {
        Ok(pid) => context.rax = pid,
        Err(e) => {
            crate::debugln!("Spawn Error: {}", e);
            context.rax = u64::MAX;
        }
    }
}

pub fn handle_kill(context: &mut CPUState) {
    let target_pid = context.rdi as u64;
    let mut tm = crate::task::TASK_MANAGER.int_lock();
    
    let current_idx = tm.current_task;
    let mut can_kill = false;

    if current_idx < 0 { // Kernel
        can_kill = true;
    } else {
        if let Some(current_thread) = tm.tasks.get(&(current_idx as usize)) {
            let current_proc = current_thread.process.as_ref().unwrap();
            
            // Allow if killing self
            if current_proc.pid == target_pid {
                can_kill = true;
            } else if current_proc.pid == 0 {
                can_kill = true;
            } else {
                // Allow if current is parent of target
                for (_, t) in tm.tasks.iter() {
                    if true {
                        if let Some(p) = t.process.as_ref() {
                            if p.pid == target_pid && p.parent_pid == Some(current_proc.pid) {
                                can_kill = true;
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    if can_kill {
        tm.kill_process(target_pid);
        context.rax = 0;
    } else {
        context.rax = u64::MAX;
    }
}

pub fn handle_getpid(context: &mut CPUState) {
    let tm = crate::task::TASK_MANAGER.int_lock();
    let current = tm.current_task;
    if current >= 0 {
        if let Some(thread) = tm.tasks.get(&(current as usize)) {
            let proc = thread.process.as_ref().expect("Thread has no process");
            context.rax = proc.pid;
            return;
        }
    }
    context.rax = u64::MAX;
}

pub fn handle_wait_pid(context: &mut CPUState) {
    let target_pid = context.rdi as usize;
    if target_pid >= crate::task::MAX_TASKS {
        context.rax = u64::MAX;
        return;
    }

    let mut tm = crate::task::TASK_MANAGER.int_lock();
    if let Some(task) = tm.tasks.get_mut(&target_pid) {
        match task.state {
            crate::task::ThreadState::Zombie => {
                let exit_code = task.exit_code;
                context.rax = exit_code;

                let pid = target_pid as u64;
                let k_stack_top = task.kernel_stack;

                crate::memory::vma::GLOBAL_VMA.lock().remove_by_pid(pid);

                if k_stack_top != 0 {
                    let k_stack_start = k_stack_top - (1024 * 1024 + paging::HHDM_OFFSET);
                    crate::memory::pmm::free_frame(k_stack_start);
                }

                tm.tasks.remove(&target_pid);
            }
            crate::task::ThreadState::Null => {
                context.rax = 0;
            }
            _ => {
                context.rax = u64::MAX;
            }
        }
    } else {
        context.rax = 0;
    }
}

pub fn handle_get_process_list(context: &mut CPUState) {
    let buf_ptr = context.rdi as *mut u8;
    let max_count = context.rsi as usize;

    if buf_ptr.is_null() || max_count == 0 {
        context.rax = 0;
        return;
    }
    let struct_size = 48u64;
    if !super::validate_user_buf(context, buf_ptr as u64, max_count as u64 * struct_size) { return; }

    let mut count = 0;
    let tm = crate::task::TASK_MANAGER.int_lock();

    let struct_size = 48;

    for (i, task) in tm.tasks.iter() {
        if task.state != crate::task::ThreadState::Null {
            if count >= max_count {
                break;
            }

            let offset = count * struct_size;
            unsafe {
                let ptr = buf_ptr.add(offset);
                *(ptr as *mut u64) = *i as u64;
                *(ptr.add(8) as *mut u64) = match task.state {
                    crate::task::ThreadState::Null => 0,
                    crate::task::ThreadState::Reserved => 1,
                    crate::task::ThreadState::Ready => 2,
                    crate::task::ThreadState::Zombie => 3,
                    crate::task::ThreadState::Sleeping => 4,
                    _ => 0,
                };

                let name_ptr = ptr.add(16);
                core::ptr::copy_nonoverlapping(task.name.as_ptr(), name_ptr, 32);
            }
            count += 1;
        }
    }
    context.rax = count as u64;
}

#[repr(C)]
pub struct SlotInfo {
    pub slot_id: u16,
    pub linear_memory_base: u64,
    pub linear_memory_size: u64,
    pub code_base: u64,
    pub stack_base: u64,
}

pub fn handle_get_slot_info(context: &mut CPUState) {
    let buf_ptr = context.rdi as *mut SlotInfo;
    if buf_ptr.is_null() {
        context.rax = u64::MAX;
        return;
    }
    if !super::validate_user_buf(context, buf_ptr as u64, core::mem::size_of::<SlotInfo>() as u64) { return; }

    let tm = crate::task::TASK_MANAGER.int_lock();
    if let Some(current) = tm.current_task_idx() {
        if let Some(thread) = tm.tasks.get(&(current)) {
            let proc = thread.process.as_ref().expect("Thread has no process");
            
            use crate::memory::address_space::*;
            let info = SlotInfo {
                slot_id: proc.slot_id,
                linear_memory_base: proc.linear_memory_base,
                linear_memory_size: LINEAR_MEMORY_SLOT_SIZE,
                code_base: CODE_REGION_BASE + (proc.slot_id as u64) * CODE_SLOT_SIZE,
                stack_base: STACK_REGION_BASE + (proc.slot_id as u64) * STACK_SLOT_SIZE,
            };

            unsafe {
                core::ptr::write_unaligned(buf_ptr, info);
            }
            context.rax = 0;
            return;
        }
    }
    context.rax = u64::MAX;
}

pub fn handle_sleep(context: &mut CPUState) {
    let duration = context.rdi;
    let mut tm = crate::task::TASK_MANAGER.int_lock();
    let current = tm.current_task;

    if current >= 0 {
        if let Some(task) = tm.tasks.get_mut(&(current as usize)) {
            task.wake_ticks = unsafe { crate::task::SYSTEM_TICKS } + duration;
            task.state = crate::task::ThreadState::Sleeping;
        }
    }
}

pub fn handle_spawn_thread(context: &mut CPUState) {
    let entry = context.rdi;
    let stack = context.rsi;
    let arg = context.rdx;

    let mut tm = crate::task::TASK_MANAGER.int_lock();
    let current = tm.current_task;

    if current < 0 {
        context.rax = u64::MAX;
        return;
    }

    match tm.spawn_thread(current as usize, entry, stack, arg) {
        Ok(tid) => context.rax = tid as u64,
        Err(_) => context.rax = u64::MAX,
    }
}

pub fn handle_thread_exit(context: &mut CPUState) {
    let exit_code = context.rdi;
    debugln!("[Syscall] Thread exited");
    {
        let mut tm = crate::task::TASK_MANAGER.int_lock();
        let current = tm.current_task;
        if current >= 0 {
            if let Some(task) = tm.tasks.get_mut(&(current as usize)) {
                task.state = crate::task::ThreadState::Zombie;
                task.exit_code = 0;
            }
        }
    }

    unsafe {
        core::arch::asm!("sti");
        loop { core::arch::asm!("hlt"); }
    }
}
