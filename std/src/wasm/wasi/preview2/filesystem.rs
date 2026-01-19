use super::write_u32;
use crate::rust_alloc::{vec, vec::Vec};
use crate::wasm::{
    execution::{
        config::Config,
        store::{HaltExecutionError, Store},
        value::Value,
    },
    wasi::ctx::{InputStreamSource, OutputStreamSource, WasiResource},
};
// Reuse helpers

pub fn get_directories<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let ret_ptr = match args.get(0) {
        Some(Value::I32(v)) => *v as u32,
        _ => return Ok(vec![]),
    };
    write_u32(store, ret_ptr + 4, 0).map_err(|_| HaltExecutionError)?;
    write_u32(store, ret_ptr, 0).map_err(|_| HaltExecutionError)?;
    Ok(vec![])
}

pub fn filesystem_types_read_via_stream<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let handle = match args.get(0) {
        Some(Value::I32(v)) => *v as i32,
        _ => return Ok(vec![Value::I32(0)]),
    };
    let wasi = store.wasi_ctx.as_mut().ok_or(HaltExecutionError)?;
    let fd = match wasi.resource_table.get(&handle) {
        Some(WasiResource::File(f)) => f.as_raw_fd(),
        _ => return Ok(vec![Value::I32(0)]),
    };
    let id = wasi.next_resource_id;
    wasi.next_resource_id += 1;
    wasi.resource_table.insert(id, WasiResource::InputStream(InputStreamSource::File(fd)));
    // Result::Ok(id) -> Tag 0, Payload id
    Ok(vec![Value::I32(0), Value::I32(id as u32)])
}

pub fn filesystem_types_write_via_stream<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let handle = match args.get(0) {
        Some(Value::I32(v)) => *v as i32,
        _ => return Ok(vec![Value::I32(0)]),
    };
    let wasi = store.wasi_ctx.as_mut().ok_or(HaltExecutionError)?;
    let fd = match wasi.resource_table.get(&handle) {
        Some(WasiResource::File(f)) => f.as_raw_fd(),
        _ => return Ok(vec![Value::I32(0)]),
    };
    let id = wasi.next_resource_id;
    wasi.next_resource_id += 1;
    wasi.resource_table.insert(id, WasiResource::OutputStream(OutputStreamSource::File(fd)));
    Ok(vec![Value::I32(0), Value::I32(id as u32)])
}

pub fn filesystem_types_append_via_stream<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    filesystem_types_write_via_stream(store, args)
}
