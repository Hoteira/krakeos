use crate::rust_alloc::{string::String, vec, vec::Vec};
use crate::wasm::{
    common::{
        checked::{AbstractStored, Stored},
        config::Config,
        interop::Linker,
        reader::types::{ValType},
        value::Value,
    },
    interpreter::{
        store::{HaltExecutionError, Store, addrs::FuncAddr},
        resumable::RunState,
    },
};

pub fn create_wasi_p2_imports<T: Config>(linker: &mut Linker, store: &mut Store<'_, T>) {
    if store.wasi_ctx.is_none() {
        store.wasi_ctx = Some(crate::wasm::wasi::ctx::WasiCtx::default());
    }

    crate::time::wasi::register_wasi(linker, store);
    crate::fs::wasi::register_wasi(linker, store);
    crate::io::wasi::register_wasi(linker, store);
    crate::env::wasi::register_wasi(linker, store);
    crate::process::wasi::register_wasi(linker, store);
    crate::random::wasi::register_wasi(linker, store);
    crate::net::wasi::register_wasi(linker, store);
    crate::os::krakeos::wasi::register_wasi(linker, store);
}

// Helpers
pub fn find_cabi_realloc<T: Config>(store: &Store<'_, T>) -> Option<FuncAddr> {
    let module_addr = store.caller_module?;
    if let Ok(export) = store.instance_export(
        unsafe { Stored::from_bare(module_addr, store.id) },
        "cabi_realloc",
    ) {
        if let Some(func) = export.as_func() {
            return Some(func.into_bare());
        }
    }
    None
}

pub fn call_cabi_realloc<T: Config>(
    store: &mut Store<'_, T>,
    new_size: u32,
    align: u32,
) -> Result<u32, HaltExecutionError> {
    let cabi_realloc_addr = find_cabi_realloc(store).ok_or(HaltExecutionError(1))?;
    let args = vec![
        Value::I32(0),
        Value::I32(0),
        Value::I32(align),
        Value::I32(new_size),
    ];
    match store.invoke_unchecked(cabi_realloc_addr, args, None) {
        Ok(RunState::Finished { values, .. }) => {
            if let Some(Value::I32(ptr)) = values.first() {
                Ok(*ptr as u32)
            } else {
                Err(HaltExecutionError(1))
            }
        }
        _ => Err(HaltExecutionError(1)),
    }
}

pub fn write_bytes<T: Config>(
    store: &mut Store<'_, T>,
    addr: u32,
    bytes: &[u8],
) -> Result<(), ()> {
    let module_addr = store.caller_module.ok_or(())?;
    let mem_addr = *store.modules.get(module_addr).mem_addrs.get(0).ok_or(())?;
    let mem = store.memories.get(mem_addr);
    mem.mem
        .init(addr as usize, bytes, 0, bytes.len())
        .map_err(|_| ())
}

pub fn write_u32<T: Config>(
    store: &mut Store<'_, T>,
    addr: u32,
    val: u32,
) -> Result<(), ()> {
    write_bytes(store, addr, &val.to_le_bytes())
}

pub fn write_u64<T: Config>(
    store: &mut Store<'_, T>,
    addr: u32,
    val: u64,
) -> Result<(), ()> {
    write_bytes(store, addr, &val.to_le_bytes())
}

pub fn read_bytes<T: Config>(
    store: &Store<'_, T>,
    addr: u32,
    buf: &mut [u8],
) -> Result<(), ()> {
    let module_addr = store.caller_module.ok_or(())?;
    let mem_addr = *store.modules.get(module_addr).mem_addrs.get(0).ok_or(())?;
    let mem = store.memories.get(mem_addr);
    mem.mem.read_slice(addr as usize, buf).map_err(|_| ())
}

pub fn read_mem<T: Config>(
    store: &Store<'_, T>,
    addr: u32,
    buf: &mut [u8],
) -> Result<(), ()> {
    read_bytes(store, addr, buf)
}

pub fn read_mem_u32<T: Config>(
    store: &Store<'_, T>,
    addr: u32,
) -> Result<u32, HaltExecutionError> {
    let mut buf = [0u8; 4];
    read_mem(store, addr, &mut buf).map_err(|_| HaltExecutionError(1))?;
    Ok(u32::from_le_bytes(buf))
}

pub fn read_mem_u64<T: Config>(
    store: &Store<'_, T>,
    addr: u32,
) -> Result<u64, HaltExecutionError> {
    let mut buf = [0u8; 8];
    read_mem(store, addr, &mut buf).map_err(|_| HaltExecutionError(1))?;
    Ok(u64::from_le_bytes(buf))
}

pub fn read_mem_string<T: Config>(
    store: &Store<'_, T>,
    ptr: u32,
) -> Result<String, HaltExecutionError> {
    let mut buf = Vec::new();
    let mut offset = 0;
    loop {
        let mut byte = [0u8; 1];
        read_mem(store, ptr + offset, &mut byte).map_err(|_| HaltExecutionError(1))?;
        if byte[0] == 0 { break; }
        buf.push(byte[0]);
        offset += 1;
        if offset > 4096 { return Err(HaltExecutionError(1)); }
    }
    String::from_utf8(buf).map_err(|_| HaltExecutionError(1))
}

pub fn resource_drop<T: Config>(
    store: &mut Store<'_, T>,
    args: Vec<Value>,
) -> Result<Vec<Value>, HaltExecutionError> {
    let handle = match args.get(0) {
        Some(Value::I32(v)) => *v as i32,
        _ => return Ok(vec![]),
    };
    let wasi = store.wasi_ctx.as_mut().ok_or(HaltExecutionError(1))?;
    wasi.resource_table.remove(&handle);
    Ok(vec![])
}
