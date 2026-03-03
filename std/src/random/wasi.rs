use crate::alloc::{vec, vec::Vec};
use crate::wasm::{
    common::{config::Config, value::Value, reader::types::{ValType, NumType}},
    interpreter::store::{HaltExecutionError, Store},
};
use crate::wasm::wasi::preview2::{call_cabi_realloc, write_bytes, write_u32, write_u64};

crate::export_method!(
    "wasi:random/random@0.2.0", "get-random-bytes",
    [],
    vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![],
    pub fn get_random_bytes<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let len = match args.get(0) { Some(Value::I64(v)) => *v as u64, _ => return Ok(vec![]) };
        let ret_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![]) };
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
        let _ = write_u32(store, ret_ptr, ptr);
        let _ = write_u32(store, ret_ptr + 4, len as u32);
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi:random/insecure-seed@0.2.0", "insecure-seed",
    [],
    vec![], vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I64)],
    pub fn insecure_seed<T: Config>(store: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let mut buf = [0u8; 16];
        if let Some(ctx) = store.wasi_ctx.as_mut() {
            let _ = ctx.env.random_get(&mut buf);
        }
        let low = u64::from_le_bytes(buf[0..8].try_into().unwrap());
        let high = u64::from_le_bytes(buf[8..16].try_into().unwrap());
        Ok(vec![Value::I64(low), Value::I64(high)])
    }
);

crate::export_method!(
    "wasi:random/insecure@0.2.0", "get-insecure-random-bytes",
    [],
    vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![],
    pub fn get_insecure_random_bytes<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        get_random_bytes(store, args)
    }
);

crate::export_method!(
    "wasi:random/insecure@0.2.0", "get-insecure-random-u64",
    [],
    vec![], vec![ValType::NumType(NumType::I64)],
    pub fn get_insecure_random_u64<T: Config>(store: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let mut buf = [0u8; 8];
        if let Some(ctx) = store.wasi_ctx.as_mut() {
            let _ = ctx.env.random_get(&mut buf);
        }
        let val = u64::from_le_bytes(buf);
        Ok(vec![Value::I64(val)])
    }
);

crate::export_method!(
    "wasi_snapshot_preview1", "random_get",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn random_get_p1<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let ptr = match args.get(0) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let len = match args.get(1) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let mut buf = vec![0u8; len as usize];
        let _ = store.wasi_ctx.as_mut().ok_or(HaltExecutionError(1))?.env.random_get(&mut buf);
        if write_bytes(store, ptr, &buf).is_err() {
            return Ok(vec![Value::I32(28)]);
        }
        Ok(vec![Value::I32(0)])
    }
);

pub fn register_wasi<T: Config + Clone>(linker: &mut crate::wasm::Linker, store: &mut crate::wasm::Store<'_, T>) {
    get_random_bytes::register(linker, store);
    insecure_seed::register(linker, store);
    get_insecure_random_bytes::register(linker, store);
    get_insecure_random_u64::register(linker, store);
    random_get_p1::register(linker, store);
}
