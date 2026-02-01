use crate::rust_alloc::{vec, vec::Vec};
use crate::wasm::{
    common::{config::Config, value::Value},
    interpreter::store::{HaltExecutionError, Store},
    wasi::ctx::{PollableTarget, WasiResource},
};

pub fn monotonic_clock_now<T: Config>(_: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    Ok(vec![Value::I64((crate::os::get_system_ticks() * 1_000_000) as u64)])
}

pub fn monotonic_clock_resolution<T: Config>(_: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    Ok(vec![Value::I64(1_000_000)])
}

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

pub fn wall_clock_now<T: Config>(_: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let (d, m, y) = crate::os::get_date();
    let (h, min, s) = crate::os::get_time();
    let yrs = if y >= 1970 { (y - 1970) as u64 } else { 0 };
    let secs = yrs * 31_536_000
        + (m as u64).saturating_sub(1) * 2_592_000
        + (d as u64).saturating_sub(1) * 86_400
        + (h as u64) * 3600
        + (min as u64) * 60
        + s as u64;
    Ok(vec![Value::I64(secs), Value::I32(0)])
}

pub fn wall_clock_resolution<T: Config>(_: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    Ok(vec![Value::I64(1), Value::I32(0)])
}
