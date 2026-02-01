use super::{call_cabi_realloc, write_bytes, write_u32};
use crate::rust_alloc::{vec, vec::Vec};
use crate::wasm::{
    common::{config::Config, value::Value},
    interpreter::store::{HaltExecutionError, Store},
};

pub fn get_random_bytes<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let len = match args.get(0) {
        Some(Value::I64(v)) => *v as u64,
        _ => return Ok(vec![]),
    };
    let ret_ptr = match args.get(1) {
        Some(Value::I32(v)) => *v as u32,
        _ => return Ok(vec![]),
    };
    let mut buf = vec![0u8; len as usize];
    unsafe {
        if crate::wasm::wasi::preview1::RANDOM_STATE == 0 {
            crate::wasm::wasi::preview1::RANDOM_STATE = crate::os::get_system_ticks().wrapping_add(0xACE1BADE);
        }
        for i in 0..len as usize {
            crate::wasm::wasi::preview1::RANDOM_STATE ^= crate::wasm::wasi::preview1::RANDOM_STATE << 13;
            crate::wasm::wasi::preview1::RANDOM_STATE ^= crate::wasm::wasi::preview1::RANDOM_STATE >> 17;
            crate::wasm::wasi::preview1::RANDOM_STATE ^= crate::wasm::wasi::preview1::RANDOM_STATE << 5;
            buf[i] = (crate::wasm::wasi::preview1::RANDOM_STATE & 0xFF) as u8;
        }
    }
    let ptr = match call_cabi_realloc(store, len as u32, 1) {
        Ok(p) => p,
        Err(_) => return Ok(vec![]),
    };
    if write_bytes(store, ptr, &buf).is_err() {
        return Ok(vec![]);
    }
    write_u32(store, ret_ptr + 4, len as u32).map_err(|_| HaltExecutionError(1))?;
    write_u32(store, ret_ptr, ptr).map_err(|_| HaltExecutionError(1))?;
    Ok(vec![])
}
