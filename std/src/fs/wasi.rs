use crate::alloc::{vec, vec::Vec};
use crate::wasm::{
    common::{config::Config, value::Value, reader::types::{ValType, NumType}},
    interpreter::store::{HaltExecutionError, Store},
    wasi::ctx::{InputStreamSource, OutputStreamSource, WasiResource},
};
use crate::wasm::wasi::preview2::{call_cabi_realloc, read_mem, write_bytes, write_u32, write_u64};

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

crate::export_method!(
    "wasi:filesystem/preopens@0.2.0", "get-directories",
    [],
    vec![ValType::NumType(NumType::I32)], vec![],
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
);

crate::export_method!(
    "wasi:filesystem/types@0.2.0", "[method]descriptor.read-via-stream",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
    pub fn read_via_stream<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
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
);

crate::export_method!(
    "wasi:filesystem/types@0.2.0", "[method]descriptor.write-via-stream",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
    pub fn write_via_stream<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
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
);

crate::export_method!(
    "wasi:filesystem/types@0.2.0", "[method]descriptor.append-via-stream",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
    pub fn append_via_stream<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        write_via_stream(store, args)
    }
);

crate::export_method!(
    "wasi:filesystem/types@0.2.0", "[method]descriptor.type",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
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
);

crate::export_method!(
    "wasi:filesystem/types@0.2.0", "[method]descriptor.stat",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
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
                let _ = write_u32(store, ret_ptr, 0);
                let base = ret_ptr + 8;
                let _ = write_u64(store, base, stat.dev);
                let _ = write_u64(store, base + 8, stat.ino);
                let _ = write_bytes(store, base + 16, &[stat.filetype]);
                let _ = write_u64(store, base + 24, stat.nlink);
                let _ = write_u64(store, base + 32, stat.size);
                let _ = write_u64(store, base + 40, stat.atime / 1_000_000_000);
                let _ = write_u64(store, base + 48, stat.mtime / 1_000_000_000);
                let _ = write_u64(store, base + 56, stat.ctime / 1_000_000_000);
            }
            Err(e) => {
                let _ = write_u32(store, ret_ptr, 1);
                let _ = write_u32(store, ret_ptr + 4, e as u32);
            }
        }
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi:filesystem/types@0.2.0", "[method]descriptor.open-at",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
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
            let _ = write_u32(store, ret_ptr + 4, 21);
            return Ok(vec![]);
        }
        let path = crate::alloc::string::String::from_utf8_lossy(&path_buf).into_owned();

        let rights = 0x3F;
        match store.wasi_ctx.as_mut().unwrap().env.path_open(dirfd, dirflags, &path, oflags, rights, rights, flags_val as u16) {
            Ok(fd) => {
                let wasi = store.wasi_ctx.as_mut().unwrap();
                let id = wasi.next_resource_id;
                wasi.next_resource_id += 1;
                wasi.resource_table.insert(id, WasiResource::Descriptor(fd));
                
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
);

crate::export_method!(
    "wasi:filesystem/types@0.2.0", "[method]descriptor.read-directory",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
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
);

crate::export_method!(
    "wasi:filesystem/types@0.2.0", "[method]directory-entry-stream.read-directory-entry",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
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
                    let _ = write_u32(store, payload_ptr + 12, name_bytes.len() as u32); 
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
);

crate::export_method!(
    "wasi:filesystem/types@0.2.0", "[method]descriptor.stat-at",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
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
        let path = crate::alloc::string::String::from_utf8_lossy(&pb).into_owned();

        match store.wasi_ctx.as_mut().unwrap().env.path_filestat_get(fd, flags, &path) {
            Ok(stat) => {
                let _ = write_u32(store, ret_ptr, 0);
                let base = ret_ptr + 8;
                let _ = write_u64(store, base, stat.dev);
                let _ = write_u64(store, base + 8, stat.ino);
                let _ = write_bytes(store, base + 16, &[stat.filetype]);
                let _ = write_u64(store, base + 24, stat.nlink);
                let _ = write_u64(store, base + 32, stat.size);
                let _ = write_u64(store, base + 40, stat.atime / 1_000_000_000);
                let _ = write_u64(store, base + 48, stat.mtime / 1_000_000_000);
                let _ = write_u64(store, base + 56, stat.ctime / 1_000_000_000);
            }
            Err(e) => {
                let _ = write_u32(store, ret_ptr, 1);
                let _ = write_u32(store, ret_ptr + 4, e as u32);
            }
        }
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi:filesystem/types@0.2.0", "[method]descriptor.set-times-at",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
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
        let path = crate::alloc::string::String::from_utf8_lossy(&pb).into_owned();

        match store.wasi_ctx.as_mut().unwrap().env.path_filestat_set_times(fd, flags, &path, atime, mtime, fst_flags) {
            Ok(_) => { let _ = write_u32(store, ret_ptr, 0); }
            Err(e) => {
                let _ = write_u32(store, ret_ptr, 1);
                let _ = write_u32(store, ret_ptr + 4, e as u32);
            }
        }
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi:filesystem/types@0.2.0", "[method]descriptor.create-directory-at",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
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
        let path = crate::alloc::string::String::from_utf8_lossy(&pb).into_owned();

        match store.wasi_ctx.as_mut().unwrap().env.path_create_directory(fd, &path) {
            Ok(_) => { let _ = write_u32(store, ret_ptr, 0); }
            Err(e) => {
                let _ = write_u32(store, ret_ptr, 1);
                let _ = write_u32(store, ret_ptr + 4, e as u32);
            }
        }
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi:filesystem/types@0.2.0", "[method]descriptor.unlink-file-at",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
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
        let path = crate::alloc::string::String::from_utf8_lossy(&pb).into_owned();

        match store.wasi_ctx.as_mut().unwrap().env.path_unlink_file(fd, &path) {
            Ok(_) => { let _ = write_u32(store, ret_ptr, 0); }
            Err(e) => {
                let _ = write_u32(store, ret_ptr, 1);
                let _ = write_u32(store, ret_ptr + 4, e as u32);
            }
        }
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi:filesystem/types@0.2.0", "[method]descriptor.remove-directory-at",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
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
        let path = crate::alloc::string::String::from_utf8_lossy(&pb).into_owned();

        match store.wasi_ctx.as_mut().unwrap().env.path_remove_directory(fd, &path) {
            Ok(_) => { let _ = write_u32(store, ret_ptr, 0); }
            Err(e) => {
                let _ = write_u32(store, ret_ptr, 1);
                let _ = write_u32(store, ret_ptr + 4, e as u32);
            }
        }
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi:filesystem/types@0.2.0", "[method]descriptor.link-at",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
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
        let old_path = crate::alloc::string::String::from_utf8_lossy(&ob).into_owned();

        let mut nb = vec![0u8; new_path_len as usize];
        if read_mem(store, new_path_ptr, &mut nb).is_err() { return Ok(vec![]); }
        let new_path = crate::alloc::string::String::from_utf8_lossy(&nb).into_owned();

        match store.wasi_ctx.as_mut().unwrap().env.path_link(old_fd, old_flags, &old_path, new_fd, &new_path) {
            Ok(_) => { let _ = write_u32(store, ret_ptr, 0); }
            Err(e) => {
                let _ = write_u32(store, ret_ptr, 1);
                let _ = write_u32(store, ret_ptr + 4, e as u32);
            }
        }
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi:filesystem/types@0.2.0", "[method]descriptor.rename-at",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
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
        let old_path = crate::alloc::string::String::from_utf8_lossy(&ob).into_owned();

        let mut nb = vec![0u8; new_path_len as usize];
        if read_mem(store, new_path_ptr, &mut nb).is_err() { return Ok(vec![]); }
        let new_path = crate::alloc::string::String::from_utf8_lossy(&nb).into_owned();

        match store.wasi_ctx.as_mut().unwrap().env.path_rename(old_fd, &old_path, new_fd, &new_path) {
            Ok(_) => { let _ = write_u32(store, ret_ptr, 0); }
            Err(e) => {
                let _ = write_u32(store, ret_ptr, 1);
                let _ = write_u32(store, ret_ptr + 4, e as u32);
            }
        }
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi:filesystem/types@0.2.0", "[method]descriptor.symlink-at",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
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
        let old_path = crate::alloc::string::String::from_utf8_lossy(&ob).into_owned();

        let mut nb = vec![0u8; new_path_len as usize];
        if read_mem(store, new_path_ptr, &mut nb).is_err() { return Ok(vec![]); }
        let new_path = crate::alloc::string::String::from_utf8_lossy(&nb).into_owned();

        match store.wasi_ctx.as_mut().unwrap().env.path_symlink(&old_path, fd, &new_path) {
            Ok(_) => { let _ = write_u32(store, ret_ptr, 0); }
            Err(e) => {
                let _ = write_u32(store, ret_ptr, 1);
                let _ = write_u32(store, ret_ptr + 4, e as u32);
            }
        }
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi:filesystem/types@0.2.0", "[method]descriptor.readlink-at",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
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
        let path = crate::alloc::string::String::from_utf8_lossy(&pb).into_owned();

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
);

crate::export_method!(
    "wasi:filesystem/types@0.2.0", "[method]descriptor.sync",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
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
);

crate::export_method!(
    "wasi:filesystem/types@0.2.0", "[method]descriptor.set-size",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![],
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
);

crate::export_method!(
    "wasi:filesystem/types@0.2.0", "[method]descriptor.set-times",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
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
);

crate::export_method!(
    "wasi:filesystem/types@0.2.0", "[method]descriptor.seek",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
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
);

crate::export_method!(
    "wasi:filesystem/types@0.2.0", "[method]descriptor.advise",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
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
);

crate::export_method!(
    "wasi:filesystem/types@0.2.0", "[method]descriptor.get-flags",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
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
);

crate::export_method!(
    "wasi:filesystem/types@0.2.0", "[method]descriptor.sync-data",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
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
);

crate::export_method!(
    "wasi:filesystem/types@0.2.0", "[method]descriptor.is-same-object",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
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
);

crate::export_method!(
    "wasi:filesystem/types@0.2.0", "[method]descriptor.metadata-hash",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
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
);

crate::export_method!(
    "wasi:filesystem/types@0.2.0", "[method]descriptor.metadata-hash-at",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
    pub fn descriptor_metadata_hash_at<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        // Similar to stat_at but returns hash
        descriptor_metadata_hash(store, args) // Stub reuse for now, ignoring path arg
    }
);

crate::export_method!(
    "wasi:filesystem/types@0.2.0", "[method]descriptor.read",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![],
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
);

crate::export_method!(
    "wasi:filesystem/types@0.2.0", "[method]descriptor.write",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![],
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
);

crate::export_method!(
    "wasi:filesystem/types@0.2.0", "filesystem-error-code",
    [],
    vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)],
    pub fn filesystem_error_code<T: Config>(_: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        Ok(vec![Value::I32(0), Value::I32(0)]) // Stub: None
    }
);

crate::export_method!(
    "wasi_snapshot_preview1", "fd_close",
    [],
    vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn fd_close_p1<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let fd = match args.get(0) { Some(Value::I32(x)) => *x as i32, _ => -1 };
        match wasi_ctx(store).env.fd_close(fd) {
            Ok(_) => Ok(vec![Value::I32(0)]),
            Err(e) => Ok(vec![Value::I32(e as u32)]),
        }
    }
);

crate::export_method!(
    "wasi_snapshot_preview1", "fd_fdstat_get",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn fd_fdstat_get_p1<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let fd = match args.get(0) { Some(Value::I32(x)) => *x as i32, _ => -1 };
        let s_ptr = match args.get(1) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        match wasi_ctx(store).env.fd_fdstat_get(fd) {
            Ok(s) => {
                let _ = write_bytes(store, s_ptr, &[s.filetype]);
                let _ = write_bytes(store, s_ptr + 2, &s.flags.to_le_bytes());
                let _ = write_u64(store, s_ptr + 8, s.rights_base);
                let _ = write_u64(store, s_ptr + 16, s.rights_inheriting);
                Ok(vec![Value::I32(0)])
            }
            Err(e) => Ok(vec![Value::I32(e as u32)]),
        }
    }
);

crate::export_method!(
    "wasi_snapshot_preview1", "fd_filestat_get",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn fd_filestat_get_p1<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let fd = match args.get(0) { Some(Value::I32(x)) => *x as i32, _ => -1 };
        let s_ptr = match args.get(1) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        match wasi_ctx(store).env.fd_filestat_get(fd) {
            Ok(s) => {
                let _ = write_u64(store, s_ptr, s.dev);
                let _ = write_u64(store, s_ptr + 8, s.ino);
                let _ = write_bytes(store, s_ptr + 16, &[s.filetype]);
                let _ = write_u64(store, s_ptr + 24, s.nlink);
                let _ = write_u64(store, s_ptr + 32, s.size);
                let _ = write_u64(store, s_ptr + 40, s.atime);
                let _ = write_u64(store, s_ptr + 48, s.mtime);
                let _ = write_u64(store, s_ptr + 56, s.ctime);
                Ok(vec![Value::I32(0)])
            }
            Err(e) => Ok(vec![Value::I32(e as u32)]),
        }
    }
);

crate::export_method!(
    "wasi_snapshot_preview1", "fd_read",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn fd_read_p1<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let fd = match args.get(0) { Some(Value::I32(x)) => *x as i32, _ => -1 };
        let i_ptr = match args.get(1) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let i_len = match args.get(2) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let n_ptr = match args.get(3) { Some(Value::I32(x)) => *x as u32, _ => 0 };

        let mut iovs = Vec::new();
        for i in 0..i_len {
            let mut iov = [0u8; 8];
            if crate::wasm::wasi::preview2::read_bytes(store, i_ptr + i * 8, &mut iov).is_err() { return Ok(vec![Value::I32(21)]); }
            let b_ptr = u32::from_le_bytes(iov[0..4].try_into().unwrap());
            let b_len = u32::from_le_bytes(iov[4..8].try_into().unwrap());
            iovs.push((b_ptr, b_len));
        }

        let mut buffers = Vec::new();
        for (_, len) in &iovs { buffers.push(vec![0u8; *len as usize]); }
        let mut slices: Vec<&mut [u8]> = buffers.iter_mut().map(|v| v.as_mut_slice()).collect();

        match wasi_ctx(store).env.fd_read(fd, &mut slices) {
            Ok(n) => {
                let mut remaining = n;
                for ((ptr, _), buf) in iovs.iter().zip(buffers.iter()) {
                    let to_write = core::cmp::min(remaining, buf.len());
                    if to_write > 0 {
                        let _ = write_bytes(store, *ptr, &buf[..to_write]);
                        remaining -= to_write;
                    }
                }
                let _ = write_u32(store, n_ptr, n as u32);
                Ok(vec![Value::I32(0)])
            }
            Err(e) => Ok(vec![Value::I32(e as u32)]),
        }
    }
);

crate::export_method!(
    "wasi_snapshot_preview1", "fd_write",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn fd_write_p1<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let fd = match args.get(0) { Some(Value::I32(x)) => *x as i32, _ => -1 };
        let i_ptr = match args.get(1) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let i_len = match args.get(2) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let n_ptr = match args.get(3) { Some(Value::I32(x)) => *x as u32, _ => 0 };

        let mut buffers = Vec::new();
        for i in 0..i_len {
            let mut iov = [0u8; 8];
            if crate::wasm::wasi::preview2::read_bytes(store, i_ptr + i * 8, &mut iov).is_err() { return Ok(vec![Value::I32(21)]); }
            let b_ptr = u32::from_le_bytes(iov[0..4].try_into().unwrap());
            let b_len = u32::from_le_bytes(iov[4..8].try_into().unwrap());
            let mut b = vec![0u8; b_len as usize];
            if crate::wasm::wasi::preview2::read_bytes(store, b_ptr, &mut b).is_err() { return Ok(vec![Value::I32(21)]); }
            buffers.push(b);
        }

        let slices: Vec<&[u8]> = buffers.iter().map(|v| v.as_slice()).collect();
        match wasi_ctx(store).env.fd_write(fd, &slices) {
            Ok(n) => {
                let _ = write_u32(store, n_ptr, n as u32);
                Ok(vec![Value::I32(0)])
            }
            Err(e) => Ok(vec![Value::I32(e as u32)]),
        }
    }
);

crate::export_method!(
    "wasi_snapshot_preview1", "fd_seek",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn fd_seek_p1<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let fd = match args.get(0) { Some(Value::I32(x)) => *x as i32, _ => -1 };
        let off = match args.get(1) { Some(Value::I64(x)) => *x as i64, _ => 0 };
        let wh = match args.get(2) { Some(Value::I32(x)) => *x as u32, _ => 0 } as u8;
        let n_ptr = match args.get(3) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        match wasi_ctx(store).env.fd_seek(fd, off, wh) {
            Ok(n) => {
                let _ = write_u64(store, n_ptr, n);
                Ok(vec![Value::I32(0)])
            }
            Err(e) => Ok(vec![Value::I32(e as u32)]),
        }
    }
);

crate::export_method!(
    "wasi_snapshot_preview1", "fd_tell",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn fd_tell_p1<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let fd = match args.get(0) { Some(Value::I32(x)) => *x as i32, _ => -1 };
        let p_ptr = match args.get(1) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        match wasi_ctx(store).env.fd_tell(fd) {
            Ok(n) => {
                let _ = write_u64(store, p_ptr, n);
                Ok(vec![Value::I32(0)])
            }
            Err(e) => Ok(vec![Value::I32(e as u32)]),
        }
    }
);

crate::export_method!(
    "wasi_snapshot_preview1", "fd_renumber",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn fd_renumber_p1<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let from = match args.get(0) { Some(Value::I32(x)) => *x as i32, _ => -1 };
        let to = match args.get(1) { Some(Value::I32(x)) => *x as i32, _ => -1 };
        match wasi_ctx(store).env.fd_renumber(from, to) {
            Ok(_) => Ok(vec![Value::I32(0)]),
            Err(e) => Ok(vec![Value::I32(e as u32)]),
        }
    }
);

crate::export_method!(
    "wasi_snapshot_preview1", "fd_filestat_set_size",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64)], vec![ValType::NumType(NumType::I32)],
    pub fn fd_filestat_set_size_p1<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let fd = match args.get(0) { Some(Value::I32(x)) => *x as i32, _ => -1 };
        let size = match args.get(1) { Some(Value::I64(x)) => *x as u64, _ => 0 };
        match wasi_ctx(store).env.fd_filestat_set_size(fd, size) {
            Ok(_) => Ok(vec![Value::I32(0)]),
            Err(e) => Ok(vec![Value::I32(e as u32)]),
        }
    }
);

crate::export_method!(
    "wasi_snapshot_preview1", "fd_filestat_set_times",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn fd_filestat_set_times_p1<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let fd = match args.get(0) { Some(Value::I32(x)) => *x as i32, _ => -1 };
        let atime = match args.get(1) { Some(Value::I64(x)) => *x as u64, _ => 0 };
        let mtime = match args.get(2) { Some(Value::I64(x)) => *x as u64, _ => 0 };
        let fst_flags = match args.get(3) { Some(Value::I32(x)) => *x as u16, _ => 0 };
        match wasi_ctx(store).env.fd_filestat_set_times(fd, atime, mtime, fst_flags) {
            Ok(_) => Ok(vec![Value::I32(0)]),
            Err(e) => Ok(vec![Value::I32(e as u32)]),
        }
    }
);

crate::export_method!(
    "wasi_snapshot_preview1", "fd_pread",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn fd_pread_p1<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let fd = match args.get(0) { Some(Value::I32(x)) => *x as i32, _ => -1 };
        let i_ptr = match args.get(1) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let i_len = match args.get(2) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let offset = match args.get(3) { Some(Value::I64(x)) => *x as u64, _ => 0 };
        let n_ptr = match args.get(4) { Some(Value::I32(x)) => *x as u32, _ => 0 };

        let mut iovs = Vec::new();
        for i in 0..i_len {
            let mut iov = [0u8; 8];
            if crate::wasm::wasi::preview2::read_bytes(store, i_ptr + i * 8, &mut iov).is_err() { return Ok(vec![Value::I32(21)]); }
            let b_ptr = u32::from_le_bytes(iov[0..4].try_into().unwrap());
            let b_len = u32::from_le_bytes(iov[4..8].try_into().unwrap());
            iovs.push((b_ptr, b_len));
        }

        let mut buffers = Vec::new();
        for (_, len) in &iovs { buffers.push(vec![0u8; *len as usize]); }
        let mut slices: Vec<&mut [u8]> = buffers.iter_mut().map(|v| v.as_mut_slice()).collect();

        match wasi_ctx(store).env.fd_pread(fd, &mut slices, offset) {
            Ok(n) => {
                let mut remaining = n;
                for ((ptr, _), buf) in iovs.iter().zip(buffers.iter()) {
                    let to_write = core::cmp::min(remaining, buf.len());
                    if to_write > 0 {
                        let _ = write_bytes(store, *ptr, &buf[..to_write]);
                        remaining -= to_write;
                    }
                }
                let _ = write_u32(store, n_ptr, n as u32);
                Ok(vec![Value::I32(0)])
            }
            Err(e) => Ok(vec![Value::I32(e as u32)]),
        }
    }
);

crate::export_method!(
    "wasi_snapshot_preview1", "fd_pwrite",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn fd_pwrite_p1<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let fd = match args.get(0) { Some(Value::I32(x)) => *x as i32, _ => -1 };
        let i_ptr = match args.get(1) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let i_len = match args.get(2) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let offset = match args.get(3) { Some(Value::I64(x)) => *x as u64, _ => 0 };
        let n_ptr = match args.get(4) { Some(Value::I32(x)) => *x as u32, _ => 0 };

        let mut buffers = Vec::new();
        for i in 0..i_len {
            let mut iov = [0u8; 8];
            if crate::wasm::wasi::preview2::read_bytes(store, i_ptr + i * 8, &mut iov).is_err() { return Ok(vec![Value::I32(21)]); }
            let b_ptr = u32::from_le_bytes(iov[0..4].try_into().unwrap());
            let b_len = u32::from_le_bytes(iov[4..8].try_into().unwrap());
            let mut b = vec![0u8; b_len as usize];
            if crate::wasm::wasi::preview2::read_bytes(store, b_ptr, &mut b).is_err() { return Ok(vec![Value::I32(21)]); }
            buffers.push(b);
        }

        let slices: Vec<&[u8]> = buffers.iter().map(|v| v.as_slice()).collect();
        match wasi_ctx(store).env.fd_pwrite(fd, &slices, offset) {
            Ok(n) => {
                let _ = write_u32(store, n_ptr, n as u32);
                Ok(vec![Value::I32(0)])
            }
            Err(e) => Ok(vec![Value::I32(e as u32)]),
        }
    }
);

crate::export_method!(
    "wasi_snapshot_preview1", "fd_prestat_get",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn fd_prestat_get_p1<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let fd = match args.get(0) { Some(Value::I32(x)) => *x as i32, _ => -1 };
        let p_ptr = match args.get(1) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        match wasi_ctx(store).env.fd_prestat_get(fd) {
            Ok(_) => {
                let name = wasi_ctx(store).env.fd_prestat_dir_name(fd).unwrap_or_default();
                let _ = write_u32(store, p_ptr, 0); // pr_type = 0 (preopentype_dir)
                let _ = write_u32(store, p_ptr + 4, name.len() as u32);
                Ok(vec![Value::I32(0)])
            }
            Err(e) => Ok(vec![Value::I32(e as u32)]),
        }
    }
);

crate::export_method!(
    "wasi_snapshot_preview1", "fd_prestat_dir_name",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn fd_prestat_dir_name_p1<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let fd = match args.get(0) { Some(Value::I32(x)) => *x as i32, _ => -1 };
        let ptr = match args.get(1) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let _len = match args.get(2) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        match wasi_ctx(store).env.fd_prestat_dir_name(fd) {
            Ok(name) => {
                let _ = write_bytes(store, ptr, name.as_bytes());
                Ok(vec![Value::I32(0)])
            }
            Err(e) => Ok(vec![Value::I32(e as u32)]),
        }
    }
);

crate::export_method!(
    "wasi_snapshot_preview1", "fd_readdir",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn fd_readdir_p1<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let fd = match args.get(0) { Some(Value::I32(x)) => *x as i32, _ => -1 };
        let ptr = match args.get(1) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let len = match args.get(2) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let cookie = match args.get(3) { Some(Value::I64(x)) => *x as u64, _ => 0 };
        let n_ptr = match args.get(4) { Some(Value::I32(x)) => *x as u32, _ => 0 };

        match wasi_ctx(store).env.fd_readdir(fd, cookie) {
            Ok(entries) => {
                let mut written = 0u32;
                for (name, kind, next_cookie) in entries {
                    let name_bytes = name.as_bytes();
                    let entry_len = 24 + name_bytes.len() as u32;
                    if written + entry_len > len {
                        // Entry doesn't fit completely. Write a partial entry to fill
                        // the buffer so bufused == buflen, signaling "more entries"
                        // per the WASI spec (bufused < buflen means end-of-directory).
                        let remaining = (len - written) as usize;
                        if remaining > 0 {
                            let base = ptr + written;
                            let mut full = crate::alloc::vec![0u8; entry_len as usize];
                            full[0..8].copy_from_slice(&next_cookie.to_le_bytes());
                            full[16..20].copy_from_slice(&(name_bytes.len() as u32).to_le_bytes());
                            full[20] = kind;
                            if !name_bytes.is_empty() {
                                full[24..24 + name_bytes.len()].copy_from_slice(name_bytes);
                            }
                            let _ = write_bytes(store, base, &full[..remaining]);
                            written = len;
                        }
                        break;
                    }

                    let base = ptr + written;
                    let _ = write_u64(store, base, next_cookie);
                    let _ = write_u64(store, base + 8, 0); // inode stub
                    let _ = write_u32(store, base + 16, name_bytes.len() as u32);
                    let _ = write_bytes(store, base + 20, &[kind]);
                    let _ = write_bytes(store, base + 24, name_bytes);
                    written += entry_len;
                }
                let _ = write_u32(store, n_ptr, written);
                Ok(vec![Value::I32(0)])
            }
            Err(e) => Ok(vec![Value::I32(e as u32)]),
        }
    }
);

crate::export_method!(
    "wasi_snapshot_preview1", "fd_advise",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn fd_advise_p1<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let fd = match args.get(0) { Some(Value::I32(x)) => *x as i32, _ => -1 };
        let offset = match args.get(1) { Some(Value::I64(x)) => *x as u64, _ => 0 };
        let len = match args.get(2) { Some(Value::I64(x)) => *x as u64, _ => 0 };
        let advice = match args.get(3) { Some(Value::I32(x)) => *x as u8, _ => 0 };
        match wasi_ctx(store).env.fd_advise(fd, offset, len, advice) {
            Ok(_) => Ok(vec![Value::I32(0)]),
            Err(e) => Ok(vec![Value::I32(e as u32)]),
        }
    }
);

crate::export_method!(
    "wasi_snapshot_preview1", "fd_allocate",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I64)], vec![ValType::NumType(NumType::I32)],
    pub fn fd_allocate_p1<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let fd = match args.get(0) { Some(Value::I32(x)) => *x as i32, _ => -1 };
        let offset = match args.get(1) { Some(Value::I64(x)) => *x as u64, _ => 0 };
        let len = match args.get(2) { Some(Value::I64(x)) => *x as u64, _ => 0 };
        match wasi_ctx(store).env.fd_allocate(fd, offset, len) {
            Ok(_) => Ok(vec![Value::I32(0)]),
            Err(e) => Ok(vec![Value::I32(e as u32)]),
        }
    }
);

crate::export_method!(
    "wasi_snapshot_preview1", "fd_datasync",
    [],
    vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn fd_datasync_p1<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let fd = match args.get(0) { Some(Value::I32(x)) => *x as i32, _ => -1 };
        match wasi_ctx(store).env.fd_datasync(fd) {
            Ok(_) => Ok(vec![Value::I32(0)]),
            Err(e) => Ok(vec![Value::I32(e as u32)]),
        }
    }
);

crate::export_method!(
    "wasi_snapshot_preview1", "fd_fdstat_set_flags",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn fd_fdstat_set_flags_p1<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let fd = match args.get(0) { Some(Value::I32(x)) => *x as i32, _ => -1 };
        let flags = match args.get(1) { Some(Value::I32(x)) => *x as u16, _ => 0 };
        match wasi_ctx(store).env.fd_fdstat_set_flags(fd, flags) {
            Ok(_) => Ok(vec![Value::I32(0)]),
            Err(e) => Ok(vec![Value::I32(e as u32)]),
        }
    }
);

crate::export_method!(
    "wasi_snapshot_preview1", "fd_fdstat_set_rights",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I64)], vec![ValType::NumType(NumType::I32)],
    pub fn fd_fdstat_set_rights_p1<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let fd = match args.get(0) { Some(Value::I32(x)) => *x as i32, _ => -1 };
        let rb = match args.get(1) { Some(Value::I64(x)) => *x as u64, _ => 0 };
        let ri = match args.get(2) { Some(Value::I64(x)) => *x as u64, _ => 0 };
        match wasi_ctx(store).env.fd_fdstat_set_rights(fd, rb, ri) {
            Ok(_) => Ok(vec![Value::I32(0)]),
            Err(e) => Ok(vec![Value::I32(e as u32)]),
        }
    }
);

crate::export_method!(
    "wasi_snapshot_preview1", "fd_sync",
    [],
    vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn fd_sync_p1<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let fd = match args.get(0) { Some(Value::I32(x)) => *x as i32, _ => -1 };
        match wasi_ctx(store).env.fd_sync(fd) {
            Ok(_) => Ok(vec![Value::I32(0)]),
            Err(e) => Ok(vec![Value::I32(e as u32)]),
        }
    }
);

crate::export_method!(
    "wasi_snapshot_preview1", "path_filestat_get",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn path_filestat_get_p1<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let dirfd = match args.get(0) { Some(Value::I32(x)) => *x as i32, _ => -1 };
        let flags = match args.get(1) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let ptr = match args.get(2) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let len = match args.get(3) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let s_ptr = match args.get(4) { Some(Value::I32(x)) => *x as u32, _ => 0 };

        let mut pb = vec![0u8; len as usize];
        if crate::wasm::wasi::preview2::read_bytes(store, ptr, &mut pb).is_err() { return Ok(vec![Value::I32(21)]); }
        let path = crate::alloc::string::String::from_utf8_lossy(&pb).into_owned();

        match wasi_ctx(store).env.path_filestat_get(dirfd, flags, &path) {
            Ok(s) => {
                let _ = write_u64(store, s_ptr, s.dev);
                let _ = write_u64(store, s_ptr + 8, s.ino);
                let _ = write_bytes(store, s_ptr + 16, &[s.filetype]);
                let _ = write_u64(store, s_ptr + 24, s.nlink);
                let _ = write_u64(store, s_ptr + 32, s.size);
                let _ = write_u64(store, s_ptr + 40, s.atime);
                let _ = write_u64(store, s_ptr + 48, s.mtime);
                let _ = write_u64(store, s_ptr + 56, s.ctime);
                Ok(vec![Value::I32(0)])
            }
            Err(e) => Ok(vec![Value::I32(e as u32)]),
        }
    }
);

crate::export_method!(
    "wasi_snapshot_preview1", "path_create_directory",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn path_create_directory_p1<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let dirfd = match args.get(0) { Some(Value::I32(x)) => *x as i32, _ => -1 };
        let ptr = match args.get(1) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let len = match args.get(2) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let mut pb = vec![0u8; len as usize];
        if crate::wasm::wasi::preview2::read_bytes(store, ptr, &mut pb).is_err() { return Ok(vec![Value::I32(21)]); }
        let path = crate::alloc::string::String::from_utf8_lossy(&pb).into_owned();
        match wasi_ctx(store).env.path_create_directory(dirfd, &path) {
            Ok(_) => Ok(vec![Value::I32(0)]),
            Err(e) => Ok(vec![Value::I32(e as u32)]),
        }
    }
);

crate::export_method!(
    "wasi_snapshot_preview1", "path_remove_directory",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn path_remove_directory_p1<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let dirfd = match args.get(0) { Some(Value::I32(x)) => *x as i32, _ => -1 };
        let ptr = match args.get(1) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let len = match args.get(2) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let mut pb = vec![0u8; len as usize];
        if crate::wasm::wasi::preview2::read_bytes(store, ptr, &mut pb).is_err() { return Ok(vec![Value::I32(21)]); }
        let path = crate::alloc::string::String::from_utf8_lossy(&pb).into_owned();
        match wasi_ctx(store).env.path_remove_directory(dirfd, &path) {
            Ok(_) => Ok(vec![Value::I32(0)]),
            Err(e) => Ok(vec![Value::I32(e as u32)]),
        }
    }
);

crate::export_method!(
    "wasi_snapshot_preview1", "path_unlink_file",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn path_unlink_file_p1<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let dirfd = match args.get(0) { Some(Value::I32(x)) => *x as i32, _ => -1 };
        let ptr = match args.get(1) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let len = match args.get(2) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let mut pb = vec![0u8; len as usize];
        if crate::wasm::wasi::preview2::read_bytes(store, ptr, &mut pb).is_err() { return Ok(vec![Value::I32(21)]); }
        let path = crate::alloc::string::String::from_utf8_lossy(&pb).into_owned();
        match wasi_ctx(store).env.path_unlink_file(dirfd, &path) {
            Ok(_) => Ok(vec![Value::I32(0)]),
            Err(e) => Ok(vec![Value::I32(e as u32)]),
        }
    }
);

crate::export_method!(
    "wasi_snapshot_preview1", "path_rename",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn path_rename_p1<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let old_fd = match args.get(0) { Some(Value::I32(x)) => *x as i32, _ => -1 };
        let old_ptr = match args.get(1) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let old_len = match args.get(2) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let new_fd = match args.get(3) { Some(Value::I32(x)) => *x as i32, _ => -1 };
        let new_ptr = match args.get(4) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let new_len = match args.get(5) { Some(Value::I32(x)) => *x as u32, _ => 0 };

        let mut old_pb = vec![0u8; old_len as usize];
        if crate::wasm::wasi::preview2::read_bytes(store, old_ptr, &mut old_pb).is_err() { return Ok(vec![Value::I32(21)]); }
        let old_path = crate::alloc::string::String::from_utf8_lossy(&old_pb).into_owned();

        let mut new_pb = vec![0u8; new_len as usize];
        if crate::wasm::wasi::preview2::read_bytes(store, new_ptr, &mut new_pb).is_err() { return Ok(vec![Value::I32(21)]); }
        let new_path = crate::alloc::string::String::from_utf8_lossy(&new_pb).into_owned();

        match wasi_ctx(store).env.path_rename(old_fd, &old_path, new_fd, &new_path) {
            Ok(_) => Ok(vec![Value::I32(0)]),
            Err(e) => Ok(vec![Value::I32(e as u32)]),
        }
    }
);

crate::export_method!(
    "wasi_snapshot_preview1", "path_link",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn path_link_p1<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let old_fd = match args.get(0) { Some(Value::I32(x)) => *x as i32, _ => -1 };
        let old_flags = match args.get(1) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let old_ptr = match args.get(2) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let old_len = match args.get(3) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let new_fd = match args.get(4) { Some(Value::I32(x)) => *x as i32, _ => -1 };
        let new_ptr = match args.get(5) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let new_len = match args.get(6) { Some(Value::I32(x)) => *x as u32, _ => 0 };

        let mut old_pb = vec![0u8; old_len as usize];
        if crate::wasm::wasi::preview2::read_bytes(store, old_ptr, &mut old_pb).is_err() { return Ok(vec![Value::I32(21)]); }
        let old_path = crate::alloc::string::String::from_utf8_lossy(&old_pb).into_owned();

        let mut new_pb = vec![0u8; new_len as usize];
        if crate::wasm::wasi::preview2::read_bytes(store, new_ptr, &mut new_pb).is_err() { return Ok(vec![Value::I32(21)]); }
        let new_path = crate::alloc::string::String::from_utf8_lossy(&new_pb).into_owned();

        match wasi_ctx(store).env.path_link(old_fd, old_flags, &old_path, new_fd, &new_path) {
            Ok(_) => Ok(vec![Value::I32(0)]),
            Err(e) => Ok(vec![Value::I32(e as u32)]),
        }
    }
);

crate::export_method!(
    "wasi_snapshot_preview1", "path_symlink",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn path_symlink_p1<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let old_ptr = match args.get(0) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let old_len = match args.get(1) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let fd = match args.get(2) { Some(Value::I32(x)) => *x as i32, _ => -1 };
        let new_ptr = match args.get(3) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let new_len = match args.get(4) { Some(Value::I32(x)) => *x as u32, _ => 0 };

        let mut old_pb = vec![0u8; old_len as usize];
        if crate::wasm::wasi::preview2::read_bytes(store, old_ptr, &mut old_pb).is_err() { return Ok(vec![Value::I32(21)]); }
        let old_path = crate::alloc::string::String::from_utf8_lossy(&old_pb).into_owned();

        let mut new_pb = vec![0u8; new_len as usize];
        if crate::wasm::wasi::preview2::read_bytes(store, new_ptr, &mut new_pb).is_err() { return Ok(vec![Value::I32(21)]); }
        let new_path = crate::alloc::string::String::from_utf8_lossy(&new_pb).into_owned();

        match wasi_ctx(store).env.path_symlink(&old_path, fd, &new_path) {
            Ok(_) => Ok(vec![Value::I32(0)]),
            Err(e) => Ok(vec![Value::I32(e as u32)]),
        }
    }
);

crate::export_method!(
    "wasi_snapshot_preview1", "path_readlink",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn path_readlink_p1<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let dirfd = match args.get(0) { Some(Value::I32(x)) => *x as i32, _ => -1 };
        let ptr = match args.get(1) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let len = match args.get(2) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let buf_ptr = match args.get(3) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let buf_len = match args.get(4) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let n_ptr = match args.get(5) { Some(Value::I32(x)) => *x as u32, _ => 0 };

        let mut pb = vec![0u8; len as usize];
        if crate::wasm::wasi::preview2::read_bytes(store, ptr, &mut pb).is_err() { return Ok(vec![Value::I32(21)]); }
        let path = crate::alloc::string::String::from_utf8_lossy(&pb).into_owned();

        let mut buf = vec![0u8; buf_len as usize];
        match wasi_ctx(store).env.path_readlink(dirfd, &path, &mut buf) {
            Ok(n) => {
                let _ = write_bytes(store, buf_ptr, &buf[..n]);
                let _ = write_u32(store, n_ptr, n as u32);
                Ok(vec![Value::I32(0)])
            }
            Err(e) => Ok(vec![Value::I32(e as u32)]),
        }
    }
);

crate::export_method!(
    "wasi_snapshot_preview1", "path_filestat_set_times",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn path_filestat_set_times_p1<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let dirfd = match args.get(0) { Some(Value::I32(x)) => *x as i32, _ => -1 };
        let flags = match args.get(1) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let ptr = match args.get(2) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let len = match args.get(3) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let atime = match args.get(4) { Some(Value::I64(x)) => *x as u64, _ => 0 };
        let mtime = match args.get(5) { Some(Value::I64(x)) => *x as u64, _ => 0 };
        let fst_flags = match args.get(6) { Some(Value::I32(x)) => *x as u16, _ => 0 };

        let mut pb = vec![0u8; len as usize];
        if crate::wasm::wasi::preview2::read_bytes(store, ptr, &mut pb).is_err() { return Ok(vec![Value::I32(21)]); }
        let path = crate::alloc::string::String::from_utf8_lossy(&pb).into_owned();

        match wasi_ctx(store).env.path_filestat_set_times(dirfd, flags, &path, atime, mtime, fst_flags) {
            Ok(_) => Ok(vec![Value::I32(0)]),
            Err(e) => Ok(vec![Value::I32(e as u32)]),
        }
    }
);

crate::export_method!(
    "wasi_snapshot_preview1", "path_open",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn path_open_p1<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let dirfd = match args.get(0) { Some(Value::I32(x)) => *x as i32, _ => -1 };
        let dirflags = match args.get(1) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let ptr = match args.get(2) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let len = match args.get(3) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let of = match args.get(4) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let rb = match args.get(5) { Some(Value::I64(x)) => *x as u64, _ => 0 };
        let ri = match args.get(6) { Some(Value::I64(x)) => *x as u64, _ => 0 };
        let fdflags = match args.get(7) { Some(Value::I32(x)) => *x as u16, _ => 0 };
        let n_ptr = match args.get(8) { Some(Value::I32(x)) => *x as u32, _ => 0 };

        let mut pb = vec![0u8; len as usize];
        if crate::wasm::wasi::preview2::read_bytes(store, ptr, &mut pb).is_err() { return Ok(vec![Value::I32(21)]); }
        let path = crate::alloc::string::String::from_utf8_lossy(&pb).into_owned();

        match wasi_ctx(store).env.path_open(dirfd, dirflags, &path, of, rb, ri, fdflags) {
            Ok(fd) => {
                let _ = write_u32(store, n_ptr, fd as u32);
                Ok(vec![Value::I32(0)])
            }
            Err(e) => Ok(vec![Value::I32(e as u32)]),
        }
    }
);

crate::export_method!(
    "wasi:filesystem/types@0.2.0", "[resource-drop]descriptor",
    [],
    vec![ValType::NumType(NumType::I32)], vec![],
    pub fn descriptor_drop<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        crate::wasm::wasi::preview2::resource_drop(store, args)
    }
);

crate::export_method!(
    "wasi:filesystem/types@0.2.0", "[resource-drop]directory-entry-stream",
    [],
    vec![ValType::NumType(NumType::I32)], vec![],
    pub fn directory_entry_stream_drop<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        crate::wasm::wasi::preview2::resource_drop(store, args)
    }
);

pub fn register_wasi<T: Config + Clone>(linker: &mut crate::wasm::Linker, store: &mut crate::wasm::Store<'_, T>) {
    get_directories::register(linker, store);
    read_via_stream::register(linker, store);
    write_via_stream::register(linker, store);
    append_via_stream::register(linker, store);
    descriptor_type::register(linker, store);
    descriptor_stat::register(linker, store);
    descriptor_open_at::register(linker, store);
    descriptor_read_directory::register(linker, store);
    directory_entry_stream_read_directory_entry::register(linker, store);
    descriptor_stat_at::register(linker, store);
    descriptor_set_times_at::register(linker, store);
    descriptor_create_directory_at::register(linker, store);
    descriptor_unlink_file_at::register(linker, store);
    descriptor_remove_directory_at::register(linker, store);
    descriptor_link_at::register(linker, store);
    descriptor_rename_at::register(linker, store);
    descriptor_symlink_at::register(linker, store);
    descriptor_readlink_at::register(linker, store);
    descriptor_sync::register(linker, store);
    descriptor_set_size::register(linker, store);
    descriptor_set_times::register(linker, store);
    descriptor_seek::register(linker, store);
    descriptor_advise::register(linker, store);
    descriptor_get_flags::register(linker, store);
    descriptor_sync_data::register(linker, store);
    descriptor_is_same_object::register(linker, store);
    descriptor_metadata_hash::register(linker, store);
    descriptor_metadata_hash_at::register(linker, store);
    descriptor_read::register(linker, store);
    descriptor_write::register(linker, store);
    filesystem_error_code::register(linker, store);
    fd_close_p1::register(linker, store);
    fd_fdstat_get_p1::register(linker, store);
    fd_filestat_get_p1::register(linker, store);
    fd_filestat_set_size_p1::register(linker, store);
    fd_filestat_set_times_p1::register(linker, store);
    fd_prestat_get_p1::register(linker, store);
    fd_prestat_dir_name_p1::register(linker, store);
    fd_pread_p1::register(linker, store);
    fd_pwrite_p1::register(linker, store);
    fd_readdir_p1::register(linker, store);
    fd_renumber_p1::register(linker, store);
    fd_sync_p1::register(linker, store);
    fd_advise_p1::register(linker, store);
    fd_allocate_p1::register(linker, store);
    fd_datasync_p1::register(linker, store);
    fd_fdstat_set_flags_p1::register(linker, store);
    fd_fdstat_set_rights_p1::register(linker, store);
    fd_read_p1::register(linker, store);
    fd_write_p1::register(linker, store);
    fd_seek_p1::register(linker, store);
    fd_tell_p1::register(linker, store);
    path_create_directory_p1::register(linker, store);
    path_filestat_get_p1::register(linker, store);
    path_filestat_set_times_p1::register(linker, store);
    path_link_p1::register(linker, store);
    path_open_p1::register(linker, store);
    path_readlink_p1::register(linker, store);
    path_remove_directory_p1::register(linker, store);
    path_rename_p1::register(linker, store);
    path_symlink_p1::register(linker, store);
    path_unlink_file_p1::register(linker, store);
    descriptor_drop::register(linker, store);
    directory_entry_stream_drop::register(linker, store);
}

fn wasi_ctx<'a, T: Config>(store: &'a mut Store<'_, T>) -> &'a mut crate::wasm::wasi::ctx::WasiCtx {
    if store.wasi_ctx.is_none() {
        store.wasi_ctx = Some(crate::wasm::wasi::ctx::WasiCtx::default());
    }
    store.wasi_ctx.as_mut().unwrap()
}
