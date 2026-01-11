use crate::rust_alloc::vec::Vec;
use crate::rust_alloc::vec;
use crate::rust_alloc::string::String;
use crate::rust_alloc::string::ToString;
use crate::rust_alloc::rc::Rc;
use core::cell::RefCell;
use crate::wasm::runtime::{Store, Value, HostFunc, FuncInstance, ModuleInstance, ExportInstance, ExternalVal, MemoryInstance};
use crate::wasm::types::{FuncType, ValType, FunctionBody};

fn read_memory(mem: &MemoryInstance, addr: usize, len: usize) -> Option<&[u8]> {
    if addr + len > mem.data.len() { return None; }
    Some(&mem.data[addr..addr+len])
}

fn read_u32(mem: &MemoryInstance, addr: usize) -> Option<u32> {
    let bytes = read_memory(mem, addr, 4)?;
    Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn write_u32(mem: &mut MemoryInstance, addr: usize, val: u32) -> bool {
    if addr + 4 > mem.data.len() { return false; }
    mem.data[addr..addr+4].copy_from_slice(&val.to_le_bytes());
    true
}

// --- WASI Preview 1 Adapters ---
fn wasi_fd_write(store: &mut Store, args: &[Value]) -> Vec<Value> {
    if args.len() != 4 { return vec![Value::I32(28)]; }
    let fd = match args[0] { Value::I32(x) => x as u32, _ => return vec![Value::I32(28)] };
    let iovs_ptr = match args[1] { Value::I32(x) => x as usize, _ => return vec![Value::I32(28)] };
    let iovs_len = match args[2] { Value::I32(x) => x as usize, _ => return vec![Value::I32(28)] };
    let nwritten_ptr = match args[3] { Value::I32(x) => x as usize, _ => return vec![Value::I32(28)] };
    if store.memories.is_empty() { return vec![Value::I32(9)]; }
    
    let mut total_written = 0;
    for i in 0..iovs_len {
        let iov_ptr = iovs_ptr + i * 8;
        let buf_ptr = match read_u32(&store.memories[0], iov_ptr) { Some(x) => x as usize, None => return vec![Value::I32(21)] };
        let buf_len = match read_u32(&store.memories[0], iov_ptr + 4) { Some(x) => x as usize, None => return vec![Value::I32(21)] };
        
        let mem = &mut store.memories[0];
        if let Some(buf) = read_memory(mem, buf_ptr, buf_len) {
            if fd == 1 || fd == 2 {
                 if let Ok(s) = core::str::from_utf8(buf) { crate::print!("{}", s); }
                 else { for b in buf { crate::print!("{}", *b as char); } }
                 total_written += buf_len;
            } else if let Some(&host_fd) = store.wasi.files.get(&fd) {
                let res = unsafe { crate::os::syscall(1, host_fd as u64, buf.as_ptr() as u64, buf.len() as u64) };
                if res != u64::MAX { total_written += res as usize; }
            }
        }
    }
    let mem = &mut store.memories[0];
    write_u32(mem, nwritten_ptr, total_written as u32);
    vec![Value::I32(0)]
}

fn wasi_fd_read(store: &mut Store, args: &[Value]) -> Vec<Value> {
    if args.len() != 4 { return vec![Value::I32(28)]; }
    let fd = match args[0] { Value::I32(x) => x as u32, _ => return vec![Value::I32(28)] };
    let iovs_ptr = match args[1] { Value::I32(x) => x as usize, _ => return vec![Value::I32(28)] };
    let iovs_len = match args[2] { Value::I32(x) => x as usize, _ => return vec![Value::I32(28)] };
    let nread_ptr = match args[3] { Value::I32(x) => x as usize, _ => return vec![Value::I32(28)] };
    if store.memories.is_empty() { return vec![Value::I32(9)]; }

    let mut total_read = 0;
    if let Some(&host_fd) = store.wasi.files.get(&fd) {
        for i in 0..iovs_len {
            let iov_ptr = iovs_ptr + i * 8;
            let buf_ptr = match read_u32(&store.memories[0], iov_ptr) { Some(x) => x as usize, None => return vec![Value::I32(21)] };
            let buf_len = match read_u32(&store.memories[0], iov_ptr + 4) { Some(x) => x as usize, None => return vec![Value::I32(21)] };
            
            let mem = &mut store.memories[0];
            if buf_ptr + buf_len <= mem.data.len() {
                let res = unsafe { crate::os::syscall(0, host_fd as u64, mem.data[buf_ptr..].as_mut_ptr() as u64, buf_len as u64) };
                if res != u64::MAX { 
                    total_read += res as usize; 
                    if (res as usize) < buf_len { break; }
                }
            }
        }
    }
    let mem = &mut store.memories[0];
    write_u32(mem, nread_ptr, total_read as u32);
    vec![Value::I32(0)]
}

fn wasi_fd_seek(store: &mut Store, args: &[Value]) -> Vec<Value> {
    if args.len() != 4 { return vec![Value::I32(28)]; }
    let fd = match args[0] { Value::I32(x) => x as u32, _ => return vec![Value::I32(28)] };
    let offset = match args[1] { Value::I64(x) => x, _ => return vec![Value::I32(28)] };
    let whence = match args[2] { Value::I32(x) => x as u32, _ => return vec![Value::I32(28)] };
    let newoffset_ptr = match args[3] { Value::I32(x) => x as usize, _ => return vec![Value::I32(28)] };
    
    if let Some(&host_fd) = store.wasi.files.get(&fd) {
        let res = unsafe { crate::os::syscall(8, host_fd as u64, offset as u64, whence as u64) };
        if res != u64::MAX {
            let mem = &mut store.memories[0];
            if newoffset_ptr + 8 <= mem.data.len() {
                mem.data[newoffset_ptr..newoffset_ptr+8].copy_from_slice(&res.to_le_bytes());
            }
            return vec![Value::I32(0)];
        }
    }
    vec![Value::I32(21)]
}

fn wasi_fd_close(store: &mut Store, args: &[Value]) -> Vec<Value> {
    if args.len() != 1 { return vec![Value::I32(28)]; }
    let fd = match args[0] { Value::I32(x) => x as u32, _ => return vec![Value::I32(28)] };
    if let Some(host_fd) = store.wasi.files.remove(&fd) {
        crate::os::file_close(host_fd);
        return vec![Value::I32(0)];
    }
    vec![Value::I32(8)] // EBADF
}

fn wasi_path_open(store: &mut Store, args: &[Value]) -> Vec<Value> {
    if args.len() < 9 { return vec![Value::I32(28)]; }
    let path_ptr = match args[2] { Value::I32(x) => x as usize, _ => return vec![Value::I32(28)] };
    let path_len = match args[3] { Value::I32(x) => x as usize, _ => return vec![Value::I32(28)] };
    let ret_fd_ptr = match args[8] { Value::I32(x) => x as usize, _ => return vec![Value::I32(28)] };
    
    if let Some(path_bytes) = read_memory(&store.memories[0], path_ptr, path_len) {
        if let Ok(path) = core::str::from_utf8(path_bytes) {
            let mut actual_path = String::from(path);
            if actual_path.starts_with('/') { actual_path = String::from("@0xE0") + &actual_path; }
            else if !actual_path.starts_with('@') { actual_path = String::from("@0xE0/") + &actual_path; }
            
            let res = unsafe { crate::os::syscall(2, actual_path.as_ptr() as u64, actual_path.len() as u64, 0) };
            if res != u64::MAX {
                let wasi_fd = (3..1000).find(|i| !store.wasi.files.contains_key(i)).unwrap_or(100);
                store.wasi.files.insert(wasi_fd, res as usize);
                write_u32(&mut store.memories[0], ret_fd_ptr, wasi_fd);
                return vec![Value::I32(0)];
            }
        }
    }
    vec![Value::I32(44)] 
}

fn wasi_random_get(store: &mut Store, args: &[Value]) -> Vec<Value> {
    if args.len() != 2 { return vec![Value::I32(28)]; }
    let ptr = match args[0] { Value::I32(x) => x as usize, _ => return vec![Value::I32(28)] };
    let len = match args[1] { Value::I32(x) => x as usize, _ => return vec![Value::I32(28)] };
    if store.memories.is_empty() { return vec![Value::I32(9)]; }
    let mem = &mut store.memories[0];
    if ptr + len <= mem.data.len() {
        let mut seed = crate::os::get_system_ticks();
        for i in 0..len {
            seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            mem.data[ptr + i] = (seed >> 16) as u8;
        }
    }
    vec![Value::I32(0)]
}

fn wasi_environ_sizes_get(store: &mut Store, args: &[Value]) -> Vec<Value> {
    if args.len() != 2 { return vec![Value::I32(28)]; }
    let count_ptr = match args[0] { Value::I32(x) => x as usize, _ => return vec![Value::I32(28)] };
    let size_ptr = match args[1] { Value::I32(x) => x as usize, _ => return vec![Value::I32(28)] };
    if let Some(mem) = store.memories.get_mut(0) {
        write_u32(mem, count_ptr, 0); 
        write_u32(mem, size_ptr, 0);
        return vec![Value::I32(0)];
    }
    vec![Value::I32(9)]
}

fn wasi_environ_get(_store: &mut Store, args: &[Value]) -> Vec<Value> {
    if args.len() != 2 { return vec![Value::I32(28)]; }
    vec![Value::I32(0)]
}

fn wasi_args_sizes_get(store: &mut Store, args: &[Value]) -> Vec<Value> {
    if args.len() != 2 { return vec![Value::I32(28)]; }
    let count_ptr = match args[0] { Value::I32(x) => x as usize, _ => return vec![Value::I32(28)] };
    let size_ptr = match args[1] { Value::I32(x) => x as usize, _ => return vec![Value::I32(28)] };
    if let Some(mem) = store.memories.get_mut(0) {
        let count = store.wasi.args.len() as u32;
        let mut total_size = 0;
        for arg in &store.wasi.args { total_size += arg.len() + 1; }
        write_u32(mem, count_ptr, count);
        write_u32(mem, size_ptr, total_size as u32);
        return vec![Value::I32(0)];
    }
    vec![Value::I32(9)]
}

fn wasi_args_get(store: &mut Store, args: &[Value]) -> Vec<Value> {
    if args.len() != 2 { return vec![Value::I32(28)]; }
    let argv_ptr = match args[0] { Value::I32(x) => x as usize, _ => return vec![Value::I32(28)] };
    let argv_buf_ptr = match args[1] { Value::I32(x) => x as usize, _ => return vec![Value::I32(28)] };
    
    if let Some(mem) = store.memories.get_mut(0) {
        let mut current_argv_ptr = argv_ptr;
        let mut current_argv_buf_ptr = argv_buf_ptr;
        for arg in &store.wasi.args {
            write_u32(mem, current_argv_ptr, current_argv_buf_ptr as u32);
            current_argv_ptr += 4;
            let bytes = arg.as_bytes();
            if current_argv_buf_ptr + bytes.len() + 1 <= mem.data.len() {
                mem.data[current_argv_buf_ptr..current_argv_buf_ptr + bytes.len()].copy_from_slice(bytes);
                mem.data[current_argv_buf_ptr + bytes.len()] = 0;
                current_argv_buf_ptr += bytes.len() + 1;
            }
        }
        return vec![Value::I32(0)];
    }
    vec![Value::I32(9)]
}

fn wasi_fd_prestat_get(store: &mut Store, args: &[Value]) -> Vec<Value> {
    if args.len() != 2 { return vec![Value::I32(28)]; }
    let fd = match args[0] { Value::I32(x) => x, _ => return vec![Value::I32(28)] };
    if fd == 3 { 
        let ptr = match args[1] { Value::I32(x) => x as usize, _ => return vec![Value::I32(28)] };
        if let Some(mem) = store.memories.get_mut(0) {
            if ptr + 8 <= mem.data.len() {
                mem.data[ptr] = 0; // prestat_dir (tag 0)
                write_u32(mem, ptr + 4, 1); // pr_name_len (1 for "/")
                return vec![Value::I32(0)];
            }
        }
    }
    vec![Value::I32(8)] 
}

fn wasi_fd_prestat_dir_name(store: &mut Store, args: &[Value]) -> Vec<Value> {
    if args.len() != 3 { return vec![Value::I32(28)]; }
    let fd = match args[0] { Value::I32(x) => x, _ => return vec![Value::I32(28)] };
    let path_ptr = match args[1] { Value::I32(x) => x as usize, _ => return vec![Value::I32(28)] };
    let path_len = match args[2] { Value::I32(x) => x as usize, _ => return vec![Value::I32(28)] };
    if fd == 3 && path_len >= 1 {
        if let Some(mem) = store.memories.get_mut(0) {
            if path_ptr < mem.data.len() {
                mem.data[path_ptr] = b'/';
                return vec![Value::I32(0)];
            }
        }
    }
    vec![Value::I32(8)]
}

fn wasi_proc_exit(_store: &mut Store, args: &[Value]) -> Vec<Value> {
    let code = match args.get(0) { Some(Value::I32(x)) => *x, _ => 0 };
    crate::os::exit(code as u64);
}

// --- Preview 2 Mocks ---
fn wasi_cli_stdout_get_stdout(_store: &mut Store, _args: &[Value]) -> Vec<Value> { vec![Value::I32(1)] }
fn wasi_cli_stderr_get_stderr(_store: &mut Store, _args: &[Value]) -> Vec<Value> { vec![Value::I32(2)] }

fn wasi_random_get_random_bytes(store: &mut Store, args: &[Value]) -> Vec<Value> {
    if args.len() != 2 { return vec![]; }
    let len = match args[0] { Value::I32(x) => x as usize, _ => return vec![] };
    let ret_ptr = match args[1] { Value::I32(x) => x as usize, _ => return vec![] };
    if store.memories.is_empty() { return vec![]; }
    let mem = &mut store.memories[0];
    if ret_ptr + len <= mem.data.len() {
        let mut seed = crate::os::get_system_ticks();
        for i in 0..len {
            seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            mem.data[ret_ptr + i] = (seed >> 16) as u8;
        }
    }
    vec![]
}

fn wasi_io_streams_blocking_write_and_flush(store: &mut Store, args: &[Value]) -> Vec<Value> {
    if args.len() < 3 { return vec![]; }
    let fd = match args[0] { Value::I32(x) => x as u32, _ => return vec![] };
    let ptr = match args[1] { Value::I32(x) => x as usize, _ => return vec![] };
    let len = match args[2] { Value::I32(x) => x as usize, _ => return vec![] };
    if store.memories.is_empty() { return vec![]; }
    let mem = &mut store.memories[0];
    let mut ret_ptr_opt = None;
    if args.len() >= 4 {
        if let Value::I32(ret_ptr) = args[3] {
            let rp = ret_ptr as usize;
            if rp + 4 <= mem.data.len() { ret_ptr_opt = Some(rp); }
        }
    }
    if let Some(buf) = read_memory(mem, ptr, len) {
        if fd == 1 || fd == 2 {
             if let Ok(s) = core::str::from_utf8(buf) { crate::print!("{}", s); }
             else { for b in buf { crate::print!("{}", *b as char); } }
        } else if let Some(&host_fd) = store.wasi.files.get(&fd) {
             unsafe { crate::os::syscall(1, host_fd as u64, buf.as_ptr() as u64, buf.len() as u64) };
        }
    }
    if let Some(ret_ptr) = ret_ptr_opt { write_u32(&mut store.memories[0], ret_ptr, 0); }
    vec![]
}

fn wasi_clocks_wall_clock_now(store: &mut Store, args: &[Value]) -> Vec<Value> {
    if args.len() != 1 { return vec![]; }
    let ret_ptr = match args[0] { Value::I32(x) => x as usize, _ => return vec![] };
    let ticks = crate::os::get_system_ticks(); 
    let seconds = 1700000000 + (ticks / 1000);
    let nanos = (ticks % 1000) * 1_000_000;
    if store.memories.is_empty() { return vec![]; }
    let mem = &mut store.memories[0];
    if ret_ptr + 12 <= mem.data.len() {
        mem.data[ret_ptr..ret_ptr+8].copy_from_slice(&seconds.to_le_bytes());
        mem.data[ret_ptr+8..ret_ptr+12].copy_from_slice(&(nanos as u32).to_le_bytes());
    }
    vec![]
}

fn wasi_cli_environment_get_arguments(store: &mut Store, args: &[Value]) -> Vec<Value> {
    if args.len() != 1 { return vec![]; }
    let ret_ptr = match args[0] { Value::I32(x) => x as usize, _ => return vec![] };
    if store.memories.is_empty() { return vec![]; }
    let mem = &mut store.memories[0];
    if ret_ptr + 8 <= mem.data.len() { mem.data[ret_ptr..ret_ptr+8].copy_from_slice(&0u64.to_le_bytes()); }
    vec![]
}

fn stub_resource_drop(_store: &mut Store, _args: &[Value]) -> Vec<Value> { vec![] }

pub fn create_wasi_module(store: &mut Store) -> Rc<ModuleInstance> {
    let mut exports = Vec::new();
    let mut func_addrs = Vec::new();
    let mut add_func = |name: &str, params: Vec<ValType>, results: Vec<ValType>, func: HostFunc| {
        let ty = FuncType { params, results };
        store.funcs.push(FuncInstance { ty, module: None, code: None, host_code: Some(func) });
        let addr = (store.funcs.len() - 1) as u32;
        func_addrs.push(addr);
        exports.push(ExportInstance { name: name.to_string(), value: ExternalVal::Func(addr) });
    };

    add_func("fd_write", vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32], vec![ValType::I32], wasi_fd_write);
    add_func("fd_read", vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32], vec![ValType::I32], wasi_fd_read);
    add_func("random_get", vec![ValType::I32, ValType::I32], vec![ValType::I32], wasi_random_get);
    add_func("fd_close", vec![ValType::I32], vec![ValType::I32], wasi_fd_close);
    add_func("fd_readdir", vec![ValType::I32, ValType::I32, ValType::I32, ValType::I64], vec![ValType::I32], |s, a| vec![Value::I32(0)]); 
    add_func("fd_filestat_get", vec![ValType::I32, ValType::I32], vec![ValType::I32], |s, a| vec![Value::I32(0)]);
    add_func("fd_seek", vec![ValType::I32, ValType::I64, ValType::I32, ValType::I32], vec![ValType::I32], wasi_fd_seek);
    add_func("path_open", vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32, ValType::I32, ValType::I64, ValType::I64, ValType::I32, ValType::I32], vec![ValType::I32], wasi_path_open);
    add_func("proc_exit", vec![ValType::I32], vec![], wasi_proc_exit);
    add_func("environ_get", vec![ValType::I32, ValType::I32], vec![ValType::I32], wasi_environ_get);
    add_func("environ_sizes_get", vec![ValType::I32, ValType::I32], vec![ValType::I32], wasi_environ_sizes_get);
    add_func("args_get", vec![ValType::I32, ValType::I32], vec![ValType::I32], wasi_args_get);
    add_func("args_sizes_get", vec![ValType::I32, ValType::I32], vec![ValType::I32], wasi_args_sizes_get);
    add_func("fd_prestat_get", vec![ValType::I32, ValType::I32], vec![ValType::I32], wasi_fd_prestat_get);
    add_func("fd_prestat_dir_name", vec![ValType::I32, ValType::I32, ValType::I32], vec![ValType::I32], wasi_fd_prestat_dir_name);      
    add_func("adapter_close_badfd", vec![ValType::I32], vec![ValType::I32], |s, a| vec![Value::I32(0)]);
    add_func("fd_fdstat_get", vec![ValType::I32, ValType::I32], vec![ValType::I32], |s, a| vec![Value::I32(0)]);

    add_func("get-stdout", vec![], vec![ValType::I32], wasi_cli_stdout_get_stdout);
    add_func("get-stderr", vec![], vec![ValType::I32], wasi_cli_stderr_get_stderr);
    add_func("get-random-bytes", vec![ValType::I32, ValType::I32], vec![], wasi_random_get_random_bytes);
    add_func("sock_send", vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32, ValType::I32], vec![ValType::I32], |s, a| vec![Value::I32(52)]); 
    add_func("sock_recv", vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32, ValType::I32, ValType::I32], vec![ValType::I32], |s, a| vec![Value::I32(52)]);
    add_func("sock_shutdown", vec![ValType::I32, ValType::I32], vec![ValType::I32], |s, a| vec![Value::I32(52)]);
    add_func("poll_oneoff", vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32], vec![ValType::I32], |s, a| vec![Value::I32(52)]);
    add_func("[method]output-stream.blocking-write-and-flush", vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32], vec![], wasi_io_streams_blocking_write_and_flush);    
    add_func("now", vec![ValType::I32], vec![], wasi_clocks_wall_clock_now);
    add_func("get-arguments", vec![ValType::I32], vec![], wasi_cli_environment_get_arguments);
    add_func("get-environment", vec![ValType::I32], vec![], |s, a| {
        if let Some(Value::I32(ret_ptr)) = a.get(0) {
            if let Some(mem) = s.memories.get_mut(0) {
                let rp = *ret_ptr as usize;
                if rp + 8 <= mem.data.len() { mem.data[rp..rp+8].copy_from_slice(&0u64.to_le_bytes()); }
            }
        }
        vec![]
    });
    add_func("initial-cwd", vec![ValType::I32], vec![], |s, a| {
        if let Some(Value::I32(ret_ptr)) = a.get(0) {
            if let Some(mem) = s.memories.get_mut(0) {
                let rp = *ret_ptr as usize;
                if rp + 8 <= mem.data.len() { mem.data[rp..rp+8].copy_from_slice(&0u64.to_le_bytes()); }
            }
        }
        vec![]
    });

    for name in &["[resource-drop]error", "[resource-drop]output-stream", "[resource-drop]input-stream", "[resource-drop]pollable", "[resource-drop]udp-socket", "[resource-drop]incoming-datagram-stream", "[resource-drop]outgoing-datagram-stream", "[resource-drop]tcp-socket"] {
        add_func(name, vec![ValType::I32], vec![], stub_resource_drop);
    }
    add_func("[method]error.to-debug-string", vec![ValType::I32, ValType::I32], vec![], |s, a| vec![]);

    Rc::new(ModuleInstance { 
        func_addrs, 
        table_addrs: Vec::new(), 
        mem_addrs: Vec::new(), 
        global_addrs: Vec::new(), 
        data_segments: RefCell::new(Vec::new()), 
        element_segments: RefCell::new(Vec::new()),
        exports 
    })
}