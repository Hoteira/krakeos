use crate::debugln;
use crate::interrupts::syscalls::fs::resolve_path;
use crate::interrupts::task::CPUState;
use crate::memory::paging;
use alloc::string::String;
use alloc::vec::Vec;
use alloc::boxed::Box;

pub fn spawn_process(path: &str, args: Option<&[&str]>, fd_inheritance: Option<&[(u8, u8)]>, parent_pid: Option<u64>) -> Result<u64, String> {
    let cwd_str = {
        let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
        if tm.current_task >= 0 {
            if let Some(thread) = tm.tasks[tm.current_task as usize].as_ref() {
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
        return Err(String::from("Invalid path format"));
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
        return Err(String::from("File not found"));
    }

    // WASM Detection and Execution via Native Kernel Thread
    if file_buf.len() > 4 && &file_buf[0..4] == b"\0asm" {
        let pid_idx = crate::interrupts::task::TASK_MANAGER.int_lock().reserve_pid().map_err(|_| String::from("No free process slots"))?;
        let pid = pid_idx as u64;

        let (new_fd_table, term_size) = {
            let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
            let mut fds = [-1i16; 16];
            let mut size = (80u16, 25u16);
            if tm.current_task >= 0 {
                if let Some(thread) = tm.tasks[tm.current_task as usize].as_ref() {
                    let proc = thread.process.as_ref().expect("Thread has no process");
                    fds = *proc.fd_table.lock();
                    size = (*proc.terminal_width.lock(), *proc.terminal_height.lock());

                    if let Some(map) = fd_inheritance {
                        let mut custom_fds = [-1i16; 16];
                        for &(child_fd, parent_fd) in map {
                            if (parent_fd as usize) < 16 && (child_fd as usize) < 16 {
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

        let slot_id = {
            let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
            tm.init_user_task(pid_idx, 0, 0, args, Some(new_fd_table), process_name_bytes, term_size, parent_pid).map_err(|_| String::from("Failed to init task"))?;
            
            // FORCE the parent thread to not be scheduled at all
            let proc = tm.tasks[pid_idx].as_ref().unwrap().process.as_ref().unwrap().clone();
            tm.tasks[pid_idx].as_mut().unwrap().state = crate::interrupts::task::ThreadState::Null;
            
            proc.slot_id
        };

        struct WasmThreadArgs {
            path_ptr: *mut u8,
            path_len: usize,
            buf_ptr: *mut u8,
            buf_len: usize,
            pid: u64,
            slot_id: u16,
            wasm_args: *mut Vec<String>,
        }

        extern "C" fn wasm_thread_entry(args_ptr: u64) {
            let args = unsafe { Box::from_raw(args_ptr as *mut WasmThreadArgs) };

            let path_slice = unsafe { core::slice::from_raw_parts(args.path_ptr, args.path_len) };
            let path = String::from_utf8_lossy(path_slice).into_owned();

            let buffer = unsafe { core::slice::from_raw_parts(args.buf_ptr, args.buf_len) };

            let wasm_args = unsafe { *Box::from_raw(args.wasm_args) };

            let res = std::wasm::runner::run_with_buffer(
                &path,
                buffer,
                wasm_args,
                "@0xE0/",
                &[],
                Vec::new(),
                true, // AOT
                args.pid,
                args.slot_id,
            );

            unsafe {
                alloc::alloc::dealloc(args.path_ptr, core::alloc::Layout::from_size_align(args.path_len, 1).unwrap());
                alloc::alloc::dealloc(args.buf_ptr, core::alloc::Layout::from_size_align(args.buf_len, 1).unwrap());

                crate::interrupts::syscalls::syscall_dispatcher(
                    crate::interrupts::syscalls::SYS_EXIT,
                    res as u64,
                    0, 0, 0, 0, 0
                );
            }
        }

        // Build args vec from what spawn_with_fds passed (already includes argv[0])
        let wasm_args_vec: Vec<String> = args.unwrap_or(&[]).iter().map(|s| String::from(*s)).collect();

        // Leak strings/buffers manually to avoid large boxed structs
        let path_bytes = path.as_bytes();
        let path_ptr = unsafe { alloc::alloc::alloc(core::alloc::Layout::from_size_align(path_bytes.len(), 1).unwrap()) };
        unsafe { core::ptr::copy_nonoverlapping(path_bytes.as_ptr(), path_ptr, path_bytes.len()) };

        let buf_ptr = unsafe { alloc::alloc::alloc(core::alloc::Layout::from_size_align(file_buf.len(), 1).unwrap()) };
        unsafe { core::ptr::copy_nonoverlapping(file_buf.as_ptr(), buf_ptr, file_buf.len()) };

        let thread_args = Box::into_raw(Box::new(WasmThreadArgs {
            path_ptr,
            path_len: path_bytes.len(),
            buf_ptr,
            buf_len: file_buf.len(),
            pid,
            slot_id,
            wasm_args: Box::into_raw(Box::new(wasm_args_vec)),
        }));

        crate::debugln!("[Spawn] Spawning WASM kernel thread for PID {} (slot {}) at {:#x}...", pid, slot_id, wasm_thread_entry as u64);

        {
            let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
            tm.spawn_thread(pid_idx, wasm_thread_entry as u64, 0, thread_args as u64).map_err(|_| String::from("Failed to spawn WASM thread"))?;
        }

        return Ok(pid);
        }


    let pid_idx_elf = crate::interrupts::task::TASK_MANAGER.int_lock().reserve_pid().map_err(|_| String::from("No free process slots"))?;
    let pid_elf = pid_idx_elf as u64;


    let (new_fd_table_elf, term_size_elf) = {
        let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
        let mut fds = [-1i16; 16];
        let mut size = (80u16, 25u16);
        if tm.current_task >= 0 {
            if let Some(thread) = tm.tasks[tm.current_task as usize].as_ref() {
                let proc = thread.process.as_ref().expect("Thread has no process");
                fds = *proc.fd_table.lock();
                size = (*proc.terminal_width.lock(), *proc.terminal_height.lock());

                if let Some(map) = fd_inheritance {
                    let mut custom_fds = [-1i16; 16];
                    for &(child_fd, parent_fd) in map {
                        if (parent_fd as usize) < 16 && (child_fd as usize) < 16 {
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
        let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();

        tm.init_user_task(pid_idx_elf, 0, 0, args, Some(new_fd_table_elf), process_name_bytes, term_size_elf, parent_pid).map_err(|_| String::from("Failed to init task"))?;
    }


    #[cfg(feature = "elf_support")]
    {
        match crate::fs::elf::load_elf(&file_buf, pid_elf) {
            Ok(entry_point) => {
                let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
                let task = tm.tasks[pid_idx_elf].as_mut().unwrap();


                unsafe {
                    let cpu_state = &mut *(task.cpu_state_ptr as *mut crate::interrupts::task::CPUState);
                    cpu_state.rip = entry_point;
                }

                Ok(pid_elf)
            }
            Err(e) => {
                crate::interrupts::task::TASK_MANAGER.int_lock().kill_process(pid_elf);
                Err(e)
            }
        }
    }

    #[cfg(not(feature = "elf_support"))]
    {
        crate::interrupts::task::TASK_MANAGER.int_lock().kill_process(pid_elf);
        Err(String::from("ELF support is disabled"))
    }
}

pub fn spawn_ext_process(name: &str, state: CPUState, parent_pid: Option<u64>) -> Result<u64, String> {
    let pid_idx = crate::interrupts::task::TASK_MANAGER.int_lock().reserve_pid().map_err(|_| String::from("No free process slots"))?;
    let pid = pid_idx as u64;

    let process_name_bytes = name.as_bytes();
    let term_size = (80u16, 25u16); // Default

    {
        let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
        tm.init_user_task(pid_idx, 0, 0, None, None, process_name_bytes, term_size, parent_pid).map_err(|_| String::from("Failed to init task"))?;
        
        let task = tm.tasks[pid_idx].as_mut().unwrap();
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
        let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
        let current = tm.current_task;
        if current >= 0 {
            tm.tasks[current as usize].as_ref().and_then(|t| t.process.as_ref()).map(|p| p.pid)
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

        let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
        let current = tm.current_task;
        if current >= 0 {
            if let Some(thread) = tm.tasks[current as usize].as_mut() {
                thread.exit_code = exit_code;
                thread.state = crate::interrupts::task::ThreadState::Zombie;

                unsafe {
                    (*(&raw mut COMPOSER)).remove_windows_by_pid(current as u64);
                }

                let proc = thread.process.as_ref().expect("Thread has no process");
                let mut fd_table = proc.fd_table.lock();
                for i in 0..16 {
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
        let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
        let current = tm.current_task;
        if current >= 0 {
            tm.tasks[current as usize].as_ref().and_then(|t| t.process.as_ref()).map(|p| p.pid)
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
    let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
    
    let current_idx = tm.current_task;
    let mut can_kill = false;

    if current_idx < 0 { // Kernel
        can_kill = true;
    } else {
        if let Some(current_thread) = tm.tasks[current_idx as usize].as_ref() {
            let current_proc = current_thread.process.as_ref().unwrap();
            
            // Allow if killing self
            if current_proc.pid == target_pid {
                can_kill = true;
            } else if current_proc.pid == 0 {
                can_kill = true;
            } else {
                // Allow if current is parent of target
                for i in 0..crate::interrupts::task::MAX_THREADS {
                    if let Some(t) = tm.tasks[i].as_ref() {
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
    let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
    let current = tm.current_task;
    if current >= 0 {
        if let Some(thread) = tm.tasks[current as usize].as_ref() {
            let proc = thread.process.as_ref().expect("Thread has no process");
            context.rax = proc.pid;
            return;
        }
    }
    context.rax = u64::MAX;
}

pub fn handle_wait_pid(context: &mut CPUState) {
    let target_pid = context.rdi as usize;
    if target_pid >= crate::interrupts::task::MAX_TASKS {
        context.rax = u64::MAX;
        return;
    }

    let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
    let task_opt = &mut tm.tasks[target_pid];

    if let Some(task) = task_opt {
        match task.state {
            crate::interrupts::task::ThreadState::Zombie => {
                let exit_code = task.exit_code;
                context.rax = exit_code;

                let pid = target_pid as u64;
                let k_stack_top = task.kernel_stack;

                crate::memory::pmm::free_frames_by_pid(pid);
                crate::memory::vma::GLOBAL_VMA.lock().remove_by_pid(pid);

                if k_stack_top != 0 {
                    let k_stack_start = k_stack_top - (1024 * 1024 + paging::HHDM_OFFSET);
                    crate::memory::pmm::free_frame(k_stack_start);
                }

                *task_opt = None;
            }
            crate::interrupts::task::ThreadState::Null => {
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
    let tm = crate::interrupts::task::TASK_MANAGER.int_lock();

    let struct_size = 48;

    for (i, task_opt) in tm.tasks.iter().enumerate() {
        if let Some(task) = task_opt {
            if task.state != crate::interrupts::task::ThreadState::Null {
                if count >= max_count {
                    break;
                }

                let offset = count * struct_size;
                unsafe {
                    let ptr = buf_ptr.add(offset);
                    *(ptr as *mut u64) = i as u64;
                    *(ptr.add(8) as *mut u64) = match task.state {
                        crate::interrupts::task::ThreadState::Null => 0,
                        crate::interrupts::task::ThreadState::Reserved => 1,
                        crate::interrupts::task::ThreadState::Ready => 2,
                        crate::interrupts::task::ThreadState::Zombie => 3,
                        crate::interrupts::task::ThreadState::Sleeping => 4,
                        _ => 0,
                    };

                    let name_ptr = ptr.add(16);
                    core::ptr::copy_nonoverlapping(task.name.as_ptr(), name_ptr, 32);
                }
                count += 1;
            }
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

    let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
    if let Some(current) = tm.current_task_idx() {
        if let Some(thread) = tm.tasks[current].as_ref() {
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
    let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
    let current = tm.current_task;

    if current >= 0 {
        if let Some(task) = tm.tasks[current as usize].as_mut() {
            task.wake_ticks = unsafe { crate::interrupts::task::SYSTEM_TICKS } + duration;
            task.state = crate::interrupts::task::ThreadState::Sleeping;
        }
    }
}

pub fn handle_spawn_thread(context: &mut CPUState) {
    let entry = context.rdi;
    let stack = context.rsi;
    let arg = context.rdx;

    let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
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
        let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
        let current = tm.current_task;
        if current >= 0 {
            if let Some(task) = tm.tasks[current as usize].as_mut() {
                task.state = crate::interrupts::task::ThreadState::Zombie;
                task.exit_code = 0;
            }
        }
    }

    unsafe {
        core::arch::asm!("sti");
        loop { core::arch::asm!("hlt"); }
    }
}
