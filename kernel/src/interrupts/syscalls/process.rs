use crate::interrupts::task::{CPUState, TASK_MANAGER, TaskState, MAX_TASKS, NULL_TASK};
use crate::window_manager::composer::COMPOSER;
use crate::{debugln, debug_print};
use alloc::vec::Vec;
use alloc::string::String;

pub fn sys_exit(context: &mut CPUState) {
    let exit_code = context.rdi;
    debugln!("[Syscall] Process exited with code {}", exit_code);
    {
        let mut tm = TASK_MANAGER.int_lock();
        let current = tm.current_task;
        if current >= 0 {
            let task = &mut tm.tasks[current as usize];
            task.exit_code = exit_code;
            task.state = TaskState::Zombie;
            unsafe {
                (*(&raw mut COMPOSER)).remove_windows_by_pid(current as u64);
            }
            for i in 0..16 {
                let global = task.fd_table[i];
                if global != -1 {
                    crate::fs::vfs::close_file(global as usize);
                    task.fd_table[i] = -1;
                }
            }
        }
    }
    unsafe {
        core::arch::asm!("sti");
        loop { core::arch::asm!("hlt"); }
    }
}

pub fn sys_spawn(context: &mut CPUState) {
    let path_ptr = context.rdi as *const u8;
    let path_len = context.rsi as usize;
    let fd_map_ptr = context.rdx as *const (u8, u8); 
    let fd_map_len = context.r10 as usize;

    if path_ptr.is_null() || path_len == 0 {
        context.rax = u64::MAX;
        return;
    }

    let path_slice = unsafe { core::slice::from_raw_parts(path_ptr, path_len) };
    let path_str = String::from_utf8_lossy(path_slice);

    let fd_map = if !fd_map_ptr.is_null() && fd_map_len > 0 {
        unsafe { Some(core::slice::from_raw_parts(fd_map_ptr, fd_map_len)) }
    } else {
        None
    };

    match spawn_process(&path_str, fd_map) {
        Ok(pid) => context.rax = pid,
        Err(e) => {
            debugln!("Spawn Error: {}", e);
            context.rax = u64::MAX;
        }
    }
}

pub fn sys_waitpid(context: &mut CPUState) {
    let target_pid = context.rdi as usize;
    let mut tm = TASK_MANAGER.int_lock();
    if target_pid < MAX_TASKS {
        match tm.tasks[target_pid].state {
            TaskState::Ready | TaskState::Reserved => {
                context.rax = u64::MAX; 
            }
            TaskState::Zombie => {
                let exit_code = tm.tasks[target_pid].exit_code;
                context.rax = exit_code;
                let pid = target_pid as u64;
                let k_stack_top = tm.tasks[target_pid].kernel_stack;
                crate::memory::pmm::free_frames_by_pid(pid);
                if k_stack_top != 0 {
                     let k_stack_start = k_stack_top - (4096 * 16);
                     crate::memory::pmm::free_frame(k_stack_start);
                }
                tm.tasks[target_pid] = NULL_TASK;
            }
            _ => {
                context.rax = 0; 
            }
        }
    } else {
        context.rax = 0;
    }
}

pub fn sys_kill(context: &mut CPUState) {
    let pid = context.rdi as u64;
    let mut tm = TASK_MANAGER.int_lock();
    tm.kill_process(pid, 9);
    context.rax = 0;
}

pub fn sys_get_process_list(context: &mut CPUState) {
    let buf_ptr = context.rdi as *mut u8;
    let max_count = context.rsi as usize;
    
    if buf_ptr.is_null() || max_count == 0 {
        context.rax = 0;
        return;
    }

    let mut count = 0;
    let tm = TASK_MANAGER.int_lock();
    let struct_size = 48; 

    for (i, task) in tm.tasks.iter().enumerate() {
        if task.state != TaskState::Null {
            if count >= max_count { break; }
            let offset = count * struct_size;
            unsafe {
                let ptr = buf_ptr.add(offset);
                *(ptr as *mut u64) = i as u64; 
                *(ptr.add(8) as *mut u64) = match task.state {
                     TaskState::Null => 0,
                     TaskState::Reserved => 1,
                     TaskState::Ready => 2,
                     TaskState::Zombie => 3,
                     TaskState::Sleeping => 4,
                };
                let name_ptr = ptr.add(16);
                core::ptr::copy_nonoverlapping(task.name.as_ptr(), name_ptr, 32);
            }
            count += 1;
        }
    }
    context.rax = count as u64;
}

pub fn spawn_process(path: &str, fd_inheritance: Option<&[(u8, u8)]>) -> Result<u64, String> {
    let path_parts: Vec<&str> = path.split('/').collect();
    if path_parts.len() < 1 || !path_parts[0].starts_with('@') {
        return Err(String::from("Invalid path format"));
    }

    let disk_part = &path_parts[0][1..];
    let disk_id = if disk_part.starts_with("0x") || disk_part.starts_with("0X") {
        u8::from_str_radix(&disk_part[2..], 16).unwrap_or(0xFF)
    } else {
        disk_part.parse::<u8>().unwrap_or(0xFF)
    };

    let actual_path = if path_parts.len() > 1 { path_parts[1..].join("/") } else { String::from("") };
    let process_name_str = if let Some(last_slash) = actual_path.rfind('/') {
        &actual_path[last_slash+1..]
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

    let pml4_phys = unsafe { (*(&raw const crate::boot::BOOT_INFO)).pml4 };
    let pid_idx = TASK_MANAGER.lock().reserve_pid().map_err(|_| String::from("No free process slots"))?;
    let pid = pid_idx as u64;

    match crate::fs::elf::load_elf(&file_buf, pml4_phys, pid) {
        Ok(entry_point) => {
            let mut new_fd_table = [-1i16; 16];
            let tm = TASK_MANAGER.int_lock();
            if tm.current_task >= 0 {
                let current_fds = tm.tasks[tm.current_task as usize].fd_table;
                if let Some(map) = fd_inheritance {
                    for &(child_fd, parent_fd) in map {
                        if (parent_fd as usize) < 16 && (child_fd as usize) < 16 {
                            new_fd_table[child_fd as usize] = current_fds[parent_fd as usize];
                        }
                    }
                } else {
                    new_fd_table = current_fds;
                }
            }
            drop(tm); 

            for &g_fd in new_fd_table.iter() {
                if g_fd != -1 {
                    crate::fs::vfs::increment_ref(g_fd as usize);
                }
            }

            let init_res = {
                let mut tm = TASK_MANAGER.int_lock();
                tm.init_user_task(pid_idx, entry_point, pml4_phys, None, Some(new_fd_table), process_name_bytes, None, None)
            };
            
            match init_res {
                Ok(_) => Ok(pid),
                Err(_) => Err(String::from("Failed to init task")),
            }
        },
        Err(e) => Err(e),
    }
}
