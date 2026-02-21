use crate::rust_alloc::{vec, vec::Vec};
use crate::wasm::{
    common::{config::Config, value::Value},
    interpreter::store::{HaltExecutionError, Store},
    wasi::ctx::{InputStreamSource, OutputStreamSource, WasiResource},
};
use super::{call_cabi_realloc, read_bytes, read_mem, write_bytes, write_u32, write_u64};

fn get_fd<T: Config>(store: &mut Store<'_, T>, handle: i32) -> Result<i32, HaltExecutionError> {
    if handle >= 0 && handle <= 2 {
        let wasi = store.wasi_ctx.as_ref().ok_or(HaltExecutionError(1))?;
        return Ok(wasi.env.stdio_map()[handle as usize]);
    }
    let wasi = store.wasi_ctx.as_mut().ok_or(HaltExecutionError(1))?;
    match wasi.resource_table.get(&handle) {
        Some(WasiResource::Descriptor(fd)) => Ok(*fd),
        Some(WasiResource::Directory(_)) => Ok(3),
        _ => Err(HaltExecutionError(1)) // EBADF
    }
}

pub fn get_directories<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let ret_ptr = match args.get(0) {
        Some(Value::I32(v)) => *v as u32,
        _ => return Ok(vec![]),
    };

    let wasi = store.wasi_ctx.as_ref().ok_or(HaltExecutionError(1))?;
    let mut preopens = Vec::new();
    for (id, res) in &wasi.resource_table {
        if let WasiResource::Directory(path) = res {
            preopens.push((*id, path.clone()));
        }
    }

    let count = preopens.len() as u32;
    let array_ptr = if count > 0 {
        call_cabi_realloc(store, count * 12, 4)?
    } else {
        0
    };

    for (i, (id, path)) in preopens.into_iter().enumerate() {
        let bytes = path.as_bytes();
        let s_ptr = call_cabi_realloc(store, bytes.len() as u32, 1)?;
        write_bytes(store, s_ptr, bytes).map_err(|_| HaltExecutionError(1))?;

        let tuple_off = array_ptr + (i as u32 * 12);
        write_u32(store, tuple_off, id as u32).map_err(|_| HaltExecutionError(1))?;
        write_u32(store, tuple_off + 4, s_ptr).map_err(|_| HaltExecutionError(1))?;
        write_u32(store, tuple_off + 8, bytes.len() as u32).map_err(|_| HaltExecutionError(1))?;
    }

    write_u32(store, ret_ptr, array_ptr).map_err(|_| HaltExecutionError(1))?;
    write_u32(store, ret_ptr + 4, count).map_err(|_| HaltExecutionError(1))?;

    Ok(vec![])
}

pub fn filesystem_types_read_via_stream<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let handle = match args.get(0) { Some(Value::I32(v)) => *v as i32, _ => return Err(HaltExecutionError(1)) };
    let ret_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
    
    let wasi = store.wasi_ctx.as_mut().ok_or(HaltExecutionError(1))?;
    let fd_res = match wasi.resource_table.get(&handle) {
        Some(WasiResource::File(f)) => Ok(f.as_raw_fd()),
        Some(WasiResource::Descriptor(fd)) => Ok(*fd as usize),
        _ => Err(5),
    };

    match fd_res {
        Ok(fd) => {
            let id = wasi.next_resource_id;
            wasi.next_resource_id += 1;
            wasi.resource_table.insert(id, WasiResource::InputStream(InputStreamSource::GuestFd(fd as i32)));
            let _ = write_u32(store, ret_ptr, 0);
            let _ = write_u32(store, ret_ptr + 4, id as u32);
        }
        Err(e) => {
            let _ = write_u32(store, ret_ptr, 1);
            let _ = write_u32(store, ret_ptr + 4, e as u32);
        }
    }
    Ok(vec![])
}

pub fn filesystem_types_write_via_stream<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let handle = match args.get(0) { Some(Value::I32(v)) => *v as i32, _ => return Err(HaltExecutionError(1)) };
    let ret_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
    
    let wasi = store.wasi_ctx.as_mut().ok_or(HaltExecutionError(1))?;
    let fd_res = match wasi.resource_table.get(&handle) {
        Some(WasiResource::File(f)) => Ok(f.as_raw_fd()),
        Some(WasiResource::Descriptor(fd)) => Ok(*fd as usize),
        _ => Err(5),
    };

    match fd_res {
        Ok(fd) => {
            let id = wasi.next_resource_id;
            wasi.next_resource_id += 1;
            wasi.resource_table.insert(id, WasiResource::OutputStream(OutputStreamSource::GuestFd(fd as i32)));
            let _ = write_u32(store, ret_ptr, 0);
            let _ = write_u32(store, ret_ptr + 4, id as u32);
        }
        Err(e) => {
            let _ = write_u32(store, ret_ptr, 1);
            let _ = write_u32(store, ret_ptr + 4, e as u32);
        }
    }
    Ok(vec![])
}

pub fn filesystem_types_append_via_stream<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    filesystem_types_write_via_stream(store, args)
}

pub fn descriptor_type<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let handle = match args.get(0) { Some(Value::I32(v)) => *v as i32, _ => return Err(HaltExecutionError(1)) };
    let ret_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
    let fd = match get_fd(store, handle) {
        Ok(fd) => fd,
        Err(_) => {
            let _ = write_u32(store, ret_ptr, 1);
            let _ = write_u32(store, ret_ptr + 4, 8);
            return Ok(vec![]);
        }
    };
    match store.wasi_ctx.as_ref().unwrap().env.fd_fdstat_get(fd) {
        Ok(stat) => {
            let _ = write_u32(store, ret_ptr, 0);
            let _ = write_u32(store, ret_ptr + 4, stat.filetype as u32);
        }
        Err(e) => {
            let _ = write_u32(store, ret_ptr, 1);
            let _ = write_u32(store, ret_ptr + 4, e as u32);
        }
    }
    Ok(vec![])
}

pub fn descriptor_stat<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let handle = match args.get(0) { Some(Value::I32(v)) => *v as i32, _ => return Err(HaltExecutionError(1)) };
    let ret_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
    let fd = match get_fd(store, handle) {
        Ok(fd) => fd,
        Err(_) => {
            let _ = write_u32(store, ret_ptr, 1);
            let _ = write_u32(store, ret_ptr + 4, 8);
            return Ok(vec![]);
        }
    };
    match store.wasi_ctx.as_ref().unwrap().env.fd_filestat_get(fd) {
        Ok(stat) => {
            // Write Result::Ok tag (0) at ret_ptr
            let _ = write_u32(store, ret_ptr, 0);
            
            // Write KrakeOS Stat record inline at ret_ptr + 8 (padding at +4)
            let base = ret_ptr + 8;
            let _ = write_u64(store, base, stat.dev);
            let _ = write_u64(store, base + 8, stat.ino);
            let _ = write_u32(store, base + 16, 0); // mode (todo)
            let _ = write_u32(store, base + 20, stat.nlink as u32);
            let _ = write_u64(store, base + 24, stat.size);
            let _ = write_u64(store, base + 32, stat.atime / 1_000_000_000);
            let _ = write_u64(store, base + 40, stat.mtime / 1_000_000_000);
            let _ = write_u64(store, base + 48, stat.ctime / 1_000_000_000);
        }
        Err(e) => {
            let _ = write_u32(store, ret_ptr, 1);
            let _ = write_u32(store, ret_ptr + 4, e as u32);
        }
    }
    Ok(vec![])
}

pub fn descriptor_open_at<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let dir_handle = match args.get(0) { Some(Value::I32(v)) => *v as i32, _ => return Err(HaltExecutionError(1)) };
    let dirflags = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
    let path_ptr = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
    let path_len = match args.get(3) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
    let oflags = match args.get(4) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
    let flags_val = match args.get(5) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
    let ret_ptr = match args.get(6) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };

    let dirfd = match get_fd(store, dir_handle) {
        Ok(fd) => fd,
        Err(_) => {
            let _ = write_u32(store, ret_ptr, 1);
            let _ = write_u32(store, ret_ptr + 4, 8);
            return Ok(vec![]);
        }
    };
    let mut path_buf = vec![0u8; path_len as usize];
    if read_mem(store, path_ptr, &mut path_buf).is_err() {
        let _ = write_u32(store, ret_ptr, 1);
        let _ = write_u32(store, ret_ptr + 4, 21); // EINVAL
        return Ok(vec![]);
    }
    let path = crate::rust_alloc::string::String::from_utf8_lossy(&path_buf).into_owned();

    let rights = 0x3F;
    match store.wasi_ctx.as_mut().unwrap().env.path_open(dirfd, dirflags, &path, oflags, rights, rights, flags_val as u16) {
        Ok(fd) => {
            let wasi = store.wasi_ctx.as_mut().unwrap();
            let id = wasi.next_resource_id;
            wasi.next_resource_id += 1;
            wasi.resource_table.insert(id, WasiResource::Descriptor(fd));
            
            let _ = write_u32(store, ret_ptr, 0); // Ok tag
            let _ = write_u32(store, ret_ptr + 4, id as u32);
        }
        Err(e) => {
            let _ = write_u32(store, ret_ptr, 1); // Err tag
            let _ = write_u32(store, ret_ptr + 4, e as u32);
        }
    }
    Ok(vec![])
}

pub fn descriptor_read_directory<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let handle = match args.get(0) { Some(Value::I32(v)) => *v as i32, _ => return Err(HaltExecutionError(1)) };
    let ret_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
    let fd = match get_fd(store, handle) {
        Ok(fd) => fd,
        Err(_) => {
            let _ = write_u32(store, ret_ptr, 1);
            let _ = write_u32(store, ret_ptr + 4, 8);
            return Ok(vec![]);
        }
    };
    
    match store.wasi_ctx.as_mut().unwrap().env.fd_readdir(fd, 0) {
        Ok(entries) => {
            let wasi = store.wasi_ctx.as_mut().unwrap();
            let id = wasi.next_resource_id;
            wasi.next_resource_id += 1;
            wasi.resource_table.insert(id, WasiResource::DirStream { entries, index: 0 });
            
            let _ = write_u32(store, ret_ptr, 0);
            let _ = write_u32(store, ret_ptr + 4, id as u32);
        }
        Err(e) => {
            let _ = write_u32(store, ret_ptr, 1);
            let _ = write_u32(store, ret_ptr + 4, e as u32);
        }
    }
    Ok(vec![])
}

pub fn directory_entry_stream_read_directory_entry<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let handle = match args.get(0) { Some(Value::I32(v)) => *v as i32, _ => return Err(HaltExecutionError(1)) };
    let ret_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };

    let entry_res = {
        let wasi = store.wasi_ctx.as_mut().ok_or(HaltExecutionError(1))?;
        match wasi.resource_table.get_mut(&handle) {
            Some(WasiResource::DirStream { entries, index }) => {
                if *index < entries.len() {
                    let e = entries[*index].clone();
                    *index += 1;
                    Ok(Some(e))
                } else {
                    Ok(None)
                }
            }
            _ => Err(8),
        }
    };

    match entry_res {
        Ok(entry) => {
            if let Some((name, ty, inode)) = entry {
                let name_bytes = name.as_bytes();
                let name_ptr = call_cabi_realloc(store, name_bytes.len() as u32, 1)?;
                let _ = write_bytes(store, name_ptr, name_bytes);

                let _ = write_u32(store, ret_ptr, 0); // Ok Result
                let _ = write_u32(store, ret_ptr + 4, 1); // Some Option
                let payload_ptr = ret_ptr + 8;
                let _ = write_bytes(store, payload_ptr, &[ty]); 
                let _ = write_u32(store, payload_ptr + 4, name_ptr);
                let _ = write_u32(store, payload_ptr + 8, name_bytes.len() as u32);
                let _ = write_u64(store, payload_ptr + 16, inode);
            } else {
                let _ = write_u32(store, ret_ptr, 0); // Ok Result
                let _ = write_u32(store, ret_ptr + 4, 0); // None Option
            }
        }
        Err(e) => {
            let _ = write_u32(store, ret_ptr, 1);
            let _ = write_u32(store, ret_ptr + 4, e as u32);
        }
    }
    
    Ok(vec![])
}

pub fn descriptor_stat_at<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let handle = match args.get(0) { Some(Value::I32(v)) => *v as i32, _ => return Err(HaltExecutionError(1)) };
    let flags = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
    let ptr = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
    let len = match args.get(3) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
    let ret_ptr = match args.get(4) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };

    let fd = match get_fd(store, handle) {
        Ok(fd) => fd,
        Err(_) => {
            let _ = write_u32(store, ret_ptr, 1);
            let _ = write_u32(store, ret_ptr + 4, 8);
            return Ok(vec![]);
        }
    };
    let mut pb = vec![0u8; len as usize];
    if read_mem(store, ptr, &mut pb).is_err() {
        let _ = write_u32(store, ret_ptr, 1);
        let _ = write_u32(store, ret_ptr + 4, 21);
        return Ok(vec![]);
    }
    let path = crate::rust_alloc::string::String::from_utf8_lossy(&pb).into_owned();

    match store.wasi_ctx.as_mut().unwrap().env.path_filestat_get(fd, flags, &path) {
        Ok(stat) => {
            let _ = write_u32(store, ret_ptr, 0);
            let base = ret_ptr + 8;
            let _ = write_u64(store, base, stat.dev);
            let _ = write_u64(store, base + 8, stat.ino);
            let _ = write_u32(store, base + 16, 0);
            let _ = write_u32(store, base + 20, stat.nlink as u32);
            let _ = write_u64(store, base + 24, stat.size);
            let _ = write_u64(store, base + 32, stat.atime / 1_000_000_000);
            let _ = write_u64(store, base + 40, stat.mtime / 1_000_000_000);
            let _ = write_u64(store, base + 48, stat.ctime / 1_000_000_000);
        }
        Err(e) => {
            let _ = write_u32(store, ret_ptr, 1);
            let _ = write_u32(store, ret_ptr + 4, e as u32);
        }
    }
    Ok(vec![])
}

pub fn descriptor_set_times_at<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = match get_fd(store, match args.get(0) { Some(Value::I32(v)) => *v as i32, _ => -1 }) {
        Ok(fd) => fd,
        Err(_) => return Ok(vec![]),
    };
    let flags = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let ptr = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let len = match args.get(3) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let atime = match args.get(4) { Some(Value::I64(x)) => *x as u64, _ => 0 };
    let mtime = match args.get(5) { Some(Value::I64(x)) => *x as u64, _ => 0 };
    let fst_flags = match args.get(6) { Some(Value::I32(x)) => *x as u16, _ => 0 };
    let ret_ptr = match args.get(7) { Some(Value::I32(v)) => *v as u32, _ => 0 };

    let mut pb = vec![0u8; len as usize];
    if read_mem(store, ptr, &mut pb).is_err() { return Ok(vec![]); }
    let path = crate::rust_alloc::string::String::from_utf8_lossy(&pb).into_owned();

    match store.wasi_ctx.as_mut().unwrap().env.path_filestat_set_times(fd, flags, &path, atime, mtime, fst_flags) {
        Ok(_) => { let _ = write_u32(store, ret_ptr, 0); }
        Err(e) => {
            let _ = write_u32(store, ret_ptr, 1);
            let _ = write_u32(store, ret_ptr + 4, e as u32);
        }
    }
    Ok(vec![])
}

pub fn descriptor_create_directory_at<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = match get_fd(store, match args.get(0) { Some(Value::I32(v)) => *v as i32, _ => -1 }) {
        Ok(fd) => fd,
        Err(_) => return Ok(vec![]),
    };
    let ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let len = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let ret_ptr = match args.get(3) { Some(Value::I32(v)) => *v as u32, _ => 0 };

    let mut pb = vec![0u8; len as usize];
    if read_mem(store, ptr, &mut pb).is_err() { return Ok(vec![]); }
    let path = crate::rust_alloc::string::String::from_utf8_lossy(&pb).into_owned();

    match store.wasi_ctx.as_mut().unwrap().env.path_create_directory(fd, &path) {
        Ok(_) => { let _ = write_u32(store, ret_ptr, 0); }
        Err(e) => {
            let _ = write_u32(store, ret_ptr, 1);
            let _ = write_u32(store, ret_ptr + 4, e as u32);
        }
    }
    Ok(vec![])
}

pub fn descriptor_unlink_file_at<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = match get_fd(store, match args.get(0) { Some(Value::I32(v)) => *v as i32, _ => -1 }) {
        Ok(fd) => fd,
        Err(_) => return Ok(vec![]),
    };
    let ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let len = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let ret_ptr = match args.get(3) { Some(Value::I32(v)) => *v as u32, _ => 0 };

    let mut pb = vec![0u8; len as usize];
    if read_mem(store, ptr, &mut pb).is_err() { return Ok(vec![]); }
    let path = crate::rust_alloc::string::String::from_utf8_lossy(&pb).into_owned();

    match store.wasi_ctx.as_mut().unwrap().env.path_unlink_file(fd, &path) {
        Ok(_) => { let _ = write_u32(store, ret_ptr, 0); }
        Err(e) => {
            let _ = write_u32(store, ret_ptr, 1);
            let _ = write_u32(store, ret_ptr + 4, e as u32);
        }
    }
    Ok(vec![])
}

pub fn descriptor_remove_directory_at<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = match get_fd(store, match args.get(0) { Some(Value::I32(v)) => *v as i32, _ => -1 }) {
        Ok(fd) => fd,
        Err(_) => return Ok(vec![]),
    };
    let ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let len = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let ret_ptr = match args.get(3) { Some(Value::I32(v)) => *v as u32, _ => 0 };

    let mut pb = vec![0u8; len as usize];
    if read_mem(store, ptr, &mut pb).is_err() { return Ok(vec![]); }
    let path = crate::rust_alloc::string::String::from_utf8_lossy(&pb).into_owned();

    match store.wasi_ctx.as_mut().unwrap().env.path_remove_directory(fd, &path) {
        Ok(_) => { let _ = write_u32(store, ret_ptr, 0); }
        Err(e) => {
            let _ = write_u32(store, ret_ptr, 1);
            let _ = write_u32(store, ret_ptr + 4, e as u32);
        }
    }
    Ok(vec![])
}

pub fn descriptor_link_at<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let old_fd = match get_fd(store, match args.get(0) { Some(Value::I32(v)) => *v as i32, _ => -1 }) {
        Ok(fd) => fd,
        Err(_) => return Ok(vec![]),
    };
    let old_flags = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let old_path_ptr = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let old_path_len = match args.get(3) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let new_fd = match get_fd(store, match args.get(4) { Some(Value::I32(v)) => *v as i32, _ => -1 }) {
        Ok(fd) => fd,
        Err(_) => return Ok(vec![]),
    };
    let new_path_ptr = match args.get(5) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let new_path_len = match args.get(6) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let ret_ptr = match args.get(7) { Some(Value::I32(v)) => *v as u32, _ => 0 };

    let mut ob = vec![0u8; old_path_len as usize];
    if read_mem(store, old_path_ptr, &mut ob).is_err() { return Ok(vec![]); }
    let old_path = crate::rust_alloc::string::String::from_utf8_lossy(&ob).into_owned();

    let mut nb = vec![0u8; new_path_len as usize];
    if read_mem(store, new_path_ptr, &mut nb).is_err() { return Ok(vec![]); }
    let new_path = crate::rust_alloc::string::String::from_utf8_lossy(&nb).into_owned();

    match store.wasi_ctx.as_mut().unwrap().env.path_link(old_fd, old_flags, &old_path, new_fd, &new_path) {
        Ok(_) => { let _ = write_u32(store, ret_ptr, 0); }
        Err(e) => {
            let _ = write_u32(store, ret_ptr, 1);
            let _ = write_u32(store, ret_ptr + 4, e as u32);
        }
    }
    Ok(vec![])
}

pub fn descriptor_rename_at<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let old_fd = match get_fd(store, match args.get(0) { Some(Value::I32(v)) => *v as i32, _ => -1 }) {
        Ok(fd) => fd,
        Err(_) => return Ok(vec![]),
    };
    let old_path_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let old_path_len = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let new_fd = match get_fd(store, match args.get(3) { Some(Value::I32(v)) => *v as i32, _ => -1 }) {
        Ok(fd) => fd,
        Err(_) => return Ok(vec![]),
    };
    let new_path_ptr = match args.get(4) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let new_path_len = match args.get(5) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let ret_ptr = match args.get(6) { Some(Value::I32(v)) => *v as u32, _ => 0 };

    let mut ob = vec![0u8; old_path_len as usize];
    if read_mem(store, old_path_ptr, &mut ob).is_err() { return Ok(vec![]); }
    let old_path = crate::rust_alloc::string::String::from_utf8_lossy(&ob).into_owned();

    let mut nb = vec![0u8; new_path_len as usize];
    if read_mem(store, new_path_ptr, &mut nb).is_err() { return Ok(vec![]); }
    let new_path = crate::rust_alloc::string::String::from_utf8_lossy(&nb).into_owned();

    match store.wasi_ctx.as_mut().unwrap().env.path_rename(old_fd, &old_path, new_fd, &new_path) {
        Ok(_) => { let _ = write_u32(store, ret_ptr, 0); }
        Err(e) => {
            let _ = write_u32(store, ret_ptr, 1);
            let _ = write_u32(store, ret_ptr + 4, e as u32);
        }
    }
    Ok(vec![])
}

pub fn descriptor_symlink_at<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let old_path_ptr = match args.get(0) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let old_path_len = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let fd = match get_fd(store, match args.get(2) { Some(Value::I32(v)) => *v as i32, _ => -1 }) {
        Ok(fd) => fd,
        Err(_) => return Ok(vec![]),
    };
    let new_path_ptr = match args.get(3) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let new_path_len = match args.get(4) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let ret_ptr = match args.get(5) { Some(Value::I32(v)) => *v as u32, _ => 0 };

    let mut ob = vec![0u8; old_path_len as usize];
    if read_mem(store, old_path_ptr, &mut ob).is_err() { return Ok(vec![]); }
    let old_path = crate::rust_alloc::string::String::from_utf8_lossy(&ob).into_owned();

    let mut nb = vec![0u8; new_path_len as usize];
    if read_mem(store, new_path_ptr, &mut nb).is_err() { return Ok(vec![]); }
    let new_path = crate::rust_alloc::string::String::from_utf8_lossy(&nb).into_owned();

    match store.wasi_ctx.as_mut().unwrap().env.path_symlink(&old_path, fd, &new_path) {
        Ok(_) => { let _ = write_u32(store, ret_ptr, 0); }
        Err(e) => {
            let _ = write_u32(store, ret_ptr, 1);
            let _ = write_u32(store, ret_ptr + 4, e as u32);
        }
    }
    Ok(vec![])
}

pub fn descriptor_readlink_at<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = match get_fd(store, match args.get(0) { Some(Value::I32(v)) => *v as i32, _ => -1 }) {
        Ok(fd) => fd,
        Err(_) => return Ok(vec![]),
    };
    let path_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let path_len = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let ret_ptr = match args.get(3) { Some(Value::I32(v)) => *v as u32, _ => 0 };

    let mut pb = vec![0u8; path_len as usize];
    if read_mem(store, path_ptr, &mut pb).is_err() { return Ok(vec![]); }
    let path = crate::rust_alloc::string::String::from_utf8_lossy(&pb).into_owned();

    let mut buf = vec![0u8; 1024];
    match store.wasi_ctx.as_mut().unwrap().env.path_readlink(fd, &path, &mut buf) {
        Ok(n) => {
            let s_ptr = call_cabi_realloc(store, n as u32, 1)?;
            let _ = write_bytes(store, s_ptr, &buf[..n]);
            let _ = write_u32(store, ret_ptr, 0);
            let _ = write_u32(store, ret_ptr + 4, s_ptr);
            let _ = write_u32(store, ret_ptr + 8, n as u32);
        }
        Err(e) => {
            let _ = write_u32(store, ret_ptr, 1);
            let _ = write_u32(store, ret_ptr + 4, e as u32);
        }
    }
    Ok(vec![])
}

pub fn descriptor_sync<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = match get_fd(store, match args.get(0) { Some(Value::I32(v)) => *v as i32, _ => -1 }) {
        Ok(fd) => fd,
        Err(_) => return Ok(vec![]),
    };
    let ret_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    match store.wasi_ctx.as_mut().unwrap().env.fd_sync(fd) {
        Ok(_) => { let _ = write_u32(store, ret_ptr, 0); }
        Err(e) => {
            let _ = write_u32(store, ret_ptr, 1);
            let _ = write_u32(store, ret_ptr + 4, e as u32);
        }
    }
    Ok(vec![])
}

pub fn descriptor_set_size<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = match get_fd(store, match args.get(0) { Some(Value::I32(v)) => *v as i32, _ => -1 }) {
        Ok(fd) => fd,
        Err(_) => return Ok(vec![]),
    };
    let size = match args.get(1) { Some(Value::I64(x)) => *x as u64, _ => 0 };
    let ret_ptr = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    match store.wasi_ctx.as_mut().unwrap().env.fd_filestat_set_size(fd, size) {
        Ok(_) => { let _ = write_u32(store, ret_ptr, 0); }
        Err(e) => {
            let _ = write_u32(store, ret_ptr, 1);
            let _ = write_u32(store, ret_ptr + 4, e as u32);
        }
    }
    Ok(vec![])
}

pub fn descriptor_set_times<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = match get_fd(store, match args.get(0) { Some(Value::I32(v)) => *v as i32, _ => -1 }) {
        Ok(fd) => fd,
        Err(_) => return Ok(vec![]),
    };
    let atime = match args.get(1) { Some(Value::I64(x)) => *x as u64, _ => 0 };
    let mtime = match args.get(2) { Some(Value::I64(x)) => *x as u64, _ => 0 };
    let fst_flags = match args.get(3) { Some(Value::I32(x)) => *x as u16, _ => 0 };
    let ret_ptr = match args.get(4) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    match store.wasi_ctx.as_mut().unwrap().env.fd_filestat_set_times(fd, atime, mtime, fst_flags) {
        Ok(_) => { let _ = write_u32(store, ret_ptr, 0); }
        Err(e) => {
            let _ = write_u32(store, ret_ptr, 1);
            let _ = write_u32(store, ret_ptr + 4, e as u32);
        }
    }
    Ok(vec![])
}

pub fn descriptor_seek<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = match get_fd(store, match args.get(0) { Some(Value::I32(v)) => *v as i32, _ => -1 }) {
        Ok(fd) => fd,
        Err(_) => return Ok(vec![]),
    };
    let offset = match args.get(1) { Some(Value::I64(v)) => *v as u64, _ => 0 };
    let whence = match args.get(2) { Some(Value::I32(v)) => *v as u8, _ => 0 };
    let ret_ptr = match args.get(3) { Some(Value::I32(v)) => *v as u32, _ => 0 };

    match store.wasi_ctx.as_mut().unwrap().env.fd_seek(fd, offset as i64, whence) {
        Ok(new_off) => {
            let _ = write_u32(store, ret_ptr, 0);
            let _ = write_u64(store, ret_ptr + 8, new_off);
        }
        Err(e) => {
            let _ = write_u32(store, ret_ptr, 1);
            let _ = write_u32(store, ret_ptr + 4, e as u32);
        }
    }
    Ok(vec![])
}

pub fn descriptor_advise<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = match get_fd(store, match args.get(0) { Some(Value::I32(v)) => *v as i32, _ => -1 }) {
        Ok(fd) => fd,
        Err(_) => return Ok(vec![]),
    };
    let offset = match args.get(1) { Some(Value::I64(x)) => *x as u64, _ => 0 };
    let len = match args.get(2) { Some(Value::I64(x)) => *x as u64, _ => 0 };
    let advice = match args.get(3) { Some(Value::I32(x)) => *x as u8, _ => 0 };
    let ret_ptr = match args.get(4) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    match store.wasi_ctx.as_mut().unwrap().env.fd_advise(fd, offset, len, advice) {
        Ok(_) => { let _ = write_u32(store, ret_ptr, 0); }
        Err(e) => {
            let _ = write_u32(store, ret_ptr, 1);
            let _ = write_u32(store, ret_ptr + 4, e as u32);
        }
    }
    Ok(vec![])
}

pub fn descriptor_sync_data<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = match get_fd(store, match args.get(0) { Some(Value::I32(v)) => *v as i32, _ => -1 }) {
        Ok(fd) => fd,
        Err(_) => return Ok(vec![]),
    };
    let ret_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    match store.wasi_ctx.as_mut().unwrap().env.fd_datasync(fd) {
        Ok(_) => { let _ = write_u32(store, ret_ptr, 0); }
        Err(e) => {
            let _ = write_u32(store, ret_ptr, 1);
            let _ = write_u32(store, ret_ptr + 4, e as u32);
        }
    }
    Ok(vec![])
}

pub fn descriptor_get_flags<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = match get_fd(store, match args.get(0) { Some(Value::I32(v)) => *v as i32, _ => -1 }) {
        Ok(fd) => fd,
        Err(_) => return Ok(vec![]),
    };
    let ret_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    match store.wasi_ctx.as_ref().unwrap().env.fd_fdstat_get(fd) {
        Ok(stat) => {
            let _ = write_u32(store, ret_ptr, 0);
            let _ = write_u32(store, ret_ptr + 4, stat.flags as u32);
        }
        Err(e) => {
            let _ = write_u32(store, ret_ptr, 1);
            let _ = write_u32(store, ret_ptr + 4, e as u32);
        }
    }
    Ok(vec![])
}

pub fn descriptor_read<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    // read(length, offset) -> result<(list<u8>, bool), error-code>
    let fd = match get_fd(store, match args.get(0) { Some(Value::I32(v)) => *v as i32, _ => -1 }) {
        Ok(fd) => fd,
        Err(_) => return Ok(vec![]),
    };
    let len = match args.get(1) { Some(Value::I64(v)) => *v as u64, _ => 0 };
    let offset = match args.get(2) { Some(Value::I64(v)) => *v as u64, _ => 0 };
    let ret_ptr = match args.get(3) { Some(Value::I32(v)) => *v as u32, _ => 0 };

    let mut buf = vec![0u8; len as usize];
    let mut slices = [buf.as_mut_slice()];
    
    match store.wasi_ctx.as_mut().unwrap().env.fd_pread(fd, &mut slices, offset) {
        Ok(n) => {
            buf.truncate(n);
            let ptr = call_cabi_realloc(store, n as u32, 1)?;
            let _ = write_bytes(store, ptr, &buf);
            let _ = write_u32(store, ret_ptr, 0);
            let _ = write_u32(store, ret_ptr + 4, ptr);
            let _ = write_u32(store, ret_ptr + 8, n as u32);
            let _ = write_u32(store, ret_ptr + 12, if n == 0 { 1 } else { 0 }); // EOF?
        }
        Err(e) => {
            let _ = write_u32(store, ret_ptr, 1);
            let _ = write_u32(store, ret_ptr + 4, e as u32);
        }
    }
    Ok(vec![])
}

pub fn descriptor_write<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    // write(buffer, offset) -> result<u64, error-code>
    let fd = match get_fd(store, match args.get(0) { Some(Value::I32(v)) => *v as i32, _ => -1 }) {
        Ok(fd) => fd,
        Err(_) => return Ok(vec![]),
    };
    let buf_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let buf_len = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let offset = match args.get(3) { Some(Value::I64(v)) => *v as u64, _ => 0 };
    let ret_ptr = match args.get(4) { Some(Value::I32(v)) => *v as u32, _ => 0 };

    let mut buf = vec![0u8; buf_len as usize];
    if read_mem(store, buf_ptr, &mut buf).is_err() {
        let _ = write_u32(store, ret_ptr, 1);
        let _ = write_u32(store, ret_ptr + 4, 21);
        return Ok(vec![]);
    }
    let slices = [buf.as_slice()];

    match store.wasi_ctx.as_mut().unwrap().env.fd_pwrite(fd, &slices, offset) {
        Ok(n) => {
            let _ = write_u32(store, ret_ptr, 0);
            let _ = write_u64(store, ret_ptr + 8, n as u64);
        }
        Err(e) => {
            let _ = write_u32(store, ret_ptr, 1);
            let _ = write_u32(store, ret_ptr + 4, e as u32);
        }
    }
    Ok(vec![])
}

pub fn descriptor_is_same_object<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let h1 = match args.get(0) { Some(Value::I32(v)) => *v as i32, _ => return Ok(vec![]) };
    let h2 = match args.get(1) { Some(Value::I32(v)) => *v as i32, _ => return Ok(vec![]) };
    let fd1 = get_fd(store, h1).unwrap_or(-1);
    let fd2 = get_fd(store, h2).unwrap_or(-1);
    
    // Check dev/ino
    let env = &mut store.wasi_ctx.as_mut().unwrap().env;
    let s1 = env.fd_filestat_get(fd1);
    let s2 = env.fd_filestat_get(fd2);
    
    let same = if let (Ok(st1), Ok(stat2)) = (s1, s2) {
        st1.dev == stat2.dev && st1.ino == stat2.ino
    } else {
        false
    };
    Ok(vec![Value::I32(if same { 1 } else { 0 })])
}

pub fn descriptor_metadata_hash<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = match get_fd(store, match args.get(0) { Some(Value::I32(v)) => *v as i32, _ => -1 }) {
        Ok(fd) => fd,
        Err(_) => return Ok(vec![]),
    };
    let ret_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    match store.wasi_ctx.as_ref().unwrap().env.fd_filestat_get(fd) {
        Ok(stat) => {
            let _ = write_u32(store, ret_ptr, 0);
            let hash = stat.dev.wrapping_add(stat.ino).wrapping_add(stat.mtime);
            let _ = write_u64(store, ret_ptr + 8, hash); // lower
            let _ = write_u64(store, ret_ptr + 16, 0); // upper
        }
        Err(e) => {
            let _ = write_u32(store, ret_ptr, 1);
            let _ = write_u32(store, ret_ptr + 4, e as u32);
        }
    }
    Ok(vec![])
}

pub fn descriptor_metadata_hash_at<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    // Similar to stat_at but returns hash
    descriptor_metadata_hash(store, args) // Stub reuse for now, ignoring path arg
}

pub fn filesystem_error_code<T: Config>(_: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    // err -> option<code>
    // For now we map errno directly if possible, or just return it if it is one.
    // The error passed here is a result of a previous call, likely encoded as u32.
    // But this function expects an error resource handle? Or the error code itself?
    // "filesystem-error-code(err) -> option<error-code>"
    // If err is handle, we need resource.
    Ok(vec![Value::I32(0), Value::I32(0)]) // Stub: None
}
