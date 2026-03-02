use crate::alloc::{vec, vec::Vec, string::String};
use crate::wasm::{
    common::{config::Config, value::Value, reader::types::{ValType, NumType}},
    interpreter::store::{HaltExecutionError, Store},
};
use crate::wasm::wasi::preview2::{call_cabi_realloc, write_bytes, write_u32};

crate::export_method!(
    "wasi:cli/environment@0.2.0", "get-environment",
    [],
    vec![ValType::NumType(NumType::I32)], vec![],
    pub fn get_environment<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let ret_ptr = match args.get(0) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![]) };
        let env_vars = store.wasi_ctx.as_ref().ok_or(HaltExecutionError(1))?.env.environ_get().unwrap_or_default();
        let count = env_vars.len() as u32;
        let array_ptr = if count > 0 { call_cabi_realloc(store, count * 16, 4)? } else { 0 };
        for (i, (k, v)) in env_vars.into_iter().enumerate() {
            let k_bytes = k.as_bytes();
            let v_bytes = v.as_bytes();
            let k_ptr = call_cabi_realloc(store, k_bytes.len() as u32, 1)?;
            let _ = write_bytes(store, k_ptr, k_bytes);
            let v_ptr = call_cabi_realloc(store, v_bytes.len() as u32, 1)?;
            let _ = write_bytes(store, v_ptr, v_bytes);
            let tuple_off = array_ptr + (i as u32 * 16);
            let _ = write_u32(store, tuple_off, k_ptr);
            let _ = write_u32(store, tuple_off + 4, k_bytes.len() as u32);
            let _ = write_u32(store, tuple_off + 8, v_ptr);
            let _ = write_u32(store, tuple_off + 12, v_bytes.len() as u32);
        }
        let _ = write_u32(store, ret_ptr, array_ptr);
        let _ = write_u32(store, ret_ptr + 4, count);
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi:cli/environment@0.2.0", "get-arguments",
    [],
    vec![ValType::NumType(NumType::I32)], vec![],
    pub fn get_arguments<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let ret_ptr = match args.get(0) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![]) };
        let env_args = store.wasi_ctx.as_ref().ok_or(HaltExecutionError(1))?.env.args_get().unwrap_or_default();
        let count = env_args.len() as u32;
        let array_ptr = if count > 0 { call_cabi_realloc(store, count * 8, 4)? } else { 0 };
        for (i, arg) in env_args.into_iter().enumerate() {
            let bytes = arg.as_bytes();
            let s_ptr = call_cabi_realloc(store, bytes.len() as u32, 1)?;
            let _ = write_bytes(store, s_ptr, bytes);
            let struct_off = array_ptr + (i as u32 * 8);
            let _ = write_u32(store, struct_off, s_ptr);
            let _ = write_u32(store, struct_off + 4, bytes.len() as u32);
        }
        let _ = write_u32(store, ret_ptr, array_ptr);
        let _ = write_u32(store, ret_ptr + 4, count);
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi:cli/environment@0.2.0", "initial-cwd",
    [],
    vec![ValType::NumType(NumType::I32)], vec![],
    pub fn initial_cwd<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let ret_ptr = match args.get(0) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![]) };
        let cwd = store.wasi_ctx.as_ref().ok_or(HaltExecutionError(1))?.env.initial_cwd().unwrap_or_else(|_| String::from("/"));
        let ptr = call_cabi_realloc(store, cwd.len() as u32, 1)?;
        let _ = write_bytes(store, ptr, cwd.as_bytes());
        let _ = write_u32(store, ret_ptr, 1); // Some
        let _ = write_u32(store, ret_ptr + 4, ptr);
        let _ = write_u32(store, ret_ptr + 8, cwd.len() as u32);
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi_snapshot_preview1", "args_get",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn args_get<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let argv_ptr = match args.get(0) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let argv_buf_ptr = match args.get(1) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let env_args = store.wasi_ctx.as_ref().ok_or(HaltExecutionError(1))?.env.args_get().unwrap_or_default();
        let mut offset = 0;
        for (i, arg) in env_args.iter().enumerate() {
            let p = argv_buf_ptr + offset;
            let _ = write_u32(store, argv_ptr + (i as u32 * 4), p);
            let b = arg.as_bytes();
            let _ = write_bytes(store, p, b);
            let _ = write_bytes(store, p + b.len() as u32, &[0]);
            offset += b.len() as u32 + 1;
        }
        Ok(vec![Value::I32(0)])
    }
);

crate::export_method!(
    "wasi_snapshot_preview1", "args_sizes_get",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn args_sizes_get<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let c_ptr = match args.get(0) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let b_ptr = match args.get(1) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let env_args = store.wasi_ctx.as_ref().ok_or(HaltExecutionError(1))?.env.args_get().unwrap_or_default();
        let _ = write_u32(store, c_ptr, env_args.len() as u32);
        let _ = write_u32(store, b_ptr, env_args.iter().map(|s| s.len() + 1).sum::<usize>() as u32);
        Ok(vec![Value::I32(0)])
    }
);

crate::export_method!(
    "wasi_snapshot_preview1", "environ_get",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn environ_get_p1<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let e_ptr = match args.get(0) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let b_ptr = match args.get(1) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let env_vars = store.wasi_ctx.as_ref().ok_or(HaltExecutionError(1))?.env.environ_get().unwrap_or_default();
        let mut offset = 0;
        for (i, (k, v)) in env_vars.iter().enumerate() {
            let entry = crate::alloc::format!("{}={}", k, v);
            let p = b_ptr + offset;
            let _ = write_u32(store, e_ptr + (i as u32 * 4), p);
            let b = entry.as_bytes();
            let _ = write_bytes(store, p, b);
            let _ = write_bytes(store, p + b.len() as u32, &[0]);
            offset += b.len() as u32 + 1;
        }
        Ok(vec![Value::I32(0)])
    }
);

crate::export_method!(
    "wasi_snapshot_preview1", "environ_sizes_get",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn environ_sizes_get_p1<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let c_ptr = match args.get(0) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let b_ptr = match args.get(1) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let env_vars = store.wasi_ctx.as_ref().ok_or(HaltExecutionError(1))?.env.environ_get().unwrap_or_default();
        let _ = write_u32(store, c_ptr, env_vars.len() as u32);
        let _ = write_u32(store, b_ptr, env_vars.iter().map(|(k, v)| k.len() + v.len() + 2).sum::<usize>() as u32);
        Ok(vec![Value::I32(0)])
    }
);

crate::export_method!(
    "env", "__wasm_call_dtors",
    [],
    vec![], vec![],
    pub fn wasm_call_dtors<T: Config>(_: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        Ok(vec![])
    }
);

pub fn register_wasi<T: Config>(linker: &mut crate::wasm::Linker, store: &mut crate::wasm::Store<'_, T>) {
    get_environment::register(linker, store);
    get_arguments::register(linker, store);
    initial_cwd::register(linker, store);
    args_get::register(linker, store);
    args_sizes_get::register(linker, store);
    environ_get_p1::register(linker, store);
    environ_sizes_get_p1::register(linker, store);
    wasm_call_dtors::register(linker, store);
}
