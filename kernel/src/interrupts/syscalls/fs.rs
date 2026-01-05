use crate::drivers::periferics::keyboard::KEYBOARD_BUFFER;
use crate::interrupts::task::CPUState;
use alloc::string::String;
use alloc::vec::Vec;

use super::{PollFd, POLLERR, POLLIN, POLLNVAL, POLLOUT};

pub fn resolve_path(cwd: &str, path: &str) -> String {
    let mut full_path = String::new();

    if path.starts_with('@') {
        full_path = String::from(path);
    } else if path.starts_with('/') {
        // Absolute path from boot disk
        full_path = alloc::format!("@0xE0{}", path);
    } else {
        // Relative path
        full_path = alloc::format!("{}{}", cwd, path);
    }

    let mut parts: Vec<&str> = Vec::new();
    for part in full_path.split('/') {
        if part.is_empty() || part == "." {
            continue;
        } else if part == ".." {
            if parts.len() > 1 {
                parts.pop();
            }
        } else {
            parts.push(part);
        }
    }

    let mut res = String::new();
    for (i, p) in parts.iter().enumerate() {
        if i > 0 { res.push('/'); }
        res.push_str(p);
    }
    
    // Ensure it starts with the disk identifier
    if !res.starts_with('@') && !parts.is_empty() {
        // This shouldn't happen with the logic above, but let's be safe
    }

    res
}

pub fn handle_read(context: &mut CPUState) {
    let _fd = context.rdi;
    let user_ptr = context.rsi as *mut u8;
    let user_len = context.rdx as usize;
    let mut bytes_written_to_user = 0;

    if user_ptr.is_null() {
        context.rax = 0;
        return;
    }

    loop {
        {
            let mut keyboard_buffer = KEYBOARD_BUFFER.lock();
            while bytes_written_to_user < user_len {
                if let Some(keycode) = keyboard_buffer.pop_front() {
                    unsafe {
                        *user_ptr.add(bytes_written_to_user) = keycode as u8;
                    }
                    bytes_written_to_user += 1;
                } else {
                    break;
                }
            }
        }

        if bytes_written_to_user > 0 {
            break;
        }

        // Wait for input
        unsafe {
            core::arch::asm!("int 0x81");
        }
    }
    
    context.rax = bytes_written_to_user as u64;
}

pub fn handle_poll(context: &mut CPUState) {
    let fds_ptr = context.rdi as *const PollFd;
    let nfds = context.rsi as usize;
    let _timeout = context.rdx as i32;

    if fds_ptr.is_null() || nfds == 0 {
        context.rax = 0;
        return;
    }

    let mut ready_count = 0;

    {
        let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
        let current = tm.current_task;

        if current >= 0 {
            let task = &tm.tasks[current as usize];

            for i in 0..nfds {
                unsafe {
                    let pfd = &mut *(fds_ptr.add(i) as *mut PollFd);
                    pfd.revents = 0;

                    let fd = pfd.fd;
                    if fd >= 0 && (fd as usize) < 16 {
                        let global_fd = task.fd_table[fd as usize];
                        if global_fd != -1 {
                            if let Some(handle) = crate::fs::vfs::get_file(global_fd as usize) {
                                use crate::fs::vfs::FileHandle;
                                match handle {
                                    FileHandle::Pipe { pipe } => {
                                        if (pfd.events & POLLIN) != 0 {
                                            if pipe.available() > 0 {
                                                pfd.revents |= POLLIN;
                                            }
                                        }
                                        if (pfd.events & POLLOUT) != 0 {
                                            pfd.revents |= POLLOUT;
                                        }
                                    }
                                    FileHandle::File { .. } => {
                                        if (pfd.events & POLLIN) != 0 { pfd.revents |= POLLIN; }
                                        if (pfd.events & POLLOUT) != 0 { pfd.revents |= POLLOUT; }
                                    }
                                }
                            } else {
                                pfd.revents = POLLERR;
                            }
                        } else {
                            pfd.revents = POLLNVAL;
                        }
                    } else {
                        pfd.revents = POLLNVAL;
                    }

                    if pfd.revents != 0 {
                        ready_count += 1;
                    }
                }
            }
        }
    }

    context.rax = ready_count as u64;
}

pub fn handle_chdir(context: &mut CPUState) {
    let ptr = context.rdi as *const u8;
    let len = context.rsi as usize;
    crate::debugln!("[SYS_CHDIR] ptr: {:p}, len: {}", ptr, len);

    let path_str_full = if ptr.is_null() || len == 0 {
        String::from("")
    } else {
        let s = unsafe { core::slice::from_raw_parts(ptr, len) };
        String::from_utf8_lossy(s).into_owned()
    };

    let cwd_str = {
        let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
        if tm.current_task >= 0 {
            let task = &tm.tasks[tm.current_task as usize];
            let cwd_len = task.cwd.iter().position(|&c| c == 0).unwrap_or(task.cwd.len());
            String::from_utf8_lossy(&task.cwd[..cwd_len]).into_owned()
        } else {
            String::from("@0xE0/")
        }
    };

    let resolved = resolve_path(&cwd_str, &path_str_full);

    let path_parts: Vec<&str> = resolved.split('/').collect();
    if path_parts.len() < 1 || !path_parts[0].starts_with('@') {
        context.rax = u64::MAX;
        return;
    }

    let disk_part = &path_parts[0][1..];
    let disk_id = if disk_part.starts_with("0x") || disk_part.starts_with("0X") {
        u8::from_str_radix(&disk_part[2..], 16).unwrap_or(0xFF)
    } else {
        disk_part.parse::<u8>().unwrap_or_else(|_| u8::from_str_radix(disk_part, 16).unwrap_or(0xFF))
    };

    let actual_path = if path_parts.len() > 1 { path_parts[1..].join("/") } else { String::from("") };

    if let Ok(node) = crate::fs::vfs::open(disk_id, &actual_path) {
        use crate::fs::vfs::FileType;
        if node.kind() == FileType::Directory {
            let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
            let current_idx = tm.current_task as usize;
            if tm.current_task >= 0 {
                let task = &mut tm.tasks[current_idx];
                task.cwd.fill(0);
                let bytes = resolved.as_bytes();
                let len = core::cmp::min(bytes.len(), 127);
                task.cwd[..len].copy_from_slice(&bytes[..len]);
                if !resolved.ends_with('/') {
                    if len < 127 {
                        task.cwd[len] = b'/';
                    }
                }
                context.rax = 0;
            } else {
                context.rax = u64::MAX;
            }
        } else {
            context.rax = u64::MAX;
        }
    } else {
        context.rax = u64::MAX;
    }
}

pub fn handle_create(context: &mut CPUState, syscall_num: u64) {
    let ptr = context.rdi as *const u8;
    let len = context.rsi as usize;
    crate::debugln!("[SYS_CREATE] ptr: {:p}, len: {}, type: {}", ptr, len, syscall_num);
    let s = unsafe { core::slice::from_raw_parts(ptr, len) };
    let path_str_full = String::from_utf8_lossy(s);

    let cwd_str = {
        let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
        if tm.current_task >= 0 {
            let task = &tm.tasks[tm.current_task as usize];
            let cwd_len = task.cwd.iter().position(|&c| c == 0).unwrap_or(task.cwd.len());
            String::from_utf8_lossy(&task.cwd[..cwd_len]).into_owned()
        } else {
            String::from("@0xE0/")
        }
    };

    let resolved = resolve_path(&cwd_str, &path_str_full);

    let path_parts: Vec<&str> = resolved.split('/').collect();
    if path_parts.len() < 1 || !path_parts[0].starts_with('@') {
        context.rax = u64::MAX;
        return;
    }

    let disk_part = &path_parts[0][1..];
    let disk_id = if disk_part.starts_with("0x") || disk_part.starts_with("0X") {
        u8::from_str_radix(&disk_part[2..], 16).unwrap_or(0xFF)
    } else {
        disk_part.parse::<u8>().unwrap_or_else(|_| u8::from_str_radix(disk_part, 16).unwrap_or(0xFF))
    };

    if disk_id == 0xFF {
        context.rax = u64::MAX;
        return;
    }

    let actual_path = if path_parts.len() > 1 { path_parts[1..].join("/") } else { String::from("") };

    if let Some(last_slash) = actual_path.rfind('/') {
        let parent_path = &actual_path[..last_slash];
        let new_name = &actual_path[last_slash + 1..];

        if let Ok(mut parent) = crate::fs::vfs::open(disk_id, parent_path) {
            let res = if syscall_num == 83 {
                parent.create_dir(new_name)
            } else {
                parent.create_file(new_name)
            };

            match res {
                Ok(_) => context.rax = 0,
                Err(_) => context.rax = u64::MAX,
            }
        } else {
            context.rax = u64::MAX;
        }
    } else {
        if let Ok(mut root) = crate::fs::vfs::open(disk_id, "") {
            let res = if syscall_num == 83 {
                root.create_dir(&actual_path)
            } else {
                root.create_file(&actual_path)
            };

            match res {
                Ok(_) => context.rax = 0,
                Err(_) => context.rax = u64::MAX,
            }
        } else {
            context.rax = u64::MAX;
        }
    }
}

pub fn handle_remove(context: &mut CPUState) {
    let ptr = context.rdi as *const u8;
    let len = context.rsi as usize;
    crate::debugln!("[SYS_REMOVE] ptr: {:p}, len: {}", ptr, len);
    let s = unsafe { core::slice::from_raw_parts(ptr, len) };
    let path_str_full = String::from_utf8_lossy(s);

    let cwd_str = {
        let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
        if tm.current_task >= 0 {
            let task = &tm.tasks[tm.current_task as usize];
            let cwd_len = task.cwd.iter().position(|&c| c == 0).unwrap_or(task.cwd.len());
            String::from_utf8_lossy(&task.cwd[..cwd_len]).into_owned()
        } else {
            String::from("@0xE0/")
        }
    };

    let resolved = resolve_path(&cwd_str, &path_str_full);

    let path_parts: Vec<&str> = resolved.split('/').collect();
    if path_parts.len() < 1 || !path_parts[0].starts_with('@') {
        context.rax = u64::MAX;
        return;
    }

    let disk_part = &path_parts[0][1..];
    let disk_id = if disk_part.starts_with("0x") || disk_part.starts_with("0X") {
        u8::from_str_radix(&disk_part[2..], 16).unwrap_or(0xFF)
    } else {
        disk_part.parse::<u8>().unwrap_or_else(|_| u8::from_str_radix(disk_part, 16).unwrap_or(0xFF))
    };

    let actual_path = if path_parts.len() > 1 { path_parts[1..].join("/") } else { String::from("") };

    if let Some(last_slash) = actual_path.rfind('/') {
        let parent_path = &actual_path[..last_slash];
        let name = &actual_path[last_slash + 1..];

        if let Ok(mut parent) = crate::fs::vfs::open(disk_id, parent_path) {
            match parent.remove(name) {
                Ok(_) => context.rax = 0,
                Err(_) => context.rax = u64::MAX,
            }
        } else { context.rax = u64::MAX; }
    } else {
        if let Ok(mut root) = crate::fs::vfs::open(disk_id, "") {
            match root.remove(&actual_path) {
                Ok(_) => context.rax = 0,
                Err(_) => context.rax = u64::MAX,
            }
        } else { context.rax = u64::MAX; }
    }
}

pub fn handle_rename(context: &mut CPUState) {
    let old_ptr = context.rdi as *const u8;
    let old_len = context.rsi as usize;
    let new_ptr = context.rdx as *const u8;
    let new_len = context.r10 as usize;
    crate::debugln!("[SYS_RENAME] old_ptr: {:p}, old_len: {}, new_ptr: {:p}, new_len: {}", old_ptr, old_len, new_ptr, new_len);

    let s_old = unsafe { core::slice::from_raw_parts(old_ptr, old_len) };
    let s_new = unsafe { core::slice::from_raw_parts(new_ptr, new_len) };
    let path_old = String::from_utf8_lossy(s_old);
    let path_new = String::from_utf8_lossy(s_new);

    let cwd_str = {
        let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
        if tm.current_task >= 0 {
            let task = &tm.tasks[tm.current_task as usize];
            let cwd_len = task.cwd.iter().position(|&c| c == 0).unwrap_or(task.cwd.len());
            String::from_utf8_lossy(&task.cwd[..cwd_len]).into_owned()
        } else {
            String::from("@0xE0/")
        }
    };

    let resolved_old = resolve_path(&cwd_str, &path_old);
    let resolved_new = resolve_path(&cwd_str, &path_new);

    let parts_old: Vec<&str> = resolved_old.split('/').collect();
    if parts_old.len() < 1 || !parts_old[0].starts_with('@') {
        context.rax = u64::MAX;
        return;
    }

    let disk_part = &parts_old[0][1..];
    let disk_id = if disk_part.starts_with("0x") || disk_part.starts_with("0X") {
        u8::from_str_radix(&disk_part[2..], 16).unwrap_or(0xFF)
    } else {
        disk_part.parse::<u8>().unwrap_or_else(|_| u8::from_str_radix(disk_part, 16).unwrap_or(0xFF))
    };
    let actual_old = if parts_old.len() > 1 { parts_old[1..].join("/") } else { String::from("") };

    let parts_new: Vec<&str> = resolved_new.split('/').collect();
    let actual_new = if parts_new.len() > 1 { parts_new[1..].join("/") } else { String::from("") };

    let (parent_old, name_old) = if let Some(idx) = actual_old.rfind('/') {
        (&actual_old[..idx], &actual_old[idx + 1..])
    } else {
        ("", actual_old.as_str())
    };

    let (parent_new, name_new) = if let Some(idx) = actual_new.rfind('/') {
        (&actual_new[..idx], &actual_new[idx + 1..])
    } else {
        ("", actual_new.as_str())
    };

    if parent_old != parent_new {
        crate::debugln!("SYS_RENAME: Moving between directories not supported yet");
        context.rax = u64::MAX;
        return;
    }

    if let Ok(mut parent) = crate::fs::vfs::open(disk_id, parent_old) {
        match parent.rename(name_old, name_new) {
            Ok(_) => context.rax = 0,
            Err(_) => context.rax = u64::MAX,
        }
    } else {
        context.rax = u64::MAX;
    }
}

pub fn handle_open(context: &mut CPUState) {
    let ptr = context.rdi as *const u8;
    let len = context.rsi as usize;
    crate::debugln!("[SYS_OPEN] ptr: {:p}, len: {}", ptr, len);
    let s = unsafe { core::slice::from_raw_parts(ptr, len) };
    let path_str_full = String::from_utf8_lossy(s);

    let cwd_str = {
        let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
        if tm.current_task >= 0 {
            let task = &tm.tasks[tm.current_task as usize];
            let cwd_len = task.cwd.iter().position(|&c| c == 0).unwrap_or(task.cwd.len());
            String::from_utf8_lossy(&task.cwd[..cwd_len]).into_owned()
        } else {
            String::from("@0xE0/")
        }
    };

    let resolved = resolve_path(&cwd_str, &path_str_full);

    // Extract disk_id and path from resolved string
    let path_parts: Vec<&str> = resolved.split('/').collect();
    let disk_part = &path_parts[0][1..];
    let disk_id = if disk_part.starts_with("0x") || disk_part.starts_with("0X") {
        u8::from_str_radix(&disk_part[2..], 16).unwrap_or(0xFF)
    } else {
        disk_part.parse::<u8>().unwrap_or_else(|_| u8::from_str_radix(disk_part, 16).unwrap_or(0xFF))
    };
    let actual_path_str = if path_parts.len() > 1 { path_parts[1..].join("/") } else { String::from("") };

    match crate::fs::vfs::open_file(disk_id, &actual_path_str) {
        Ok(global_fd) => {
            let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
            let current = tm.current_task;
            if current >= 0 {
                let task = &mut tm.tasks[current as usize];

                let mut local_fd = -1;
                for i in 0..16 {
                    if task.fd_table[i] == -1 {
                        local_fd = i as i32;
                        break;
                    }
                }

                if local_fd != -1 {
                    task.fd_table[local_fd as usize] = global_fd as i16;
                    context.rax = local_fd as u64;
                } else {
                    context.rax = u64::MAX;
                }
            } else {
                context.rax = global_fd as u64;
            }
        }
        Err(_) => context.rax = u64::MAX,
    }
}

pub fn handle_read_file(context: &mut CPUState) {
    let local_fd = context.rdi as usize;
    let buf_ptr = context.rsi as *mut u8;
    let len = context.rdx as usize;
    if local_fd > 3 {
        crate::debugln!("[SYS_READ_FILE] fd: {}, ptr: {:p}, len: {}", local_fd, buf_ptr, len);
    }

    let global_fd_opt = {
        let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
        let current = tm.current_task;
        if current >= 0 && local_fd < 16 {
            let g = tm.tasks[current as usize].fd_table[local_fd];
            if g != -1 { Some(g as usize) } else { None }
        } else {
            None
        }
    };

    if let Some(fd) = global_fd_opt {
        let buf = unsafe { core::slice::from_raw_parts_mut(buf_ptr, len) };

        if let Some(handle) = crate::fs::vfs::get_file(fd) {
            use crate::fs::vfs::FileHandle;
            match handle {
                FileHandle::File { node, offset } => {
                    match node.read(*offset, buf) {
                        Ok(n) => {
                            *offset += n as u64;
                            context.rax = n as u64;
                        }
                        Err(_) => context.rax = u64::MAX,
                    }
                }
                FileHandle::Pipe { pipe } => {
                    let n = pipe.read(buf);
                    context.rax = n as u64;
                }
            }
        } else {
            context.rax = u64::MAX;
        }
        return;
    }

    if local_fd == 0 {
        handle_read(context);
        return;
    }

    context.rax = u64::MAX;
}

pub fn handle_write_file(context: &mut CPUState) {
    let local_fd = context.rdi as usize;
    let buf_ptr = context.rsi as *const u8;
    let len = context.rdx as usize;
    if local_fd > 2 {
        crate::debugln!("[SYS_WRITE_FILE] fd: {}, ptr: {:p}, len: {}", local_fd, buf_ptr, len);
    }

    let global_fd_opt = {
        let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
        let current = tm.current_task;
        if current >= 0 && local_fd < 16 {
            let g = tm.tasks[current as usize].fd_table[local_fd];
            if g != -1 { Some(g as usize) } else { None }
        } else {
            None
        }
    };

    if let Some(fd) = global_fd_opt {
        let buf = unsafe { core::slice::from_raw_parts(buf_ptr, len) };

        if let Some(handle) = crate::fs::vfs::get_file(fd) {
            use crate::fs::vfs::FileHandle;
            match handle {
                FileHandle::File { node, offset } => {
                    match node.write(*offset, buf) {
                        Ok(n) => {
                            *offset += n as u64;
                            context.rax = n as u64;
                        }
                        Err(_) => context.rax = u64::MAX,
                    }
                }
                FileHandle::Pipe { pipe } => {
                    let n = pipe.write(buf);
                    context.rax = n as u64;
                }
            }
        } else {
            context.rax = u64::MAX;
        }
        return;
    }

    if local_fd == 1 || local_fd == 2 {
        // By default, stdout/stderr do nothing if not redirected.
        // User wants NEVER EVER serial for println.
        context.rax = len as u64;
        return;
    }

    context.rax = u64::MAX;
}

pub fn handle_read_dir(context: &mut CPUState) {
    let local_fd = context.rdi as usize;
    let buf_ptr = context.rsi as *mut u8;
    let len = context.rdx as usize;
    crate::debugln!("[SYS_READ_DIR] fd: {}, ptr: {:p}, len: {}", local_fd, buf_ptr, len);

    if buf_ptr.is_null() {
        context.rax = u64::MAX;
        return;
    }

    let global_fd_opt = {
        let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
        let current = tm.current_task;
        if current >= 0 && local_fd < 16 {
            let g = tm.tasks[current as usize].fd_table[local_fd];
            if g != -1 { Some(g as usize) } else { None }
        } else {
            None
        }
    };

    if let Some(fd) = global_fd_opt {
        if let Some(handle) = crate::fs::vfs::get_file(fd) {
            use crate::fs::vfs::FileHandle;
            match handle {
                FileHandle::File { node, offset } => {
                    let buf = unsafe { core::slice::from_raw_parts_mut(buf_ptr, len) };
                    match node.read_dir(*offset, buf) {
                        Ok((bytes_written, count_read)) => {
                            *offset += count_read as u64;
                            context.rax = bytes_written as u64;
                        }
                        Err(_) => context.rax = u64::MAX,
                    }
                }
                FileHandle::Pipe { .. } => context.rax = u64::MAX,
            }
        } else {
            context.rax = u64::MAX;
        }
    } else {
        context.rax = u64::MAX;
    }
}

pub fn handle_file_size(context: &mut CPUState) {
    let local_fd = context.rdi as usize;

    let global_fd_opt = {
        let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
        let current = tm.current_task;
        if current >= 0 && local_fd < 16 {
            let g = tm.tasks[current as usize].fd_table[local_fd];
            if g != -1 { Some(g as usize) } else { None }
        } else {
            None
        }
    };

    if let Some(fd) = global_fd_opt {
        if let Some(handle) = crate::fs::vfs::get_file(fd) {
            use crate::fs::vfs::FileHandle;
            match handle {
                FileHandle::File { node, .. } => context.rax = node.size(),
                FileHandle::Pipe { .. } => context.rax = 0,
            }
        } else {
            context.rax = u64::MAX;
        }
    } else {
        context.rax = u64::MAX;
    }
}

pub fn handle_pipe(context: &mut CPUState) {
    let fds_ptr = context.rdi as *mut i32;
    if fds_ptr.is_null() {
        context.rax = u64::MAX;
        return;
    }

    unsafe {
        use crate::fs::vfs::{FileHandle, GLOBAL_FILE_REFCOUNT, OPEN_FILES};
        use crate::fs::pipe::Pipe;

        let mut g1 = -1;
        let mut g2 = -1;

        for i in 3..256 {
            if OPEN_FILES[i].is_none() {
                if g1 == -1 {
                    g1 = i as i32;
                } else {
                    g2 = i as i32;
                    break;
                }
            }
        }

        if g1 != -1 && g2 != -1 {
            let mut l1 = -1;
            let mut l2 = -1;

            let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
            let current = tm.current_task;
            if current >= 0 {
                let task = &mut tm.tasks[current as usize];
                for i in 0..16 {
                    if task.fd_table[i] == -1 {
                        if l1 == -1 {
                            l1 = i as i32;
                        } else {
                            l2 = i as i32;
                            break;
                        }
                    }
                }
            }

            if l1 != -1 && l2 != -1 {
                let pipe = Pipe::new();
                OPEN_FILES[g1 as usize] = Some(FileHandle::Pipe { pipe: pipe.clone() });
                OPEN_FILES[g2 as usize] = Some(FileHandle::Pipe { pipe });
                GLOBAL_FILE_REFCOUNT[g1 as usize] = 1;
                GLOBAL_FILE_REFCOUNT[g2 as usize] = 1;

                let task = &mut tm.tasks[current as usize];
                task.fd_table[l1 as usize] = g1 as i16;
                task.fd_table[l2 as usize] = g2 as i16;

                *fds_ptr.add(0) = l1;
                *fds_ptr.add(1) = l2;
                context.rax = 0;
            } else {
                context.rax = u64::MAX;
            }
        } else {
            context.rax = u64::MAX;
        }
    }
}

pub fn handle_close(context: &mut CPUState) {
    let local_fd = context.rdi as usize;

    let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
    let current = tm.current_task;
    if current >= 0 {
        let task = &mut tm.tasks[current as usize];
        if local_fd < 16 {
            let global = task.fd_table[local_fd];
            if global != -1 {
                crate::fs::vfs::close_file(global as usize);
                task.fd_table[local_fd] = -1;
                context.rax = 0;
            } else {
                context.rax = u64::MAX;
            }
        } else {
            context.rax = u64::MAX;
        }
    } else {
        context.rax = u64::MAX;
    }
}

pub fn handle_seek(context: &mut CPUState) {
    let local_fd = context.rdi as usize;
    let offset = context.rsi as i64;
    let whence = context.rdx as usize;

    let global_fd_opt = {
        let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
        let current = tm.current_task;
        if current >= 0 && local_fd < 16 {
            let g = tm.tasks[current as usize].fd_table[local_fd];
            if g != -1 { Some(g as usize) } else { None }
        } else {
            None
        }
    };

    if let Some(fd) = global_fd_opt {
        if let Some(handle) = crate::fs::vfs::get_file(fd) {
            use crate::fs::vfs::FileHandle;
            match handle {
                FileHandle::File { node, offset: current_offset } => {
                    let size = node.size() as i64;
                    let new_offset = match whence {
                        0 => offset,
                        1 => (*current_offset as i64) + offset,
                        2 => size + offset,
                        _ => -1,
                    };

                    if new_offset >= 0 {
                        *current_offset = new_offset as u64;
                        context.rax = new_offset as u64;
                    } else {
                        context.rax = u64::MAX;
                    }
                }
                FileHandle::Pipe { .. } => context.rax = u64::MAX,
            }
        } else {
            context.rax = u64::MAX;
        }
    } else {
        context.rax = u64::MAX;
    }
}