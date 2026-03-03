use crate::alloc::{vec, vec::Vec};
use crate::wasm::{
    common::{config::Config, value::Value, reader::types::{ValType, NumType}},
    interpreter::store::{HaltExecutionError, Store},
    wasi::ctx::{PollableTarget, WasiResource},
};

crate::export_method!(
    "wasi:clocks/monotonic-clock@0.2.0", "now",
    [],
    vec![], vec![ValType::NumType(NumType::I64)],
    pub fn monotonic_clock_now<T: Config>(_: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        Ok(vec![Value::I64((crate::os::get_system_ticks() * 1_000_000) as u64)])
    }
);

crate::export_method!(
    "wasi:clocks/monotonic-clock@0.2.0", "resolution",
    [],
    vec![], vec![ValType::NumType(NumType::I64)],
    pub fn monotonic_clock_resolution<T: Config>(_: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        Ok(vec![Value::I64(1_000_000)])
    }
);

crate::export_method!(
    "wasi:clocks/monotonic-clock@0.2.0", "subscribe-duration",
    [],
    vec![ValType::NumType(NumType::I64)], vec![ValType::NumType(NumType::I32)],
    pub fn monotonic_clock_subscribe_duration<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let duration = match args.get(0) {
            Some(Value::I64(v)) => *v as u64,
            _ => return Ok(vec![Value::I32(0)]),
        };
        let now = (crate::os::get_system_ticks() * 1_000_000) as u64;
        let deadline = now.wrapping_add(duration);
        let wasi = store.wasi_ctx.as_mut().ok_or(HaltExecutionError(1))?;
        let id = wasi.next_resource_id;
        wasi.next_resource_id += 1;
        wasi.resource_table.insert(id, WasiResource::Pollable(PollableTarget::Timer(deadline)));
        Ok(vec![Value::I32(id as u32)])
    }
);

crate::export_method!(
    "wasi:clocks/wall-clock@0.2.0", "now",
    [("wasi_snapshot_preview1", "clock_time_get")],
    vec![ValType::NumType(NumType::I32)], vec![],
    pub fn wall_clock_now<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let ret_ptr = match args.get(0) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![]) };
        let (d, m, y) = crate::os::get_date();
        let (h, min, s) = crate::os::get_time();
        let yrs = if y >= 1970 { (y - 1970) as u64 } else { 0 };
        let secs = yrs * 31_536_000
            + (m as u64).saturating_sub(1) * 2_592_000
            + (d as u64).saturating_sub(1) * 86_400
            + (h as u64) * 3600
            + (min as u64) * 60
            + s as u64;
        
        let _ = crate::wasm::wasi::preview2::write_u64(store, ret_ptr, secs);
        let _ = crate::wasm::wasi::preview2::write_u32(store, ret_ptr + 8, 0);
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi:clocks/wall-clock@0.2.0", "resolution",
    [],
    vec![ValType::NumType(NumType::I32)], vec![],
    pub fn wall_clock_resolution<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let ret_ptr = match args.get(0) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![]) };
        let _ = crate::wasm::wasi::preview2::write_u64(store, ret_ptr, 1);
        let _ = crate::wasm::wasi::preview2::write_u32(store, ret_ptr + 8, 0);
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi_snapshot_preview1", "clock_res_get",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn clock_res_get<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let id = match args.get(0) { Some(Value::I32(x)) => *x, _ => 0 };
        let r_ptr = match args.get(1) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        match store.wasi_ctx.as_ref().ok_or(HaltExecutionError(1))?.env.clock_res_get(id) {
            Ok(res) => {
                let _ = crate::wasm::wasi::preview2::write_u64(store, r_ptr, res);
                Ok(vec![Value::I32(0)])
            }
            Err(e) => Ok(vec![Value::I32(e as u32)]),
        }
    }
);

crate::export_method!(
    "wasi_snapshot_preview1", "clock_time_get",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn clock_time_get<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let id = match args.get(0) { Some(Value::I32(x)) => *x, _ => 0 };
        let precision = match args.get(1) { Some(Value::I64(x)) => *x as u64, _ => 0 };
        let t_ptr = match args.get(2) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        match store.wasi_ctx.as_ref().ok_or(HaltExecutionError(1))?.env.clock_time_get(id, precision) {
            Ok(t) => {
                let _ = crate::wasm::wasi::preview2::write_u64(store, t_ptr, t);
                Ok(vec![Value::I32(0)])
            }
            Err(e) => Ok(vec![Value::I32(e as u32)]),
        }
    }
);

pub fn register_wasi<T: Config + Clone>(linker: &mut crate::wasm::Linker, store: &mut crate::wasm::Store<'_, T>) {
    monotonic_clock_now::register(linker, store);
    monotonic_clock_resolution::register(linker, store);
    monotonic_clock_subscribe_duration::register(linker, store);
    wall_clock_now::register(linker, store);
    wall_clock_resolution::register(linker, store);
    clock_res_get::register(linker, store);
    clock_time_get::register(linker, store);
}
