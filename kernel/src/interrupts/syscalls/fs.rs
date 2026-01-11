use crate::drivers::periferics::keyboard::KEYBOARD_BUFFER;
use crate::interrupts::task::CPUState;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;

use super::{PollFd, POLLERR, POLLIN, POLLNVAL, POLLOUT};


pub fn copy_string_from_user(ptr: *const u8, len: usize) -> String {
    if ptr.is_null() || len == 0 {
        return String::new();
    }

    unsafe {
        let slice = core::slice::from_raw_parts(ptr, len);
        let s = String::from_utf8_lossy(slice).into_owned();
        s.trim_matches('\0').to_string()
    }
}

pub fn resolve_path(cwd: &str, path: &str) -> String {
    let mut full_path = String::new();

    if path.starts_with('@') {
        full_path = String::from(path);
    } else if path.starts_with('/') {
        full_path = alloc::format!("@0xE0{}", path);
    } else {
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
            if let Some(thread) = tm.tasks[current as usize].as_ref() {
                let proc = thread.process.as_ref().expect("Thread has no process");
                let fd_table = proc.fd_table.lock();

                for i in 0..nfds {
                    unsafe {
                        let pfd = &mut *(fds_ptr.add(i) as *mut PollFd);
                        pfd.revents = 0;

                        let fd = pfd.fd;
                        if fd >= 0 && (fd as usize) < 16 {
                            let global_fd = fd_table[fd as usize];
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
    }

    context.rax = ready_count as u64;
}

pub fn handle_chdir(context: &mut CPUState) {
    let ptr = context.rdi as *const u8;
    let len = context.rsi as usize;

    let path_str_full = copy_string_from_user(ptr, len);

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
                if let Some(thread) = tm.tasks[current_idx].as_mut() {
                    let proc = thread.process.as_ref().expect("Thread has no process");
                    let mut cwd = proc.cwd.lock();
                    cwd.fill(0);
                    let bytes = resolved.as_bytes();
                    let len = core::cmp::min(bytes.len(), 127);
                    cwd[..len].copy_from_slice(&bytes[..len]);
                    if !resolved.ends_with('/') {
                        if len < 127 {
                            cwd[len] = b'/';
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
    } else {
        context.rax = u64::MAX;
    }
}

pub fn handle_create(context: &mut CPUState, syscall_num: u64) {
    let ptr = context.rdi as *const u8;
    let len = context.rsi as usize;

    let path_str_full = copy_string_from_user(ptr, len);

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

    let path_str_full = copy_string_from_user(ptr, len);

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

    let path_old = copy_string_from_user(old_ptr, old_len);
    let path_new = copy_string_from_user(new_ptr, new_len);

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

    let path_str_full = copy_string_from_user(ptr, len);

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

    let resolved = resolve_path(&cwd_str, &path_str_full);


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
                if let Some(thread) = tm.tasks[current as usize].as_mut() {
                    let proc = thread.process.as_ref().expect("Thread has no process");
                    let mut fd_table = proc.fd_table.lock();

                    let mut local_fd = -1;
                    for i in 0..16 {
                        if fd_table[i] == -1 {
                            local_fd = i as i32;
                            break;
                        }
                    }

                    if local_fd != -1 {
                        fd_table[local_fd as usize] = global_fd as i16;
                        context.rax = local_fd as u64;
                    } else {
                        context.rax = u64::MAX;
                    }
                } else {
                    context.rax = u64::MAX;
                }
            } else {
                context.rax = global_fd as u64;
            }
        }
        Err(_) => {
            context.rax = u64::MAX;
        }
    }
}

pub fn handle_read_file(context: &mut CPUState) {
    let local_fd = context.rdi as usize;
    let buf_ptr = context.rsi as *mut u8;
    let len = context.rdx as usize;

    let global_fd_opt = {
        let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
        let current = tm.current_task;
        if current >= 0 && local_fd < 16 {
            if let Some(thread) = tm.tasks[current as usize].as_ref() {
                let proc = thread.process.as_ref().expect("Thread has no process");
                Some(proc.fd_table.lock()[local_fd])
            } else { None }
        } else {
            None
        }
    };

    if let Some(fd_val) = global_fd_opt {
        if fd_val == -1 {
            if local_fd == 0 {
                handle_read(context);
                return;
            }
            context.rax = u64::MAX;
            return;
        }
        let fd = fd_val as usize;
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

    context.rax = u64::MAX;
}

pub fn handle_write_file(context: &mut CPUState) {
    let local_fd = context.rdi as usize;
    let buf_ptr = context.rsi as *const u8;
    let len = context.rdx as usize;

    let global_fd_opt = {
        let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
        let current = tm.current_task;
        if current >= 0 && local_fd < 16 {
            if let Some(thread) = tm.tasks[current as usize].as_ref() {
                let proc = thread.process.as_ref().expect("Thread has no process");
                Some(proc.fd_table.lock()[local_fd])
            } else { None }
        } else {
            None
        }
    };

    if let Some(fd_val) = global_fd_opt {
        if fd_val == -1 {
            if local_fd == 1 || local_fd == 2 {
                context.rax = len as u64;
                return;
            }
            context.rax = u64::MAX;
            return;
        }
        let fd = fd_val as usize;
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

    context.rax = u64::MAX;
}

pub fn handle_read_dir(context: &mut CPUState) {
    let local_fd = context.rdi as usize;
    let buf_ptr = context.rsi as *mut u8;
    let len = context.rdx as usize;

    if buf_ptr.is_null() {
        context.rax = u64::MAX;
        return;
    }

    let global_fd_opt = {
        let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
        let current = tm.current_task;
        if current >= 0 && local_fd < 16 {
            if let Some(thread) = tm.tasks[current as usize].as_ref() {
                let proc = thread.process.as_ref().expect("Thread has no process");
                Some(proc.fd_table.lock()[local_fd])
            } else { None }
        } else {
            None
        }
    };

    if let Some(fd_val) = global_fd_opt {
        if fd_val == -1 {
            context.rax = u64::MAX;
            return;
        }
        let fd = fd_val as usize;
        if let Some(handle) = crate::fs::vfs::get_file(fd) {
            use crate::fs::vfs::FileHandle;
            match handle {
                FileHandle::File { node, offset } => {
                    if node.kind() != crate::fs::vfs::FileType::Directory {
                        context.rax = u64::MAX;
                        return;
                    }
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
    let syscall_num = context.rax;

    if syscall_num == 4 { // SYS_STAT (path)
        let ptr = context.rdi as *const u8;
        let len = context.rsi as usize;
        let path_str_full = copy_string_from_user(ptr, len);

        let cwd_str = {
            let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
            if tm.current_task >= 0 {
                if let Some(thread) = tm.tasks[tm.current_task as usize].as_ref() {
                    let proc = thread.process.as_ref().expect("Thread has no process");
                    let cwd = proc.cwd.lock();
                    let cwd_len = cwd.iter().position(|&c| c == 0).unwrap_or(cwd.len());
                    String::from_utf8_lossy(&cwd[..cwd_len]).into_owned()
                } else { String::from("@0xE0/") }
            } else { String::from("@0xE0/") }
        };

        let resolved = resolve_path(&cwd_str, &path_str_full);
        let path_parts: Vec<&str> = resolved.split('/').collect();
        let disk_part = &path_parts[0][1..];
        let disk_id = if disk_part.starts_with("0x") || disk_part.starts_with("0X") {
            u8::from_str_radix(&disk_part[2..], 16).unwrap_or(0xFF)
        } else {
            disk_part.parse::<u8>().unwrap_or_else(|_| u8::from_str_radix(disk_part, 16).unwrap_or(0xFF))
        };
        let actual_path = if path_parts.len() > 1 { path_parts[1..].join("/") } else { String::from("") };

        if let Ok(node) = crate::fs::vfs::open(disk_id, &actual_path) {
            context.rax = node.size();
        } else {
            context.rax = u64::MAX;
        }
        return;
    }

    // SYS_FSTAT (FD)
    let local_fd = context.rdi as usize;

    let global_fd_opt = {
        let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
        let current = tm.current_task;
        if current >= 0 && local_fd < 16 {
            if let Some(thread) = tm.tasks[current as usize].as_ref() {
                let proc = thread.process.as_ref().expect("Thread has no process");
                Some(proc.fd_table.lock()[local_fd])
            } else { None }
        } else {
            None
        }
    };

    if let Some(fd_val) = global_fd_opt {
        if fd_val == -1 {
            context.rax = u64::MAX;
            return;
        }
        let fd = fd_val as usize;
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

pub fn handle_ftruncate(context: &mut CPUState) {
    let local_fd = context.rdi as usize;
    let length = context.rsi as u64;

    let global_fd_opt = {
        let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
        let current = tm.current_task;
        if current >= 0 && local_fd < 16 {
            if let Some(thread) = tm.tasks[current as usize].as_ref() {
                let proc = thread.process.as_ref().expect("Thread has no process");
                Some(proc.fd_table.lock()[local_fd])
            } else { None }
        } else {
            None
        }
    };

    if let Some(fd_val) = global_fd_opt {
        if fd_val == -1 {
            context.rax = u64::MAX;
            return;
        }
        let fd = fd_val as usize;
        if let Some(handle) = crate::fs::vfs::get_file(fd) {
            use crate::fs::vfs::FileHandle;
            match handle {
                FileHandle::File { node, .. } => {
                    match node.truncate(length) {
                        Ok(_) => context.rax = 0,
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

pub fn handle_pipe(context: &mut CPUState) {
    let fds_ptr = context.rdi as *mut i32;
    if fds_ptr.is_null() {
        context.rax = u64::MAX;
        return;
    }

    use crate::fs::vfs::{FileHandle, GLOBAL_FILE_REFCOUNT, OPEN_FILES};
    use crate::fs::pipe::Pipe;

    let mut g1 = -1;
    let mut g2 = -1;

    {
        for i in 3..256 {
            unsafe {
                if OPEN_FILES[i].is_none() {
                    if g1 == -1 {
                        g1 = i as i32;
                    } else {
                        g2 = i as i32;
                        break;
                    }
                }
            }
        }

        if g1 != -1 && g2 != -1 {
            let mut l1 = -1;
            let mut l2 = -1;

            let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
            let current = tm.current_task;
            if current >= 0 {
                if let Some(thread) = tm.tasks[current as usize].as_mut() {
                    let proc = thread.process.as_ref().expect("Thread has no process");
                    let mut fd_table = proc.fd_table.lock();
                    for i in 0..16 {
                        if fd_table[i] == -1 {
                            if l1 == -1 {
                                l1 = i as i32;
                            } else {
                                l2 = i as i32;
                                break;
                            }
                        }
                    }

                    if l1 != -1 && l2 != -1 {
                        let pipe = Pipe::new();
                        unsafe {
                            OPEN_FILES[g1 as usize] = Some(FileHandle::Pipe { pipe: pipe.clone() });
                            OPEN_FILES[g2 as usize] = Some(FileHandle::Pipe { pipe });

                            GLOBAL_FILE_REFCOUNT[g1 as usize] = 1;
                            GLOBAL_FILE_REFCOUNT[g2 as usize] = 1;
                        }

                        fd_table[l1 as usize] = g1 as i16;
                        fd_table[l2 as usize] = g2 as i16;

                        unsafe {
                            *fds_ptr.add(0) = l1;
                            *fds_ptr.add(1) = l2;
                        }
                        context.rax = 0;
                        return;
                    }
                }
            }
        }
    }
    context.rax = u64::MAX;
}

pub fn handle_close(context: &mut CPUState) {
    let local_fd = context.rdi as usize;

    let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
    let current = tm.current_task;
    if current >= 0 {
        if let Some(thread) = tm.tasks[current as usize].as_mut() {
            let proc = thread.process.as_ref().expect("Thread has no process");
            let mut fd_table = proc.fd_table.lock();
            if local_fd < 16 {
                let global = fd_table[local_fd];
                if global != -1 {
                    crate::fs::vfs::close_file(global as usize);
                    fd_table[local_fd] = -1;
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
            if let Some(thread) = tm.tasks[current as usize].as_ref() {
                let proc = thread.process.as_ref().expect("Thread has no process");
                Some(proc.fd_table.lock()[local_fd])
            } else { None }
        } else {
            None
        }
    };

    if let Some(fd_val) = global_fd_opt {
        if fd_val == -1 {
            context.rax = u64::MAX;
            return;
        }
        let fd = fd_val as usize;
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

pub const TIOCGWINSZ: u64 = 0x5413;
pub const TIOCSWINSZ: u64 = 0x5414;

#[repr(C)]
pub struct WinSize {
    pub ws_row: u16,
    pub ws_col: u16,
    pub ws_xpixel: u16,
    pub ws_ypixel: u16,
}

pub fn handle_ioctl(context: &mut CPUState) {
    let _fd = context.rdi;
    let request = context.rsi;
    let arg = context.rdx as *mut WinSize;

    match request {
        TIOCGWINSZ => {
            let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
            if tm.current_task >= 0 {
                if let Some(thread) = tm.tasks[tm.current_task as usize].as_ref() {
                    let proc = thread.process.as_ref().expect("Thread has no process");
                    if !arg.is_null() {
                        unsafe {
                            (*arg).ws_row = *proc.terminal_height.lock();
                            (*arg).ws_col = *proc.terminal_width.lock();
                            (*arg).ws_xpixel = 0;
                            (*arg).ws_ypixel = 0;
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
        TIOCSWINSZ => {
            let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
            let current = tm.current_task;
            if current >= 0 {
                if let Some(thread) = tm.tasks[current as usize].as_mut() {
                    let proc = thread.process.as_ref().expect("Thread has no process");
                    if !arg.is_null() {
                        unsafe {
                            *proc.terminal_height.lock() = (*arg).ws_row;
                            *proc.terminal_width.lock() = (*arg).ws_col;
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
        _ => {
            context.rax = u64::MAX;
        }
    }
}
