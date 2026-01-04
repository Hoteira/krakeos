use crate::interrupts::task::CPUState;
use alloc::vec::Vec;
use alloc::string::String;

pub fn sys_open(context: &mut CPUState) {
    let ptr = context.rdi as *const u8;
    let len = context.rsi as usize;
    let s = unsafe { core::slice::from_raw_parts(ptr, len) };
    let path_str_full = String::from_utf8_lossy(s);
    
    let path_parts: Vec<&str> = path_str_full.split('/').collect();
    if path_parts.len() < 1 || !path_parts[0].starts_with('@') {
        context.rax = u64::MAX; 
        return;
    }

    let disk_part = &path_parts[0][1..];
    let disk_id = if disk_part.starts_with("0x") || disk_part.starts_with("0X") {
        u8::from_str_radix(&disk_part[2..], 16).unwrap_or(0xFF)
    } else {
        disk_part.parse::<u8>().unwrap_or(0xFF)
    };

    if disk_id == 0xFF {
        context.rax = u64::MAX;
        return;
    }
    
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
        },
        Err(_) => context.rax = u64::MAX,
    }
}

pub fn sys_read_file(context: &mut CPUState) {
    let local_fd = context.rdi as usize;
    let buf_ptr = context.rsi as *mut u8;
    let len = context.rdx as usize;
    
    if buf_ptr.is_null() { context.rax = u64::MAX; return; }
    
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
                         },
                         Err(_) => context.rax = u64::MAX,
                     }
                 },
                 FileHandle::Pipe { pipe } => {
                     let n = pipe.read(buf);
                     context.rax = n as u64;
                 }
             }
        } else {
            context.rax = u64::MAX;
        }
    } else {
        context.rax = u64::MAX;
    }
}

pub fn sys_write_file(context: &mut CPUState) {
    let local_fd = context.rdi as usize;
    let buf_ptr = context.rsi as *const u8;
    let len = context.rdx as usize;

    if buf_ptr.is_null() { context.rax = u64::MAX; return; }

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
                         },
                         Err(_) => context.rax = u64::MAX,
                     }
                 },
                 FileHandle::Pipe { pipe } => {
                     let n = pipe.write(buf);
                     context.rax = n as u64;
                 }
             }
        } else {
            context.rax = u64::MAX;
        }
    } else {
        context.rax = u64::MAX;
    }
}

pub fn sys_readdir(context: &mut CPUState) {
    let local_fd = context.rdi as usize;
    let buf_ptr = context.rsi as *mut u8;
    let len = context.rdx as usize;
    
    if buf_ptr.is_null() { context.rax = u64::MAX; return; }

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
             use crate::fs::vfs::{FileHandle, FileType};
             match handle {
                 FileHandle::File { node, offset } => {
                     match node.children() {
                         Ok(children) => {
                             let start_idx = *offset as usize;
                             if start_idx >= children.len() {
                                 context.rax = 0; 
                             } else {
                                 let mut bytes_written = 0;
                                 let mut count = 0;
                                 let buf = unsafe { core::slice::from_raw_parts_mut(buf_ptr, len) };
                                 
                                 for child in children.iter().skip(start_idx) {
                                     let name = child.name();
                                     let name_bytes = name.as_bytes();
                                     let name_len = name_bytes.len();
                                     if bytes_written + 2 + name_len > len { break; }
                                     
                                     let type_byte = match child.kind() {
                                         FileType::File => 1,
                                         FileType::Directory => 2,
                                         FileType::Device => 3,
                                         _ => 0,
                                     };
                                     
                                     buf[bytes_written] = type_byte;
                                     buf[bytes_written + 1] = name_len as u8;
                                     buf[bytes_written + 2 .. bytes_written + 2 + name_len].copy_from_slice(name_bytes);
                                     bytes_written += 2 + name_len;
                                     count += 1;
                                 }
                                 *offset += count as u64;
                                 context.rax = bytes_written as u64;
                             }
                         },
                         Err(_) => context.rax = u64::MAX,
                     }
                 },
                 FileHandle::Pipe { .. } => context.rax = u64::MAX, 
             }
        } else {
            context.rax = u64::MAX;
        }
    } else {
        context.rax = u64::MAX;
    }
}

pub fn sys_stat(context: &mut CPUState) {
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

pub fn sys_close(context: &mut CPUState) {
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

pub fn sys_lseek(context: &mut CPUState) {
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
                 },
                 FileHandle::Pipe { .. } => context.rax = u64::MAX, 
             }
        } else {
            context.rax = u64::MAX;
        }
    } else {
        context.rax = u64::MAX;
    }
}

pub fn sys_create_fs_obj(context: &mut CPUState, is_dir: bool) {
    let ptr = context.rdi as *const u8;
    let len = context.rsi as usize;
    let s = unsafe { core::slice::from_raw_parts(ptr, len) };
    let path_str_full = String::from_utf8_lossy(s);
    
    let path_parts: Vec<&str> = path_str_full.split('/').collect();
    if path_parts.len() < 1 || !path_parts[0].starts_with('@') {
        context.rax = u64::MAX; 
        return;
    }

    let disk_part = &path_parts[0][1..];
    let disk_id = if disk_part.starts_with("0x") || disk_part.starts_with("0X") {
        u8::from_str_radix(&disk_part[2..], 16).unwrap_or(0xFF)
    } else {
        disk_part.parse::<u8>().unwrap_or(0xFF)
    };

    if disk_id == 0xFF {
        context.rax = u64::MAX;
        return;
    }
    
    let actual_path = if path_parts.len() > 1 { path_parts[1..].join("/") } else { String::from("") };
    
    if let Some(last_slash) = actual_path.rfind('/') {
        let parent_path = &actual_path[..last_slash];
        let new_name = &actual_path[last_slash+1..];
        if let Ok(mut parent) = crate::fs::vfs::open(disk_id, parent_path) {
            let res = if !is_dir { parent.create_file(new_name) } else { parent.create_dir(new_name) };
            match res {
                Ok(_) => context.rax = 0,
                Err(_) => context.rax = u64::MAX,
            }
        } else { context.rax = u64::MAX; }
    } else {
        if let Ok(mut root) = crate::fs::vfs::open(disk_id, "") {
            let res = if !is_dir { root.create_file(&actual_path) } else { root.create_dir(&actual_path) };
            match res {
                Ok(_) => context.rax = 0,
                Err(_) => context.rax = u64::MAX,
            }
        } else { context.rax = u64::MAX; }
    }
}

pub fn sys_remove(context: &mut CPUState) {
    let ptr = context.rdi as *const u8;
    let len = context.rsi as usize;
    let s = unsafe { core::slice::from_raw_parts(ptr, len) };
    let path_str_full = String::from_utf8_lossy(s);
    
    let path_parts: Vec<&str> = path_str_full.split('/').collect();
    if path_parts.len() < 1 || !path_parts[0].starts_with('@') { context.rax = u64::MAX; return; }

    let disk_part = &path_parts[0][1..];
    let disk_id = if disk_part.starts_with("0x") || disk_part.starts_with("0X") {
        u8::from_str_radix(&disk_part[2..], 16).unwrap_or(0xFF)
    } else {
        disk_part.parse::<u8>().unwrap_or(0xFF)
    };

    let actual_path = if path_parts.len() > 1 { path_parts[1..].join("/") } else { String::from("") };
    
    if let Some(last_slash) = actual_path.rfind('/') {
        let parent_path = &actual_path[..last_slash];
        let name = &actual_path[last_slash+1..];
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

pub fn sys_rename(context: &mut CPUState) {
    let old_ptr = context.rdi as *const u8;
    let old_len = context.rsi as usize;
    let new_ptr = context.rdx as *const u8;
    let new_len = context.r10 as usize;
    
    let s_old = unsafe { core::slice::from_raw_parts(old_ptr, old_len) };
    let s_new = unsafe { core::slice::from_raw_parts(new_ptr, new_len) };
    let path_old = String::from_utf8_lossy(s_old);
    let path_new = String::from_utf8_lossy(s_new);
    
    let parts_old: Vec<&str> = path_old.split('/').collect();
    if parts_old.len() < 1 || !parts_old[0].starts_with('@') { context.rax = u64::MAX; return; }
    
    let disk_part = &parts_old[0][1..];
    let disk_id = if disk_part.starts_with("0x") || disk_part.starts_with("0X") {
        u8::from_str_radix(&disk_part[2..], 16).unwrap_or(0xFF)
    } else {
        disk_part.parse::<u8>().unwrap_or(0xFF)
    };
    
    let actual_old = if parts_old.len() > 1 { parts_old[1..].join("/") } else { String::from("") };
    let parts_new: Vec<&str> = path_new.split('/').collect();
    let actual_new = if parts_new.len() > 1 { parts_new[1..].join("/") } else { String::from("") };

    let (parent_old, name_old) = if let Some(idx) = actual_old.rfind('/') {
        (&actual_old[..idx], &actual_old[idx+1..])
    } else {
        ("", actual_old.as_str())
    };
    let (parent_new, name_new) = if let Some(idx) = actual_new.rfind('/') {
        (&actual_new[..idx], &actual_new[idx+1..])
    } else {
        ("", actual_new.as_str())
    };
    
    if parent_old != parent_new {
        context.rax = u64::MAX; 
        return;
    }
    
    if let Ok(mut parent) = crate::fs::vfs::open(disk_id, parent_old) {
        match parent.rename(name_old, name_new) {
            Ok(_) => context.rax = 0,
            Err(_) => context.rax = u64::MAX,
        }
    } else { context.rax = u64::MAX; }
}

pub fn sys_pipe(context: &mut CPUState) {
    let fds_ptr = context.rdi as *mut i32;
    if fds_ptr.is_null() { context.rax = u64::MAX; return; }
    
    unsafe {
        use crate::fs::vfs::{OPEN_FILES, GLOBAL_FILE_REFCOUNT, FileHandle};
        use crate::fs::pipe::Pipe;
        let mut g1 = -1;
        let mut g2 = -1;
        for i in 3..256 {
            if OPEN_FILES[i].is_none() {
                if g1 == -1 { g1 = i as i32; } else { g2 = i as i32; break; }
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
                        if l1 == -1 { l1 = i as i32; } else { l2 = i as i32; break; }
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
            } else { context.rax = u64::MAX; }
        } else { context.rax = u64::MAX; }
    }
}

pub fn sys_poll(context: &mut CPUState) {
    use super::PollFd;
    use crate::interrupts::syscalls::POLLIN;
    use crate::interrupts::syscalls::POLLOUT;

    let fds_ptr = context.rdi as *const PollFd;
    let nfds = context.rsi as usize;
    if fds_ptr.is_null() || nfds == 0 { context.rax = 0; return; }

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
                                        if (pfd.events & POLLIN) != 0 && pipe.available() > 0 { pfd.revents |= POLLIN; }
                                        if (pfd.events & POLLOUT) != 0 { pfd.revents |= POLLOUT; }
                                    },
                                    FileHandle::File { .. } => {
                                        if (pfd.events & POLLIN) != 0 { pfd.revents |= POLLIN; }
                                        if (pfd.events & POLLOUT) != 0 { pfd.revents |= POLLOUT; }
                                    }
                                }
                            } else { pfd.revents = 8; }
                        } else { pfd.revents = 32; }
                    } else { pfd.revents = 32; }
                    if pfd.revents != 0 { ready_count += 1; }
                }
            }
        }
    }
    context.rax = ready_count as u64;
}
