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

    let wasi = store.wasi_ctx.as_ref().ok_or(HaltExecutionError(1))?;
    let mut preopens = Vec::new();
    for (id, res) in &wasi.resource_table {
        if let WasiResource::Directory(path) = res {
            preopens.push((*id, path.clone()));
        }
    }

    let count = preopens.len() as u32;
    let array_ptr = if count > 0 {
        super::call_cabi_realloc(store, count * 12, 4)?
    } else {
        0
    };

    for (i, (id, path)) in preopens.into_iter().enumerate() {
        let bytes = path.as_bytes();
        let s_ptr = super::call_cabi_realloc(store, bytes.len() as u32, 1)?;
        super::write_bytes(store, s_ptr, bytes).map_err(|_| HaltExecutionError(1))?;

        let tuple_off = array_ptr + (i as u32 * 12);
        super::write_u32(store, tuple_off, id as u32).map_err(|_| HaltExecutionError(1))?;
        super::write_u32(store, tuple_off + 4, s_ptr).map_err(|_| HaltExecutionError(1))?;
        super::write_u32(store, tuple_off + 8, bytes.len() as u32).map_err(|_| HaltExecutionError(1))?;
    }

    super::write_u32(store, ret_ptr, array_ptr).map_err(|_| HaltExecutionError(1))?;
    super::write_u32(store, ret_ptr + 4, count).map_err(|_| HaltExecutionError(1))?;

    Ok(vec![])
}

pub fn filesystem_types_read_via_stream<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let handle = match args.get(0) {
        Some(Value::I32(v)) => *v as i32,
        _ => return Ok(vec![Value::I32(0)]),
    };
    let wasi = store.wasi_ctx.as_mut().ok_or(HaltExecutionError(1))?;
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
    let wasi = store.wasi_ctx.as_mut().ok_or(HaltExecutionError(1))?;
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
