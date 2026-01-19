use crate::rust_alloc::{vec, vec::Vec};
use crate::wasm::{
    execution::{
        config::Config,
        store::{HaltExecutionError, Store},
        value::Value,
    },
    wasi::ctx::{InputStreamSource, OutputStreamSource, WasiResource},
};

pub fn get_stdout<T: Config>(store: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let wasi = store.wasi_ctx.as_mut().ok_or(HaltExecutionError)?;
    let id = wasi.next_resource_id;
    wasi.next_resource_id += 1;
    wasi.resource_table.insert(id, WasiResource::OutputStream(OutputStreamSource::Stdout));
    crate::debugln!("WASI P2: get_stdout -> handle {}", id);
    Ok(vec![Value::I32(id as u32)])
}

pub fn get_stdin<T: Config>(store: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let wasi = store.wasi_ctx.as_mut().ok_or(HaltExecutionError)?;
    let id = wasi.next_resource_id;
    wasi.next_resource_id += 1;
    wasi.resource_table.insert(id, WasiResource::InputStream(InputStreamSource::Stdin));
    crate::debugln!("WASI P2: get_stdin -> handle {}", id);
    Ok(vec![Value::I32(id as u32)])
}

pub fn get_stderr<T: Config>(store: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let wasi = store.wasi_ctx.as_mut().ok_or(HaltExecutionError)?;
    let id = wasi.next_resource_id;
    wasi.next_resource_id += 1;
    wasi.resource_table.insert(id, WasiResource::OutputStream(OutputStreamSource::Stderr));
    crate::debugln!("WASI P2: get_stderr -> handle {}", id);
    Ok(vec![Value::I32(id as u32)])
}

pub fn exit<T: Config>(_: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let code = match args.get(0) {
        Some(Value::I32(v)) => *v as i32,
        _ => 0,
    };
    crate::debugln!("WASI P2: exit({})", code);
    Err(HaltExecutionError)
}

pub fn get_environment<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    panic!("WASI P2 stub: get_environment");
}
