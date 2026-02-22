use super::ctx::WasiCtx;
use crate::rust_alloc::{format, string::String, vec, vec::Vec};
use crate::wasm::{
    common::{
        config::Config,
        interop::Linker,
        reader::types::{FuncType, NumType, ResultType, ValType},
        value::Value,
    },
    interpreter::store::{ExternVal, HaltExecutionError, Store},
};

pub static mut RANDOM_STATE: u64 = 1574;

pub fn create_wasi_imports<T: Config>(linker: &mut Linker, store: &mut Store<'_, T>) {
    let wasi_module = String::from("wasi_snapshot_preview1");
    let mut define = |name: &str,
                      params: Vec<ValType>,
                      returns: Vec<ValType>,
                      func: for<'a> fn(
        &mut Store<'a, T>,
        Vec<Value>,
    ) -> Result<Vec<Value>, HaltExecutionError>| {
        let func_type = FuncType {
            params: ResultType { valtypes: params },
            returns: ResultType { valtypes: returns },
        };
        let func_addr = store.func_alloc_unchecked(func_type, func);
        let _ = linker.define_unchecked(
            wasi_module.clone(),
            String::from(name),
            ExternVal::Func(func_addr),
        );
    };
    let i32_t = ValType::NumType(NumType::I32);
    let i64_t = ValType::NumType(NumType::I64);
    define("args_get", vec![i32_t, i32_t], vec![i32_t], args_get);
    define(
        "args_sizes_get",
        vec![i32_t, i32_t],
        vec![i32_t],
        args_sizes_get,
    );
    define("environ_get", vec![i32_t, i32_t], vec![i32_t], environ_get);
    define(
        "environ_sizes_get",
        vec![i32_t, i32_t],
        vec![i32_t],
        environ_sizes_get,
    );
    define(
        "clock_res_get",
        vec![i32_t, i32_t],
        vec![i32_t],
        clock_res_get,
    );
    define(
        "clock_time_get",
        vec![i32_t, i64_t, i32_t],
        vec![i32_t],
        clock_time_get,
    );
    define("fd_close", vec![i32_t], vec![i32_t], fd_close);
    define(
        "fd_fdstat_get",
        vec![i32_t, i32_t],
        vec![i32_t],
        fd_fdstat_get,
    );
    define(
        "fd_filestat_get",
        vec![i32_t, i32_t],
        vec![i32_t],
        fd_filestat_get,
    );
    define(
        "fd_filestat_set_size",
        vec![i32_t, i64_t],
        vec![i32_t],
        fd_filestat_set_size,
    );
    define(
        "fd_prestat_get",
        vec![i32_t, i32_t],
        vec![i32_t],
        fd_prestat_get,
    );
    define(
        "fd_prestat_dir_name",
        vec![i32_t, i32_t, i32_t],
        vec![i32_t],
        fd_prestat_dir_name,
    );
    define(
        "fd_read",
        vec![i32_t, i32_t, i32_t, i32_t],
        vec![i32_t],
        fd_read,
    );
    define(
        "fd_seek",
        vec![i32_t, i64_t, i32_t, i32_t],
        vec![i32_t],
        fd_seek,
    );
    define("fd_tell", vec![i32_t, i32_t], vec![i32_t], fd_tell);
    define(
        "fd_write",
        vec![i32_t, i32_t, i32_t, i32_t],
        vec![i32_t],
        fd_write,
    );
    define("fd_sync", vec![i32_t], vec![i32_t], fd_sync);
    define("fd_datasync", vec![i32_t], vec![i32_t], fd_datasync);
    define(
        "fd_advise",
        vec![i32_t, i64_t, i64_t, i32_t],
        vec![i32_t],
        fd_advise,
    );
    define(
        "fd_fdstat_set_flags",
        vec![i32_t, i32_t],
        vec![i32_t],
        fd_fdstat_set_flags,
    );
    define(
        "fd_filestat_set_times",
        vec![i32_t, i64_t, i64_t, i32_t],
        vec![i32_t],
        fd_filestat_set_times,
    );
    define(
        "fd_pread",
        vec![i32_t, i32_t, i32_t, i64_t, i32_t],
        vec![i32_t],
        fd_pread,
    );
    define(
        "fd_pwrite",
        vec![i32_t, i32_t, i32_t, i64_t, i32_t],
        vec![i32_t],
        fd_pwrite,
    );
    define(
        "path_open",
        vec![
            i32_t, i32_t, i32_t, i32_t, i32_t, i64_t, i64_t, i32_t, i32_t,
        ],
        vec![i32_t],
        path_open,
    );
    define("proc_exit", vec![i32_t], vec![], proc_exit);
    define(
        "fd_readdir",
        vec![i32_t, i32_t, i32_t, i64_t, i32_t],
        vec![i32_t],
        fd_readdir,
    );
    define(
        "path_filestat_get",
        vec![i32_t, i32_t, i32_t, i32_t, i32_t],
        vec![i32_t],
        path_filestat_get,
    );
    define(
        "path_filestat_set_times",
        vec![i32_t, i32_t, i32_t, i32_t, i64_t, i64_t, i32_t],
        vec![i32_t],
        path_filestat_set_times,
    );
    define(
        "path_link",
        vec![i32_t, i32_t, i32_t, i32_t, i32_t, i32_t, i32_t],
        vec![i32_t],
        path_link,
    );
    define(
        "path_symlink",
        vec![i32_t, i32_t, i32_t, i32_t, i32_t],
        vec![i32_t],
        path_symlink,
    );
    define(
        "sock_accept",
        vec![i32_t, i32_t, i32_t],
        vec![i32_t],
        sock_accept,
    );
    define(
        "sock_recv",
        vec![i32_t, i32_t, i32_t, i32_t, i32_t, i32_t],
        vec![i32_t],
        sock_recv,
    );
    define(
        "sock_send",
        vec![i32_t, i32_t, i32_t, i32_t, i32_t],
        vec![i32_t],
        sock_send,
    );
    define(
        "sock_shutdown",
        vec![i32_t, i32_t],
        vec![i32_t],
        sock_shutdown,
    );
    define("random_get", vec![i32_t, i32_t], vec![i32_t], random_get);
    define(
        "path_create_directory",
        vec![i32_t, i32_t, i32_t],
        vec![i32_t],
        path_create_directory,
    );
    define(
        "path_remove_directory",
        vec![i32_t, i32_t, i32_t],
        vec![i32_t],
        path_remove_directory,
    );
    define(
        "path_unlink_file",
        vec![i32_t, i32_t, i32_t],
        vec![i32_t],
        path_unlink_file,
    );
    define(
        "path_rename",
        vec![i32_t, i32_t, i32_t, i32_t, i32_t, i32_t],
        vec![i32_t],
        path_rename,
    );
    define(
        "path_readlink",
        vec![i32_t, i32_t, i32_t, i32_t, i32_t, i32_t],
        vec![i32_t],
        path_readlink,
    );
    define("sched_yield", vec![], vec![i32_t], sched_yield);
    define(
        "poll_oneoff",
        vec![i32_t, i32_t, i32_t, i32_t],
        vec![i32_t],
        poll_oneoff,
    );

    // Provide env.__wasm_call_dtors and __wasi_proc_exit as it is often expected by Rust WASM modules
    let func_type = FuncType {
        params: ResultType { valtypes: vec![] },
        returns: ResultType { valtypes: vec![] },
    };
    let func_addr = store.func_alloc_unchecked(func_type, |_, _| Ok(vec![]));
    let _ = linker.define_unchecked(
        String::from("env"),
        String::from("__wasm_call_dtors"),
        ExternVal::Func(func_addr),
    );

    let exit_type = FuncType {
        params: ResultType {
            valtypes: vec![i32_t],
        },
        returns: ResultType { valtypes: vec![] },
    };
    let exit_addr = store.func_alloc_unchecked(exit_type, proc_exit_host);
    let _ = linker.define_unchecked(
        String::from("env"),
        String::from("__wasi_proc_exit"),
        ExternVal::Func(exit_addr),
    );
}

// Rename proc_exit to avoid confusion with the wrapper
fn proc_exit_host<T: Config>(
    store: &mut Store<'_, T>,
    args: Vec<Value>,
) -> Result<Vec<Value>, HaltExecutionError> {
    proc_exit(store, args)
}

// Helpers
fn wasi_ctx<'a, T: Config>(store: &'a mut Store<'_, T>) -> &'a mut WasiCtx {
    if store.wasi_ctx.is_none() {
        store.wasi_ctx = Some(WasiCtx::default());
    }
    store.wasi_ctx.as_mut().unwrap()
}

fn write_bytes<T: Config>(store: &mut Store<'_, T>, addr: u32, bytes: &[u8]) -> Result<(), ()> {
    let module_addr = store.caller_module.ok_or(())?;
    let mem_addr = *store.modules.get(module_addr).mem_addrs.get(0).ok_or(())?;
    let mem = store.memories.get(mem_addr);
    mem.mem
        .init(addr as usize, bytes, 0, bytes.len())
        .map_err(|_| ())
}
fn read_bytes<T: Config>(store: &Store<'_, T>, addr: u32, buf: &mut [u8]) -> Result<(), ()> {
    let module_addr = store.caller_module.ok_or(())?;
    let mem_addr = *store.modules.get(module_addr).mem_addrs.get(0).ok_or(())?;
    let mem = store.memories.get(mem_addr);
    mem.mem.read_slice(addr as usize, buf).map_err(|_| ())
}
fn write_u16<T: Config>(store: &mut Store<'_, T>, addr: u32, val: u16) -> Result<(), ()> {
    write_bytes(store, addr, &val.to_le_bytes())
}
fn write_u32<T: Config>(store: &mut Store<'_, T>, addr: u32, val: u32) -> Result<(), ()> {
    write_bytes(store, addr, &val.to_le_bytes())
}
fn write_u64<T: Config>(store: &mut Store<'_, T>, addr: u32, val: u64) -> Result<(), ()> {
    write_bytes(store, addr, &val.to_le_bytes())
}

// --- Implementation Wrappers ---

fn fd_fdstat_set_flags<T: Config>(
    store: &mut Store<'_, T>,
    args: Vec<Value>,
) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = args
        .get(0)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as i32)
            } else {
                None
            }
        })
        .unwrap_or(-1);
    let flags = args
        .get(1)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u16)
            } else {
                None
            }
        })
        .unwrap_or(0);
    match wasi_ctx(store).env.fd_fdstat_set_flags(fd, flags) {
        Ok(_) => Ok(vec![Value::I32(0)]),
        Err(e) => Ok(vec![Value::I32(e as u32)]),
    }
}

fn fd_filestat_set_times<T: Config>(
    store: &mut Store<'_, T>,
    args: Vec<Value>,
) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = args
        .get(0)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as i32)
            } else {
                None
            }
        })
        .unwrap_or(-1);
    let atime = args
        .get(1)
        .and_then(|v| {
            if let Value::I64(x) = v {
                Some(*x as u64)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let mtime = args
        .get(2)
        .and_then(|v| {
            if let Value::I64(x) = v {
                Some(*x as u64)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let fst_flags = args
        .get(3)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u16)
            } else {
                None
            }
        })
        .unwrap_or(0);
    match wasi_ctx(store)
        .env
        .fd_filestat_set_times(fd, atime, mtime, fst_flags)
    {
        Ok(_) => Ok(vec![Value::I32(0)]),
        Err(e) => Ok(vec![Value::I32(e as u32)]),
    }
}

fn fd_pread<T: Config>(
    store: &mut Store<'_, T>,
    args: Vec<Value>,
) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = args
        .get(0)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as i32)
            } else {
                None
            }
        })
        .unwrap_or(-1);
    let i_ptr = args
        .get(1)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let i_len = args
        .get(2)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let offset = args
        .get(3)
        .and_then(|v| {
            if let Value::I64(x) = v {
                Some(*x as u64)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let n_ptr = args
        .get(4)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);

    let mut iovs = Vec::new();
    for i in 0..i_len {
        let mut iov = [0u8; 8];
        if read_bytes(store, i_ptr + i * 8, &mut iov).is_err() {
            return Ok(vec![Value::I32(21)]);
        }
        let b_ptr = u32::from_le_bytes(iov[0..4].try_into().unwrap());
        let b_len = u32::from_le_bytes(iov[4..8].try_into().unwrap());
        iovs.push((b_ptr, b_len));
    }

    let mut buffers = Vec::new();
    for (_, len) in &iovs {
        buffers.push(vec![0u8; *len as usize]);
    }

    let mut slices: Vec<&mut [u8]> = buffers.iter_mut().map(|v| v.as_mut_slice()).collect();

    match wasi_ctx(store).env.fd_pread(fd, &mut slices, offset) {
        Ok(n) => {
            let mut remaining = n;
            for ((ptr, _), buf) in iovs.iter().zip(buffers.iter()) {
                let to_write = core::cmp::min(remaining, buf.len());
                if to_write > 0 {
                    if write_bytes(store, *ptr, &buf[..to_write]).is_err() {
                        return Ok(vec![Value::I32(28)]);
                    }
                    remaining -= to_write;
                }
            }
            if n_ptr != 0 {
                let _ = write_u32(store, n_ptr, n as u32);
            }
            Ok(vec![Value::I32(0)])
        }
        Err(e) => Ok(vec![Value::I32(e as u32)]),
    }
}

fn fd_pwrite<T: Config>(
    store: &mut Store<'_, T>,
    args: Vec<Value>,
) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = args
        .get(0)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as i32)
            } else {
                None
            }
        })
        .unwrap_or(-1);
    let i_ptr = args
        .get(1)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let i_len = args
        .get(2)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let offset = args
        .get(3)
        .and_then(|v| {
            if let Value::I64(x) = v {
                Some(*x as u64)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let n_ptr = args
        .get(4)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);

    let mut buffers = Vec::new();
    for i in 0..i_len {
        let mut iov = [0u8; 8];
        if read_bytes(store, i_ptr + i * 8, &mut iov).is_err() {
            return Ok(vec![Value::I32(21)]);
        }
        let b_ptr = u32::from_le_bytes(iov[0..4].try_into().unwrap());
        let b_len = u32::from_le_bytes(iov[4..8].try_into().unwrap());
        let mut b = vec![0u8; b_len as usize];
        if read_bytes(store, b_ptr, &mut b).is_err() {
            return Ok(vec![Value::I32(21)]);
        }
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

fn path_filestat_set_times<T: Config>(
    store: &mut Store<'_, T>,
    args: Vec<Value>,
) -> Result<Vec<Value>, HaltExecutionError> {
    let dirfd = args
        .get(0)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as i32)
            } else {
                None
            }
        })
        .unwrap_or(-1);
    let flags = args
        .get(1)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let ptr = args
        .get(2)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let len = args
        .get(3)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let atime = args
        .get(4)
        .and_then(|v| {
            if let Value::I64(x) = v {
                Some(*x as u64)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let mtime = args
        .get(5)
        .and_then(|v| {
            if let Value::I64(x) = v {
                Some(*x as u64)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let fst_flags = args
        .get(6)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u16)
            } else {
                None
            }
        })
        .unwrap_or(0);

    let mut pb = vec![0u8; len as usize];
    if read_bytes(store, ptr, &mut pb).is_err() {
        return Ok(vec![Value::I32(21)]);
    }
    let path = String::from_utf8_lossy(&pb).into_owned();

    match wasi_ctx(store)
        .env
        .path_filestat_set_times(dirfd, flags, &path, atime, mtime, fst_flags)
    {
        Ok(_) => Ok(vec![Value::I32(0)]),
        Err(e) => Ok(vec![Value::I32(e as u32)]),
    }
}

fn path_link<T: Config>(
    store: &mut Store<'_, T>,
    args: Vec<Value>,
) -> Result<Vec<Value>, HaltExecutionError> {
    let ofd = args
        .get(0)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as i32)
            } else {
                None
            }
        })
        .unwrap_or(-1);
    let oflags = args
        .get(1)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let optr = args
        .get(2)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let olen = args
        .get(3)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let nfd = args
        .get(4)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as i32)
            } else {
                None
            }
        })
        .unwrap_or(-1);
    let nptr = args
        .get(5)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let nlen = args
        .get(6)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);

    let mut ob = vec![0u8; olen as usize];
    let mut nb = vec![0u8; nlen as usize];
    if read_bytes(store, optr, &mut ob).is_err() || read_bytes(store, nptr, &mut nb).is_err() {
        return Ok(vec![Value::I32(21)]);
    }
    let op = String::from_utf8_lossy(&ob).into_owned();
    let np = String::from_utf8_lossy(&nb).into_owned();

    match wasi_ctx(store).env.path_link(ofd, oflags, &op, nfd, &np) {
        Ok(_) => Ok(vec![Value::I32(0)]),
        Err(e) => Ok(vec![Value::I32(e as u32)]),
    }
}

fn path_symlink<T: Config>(
    store: &mut Store<'_, T>,
    args: Vec<Value>,
) -> Result<Vec<Value>, HaltExecutionError> {
    let tptr = args
        .get(0)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let tlen = args
        .get(1)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let fd = args
        .get(2)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as i32)
            } else {
                None
            }
        })
        .unwrap_or(-1);
    let ptr = args
        .get(3)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let len = args
        .get(4)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);

    let mut tb = vec![0u8; tlen as usize];
    let mut pb = vec![0u8; len as usize];
    if read_bytes(store, tptr, &mut tb).is_err() || read_bytes(store, ptr, &mut pb).is_err() {
        return Ok(vec![Value::I32(21)]);
    }
    let target = String::from_utf8_lossy(&tb).into_owned();
    let path = String::from_utf8_lossy(&pb).into_owned();

    match wasi_ctx(store).env.path_symlink(&target, fd, &path) {
        Ok(_) => Ok(vec![Value::I32(0)]),
        Err(e) => Ok(vec![Value::I32(e as u32)]),
    }
}

fn sock_accept<T: Config>(
    store: &mut Store<'_, T>,
    args: Vec<Value>,
) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = args
        .get(0)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as i32)
            } else {
                None
            }
        })
        .unwrap_or(-1);
    let flags = args
        .get(1)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u16)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let ptr = args
        .get(2)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);

    match wasi_ctx(store).env.sock_accept(fd, flags) {
        Ok(new_fd) => {
            if write_u32(store, ptr, new_fd as u32).is_err() {
                return Ok(vec![Value::I32(28)]);
            }
            Ok(vec![Value::I32(0)])
        }
        Err(e) => Ok(vec![Value::I32(e as u32)]),
    }
}

fn sock_recv<T: Config>(
    store: &mut Store<'_, T>,
    args: Vec<Value>,
) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = args
        .get(0)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as i32)
            } else {
                None
            }
        })
        .unwrap_or(-1);
    let ri_data_ptr = args
        .get(1)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let ri_data_len = args
        .get(2)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let ri_flags = args
        .get(3)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u16)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let ro_datalen_ptr = args
        .get(4)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let ro_flags_ptr = args
        .get(5)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);

    let mut iovs = Vec::new();
    for i in 0..ri_data_len {
        let mut iov = [0u8; 8];
        if read_bytes(store, ri_data_ptr + i * 8, &mut iov).is_err() {
            return Ok(vec![Value::I32(21)]);
        }
        let b_ptr = u32::from_le_bytes(iov[0..4].try_into().unwrap());
        let b_len = u32::from_le_bytes(iov[4..8].try_into().unwrap());
        iovs.push((b_ptr, b_len));
    }

    let mut buffers = Vec::new();
    for (_, len) in &iovs {
        buffers.push(vec![0u8; *len as usize]);
    }

    let mut slices: Vec<&mut [u8]> = buffers.iter_mut().map(|v| v.as_mut_slice()).collect();

    match wasi_ctx(store).env.sock_recv(fd, &mut slices, ri_flags) {
        Ok((len, flags)) => {
            let mut remaining = len;
            for ((ptr, _), buf) in iovs.iter().zip(buffers.iter()) {
                let swallowed = core::cmp::min(remaining, buf.len());
                if swallowed > 0 {
                    if write_bytes(store, *ptr, &buf[..swallowed]).is_err() {
                        return Ok(vec![Value::I32(28)]);
                    }
                    remaining -= swallowed;
                }
            }
            if write_u32(store, ro_datalen_ptr, len as u32).is_err()
                || write_u16(store, ro_flags_ptr, flags).is_err()
            {
                return Ok(vec![Value::I32(28)]);
            }
            Ok(vec![Value::I32(0)])
        }
        Err(e) => Ok(vec![Value::I32(e as u32)]),
    }
}

fn sock_send<T: Config>(
    store: &mut Store<'_, T>,
    args: Vec<Value>,
) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = args
        .get(0)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as i32)
            } else {
                None
            }
        })
        .unwrap_or(-1);
    let si_data_ptr = args
        .get(1)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let si_data_len = args
        .get(2)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let si_flags = args
        .get(3)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u16)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let so_datalen_ptr = args
        .get(4)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);

    let mut buffers = Vec::new();
    for i in 0..si_data_len {
        let mut iov = [0u8; 8];
        if read_bytes(store, si_data_ptr + i * 8, &mut iov).is_err() {
            return Ok(vec![Value::I32(21)]);
        }
        let b_ptr = u32::from_le_bytes(iov[0..4].try_into().unwrap());
        let b_len = u32::from_le_bytes(iov[4..8].try_into().unwrap());
        let mut b = vec![0u8; b_len as usize];
        if read_bytes(store, b_ptr, &mut b).is_err() {
            return Ok(vec![Value::I32(21)]);
        }
        buffers.push(b);
    }

    let slices: Vec<&[u8]> = buffers.iter().map(|v| v.as_slice()).collect();

    match wasi_ctx(store).env.sock_send(fd, &slices, si_flags) {
        Ok(len) => {
            if write_u32(store, so_datalen_ptr, len as u32).is_err() {
                return Ok(vec![Value::I32(28)]);
            }
            Ok(vec![Value::I32(0)])
        }
        Err(e) => Ok(vec![Value::I32(e as u32)]),
    }
}

fn sock_shutdown<T: Config>(
    store: &mut Store<'_, T>,
    args: Vec<Value>,
) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = args
        .get(0)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as i32)
            } else {
                None
            }
        })
        .unwrap_or(-1);
    let how = args
        .get(1)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u8)
            } else {
                None
            }
        })
        .unwrap_or(0);
    match wasi_ctx(store).env.sock_shutdown(fd, how) {
        Ok(_) => Ok(vec![Value::I32(0)]),
        Err(e) => Ok(vec![Value::I32(e as u32)]),
    }
}

fn args_get<T: Config>(
    store: &mut Store<'_, T>,
    args: Vec<Value>,
) -> Result<Vec<Value>, HaltExecutionError> {
    let argv_ptr = args
        .get(0)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let argv_buf_ptr = args
        .get(1)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);

    let env_args = wasi_ctx(store).env.args_get().unwrap_or_default();

    let mut offset = 0;
    for (i, arg) in env_args.iter().enumerate() {
        let p = argv_buf_ptr + offset;
        if write_u32(store, argv_ptr + (i as u32 * 4), p).is_err() {
            return Ok(vec![Value::I32(28)]);
        }
        let b = arg.as_bytes();
        if write_bytes(store, p, b).is_err()
            || write_bytes(store, p + b.len() as u32, &[0]).is_err()
        {
            return Ok(vec![Value::I32(28)]);
        }
        offset += b.len() as u32 + 1;
    }
    Ok(vec![Value::I32(0)])
}

fn args_sizes_get<T: Config>(
    store: &mut Store<'_, T>,
    args: Vec<Value>,
) -> Result<Vec<Value>, HaltExecutionError> {
    let c_ptr = args
        .get(0)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let b_ptr = args
        .get(1)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);

    let env_args = wasi_ctx(store).env.args_get().unwrap_or_default();

    if write_u32(store, c_ptr, env_args.len() as u32).is_err()
        || write_u32(
            store,
            b_ptr,
            env_args.iter().map(|s| s.len() + 1).sum::<usize>() as u32,
        )
        .is_err()
    {
        return Ok(vec![Value::I32(28)]);
    }
    Ok(vec![Value::I32(0)])
}

fn environ_get<T: Config>(
    store: &mut Store<'_, T>,
    args: Vec<Value>,
) -> Result<Vec<Value>, HaltExecutionError> {
    let e_ptr = args
        .get(0)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let b_ptr = args
        .get(1)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);

    let env_vars = wasi_ctx(store).env.environ_get().unwrap_or_default();

    let mut offset = 0;
    for (i, (k, v)) in env_vars.iter().enumerate() {
        let entry = format!("{}={}", k, v);
        let p = b_ptr + offset;
        if write_u32(store, e_ptr + (i as u32 * 4), p).is_err() {
            return Ok(vec![Value::I32(28)]);
        }
        let b = entry.as_bytes();
        if write_bytes(store, p, b).is_err()
            || write_bytes(store, p + b.len() as u32, &[0]).is_err()
        {
            return Ok(vec![Value::I32(28)]);
        }
        offset += b.len() as u32 + 1;
    }
    Ok(vec![Value::I32(0)])
}

fn environ_sizes_get<T: Config>(
    store: &mut Store<'_, T>,
    args: Vec<Value>,
) -> Result<Vec<Value>, HaltExecutionError> {
    let c_ptr = args
        .get(0)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let b_ptr = args
        .get(1)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);

    let env_vars = wasi_ctx(store).env.environ_get().unwrap_or_default();

    if write_u32(store, c_ptr, env_vars.len() as u32).is_err()
        || write_u32(
            store,
            b_ptr,
            env_vars
                .iter()
                .map(|(k, v)| k.len() + v.len() + 2)
                .sum::<usize>() as u32,
        )
        .is_err()
    {
        return Ok(vec![Value::I32(28)]);
    }
    Ok(vec![Value::I32(0)])
}

fn clock_res_get<T: Config>(
    store: &mut Store<'_, T>,
    args: Vec<Value>,
) -> Result<Vec<Value>, HaltExecutionError> {
    let id = args
        .get(0)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let r_ptr = args
        .get(1)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);

    match wasi_ctx(store).env.clock_res_get(id) {
        Ok(res) => {
            let _ = write_u64(store, r_ptr, res);
            Ok(vec![Value::I32(0)])
        }
        Err(e) => Ok(vec![Value::I32(e as u32)]),
    }
}

fn clock_time_get<T: Config>(
    store: &mut Store<'_, T>,
    args: Vec<Value>,
) -> Result<Vec<Value>, HaltExecutionError> {
    let id = args
        .get(0)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let precision = args
        .get(1)
        .and_then(|v| {
            if let Value::I64(x) = v {
                Some(*x as u64)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let t_ptr = args
        .get(2)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);

    match wasi_ctx(store).env.clock_time_get(id, precision) {
        Ok(t) => {
            let _ = write_u64(store, t_ptr, t);
            Ok(vec![Value::I32(0)])
        }
        Err(e) => Ok(vec![Value::I32(e as u32)]),
    }
}

fn fd_close<T: Config>(
    store: &mut Store<'_, T>,
    args: Vec<Value>,
) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = args
        .get(0)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as i32)
            } else {
                None
            }
        })
        .unwrap_or(-1);
    match wasi_ctx(store).env.fd_close(fd) {
        Ok(_) => Ok(vec![Value::I32(0)]),
        Err(e) => Ok(vec![Value::I32(e as u32)]),
    }
}

fn fd_fdstat_get<T: Config>(
    store: &mut Store<'_, T>,
    args: Vec<Value>,
) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = args
        .get(0)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as i32)
            } else {
                None
            }
        })
        .unwrap_or(-1);
    let s_ptr = args
        .get(1)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);

    match wasi_ctx(store).env.fd_fdstat_get(fd) {
        Ok(s) => {
            if write_u16(store, s_ptr, s.filetype as u16).is_err()
                || write_u16(store, s_ptr + 2, s.flags).is_err()
                || write_u64(store, s_ptr + 8, s.rights_base).is_err()
                || write_u64(store, s_ptr + 16, s.rights_inheriting).is_err()
            {
                return Ok(vec![Value::I32(28)]);
            }
            Ok(vec![Value::I32(0)])
        }
        Err(e) => Ok(vec![Value::I32(e as u32)]),
    }
}

fn fd_filestat_get<T: Config>(
    store: &mut Store<'_, T>,
    args: Vec<Value>,
) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = args
        .get(0)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as i32)
            } else {
                None
            }
        })
        .unwrap_or(-1);
    let b_ptr = args
        .get(1)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);

    match wasi_ctx(store).env.fd_filestat_get(fd) {
        Ok(s) => {
            if write_u64(store, b_ptr, s.dev).is_err()
                || write_u64(store, b_ptr + 8, s.ino).is_err()
                || write_bytes(store, b_ptr + 16, &[s.filetype]).is_err()
                || write_u64(store, b_ptr + 24, s.nlink).is_err()
                || write_u64(store, b_ptr + 32, s.size).is_err()
                || write_u64(store, b_ptr + 40, s.atime).is_err()
                || write_u64(store, b_ptr + 48, s.mtime).is_err()
                || write_u64(store, b_ptr + 56, s.ctime).is_err()
            {
                return Ok(vec![Value::I32(28)]);
            }
            Ok(vec![Value::I32(0)])
        }
        Err(e) => Ok(vec![Value::I32(e as u32)]),
    }
}

fn fd_filestat_set_size<T: Config>(
    store: &mut Store<'_, T>,
    args: Vec<Value>,
) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = args
        .get(0)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as i32)
            } else {
                None
            }
        })
        .unwrap_or(-1);
    let sz = args
        .get(1)
        .and_then(|v| {
            if let Value::I64(x) = v {
                Some(*x as u64)
            } else {
                None
            }
        })
        .unwrap_or(0);
    match wasi_ctx(store).env.fd_filestat_set_size(fd, sz) {
        Ok(_) => Ok(vec![Value::I32(0)]),
        Err(e) => Ok(vec![Value::I32(e as u32)]),
    }
}

fn fd_prestat_get<T: Config>(
    store: &mut Store<'_, T>,
    args: Vec<Value>,
) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = args
        .get(0)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as i32)
            } else {
                None
            }
        })
        .unwrap_or(-1);
    let ptr = args
        .get(1)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);

    match wasi_ctx(store).env.fd_prestat_get(fd) {
        Ok(t) => {
            let name_len = wasi_ctx(store)
                .env
                .fd_prestat_dir_name(fd)
                .map(|s| s.len() as u32)
                .unwrap_or(0);
            if write_bytes(store, ptr, &[t as u8]).is_err()
                || write_u32(store, ptr + 4, name_len).is_err()
            {
                return Ok(vec![Value::I32(28)]);
            }
            Ok(vec![Value::I32(0)])
        }
        Err(e) => Ok(vec![Value::I32(e as u32)]),
    }
}

fn fd_prestat_dir_name<T: Config>(
    store: &mut Store<'_, T>,
    args: Vec<Value>,
) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = args
        .get(0)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as i32)
            } else {
                None
            }
        })
        .unwrap_or(-1);
    let ptr = args
        .get(1)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let _len = args
        .get(2)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);

    match wasi_ctx(store).env.fd_prestat_dir_name(fd) {
        Ok(s) => {
            if write_bytes(store, ptr, s.as_bytes()).is_err() {
                return Ok(vec![Value::I32(28)]);
            }
            Ok(vec![Value::I32(0)])
        }
        Err(e) => Ok(vec![Value::I32(e as u32)]),
    }
}

fn fd_read<T: Config>(
    store: &mut Store<'_, T>,
    args: Vec<Value>,
) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = args
        .get(0)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as i32)
            } else {
                None
            }
        })
        .unwrap_or(-1);
    let i_ptr = args
        .get(1)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let i_len = args
        .get(2)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let n_ptr = args
        .get(3)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);

    let mut iovs = Vec::new();
    for i in 0..i_len {
        let mut iov = [0u8; 8];
        if read_bytes(store, i_ptr + i * 8, &mut iov).is_err() {
            return Ok(vec![Value::I32(21)]);
        }
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

    let wasi = wasi_ctx(store);
    let stdio_map = wasi.env.stdio_map();
    let actual_fd = if fd >= 0 && fd <= 2 {
        stdio_map[fd as usize]
    } else {
        fd
    };

    match wasi.env.fd_read(actual_fd, &mut slices) {
        Ok(n) => {
            // Write back
            let mut remaining = n;
            for ((ptr, _), buf) in iovs.iter().zip(buffers.iter()) {
                let to_write = core::cmp::min(remaining, buf.len());
                if to_write > 0 {
                    if write_bytes(store, *ptr, &buf[..to_write]).is_err() {
                        return Ok(vec![Value::I32(28)]);
                    }
                    remaining -= to_write;
                }
            }
            if n_ptr != 0 {
                let _ = write_u32(store, n_ptr, n as u32);
            }
            Ok(vec![Value::I32(0)])
        }
        Err(e) => Ok(vec![Value::I32(e as u32)]),
    }
}

fn fd_seek<T: Config>(
    store: &mut Store<'_, T>,
    args: Vec<Value>,
) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = args
        .get(0)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as i32)
            } else {
                None
            }
        })
        .unwrap_or(-1);
    let off = args
        .get(1)
        .and_then(|v| {
            if let Value::I64(x) = v {
                Some(*x as i64)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let wh = args
        .get(2)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0) as u8;
    let n_ptr = args
        .get(3)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);

    match wasi_ctx(store).env.fd_seek(fd, off, wh) {
        Ok(n) => {
            if n_ptr != 0 {
                let _ = write_u64(store, n_ptr, n);
            }
            Ok(vec![Value::I32(0)])
        }
        Err(e) => Ok(vec![Value::I32(e as u32)]),
    }
}

fn fd_tell<T: Config>(
    store: &mut Store<'_, T>,
    args: Vec<Value>,
) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = args
        .get(0)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as i32)
            } else {
                None
            }
        })
        .unwrap_or(-1);
    let p_ptr = args
        .get(1)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);

    match wasi_ctx(store).env.fd_tell(fd) {
        Ok(n) => {
            if p_ptr != 0 {
                let _ = write_u64(store, p_ptr, n);
            }
            Ok(vec![Value::I32(0)])
        }
        Err(e) => Ok(vec![Value::I32(e as u32)]),
    }
}

fn fd_write<T: Config>(
    store: &mut Store<'_, T>,
    args: Vec<Value>,
) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = args
        .get(0)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as i32)
            } else {
                None
            }
        })
        .unwrap_or(-1);
    let i_ptr = args
        .get(1)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let i_len = args
        .get(2)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let n_ptr = args
        .get(3)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);

    let mut buffers = Vec::new();
    for i in 0..i_len {
        let mut iov = [0u8; 8];
        if read_bytes(store, i_ptr + i * 8, &mut iov).is_err() {
            return Ok(vec![Value::I32(21)]);
        }
        let b_ptr = u32::from_le_bytes(iov[0..4].try_into().unwrap());
        let b_len = u32::from_le_bytes(iov[4..8].try_into().unwrap());
        let mut b = vec![0u8; b_len as usize];
        if read_bytes(store, b_ptr, &mut b).is_err() {
            return Ok(vec![Value::I32(21)]);
        }
        buffers.push(b);
    }

    let slices: Vec<&[u8]> = buffers.iter().map(|v| v.as_slice()).collect();

    let wasi = wasi_ctx(store);
    let stdio_map = wasi.env.stdio_map();

    // Check for stdio mapping
    let actual_fd = if fd >= 0 && fd <= 2 {
        stdio_map[fd as usize]
    } else {
        fd
    };

    match wasi.env.fd_write(actual_fd, &slices) {
        Ok(n) => {
            let _ = write_u32(store, n_ptr, n as u32);
            Ok(vec![Value::I32(0)])
        }
        Err(e) => Ok(vec![Value::I32(e as u32)]),
    }
}

fn fd_sync<T: Config>(
    store: &mut Store<'_, T>,
    args: Vec<Value>,
) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = args
        .get(0)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as i32)
            } else {
                None
            }
        })
        .unwrap_or(-1);
    match wasi_ctx(store).env.fd_sync(fd) {
        Ok(_) => Ok(vec![Value::I32(0)]),
        Err(e) => Ok(vec![Value::I32(e as u32)]),
    }
}

fn fd_datasync<T: Config>(
    store: &mut Store<'_, T>,
    args: Vec<Value>,
) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = args
        .get(0)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as i32)
            } else {
                None
            }
        })
        .unwrap_or(-1);
    match wasi_ctx(store).env.fd_datasync(fd) {
        Ok(_) => Ok(vec![Value::I32(0)]),
        Err(e) => Ok(vec![Value::I32(e as u32)]),
    }
}

fn fd_advise<T: Config>(
    store: &mut Store<'_, T>,
    args: Vec<Value>,
) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = args
        .get(0)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as i32)
            } else {
                None
            }
        })
        .unwrap_or(-1);
    let offset = args
        .get(1)
        .and_then(|v| {
            if let Value::I64(x) = v {
                Some(*x as u64)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let len = args
        .get(2)
        .and_then(|v| {
            if let Value::I64(x) = v {
                Some(*x as u64)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let advice = args
        .get(3)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u8)
            } else {
                None
            }
        })
        .unwrap_or(0);

    match wasi_ctx(store).env.fd_advise(fd, offset, len, advice) {
        Ok(_) => Ok(vec![Value::I32(0)]),
        Err(e) => Ok(vec![Value::I32(e as u32)]),
    }
}

fn path_open<T: Config>(
    store: &mut Store<'_, T>,
    args: Vec<Value>,
) -> Result<Vec<Value>, HaltExecutionError> {
    let dirfd = args
        .get(0)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as i32)
            } else {
                None
            }
        })
        .unwrap_or(-1);
    let dirflags = args
        .get(1)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let ptr = args
        .get(2)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let len = args
        .get(3)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let of = args
        .get(4)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let rights_base = args
        .get(5)
        .and_then(|v| {
            if let Value::I64(x) = v {
                Some(*x as u64)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let rights_inh = args
        .get(6)
        .and_then(|v| {
            if let Value::I64(x) = v {
                Some(*x as u64)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let fdflags = args
        .get(7)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u16)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let f_ptr = args
        .get(8)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);

    let mut pb = vec![0u8; len as usize];
    if read_bytes(store, ptr, &mut pb).is_err() {
        return Ok(vec![Value::I32(21)]);
    }
    let path = String::from_utf8_lossy(&pb).into_owned();

    match wasi_ctx(store).env.path_open(
        dirfd,
        dirflags,
        &path,
        of,
        rights_base,
        rights_inh,
        fdflags,
    ) {
        Ok(fd) => {
            if f_ptr != 0 {
                let _ = write_u32(store, f_ptr, fd as u32);
            }
            Ok(vec![Value::I32(0)])
        }
        Err(e) => Ok(vec![Value::I32(e as u32)]),
    }
}

fn path_filestat_get<T: Config>(
    store: &mut Store<'_, T>,
    args: Vec<Value>,
) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = args
        .get(0)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as i32)
            } else {
                None
            }
        })
        .unwrap_or(-1);
    let flags = args
        .get(1)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let ptr = args
        .get(2)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let len = args
        .get(3)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let b_ptr = args
        .get(4)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);

    let mut pb = vec![0u8; len as usize];
    if read_bytes(store, ptr, &mut pb).is_err() {
        return Ok(vec![Value::I32(21)]);
    }
    let path = String::from_utf8_lossy(&pb).into_owned();

    match wasi_ctx(store).env.path_filestat_get(fd, flags, &path) {
        Ok(s) => {
            if write_u64(store, b_ptr, s.dev).is_err()
                || write_u64(store, b_ptr + 8, s.ino).is_err()
                || write_bytes(store, b_ptr + 16, &[s.filetype]).is_err()
                || write_u64(store, b_ptr + 24, s.nlink).is_err()
                || write_u64(store, b_ptr + 32, s.size).is_err()
                || write_u64(store, b_ptr + 40, s.atime).is_err()
                || write_u64(store, b_ptr + 48, s.mtime).is_err()
                || write_u64(store, b_ptr + 56, s.ctime).is_err()
            {
                return Ok(vec![Value::I32(28)]);
            }
            Ok(vec![Value::I32(0)])
        }
        Err(e) => Ok(vec![Value::I32(e as u32)]),
    }
}

fn random_get<T: Config>(
    store: &mut Store<'_, T>,
    args: Vec<Value>,
) -> Result<Vec<Value>, HaltExecutionError> {
    let ptr = args
        .get(0)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let len = args
        .get(1)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);

    let mut b = vec![0u8; len as usize];
    match wasi_ctx(store).env.random_get(&mut b) {
        Ok(_) => {
            if write_bytes(store, ptr, &b).is_err() {
                return Ok(vec![Value::I32(28)]);
            }
            Ok(vec![Value::I32(0)])
        }
        Err(e) => Ok(vec![Value::I32(e as u32)]),
    }
}

fn sched_yield<T: Config>(
    store: &mut Store<'_, T>,
    _: Vec<Value>,
) -> Result<Vec<Value>, HaltExecutionError> {
    match wasi_ctx(store).env.sched_yield() {
        Ok(_) => Ok(vec![Value::I32(0)]),
        Err(e) => Ok(vec![Value::I32(e as u32)]),
    }
}

fn poll_oneoff<T: Config>(
    store: &mut Store<'_, T>,
    args: Vec<Value>,
) -> Result<Vec<Value>, HaltExecutionError> {
    let in_p = args
        .get(0)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let out_p = args
        .get(1)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let nsub = args
        .get(2)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let nev_p = args
        .get(3)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);

    // Read input events manually as it's complex structure
    let mut in_events = vec![0u8; (nsub * 48) as usize];
    if read_bytes(store, in_p, &mut in_events).is_err() {
        return Ok(vec![Value::I32(21)]);
    }

    // We pass raw bytes to env for now as parsing Subscription struct is tedious here
    // Ideally WasiEnv should take a list of Subscription structs
    // For this refactor I'll keep it raw or simple

    let mut out_events = vec![0u8; (nsub * 32) as usize];

    match wasi_ctx(store)
        .env
        .poll_oneoff(&in_events, &mut out_events, nsub)
    {
        Ok(nev) => {
            if write_bytes(store, out_p, &out_events[0..(nev as usize * 32)]).is_err() {
                return Ok(vec![Value::I32(28)]);
            }
            if write_u32(store, nev_p, nev).is_err() {
                return Ok(vec![Value::I32(28)]);
            }
            Ok(vec![Value::I32(0)])
        }
        Err(e) => Ok(vec![Value::I32(e as u32)]),
    }
}

fn proc_exit<T: Config>(
    store: &mut Store<'_, T>,
    args: Vec<Value>,
) -> Result<Vec<Value>, HaltExecutionError> {
    let exit_code = args
        .get(0)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as i32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    // KrakeosWasiEnv::proc_exit returns Err(code) on exit
    match wasi_ctx(store).env.proc_exit(exit_code) {
        Ok(_) => Ok(vec![]),
        Err(code) => Err(HaltExecutionError(code)),
    }
}

fn path_create_directory<T: Config>(
    store: &mut Store<'_, T>,
    args: Vec<Value>,
) -> Result<Vec<Value>, HaltExecutionError> {
    let dirfd = args
        .get(0)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as i32)
            } else {
                None
            }
        })
        .unwrap_or(-1);
    let ptr = args
        .get(1)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let len = args
        .get(2)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);

    let mut pb = vec![0u8; len as usize];
    if read_bytes(store, ptr, &mut pb).is_err() {
        return Ok(vec![Value::I32(21)]);
    }
    let path = String::from_utf8_lossy(&pb).into_owned();

    match wasi_ctx(store).env.path_create_directory(dirfd, &path) {
        Ok(_) => Ok(vec![Value::I32(0)]),
        Err(e) => Ok(vec![Value::I32(e as u32)]),
    }
}

fn path_remove_directory<T: Config>(
    store: &mut Store<'_, T>,
    args: Vec<Value>,
) -> Result<Vec<Value>, HaltExecutionError> {
    let dirfd = args
        .get(0)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as i32)
            } else {
                None
            }
        })
        .unwrap_or(-1);
    let ptr = args
        .get(1)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let len = args
        .get(2)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);

    let mut pb = vec![0u8; len as usize];
    if read_bytes(store, ptr, &mut pb).is_err() {
        return Ok(vec![Value::I32(21)]);
    }
    let path = String::from_utf8_lossy(&pb).into_owned();

    match wasi_ctx(store).env.path_remove_directory(dirfd, &path) {
        Ok(_) => Ok(vec![Value::I32(0)]),
        Err(e) => Ok(vec![Value::I32(e as u32)]),
    }
}

fn path_unlink_file<T: Config>(
    store: &mut Store<'_, T>,
    args: Vec<Value>,
) -> Result<Vec<Value>, HaltExecutionError> {
    let dirfd = args
        .get(0)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as i32)
            } else {
                None
            }
        })
        .unwrap_or(-1);
    let ptr = args
        .get(1)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let len = args
        .get(2)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);

    let mut pb = vec![0u8; len as usize];
    if read_bytes(store, ptr, &mut pb).is_err() {
        return Ok(vec![Value::I32(21)]);
    }
    let path = String::from_utf8_lossy(&pb).into_owned();

    match wasi_ctx(store).env.path_unlink_file(dirfd, &path) {
        Ok(_) => Ok(vec![Value::I32(0)]),
        Err(e) => Ok(vec![Value::I32(e as u32)]),
    }
}

fn path_rename<T: Config>(
    store: &mut Store<'_, T>,
    args: Vec<Value>,
) -> Result<Vec<Value>, HaltExecutionError> {
    let ofd = args
        .get(0)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as i32)
            } else {
                None
            }
        })
        .unwrap_or(-1);
    let optr = args
        .get(1)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let olen = args
        .get(2)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let nfd = args
        .get(3)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as i32)
            } else {
                None
            }
        })
        .unwrap_or(-1);
    let nptr = args
        .get(4)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let nlen = args
        .get(5)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);

    let mut ob = vec![0u8; olen as usize];
    let mut nb = vec![0u8; nlen as usize];
    if read_bytes(store, optr, &mut ob).is_err() || read_bytes(store, nptr, &mut nb).is_err() {
        return Ok(vec![Value::I32(21)]);
    }
    let op = String::from_utf8_lossy(&ob).into_owned();
    let np = String::from_utf8_lossy(&nb).into_owned();

    match wasi_ctx(store).env.path_rename(ofd, &op, nfd, &np) {
        Ok(_) => Ok(vec![Value::I32(0)]),
        Err(e) => Ok(vec![Value::I32(e as u32)]),
    }
}

fn path_readlink<T: Config>(
    store: &mut Store<'_, T>,
    args: Vec<Value>,
) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = args
        .get(0)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as i32)
            } else {
                None
            }
        })
        .unwrap_or(-1);
    let p_ptr = args
        .get(1)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let p_len = args
        .get(2)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let b_ptr = args
        .get(3)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let b_len = args
        .get(4)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let n_ptr = args
        .get(5)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);

    let mut pb = vec![0u8; p_len as usize];
    if read_bytes(store, p_ptr, &mut pb).is_err() {
        return Ok(vec![Value::I32(21)]);
    }
    let path = String::from_utf8_lossy(&pb).into_owned();

    let mut buf = vec![0u8; b_len as usize];
    let res = match wasi_ctx(store).env.path_readlink(fd, &path, &mut buf) {
        Ok(n) => {
            if write_bytes(store, b_ptr, &buf[..n]).is_err() {
                return Ok(vec![Value::I32(28)]);
            }
            if write_u32(store, n_ptr, n as u32).is_err() {
                return Ok(vec![Value::I32(28)]);
            }
            Ok(vec![Value::I32(0)])
        }
        Err(e) => Ok(vec![Value::I32(e as u32)]),
    };
    res
}

fn fd_readdir<T: Config>(
    store: &mut Store<'_, T>,
    args: Vec<Value>,
) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = args
        .get(0)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as i32)
            } else {
                None
            }
        })
        .unwrap_or(-1);
    let b_ptr = args
        .get(1)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let b_len = args
        .get(2)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let ck = args
        .get(3)
        .and_then(|v| {
            if let Value::I64(x) = v {
                Some(*x as u64)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let u_ptr = args
        .get(4)
        .and_then(|v| {
            if let Value::I32(x) = v {
                Some(*x as u32)
            } else {
                None
            }
        })
        .unwrap_or(0);

    match wasi_ctx(store).env.fd_readdir(fd, ck) {
        Ok(entries) => {
            let mut used: usize = 0;
            let buf_max = b_len as usize;

            for (name, ft, cookie) in entries {
                let nb = name.as_bytes();
                let nl = nb.len();
                // WASI dirent: 8 (d_next) + 8 (d_ino) + 4 (d_namlen) + 4 (d_type+pad) + name
                let entry_size = 24 + nl;

                if used >= buf_max {
                    break;
                }

                // Serialize the dirent header (24 bytes)
                let header = {
                    let mut h = [0u8; 24];
                    h[0..8].copy_from_slice(&cookie.to_le_bytes()); // d_next
                    h[8..16].copy_from_slice(&cookie.to_le_bytes()); // d_ino (use cookie as pseudo-inode)
                    h[16..20].copy_from_slice(&(nl as u32).to_le_bytes()); // d_namlen
                    h[20] = ft; // d_type
                    // h[21..24] = padding zeros
                    h
                };

                // Write as much of this entry as fits in the buffer
                // (WASI spec: partial entries are allowed to fill the buffer)
                let full_entry_bytes: Vec<u8> = {
                    let mut v = Vec::with_capacity(entry_size);
                    v.extend_from_slice(&header);
                    v.extend_from_slice(nb);
                    v
                };

                let remaining_space = buf_max - used;
                let to_write = core::cmp::min(full_entry_bytes.len(), remaining_space);

                if to_write > 0 {
                    let eb = b_ptr + used as u32;
                    if write_bytes(store, eb, &full_entry_bytes[..to_write]).is_err() {
                        return Ok(vec![Value::I32(28)]);
                    }
                    used += to_write;
                }

                if to_write < entry_size {
                    // We wrote a partial entry — buffer is full
                    break;
                }
            }

            if u_ptr != 0 {
                let _ = write_u32(store, u_ptr, used as u32);
            }
            Ok(vec![Value::I32(0)])
        }
        Err(e) => Ok(vec![Value::I32(e as u32)]),
    }
}
