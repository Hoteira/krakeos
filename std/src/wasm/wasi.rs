use crate::rust_alloc::{string::String, string::ToString, vec, vec::Vec, collections::BTreeMap, format};
use crate::fs;
use crate::wasm::{
    core::reader::types::{FuncType, ResultType, ValType, NumType},
    execution::{
        linker::Linker,
        store::{Store, ExternVal, HaltExecutionError},
        value::Value,
        config::Config,
    },
};
use crate::{debugln, debug_print};
use crate::sync::Mutex;

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

fn write_u16<T: Config>(store: &mut Store<'_, T>, addr: u32, val: u16) -> Result<(), ()> { write_bytes(store, addr, &val.to_le_bytes()) }
fn write_u32<T: Config>(store: &mut Store<'_, T>, addr: u32, val: u32) -> Result<(), ()> { write_bytes(store, addr, &val.to_le_bytes()) }
fn write_u64<T: Config>(store: &mut Store<'_, T>, addr: u32, val: u64) -> Result<(), ()> { write_bytes(store, addr, &val.to_le_bytes()) }
fn write_bytes<T: Config>(store: &mut Store<'_, T>, addr: u32, bytes: &[u8]) -> Result<(), ()> {
    let mem_addr = if let Some(addr) = store.memories.iter().next() { addr } else { return Err(()); };
    let mem = store.memories.get(mem_addr);
    for (i, &b) in bytes.iter().enumerate() { mem.mem.store::<1, u8>(addr as usize + i, b).map_err(|_| ())?; }
    Ok(())
}
fn read_bytes<T: Config>(store: &Store<'_, T>, addr: u32, buf: &mut [u8]) -> Result<(), ()> {
    let mem_addr = if let Some(addr) = store.memories.iter().next() { addr } else { return Err(()); };
    let mem = store.memories.get(mem_addr);
    for i in 0..buf.len() { buf[i] = mem.mem.load::<1, u8>(addr as usize + i).map_err(|_| ())?; }
    Ok(())
}

fn args_get<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let argv_ptr = match args.get(0) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![Value::I32(28)]) };
    let argv_buf_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![Value::I32(28)]) };
    let args_vec: Vec<String> = crate::env::args().collect();
    let mut offset = 0;
    for (i, arg) in args_vec.iter().enumerate() {
        let p = argv_buf_ptr + offset;
        if write_u32(store, argv_ptr + (i as u32 * 4), p).is_err() { return Ok(vec![Value::I32(28)]); }
        let b = arg.as_bytes();
        if write_bytes(store, p, b).is_err() || write_bytes(store, p + b.len() as u32, &[0]).is_err() { return Ok(vec![Value::I32(28)]); }
        offset += b.len() as u32 + 1;
    }
    Ok(vec![Value::I32(0)])
}

fn args_sizes_get<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let c_ptr = match args.get(0) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![Value::I32(28)]) };
    let b_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![Value::I32(28)]) };
    let args_vec: Vec<String> = crate::env::args().collect();
    if write_u32(store, c_ptr, args_vec.len() as u32).is_err() || write_u32(store, b_ptr, args_vec.iter().map(|s| s.len() + 1).sum::<usize>() as u32).is_err() { return Ok(vec![Value::I32(28)]); }
    Ok(vec![Value::I32(0)])
}

fn environ_get<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let e_ptr = match args.get(0) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![Value::I32(28)]) };
    let b_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![Value::I32(28)]) };
    let env_vars: Vec<(String, String)> = crate::env::vars().collect();
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
    let c_ptr = match args.get(0) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![Value::I32(28)]) };
    let b_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![Value::I32(28)]) };
    let env_vars: Vec<(String, String)> = crate::env::vars().collect();
    if write_u32(store, c_ptr, env_vars.len() as u32).is_err() || write_u32(store, b_ptr, env_vars.iter().map(|(k, v)| k.len() + v.len() + 2).sum::<usize>() as u32).is_err() { return Ok(vec![Value::I32(28)]); }
    Ok(vec![Value::I32(0)])
}

fn clock_res_get<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let r_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![Value::I32(28)]) };
    let _ = write_u64(store, r_ptr, 1_000_000);
    Ok(vec![Value::I32(0)])
}

fn clock_time_get<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let id = match args.get(0) { Some(Value::I32(v)) => *v, _ => 0 };
    let t_ptr = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![Value::I32(28)]) };
    let nanos = if id == 1 { crate::os::get_system_ticks() * 1_000_000 } else {
        let (d, m, y) = crate::os::get_date();
        let (h, min, s) = crate::os::get_time();
        let yrs = if y >= 1970 { (y - 1970) as u64 } else { 0 };
        let mut secs = yrs * 31_536_000 + (m as u64).saturating_sub(1) * 2_592_000 + (d as u64).saturating_sub(1) * 86_400 + (h as u64) * 3600 + (min as u64) * 60 + s as u64;
        (secs * 1_000_000_000) + (crate::os::get_system_ticks() % 1000) * 1_000_000
    };
    let _ = write_u64(store, t_ptr, nanos);
    Ok(vec![Value::I32(0)])
}

fn fd_close<T: Config>(_: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = match args.get(0) { Some(Value::I32(v)) => *v as i32, _ => -1 };
    if fd >= 10 { FD_TABLE.lock().remove(&fd); }
    Ok(vec![Value::I32(0)])
}

fn fd_fdstat_get<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = match args.get(0) { Some(Value::I32(v)) => *v as i32, _ => return Ok(vec![Value::I32(28)]) };
    let s_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![Value::I32(28)]) };
    let (ft, r) = if fd >= 0 && fd <= 2 { (2u8, 0x3Fu64) } else if fd == 3 || fd == 4 { (3u8, !0u64) } else { (4u8, !0u64) };
    if write_u16(store, s_ptr, ft as u16).is_err() || write_u16(store, s_ptr + 2, 0).is_err() || write_u64(store, s_ptr + 8, r).is_err() || write_u64(store, s_ptr + 16, r).is_err() { return Ok(vec![Value::I32(28)]); }
    Ok(vec![Value::I32(0)])
}

fn fd_filestat_get<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = match args.get(0) { Some(Value::I32(v)) => *v as i32, _ => -1 };
    let b_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    if let Some(wf) = FD_TABLE.lock().get_mut(&fd) {
        if let Ok(s) = wf.file.stat() {
            let ft = if (s.mode & 0xF000) == 0x4000 { 3u8 } else { 4u8 };
            if write_u64(store, b_ptr, s.dev).is_err() || write_u64(store, b_ptr + 8, s.ino).is_err() || write_bytes(store, b_ptr + 16, &[ft]).is_err() || write_u64(store, b_ptr + 24, s.nlink as u64).is_err() || write_u64(store, b_ptr + 32, s.size).is_err() || write_u64(store, b_ptr + 40, s.atime * 1_000_000_000).is_err() || write_u64(store, b_ptr + 48, s.mtime * 1_000_000_000).is_err() || write_u64(store, b_ptr + 56, s.ctime * 1_000_000_000).is_err() { return Ok(vec![Value::I32(28)]); } 
            return Ok(vec![Value::I32(0)]);
        }
    }
    if fd == 3 || fd == 4 {
        if write_u64(store, b_ptr, 1).is_err() || write_u64(store, b_ptr + 8, 2).is_err() || write_bytes(store, b_ptr + 16, &[3u8]).is_err() || write_u64(store, b_ptr + 24, 1).is_err() || write_u64(store, b_ptr + 32, 0).is_err() || write_u64(store, b_ptr + 40, 0).is_err() || write_u64(store, b_ptr + 48, 0).is_err() || write_u64(store, b_ptr + 56, 0).is_err() { return Ok(vec![Value::I32(28)]); }
        Ok(vec![Value::I32(0)])
    } else { Ok(vec![Value::I32(8)]) }
}

fn fd_filestat_set_size<T: Config>(_: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = match args.get(0) { Some(Value::I32(v)) => *v as i32, _ => -1 };
    let sz = match args.get(1) { Some(Value::I64(v)) => *v as u64, _ => 0 };
    if let Some(wf) = FD_TABLE.lock().get_mut(&fd) { match wf.file.set_len(sz) { Ok(_) => Ok(vec![Value::I32(0)]), Err(_) => Ok(vec![Value::I32(28)]) } }
    else { Ok(vec![Value::I32(8)]) }
}

fn fd_prestat_get<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = match args.get(0) { Some(Value::I32(v)) => *v as i32, _ => -1 };
    let ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    if fd == 3 {
        if write_bytes(store, ptr, &[0]).is_err() || write_u32(store, ptr + 4, 1).is_err() { return Ok(vec![Value::I32(28)]); } 
        return Ok(vec![Value::I32(0)]);
    }
    Ok(vec![Value::I32(8)]) 
}

fn fd_prestat_dir_name<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = match args.get(0) { Some(Value::I32(v)) => *v as i32, _ => -1 };
    let ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let len = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    if fd == 3 && len >= 1 { if write_bytes(store, ptr, b"/").is_err() { return Ok(vec![Value::I32(28)]); } return Ok(vec![Value::I32(0)]); }
    Ok(vec![Value::I32(8)])
}

fn fd_read<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = match args.get(0) { Some(Value::I32(v)) => *v as i32, _ => -1 };
    let i_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![Value::I32(28)]) };
    let i_len = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![Value::I32(28)]) };
    let n_ptr = match args.get(3) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let mut total = 0;
    if let Some(wf) = FD_TABLE.lock().get_mut(&fd) {
        use crate::io::Read;
        for i in 0..i_len {
            let mut iov = [0u8; 8];
            if read_bytes(store, i_ptr + i * 8, &mut iov).is_err() { return Ok(vec![Value::I32(21)]); }
            let b_ptr = u32::from_le_bytes(iov[0..4].try_into().unwrap());
            let b_len = u32::from_le_bytes(iov[4..8].try_into().unwrap());
            let mut b = vec![0u8; b_len as usize];
            if let Ok(n) = wf.file.read(&mut b) {
                if write_bytes(store, b_ptr, &b[..n]).is_err() { return Ok(vec![Value::I32(28)]); }
                total += n; if n < b_len as usize { break; }
            } else { return Ok(vec![Value::I32(28)]); }
        }
    }
    if n_ptr != 0 { let _ = write_u32(store, n_ptr, total as u32); }
    Ok(vec![Value::I32(0)])
}

fn fd_seek<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = match args.get(0) { Some(Value::I32(v)) => *v as i32, _ => -1 };
    let off = match args.get(1) { Some(Value::I64(v)) => *v as i64, _ => 0 };
    let wh = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let n_ptr = match args.get(3) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    if let Some(wf) = FD_TABLE.lock().get_mut(&fd) {
        use crate::io::{Seek, SeekFrom};
        let p = match wh { 0 => SeekFrom::Start(off as u64), 1 => SeekFrom::Current(off), 2 => SeekFrom::End(off), _ => return Ok(vec![Value::I32(28)]) };
        match wf.file.seek(p) { Ok(n) => { if n_ptr != 0 { let _ = write_u64(store, n_ptr, n); } Ok(vec![Value::I32(0)]) }, Err(_) => Ok(vec![Value::I32(28)]) }
    } else { Ok(vec![Value::I32(8)]) }
}

fn fd_tell<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = match args.get(0) { Some(Value::I32(v)) => *v as i32, _ => -1 };
    let p_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    if let Some(wf) = FD_TABLE.lock().get_mut(&fd) {
        use crate::io::{Seek, SeekFrom};
        match wf.file.seek(SeekFrom::Current(0)) { Ok(n) => { if p_ptr != 0 { let _ = write_u64(store, p_ptr, n); } Ok(vec![Value::I32(0)]) }, Err(_) => Ok(vec![Value::I32(28)]) }
    } else { Ok(vec![Value::I32(8)]) }
}

fn fd_sync<T: Config>(_: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = match args.get(0) { Some(Value::I32(v)) => *v as i32, _ => -1 };
    if FD_TABLE.lock().contains_key(&fd) { Ok(vec![Value::I32(0)]) } else { Ok(vec![Value::I32(8)]) }
}

fn fd_write<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = match args.get(0) { Some(Value::I32(v)) => *v as i32, _ => return Ok(vec![Value::I32(28)]) }; 
    let i_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![Value::I32(28)]) };
    let i_len = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![Value::I32(28)]) };
    let n_ptr = match args.get(3) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![Value::I32(28)]) };
    let mut total = 0;
    for i in 0..i_len {
        let mut iov = [0u8; 8];
        if read_bytes(store, i_ptr + i * 8, &mut iov).is_err() { return Ok(vec![Value::I32(21)]); }
        let b_ptr = u32::from_le_bytes(iov[0..4].try_into().unwrap());
        let b_len = u32::from_le_bytes(iov[4..8].try_into().unwrap());
        let mut b = vec![0u8; b_len as usize];
        if read_bytes(store, b_ptr, &mut b).is_err() { return Ok(vec![Value::I32(21)]); }
        if fd >= 1 && fd <= 2 { if let Ok(s) = core::str::from_utf8(&b) { debug_print!("{}", s); } else { debug_print!("{:?}", b); } total += b_len; }
        else { if let Some(wf) = FD_TABLE.lock().get_mut(&fd) { use crate::io::Write; if let Ok(n) = wf.file.write(&b) { total += n as u32; } } }
    }
    let _ = write_u32(store, n_ptr, total);
    Ok(vec![Value::I32(0)]) 
}

fn path_open<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let ptr = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let len = match args.get(3) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let of = match args.get(4) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let f_ptr = match args.get(8) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let mut pb = vec![0u8; len as usize];
    if read_bytes(store, ptr, &mut pb).is_err() { return Ok(vec![Value::I32(21)]); }
    let ps = String::from_utf8_lossy(&pb).into_owned();
    let cp = ps.trim_start_matches('.').trim_start_matches('/').to_string();
    let kp = format!("@0xE0/{}", cp);
    let res = if (of & 0x1) != 0 { fs::File::create(&kp) } else { fs::File::open(&kp) };
    match res {
        Ok(mut f) => {
            if (of & 0x8) != 0 { let _ = f.set_len(0); }
            let mut nl = NEXT_FD.lock();
            let fd = *nl; *nl += 1;
            FD_TABLE.lock().insert(fd, WasiFile { file: f, _path: ps });
            if f_ptr != 0 { let _ = write_u32(store, f_ptr, fd as u32); }
            Ok(vec![Value::I32(0)])
        },
        Err(_) => Ok(vec![Value::I32(44)]) 
    }
}

fn path_filestat_get<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let ptr = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let len = match args.get(3) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let b_ptr = match args.get(4) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let mut pb = vec![0u8; len as usize];
    if read_bytes(store, ptr, &mut pb).is_err() { return Ok(vec![Value::I32(21)]); }
    let ps = String::from_utf8_lossy(&pb).into_owned();
    let cp = ps.trim_start_matches('.').trim_start_matches('/').to_string();
    let kp = format!("@0xE0/{}", cp);
    if let Ok(f) = fs::File::open(&kp) {
        if let Ok(s) = f.stat() {
            let ft = if (s.mode & 0xF000) == 0x4000 { 3u8 } else { 4u8 };
            if write_u64(store, b_ptr, s.dev).is_err() || write_u64(store, b_ptr + 8, s.ino).is_err() || write_bytes(store, b_ptr + 16, &[ft]).is_err() || write_u64(store, b_ptr + 24, s.nlink as u64).is_err() || write_u64(store, b_ptr + 32, s.size).is_err() || write_u64(store, b_ptr + 40, s.atime * 1_000_000_000).is_err() || write_u64(store, b_ptr + 48, s.mtime * 1_000_000_000).is_err() || write_u64(store, b_ptr + 56, s.ctime * 1_000_000_000).is_err() { return Ok(vec![Value::I32(28)]); } 
            return Ok(vec![Value::I32(0)]);
        }
    }
    Ok(vec![Value::I32(44)]) 
}

fn random_get<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let ptr = match args.get(0) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let len = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    unsafe { if RANDOM_STATE == 0 { RANDOM_STATE = crate::os::get_system_ticks().wrapping_add(0xACE1BADE); } let mut b = vec![0u8; len as usize]; for i in 0..len as usize { RANDOM_STATE ^= RANDOM_STATE << 13; RANDOM_STATE ^= RANDOM_STATE >> 17; RANDOM_STATE ^= RANDOM_STATE << 5; b[i] = (RANDOM_STATE & 0xFF) as u8; } if write_bytes(store, ptr, &b).is_err() { return Ok(vec![Value::I32(28)]); } }
    Ok(vec![Value::I32(0)])
}

fn sched_yield<T: Config>(_: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> { crate::os::yield_task(); Ok(vec![Value::I32(0)]) }

fn poll_oneoff<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let in_p = match args.get(0) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let out_p = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let nsub = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let nev_p = match args.get(3) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let mut nev = 0;
    for i in 0..nsub {
        let mut sub = [0u8; 48];
        if read_bytes(store, in_p + i * 48, &mut sub).is_err() { return Ok(vec![Value::I32(21)]); }
        let ud = u64::from_le_bytes(sub[0..8].try_into().unwrap());
        if sub[8] == 0 {
            let to = u64::from_le_bytes(sub[24..32].try_into().unwrap());
            if u16::from_le_bytes(sub[40..42].try_into().unwrap()) == 0 { crate::os::sleep(to / 1_000_000); }
            let eb = out_p + (nev * 32);
            if write_u64(store, eb, ud).is_err() || write_u16(store, eb + 8, 0).is_err() || write_bytes(store, eb + 10, &[0]).is_err() { return Ok(vec![Value::I32(28)]); }
            nev += 1;
        } else { let eb = out_p + (nev * 32); let _ = write_u64(store, eb, ud); let _ = write_u16(store, eb + 8, 58); nev += 1; }
    }
    if write_u32(store, nev_p, nev).is_err() { return Ok(vec![Value::I32(28)]); }
    Ok(vec![Value::I32(0)])
}

fn path_create_directory<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let len = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let mut pb = vec![0u8; len as usize];
    if read_bytes(store, ptr, &mut pb).is_err() { return Ok(vec![Value::I32(21)]); }
    let cp = String::from_utf8_lossy(&pb).into_owned().trim_start_matches('.').trim_start_matches('/').to_string();
    match fs::create_dir(&format!("@0xE0/{}", cp)) { Ok(_) => Ok(vec![Value::I32(0)]), Err(_) => Ok(vec![Value::I32(28)]) }
}

fn path_remove_directory<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let len = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let mut pb = vec![0u8; len as usize];
    if read_bytes(store, ptr, &mut pb).is_err() { return Ok(vec![Value::I32(21)]); }
    let cp = String::from_utf8_lossy(&pb).into_owned().trim_start_matches('.').trim_start_matches('/').to_string();
    match fs::remove_dir(&format!("@0xE0/{}", cp)) { Ok(_) => Ok(vec![Value::I32(0)]), Err(_) => Ok(vec![Value::I32(28)]) }
}

fn path_unlink_file<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let len = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let mut pb = vec![0u8; len as usize];
    if read_bytes(store, ptr, &mut pb).is_err() { return Ok(vec![Value::I32(21)]); }
    let cp = String::from_utf8_lossy(&pb).into_owned().trim_start_matches('.').trim_start_matches('/').to_string();
    match fs::remove_file(&format!("@0xE0/{}", cp)) { Ok(_) => Ok(vec![Value::I32(0)]), Err(_) => Ok(vec![Value::I32(28)]) }
}

fn path_rename<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let o_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let o_len = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let n_ptr = match args.get(4) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let n_len = match args.get(5) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let mut ob = vec![0u8; o_len as usize]; let mut nb = vec![0u8; n_len as usize];
    if read_bytes(store, o_ptr, &mut ob).is_err() || read_bytes(store, n_ptr, &mut nb).is_err() { return Ok(vec![Value::I32(21)]); }
    let co = String::from_utf8_lossy(&ob).into_owned().trim_start_matches('.').trim_start_matches('/').to_string();
    let cn = String::from_utf8_lossy(&nb).into_owned().trim_start_matches('.').trim_start_matches('/').to_string();
    match fs::rename(&format!("@0xE0/{}", co), &format!("@0xE0/{}", cn)) { Ok(_) => Ok(vec![Value::I32(0)]), Err(_) => Ok(vec![Value::I32(28)]) }
}

fn path_readlink<T: Config>(_: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> { Ok(vec![Value::I32(58)]) }

fn proc_exit<T: Config>(_: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let exit_code = match args.get(0) { Some(Value::I32(v)) => *v as i32, _ => 0 };
    debugln!("WASI: proc_exit({})", exit_code);
    Err(HaltExecutionError)
}

fn fd_readdir<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let fd = match args.get(0) { Some(Value::I32(v)) => *v as i32, _ => -1 };
    let b_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let b_len = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let ck = match args.get(3) { Some(Value::I64(v)) => *v as u64, _ => 0 };
    let u_ptr = match args.get(4) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let mut entries = Vec::new();
    let p = if fd == 3 || fd == 4 { "/" } else { "." };
    match crate::fs::read_dir(p) { Ok(re) => { for e in re { let wt = match e.file_type { crate::fs::FileType::File => 4, crate::fs::FileType::Directory => 3, crate::fs::FileType::Device => 2, _ => 0 }; entries.push((e.name, wt)); } } Err(_) => return Ok(vec![Value::I32(28)]), }
    let mut used = 0;
    if ck < entries.len() as u64 {
        for (i, (name, ft)) in entries.iter().enumerate().skip(ck as usize) {
            let nb = name.as_bytes(); let nl = nb.len(); let es = 24 + nl;
            if (used + es) > b_len as usize { used = b_len as usize; break; }
            let eb = b_ptr + used as u32; let nc = (i + 1) as u64;
            if write_u64(store, eb, nc).is_err() || write_u64(store, eb + 8, (i + 1024) as u64).is_err() || write_u32(store, eb + 16, nl as u32).is_err() || write_bytes(store, eb + 20, &[*ft, 0, 0, 0]).is_err() || write_bytes(store, eb + 24, nb).is_err() { return Ok(vec![Value::I32(28)]); }
            used += es;
        }
    }
    if u_ptr != 0 { let _ = write_u32(store, u_ptr, used as u32); }
    Ok(vec![Value::I32(0)])
}