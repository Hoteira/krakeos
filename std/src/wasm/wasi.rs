use crate::fs;
use crate::rust_alloc::{collections::BTreeMap, format, string::String, vec, vec::Vec};
use crate::sync::Mutex;
use crate::wasm::{
    core::reader::types::{FuncType, NumType, ResultType, ValType},
    execution::{
        config::Config,
        linker::Linker,
        store::{ExternVal, HaltExecutionError, Store},
        value::Value,
    },
};
use crate::{debug_print, debugln};

struct WasiFile {
    file: fs::File,
    _path: String,
}

static FD_TABLE: Mutex<BTreeMap<i32, WasiFile>> = Mutex::new(BTreeMap::new());
static NEXT_FD: Mutex<i32> = Mutex::new(10);
static mut RANDOM_STATE: u64 = 0;

pub fn create_wasi_imports<T: Config>(linker: &mut Linker, store: &mut Store<'_, T>) {
    let wasi_module = String::from("wasi_snapshot_preview1");

    let mut define = |name: &str, params: Vec<ValType>, returns: Vec<ValType>, func: for<'a> fn(&mut Store<'a, T>, Vec<Value>) -> Result<Vec<Value>, HaltExecutionError>| {
        let func_type = FuncType {
            params: ResultType { valtypes: params },
            returns: ResultType { valtypes: returns },
        };
        let func_addr = store.func_alloc_unchecked(func_type, func);
        if let Err(e) = linker.define_unchecked(wasi_module.clone(), String::from(name), ExternVal::Func(func_addr)) {
            debugln!("WASI: Failed to define {}: {:?}", name, e);
        }
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
    define("fd_prestat_get", vec![i32_t, i32_t], vec![i32_t], fd_prestat_get);
    define("fd_prestat_dir_name", vec![i32_t, i32_t, i32_t], vec![i32_t], fd_prestat_dir_name);
    define("fd_read", vec![i32_t, i32_t, i32_t, i32_t], vec![i32_t], fd_read);
    define("fd_seek", vec![i32_t, i64_t, i32_t, i32_t], vec![i32_t], fd_seek);
    define("fd_tell", vec![i32_t, i32_t], vec![i32_t], fd_tell);
    define("fd_write", vec![i32_t, i32_t, i32_t, i32_t], vec![i32_t], fd_write);
    define("path_open", vec![i32_t, i32_t, i32_t, i32_t, i32_t, i64_t, i64_t, i32_t, i32_t], vec![i32_t], path_open);
    define("proc_exit", vec![i32_t], vec![], proc_exit);
    define("fd_readdir", vec![i32_t, i32_t, i32_t, i64_t, i32_t], vec![i32_t], fd_readdir);
    define("path_filestat_get", vec![i32_t, i32_t, i32_t, i32_t, i32_t], vec![i32_t], path_filestat_get);
    define("random_get", vec![i32_t, i32_t], vec![i32_t], random_get);
    define("path_create_directory", vec![i32_t, i32_t, i32_t], vec![i32_t], path_create_directory);
    define("path_remove_directory", vec![i32_t, i32_t, i32_t], vec![i32_t], path_remove_directory);
    define("path_unlink_file", vec![i32_t, i32_t, i32_t], vec![i32_t], path_unlink_file);
    define("path_rename", vec![i32_t, i32_t, i32_t, i32_t, i32_t, i32_t], vec![i32_t], path_rename);
    define("sched_yield", vec![], vec![i32_t], sched_yield);
}

fn write_u32<T: Config>(store: &mut Store<'_, T>, addr: u32, val: u32) -> Result<(), ()> {
    let mem_addr = if let Some(addr) = store.memories.iter().next() { addr } else { return Err(()); };
    let mem = store.memories.get(mem_addr);
    mem.mem.store::<4, u32>(addr as usize, val).map_err(|_| ())
}

fn write_u64<T: Config>(store: &mut Store<'_, T>, addr: u32, val: u64) -> Result<(), ()> {
    let mem_addr = if let Some(addr) = store.memories.iter().next() { addr } else { return Err(()); };
    let mem = store.memories.get(mem_addr);
    mem.mem.store::<8, u64>(addr as usize, val).map_err(|_| ())
}

fn write_bytes<T: Config>(store: &mut Store<'_, T>, addr: u32, bytes: &[u8]) -> Result<(), ()> {
    let mem_addr = if let Some(addr) = store.memories.iter().next() { addr } else { return Err(()); };
    let mem = store.memories.get(mem_addr);
    for (i, &b) in bytes.iter().enumerate() {
        mem.mem.store::<1, u8>(addr as usize + i, b).map_err(|_| ())?;
    }
    Ok(())
}

fn args_get<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let argv_ptr = match args.get(0) {
        Some(Value::I32(v)) => *v as u32,
        _ => return Ok(vec![Value::I32(28)])
    };
    let argv_buf_ptr = match args.get(1) {
        Some(Value::I32(v)) => *v as u32,
        _ => return Ok(vec![Value::I32(28)])
    };

    let args_vec: Vec<String> = crate::env::args().collect();
    let mut current_buf_offset = 0;

    for (i, arg) in args_vec.iter().enumerate() {
        let ptr_to_write = argv_buf_ptr + current_buf_offset;
        if write_u32(store, argv_ptr + (i as u32 * 4), ptr_to_write).is_err() {
            return Ok(vec![Value::I32(28)]);
        }

        let bytes = arg.as_bytes();
        if write_bytes(store, ptr_to_write, bytes).is_err() { return Ok(vec![Value::I32(28)]); }
        if write_bytes(store, ptr_to_write + bytes.len() as u32, &[0]).is_err() { return Ok(vec![Value::I32(28)]); }

        current_buf_offset += bytes.len() as u32 + 1;
    }

    Ok(vec![Value::I32(0)])
}

fn args_sizes_get<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let argc_ptr = match args.get(0) {
        Some(Value::I32(v)) => *v as u32,
        _ => return Ok(vec![Value::I32(28)])
    };
    let buf_ptr = match args.get(1) {
        Some(Value::I32(v)) => *v as u32,
        _ => return Ok(vec![Value::I32(28)])
    };

    let args_vec: Vec<String> = crate::env::args().collect();
    let count = args_vec.len();
    let total_size: usize = args_vec.iter().map(|s| s.len() + 1).sum();

    if write_u32(store, argc_ptr, count as u32).is_err() { return Ok(vec![Value::I32(28)]); }
    if write_u32(store, buf_ptr, total_size as u32).is_err() { return Ok(vec![Value::I32(28)]); }

    Ok(vec![Value::I32(0)])
}

fn environ_get<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let env_ptr = match args.get(0) {
        Some(Value::I32(v)) => *v as u32,
        _ => return Ok(vec![Value::I32(28)])
    };
    let env_buf_ptr = match args.get(1) {
        Some(Value::I32(v)) => *v as u32,
        _ => return Ok(vec![Value::I32(28)])
    };

    let env_vars: Vec<(String, String)> = crate::env::vars().collect();
    let mut current_buf_offset = 0;

    for (i, (key, value)) in env_vars.iter().enumerate() {
        let entry = format!("{}={}", key, value);
        let ptr_to_write = env_buf_ptr + current_buf_offset;

        // Write pointer to this entry into the env_ptr array
        if write_u32(store, env_ptr + (i as u32 * 4), ptr_to_write).is_err() {
            return Ok(vec![Value::I32(28)]);
        }

        // Write the "KEY=VALUE\0" string
        let bytes = entry.as_bytes();
        if write_bytes(store, ptr_to_write, bytes).is_err() { return Ok(vec![Value::I32(28)]); }
        if write_bytes(store, ptr_to_write + bytes.len() as u32, &[0]).is_err() { return Ok(vec![Value::I32(28)]); }

        current_buf_offset += bytes.len() as u32 + 1;
    }

    Ok(vec![Value::I32(0)])
}

fn environ_sizes_get<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let count_ptr = match args.get(0) {
        Some(Value::I32(v)) => *v as u32,
        _ => return Ok(vec![Value::I32(28)])
    };
    let buf_ptr = match args.get(1) {
        Some(Value::I32(v)) => *v as u32,
        _ => return Ok(vec![Value::I32(28)])
    };

    let env_vars: Vec<(String, String)> = crate::env::vars().collect();
    let count = env_vars.len();
    let total_size: usize = env_vars.iter().map(|(k, v)| k.len() + v.len() + 2).sum(); // k=v\0

    if write_u32(store, count_ptr, count as u32).is_err() { return Ok(vec![Value::I32(28)]); }
    if write_u32(store, buf_ptr, total_size as u32).is_err() { return Ok(vec![Value::I32(28)]); }

    Ok(vec![Value::I32(0)])
}

fn clock_res_get<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let _id = match args.get(0) {
        Some(Value::I32(v)) => *v,
        _ => 0
    };
    let res_ptr = match args.get(1) {
        Some(Value::I32(v)) => *v as u32,
        _ => return Ok(vec![Value::I32(28)])
    };

    // System ticks are usually 1ms = 1,000,000 ns
    let resolution: u64 = 1_000_000;
    if write_u64(store, res_ptr, resolution).is_err() {
        return Ok(vec![Value::I32(28)]);
    }

    Ok(vec![Value::I32(0)])
}

fn clock_time_get<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let id = match args.get(0) {
        Some(Value::I32(v)) => *v,
        _ => 0
    };
    let time_ptr = match args.get(2) {
        Some(Value::I32(v)) => *v as u32,
        _ => return Ok(vec![Value::I32(28)])
    };

    let (d, m, y) = crate::os::get_date();
    let (h, min, s) = crate::os::get_time();

    let years_since_1970 = if y >= 1970 { (y - 1970) as u64 } else { 0 };
    let mut secs = years_since_1970 * 31_536_000;
    secs += (m as u64).saturating_sub(1) * 2_592_000;
    secs += (d as u64).saturating_sub(1) * 86_400;
    secs += (h as u64) * 3600;
    secs += (min as u64) * 60;
    secs += s as u64;

    let nanos = (secs * 1_000_000_000) + (crate::os::get_system_ticks() % 1000) * 1_000_000;

    let _ = write_u64(store, time_ptr, nanos);
    Ok(vec![Value::I32(0)])
}

fn fd_close<T: Config>(_: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = match args.get(0) {
        Some(Value::I32(v)) => *v as i32,
        _ => -1
    };
    if fd >= 10 {
        FD_TABLE.lock().remove(&fd);
    }
    Ok(vec![Value::I32(0)])
}

fn fd_fdstat_get<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = match args.get(0) {
        Some(Value::I32(v)) => *v as i32,
        _ => return Ok(vec![Value::I32(28)])
    };
    let stat_ptr = match args.get(1) {
        Some(Value::I32(v)) => *v as u32,
        _ => return Ok(vec![Value::I32(28)])
    };

    let mem_addr = store.memories.iter().next();
    if let Some(mem_addr) = mem_addr {
        let (filetype, rights) = if fd >= 0 && fd <= 2 {
            (2u8, 0x000000000000003Fu64) // Character device
        } else if fd == 3 || fd == 4 {
            (3u8, 0xFFFFFFFFFFFFFFFFu64) // Directory + Full Rights
        } else {
            (4u8, 0xFFFFFFFFFFFFFFFFu64) // Regular file
        };

        if write_u32(store, stat_ptr, filetype as u32).is_err() { return Ok(vec![Value::I32(28)]); }
        let mem = store.memories.get(mem_addr);
        if let Err(_) = mem.mem.store::<2, u16>((stat_ptr + 2) as usize, 0) { return Ok(vec![Value::I32(28)]); }
        if write_u64(store, stat_ptr + 8, rights).is_err() { return Ok(vec![Value::I32(28)]); }
        if write_u64(store, stat_ptr + 16, rights).is_err() { return Ok(vec![Value::I32(28)]); }
        Ok(vec![Value::I32(0)])
    } else {
        Ok(vec![Value::I32(28)])
    }
}

fn fd_filestat_get<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = match args.get(0) {
        Some(Value::I32(v)) => *v as i32,
        _ => -1
    };
    let buf_ptr = match args.get(1) {
        Some(Value::I32(v)) => *v as u32,
        _ => 0
    };

    let mem_addr = store.memories.iter().next();
    if let Some(mem_addr) = mem_addr {
        let (filetype, size) = if fd >= 0 && fd <= 2 {
            (2u8, 0u64)
        } else if fd == 3 || fd == 4 {
            (3u8, 0u64)
        } else {
            let size = FD_TABLE.lock().get(&fd).map(|f| f.file.size() as u64).unwrap_or(0);
            (4u8, size)
        };

        if write_u64(store, buf_ptr, 1).is_err() { return Ok(vec![Value::I32(28)]); } // dev
        if write_u64(store, buf_ptr + 8, (fd + 1024) as u64).is_err() { return Ok(vec![Value::I32(28)]); } // ino
        let mem = store.memories.get(mem_addr);
        if let Err(_) = mem.mem.store::<1, u8>((buf_ptr + 16) as usize, filetype) { return Ok(vec![Value::I32(28)]); } // filetype
        if write_u64(store, buf_ptr + 24, 1).is_err() { return Ok(vec![Value::I32(28)]); } // nlink
        if write_u64(store, buf_ptr + 32, size).is_err() { return Ok(vec![Value::I32(28)]); } // size
        if write_u64(store, buf_ptr + 40, 0).is_err() { return Ok(vec![Value::I32(28)]); } // atim
        if write_u64(store, buf_ptr + 48, 0).is_err() { return Ok(vec![Value::I32(28)]); } // mtim
        if write_u64(store, buf_ptr + 56, 0).is_err() { return Ok(vec![Value::I32(28)]); } // ctim
        Ok(vec![Value::I32(0)])
    } else {
        Ok(vec![Value::I32(28)])
    }
}

fn fd_prestat_get<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = match args.get(0) {
        Some(Value::I32(v)) => *v as i32,
        _ => -1
    };
    let prestat_ptr = match args.get(1) {
        Some(Value::I32(v)) => *v as u32,
        _ => 0
    };

    if fd == 3 {
        let mem_addr = store.memories.iter().next();
        if let Some(mem_addr) = mem_addr {
            let mem = store.memories.get(mem_addr);
            if let Err(_) = mem.mem.store::<1, u8>(prestat_ptr as usize, 0) { return Ok(vec![Value::I32(28)]); }
            if write_u32(store, prestat_ptr + 4, 1).is_err() { return Ok(vec![Value::I32(28)]); }
            return Ok(vec![Value::I32(0)]);
        }
    }
    Ok(vec![Value::I32(8)])
}

fn fd_prestat_dir_name<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = match args.get(0) {
        Some(Value::I32(v)) => *v as i32,
        _ => -1
    };
    let path_ptr = match args.get(1) {
        Some(Value::I32(v)) => *v as u32,
        _ => 0
    };
    let path_len = match args.get(2) {
        Some(Value::I32(v)) => *v as u32,
        _ => 0
    };

    if fd == 3 && path_len >= 1 {
        let mem_addr = store.memories.iter().next();
        if let Some(mem_addr) = mem_addr {
            let mem = store.memories.get(mem_addr);
            if let Err(_) = mem.mem.store::<1, u8>(path_ptr as usize, b'/') { return Ok(vec![Value::I32(28)]); }
            return Ok(vec![Value::I32(0)]);
        }
    }
    Ok(vec![Value::I32(8)])
}

fn fd_read<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = match args.get(0) {
        Some(Value::I32(v)) => *v as i32,
        _ => -1
    };
    let iovs_ptr = match args.get(1) {
        Some(Value::I32(v)) => *v as u32,
        _ => return Ok(vec![Value::I32(28)])
    };
    let iovs_len = match args.get(2) {
        Some(Value::I32(v)) => *v as u32,
        _ => return Ok(vec![Value::I32(28)])
    };
    let nread_ptr = match args.get(3) {
        Some(Value::I32(v)) => *v as u32,
        _ => 0
    };

    let mem_addr = store.memories.iter().next();
    if let Some(mem_addr) = mem_addr {
        let mut read_total = 0;
        let mut fd_table = FD_TABLE.lock();
        if let Some(wasi_file) = fd_table.get_mut(&fd) {
            use crate::io::Read;
            for i in 0..iovs_len {
                let mem = store.memories.get(mem_addr);
                let ptr_offset = iovs_ptr + i * 8;
                let len_offset = ptr_offset + 4;
                let buf_ptr = match mem.mem.load::<4, u32>(ptr_offset as usize) {
                    Ok(v) => v,
                    Err(_) => return Ok(vec![Value::I32(21)])
                };
                let buf_len = match mem.mem.load::<4, u32>(len_offset as usize) {
                    Ok(v) => v,
                    Err(_) => return Ok(vec![Value::I32(21)])
                };

                let mut bytes = vec![0u8; buf_len as usize];
                match wasi_file.file.read(&mut bytes) {
                    Ok(n) => {
                        let mem = store.memories.get(mem_addr);
                        for j in 0..n {
                            let _ = mem.mem.store::<1, u8>((buf_ptr + j as u32) as usize, bytes[j]);
                        }
                        read_total += n;
                        if n < buf_len as usize { break; }
                    }
                    Err(_) => return Ok(vec![Value::I32(28)]),
                }
            }
        }

        if nread_ptr != 0 {
            let _ = write_u32(store, nread_ptr, read_total as u32);
        }
        Ok(vec![Value::I32(0)])
    } else {
        Ok(vec![Value::I32(28)])
    }
}

fn fd_seek<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = match args.get(0) {
        Some(Value::I32(v)) => *v as i32,
        _ => -1
    };
    let offset = match args.get(1) {
        Some(Value::I64(v)) => *v as i64,
        _ => 0
    };
    let whence = match args.get(2) {
        Some(Value::I32(v)) => *v as u32,
        _ => 0
    };
    let new_offset_ptr = match args.get(3) {
        Some(Value::I32(v)) => *v as u32,
        _ => 0
    };

    let mut fd_table = FD_TABLE.lock();
    if let Some(wasi_file) = fd_table.get_mut(&fd) {
        use crate::io::{Seek, SeekFrom};
        let pos = match whence {
            0 => SeekFrom::Start(offset as u64),
            1 => SeekFrom::Current(offset),
            2 => SeekFrom::End(offset),
            _ => return Ok(vec![Value::I32(28)]),
        };

        match wasi_file.file.seek(pos) {
            Ok(new_offset) => {
                if new_offset_ptr != 0 {
                    let _ = write_u64(store, new_offset_ptr, new_offset);
                }
                Ok(vec![Value::I32(0)])
            }
            Err(_) => Ok(vec![Value::I32(28)]),
        }
    } else {
        Ok(vec![Value::I32(8)])
    }
}

fn fd_tell<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = match args.get(0) {
        Some(Value::I32(v)) => *v as i32,
        _ => -1
    };
    let offset_ptr = match args.get(1) {
        Some(Value::I32(v)) => *v as u32,
        _ => 0
    };

    let mut fd_table = FD_TABLE.lock();
    if let Some(wasi_file) = fd_table.get_mut(&fd) {
        use crate::io::{Seek, SeekFrom};
        match wasi_file.file.seek(SeekFrom::Current(0)) {
            Ok(offset) => {
                if offset_ptr != 0 {
                    let _ = write_u64(store, offset_ptr, offset);
                }
                Ok(vec![Value::I32(0)])
            }
            Err(_) => Ok(vec![Value::I32(28)]),
        }
    } else {
        Ok(vec![Value::I32(8)])
    }
}

fn fd_write<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = match args.get(0) {
        Some(Value::I32(v)) => *v as i32,
        _ => return Ok(vec![Value::I32(28)])
    };
    let iovs_ptr = match args.get(1) {
        Some(Value::I32(v)) => *v as u32,
        _ => return Ok(vec![Value::I32(28)])
    };
    let iovs_len = match args.get(2) {
        Some(Value::I32(v)) => *v as u32,
        _ => return Ok(vec![Value::I32(28)])
    };
    let nwritten_ptr = match args.get(3) {
        Some(Value::I32(v)) => *v as u32,
        _ => return Ok(vec![Value::I32(28)])
    };

    let mem_addr = store.memories.iter().next();
    if let Some(mem_addr) = mem_addr {
        let mut written_total = 0;
        for i in 0..iovs_len {
            let mem = store.memories.get(mem_addr);
            let ptr_offset = iovs_ptr + i * 8;
            let len_offset = ptr_offset + 4;
            let buf_ptr = match mem.mem.load::<4, u32>(ptr_offset as usize) {
                Ok(v) => v,
                Err(_) => return Ok(vec![Value::I32(21)])
            };
            let buf_len = match mem.mem.load::<4, u32>(len_offset as usize) {
                Ok(v) => v,
                Err(_) => return Ok(vec![Value::I32(21)])
            };

            let mut bytes = vec![0u8; buf_len as usize];
            for j in 0..buf_len {
                match mem.mem.load::<1, u8>((buf_ptr + j) as usize) {
                    Ok(b) => bytes[j as usize] = b,
                    Err(_) => return Ok(vec![Value::I32(21)])
                }
            }

            if fd >= 1 && fd <= 2 {
                if let Ok(s) = core::str::from_utf8(&bytes) {
                    debug_print!("{}", s);
                } else {
                    debug_print!("{:?}", bytes);
                }
                written_total += buf_len;
            } else {
                let mut fd_table = FD_TABLE.lock();
                if let Some(wasi_file) = fd_table.get_mut(&fd) {
                    use crate::io::Write;
                    if let Ok(n) = wasi_file.file.write(&bytes) {
                        written_total += n as u32;
                    }
                }
            }
        }

        let _ = write_u32(store, nwritten_ptr, written_total);
        Ok(vec![Value::I32(0)])
    } else {
        Ok(vec![Value::I32(28)])
    }
}

fn path_open<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let dir_fd = match args.get(0) {
        Some(Value::I32(v)) => *v as i32,
        _ => -1
    };
    let path_ptr = match args.get(2) {
        Some(Value::I32(v)) => *v as u32,
        _ => 0
    };
    let path_len = match args.get(3) {
        Some(Value::I32(v)) => *v as u32,
        _ => 0
    };
    let oflags = match args.get(4) {
        Some(Value::I32(v)) => *v as u32,
        _ => 0
    };
    let opened_fd_ptr = match args.get(8) {
        Some(Value::I32(v)) => *v as u32,
        _ => 0
    };

    let mem_addr = store.memories.iter().next();
    if let Some(mem_addr) = mem_addr {
        let mut path_bytes = vec![0u8; path_len as usize];
        let mem = store.memories.get(mem_addr);
        for i in 0..path_len {
            if let Ok(b) = mem.mem.load::<1, u8>((path_ptr + i) as usize) {
                path_bytes[i as usize] = b;
            }
        }

        let mut path_str = String::from_utf8_lossy(&path_bytes).into_owned();
        if !path_str.starts_with('/') {
            path_str = String::from("/") + &path_str;
        }

        let krake_path = String::from("@0xE0") + &path_str;
        let file_res = if (oflags & 0x8) != 0 { // O_CREAT
            fs::File::create(&krake_path)
        } else {
            fs::File::open(&krake_path)
        };

        match file_res {
            Ok(file) => {
                let mut next_fd_lock = NEXT_FD.lock();
                let fd = *next_fd_lock;
                *next_fd_lock += 1;

                FD_TABLE.lock().insert(fd, WasiFile { file, _path: path_str });

                if opened_fd_ptr != 0 {
                    let _ = write_u32(store, opened_fd_ptr, fd as u32);
                }
                Ok(vec![Value::I32(0)])
            }
            Err(_) => Ok(vec![Value::I32(44)])
        }
    } else {
        Ok(vec![Value::I32(28)])
    }
}

fn path_filestat_get<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let _dir_fd = match args.get(0) {
        Some(Value::I32(v)) => *v as i32,
        _ => -1
    };
    let path_ptr = match args.get(2) {
        Some(Value::I32(v)) => *v as u32,
        _ => 0
    };
    let path_len = match args.get(3) {
        Some(Value::I32(v)) => *v as u32,
        _ => 0
    };
    let buf_ptr = match args.get(4) {
        Some(Value::I32(v)) => *v as u32,
        _ => 0
    };

    let mem_addr = store.memories.iter().next();
    if let Some(mem_addr) = mem_addr {
        let mut path_bytes = vec![0u8; path_len as usize];
        let mem = store.memories.get(mem_addr);
        for i in 0..path_len {
            if let Ok(b) = mem.mem.load::<1, u8>((path_ptr + i) as usize) {
                path_bytes[i as usize] = b;
            }
        }

        let mut path_str = String::from_utf8_lossy(&path_bytes).into_owned();
        if !path_str.starts_with('/') { path_str = String::from("/") + &path_str; }
        let krake_path = String::from("@0xE0") + &path_str;

        let (filetype, size) = if let Ok(file) = fs::File::open(&krake_path) {
            (4u8, file.size() as u64)
        } else if fs::read_dir(&krake_path).is_ok() {
            (3u8, 0u64)
        } else {
            return Ok(vec![Value::I32(44)]);
        };

        if write_u64(store, buf_ptr, 1).is_err() { return Ok(vec![Value::I32(28)]); } // dev
        if write_u64(store, buf_ptr + 8, 1024).is_err() { return Ok(vec![Value::I32(28)]); } // ino
        let mem = store.memories.get(mem_addr);
        if let Err(_) = mem.mem.store::<1, u8>((buf_ptr + 16) as usize, filetype) { return Ok(vec![Value::I32(28)]); } // filetype
        if write_u64(store, buf_ptr + 24, 1).is_err() { return Ok(vec![Value::I32(28)]); } // nlink
        if write_u64(store, buf_ptr + 32, size).is_err() { return Ok(vec![Value::I32(28)]); } // size
        if write_u64(store, buf_ptr + 40, 0).is_err() { return Ok(vec![Value::I32(28)]); } // atim
        if write_u64(store, buf_ptr + 48, 0).is_err() { return Ok(vec![Value::I32(28)]); } // mtim
        if write_u64(store, buf_ptr + 56, 0).is_err() { return Ok(vec![Value::I32(28)]); } // ctim
        Ok(vec![Value::I32(0)])
    } else {
        Ok(vec![Value::I32(28)])
    }
}

fn random_get<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let buf_ptr = match args.get(0) {
        Some(Value::I32(v)) => *v as u32,
        _ => 0
    };
    let buf_len = match args.get(1) {
        Some(Value::I32(v)) => *v as u32,
        _ => 0
    };

    unsafe {
        if RANDOM_STATE == 0 {
            RANDOM_STATE = crate::os::get_system_ticks().wrapping_add(0xACE1BADE);
        }
        let mut bytes = vec![0u8; buf_len as usize];
        for i in 0..buf_len as usize {
            RANDOM_STATE ^= RANDOM_STATE << 13;
            RANDOM_STATE ^= RANDOM_STATE >> 17;
            RANDOM_STATE ^= RANDOM_STATE << 5;
            bytes[i] = (RANDOM_STATE & 0xFF) as u8;
        }
        if write_bytes(store, buf_ptr, &bytes).is_err() {
            return Ok(vec![Value::I32(28)]);
        }
    }
    Ok(vec![Value::I32(0)])
}

fn sched_yield<T: Config>(_: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    Ok(vec![Value::I32(0)])
}

fn path_create_directory<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let _dir_fd = match args.get(0) {
        Some(Value::I32(v)) => *v as i32,
        _ => -1
    };
    let path_ptr = match args.get(1) {
        Some(Value::I32(v)) => *v as u32,
        _ => 0
    };
    let path_len = match args.get(2) {
        Some(Value::I32(v)) => *v as u32,
        _ => 0
    };

    if let Some(mem_addr) = store.memories.iter().next() {
        let mut path_bytes = vec![0u8; path_len as usize];
        let mem = store.memories.get(mem_addr);
        for i in 0..path_len {
            if let Ok(b) = mem.mem.load::<1, u8>((path_ptr + i) as usize) {
                path_bytes[i as usize] = b;
            }
        }
        let mut path_str = String::from_utf8_lossy(&path_bytes).into_owned();
        if !path_str.starts_with('/') { path_str = String::from("/") + &path_str; }
        let krake_path = String::from("@0xE0") + &path_str;
        match fs::create_dir(&krake_path) {
            Ok(_) => Ok(vec![Value::I32(0)]),
            Err(_) => Ok(vec![Value::I32(28)]),
        }
    } else {
        Ok(vec![Value::I32(28)])
    }
}

fn path_remove_directory<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let _dir_fd = match args.get(0) {
        Some(Value::I32(v)) => *v as i32,
        _ => -1
    };
    let path_ptr = match args.get(1) {
        Some(Value::I32(v)) => *v as u32,
        _ => 0
    };
    let path_len = match args.get(2) {
        Some(Value::I32(v)) => *v as u32,
        _ => 0
    };

    if let Some(mem_addr) = store.memories.iter().next() {
        let mut path_bytes = vec![0u8; path_len as usize];
        let mem = store.memories.get(mem_addr);
        for i in 0..path_len {
            if let Ok(b) = mem.mem.load::<1, u8>((path_ptr + i) as usize) {
                path_bytes[i as usize] = b;
            }
        }
        let mut path_str = String::from_utf8_lossy(&path_bytes).into_owned();
        if !path_str.starts_with('/') { path_str = String::from("/") + &path_str; }
        let krake_path = String::from("@0xE0") + &path_str;
        match fs::remove_dir(&krake_path) {
            Ok(_) => Ok(vec![Value::I32(0)]),
            Err(_) => Ok(vec![Value::I32(28)]),
        }
    } else {
        Ok(vec![Value::I32(28)])
    }
}

fn path_unlink_file<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let _dir_fd = match args.get(0) {
        Some(Value::I32(v)) => *v as i32,
        _ => -1
    };
    let path_ptr = match args.get(1) {
        Some(Value::I32(v)) => *v as u32,
        _ => 0
    };
    let path_len = match args.get(2) {
        Some(Value::I32(v)) => *v as u32,
        _ => 0
    };

    if let Some(mem_addr) = store.memories.iter().next() {
        let mut path_bytes = vec![0u8; path_len as usize];
        let mem = store.memories.get(mem_addr);
        for i in 0..path_len {
            if let Ok(b) = mem.mem.load::<1, u8>((path_ptr + i) as usize) {
                path_bytes[i as usize] = b;
            }
        }
        let mut path_str = String::from_utf8_lossy(&path_bytes).into_owned();
        if !path_str.starts_with('/') { path_str = String::from("/") + &path_str; }
        let krake_path = String::from("@0xE0") + &path_str;
        match fs::remove_file(&krake_path) {
            Ok(_) => Ok(vec![Value::I32(0)]),
            Err(_) => Ok(vec![Value::I32(28)]),
        }
    } else {
        Ok(vec![Value::I32(28)])
    }
}

fn path_rename<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let _old_dir_fd = match args.get(0) {
        Some(Value::I32(v)) => *v as i32,
        _ => -1
    };
    let old_path_ptr = match args.get(1) {
        Some(Value::I32(v)) => *v as u32,
        _ => 0
    };
    let old_path_len = match args.get(2) {
        Some(Value::I32(v)) => *v as u32,
        _ => 0
    };
    let _new_dir_fd = match args.get(3) {
        Some(Value::I32(v)) => *v as i32,
        _ => -1
    };
    let new_path_ptr = match args.get(4) {
        Some(Value::I32(v)) => *v as u32,
        _ => 0
    };
    let new_path_len = match args.get(5) {
        Some(Value::I32(v)) => *v as u32,
        _ => 0
    };

    if let Some(mem_addr) = store.memories.iter().next() {
        let mut old_path_bytes = vec![0u8; old_path_len as usize];
        let mut new_path_bytes = vec![0u8; new_path_len as usize];
        let mem = store.memories.get(mem_addr);
        for i in 0..old_path_len {
            if let Ok(b) = mem.mem.load::<1, u8>((old_path_ptr + i) as usize) {
                old_path_bytes[i as usize] = b;
            }
        }
        for i in 0..new_path_len {
            if let Ok(b) = mem.mem.load::<1, u8>((new_path_ptr + i) as usize) {
                new_path_bytes[i as usize] = b;
            }
        }
        let mut old_path = String::from_utf8_lossy(&old_path_bytes).into_owned();
        let mut new_path = String::from_utf8_lossy(&new_path_bytes).into_owned();
        if !old_path.starts_with('/') { old_path = String::from("/") + &old_path; }
        if !new_path.starts_with('/') { new_path = String::from("/") + &new_path; }
        let old_krake = String::from("@0xE0") + &old_path;
        let new_krake = String::from("@0xE0") + &new_path;
        match fs::rename(&old_krake, &new_krake) {
            Ok(_) => Ok(vec![Value::I32(0)]),
            Err(_) => Ok(vec![Value::I32(28)]),
        }
    } else {
        Ok(vec![Value::I32(28)])
    }
}

fn proc_exit<T: Config>(_: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let exit_code = match args.get(0) {
        Some(Value::I32(v)) => *v as i32,
        _ => 0
    };
    debugln!("WASI: proc_exit({})", exit_code);
    Err(HaltExecutionError)
}

fn fd_readdir<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = match args.get(0) {
        Some(Value::I32(v)) => *v as i32,
        _ => -1
    };
    let buf_ptr = match args.get(1) {
        Some(Value::I32(v)) => *v as u32,
        _ => 0
    };
    let buf_len = match args.get(2) {
        Some(Value::I32(v)) => *v as u32,
        _ => 0
    };
    let cookie = match args.get(3) {
        Some(Value::I64(v)) => *v as u64,
        _ => 0
    };
    let bufused_ptr = match args.get(4) {
        Some(Value::I32(v)) => *v as u32,
        _ => 0
    };

    let path = if fd == 3 || fd == 4 { "/" } else { "." };
    let mut entries = Vec::new();

    match crate::fs::read_dir(path) {
        Ok(real_entries) => {
            for entry in real_entries {
                let wasi_type = match entry.file_type {
                    crate::fs::FileType::File => 4,
                    crate::fs::FileType::Directory => 3,
                    crate::fs::FileType::Device => 2,
                    crate::fs::FileType::Unknown => 0,
                };
                entries.push((entry.name, wasi_type));
            }
        }
        Err(_) => return Ok(vec![Value::I32(28)]),
    }

    let mut bufused = 0;
    if cookie < entries.len() as u64 {
        let mem_addr = store.memories.iter().next();
        for (i, (name, file_type)) in entries.iter().enumerate().skip(cookie as usize) {
            let name_bytes = name.as_bytes();
            let name_len = name_bytes.len();
            let entry_size = 24 + name_len;

            if (bufused + entry_size) > buf_len as usize {
                bufused = buf_len as usize;
                break;
            }

            let entry_base = buf_ptr + bufused as u32;
            let next_cookie = (i + 1) as u64;

            let _ = write_u64(store, entry_base, next_cookie);
            let _ = write_u64(store, entry_base + 8, (i + 1024) as u64);
            let _ = write_u32(store, entry_base + 16, name_len as u32);

            if let Some(mem_addr) = mem_addr {
                let mem = store.memories.get(mem_addr);
                let _ = mem.mem.store::<1, u8>((entry_base + 20) as usize, *file_type);
                let _ = mem.mem.store::<1, u8>((entry_base + 21) as usize, 0);
                let _ = mem.mem.store::<1, u8>((entry_base + 22) as usize, 0);
                let _ = mem.mem.store::<1, u8>((entry_base + 23) as usize, 0);
            }

            let _ = write_bytes(store, entry_base + 24, name_bytes);
            bufused += entry_size;
        }
    }

    if bufused_ptr != 0 {
        let _ = write_u32(store, bufused_ptr, bufused as u32);
    }

    Ok(vec![Value::I32(0)])
}
