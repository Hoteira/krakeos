use super::{PollFd, POLLERR, POLLIN, POLLNVAL, POLLOUT};
use crate::debugln;
use crate::drivers::periferics::keyboard::KEYBOARD_BUFFER;
use crate::interrupts::task::CPUState;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;

fn acquire_fs_lock() {}

fn release_fs_lock() {}

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
    let fd = context.rdi as usize;
    let user_ptr = context.rsi as *mut u8;
    let user_len = context.rdx as usize;
    let mut bytes_written_to_user = 0;

    if user_ptr.is_null() {
        context.rax = 0;
        return;
    }

    let is_nonblock = {
        let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
        if let Some(idx) = tm.current_task_idx() {
            tm.tasks[idx].as_ref().unwrap().process.as_ref().unwrap().fd_nonblock.lock()[fd]
        } else { false }
    };

    loop {
        {
            let mut keyboard_buffer = KEYBOARD_BUFFER.int_lock();
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

        if is_nonblock {
            context.rax = u64::MAX - 1; // EWOULDBLOCK
            return;
        }


        unsafe {
            core::arch::asm!("sti");
            core::arch::asm!("int 0x81");
            core::arch::asm!("cli");
        }
    }

    context.rax = bytes_written_to_user as u64;
}

pub fn handle_poll(context: &mut CPUState) {
    let fds_ptr = context.rdi as *const PollFd;
    let nfds = context.rsi as usize;
    let timeout_ms = context.rdx as i32;

    if fds_ptr.is_null() || nfds == 0 {
        context.rax = 0;
        return;
    }

    let start_ticks = unsafe { crate::interrupts::task::SYSTEM_TICKS };
    let end_ticks = if timeout_ms >= 0 { Some(start_ticks + timeout_ms as u64) } else { None };

    loop {
        let mut ready_count = 0;

        let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
        let current_idx = tm.current_task_idx().unwrap();
        let mut em = crate::interrupts::event_manager::EVENT_MANAGER.int_lock();

        if em.check_pending(current_idx, crate::interrupts::event_manager::AsyncEvent::Generic(current_idx as u64)) {
            em.unregister_thread(current_idx);
            context.rax = 0;
            return;
        }

        // 1. Register interests and check readiness in one pass
        for i in 0..nfds {
            let pfd = unsafe { &mut *(fds_ptr.add(i) as *mut PollFd) };
            pfd.revents = 0;

            if pfd.fd == 0 {
                em.register(current_idx, crate::interrupts::event_manager::AsyncEvent::Read(0));
                if !KEYBOARD_BUFFER.lock().is_empty() {
                    pfd.revents |= POLLIN;
                }
            } else if pfd.fd >= 0 && (pfd.fd as usize) < 16 {
                let proc = tm.tasks[current_idx].as_ref().unwrap().process.as_ref().unwrap();
                let gfd = proc.fd_table.lock()[pfd.fd as usize];
                if gfd != -1 {
                    if let Some(handle) = crate::fs::vfs::get_file(gfd as usize) {
                        use crate::fs::vfs::FileHandle;
                        match handle {
                            FileHandle::Pipe { pipe } => {
                                em.register(current_idx, crate::interrupts::event_manager::AsyncEvent::IO(pipe.id()));
                                if (pfd.events & POLLIN) != 0 && pipe.available() > 0 { pfd.revents |= POLLIN; }
                                if (pfd.events & POLLOUT) != 0 && pipe.available() < 4096 { pfd.revents |= POLLOUT; }
                            }
                            FileHandle::File { .. } => {
                                // Regular files are always ready
                                if (pfd.events & POLLIN) != 0 { pfd.revents |= POLLIN; }
                                if (pfd.events & POLLOUT) != 0 { pfd.revents |= POLLOUT; }
                            }
                        }
                    } else { pfd.revents = POLLERR; }
                } else { pfd.revents = POLLNVAL; }
            } else { pfd.revents = POLLNVAL; }

            if pfd.revents != 0 { ready_count += 1; }
        }

        // Always register for generic wakeup
        em.register(current_idx, crate::interrupts::event_manager::AsyncEvent::Generic(current_idx as u64));

        if ready_count > 0 {
            em.unregister_thread(current_idx);
            context.rax = ready_count as u64;
            return;
        }

        if let Some(end) = end_ticks {
            if unsafe { crate::interrupts::task::SYSTEM_TICKS } >= end {
                em.unregister_thread(current_idx);
                context.rax = 0;
                return;
            }
        }

        if let Some(thread) = &mut tm.tasks[current_idx] {
            thread.state = crate::interrupts::task::ThreadState::WaitingForEvent;
        }

        drop(em);
        drop(tm);

        unsafe {
            core::arch::asm!("sti");
            core::arch::asm!("int 0x81");
            core::arch::asm!("cli");
        }
    }
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

    acquire_fs_lock();
    let open_res = crate::fs::vfs::open(0, &resolved);
    release_fs_lock();

    if let Ok(node) = open_res {
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
                } else { context.rax = u64::MAX; }
            } else { context.rax = u64::MAX; }
        } else { context.rax = u64::MAX; }
    } else { context.rax = u64::MAX; }
}

pub fn handle_create(context: &mut CPUState, syscall_num: u64) {
    let ptr = context.rdi as *const u8;
    let len = context.rsi as usize;
    let path_str_full = copy_string_from_user(ptr, len);

    let cwd_str = get_current_cwd();
    let resolved = resolve_path(&cwd_str, &path_str_full);

    crate::debugln!("SYS_CREATE: path='{}' (raw='{}')", resolved, path_str_full);

    acquire_fs_lock();
    let exists = crate::fs::vfs::open_file(0, &resolved);
    release_fs_lock();

    if let Ok(global_fd) = exists {
        crate::debugln!("SYS_CREATE: File already exists, returning FD");
        context.rax = assign_local_fd(global_fd);
        return;
    }

    let (parent_path, name) = if let Some(idx) = resolved.rfind('/') {
        (&resolved[..idx], &resolved[idx + 1..])
    } else {
        ("", resolved.as_str())
    };

    crate::debugln!("SYS_CREATE: Creating '{}' in '{}'", name, parent_path);

    acquire_fs_lock();
    let parent_res = crate::fs::vfs::open(0, parent_path);
    let final_res = if let Ok(mut parent) = parent_res {
        if syscall_num == 83 { parent.create_dir(name).map(|_| 0usize) } else { parent.create_file(name).map(|_| 0usize) }
    } else { Err(String::from("Parent not found")) };
    release_fs_lock();

    match final_res {
        Ok(_) => {
            if syscall_num == 83 {
                context.rax = 0;
                return;
            }
            crate::debugln!("SYS_CREATE: Success, opening new file...");

            acquire_fs_lock();
            let open_res = crate::fs::vfs::open_file(0, &resolved);
            release_fs_lock();

            if let Ok(global_fd) = open_res {
                context.rax = assign_local_fd(global_fd);
            } else {
                crate::debugln!("SYS_CREATE: FAILED TO OPEN AFTER CREATE!");
                context.rax = u64::MAX;
            }
        }
        Err(e) => {
            crate::debugln!("SYS_CREATE: FAILED! Error: {}", e);
            context.rax = u64::MAX;
        }
    }
}

pub fn get_current_cwd() -> String {
    let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
    if tm.current_task >= 0 {
        if let Some(thread) = tm.tasks[tm.current_task as usize].as_ref() {
            let proc = thread.process.as_ref().expect("Thread has no process");
            let cwd = proc.cwd.lock();
            let cwd_len = cwd.iter().position(|&c| c == 0).unwrap_or(cwd.len());
            return String::from_utf8_lossy(&cwd[..cwd_len]).into_owned();
        }
    }
    String::from("@0xE0/")
}

pub fn assign_local_fd(global_fd: usize) -> u64 {
    let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
    let current = tm.current_task;
    if current >= 0 {
        if let Some(thread) = tm.tasks[current as usize].as_mut() {
            let proc = thread.process.as_ref().expect("Thread has no process");
            let mut fd_table = proc.fd_table.lock();
            for i in 0..16 {
                if fd_table[i] == -1 {
                    fd_table[i] = global_fd as i16;
                    return i as u64;
                }
            }
        }
    }
    u64::MAX
}

pub fn handle_remove(context: &mut CPUState) {
    let ptr = context.rdi as *const u8;
    let len = context.rsi as usize;
    let path_str_full = copy_string_from_user(ptr, len);
    let cwd_str = get_current_cwd();
    let resolved = resolve_path(&cwd_str, &path_str_full);

    let (parent_path, name) = if let Some(idx) = resolved.rfind('/') {
        (&resolved[..idx], &resolved[idx + 1..])
    } else { ("", resolved.as_str()) };

    acquire_fs_lock();
    let parent_res = crate::fs::vfs::open(0, parent_path);

    let remove_res = if let Ok(mut parent) = parent_res {
        parent.remove(name)
    } else { Err(String::from("Parent not found")) };

    release_fs_lock();

    match remove_res {
        Ok(_) => context.rax = 0,
        Err(_) => context.rax = u64::MAX,
    }
}

pub fn handle_rename(context: &mut CPUState) {
    let old_ptr = context.rdi as *const u8;
    let old_len = context.rsi as usize;
    let new_ptr = context.rdx as *const u8;
    let new_len = context.r10 as usize;

    let path_old = copy_string_from_user(old_ptr, old_len);
    let path_new = copy_string_from_user(new_ptr, new_len);
    let cwd_str = get_current_cwd();

    let resolved_old = resolve_path(&cwd_str, &path_old);
    let resolved_new = resolve_path(&cwd_str, &path_new);

    let (parent_old, name_old) = if let Some(idx) = resolved_old.rfind('/') { (&resolved_old[..idx], &resolved_old[idx + 1..]) } else { ("", resolved_old.as_str()) };
    let (parent_new, _name_new) = if let Some(idx) = resolved_new.rfind('/') { (&resolved_new[..idx], &resolved_new[idx + 1..]) } else { ("", resolved_new.as_str()) };

    if parent_old != parent_new {
        // This limitation might need lifting, but for now it's safer
    }

    acquire_fs_lock();
    let parent_res = crate::fs::vfs::open(0, parent_old);

    let rename_res = if let Ok(mut parent) = parent_res {
        let name_new = if let Some(idx) = resolved_new.rfind('/') { &resolved_new[idx + 1..] } else { resolved_new.as_str() };
        parent.rename(name_old, name_new)
    } else { Err(String::from("Parent not found")) };

    release_fs_lock();

    match rename_res {
        Ok(_) => context.rax = 0,
        Err(_) => context.rax = u64::MAX,
    }
}

pub fn handle_open(context: &mut CPUState) {
    let ptr = context.rdi as *const u8;
    let len = context.rsi as usize;
    let path_str_full = copy_string_from_user(ptr, len);
    let cwd_str = get_current_cwd();
    let resolved = resolve_path(&cwd_str, &path_str_full);

    acquire_fs_lock();
    let res = crate::fs::vfs::open_file(0, &resolved);
    release_fs_lock();

    match res {
        Ok(global_fd) => context.rax = assign_local_fd(global_fd),
        Err(_) => context.rax = u64::MAX,
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
        } else { None }
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

        let is_nonblock = {
            let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
            if let Some(idx) = tm.current_task_idx() {
                tm.tasks[idx].as_ref().unwrap().process.as_ref().unwrap().fd_nonblock.lock()[local_fd]
            } else { false }
        };

        let fd = fd_val as usize;
        let buf = unsafe { core::slice::from_raw_parts_mut(buf_ptr, len) };

        acquire_fs_lock();
        let handle_opt = crate::fs::vfs::get_file(fd);
        let res = if let Some(handle) = handle_opt {
            use crate::fs::vfs::FileHandle;
            match handle {
                FileHandle::File { node, offset } => {
                    match node.read(*offset, buf) {
                        Ok(n) => {
                            *offset += n as u64;
                            Some(Ok(n))
                        }
                        Err(e) => Some(Err(e))
                    }
                }
                FileHandle::Pipe { pipe } => {
                    if is_nonblock && pipe.available() == 0 {
                        Some(Err(String::from("EWOULDBLOCK")))
                    } else {
                        Some(Ok(pipe.read(buf)))
                    }
                }
            }
        } else { None };
        release_fs_lock();

        match res {
            Some(Ok(n)) => context.rax = n as u64,
            Some(Err(e)) if e == "EWOULDBLOCK" => context.rax = u64::MAX - 1,
            Some(Err(_)) => context.rax = u64::MAX,
            None => context.rax = u64::MAX,
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
        } else { None }
    };

    if let Some(fd_val) = global_fd_opt {
        if fd_val == -1 {
            if local_fd == 1 || local_fd == 2 {
                let s = unsafe { core::slice::from_raw_parts(buf_ptr, len) };
                let s_str = String::from_utf8_lossy(s);
                // debug_print might be slow, but usually safe. 
                // We can acquire FS_LOCK or not. It's not VFS.
                // But it might use UART which might conflict?
                // Usually UART is separate.
                crate::debug_print!("{}", s_str);
                context.rax = len as u64;
                return;
            }
            context.rax = u64::MAX;
            return;
        }
        let fd = fd_val as usize;
        let buf = unsafe { core::slice::from_raw_parts(buf_ptr, len) };

        acquire_fs_lock();
        let handle_opt = crate::fs::vfs::get_file(fd);
        let res = if let Some(handle) = handle_opt {
            use crate::fs::vfs::FileHandle;
            match handle {
                FileHandle::File { node, offset } => {
                    match node.write(*offset, buf) {
                        Ok(n) => {
                            *offset += n as u64;
                            Some(Ok(n))
                        }
                        Err(e) => Some(Err(e))
                    }
                }
                FileHandle::Pipe { pipe } => {
                    Some(Ok(pipe.write(buf)))
                }
            }
        } else { None };
        release_fs_lock();

        match res {
            Some(Ok(n)) => context.rax = n as u64,
            Some(Err(_)) => context.rax = u64::MAX,
            None => context.rax = u64::MAX,
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
        } else { None }
    };

    if let Some(fd_val) = global_fd_opt {
        if fd_val != -1 {
            let fd = fd_val as usize;

            acquire_fs_lock();
            let handle_opt = crate::fs::vfs::get_file(fd);
            let res = if let Some(handle) = handle_opt {
                use crate::fs::vfs::FileHandle;
                match handle {
                    FileHandle::File { node, offset } => {
                        let buf = unsafe { core::slice::from_raw_parts_mut(buf_ptr, len) };
                        match node.read_dir(*offset, buf) {
                            Ok((bw, cr)) => {
                                *offset += cr as u64;
                                Some(Ok(bw))
                            }
                            Err(e) => Some(Err(e)),
                        }
                    }
                    _ => None,
                }
            } else { None };
            release_fs_lock();

            match res {
                Some(Ok(n)) => context.rax = n as u64,
                Some(Err(_)) => context.rax = u64::MAX,
                None => context.rax = u64::MAX,
            }
        } else { context.rax = u64::MAX; }
    } else { context.rax = u64::MAX; }
}

pub fn handle_stat(context: &mut CPUState) {
    let stat = if context.rax == 4 { // SYS_STAT
        let ptr = context.rdi as *const u8;
        let len = context.rsi as usize;

        if ptr.is_null() || len == 0 || len > 255 {
            crate::debugln!("SYS_STAT: Invalid path pointer or length");
            None
        } else {
            let path_str_full = copy_string_from_user(ptr, len);
            let cwd_str = get_current_cwd();
            let resolved = resolve_path(&cwd_str, &path_str_full);

            crate::debugln!("SYS_STAT: path='{}'", resolved);

            acquire_fs_lock();
            let res = crate::fs::vfs::open(0, &resolved);
            let stat_res = if let Ok(node) = res {
                Some(node.stat())
            } else {
                crate::debugln!("SYS_STAT: FAILED TO FIND NODE!");
                None
            };
            release_fs_lock();

            stat_res
        }
    } else { // SYS_FSTAT
        let local_fd = context.rdi as usize;
        let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
        let current = tm.current_task;
        if current >= 0 && local_fd < 16 {
            if let Some(thread) = tm.tasks[current as usize].as_ref() {
                let proc = thread.process.as_ref().expect("Thread has no process");
                let gfd = proc.fd_table.lock()[local_fd];
                if gfd != -1 {
                    acquire_fs_lock();
                    let handle_opt = crate::fs::vfs::get_file(gfd as usize);
                    let stat_res = if let Some(handle) = handle_opt {
                        use crate::fs::vfs::FileHandle;
                        match handle {
                            FileHandle::File { node, .. } => Some(node.stat()),
                            _ => None
                        }
                    } else { None };
                    release_fs_lock();

                    stat_res
                } else { None }
            } else { None }
        } else { None }
    };

    match stat {
        Some(s) => {
            let user_stat_ptr = context.rdx as *mut crate::fs::vfs::Stat;
            if !user_stat_ptr.is_null() {
                unsafe { core::ptr::write_unaligned(user_stat_ptr, s); }
                context.rax = 0;
            } else {
                context.rax = s.size;
            }
        }
        None => {
            context.rax = u64::MAX;
        }
    }
}

pub fn handle_ftruncate(context: &mut CPUState) {
    let local_fd = context.rdi as usize;
    let length = context.rsi as u64;
    let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
    let current = tm.current_task;
    if current >= 0 && local_fd < 16 {
        if let Some(thread) = tm.tasks[current as usize].as_ref() {
            let proc = thread.process.as_ref().expect("Thread has no process");
            let gfd = proc.fd_table.lock()[local_fd];
            if gfd != -1 {
                acquire_fs_lock();
                let handle_opt = crate::fs::vfs::get_file(gfd as usize);
                let res = if let Some(handle) = handle_opt {
                    use crate::fs::vfs::FileHandle;
                    match handle {
                        FileHandle::File { node, .. } => {
                            Some(node.truncate(length))
                        }
                        _ => None
                    }
                } else { None };
                release_fs_lock();

                match res {
                    Some(Ok(_)) => context.rax = 0,
                    _ => context.rax = u64::MAX
                }
            } else { context.rax = u64::MAX }
        } else { context.rax = u64::MAX }
    } else { context.rax = u64::MAX }
}

pub fn handle_pipe(context: &mut CPUState) {
    let fds_ptr = context.rdi as *mut i32;
    if fds_ptr.is_null() {
        context.rax = u64::MAX;
        return;
    }

    acquire_fs_lock();

    use crate::fs::vfs::{FileHandle, GLOBAL_FILE_REFCOUNT, OPEN_FILES};
    use crate::fs::pipe::Pipe;
    let mut g1 = -1;
    let mut g2 = -1;
    for i in 3..256 {
        unsafe {
            if OPEN_FILES[i].is_none() {
                if g1 == -1 { g1 = i as i32; } else {
                    g2 = i as i32;
                    break;
                }
            }
        }
    }
    if g1 != -1 && g2 != -1 {
        let pipe = Pipe::new();
        unsafe {
            OPEN_FILES[g1 as usize] = Some(FileHandle::Pipe { pipe: pipe.clone() });
            OPEN_FILES[g2 as usize] = Some(FileHandle::Pipe { pipe });
            GLOBAL_FILE_REFCOUNT[g1 as usize] = 1;
            GLOBAL_FILE_REFCOUNT[g2 as usize] = 1;
        }
        release_fs_lock();

        let l1 = assign_local_fd(g1 as usize);
        let l2 = assign_local_fd(g2 as usize);
        if l1 != u64::MAX && l2 != u64::MAX {
            unsafe {
                *fds_ptr.add(0) = l1 as i32;
                *fds_ptr.add(1) = l2 as i32;
            }
            context.rax = 0;
            return;
        }
    } else {
        release_fs_lock();
    }
    context.rax = u64::MAX;
}

pub fn handle_close(context: &mut CPUState) {
    let local_fd = context.rdi as usize;
    let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
    if let Some(current) = tm.current_task_idx() {
        if let Some(thread) = tm.tasks[current].as_mut() {
            let proc = thread.process.as_ref().expect("Thread has no process");
            let mut fd_table = proc.fd_table.lock();
            if local_fd < 16 {
                let global = fd_table[local_fd];
                if global != -1 {
                    // We cannot use FS_LOCK because we hold Task Lock.
                    // This is a known issue but acceptable for single-user scenarios.
                    crate::fs::vfs::close_file(global as usize);

                    fd_table[local_fd] = -1;
                    context.rax = 0;
                    return;
                }
            }
        }
    }
    context.rax = u64::MAX;
}

pub fn handle_seek(context: &mut CPUState) {
    let local_fd = context.rdi as usize;
    let offset = context.rsi as i64;
    let whence = context.rdx as usize;
    let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
    if let Some(current) = tm.current_task_idx() {
        if let Some(thread) = tm.tasks[current].as_ref() {
            let proc = thread.process.as_ref().expect("Thread has no process");
            let gfd = proc.fd_table.lock()[local_fd];
            if gfd != -1 {
                acquire_fs_lock();
                let handle_opt = crate::fs::vfs::get_file(gfd as usize);
                let res = if let Some(handle) = handle_opt {
                    use crate::fs::vfs::FileHandle;
                    match handle {
                        FileHandle::File { node, offset: current_offset } => {
                            let size = node.size() as i64;
                            let new_offset = match whence {
                                0 => offset,
                                1 => (*current_offset as i64) + offset,
                                2 => size + offset,
                                _ => -1
                            };
                            if new_offset >= 0 {
                                *current_offset = new_offset as u64;
                                Some(new_offset as u64)
                            } else { None }
                        }
                        _ => None,
                    }
                } else { None };
                release_fs_lock();

                if let Some(o) = res {
                    context.rax = o;
                } else {
                    context.rax = u64::MAX;
                }
            } else { context.rax = u64::MAX; }
        } else { context.rax = u64::MAX; }
    } else { context.rax = u64::MAX; }
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
    let request = context.rsi;

    let arg = context.rdx as *mut WinSize;

    match request {
        TIOCGWINSZ => {
            let tm = crate::interrupts::task::TASK_MANAGER.int_lock();

            if let Some(current) = tm.current_task_idx() {
                if let Some(thread) = tm.tasks[current].as_ref() {
                    let proc = thread.process.as_ref().expect("Thread has no process");

                    if !arg.is_null() {
                        unsafe {
                            (*arg).ws_row = *proc.terminal_height.lock();
                            (*arg).ws_col = *proc.terminal_width.lock();
                            (*arg).ws_xpixel = 0;
                            (*arg).ws_ypixel = 0;
                        }
                        context.rax = 0;
                    } else { context.rax = u64::MAX; }
                } else { context.rax = u64::MAX; }
            } else { context.rax = u64::MAX; }
        }

        TIOCSWINSZ => {
            let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();

            if let Some(current) = tm.current_task_idx() {
                if let Some(thread) = tm.tasks[current].as_mut() {
                    let proc = thread.process.as_ref().expect("Thread has no process");

                    if !arg.is_null() {
                        unsafe {
                            *proc.terminal_height.lock() = (*arg).ws_row;
                            *proc.terminal_width.lock() = (*arg).ws_col;
                        }
                        context.rax = 0;
                    } else { context.rax = u64::MAX; }
                } else { context.rax = u64::MAX; }
            } else { context.rax = u64::MAX; }
        }

        _ => context.rax = u64::MAX,
    }
}


pub fn handle_mmap_file(context: &mut CPUState) {
    let local_fd = context.rdi as usize;

    let offset = context.rsi as u64;

    let length = context.rdx as usize;


    let global_fd_opt = {
        let tm = crate::interrupts::task::TASK_MANAGER.int_lock();

        let current = tm.current_task;

        if current >= 0 && local_fd < 16 {
            if let Some(thread) = tm.tasks[current as usize].as_ref() {
                let proc = thread.process.as_ref().expect("Thread has no process");

                Some(proc.fd_table.lock()[local_fd])
            } else { None }
        } else { None }
    };


    if let Some(fd_val) = global_fd_opt {
        if fd_val != -1 {
            let fd = fd_val as usize;

            acquire_fs_lock();
            let handle_opt = crate::fs::vfs::get_file(fd);
            let res = if let Some(handle) = handle_opt {
                use crate::fs::vfs::FileHandle;

                match handle {
                    FileHandle::File { node, .. } => {
                        Some(node.mmap(offset, length))
                    }

                    _ => None,
                }
            } else { None };
            release_fs_lock();

            match res {
                Some(Ok(addr)) => context.rax = addr,
                _ => context.rax = u64::MAX,
            }
        } else { context.rax = u64::MAX; }
    } else { context.rax = u64::MAX; }
}

pub fn handle_pread64(context: &mut CPUState) {
    let local_fd = context.rdi as usize;
    let buf_ptr = context.rsi as *mut u8;
    let len = context.rdx as usize;
    let offset = context.r10 as u64;

    let global_fd_opt = {
        let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
        let current = tm.current_task;
        if current >= 0 && local_fd < 16 {
            if let Some(thread) = tm.tasks[current as usize].as_ref() {
                let proc = thread.process.as_ref().expect("Thread has no process");
                Some(proc.fd_table.lock()[local_fd])
            } else { None }
        } else { None }
    };

    if let Some(fd_val) = global_fd_opt {
        if fd_val != -1 {
            let fd = fd_val as usize;
            let buf = unsafe { core::slice::from_raw_parts_mut(buf_ptr, len) };

            acquire_fs_lock();
            let handle_opt = crate::fs::vfs::get_file(fd);
            let res = if let Some(handle) = handle_opt {
                use crate::fs::vfs::FileHandle;
                match handle {
                    FileHandle::File { node, .. } => {
                        Some(node.read(offset, buf))
                    }
                    _ => None,
                }
            } else { None };
            release_fs_lock();

            match res {
                Some(Ok(n)) => context.rax = n as u64,
                _ => context.rax = u64::MAX,
            }
        } else { context.rax = u64::MAX; }
    } else { context.rax = u64::MAX; }
}

pub fn handle_pwrite64(context: &mut CPUState) {
    let local_fd = context.rdi as usize;
    let buf_ptr = context.rsi as *const u8;
    let len = context.rdx as usize;
    let offset = context.r10 as u64;

    let global_fd_opt = {
        let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
        let current = tm.current_task;
        if current >= 0 && local_fd < 16 {
            if let Some(thread) = tm.tasks[current as usize].as_ref() {
                let proc = thread.process.as_ref().expect("Thread has no process");
                Some(proc.fd_table.lock()[local_fd])
            } else { None }
        } else { None }
    };

    if let Some(fd_val) = global_fd_opt {
        if fd_val != -1 {
            let fd = fd_val as usize;
            let buf = unsafe { core::slice::from_raw_parts(buf_ptr, len) };

            acquire_fs_lock();
            let handle_opt = crate::fs::vfs::get_file(fd);
            let res = if let Some(handle) = handle_opt {
                use crate::fs::vfs::FileHandle;
                match handle {
                    FileHandle::File { node, .. } => {
                        Some(node.write(offset, buf))
                    }
                    _ => None,
                }
            } else { None };
            release_fs_lock();

            match res {
                Some(Ok(n)) => context.rax = n as u64,
                _ => context.rax = u64::MAX,
            }
        } else { context.rax = u64::MAX; }
    } else { context.rax = u64::MAX; }
}

pub fn handle_fcntl(context: &mut CPUState) {
    context.rax = 0;
}

pub fn handle_linkat(context: &mut CPUState) {
    let old_ptr = context.rsi as *const u8;
    let old_len = context.rdx as usize;
    let new_ptr = context.r8 as *const u8;
    let new_len = context.r9 as usize;

    let old_path = copy_string_from_user(old_ptr, old_len);
    let new_path = copy_string_from_user(new_ptr, new_len);

    let cwd_str = get_current_cwd();
    let resolved_old = resolve_path(&cwd_str, &old_path);
    let resolved_new = resolve_path(&cwd_str, &new_path);

    acquire_fs_lock();
    let old_node_res = crate::fs::vfs::open(0, &resolved_old);

    let res = if let Ok(mut old_node) = old_node_res {
        let (parent_path, name) = if let Some(idx) = resolved_new.rfind('/') {
            (&resolved_new[..idx], &resolved_new[idx + 1..])
        } else {
            ("", resolved_new.as_str())
        };

        if let Ok(mut parent) = crate::fs::vfs::open(0, parent_path) {
            parent.link(name, &mut *old_node)
        } else {
            Err(String::from("Parent not found"))
        }
    } else {
        Err(String::from("Old node not found"))
    };
    release_fs_lock();

    match res {
        Ok(_) => context.rax = 0,
        Err(_) => context.rax = u64::MAX,
    }
}

pub fn handle_symlinkat(context: &mut CPUState) {
    let target_ptr = context.rdi as *const u8;
    let target_len = context.rsi as usize;
    let new_ptr = context.r10 as *const u8;
    let new_len = context.r8 as usize;

    let target = copy_string_from_user(target_ptr, target_len);
    let new_path = copy_string_from_user(new_ptr, new_len);

    let cwd_str = get_current_cwd();
    let resolved_new = resolve_path(&cwd_str, &new_path);

    let (parent_path, name) = if let Some(idx) = resolved_new.rfind('/') {
        (&resolved_new[..idx], &resolved_new[idx + 1..])
    } else {
        ("", resolved_new.as_str())
    };

    acquire_fs_lock();
    let parent_res = crate::fs::vfs::open(0, parent_path);
    let res = if let Ok(mut parent) = parent_res {
        parent.symlink(name, &target)
    } else {
        Err(String::from("Parent not found"))
    };
    release_fs_lock();

    match res {
        Ok(_) => context.rax = 0,
        Err(_) => context.rax = u64::MAX,
    }
}

pub fn handle_utimensat(context: &mut CPUState) {
    let path_ptr = context.rsi as *const u8;
    let path_len = context.rdx as usize;
    let atime = context.r10 as u64;
    let mtime = context.r8 as u64;

    let path = copy_string_from_user(path_ptr, path_len);
    let cwd_str = get_current_cwd();
    let resolved = resolve_path(&cwd_str, &path);

    acquire_fs_lock();
    let node_res = crate::fs::vfs::open(0, &resolved);
    let res = if let Ok(mut node) = node_res {
        node.set_times(atime, mtime)
    } else {
        Err(String::from("Node not found"))
    };
    release_fs_lock();

    match res {
        Ok(_) => context.rax = 0,
        Err(_) => context.rax = u64::MAX,
    }
}

pub fn handle_set_nonblock(context: &mut CPUState) {
    let fd = context.rdi as usize;
    let nonblock = context.rsi != 0;

    let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
    if let Some(current) = tm.current_task_idx() {
        if let Some(thread) = tm.tasks[current].as_mut() {
            let proc = thread.process.as_ref().expect("Thread has no process");
            if fd < 16 {
                proc.fd_nonblock.lock()[fd] = nonblock;
                context.rax = 0;
                return;
            }
        }
    }
    context.rax = u64::MAX;
}

pub fn handle_readlinkat(context: &mut CPUState) {
    crate::debugln!("SYS_READLINKAT: START");
    let dirfd = context.rdi as i32;
    let path_ptr = context.rsi as *const u8;
    let path_len = context.rdx as usize;
    let buf_ptr = context.r10 as *mut u8;
    let buf_len = context.r8 as usize;

    let path = copy_string_from_user(path_ptr, path_len);

    debugln!("SYS_READLINKAT: PATH: {}", path);

    let base_path = if dirfd == -100 { // AT_FDCWD
        get_current_cwd()
    } else if dirfd >= 0 && (dirfd as usize) < 16 {
        let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
        let current = tm.current_task;
        if current >= 0 {
            let proc = tm.tasks[current as usize].as_ref().unwrap().process.as_ref().unwrap();

            // Helper to get CWD without re-locking TM (which get_current_cwd does)
            let get_proc_cwd = || {
                let cwd = proc.cwd.lock();
                let cwd_len = cwd.iter().position(|&c| c == 0).unwrap_or(cwd.len());
                String::from_utf8_lossy(&cwd[..cwd_len]).into_owned()
            };

            let gfd = proc.fd_table.lock()[dirfd as usize];
            if gfd != -1 {
                acquire_fs_lock();
                let handle_opt = crate::fs::vfs::get_file(gfd as usize);
                release_fs_lock();

                if let Some(handle) = handle_opt {
                    if let crate::fs::vfs::FileHandle::File { node, .. } = handle {
                        if node.inode() == 2 {
                            String::from("@0xE0/")
                        } else {
                            get_proc_cwd()
                        }
                    } else { get_proc_cwd() }
                } else { get_proc_cwd() }
            } else { get_proc_cwd() }
        } else { String::from("@0xE0/") }
    } else if dirfd == 3 { // WASI Root
        String::from("@0xE0/")
    } else {
        get_current_cwd()
    };

    let resolved = resolve_path(&base_path, &path);
    crate::debugln!("SYS_READLINKAT: Resolved='{}'", resolved);

    crate::debugln!("SYS_READLINKAT: Acquire FS Lock...");
    acquire_fs_lock();
    crate::debugln!("SYS_READLINKAT: FS Lock Acquired. Opening...");
    let node_res = crate::fs::vfs::open(0, &resolved);
    crate::debugln!("SYS_READLINKAT: Opened. Reading link...");
    let res = if let Ok(mut node) = node_res {
        let r = node.readlink();
        crate::debugln!("SYS_READLINKAT: Read link done.");
        r
    } else {
        crate::debugln!("SYS_READLINKAT: Node not found.");
        Err(String::from("Node not found"))
    };
    release_fs_lock();
    crate::debugln!("SYS_READLINKAT: Lock Released.");

    match res {
        Ok(target) => {
            let target_bytes = target.as_bytes();
            let to_copy = core::cmp::min(target_bytes.len(), buf_len);
            unsafe {
                core::ptr::copy_nonoverlapping(target_bytes.as_ptr(), buf_ptr, to_copy);
            }
            context.rax = to_copy as u64;
        }
        Err(_) => context.rax = u64::MAX,
    }
}