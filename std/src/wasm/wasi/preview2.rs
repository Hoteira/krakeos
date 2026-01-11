use crate::rust_alloc::vec::Vec;
use crate::rust_alloc::vec;
use crate::rust_alloc::string::String;
use crate::rust_alloc::string::ToString;
use crate::rust_alloc::rc::Rc;
use crate::wasm::runtime::{Store, Value, HostFunc, FuncInstance, ModuleInstance, ExportInstance, ExternalVal, MemoryInstance};
use crate::wasm::types::{FuncType, ValType, FunctionBody};

pub struct WasiCtx {
    pub env: Vec<(String, String)>,
    pub args: Vec<String>,
}

impl WasiCtx {
    pub fn new() -> Self {
        Self {
            env: Vec::new(),
            args: Vec::new(),
        }
    }
}

fn read_memory(mem: &MemoryInstance, addr: usize, len: usize) -> Option<&[u8]> {
    if addr + len > mem.data.len() { return None; }
    Some(&mem.data[addr..addr+len])
}

fn read_u32(mem: &MemoryInstance, addr: usize) -> Option<u32> {
    let bytes = read_memory(mem, addr, 4)?;
    Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

// --- WASI Preview 1 Adapters ---
fn wasi_fd_write(store: &mut Store, args: &[Value]) -> Vec<Value> {
    if args.len() != 4 { return vec![Value::I32(28)]; }
    let fd = match args[0] { Value::I32(x) => x, _ => return vec![Value::I32(28)] };
    let iovs_ptr = match args[1] { Value::I32(x) => x as usize, _ => return vec![Value::I32(28)] };
    let iovs_len = match args[2] { Value::I32(x) => x as usize, _ => return vec![Value::I32(28)] };
    let nwritten_ptr = match args[3] { Value::I32(x) => x as usize, _ => return vec![Value::I32(28)] };
    if store.memories.is_empty() { return vec![Value::I32(9)]; }
    let mem = &mut store.memories[0];
    let mut total_written = 0;
    for i in 0..iovs_len {
        let iov_ptr = iovs_ptr + i * 8;
        let buf_ptr = match read_u32(mem, iov_ptr) { Some(x) => x as usize, None => return vec![Value::I32(21)] };
        let buf_len = match read_u32(mem, iov_ptr + 4) { Some(x) => x as usize, None => return vec![Value::I32(21)] };
        if let Some(buf) = read_memory(mem, buf_ptr, buf_len) {
            if fd == 1 || fd == 2 {
                 if let Ok(s) = core::str::from_utf8(buf) { crate::print!("{}", s); }
                 else { for b in buf { crate::print!("{}", *b as char); } }
                 total_written += buf_len;
            }
        }
    }
    if nwritten_ptr + 4 <= mem.data.len() {
        let bytes = (total_written as u32).to_le_bytes();
        mem.data[nwritten_ptr..nwritten_ptr+4].copy_from_slice(&bytes);
    }
    vec![Value::I32(0)]
}

fn wasi_proc_exit(_store: &mut Store, args: &[Value]) -> Vec<Value> {
    let code = match args.get(0) { Some(Value::I32(x)) => *x, _ => 0 };
    crate::os::exit(code as u64);
}

// --- Preview 2 Mocks ---
fn wasi_cli_stdout_get_stdout(_store: &mut Store, _args: &[Value]) -> Vec<Value> { vec![Value::I32(1)] }
fn wasi_cli_stderr_get_stderr(_store: &mut Store, _args: &[Value]) -> Vec<Value> { vec![Value::I32(2)] }

fn wasi_io_streams_blocking_write_and_flush(store: &mut Store, args: &[Value]) -> Vec<Value> {
    // Standard P2 Lowering: (handle, ptr, len, ret_ptr) -> ()
    if args.len() < 3 { return vec![]; }
    let fd = match args[0] { Value::I32(x) => x, _ => return vec![] };
    let ptr = match args[1] { Value::I32(x) => x as usize, _ => return vec![] };
    let len = match args[2] { Value::I32(x) => x as usize, _ => return vec![] };
    
    if store.memories.is_empty() { return vec![]; }
    let mem = &mut store.memories[0];
    if let Some(buf) = read_memory(mem, ptr, len) {
        if fd == 1 || fd == 2 {
             if let Ok(s) = core::str::from_utf8(buf) { crate::print!("{}", s); }
             else { for b in buf { crate::print!("{}", *b as char); } }
        }
    }
    
    // If there's a 4th argument, it's the result pointer
    if args.len() >= 4 {
        if let Value::I32(ret_ptr) = args[3] {
            let ret_ptr = ret_ptr as usize;
            if ret_ptr + 4 <= mem.data.len() {
                mem.data[ret_ptr..ret_ptr+4].copy_from_slice(&0u32.to_le_bytes()); // Success (tag 0)
            }
        }
    }
    
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
    if ret_ptr + 8 <= mem.data.len() {
        mem.data[ret_ptr..ret_ptr+8].copy_from_slice(&0u64.to_le_bytes()); 
    }
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
    add_func("fd_read", vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32], vec![ValType::I32], |s, a| vec![Value::I32(0)]);
    add_func("fd_close", vec![ValType::I32], vec![ValType::I32], |s, a| vec![Value::I32(0)]);
    add_func("fd_readdir", vec![ValType::I32, ValType::I32, ValType::I32, ValType::I64], vec![ValType::I32], |s, a| vec![Value::I32(0)]);
    add_func("fd_filestat_get", vec![ValType::I32, ValType::I32], vec![ValType::I32], |s, a| vec![Value::I32(0)]);
    add_func("path_open", vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32, ValType::I32, ValType::I64, ValType::I64, ValType::I32, ValType::I32], vec![ValType::I32], |s, a| vec![Value::I32(0)]);
    add_func("proc_exit", vec![ValType::I32], vec![], wasi_proc_exit);
    add_func("environ_get", vec![ValType::I32, ValType::I32], vec![ValType::I32], |s, a| vec![Value::I32(0)]);
    add_func("environ_sizes_get", vec![ValType::I32, ValType::I32], vec![ValType::I32], |s, a| vec![Value::I32(0)]);
    add_func("fd_prestat_get", vec![ValType::I32, ValType::I32], vec![ValType::I32], |s, a| vec![Value::I32(8)]);
    add_func("fd_prestat_dir_name", vec![ValType::I32, ValType::I32, ValType::I32], vec![ValType::I32], |s, a| vec![Value::I32(0)]);
    add_func("adapter_close_badfd", vec![ValType::I32], vec![ValType::I32], |s, a| vec![Value::I32(0)]);
    add_func("fd_fdstat_get", vec![ValType::I32, ValType::I32], vec![ValType::I32], |s, a| vec![Value::I32(0)]);

    add_func("get-stdout", vec![], vec![ValType::I32], wasi_cli_stdout_get_stdout);
    add_func("get-stderr", vec![], vec![ValType::I32], wasi_cli_stderr_get_stderr);
    
    // Support both 3 and 4 argument variants of blocking-write-and-flush to be extremely robust
    add_func("[method]output-stream.blocking-write-and-flush", vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32], vec![], wasi_io_streams_blocking_write_and_flush);
    
    add_func("now", vec![ValType::I32], vec![], wasi_clocks_wall_clock_now);
    add_func("get-arguments", vec![ValType::I32], vec![], wasi_cli_environment_get_arguments);
    add_func("get-environment", vec![ValType::I32], vec![], |s, a| vec![]);
    add_func("initial-cwd", vec![ValType::I32], vec![], |s, a| vec![]);

    for name in &["[resource-drop]error", "[resource-drop]output-stream", "[resource-drop]input-stream", "[resource-drop]pollable", "[resource-drop]udp-socket", "[resource-drop]incoming-datagram-stream", "[resource-drop]outgoing-datagram-stream", "[resource-drop]tcp-socket"] {
        add_func(name, vec![ValType::I32], vec![], stub_resource_drop);
    }
    add_func("[method]error.to-debug-string", vec![ValType::I32, ValType::I32], vec![], |s, a| vec![]);

    Rc::new(ModuleInstance { func_addrs, table_addrs: Vec::new(), mem_addrs: Vec::new(), global_addrs: Vec::new(), exports })
}
