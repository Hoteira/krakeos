use crate::rust_alloc::{format, string::String, string::ToString, vec, vec::Vec};
use crate::wasm::{
    core::reader::types::{FuncType, NumType, ResultType, ValType},
    execution::{
        config::Config,
        linker::Linker,
        store::{ExternVal, HaltExecutionError, Store},
        value::Value,
    },
};
use super::ctx::WasiCtx;

pub static mut RANDOM_STATE: u64 = 1574;

pub fn create_wasi_imports<T: Config>(linker: &mut Linker, store: &mut Store<'_, T>) {
    let wasi_module = String::from("wasi_snapshot_preview1");
    let mut define = |name: &str, params: Vec<ValType>, returns: Vec<ValType>, func: for<'a> fn(&mut Store<'a, T>, Vec<Value>) -> Result<Vec<Value>, HaltExecutionError>| {
        let func_type = FuncType { params: ResultType { valtypes: params }, returns: ResultType { valtypes: returns } };
        let func_addr = store.func_alloc_unchecked(func_type, func);
        let _ = linker.define_unchecked(wasi_module.clone(), String::from(name), ExternVal::Func(func_addr));
    };
    let i32_t = ValType::NumType(NumType::I32);
    let i64_t = ValType::NumType(NumType::I64);
    define("args_get", vec![i32_t, i32_t], vec![i32_t], args_get);
    define("args_sizes_get", vec![i32_t, i32_t], vec![i32_t], args_sizes_get);
    define("environ_get", vec![i32_t, i32_t], vec![i32_t], environ_get);
    define("environ_sizes_get", vec![i32_t, i32_t], vec![i32_t], environ_sizes_get);
    define("clock_res_get", vec![i32_t, i32_t], vec![i32_t], clock_res_get);
    define("clock_time_get", vec![i32_t, i64_t, i32_t], vec![i32_t], clock_time_get);
    define("fd_close", vec![i32_t], vec![i32_t], fd_close);
    define("fd_fdstat_get", vec![i32_t, i32_t], vec![i32_t], fd_fdstat_get);
    define("fd_filestat_get", vec![i32_t, i32_t], vec![i32_t], fd_filestat_get);
    define("fd_filestat_set_size", vec![i32_t, i64_t], vec![i32_t], fd_filestat_set_size);
    define("fd_prestat_get", vec![i32_t, i32_t], vec![i32_t], fd_prestat_get);
    define("fd_prestat_dir_name", vec![i32_t, i32_t, i32_t], vec![i32_t], fd_prestat_dir_name);
    define("fd_read", vec![i32_t, i32_t, i32_t, i32_t], vec![i32_t], fd_read);
    define("fd_seek", vec![i32_t, i64_t, i32_t, i32_t], vec![i32_t], fd_seek);
    define("fd_tell", vec![i32_t, i32_t], vec![i32_t], fd_tell);
    define("fd_write", vec![i32_t, i32_t, i32_t, i32_t], vec![i32_t], fd_write);
    define("fd_sync", vec![i32_t], vec![i32_t], fd_sync);
    define("path_open", vec![i32_t, i32_t, i32_t, i32_t, i32_t, i64_t, i64_t, i32_t, i32_t], vec![i32_t], path_open);
    define("proc_exit", vec![i32_t], vec![], proc_exit);
    define("fd_readdir", vec![i32_t, i32_t, i32_t, i64_t, i32_t], vec![i32_t], fd_readdir);
    define("path_filestat_get", vec![i32_t, i32_t, i32_t, i32_t, i32_t], vec![i32_t], path_filestat_get);
    define("random_get", vec![i32_t, i32_t], vec![i32_t], random_get);
    define("path_create_directory", vec![i32_t, i32_t, i32_t], vec![i32_t], path_create_directory);
    define("path_remove_directory", vec![i32_t, i32_t, i32_t], vec![i32_t], path_remove_directory);
    define("path_unlink_file", vec![i32_t, i32_t, i32_t], vec![i32_t], path_unlink_file);
    define("path_rename", vec![i32_t, i32_t, i32_t, i32_t, i32_t, i32_t], vec![i32_t], path_rename);
    define("path_readlink", vec![i32_t, i32_t, i32_t, i32_t, i32_t, i32_t], vec![i32_t], path_readlink);
    define("sched_yield", vec![], vec![i32_t], sched_yield);
    define("poll_oneoff", vec![i32_t, i32_t, i32_t, i32_t], vec![i32_t], poll_oneoff);
}

// Helpers
fn wasi_ctx<'a, T: Config>(store: &'a mut Store<'_, T>) -> &'a mut WasiCtx {
    if store.wasi_ctx.is_none() {
        store.wasi_ctx = Some(WasiCtx::default());
    }
    store.wasi_ctx.as_mut().unwrap()
}

fn write_bytes<T: Config>(store: &mut Store<'_, T>, addr: u32, bytes: &[u8]) -> Result<(), ()> {
    let mem = if let Some(m) = store.memories.iter().next() { m } else { return Err(()); };
    mem.mem.init(addr as usize, bytes, 0, bytes.len()).map_err(|_| ())
}
fn read_bytes<T: Config>(store: &Store<'_, T>, addr: u32, buf: &mut [u8]) -> Result<(), ()> {
    let mem = if let Some(m) = store.memories.iter().next() { m } else { return Err(()); };
    mem.mem.read_slice(addr as usize, buf).map_err(|_| ())
}
fn write_u16<T: Config>(store: &mut Store<'_, T>, addr: u32, val: u16) -> Result<(), ()> { write_bytes(store, addr, &val.to_le_bytes()) }
fn write_u32<T: Config>(store: &mut Store<'_, T>, addr: u32, val: u32) -> Result<(), ()> { write_bytes(store, addr, &val.to_le_bytes()) }
fn write_u64<T: Config>(store: &mut Store<'_, T>, addr: u32, val: u64) -> Result<(), ()> { write_bytes(store, addr, &val.to_le_bytes()) }

// --- Implementation Wrappers ---

fn args_get<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let argv_ptr = args.get(0).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let argv_buf_ptr = args.get(1).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    
    let env_args = wasi_ctx(store).env.args_get().unwrap_or_default();
    
    let mut offset = 0;
    for (i, arg) in env_args.iter().enumerate() {
        let p = argv_buf_ptr + offset;
        if write_u32(store, argv_ptr + (i as u32 * 4), p).is_err() { return Ok(vec![Value::I32(28)]); }
        let b = arg.as_bytes();
        if write_bytes(store, p, b).is_err() || write_bytes(store, p + b.len() as u32, &[0]).is_err() { return Ok(vec![Value::I32(28)]); }
        offset += b.len() as u32 + 1;
    }
    Ok(vec![Value::I32(0)])
}

fn args_sizes_get<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let c_ptr = args.get(0).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let b_ptr = args.get(1).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    
    let env_args = wasi_ctx(store).env.args_get().unwrap_or_default();
    
    if write_u32(store, c_ptr, env_args.len() as u32).is_err() || write_u32(store, b_ptr, env_args.iter().map(|s| s.len() + 1).sum::<usize>() as u32).is_err() { return Ok(vec![Value::I32(28)]); }
    Ok(vec![Value::I32(0)])
}

fn environ_get<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let e_ptr = args.get(0).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let b_ptr = args.get(1).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    
    let env_vars = wasi_ctx(store).env.environ_get().unwrap_or_default();
    
    let mut offset = 0;
    for (i, (k, v)) in env_vars.iter().enumerate() {
        let entry = format!("{}={}", k, v);
        let p = b_ptr + offset;
        if write_u32(store, e_ptr + (i as u32 * 4), p).is_err() { return Ok(vec![Value::I32(28)]); }
        let b = entry.as_bytes();
        if write_bytes(store, p, b).is_err() || write_bytes(store, p + b.len() as u32, &[0]).is_err() { return Ok(vec![Value::I32(28)]); }
        offset += b.len() as u32 + 1;
    }
    Ok(vec![Value::I32(0)])
}

fn environ_sizes_get<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let c_ptr = args.get(0).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let b_ptr = args.get(1).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    
    let env_vars = wasi_ctx(store).env.environ_get().unwrap_or_default();
    
    if write_u32(store, c_ptr, env_vars.len() as u32).is_err() || write_u32(store, b_ptr, env_vars.iter().map(|(k, v)| k.len() + v.len() + 2).sum::<usize>() as u32).is_err() { return Ok(vec![Value::I32(28)]); }
    Ok(vec![Value::I32(0)])
}

fn clock_res_get<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let id = args.get(0).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let r_ptr = args.get(1).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    
    match wasi_ctx(store).env.clock_res_get(id) {
        Ok(res) => {
            let _ = write_u64(store, r_ptr, res);
            Ok(vec![Value::I32(0)])
        },
        Err(e) => Ok(vec![Value::I32(e as u32)])
    }
}

fn clock_time_get<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let id = args.get(0).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let precision = args.get(1).and_then(|v| if let Value::I64(x) = v { Some(*x as u64) } else { None }).unwrap_or(0);
    let t_ptr = args.get(2).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    
    match wasi_ctx(store).env.clock_time_get(id, precision) {
        Ok(t) => {
            let _ = write_u64(store, t_ptr, t);
            Ok(vec![Value::I32(0)])
        },
        Err(e) => Ok(vec![Value::I32(e as u32)])
    }
}

fn fd_close<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = args.get(0).and_then(|v| if let Value::I32(x) = v { Some(*x as i32) } else { None }).unwrap_or(-1);
    match wasi_ctx(store).env.fd_close(fd) {
        Ok(_) => Ok(vec![Value::I32(0)]),
        Err(e) => Ok(vec![Value::I32(e as u32)])
    }
}

fn fd_fdstat_get<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = args.get(0).and_then(|v| if let Value::I32(x) = v { Some(*x as i32) } else { None }).unwrap_or(-1);
    let s_ptr = args.get(1).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    
    match wasi_ctx(store).env.fd_fdstat_get(fd) {
        Ok(s) => {
            if write_u16(store, s_ptr, s.filetype as u16).is_err() || 
               write_u16(store, s_ptr + 2, s.flags).is_err() || 
               write_u64(store, s_ptr + 8, s.rights_base).is_err() || 
               write_u64(store, s_ptr + 16, s.rights_inheriting).is_err() { 
                   return Ok(vec![Value::I32(28)]); 
            }
            Ok(vec![Value::I32(0)])
        },
        Err(e) => Ok(vec![Value::I32(e as u32)])
    }
}

fn fd_filestat_get<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = args.get(0).and_then(|v| if let Value::I32(x) = v { Some(*x as i32) } else { None }).unwrap_or(-1);
    let b_ptr = args.get(1).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    
    match wasi_ctx(store).env.fd_filestat_get(fd) {
        Ok(s) => {
            if write_u64(store, b_ptr, s.dev).is_err() || 
               write_u64(store, b_ptr + 8, s.ino).is_err() || 
               write_bytes(store, b_ptr + 16, &[s.filetype]).is_err() || 
               write_u64(store, b_ptr + 24, s.nlink).is_err() || 
               write_u64(store, b_ptr + 32, s.size).is_err() || 
               write_u64(store, b_ptr + 40, s.atime).is_err() || 
               write_u64(store, b_ptr + 48, s.mtime).is_err() || 
               write_u64(store, b_ptr + 56, s.ctime).is_err() { 
                   return Ok(vec![Value::I32(28)]); 
            }
            Ok(vec![Value::I32(0)])
        },
        Err(e) => Ok(vec![Value::I32(e as u32)])
    }
}

fn fd_filestat_set_size<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = args.get(0).and_then(|v| if let Value::I32(x) = v { Some(*x as i32) } else { None }).unwrap_or(-1);
    let sz = args.get(1).and_then(|v| if let Value::I64(x) = v { Some(*x as u64) } else { None }).unwrap_or(0);
    match wasi_ctx(store).env.fd_filestat_set_size(fd, sz) {
        Ok(_) => Ok(vec![Value::I32(0)]),
        Err(e) => Ok(vec![Value::I32(e as u32)])
    }
}

fn fd_prestat_get<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = args.get(0).and_then(|v| if let Value::I32(x) = v { Some(*x as i32) } else { None }).unwrap_or(-1);
    let ptr = args.get(1).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    
    match wasi_ctx(store).env.fd_prestat_get(fd) {
        Ok(t) => {
            let name_len = wasi_ctx(store).env.fd_prestat_dir_name(fd).map(|s| s.len() as u32).unwrap_or(0);
            if write_bytes(store, ptr, &[t as u8]).is_err() || 
               write_u32(store, ptr + 4, name_len).is_err() {
                   return Ok(vec![Value::I32(28)]);
            }
            Ok(vec![Value::I32(0)])
        },
        Err(e) => Ok(vec![Value::I32(e as u32)])
    }
}

fn fd_prestat_dir_name<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = args.get(0).and_then(|v| if let Value::I32(x) = v { Some(*x as i32) } else { None }).unwrap_or(-1);
    let ptr = args.get(1).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let _len = args.get(2).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    
    match wasi_ctx(store).env.fd_prestat_dir_name(fd) {
        Ok(s) => {
            if write_bytes(store, ptr, s.as_bytes()).is_err() { return Ok(vec![Value::I32(28)]); }
            Ok(vec![Value::I32(0)])
        },
        Err(e) => Ok(vec![Value::I32(e as u32)])
    }
}

fn fd_read<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = args.get(0).and_then(|v| if let Value::I32(x) = v { Some(*x as i32) } else { None }).unwrap_or(-1);
    let i_ptr = args.get(1).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let i_len = args.get(2).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let n_ptr = args.get(3).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    
    let mut iovs = Vec::new();
    for i in 0..i_len {
        let mut iov = [0u8; 8];
        if read_bytes(store, i_ptr + i * 8, &mut iov).is_err() { return Ok(vec![Value::I32(21)]); }
        let b_ptr = u32::from_le_bytes(iov[0..4].try_into().unwrap());
        let b_len = u32::from_le_bytes(iov[4..8].try_into().unwrap());
        iovs.push((b_ptr, b_len));
    }

    let mut buffers = Vec::new();
    // Temporarily allocate buffers to read into
    for (_, len) in &iovs {
        buffers.push(vec![0u8; *len as usize]);
    }
    
    // Construct slice of mutable slices
    let mut slices: Vec<&mut [u8]> = buffers.iter_mut().map(|v| v.as_mut_slice()).collect();
    
    match wasi_ctx(store).env.fd_read(fd, &mut slices) {
        Ok(n) => {
            // Write back
            for ((ptr, _), buf) in iovs.iter().zip(buffers.iter()) {
                if write_bytes(store, *ptr, buf).is_err() { return Ok(vec![Value::I32(28)]); }
            }
            if n_ptr != 0 { let _ = write_u32(store, n_ptr, n as u32); }
            Ok(vec![Value::I32(0)])
        },
        Err(e) => Ok(vec![Value::I32(e as u32)])
    }
}

fn fd_seek<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = args.get(0).and_then(|v| if let Value::I32(x) = v { Some(*x as i32) } else { None }).unwrap_or(-1);
    let off = args.get(1).and_then(|v| if let Value::I64(x) = v { Some(*x as i64) } else { None }).unwrap_or(0);
    let wh = args.get(2).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0) as u8;
    let n_ptr = args.get(3).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    
    match wasi_ctx(store).env.fd_seek(fd, off, wh) {
        Ok(n) => {
            if n_ptr != 0 { let _ = write_u64(store, n_ptr, n); }
            Ok(vec![Value::I32(0)])
        },
        Err(e) => Ok(vec![Value::I32(e as u32)])
    }
}

fn fd_tell<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = args.get(0).and_then(|v| if let Value::I32(x) = v { Some(*x as i32) } else { None }).unwrap_or(-1);
    let p_ptr = args.get(1).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    
    match wasi_ctx(store).env.fd_tell(fd) {
        Ok(n) => {
            if p_ptr != 0 { let _ = write_u64(store, p_ptr, n); }
            Ok(vec![Value::I32(0)])
        },
        Err(e) => Ok(vec![Value::I32(e as u32)])
    }
}

fn fd_write<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = args.get(0).and_then(|v| if let Value::I32(x) = v { Some(*x as i32) } else { None }).unwrap_or(-1);
    let i_ptr = args.get(1).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let i_len = args.get(2).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let n_ptr = args.get(3).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    
    let mut buffers = Vec::new();
    for i in 0..i_len {
        let mut iov = [0u8; 8];
        if read_bytes(store, i_ptr + i * 8, &mut iov).is_err() { return Ok(vec![Value::I32(21)]); }
        let b_ptr = u32::from_le_bytes(iov[0..4].try_into().unwrap());
        let b_len = u32::from_le_bytes(iov[4..8].try_into().unwrap());
        let mut b = vec![0u8; b_len as usize];
        if read_bytes(store, b_ptr, &mut b).is_err() { return Ok(vec![Value::I32(21)]); }
        buffers.push(b);
    }
    
    let slices: Vec<&[u8]> = buffers.iter().map(|v| v.as_slice()).collect();
    
    match wasi_ctx(store).env.fd_write(fd, &slices) {
        Ok(n) => {
            let _ = write_u32(store, n_ptr, n as u32);
            Ok(vec![Value::I32(0)])
        },
        Err(e) => Ok(vec![Value::I32(e as u32)])
    }
}

fn fd_sync<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = args.get(0).and_then(|v| if let Value::I32(x) = v { Some(*x as i32) } else { None }).unwrap_or(-1);
    match wasi_ctx(store).env.fd_sync(fd) {
        Ok(_) => Ok(vec![Value::I32(0)]),
        Err(e) => Ok(vec![Value::I32(e as u32)])
    }
}

fn path_open<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let dirfd = args.get(0).and_then(|v| if let Value::I32(x) = v { Some(*x as i32) } else { None }).unwrap_or(-1);
    let dirflags = args.get(1).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let ptr = args.get(2).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let len = args.get(3).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let of = args.get(4).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let rights_base = args.get(5).and_then(|v| if let Value::I64(x) = v { Some(*x as u64) } else { None }).unwrap_or(0);
    let rights_inh = args.get(6).and_then(|v| if let Value::I64(x) = v { Some(*x as u64) } else { None }).unwrap_or(0);
    let fdflags = args.get(7).and_then(|v| if let Value::I32(x) = v { Some(*x as u16) } else { None }).unwrap_or(0);
    let f_ptr = args.get(8).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    
    let mut pb = vec![0u8; len as usize];
    if read_bytes(store, ptr, &mut pb).is_err() { return Ok(vec![Value::I32(21)]); }
    let path = String::from_utf8_lossy(&pb).into_owned();
    
    match wasi_ctx(store).env.path_open(dirfd, dirflags, &path, of, rights_base, rights_inh, fdflags) {
        Ok(fd) => {
            if f_ptr != 0 { let _ = write_u32(store, f_ptr, fd as u32); }
            Ok(vec![Value::I32(0)])
        },
        Err(e) => Ok(vec![Value::I32(e as u32)])
    }
}

fn path_filestat_get<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = args.get(0).and_then(|v| if let Value::I32(x) = v { Some(*x as i32) } else { None }).unwrap_or(-1);
    let flags = args.get(1).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let ptr = args.get(2).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let len = args.get(3).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let b_ptr = args.get(4).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    
    let mut pb = vec![0u8; len as usize];
    if read_bytes(store, ptr, &mut pb).is_err() { return Ok(vec![Value::I32(21)]); }
    let path = String::from_utf8_lossy(&pb).into_owned();
    
    match wasi_ctx(store).env.path_filestat_get(fd, flags, &path) {
        Ok(s) => {
            if write_u64(store, b_ptr, s.dev).is_err() || 
               write_u64(store, b_ptr + 8, s.ino).is_err() || 
               write_bytes(store, b_ptr + 16, &[s.filetype]).is_err() || 
               write_u64(store, b_ptr + 24, s.nlink).is_err() || 
               write_u64(store, b_ptr + 32, s.size).is_err() || 
               write_u64(store, b_ptr + 40, s.atime).is_err() || 
               write_u64(store, b_ptr + 48, s.mtime).is_err() || 
               write_u64(store, b_ptr + 56, s.ctime).is_err() { 
                   return Ok(vec![Value::I32(28)]); 
            }
            Ok(vec![Value::I32(0)])
        },
        Err(e) => Ok(vec![Value::I32(e as u32)])
    }
}

fn random_get<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let ptr = args.get(0).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let len = args.get(1).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    
    let mut b = vec![0u8; len as usize];
    match wasi_ctx(store).env.random_get(&mut b) {
        Ok(_) => {
            if write_bytes(store, ptr, &b).is_err() { return Ok(vec![Value::I32(28)]); }
            Ok(vec![Value::I32(0)])
        },
        Err(e) => Ok(vec![Value::I32(e as u32)])
    }
}

fn sched_yield<T: Config>(store: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    match wasi_ctx(store).env.sched_yield() {
        Ok(_) => Ok(vec![Value::I32(0)]),
        Err(e) => Ok(vec![Value::I32(e as u32)])
    }
}

fn poll_oneoff<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let in_p = args.get(0).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let out_p = args.get(1).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let nsub = args.get(2).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let nev_p = args.get(3).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    
    // Read input events manually as it's complex structure
    let mut in_events = vec![0u8; (nsub * 48) as usize];
    if read_bytes(store, in_p, &mut in_events).is_err() { return Ok(vec![Value::I32(21)]); }
    
    // We pass raw bytes to env for now as parsing Subscription struct is tedious here
    // Ideally WasiEnv should take a list of Subscription structs
    // For this refactor I'll keep it raw or simple
    
    let mut out_events = vec![0u8; (nsub * 32) as usize];
    
    match wasi_ctx(store).env.poll_oneoff(&in_events, &mut out_events, nsub) {
        Ok(nev) => {
            if write_bytes(store, out_p, &out_events[0..(nev as usize * 32)]).is_err() { return Ok(vec![Value::I32(28)]); }
            if write_u32(store, nev_p, nev).is_err() { return Ok(vec![Value::I32(28)]); }
            Ok(vec![Value::I32(0)])
        },
        Err(e) => Ok(vec![Value::I32(e as u32)])
    }
}

fn proc_exit<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let exit_code = args.get(0).and_then(|v| if let Value::I32(x) = v { Some(*x as i32) } else { None }).unwrap_or(0);
    wasi_ctx(store).env.proc_exit(exit_code);
}

fn path_create_directory<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let dirfd = args.get(0).and_then(|v| if let Value::I32(x) = v { Some(*x as i32) } else { None }).unwrap_or(-1);
    let ptr = args.get(1).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let len = args.get(2).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    
    let mut pb = vec![0u8; len as usize];
    if read_bytes(store, ptr, &mut pb).is_err() { return Ok(vec![Value::I32(21)]); }
    let path = String::from_utf8_lossy(&pb).into_owned();
    
    match wasi_ctx(store).env.path_create_directory(dirfd, &path) {
        Ok(_) => Ok(vec![Value::I32(0)]),
        Err(e) => Ok(vec![Value::I32(e as u32)])
    }
}

fn path_remove_directory<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let dirfd = args.get(0).and_then(|v| if let Value::I32(x) = v { Some(*x as i32) } else { None }).unwrap_or(-1);
    let ptr = args.get(1).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let len = args.get(2).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    
    let mut pb = vec![0u8; len as usize];
    if read_bytes(store, ptr, &mut pb).is_err() { return Ok(vec![Value::I32(21)]); }
    let path = String::from_utf8_lossy(&pb).into_owned();
    
    match wasi_ctx(store).env.path_remove_directory(dirfd, &path) {
        Ok(_) => Ok(vec![Value::I32(0)]),
        Err(e) => Ok(vec![Value::I32(e as u32)])
    }
}

fn path_unlink_file<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let dirfd = args.get(0).and_then(|v| if let Value::I32(x) = v { Some(*x as i32) } else { None }).unwrap_or(-1);
    let ptr = args.get(1).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let len = args.get(2).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    
    let mut pb = vec![0u8; len as usize];
    if read_bytes(store, ptr, &mut pb).is_err() { return Ok(vec![Value::I32(21)]); }
    let path = String::from_utf8_lossy(&pb).into_owned();
    
    match wasi_ctx(store).env.path_unlink_file(dirfd, &path) {
        Ok(_) => Ok(vec![Value::I32(0)]),
        Err(e) => Ok(vec![Value::I32(e as u32)])
    }
}

fn path_rename<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let ofd = args.get(0).and_then(|v| if let Value::I32(x) = v { Some(*x as i32) } else { None }).unwrap_or(-1);
    let optr = args.get(1).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let olen = args.get(2).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let nfd = args.get(3).and_then(|v| if let Value::I32(x) = v { Some(*x as i32) } else { None }).unwrap_or(-1);
    let nptr = args.get(4).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let nlen = args.get(5).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    
    let mut ob = vec![0u8; olen as usize];
    let mut nb = vec![0u8; nlen as usize];
    if read_bytes(store, optr, &mut ob).is_err() || read_bytes(store, nptr, &mut nb).is_err() { return Ok(vec![Value::I32(21)]); }
    let op = String::from_utf8_lossy(&ob).into_owned();
    let np = String::from_utf8_lossy(&nb).into_owned();
    
    match wasi_ctx(store).env.path_rename(ofd, &op, nfd, &np) {
        Ok(_) => Ok(vec![Value::I32(0)]),
        Err(e) => Ok(vec![Value::I32(e as u32)])
    }
}

fn path_readlink<T: Config>(_: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> { panic!("WASI P1 stub: path_readlink"); }

fn fd_readdir<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = args.get(0).and_then(|v| if let Value::I32(x) = v { Some(*x as i32) } else { None }).unwrap_or(-1);
    let b_ptr = args.get(1).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let b_len = args.get(2).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    let ck = args.get(3).and_then(|v| if let Value::I64(x) = v { Some(*x as u64) } else { None }).unwrap_or(0);
    let u_ptr = args.get(4).and_then(|v| if let Value::I32(x) = v { Some(*x as u32) } else { None }).unwrap_or(0);
    
    match wasi_ctx(store).env.fd_readdir(fd, ck) {
        Ok(entries) => {
            let mut used = 0;
            for (name, ft, inode) in entries {
                let nb = name.as_bytes();
                let nl = nb.len();
                let es = 24 + nl;
                if (used + es) > b_len as usize {
                    used = b_len as usize; // Buffer full-ish
                    break;
                }
                let eb = b_ptr + used as u32;
                if write_u64(store, eb, 0).is_err() || // Next cookie - simplified
                   write_u64(store, eb + 8, inode).is_err() || 
                   write_u32(store, eb + 16, nl as u32).is_err() || 
                   write_bytes(store, eb + 20, &[ft, 0, 0, 0]).is_err() || 
                   write_bytes(store, eb + 24, nb).is_err() { return Ok(vec![Value::I32(28)]); }
                used += es;
            }
            if u_ptr != 0 { let _ = write_u32(store, u_ptr, used as u32); }
            Ok(vec![Value::I32(0)])
        },
        Err(e) => Ok(vec![Value::I32(e as u32)])
    }
}