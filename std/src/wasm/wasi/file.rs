use crate::{debug_print, println};
use crate::wasm::{Value, interpreter::Interpreter};
use crate::rust_alloc::string::{String, ToString};
use crate::rust_alloc::vec::Vec;
use crate::wasm::wasi::types::*;

fn resolve_path(interp: &Interpreter, dir_fd: u32, path_ptr: usize, path_len: usize) -> Result<String, __wasi_errno_t> {
    if path_ptr + path_len > interp.memory.len() { return Err(WASI_EFAULT); }
    let path_raw = &interp.memory[path_ptr..path_ptr+path_len];
    let path_str = core::str::from_utf8(path_raw).map_err(|_| WASI_EINVAL)?;

    let mut krake_path = if path_str.starts_with('/') || path_str.starts_with('@') {
        String::from(path_str)
    } else {
        let mut base = String::from("@0xE0/");
        for (f, p) in &interp.fd_paths {
            if *f == dir_fd as usize {
                base = p.clone();
                break;
            }
        }
        if !base.ends_with('/') { base.push('/'); }
        base + path_str
    };

    if krake_path.starts_with('/') { krake_path = String::from("@0xE0") + &krake_path; }
    else if !krake_path.starts_with('@') { krake_path = String::from("@0xE0/") + &krake_path; }
    
    // Normalize: remove trailing slash if not root
    if krake_path.len() > 6 && krake_path.ends_with('/') { krake_path.pop(); }
    if krake_path == "@0xE0/." { krake_path = String::from("@0xE0/"); }

    Ok(krake_path)
}

pub fn register(interpreter: &mut Interpreter, mod_name: &str) {
    interpreter.add_host_function(mod_name, "fd_close", |interp, args| {
        let fd = match args.get(0) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        crate::os::debug_print(crate::rust_alloc::format!("WASI: fd_close fd={}\n", fd).as_str());
        
        if let Some(pos) = interp.fd_paths.iter().position(|(f, _)| *f == fd) {
            interp.fd_paths.remove(pos);
        }

        crate::os::file_close(fd);
        Some(Value::I32(WASI_ESUCCESS as i32))
    });

    interpreter.add_host_function(mod_name, "fd_datasync", |_interp, _args| { crate::os::yield_task(); Some(Value::I32(WASI_ESUCCESS as i32)) });
    interpreter.add_host_function(mod_name, "fd_sync", |_interp, _args| { crate::os::yield_task(); Some(Value::I32(WASI_ESUCCESS as i32)) });
    
    interpreter.add_host_function(mod_name, "fd_fdstat_get", |interp, args| {
        let fd = match args.get(0) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let stat_ptr = match args.get(1) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        
        let mut file_type = WASI_FILETYPE_REGULAR_FILE;
        if fd <= 2 { file_type = WASI_FILETYPE_CHARACTER_DEVICE; }
        else {
            for (f, p) in &interp.fd_paths {
                if *f == fd {
                    if crate::fs::read_dir(p).is_ok() {
                        file_type = WASI_FILETYPE_DIRECTORY;
                    }
                    break;
                }
            }
        }

        if stat_ptr + 24 > interp.memory.len() { return Some(Value::I32(WASI_EFAULT as i32)); }
        
        for j in 0..24 { interp.memory[stat_ptr + j] = 0; }
        
        let rights: u64 = 0xFFFFFFFFFFFFFFFF; 
        interp.memory[stat_ptr+8..stat_ptr+16].copy_from_slice(&rights.to_le_bytes());
        interp.memory[stat_ptr+16..stat_ptr+24].copy_from_slice(&rights.to_le_bytes());
        interp.memory[stat_ptr] = file_type;
        
        Some(Value::I32(WASI_ESUCCESS as i32))
    });

    interpreter.add_host_function(mod_name, "fd_fdstat_set_flags", |_interp, _args| { Some(Value::I32(WASI_ESUCCESS as i32)) });
    interpreter.add_host_function(mod_name, "fd_fdstat_set_rights", |_interp, _args| { Some(Value::I32(WASI_ESUCCESS as i32)) });

    interpreter.add_host_function(mod_name, "fd_filestat_get", |interp, args| {
        let fd = match args.get(0) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let stat_ptr = match args.get(1) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        
        let size = unsafe { crate::os::syscall(5, fd as u64, 0, 0) }; 
        
        let mut file_type = WASI_FILETYPE_REGULAR_FILE;
        if fd <= 2 { file_type = WASI_FILETYPE_CHARACTER_DEVICE; }
        else {
            for (f, p) in &interp.fd_paths {
                if *f == fd {
                    if crate::fs::read_dir(p).is_ok() {
                        file_type = WASI_FILETYPE_DIRECTORY;
                    }
                    break;
                }
            }
        }

        if stat_ptr + 64 > interp.memory.len() { return Some(Value::I32(WASI_EFAULT as i32)); }
        
        for j in 0..64 { interp.memory[stat_ptr + j] = 0; }
        
        interp.memory[stat_ptr+16] = file_type;
        interp.memory[stat_ptr+32..stat_ptr+40].copy_from_slice(&size.to_le_bytes());
        
        Some(Value::I32(WASI_ESUCCESS as i32))
    });

    interpreter.add_host_function(mod_name, "fd_filestat_set_size", |_interp, args| {
        let fd = match args.get(0) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let size = match args.get(1) { Some(Value::I64(v)) => *v as u64, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let res = crate::os::file_truncate(fd, size);
        Some(Value::I32(if res == 0 { WASI_ESUCCESS as i32 } else { WASI_EIO as i32 }))
    });

    interpreter.add_host_function(mod_name, "fd_filestat_set_times", |_interp, _args| { Some(Value::I32(WASI_ESUCCESS as i32)) });

    interpreter.add_host_function(mod_name, "fd_pread", |interp, args| {
        let fd = match args.get(0) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let iovs_ptr = match args.get(1) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let iovs_len = match args.get(2) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let offset = match args.get(3) { Some(Value::I64(v)) => *v, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let nread_ptr = match args.get(4) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        
        let old_off = crate::os::file_seek(fd, 0, 1);
        crate::os::file_seek(fd, offset, 0);
        
        let mut total_read = 0;
        for i in 0..iovs_len {
            let base_ptr = iovs_ptr + (i * 8);
            if base_ptr + 8 > interp.memory.len() { return Some(Value::I32(WASI_EFAULT as i32)); }
            let buf_ptr = u32::from_le_bytes(interp.memory[base_ptr..base_ptr+4].try_into().unwrap()) as usize;
            let buf_len = u32::from_le_bytes(interp.memory[base_ptr+4..base_ptr+8].try_into().unwrap()) as usize;
            
            if buf_ptr + buf_len > interp.memory.len() { return Some(Value::I32(WASI_EFAULT as i32)); }
            
            let n = crate::os::file_read(fd, &mut interp.memory[buf_ptr..buf_ptr+buf_len]);
            if n == usize::MAX || n == 0 { break; } 
            total_read += n;
            if n < buf_len { break; }
        }
        
        crate::os::file_seek(fd, old_off as i64, 0);
        
        if nread_ptr + 4 <= interp.memory.len() { interp.memory[nread_ptr..nread_ptr+4].copy_from_slice(&(total_read as u32).to_le_bytes()); }
        Some(Value::I32(WASI_ESUCCESS as i32))
    });

    interpreter.add_host_function(mod_name, "fd_pwrite", |interp, args| {
        let fd = match args.get(0) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let iovs_ptr = match args.get(1) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let iovs_len = match args.get(2) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let offset = match args.get(3) { Some(Value::I64(v)) => *v, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let nwritten_ptr = match args.get(4) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        
        let old_off = crate::os::file_seek(fd, 0, 1);
        crate::os::file_seek(fd, offset, 0);
        
        let mut total_written = 0;
        for i in 0..iovs_len {
            let base_ptr = iovs_ptr + (i * 8);
            if base_ptr + 8 > interp.memory.len() { return Some(Value::I32(WASI_EFAULT as i32)); }
            let buf_ptr = u32::from_le_bytes(interp.memory[base_ptr..base_ptr+4].try_into().unwrap()) as usize;
            let buf_len = u32::from_le_bytes(interp.memory[base_ptr+4..base_ptr+8].try_into().unwrap()) as usize;
            
            if buf_ptr + buf_len > interp.memory.len() { return Some(Value::I32(WASI_EFAULT as i32)); }
            let data = &interp.memory[buf_ptr..buf_ptr+buf_len];
            
            if fd == 1 || fd == 2 {
                if let Ok(s) = core::str::from_utf8(data) { crate::os::debug_print(s); }
                total_written += buf_len;
            } else {
                let n = crate::os::file_write(fd, data);
                total_written += n;
            }
        }
        crate::os::file_seek(fd, old_off as i64, 0);
        
        if nwritten_ptr + 4 <= interp.memory.len() { interp.memory[nwritten_ptr..nwritten_ptr+4].copy_from_slice(&(total_written as u32).to_le_bytes()); }
        Some(Value::I32(WASI_ESUCCESS as i32))
    });

    interpreter.add_host_function(mod_name, "fd_read", |interp, args| {
        let fd = match args.get(0) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let iovs_ptr = match args.get(1) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let iovs_len = match args.get(2) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let nread_ptr = match args.get(3) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        
        let mut total_read = 0;
        for i in 0..iovs_len {
            let base_ptr = iovs_ptr + (i * 8);
            if base_ptr + 8 > interp.memory.len() { return Some(Value::I32(WASI_EFAULT as i32)); }
            let buf_ptr = u32::from_le_bytes(interp.memory[base_ptr..base_ptr+4].try_into().unwrap()) as usize;
            let buf_len = u32::from_le_bytes(interp.memory[base_ptr+4..base_ptr+8].try_into().unwrap()) as usize;
            
            if buf_ptr + buf_len > interp.memory.len() { return Some(Value::I32(WASI_EFAULT as i32)); }
            
            let n = crate::os::file_read(fd, &mut interp.memory[buf_ptr..buf_ptr+buf_len]);
            if n == usize::MAX || n == 0 { break; }
            total_read += n;
            if n < buf_len { break; }
        }
        
        if nread_ptr + 4 <= interp.memory.len() { interp.memory[nread_ptr..nread_ptr+4].copy_from_slice(&(total_read as u32).to_le_bytes()); }
        Some(Value::I32(WASI_ESUCCESS as i32))
    });

    interpreter.add_host_function(mod_name, "fd_readdir", |interp, args| {
        let fd = match args.get(0) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let buf_ptr = match args.get(1) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let buf_len = match args.get(2) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let cookie = match args.get(3) { Some(Value::I64(v)) => *v as u64, _ => 0 };
        let nused_ptr = match args.get(4) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };

        let mut path = String::new();
        let mut found = false;
        for (f, p) in &interp.fd_paths {
            if *f == fd {
                path = p.clone();
                found = true;
                break;
            }
        }
        
        if !found { 
            return Some(Value::I32(WASI_EBADF as i32)); 
        }

        if path == "/" { path = String::from("@0xE0/"); }
        else if path.starts_with('/') && !path.starts_with("@0xE0") { 
             path = String::from("@0xE0") + &path;
        }

                match crate::fs::read_dir(&path) {

                    Ok(entries) => {

                        let mut wasi_used = 0;

                        

                                                        for (i, entry) in entries.into_iter().enumerate().skip(cookie as usize) {

                        

                                                            let name_bytes = entry.name.as_bytes();

                        

                                        

                        

                        

                            let name_len = name_bytes.len();

                            let dirent_size = 24; 

                            

                            if wasi_used + dirent_size + name_len > buf_len { 

                                if wasi_used == 0 {

                                    return Some(Value::I32(WASI_EINVAL as i32));

                                }

                                break; 

                            }

        

                            let next_cookie = (i + 1) as u64;

                            let inode = (i + 1) as u64; 

                            let d_type = match entry.file_type {

                                crate::fs::FileType::Directory => WASI_FILETYPE_DIRECTORY,

                                crate::fs::FileType::File => WASI_FILETYPE_REGULAR_FILE,

                                _ => WASI_FILETYPE_UNKNOWN,

                            };

        

                            let h_off = buf_ptr + wasi_used;

                            if h_off + 24 + name_len > interp.memory.len() { break; }

        

                            for j in 0..24 { interp.memory[h_off + j] = 0; }

        

                            interp.memory[h_off..h_off+8].copy_from_slice(&next_cookie.to_le_bytes()); 

                            interp.memory[h_off+8..h_off+16].copy_from_slice(&inode.to_le_bytes()); 

                            interp.memory[h_off+16..h_off+20].copy_from_slice(&(name_len as u32).to_le_bytes()); 

                            interp.memory[h_off+20] = d_type; 

                            

                            interp.memory[h_off+24..h_off+24+name_len].copy_from_slice(name_bytes);

        

                            wasi_used += 24 + name_len;

                        }

        

                if nused_ptr + 4 <= interp.memory.len() {
                    interp.memory[nused_ptr..nused_ptr+4].copy_from_slice(&(wasi_used as u32).to_le_bytes());
                }


        

                        Some(Value::I32(WASI_ESUCCESS as i32))

                    }

                    Err(_) => Some(Value::I32(WASI_ENOENT as i32))

                }

        
    });

    interpreter.add_host_function(mod_name, "fd_renumber", |_interp, _args| Some(Value::I32(WASI_ENOTSUP as i32)));

    interpreter.add_host_function(mod_name, "fd_seek", |interp, args| {
        let fd = match args.get(0) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let offset = match args.get(1) { Some(Value::I64(v)) => *v, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let whence = match args.get(2) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let newoff_ptr = match args.get(3) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        
        let res = crate::os::file_seek(fd, offset, whence);
        if res == u64::MAX { return Some(Value::I32(WASI_EIO as i32)); }
        
        if newoff_ptr + 8 <= interp.memory.len() { interp.memory[newoff_ptr..newoff_ptr+8].copy_from_slice(&res.to_le_bytes()); }
        Some(Value::I32(WASI_ESUCCESS as i32))
    });

    interpreter.add_host_function(mod_name, "fd_tell", |interp, args| {
        let fd = match args.get(0) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let offset_ptr = match args.get(1) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        
        let res = crate::os::file_seek(fd, 0, 1); 
        if offset_ptr + 8 <= interp.memory.len() { interp.memory[offset_ptr..offset_ptr+8].copy_from_slice(&res.to_le_bytes()); }
        Some(Value::I32(WASI_ESUCCESS as i32))
    });

    interpreter.add_host_function(mod_name, "fd_write", |interp, args| {
        let fd = match args.get(0) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let iovs_ptr = match args.get(1) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let iovs_len = match args.get(2) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let nwritten_ptr = match args.get(3) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        
        let mut total_written = 0;
        for i in 0..iovs_len {
            let base_ptr = iovs_ptr + (i * 8);
            if base_ptr + 8 > interp.memory.len() { return Some(Value::I32(WASI_EFAULT as i32)); }
            let buf_ptr = u32::from_le_bytes(interp.memory[base_ptr..base_ptr+4].try_into().unwrap()) as usize;
            let buf_len = u32::from_le_bytes(interp.memory[base_ptr+4..base_ptr+8].try_into().unwrap()) as usize;
            
            if buf_ptr + buf_len > interp.memory.len() { return Some(Value::I32(WASI_EFAULT as i32)); }
            let data = &interp.memory[buf_ptr..buf_ptr+buf_len];
            
            if fd == 1 || fd == 2 {
                if let Ok(s) = core::str::from_utf8(data) { crate::os::debug_print(s); }
                total_written += buf_len;
            } else {
                let n = crate::os::file_write(fd, data);
                total_written += n;
            }
        }
        
        if nwritten_ptr + 4 <= interp.memory.len() { interp.memory[nwritten_ptr..nwritten_ptr+4].copy_from_slice(&(total_written as u32).to_le_bytes()); }
        Some(Value::I32(WASI_ESUCCESS as i32))
    });

    interpreter.add_host_function(mod_name, "path_create_directory", |interp, args| {
        let dir_fd = match args.get(0) { Some(Value::I32(v)) => *v as u32, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let path_ptr = match args.get(1) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let path_len = match args.get(2) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        
        let krake_path = match resolve_path(interp, dir_fd, path_ptr, path_len) {
            Ok(p) => p,
            Err(e) => return Some(Value::I32(e as i32)),
        };

        let res = unsafe { crate::os::syscall(83, krake_path.as_ptr() as u64, krake_path.len() as u64, 0) };
        Some(Value::I32(if res == 0 { WASI_ESUCCESS as i32 } else { WASI_EACCES as i32 }))
    });

    interpreter.add_host_function(mod_name, "path_filestat_get", |interp, args| {
        let dir_fd = match args.get(0) { Some(Value::I32(v)) => *v as u32, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let _flags = match args.get(1) { Some(Value::I32(v)) => *v, _ => 0 };
        let path_ptr = match args.get(2) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let path_len = match args.get(3) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let stat_ptr = match args.get(4) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        
        let krake_path = match resolve_path(interp, dir_fd, path_ptr, path_len) {
            Ok(p) => p,
            Err(e) => return Some(Value::I32(e as i32)),
        };

        let fd = unsafe { crate::os::syscall(2, krake_path.as_ptr() as u64, krake_path.len() as u64, 0) };
        if fd == u64::MAX { return Some(Value::I32(WASI_ENOENT as i32)); }
        
        let size = unsafe { crate::os::syscall(5, fd, 0, 0) };
        unsafe { crate::os::syscall(3, fd, 0, 0); } 
        
        if stat_ptr + 64 <= interp.memory.len() {
            for j in 0..64 { interp.memory[stat_ptr + j] = 0; }
            interp.memory[stat_ptr+32..stat_ptr+40].copy_from_slice(&size.to_le_bytes());
            interp.memory[stat_ptr+16] = WASI_FILETYPE_REGULAR_FILE; 
        }
        Some(Value::I32(WASI_ESUCCESS as i32))
    });

    interpreter.add_host_function(mod_name, "path_filestat_set_times", |_interp, _args| { Some(Value::I32(WASI_ESUCCESS as i32)) });
    interpreter.add_host_function(mod_name, "path_link", |_interp, _args| { Some(Value::I32(WASI_ENOTSUP as i32)) });
    
    interpreter.add_host_function(mod_name, "path_open", |interp, args| {
        let dir_fd = match args.get(0) { Some(Value::I32(v)) => *v as u32, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let _dirflags = match args.get(1) { Some(Value::I32(v)) => *v, _ => 0 };
        let path_ptr = match args.get(2) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let path_len = match args.get(3) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let oflags = match args.get(4) { Some(Value::I32(v)) => *v, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let rights_base = match args.get(5) { Some(Value::I64(v)) => *v, _ => 0 };
        let _rights_inheriting = match args.get(6) { Some(Value::I64(v)) => *v, _ => 0 };
        let _fd_flags = match args.get(7) { Some(Value::I32(v)) => *v, _ => 0 };
        let opened_fd_ptr = match args.get(8) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        
        let krake_path = match resolve_path(interp, dir_fd, path_ptr, path_len) {
            Ok(p) => p,
            Err(e) => return Some(Value::I32(e as i32)),
        };

        crate::os::debug_print(crate::rust_alloc::format!("WASI: path_open path='{}'\n", krake_path).as_str());

        let mut krake_flags = 0;
        if (rights_base & 64) != 0 { krake_flags = 2; } 
        if (oflags & 1) != 0 { unsafe { crate::os::syscall(85, krake_path.as_ptr() as u64, krake_path.len() as u64, 0); } } 
        
        let fd = unsafe { crate::os::syscall(2, krake_path.as_ptr() as u64, krake_path.len() as u64, krake_flags) };
        if fd == u64::MAX { return Some(Value::I32(WASI_ENOENT as i32)); }
        
        if (oflags & 8) != 0 { 
            unsafe { crate::os::syscall(77, fd, 0, 0); }
        }

        if opened_fd_ptr + 4 <= interp.memory.len() { interp.memory[opened_fd_ptr..opened_fd_ptr+4].copy_from_slice(&(fd as u32).to_le_bytes()); }
        
        interp.fd_paths.push((fd as usize, krake_path.clone()));

        Some(Value::I32(WASI_ESUCCESS as i32))
    });

    interpreter.add_host_function(mod_name, "path_readlink", |_interp, _args| { Some(Value::I32(WASI_ENOTSUP as i32)) });

    interpreter.add_host_function(mod_name, "path_remove_directory", |interp, args| {
        let dir_fd = match args.get(0) { Some(Value::I32(v)) => *v as u32, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let path_ptr = match args.get(1) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let path_len = match args.get(2) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        
        let krake_path = match resolve_path(interp, dir_fd, path_ptr, path_len) {
            Ok(p) => p,
            Err(e) => return Some(Value::I32(e as i32)),
        };

        let res = unsafe { crate::os::syscall(87, krake_path.as_ptr() as u64, krake_path.len() as u64, 0) };
        Some(Value::I32(if res == 0 { WASI_ESUCCESS as i32 } else { WASI_EACCES as i32 }))
    });

    interpreter.add_host_function(mod_name, "path_rename", |interp, args| {
        let old_dir_fd = match args.get(0) { Some(Value::I32(v)) => *v as u32, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let old_ptr = match args.get(1) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let old_len = match args.get(2) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let new_dir_fd = match args.get(3) { Some(Value::I32(v)) => *v as u32, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let new_ptr = match args.get(4) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let new_len = match args.get(5) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        
        let old_path = match resolve_path(interp, old_dir_fd, old_ptr, old_len) { Ok(p) => p, Err(e) => return Some(Value::I32(e as i32)) };
        let new_path = match resolve_path(interp, new_dir_fd, new_ptr, new_len) { Ok(p) => p, Err(e) => return Some(Value::I32(e as i32)) };
        
        let res = unsafe { crate::os::syscall4(82, old_path.as_ptr() as u64, old_path.len() as u64, new_path.as_ptr() as u64, new_path.len() as u64) };
        Some(Value::I32(if res == 0 { WASI_ESUCCESS as i32 } else { WASI_EACCES as i32 }))
    });

    interpreter.add_host_function(mod_name, "path_symlink", |_interp, _args| { Some(Value::I32(WASI_ENOTSUP as i32)) });

    interpreter.add_host_function(mod_name, "path_unlink_file", |interp, args| {
        let dir_fd = match args.get(0) { Some(Value::I32(v)) => *v as u32, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let path_ptr = match args.get(1) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let path_len = match args.get(2) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        
        let krake_path = match resolve_path(interp, dir_fd, path_ptr, path_len) { Ok(p) => p, Err(e) => return Some(Value::I32(e as i32)) };

        let res = unsafe { crate::os::syscall(87, krake_path.as_ptr() as u64, krake_path.len() as u64, 0) };
        Some(Value::I32(if res == 0 { WASI_ESUCCESS as i32 } else { WASI_EACCES as i32 }))
    });

    interpreter.add_host_function(mod_name, "fd_prestat_get", |interp, args| {
        let fd = match args.get(0) { Some(Value::I32(v)) => *v, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        let prestat_ptr = match args.get(1) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };
        
        if fd == 3 {
            if prestat_ptr + 8 <= interp.memory.len() { 
                interp.memory[prestat_ptr] = 0; 
                interp.memory[prestat_ptr+4..prestat_ptr+8].copy_from_slice(&1u32.to_le_bytes()); 
            }
            return Some(Value::I32(WASI_ESUCCESS as i32));
        }
        Some(Value::I32(WASI_EBADF as i32))
    });

        interpreter.add_host_function(mod_name, "fd_prestat_dir_name", |interp, args| {

            let fd = match args.get(0) { Some(Value::I32(v)) => *v, _ => return Some(Value::I32(WASI_EINVAL as i32)) };

            let path_ptr = match args.get(1) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };

            let path_len = match args.get(2) { Some(Value::I32(v)) => *v as usize, _ => return Some(Value::I32(WASI_EINVAL as i32)) };

            

            if fd == 3 {

                if path_len < 1 { return Some(Value::I32(WASI_ENAMETOOLONG as i32)); }

                if path_ptr + 1 > interp.memory.len() { return Some(Value::I32(WASI_EFAULT as i32)); }

                interp.memory[path_ptr] = b'/'; 

                return Some(Value::I32(WASI_ESUCCESS as i32)); 

            }

            Some(Value::I32(WASI_EBADF as i32))

        });

    }

    