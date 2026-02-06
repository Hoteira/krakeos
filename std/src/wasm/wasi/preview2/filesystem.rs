use crate::rust_alloc::{vec, vec::Vec};
use crate::wasm::{
    common::{config::Config, value::Value},
    interpreter::store::{HaltExecutionError, Store},
    wasi::ctx::{InputStreamSource, OutputStreamSource, WasiResource},
};
use super::{call_cabi_realloc, read_bytes, read_mem, write_bytes, write_u32, write_u64};

fn get_fd<T: Config>(store: &mut Store<'_, T>, handle: i32) -> Result<i32, HaltExecutionError> {
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
    let handle = match args.get(0) {
        Some(Value::I32(v)) => *v as i32,
        _ => return Ok(vec![Value::I32(0)]),
    };
    let wasi = store.wasi_ctx.as_mut().ok_or(HaltExecutionError(1))?;
    
    // Support Descriptor(fd)
    let fd = match wasi.resource_table.get(&handle) {
        Some(WasiResource::File(f)) => f.as_raw_fd(),
        Some(WasiResource::Descriptor(fd)) => *fd as usize, // Use raw FD
        _ => return Ok(vec![Value::I32(0)]),
    };
    let id = wasi.next_resource_id;
    wasi.next_resource_id += 1;
    wasi.resource_table.insert(id, WasiResource::InputStream(InputStreamSource::GuestFd(fd as i32)));
    Ok(vec![Value::I32(0), Value::I32(id as u32)])
}

pub fn filesystem_types_write_via_stream<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let handle = match args.get(0) {
        Some(Value::I32(v)) => *v as i32,
        _ => return Ok(vec![Value::I32(0)]),
    };
    let wasi = store.wasi_ctx.as_mut().ok_or(HaltExecutionError(1))?;
    let fd = match wasi.resource_table.get(&handle) {
        Some(WasiResource::File(f)) => f.as_raw_fd(),
        Some(WasiResource::Descriptor(fd)) => *fd as usize,
        _ => return Ok(vec![Value::I32(0)]),
    };
    let id = wasi.next_resource_id;
    wasi.next_resource_id += 1;
    wasi.resource_table.insert(id, WasiResource::OutputStream(OutputStreamSource::GuestFd(fd as i32)));
    Ok(vec![Value::I32(0), Value::I32(id as u32)])
}

pub fn filesystem_types_append_via_stream<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    filesystem_types_write_via_stream(store, args)
}

pub fn descriptor_type<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let handle = args.get(0).and_then(|v| if let Value::I32(x) = v { Some(*x as i32) } else { None }).unwrap_or(-1);
    let fd = get_fd(store, handle)?;
    let stat = store.wasi_ctx.as_ref().unwrap().env.fd_fdstat_get(fd).map_err(|e| HaltExecutionError(e as i32))?;
    Ok(vec![Value::I32(0), Value::I32(stat.filetype as u32)])
}

pub fn descriptor_stat<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let handle = args.get(0).and_then(|v| if let Value::I32(x) = v { Some(*x as i32) } else { None }).unwrap_or(-1);
    let ret_ptr = args.get(1).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let fd = get_fd(store, handle)?;
    let stat = store.wasi_ctx.as_ref().unwrap().env.fd_filestat_get(fd).map_err(|e| HaltExecutionError(e as i32))?;
    
    // Write DescriptorStat (type, link_count, size, data_access_timestamp, data_modification_timestamp, status_change_timestamp)
    write_bytes(store, ret_ptr, &[stat.filetype]).map_err(|_| HaltExecutionError(1))?;
    write_u64(store, ret_ptr + 8, stat.nlink).map_err(|_| HaltExecutionError(1))?;
    write_u64(store, ret_ptr + 16, stat.size).map_err(|_| HaltExecutionError(1))?;
    write_u64(store, ret_ptr + 24, stat.atime).map_err(|_| HaltExecutionError(1))?;
    write_u64(store, ret_ptr + 32, stat.mtime).map_err(|_| HaltExecutionError(1))?;
    write_u64(store, ret_ptr + 40, stat.ctime).map_err(|_| HaltExecutionError(1))?;
    
    Ok(vec![Value::I32(0)])
}

pub fn descriptor_open_at<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let dir_handle = args.get(0).and_then(|v| if let Value::I32(x) = v { Some(*x as i32) } else { None }).unwrap_or(-1);
    let flags = args.get(1).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let path_ptr = args.get(2).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let path_len = args.get(3).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let oflags = args.get(4).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let flags_val = args.get(5).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);

    let dirfd = get_fd(store, dir_handle)?;
    let mut path_buf = vec![0u8; path_len as usize];
    read_mem(store, path_ptr, &mut path_buf).map_err(|_| HaltExecutionError(1))?;
    let path = crate::rust_alloc::string::String::from_utf8_lossy(&path_buf).into_owned();

    let rights = 0x3F; // Default rights
    let fd = store.wasi_ctx.as_mut().unwrap().env.path_open(dirfd, flags, &path, oflags, rights, rights, flags_val as u16)
        .map_err(|e| HaltExecutionError(e as i32))?;

    let wasi = store.wasi_ctx.as_mut().unwrap();
    let id = wasi.next_resource_id;
    wasi.next_resource_id += 1;
    wasi.resource_table.insert(id, WasiResource::Descriptor(fd));
    
    Ok(vec![Value::I32(0), Value::I32(id as u32)])
}

pub fn descriptor_read_directory<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let handle = args.get(0).and_then(|v| if let Value::I32(x) = v { Some(*x as i32) } else { None }).unwrap_or(-1);
    let fd = get_fd(store, handle)?;
    
    let entries = store.wasi_ctx.as_mut().unwrap().env.fd_readdir(fd, 0).map_err(|e| HaltExecutionError(e as i32))?;
    
    let wasi = store.wasi_ctx.as_mut().unwrap();
    let id = wasi.next_resource_id;
    wasi.next_resource_id += 1;
    wasi.resource_table.insert(id, WasiResource::DirStream { entries, index: 0 });
    
    Ok(vec![Value::I32(0), Value::I32(id as u32)])
}

pub fn directory_entry_stream_read_directory_entry<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let handle = args.get(0).and_then(|v| if let Value::I32(x) = v { Some(*x as i32) } else { None }).unwrap_or(-1);
    let ret_ptr = args.get(1).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);

    let entry = {
        let wasi = store.wasi_ctx.as_mut().ok_or(HaltExecutionError(1))?;
        match wasi.resource_table.get_mut(&handle) {
            Some(WasiResource::DirStream { entries, index }) => {
                if *index < entries.len() {
                    let e = entries[*index].clone();
                    *index += 1;
                    Some(e)
                } else {
                    None
                }
            }
            _ => return Err(HaltExecutionError(1)),
        }
    };

    if let Some((name, ty, inode)) = entry {
        // Result::Ok(Some(DirectoryEntry))
        // DirectoryEntry: type, name_ptr, name_len, inode
        let name_bytes = name.as_bytes();
        let name_ptr = call_cabi_realloc(store, name_bytes.len() as u32, 1)?;
        write_bytes(store, name_ptr, name_bytes).map_err(|_| HaltExecutionError(1))?;

        // Write Option::Some tag (1)
        write_u32(store, ret_ptr, 1).map_err(|_| HaltExecutionError(1))?;
        // Write payload
        let payload_ptr = ret_ptr + 4;
        write_bytes(store, payload_ptr, &[ty]).map_err(|_| HaltExecutionError(1))?; // Type
        write_u32(store, payload_ptr + 4, name_ptr).map_err(|_| HaltExecutionError(1))?;
        write_u32(store, payload_ptr + 8, name_bytes.len() as u32).map_err(|_| HaltExecutionError(1))?;
        write_u64(store, payload_ptr + 16, inode).map_err(|_| HaltExecutionError(1))?;
    } else {
        // Result::Ok(None) -> Tag 0
        write_u32(store, ret_ptr, 0).map_err(|_| HaltExecutionError(1))?;
    }
    
    Ok(vec![Value::I32(0)])
}

pub fn descriptor_stat_at<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let handle = args.get(0).and_then(|v| if let Value::I32(x) = v { Some(*x as i32) } else { None }).unwrap_or(-1);
    let flags = args.get(1).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let ptr = args.get(2).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let len = args.get(3).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let ret_ptr = args.get(4).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);

    let fd = get_fd(store, handle)?;
    let mut pb = vec![0u8; len as usize];
    read_mem(store, ptr, &mut pb).map_err(|_| HaltExecutionError(1))?;
    let path = crate::rust_alloc::string::String::from_utf8_lossy(&pb).into_owned();

    let stat = store.wasi_ctx.as_mut().unwrap().env.path_filestat_get(fd, flags, &path).map_err(|e| HaltExecutionError(e as i32))?;

    write_bytes(store, ret_ptr, &[stat.filetype]).map_err(|_| HaltExecutionError(1))?;
    write_u64(store, ret_ptr + 8, stat.nlink).map_err(|_| HaltExecutionError(1))?;
    write_u64(store, ret_ptr + 16, stat.size).map_err(|_| HaltExecutionError(1))?;
    write_u64(store, ret_ptr + 24, stat.atime).map_err(|_| HaltExecutionError(1))?;
    write_u64(store, ret_ptr + 32, stat.mtime).map_err(|_| HaltExecutionError(1))?;
    write_u64(store, ret_ptr + 40, stat.ctime).map_err(|_| HaltExecutionError(1))?;

    Ok(vec![Value::I32(0)])
}

pub fn descriptor_set_times_at<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    // fd, flags, path_ptr, path_len, atime, mtime, fst_flags
    let fd = get_fd(store, args.get(0).and_then(|v| if let Value::I32(x) = v { Some(*x as i32) } else { None }).unwrap_or(-1))?;
    let flags = args.get(1).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let ptr = args.get(2).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let len = args.get(3).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let atime = args.get(4).and_then(|v| if let Value::I64(x) = v { Some(*x as u64) } else { None }).unwrap_or(0);
    let mtime = args.get(5).and_then(|v| if let Value::I64(x) = v { Some(*x as u64) } else { None }).unwrap_or(0);
    let fst_flags = args.get(6).and_then(|v| if let Value::I32(x) = v { Some(*x as u16) } else { None }).unwrap_or(0);

    let mut pb = vec![0u8; len as usize];
    read_mem(store, ptr, &mut pb).map_err(|_| HaltExecutionError(1))?;
    let path = crate::rust_alloc::string::String::from_utf8_lossy(&pb).into_owned();

    store.wasi_ctx.as_mut().unwrap().env.path_filestat_set_times(fd, flags, &path, atime, mtime, fst_flags)
        .map_err(|e| HaltExecutionError(e as i32))?;
    Ok(vec![Value::I32(0)])
}

pub fn descriptor_create_directory_at<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = get_fd(store, args.get(0).and_then(|v| if let Value::I32(x) = v { Some(*x as i32) } else { None }).unwrap_or(-1))?;
    let ptr = args.get(1).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let len = args.get(2).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);

    let mut pb = vec![0u8; len as usize];
    read_mem(store, ptr, &mut pb).map_err(|_| HaltExecutionError(1))?;
    let path = crate::rust_alloc::string::String::from_utf8_lossy(&pb).into_owned();

    store.wasi_ctx.as_mut().unwrap().env.path_create_directory(fd, &path)
        .map_err(|e| HaltExecutionError(e as i32))?;
    Ok(vec![Value::I32(0)])
}

pub fn descriptor_unlink_file_at<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = get_fd(store, args.get(0).and_then(|v| if let Value::I32(x) = v { Some(*x as i32) } else { None }).unwrap_or(-1))?;
    let ptr = args.get(1).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let len = args.get(2).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);

    let mut pb = vec![0u8; len as usize];
    read_mem(store, ptr, &mut pb).map_err(|_| HaltExecutionError(1))?;
    let path = crate::rust_alloc::string::String::from_utf8_lossy(&pb).into_owned();

    store.wasi_ctx.as_mut().unwrap().env.path_unlink_file(fd, &path)
        .map_err(|e| HaltExecutionError(e as i32))?;
    Ok(vec![Value::I32(0)])
}

pub fn descriptor_remove_directory_at<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = get_fd(store, args.get(0).and_then(|v| if let Value::I32(x) = v { Some(*x as i32) } else { None }).unwrap_or(-1))?;
    let ptr = args.get(1).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let len = args.get(2).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);

    let mut pb = vec![0u8; len as usize];
    read_mem(store, ptr, &mut pb).map_err(|_| HaltExecutionError(1))?;
    let path = crate::rust_alloc::string::String::from_utf8_lossy(&pb).into_owned();

    store.wasi_ctx.as_mut().unwrap().env.path_remove_directory(fd, &path)
        .map_err(|e| HaltExecutionError(e as i32))?;
    Ok(vec![Value::I32(0)])
}

pub fn descriptor_link_at<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let old_fd = get_fd(store, args.get(0).and_then(|v| if let Value::I32(x) = v { Some(*x as i32) } else { None }).unwrap_or(-1))?;
    let old_flags = args.get(1).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let old_path_ptr = args.get(2).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let old_path_len = args.get(3).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let new_fd = get_fd(store, args.get(4).and_then(|v| if let Value::I32(x) = v { Some(*x as i32) } else { None }).unwrap_or(-1))?;
    let new_path_ptr = args.get(5).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let new_path_len = args.get(6).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);

    let mut ob = vec![0u8; old_path_len as usize];
    read_mem(store, old_path_ptr, &mut ob).map_err(|_| HaltExecutionError(1))?;
    let old_path = crate::rust_alloc::string::String::from_utf8_lossy(&ob).into_owned();

    let mut nb = vec![0u8; new_path_len as usize];
    read_mem(store, new_path_ptr, &mut nb).map_err(|_| HaltExecutionError(1))?;
    let new_path = crate::rust_alloc::string::String::from_utf8_lossy(&nb).into_owned();

    store.wasi_ctx.as_mut().unwrap().env.path_link(old_fd, old_flags, &old_path, new_fd, &new_path)
        .map_err(|e| HaltExecutionError(e as i32))?;
    Ok(vec![Value::I32(0)])
}

pub fn descriptor_rename_at<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let old_fd = get_fd(store, args.get(0).and_then(|v| if let Value::I32(x) = v { Some(*x as i32) } else { None }).unwrap_or(-1))?;
    let old_path_ptr = args.get(1).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let old_path_len = args.get(2).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let new_fd = get_fd(store, args.get(3).and_then(|v| if let Value::I32(x) = v { Some(*x as i32) } else { None }).unwrap_or(-1))?;
    let new_path_ptr = args.get(4).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let new_path_len = args.get(5).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);

    let mut ob = vec![0u8; old_path_len as usize];
    read_mem(store, old_path_ptr, &mut ob).map_err(|_| HaltExecutionError(1))?;
    let old_path = crate::rust_alloc::string::String::from_utf8_lossy(&ob).into_owned();

    let mut nb = vec![0u8; new_path_len as usize];
    read_mem(store, new_path_ptr, &mut nb).map_err(|_| HaltExecutionError(1))?;
    let new_path = crate::rust_alloc::string::String::from_utf8_lossy(&nb).into_owned();

    store.wasi_ctx.as_mut().unwrap().env.path_rename(old_fd, &old_path, new_fd, &new_path)
        .map_err(|e| HaltExecutionError(e as i32))?;
    Ok(vec![Value::I32(0)])
}

pub fn descriptor_symlink_at<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let old_path_ptr = args.get(0).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let old_path_len = args.get(1).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let fd = get_fd(store, args.get(2).and_then(|v| if let Value::I32(x) = v { Some(*x as i32) } else { None }).unwrap_or(-1))?;
    let new_path_ptr = args.get(3).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let new_path_len = args.get(4).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);

    let mut ob = vec![0u8; old_path_len as usize];
    read_mem(store, old_path_ptr, &mut ob).map_err(|_| HaltExecutionError(1))?;
    let old_path = crate::rust_alloc::string::String::from_utf8_lossy(&ob).into_owned();

    let mut nb = vec![0u8; new_path_len as usize];
    read_mem(store, new_path_ptr, &mut nb).map_err(|_| HaltExecutionError(1))?;
    let new_path = crate::rust_alloc::string::String::from_utf8_lossy(&nb).into_owned();

    store.wasi_ctx.as_mut().unwrap().env.path_symlink(&old_path, fd, &new_path)
        .map_err(|e| HaltExecutionError(e as i32))?;
    Ok(vec![Value::I32(0)])
}

pub fn descriptor_readlink_at<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = get_fd(store, args.get(0).and_then(|v| if let Value::I32(x) = v { Some(*x as i32) } else { None }).unwrap_or(-1))?;
    let path_ptr = args.get(1).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let path_len = args.get(2).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let ret_ptr = args.get(3).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);

    let mut pb = vec![0u8; path_len as usize];
    read_mem(store, path_ptr, &mut pb).map_err(|_| HaltExecutionError(1))?;
    let path = crate::rust_alloc::string::String::from_utf8_lossy(&pb).into_owned();

    // Use a reasonable buffer size for readlink
    let mut buf = vec![0u8; 1024];
    let n = store.wasi_ctx.as_mut().unwrap().env.path_readlink(fd, &path, &mut buf).map_err(|e| HaltExecutionError(e as i32))?;
    let bytes = &buf[..n];

    let s_ptr = call_cabi_realloc(store, n as u32, 1)?;
    write_bytes(store, s_ptr, bytes).map_err(|_| HaltExecutionError(1))?;

    write_u32(store, ret_ptr, s_ptr).map_err(|_| HaltExecutionError(1))?;
    write_u32(store, ret_ptr + 4, n as u32).map_err(|_| HaltExecutionError(1))?;

    Ok(vec![Value::I32(0)])
}

pub fn descriptor_sync<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = get_fd(store, args.get(0).and_then(|v| if let Value::I32(x) = v { Some(*x as i32) } else { None }).unwrap_or(-1))?;
    store.wasi_ctx.as_mut().unwrap().env.fd_sync(fd).map_err(|e| HaltExecutionError(e as i32))?;
    Ok(vec![Value::I32(0)])
}

pub fn descriptor_set_size<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = get_fd(store, args.get(0).and_then(|v| if let Value::I32(x) = v { Some(*x as i32) } else { None }).unwrap_or(-1))?;
    let size = args.get(1).and_then(|v| if let Value::I64(x) = v { Some(*x as u64) } else { None }).unwrap_or(0);
    store.wasi_ctx.as_mut().unwrap().env.fd_filestat_set_size(fd, size).map_err(|e| HaltExecutionError(e as i32))?;
    Ok(vec![Value::I32(0)])
}

pub fn descriptor_set_times<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = get_fd(store, args.get(0).and_then(|v| if let Value::I32(x) = v { Some(*x as i32) } else { None }).unwrap_or(-1))?;
    let atime = args.get(1).and_then(|v| if let Value::I64(x) = v { Some(*x as u64) } else { None }).unwrap_or(0);
    let mtime = args.get(2).and_then(|v| if let Value::I64(x) = v { Some(*x as u64) } else { None }).unwrap_or(0);
    let fst_flags = args.get(3).and_then(|v| if let Value::I32(x) = v { Some(*x as u16) } else { None }).unwrap_or(0);
    store.wasi_ctx.as_mut().unwrap().env.fd_filestat_set_times(fd, atime, mtime, fst_flags).map_err(|e| HaltExecutionError(e as i32))?;
    Ok(vec![Value::I32(0)])
}

pub fn descriptor_advise<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = get_fd(store, args.get(0).and_then(|v| if let Value::I32(x) = v { Some(*x as i32) } else { None }).unwrap_or(-1))?;
    let offset = args.get(1).and_then(|v| if let Value::I64(x) = v { Some(*x as u64) } else { None }).unwrap_or(0);
    let len = args.get(2).and_then(|v| if let Value::I64(x) = v { Some(*x as u64) } else { None }).unwrap_or(0);
    let advice = args.get(3).and_then(|v| if let Value::I32(x) = v { Some(*x as u8) } else { None }).unwrap_or(0);
    store.wasi_ctx.as_mut().unwrap().env.fd_advise(fd, offset, len, advice).map_err(|e| HaltExecutionError(e as i32))?;
    Ok(vec![Value::I32(0)])
}
