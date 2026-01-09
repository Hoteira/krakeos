use crate::interrupts::syscalls::fs::resolve_path;
use crate::interrupts::task::CPUState;
use crate::debugln;
use crate::memory::paging;
use alloc::string::String;
use alloc::vec::Vec;

pub fn spawn_process(path: &str, args: Option<&[&str]>, fd_inheritance: Option<&[(u8, u8)]>) -> Result<u64, String> {
// ... (rest of spawn_process remains the same) ...

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

    
    let pid_idx = crate::interrupts::task::TASK_MANAGER.lock().reserve_pid().map_err(|_| String::from("No free process slots"))?;
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

    
    {
        let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
        
        tm.init_user_task(pid_idx, 0, 0, args, Some(new_fd_table), process_name_bytes, term_size).map_err(|_| String::from("Failed to init task"))?;
    }

    
    let target_pml4_phys = {
        let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
        tm.tasks[pid_idx].as_ref().unwrap().process.as_ref().unwrap().pml4_phys
    };

    match crate::fs::elf::load_elf(&file_buf, target_pml4_phys, pid) {
        Ok(entry_point) => {
            
            let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
            let task = tm.tasks[pid_idx].as_mut().unwrap();
            
            
            unsafe {
                let cpu_state = &mut *(task.cpu_state_ptr as *mut crate::interrupts::task::CPUState);
                cpu_state.rip = entry_point;
            }
            
            Ok(pid)
        }
        Err(e) => {
            
            crate::interrupts::task::TASK_MANAGER.lock().kill_process(pid);
            Err(e)
        }
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
                thread.state = crate::interrupts::task::TaskState::Zombie;

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

    let path_slice = unsafe { core::slice::from_raw_parts(path_ptr, path_len) };
    let path_str = String::from_utf8_lossy(path_slice);

    
    let mut args_vec = Vec::new();
    if !args_ptr.is_null() && args_len > 0 {
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
        unsafe { Some(core::slice::from_raw_parts(fd_map_ptr, fd_map_len)) }
    } else {
        None
    };

    match spawn_process(&path_str, args_opt, fd_map) {
        Ok(pid) => context.rax = pid,
        Err(e) => {
            crate::debugln!("Spawn Error: {}", e);
            context.rax = u64::MAX;
        }
    }
}

pub fn handle_kill(context: &mut CPUState) {
    let pid = context.rdi as u64;
    let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
    tm.kill_process(pid);
    context.rax = 0;
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
            crate::interrupts::task::TaskState::Zombie => {
                let exit_code = task.exit_code;
                context.rax = exit_code;

                let pid = target_pid as u64;
                let k_stack_top = task.kernel_stack;

                crate::memory::pmm::free_frames_by_pid(pid);

                if k_stack_top != 0 {
                    let k_stack_start = k_stack_top - (1024 * 1024 + paging::HHDM_OFFSET);
                    crate::memory::pmm::free_frame(k_stack_start);
                }

                *task_opt = crate::interrupts::task::NULL_TASK;
            }
            crate::interrupts::task::TaskState::Null => {
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

    let mut count = 0;
    let tm = crate::interrupts::task::TASK_MANAGER.int_lock();

    let struct_size = 48;

    for (i, task_opt) in tm.tasks.iter().enumerate() {
        if let Some(task) = task_opt {
            if task.state != crate::interrupts::task::TaskState::Null {
                if count >= max_count {
                    break;
                }

                let offset = count * struct_size;
                unsafe {
                    let ptr = buf_ptr.add(offset);
                    *(ptr as *mut u64) = i as u64;
                    *(ptr.add(8) as *mut u64) = match task.state {
                        crate::interrupts::task::TaskState::Null => 0,
                        crate::interrupts::task::TaskState::Reserved => 1,
                        crate::interrupts::task::TaskState::Ready => 2,
                        crate::interrupts::task::TaskState::Zombie => 3,
                        crate::interrupts::task::TaskState::Sleeping => 4,
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

pub fn handle_sleep(context: &mut CPUState) {
    let duration = context.rdi;
    let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
    let current = tm.current_task;

    if current >= 0 {
        if let Some(task) = tm.tasks[current as usize].as_mut() {
            task.wake_ticks = unsafe { crate::interrupts::task::SYSTEM_TICKS } + duration;
            task.state = crate::interrupts::task::TaskState::Sleeping;
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
    debugln!("[Syscall] Thread exited");
    {
        let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
        let current = tm.current_task;
        if current >= 0 {
            if let Some(task) = tm.tasks[current as usize].as_mut() {
                task.state = crate::interrupts::task::TaskState::Zombie;
                task.exit_code = 0; 
            }
        }
    }

    unsafe {
        core::arch::asm!("sti");
        loop { core::arch::asm!("hlt"); }
    }
}