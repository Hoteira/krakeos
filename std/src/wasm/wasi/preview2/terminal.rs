use crate::rust_alloc::{vec, vec::Vec};
use crate::wasm::{
    common::{config::Config, value::Value},
    interpreter::store::{HaltExecutionError, Store},
    wasi::ctx::WasiResource,
};

pub fn get_terminal_stdin<T: Config>(store: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let wasi = store.wasi_ctx.as_mut().ok_or(HaltExecutionError(1))?;
    let id = wasi.next_resource_id;
    wasi.next_resource_id += 1;
    // Associate with FD 0 (stdin)
    wasi.resource_table.insert(id, WasiResource::TerminalInput(0));
    // Return Option::Some(id) -> Tag 1, Payload id
    Ok(vec![Value::I32(1), Value::I32(id as u32)])
}

pub fn get_terminal_stdout<T: Config>(store: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let wasi = store.wasi_ctx.as_mut().ok_or(HaltExecutionError(1))?;
    let id = wasi.next_resource_id;
    wasi.next_resource_id += 1;
    // Associate with FD 1 (stdout)
    wasi.resource_table.insert(id, WasiResource::TerminalOutput(1));
    // Return Option::Some(id) -> Tag 1, Payload id
    Ok(vec![Value::I32(1), Value::I32(id as u32)])
}

pub fn get_terminal_stderr<T: Config>(store: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let wasi = store.wasi_ctx.as_mut().ok_or(HaltExecutionError(1))?;
    let id = wasi.next_resource_id;
    wasi.next_resource_id += 1;
    // Associate with FD 2 (stderr)
    wasi.resource_table.insert(id, WasiResource::TerminalOutput(2));
    // Return Option::Some(id) -> Tag 1, Payload id
    Ok(vec![Value::I32(1), Value::I32(id as u32)])
}
