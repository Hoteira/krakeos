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

    if let Ok(node) = crate::fs::vfs::open(0, &resolved) {
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

    if let Ok(global_fd) = crate::fs::vfs::open_file(0, &resolved) {
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

    let final_res = if let Ok(mut parent) = crate::fs::vfs::open(0, parent_path) {
        if syscall_num == 83 { parent.create_dir(name).map(|_| 0usize) }
        else { parent.create_file(name).map(|_| 0usize) }
    } else { Err(String::from("Parent not found")) };

    match final_res {
        Ok(_) => {
            crate::debugln!("SYS_CREATE: Success, opening new file...");
            if let Ok(global_fd) = crate::fs::vfs::open_file(0, &resolved) {
                context.rax = assign_local_fd(global_fd);
            } else {
                crate::debugln!("SYS_CREATE: FAILED TO OPEN AFTER CREATE!");
                context.rax = u64::MAX;
            }
        },
        Err(e) => {
            crate::debugln!("SYS_CREATE: FAILED! Error: {}", e);
            context.rax = u64::MAX;
        }
    }
}

fn get_current_cwd() -> String {
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

fn assign_local_fd(global_fd: usize) -> u64 {
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

    if let Ok(mut parent) = crate::fs::vfs::open(0, parent_path) {
        match parent.remove(name) {
            Ok(_) => context.rax = 0,
            Err(_) => context.rax = u64::MAX,
        }
    } else { context.rax = u64::MAX; }
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

    let (parent_old, name_old) = if let Some(idx) = resolved_old.rfind('/') { (&resolved_old[..idx], &resolved_old[idx+1..]) } else { ("", resolved_old.as_str()) };
    let (parent_new, name_new) = if let Some(idx) = resolved_new.rfind('/') { (&resolved_new[..idx], &resolved_new[idx+1..]) } else { ("", resolved_new.as_str()) };

    if parent_old != parent_new { context.rax = u64::MAX; return; }

    if let Ok(mut parent) = crate::fs::vfs::open(0, parent_old) {
        match parent.rename(name_old, name_new) {
            Ok(_) => context.rax = 0,
            Err(_) => context.rax = u64::MAX,
        }
    } else { context.rax = u64::MAX; }
}

pub fn handle_open(context: &mut CPUState) {
    let ptr = context.rdi as *const u8;
    let len = context.rsi as usize;
    let path_str_full = copy_string_from_user(ptr, len);
    let cwd_str = get_current_cwd();
    let resolved = resolve_path(&cwd_str, &path_str_full);

    match crate::fs::vfs::open_file(0, &resolved) {
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
            if local_fd == 0 { handle_read(context); return; }
            context.rax = u64::MAX; return;
        }
        let fd = fd_val as usize;
        let buf = unsafe { core::slice::from_raw_parts_mut(buf_ptr, len) };
        if let Some(handle) = crate::fs::vfs::get_file(fd) {
            use crate::fs::vfs::FileHandle;
            match handle {
                FileHandle::File { node, offset } => {
                    match node.read(*offset, buf) {
                        Ok(n) => { *offset += n as u64; context.rax = n as u64; }
                        Err(_) => context.rax = u64::MAX,
                    }
                }
                FileHandle::Pipe { pipe } => { context.rax = pipe.read(buf) as u64; }
            }
        } else { context.rax = u64::MAX; }
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
            if local_fd == 1 || local_fd == 2 { context.rax = len as u64; return; }
            context.rax = u64::MAX; return;
        }
        let fd = fd_val as usize;
        let buf = unsafe { core::slice::from_raw_parts(buf_ptr, len) };
        if let Some(handle) = crate::fs::vfs::get_file(fd) {
            use crate::fs::vfs::FileHandle;
            match handle {
                FileHandle::File { node, offset } => {
                    match node.write(*offset, buf) {
                        Ok(n) => { *offset += n as u64; context.rax = n as u64; }
                        Err(_) => context.rax = u64::MAX,
                    }
                }
                FileHandle::Pipe { pipe } => { context.rax = pipe.write(buf) as u64; }
            }
        } else { context.rax = u64::MAX; }
        return;
    }
    context.rax = u64::MAX;
}

pub fn handle_read_dir(context: &mut CPUState) {
    let local_fd = context.rdi as usize;
    let buf_ptr = context.rsi as *mut u8;
    let len = context.rdx as usize;

    if buf_ptr.is_null() { context.rax = u64::MAX; return; }

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
            if let Some(handle) = crate::fs::vfs::get_file(fd) {
                use crate::fs::vfs::FileHandle;
                match handle {
                    FileHandle::File { node, offset } => {
                        let buf = unsafe { core::slice::from_raw_parts_mut(buf_ptr, len) };
                        match node.read_dir(*offset, buf) {
                            Ok((bw, cr)) => { *offset += cr as u64; context.rax = bw as u64; }
                            Err(_) => context.rax = u64::MAX,
                        }
                    }
                    _ => context.rax = u64::MAX,
                }
            } else { context.rax = u64::MAX; }
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
            
            if let Ok(node) = crate::fs::vfs::open(0, &resolved) {
                Some(node.stat())
            } else {
                crate::debugln!("SYS_STAT: FAILED TO FIND NODE!");
                None
            }
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
                    if let Some(handle) = crate::fs::vfs::get_file(gfd as usize) {
                        use crate::fs::vfs::FileHandle;
                        match handle { FileHandle::File { node, .. } => Some(node.stat()), _ => None }
                    } else { None }
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
                if let Some(handle) = crate::fs::vfs::get_file(gfd as usize) {
                    use crate::fs::vfs::FileHandle;
                    match handle { FileHandle::File { node, .. } => { match node.truncate(length) { Ok(_) => context.rax = 0, Err(_) => context.rax = u64::MAX } } _ => context.rax = u64::MAX }
                } else { context.rax = u64::MAX }
            } else { context.rax = u64::MAX }
        } else { context.rax = u64::MAX }
    } else { context.rax = u64::MAX }
}

pub fn handle_pipe(context: &mut CPUState) {
    let fds_ptr = context.rdi as *mut i32;
    if fds_ptr.is_null() { context.rax = u64::MAX; return; }
    use crate::fs::vfs::{FileHandle, GLOBAL_FILE_REFCOUNT, OPEN_FILES};
    use crate::fs::pipe::Pipe;
    let mut g1 = -1; let mut g2 = -1;
    for i in 3..256 { unsafe { if OPEN_FILES[i].is_none() { if g1 == -1 { g1 = i as i32; } else { g2 = i as i32; break; } } } }
    if g1 != -1 && g2 != -1 {
        let pipe = Pipe::new();
        unsafe {
            OPEN_FILES[g1 as usize] = Some(FileHandle::Pipe { pipe: pipe.clone() });
            OPEN_FILES[g2 as usize] = Some(FileHandle::Pipe { pipe });
            GLOBAL_FILE_REFCOUNT[g1 as usize] = 1; GLOBAL_FILE_REFCOUNT[g2 as usize] = 1;
        }
        let l1 = assign_local_fd(g1 as usize);
        let l2 = assign_local_fd(g2 as usize);
        if l1 != u64::MAX && l2 != u64::MAX {
            unsafe { *fds_ptr.add(0) = l1 as i32; *fds_ptr.add(1) = l2 as i32; }
            context.rax = 0; return;
        }
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
                    crate::fs::vfs::close_file(global as usize);
                    fd_table[local_fd] = -1;
                    context.rax = 0; return;
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
                if let Some(handle) = crate::fs::vfs::get_file(gfd as usize) {
                    use crate::fs::vfs::FileHandle;
                    match handle {
                        FileHandle::File { node, offset: current_offset } => {
                            let size = node.size() as i64;
                            let new_offset = match whence { 0 => offset, 1 => (*current_offset as i64) + offset, 2 => size + offset, _ => -1 };
                            if new_offset >= 0 { *current_offset = new_offset as u64; context.rax = new_offset as u64; } else { context.rax = u64::MAX; }
                        }
                        _ => context.rax = u64::MAX,
                    }
                } else { context.rax = u64::MAX; }
            } else { context.rax = u64::MAX; }
        } else { context.rax = u64::MAX; }
    } else { context.rax = u64::MAX; }
}

pub const TIOCGWINSZ: u64 = 0x5413;
pub const TIOCSWINSZ: u64 = 0x5414;

#[repr(C)]
pub struct WinSize { pub ws_row: u16, pub ws_col: u16, pub ws_xpixel: u16, pub ws_ypixel: u16 }

pub fn handle_ioctl(context: &mut CPUState) {
    let request = context.rsi;
    let arg = context.rdx as *mut WinSize;
    match request {
        TIOCGWINSZ => {
            let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
            if let Some(current) = tm.current_task_idx() {
                if let Some(thread) = tm.tasks[current].as_ref() {
                    let proc = thread.process.as_ref().expect("Thread has no process");
                    if !arg.is_null() { unsafe { (*arg).ws_row = *proc.terminal_height.lock(); (*arg).ws_col = *proc.terminal_width.lock(); (*arg).ws_xpixel = 0; (*arg).ws_ypixel = 0; } context.rax = 0; }
                    else { context.rax = u64::MAX; }
                } else { context.rax = u64::MAX; }
            } else { context.rax = u64::MAX; }
        }
        TIOCSWINSZ => {
            let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
            if let Some(current) = tm.current_task_idx() {
                if let Some(thread) = tm.tasks[current].as_mut() {
                    let proc = thread.process.as_ref().expect("Thread has no process");
                    if !arg.is_null() { unsafe { *proc.terminal_height.lock() = (*arg).ws_row; *proc.terminal_width.lock() = (*arg).ws_col; } context.rax = 0; }
                    else { context.rax = u64::MAX; }
                } else { context.rax = u64::MAX; }
            } else { context.rax = u64::MAX; }
        }
        _ => context.rax = u64::MAX,
    }
}