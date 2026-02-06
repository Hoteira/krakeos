use crate::rust_alloc::{vec, vec::Vec};
use crate::wasm::{
    common::{config::Config, value::Value},
    interpreter::store::{HaltExecutionError, Store},
    wasi::ctx::{InputStreamSource, OutputStreamSource, WasiResource},
};
use super::{call_cabi_realloc, write_bytes, write_u32};

pub fn get_stdout<T: Config>(store: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let wasi = store.wasi_ctx.as_mut().ok_or(HaltExecutionError(1))?;
    let id = wasi.next_resource_id;
    wasi.next_resource_id += 1;
    wasi.resource_table.insert(id, WasiResource::OutputStream(OutputStreamSource::Stdout));
    Ok(vec![Value::I32(id as u32)])
}

pub fn get_stdin<T: Config>(store: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let wasi = store.wasi_ctx.as_mut().ok_or(HaltExecutionError(1))?;
    let id = wasi.next_resource_id;
    wasi.next_resource_id += 1;
    wasi.resource_table.insert(id, WasiResource::InputStream(InputStreamSource::Stdin));
    Ok(vec![Value::I32(id as u32)])
}

pub fn get_stderr<T: Config>(store: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let wasi = store.wasi_ctx.as_mut().ok_or(HaltExecutionError(1))?;
    let id = wasi.next_resource_id;
    wasi.next_resource_id += 1;
    wasi.resource_table.insert(id, WasiResource::OutputStream(OutputStreamSource::Stderr));
    Ok(vec![Value::I32(id as u32)])
}

pub fn exit<T: Config>(_: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let code = match args.get(0) {
        Some(Value::I32(v)) => *v as i32,
        _ => 0,
    };
    crate::debugln!("WASI P2: exit({})", code);
    Err(HaltExecutionError(code))
}

pub fn get_environment<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let ret_ptr = match args.get(0) {
        Some(Value::I32(v)) => *v as u32,
        _ => return Ok(vec![]),
    };

    let env_vars = store.wasi_ctx.as_ref().ok_or(HaltExecutionError(1))?.env.environ_get().unwrap_or_default();
    let count = env_vars.len() as u32;
    
    // Allocate array of tuples (ptr, len, ptr, len) -> 4 * 4 = 16 bytes per item
    let array_ptr = if count > 0 {
        call_cabi_realloc(store, count * 16, 4)?
    } else {
        0
    };

    for (i, (k, v)) in env_vars.into_iter().enumerate() {
        let k_bytes = k.as_bytes();
        let v_bytes = v.as_bytes();
        
        let k_ptr = call_cabi_realloc(store, k_bytes.len() as u32, 1)?;
        write_bytes(store, k_ptr, k_bytes).map_err(|_| HaltExecutionError(1))?;
        
        let v_ptr = call_cabi_realloc(store, v_bytes.len() as u32, 1)?;
        write_bytes(store, v_ptr, v_bytes).map_err(|_| HaltExecutionError(1))?;

        let tuple_off = array_ptr + (i as u32 * 16);
        write_u32(store, tuple_off, k_ptr).map_err(|_| HaltExecutionError(1))?;
        write_u32(store, tuple_off + 4, k_bytes.len() as u32).map_err(|_| HaltExecutionError(1))?;
        write_u32(store, tuple_off + 8, v_ptr).map_err(|_| HaltExecutionError(1))?;
        write_u32(store, tuple_off + 12, v_bytes.len() as u32).map_err(|_| HaltExecutionError(1))?;
    }

    write_u32(store, ret_ptr, array_ptr).map_err(|_| HaltExecutionError(1))?;
    write_u32(store, ret_ptr + 4, count).map_err(|_| HaltExecutionError(1))?;

    Ok(vec![])
}

pub fn get_arguments<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let ret_ptr = match args.get(0) {
        Some(Value::I32(v)) => *v as u32,
        _ => return Ok(vec![]),
    };

    let env_args = store.wasi_ctx.as_ref().ok_or(HaltExecutionError(1))?.env.args_get().unwrap_or_default();
    let count = env_args.len() as u32;

    // Allocate array of strings (ptr, len) -> 8 bytes per item
    let array_ptr = if count > 0 {
        call_cabi_realloc(store, count * 8, 4)?
    } else {
        0
    };

    for (i, arg) in env_args.into_iter().enumerate() {
        let bytes = arg.as_bytes();
        let s_ptr = call_cabi_realloc(store, bytes.len() as u32, 1)?;
        write_bytes(store, s_ptr, bytes).map_err(|_| HaltExecutionError(1))?;

        let struct_off = array_ptr + (i as u32 * 8);
        write_u32(store, struct_off, s_ptr).map_err(|_| HaltExecutionError(1))?;
        write_u32(store, struct_off + 4, bytes.len() as u32).map_err(|_| HaltExecutionError(1))?;
    }

    write_u32(store, ret_ptr, array_ptr).map_err(|_| HaltExecutionError(1))?;
    write_u32(store, ret_ptr + 4, count).map_err(|_| HaltExecutionError(1))?;

    Ok(vec![])
}
