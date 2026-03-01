use crate::rust_alloc::{string::String, vec, vec::Vec, string::ToString};
use crate::wasm::{
    common::{config::Config, value::Value, reader::types::{ValType, NumType}},
    interpreter::store::{HaltExecutionError, Store},
};
use crate::wasm::wasi::preview2::{read_mem, read_mem_u32, read_mem_string, write_bytes};
use crate::os::krakeos as host;

crate::export_method!(
    "wasi:cli/exit@0.2.0", "exit",
    [("env", "__wasi_proc_exit")],
    vec![ValType::NumType(NumType::I32)], vec![],
    pub fn exit<T: Config>(_: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let code = match args.get(0) { Some(Value::I32(v)) => *v as i32, _ => 0 };
        Err(HaltExecutionError(code))
    }
);

crate::export_method!(
    "krakeos:system/process@0.2.0", "spawn",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I64)],
    pub fn spawn<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let get_arg = |i: usize| -> u32 { match args.get(i) { Some(Value::I32(v)) => *v as u32, _ => 0 } };
        let path_ptr = get_arg(0);
        let path_len = get_arg(1);
        let argv_ptr = get_arg(2);
        let argv_len = get_arg(3);
        let fds_ptr = get_arg(4);
        let fds_len = get_arg(5);

        let mut path_buf = vec![0u8; path_len as usize];
        read_mem(store, path_ptr, &mut path_buf).map_err(|_| HaltExecutionError(1))?;
        let path = String::from_utf8_lossy(&path_buf);

        let mut host_args = Vec::new();
        for i in 0..argv_len {
            let arg_ptr = read_mem_u32(store, argv_ptr + i * 4)? as u32;
            host_args.push(read_mem_string(store, arg_ptr)?);
        }
        let host_args_refs: Vec<&str> = host_args.iter().map(|s| s.as_str()).collect();

        let mut host_fds = Vec::new();
        for i in 0..fds_len {
            let mut buf = [0u8; 2];
            read_mem(store, fds_ptr + i * 2, &mut buf).map_err(|_| HaltExecutionError(1))?;
            host_fds.push((buf[0], buf[1]));
        }

        let pid = host::spawn_with_fds(&path, &host_args_refs, &host_fds);
        Ok(vec![Value::I64(pid as u64)])
    }
);

crate::export_method!(
    "krakeos:system/process@0.2.0", "waitpid",
    [],
    vec![ValType::NumType(NumType::I64)], vec![ValType::NumType(NumType::I32)],
    pub fn waitpid<T: Config>(_: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let pid = match args.get(0) { Some(Value::I64(v)) => *v as u64, _ => 0 };
        let res = host::waitpid(pid);
        Ok(vec![Value::I32(res as u32)])
    }
);

crate::export_method!(
    "krakeos:system/process@0.2.0", "pipe",
    [],
    vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn pipe<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let ptr = match args.get(0) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![Value::I32((-1i32) as u32)]) };
        
        let mut fds = [0i32; 2];
        let res = host::pipe(&mut fds);
        if res == 0 {
            let mut bytes = [0u8; 8];
            bytes[0..4].copy_from_slice(&fds[0].to_le_bytes());
            bytes[4..8].copy_from_slice(&fds[1].to_le_bytes());
            write_bytes(store, ptr, &bytes).map_err(|_| HaltExecutionError(1))?;
            Ok(vec![Value::I32(0)])
        } else { Ok(vec![Value::I32((-1i32) as u32)]) }
    }
);

crate::export_method!(
    "krakeos:system/process@0.2.0", "yield",
    [],
    vec![], vec![],
    pub fn yield_host<T: Config>(_: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        host::yield_task();
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi_snapshot_preview1", "proc_exit",
    [],
    vec![ValType::NumType(NumType::I32)], vec![],
    pub fn proc_exit_p1<T: Config>(_: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let code = match args.get(0) { Some(Value::I32(v)) => *v as i32, _ => 0 };
        Err(HaltExecutionError(code))
    }
);

crate::export_method!(
    "wasi_snapshot_preview1", "sched_yield",
    [],
    vec![], vec![ValType::NumType(NumType::I32)],
    pub fn sched_yield_p1<T: Config>(_: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        host::yield_task();
        Ok(vec![Value::I32(0)])
    }
);

pub fn register_wasi<T: Config>(linker: &mut crate::wasm::Linker, store: &mut crate::wasm::Store<'_, T>) {
    exit::register(linker, store);
    spawn::register(linker, store);
    waitpid::register(linker, store);
    pipe::register(linker, store);
    yield_host::register(linker, store);
    proc_exit_p1::register(linker, store);
    sched_yield_p1::register(linker, store);
}
