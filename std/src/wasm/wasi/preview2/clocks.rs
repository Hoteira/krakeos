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
    
    let _ = super::write_u64(store, ret_ptr, secs);
    let _ = super::write_u32(store, ret_ptr + 8, 0);
    Ok(vec![])
}

pub fn wall_clock_resolution<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let ret_ptr = match args.get(0) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![]) };
    let _ = super::write_u64(store, ret_ptr, 1);
    let _ = super::write_u32(store, ret_ptr + 8, 0);
    Ok(vec![])
}

pub fn timezone_display<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let _when_sec = match args.get(0) { Some(Value::I64(v)) => *v as u64, _ => 0 };
    let _when_nsec = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => 0 };
    let ret_ptr = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![]) };

    // timezone-display record:
    // utc_offset: s32
    // name: string
    // in_daylight_savings_time: bool
    
    // UTC
    let _ = super::write_u32(store, ret_ptr, 0); // offset 0
    let name = "UTC";
    let bytes = name.as_bytes();
    let ptr = super::call_cabi_realloc(store, bytes.len() as u32, 1)?;
    let _ = super::write_bytes(store, ptr, bytes);
    let _ = super::write_u32(store, ret_ptr + 4, ptr);
    let _ = super::write_u32(store, ret_ptr + 8, bytes.len() as u32);
    let _ = super::write_bytes(store, ret_ptr + 12, &[0]); // false

    Ok(vec![])
}

pub fn timezone_utc_offset<T: Config>(_: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    Ok(vec![Value::I32(0)])
}
