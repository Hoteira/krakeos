use crate::rust_alloc::{string::String, vec, vec::Vec};
use crate::wasm::{
    core::reader::types::{FuncType, NumType, ResultType, ValType},
    execution::{
        checked::AbstractStored,
        config::Config,
        linker::Linker,
        store::{addrs::FuncAddr, ExternVal, HaltExecutionError, Store},
        value::Value,
    },
};

pub mod cli;
pub mod clocks;
pub mod filesystem;
pub mod io;
pub mod random;
pub mod sockets;

pub fn create_wasi_p2_imports<T: Config>(linker: &mut Linker, store: &mut Store<'_, T>) {
    if store.wasi_ctx.is_none() {
        store.wasi_ctx = Some(crate::wasm::wasi::ctx::WasiCtx::default());
    }

    // wasi:cli/stdout@0.2.0
    {
        let module = "wasi:cli/stdout@0.2.0";
        define(linker, store, module, "get-stdout", vec![], vec![ValType::NumType(NumType::I32)], cli::get_stdout);
    }
    // wasi:cli/stdin@0.2.0
    {
        let module = "wasi:cli/stdin@0.2.0";
        define(linker, store, module, "get-stdin", vec![], vec![ValType::NumType(NumType::I32)], cli::get_stdin);
    }
    // wasi:cli/stderr@0.2.0
    {
        let module = "wasi:cli/stderr@0.2.0";
        define(linker, store, module, "get-stderr", vec![], vec![ValType::NumType(NumType::I32)], cli::get_stderr);
    }
    // wasi:io/streams@0.2.0
    {
        let module = "wasi:io/streams@0.2.0";
        define(linker, store, module, "[method]output-stream.write", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)], io::stream_write);
        define(linker, store, module, "[method]output-stream.blocking-write", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)], io::stream_write);
        define(linker, store, module, "[method]output-stream.blocking-write-and-flush", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)], io::stream_write);
        define(linker, store, module, "[method]input-stream.read", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![], io::stream_read);
        define(linker, store, module, "[method]input-stream.blocking-read", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![], io::stream_read);
        define(linker, store, module, "[method]input-stream.subscribe", vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)], io::input_stream_subscribe);
        define(linker, store, module, "[method]output-stream.subscribe", vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)], io::output_stream_subscribe);
        define(linker, store, module, "[resource-drop]input-stream", vec![ValType::NumType(NumType::I32)], vec![], resource_drop);
        define(linker, store, module, "[resource-drop]output-stream", vec![ValType::NumType(NumType::I32)], vec![], resource_drop);
    }
    // wasi:io/poll@0.2.0
    {
        let module = "wasi:io/poll@0.2.0";
        define(linker, store, module, "poll", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], io::poll_poll);
        define(linker, store, module, "[method]pollable.block", vec![ValType::NumType(NumType::I32)], vec![], io::poll_block);
        define(linker, store, module, "[resource-drop]pollable", vec![ValType::NumType(NumType::I32)], vec![], resource_drop);
    }
    // wasi:io/error@0.2.0
    {
        let module = "wasi:io/error@0.2.0";
        define(linker, store, module, "[resource-drop]error", vec![ValType::NumType(NumType::I32)], vec![], resource_drop);
        define(linker, store, module, "[method]error.to-debug-string", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], io::error_to_debug_string);
    }
    // wasi:sockets/udp@0.2.0
    {
        let module = "wasi:sockets/udp@0.2.0";
        define(linker, store, module, "[resource-drop]udp-socket", vec![ValType::NumType(NumType::I32)], vec![], resource_drop);
        define(linker, store, module, "[resource-drop]incoming-datagram-stream", vec![ValType::NumType(NumType::I32)], vec![], resource_drop);
        define(linker, store, module, "[resource-drop]outgoing-datagram-stream", vec![ValType::NumType(NumType::I32)], vec![], resource_drop);
    }
    // wasi:sockets/tcp@0.2.0
    {
        let module = "wasi:sockets/tcp@0.2.0";
        define(linker, store, module, "[resource-drop]tcp-socket", vec![ValType::NumType(NumType::I32)], vec![], resource_drop);
    }
    // wasi_snapshot_preview1 (Adapter extras)
    {
        let module = "wasi_snapshot_preview1";
        define(linker, store, module, "adapter_close_badfd", vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)], sockets::adapter_close_badfd);
    }
    // wasi:clocks/monotonic-clock@0.2.0
    {
        let module = "wasi:clocks/monotonic-clock@0.2.0";
        define(linker, store, module, "now", vec![], vec![ValType::NumType(NumType::I64)], clocks::monotonic_clock_now);
        define(linker, store, module, "resolution", vec![], vec![ValType::NumType(NumType::I64)], clocks::monotonic_clock_resolution);
        define(linker, store, module, "subscribe-duration", vec![ValType::NumType(NumType::I64)], vec![ValType::NumType(NumType::I32)], clocks::monotonic_clock_subscribe_duration);
    }
    // wasi:clocks/wall-clock@0.2.0
    {
        let module = "wasi:clocks/wall-clock@0.2.0";
        define(linker, store, module, "now", vec![], vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], clocks::wall_clock_now);
        define(linker, store, module, "resolution", vec![], vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], clocks::wall_clock_resolution);
    }
    // wasi:random/random@0.2.0
    {
        let module = "wasi:random/random@0.2.0";
        define(linker, store, module, "get-random-bytes", vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![], random::get_random_bytes);
    }
    // wasi:cli/exit@0.2.0
    {
        let module = "wasi:cli/exit@0.2.0";
        define(linker, store, module, "exit", vec![ValType::NumType(NumType::I32)], vec![], cli::exit);
    }
    // wasi:cli/environment@0.2.0
    {
        let module = "wasi:cli/environment@0.2.0";
        define(linker, store, module, "get-environment", vec![ValType::NumType(NumType::I32)], vec![], cli::get_environment);
    }
    // wasi:filesystem/preopens@0.2.0
    {
        let module = "wasi:filesystem/preopens@0.2.0";
        define(linker, store, module, "get-directories", vec![ValType::NumType(NumType::I32)], vec![], filesystem::get_directories);
    }
    // wasi:filesystem/types@0.2.0
    {
        let module = "wasi:filesystem/types@0.2.0";
        define(linker, store, module, "[method]descriptor.read-via-stream", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64)], vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], filesystem::filesystem_types_read_via_stream);
        define(linker, store, module, "[method]descriptor.write-via-stream", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64)], vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], filesystem::filesystem_types_write_via_stream);
        define(linker, store, module, "[method]descriptor.append-via-stream", vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], filesystem::filesystem_types_append_via_stream);
        define(linker, store, module, "[resource-drop]descriptor", vec![ValType::NumType(NumType::I32)], vec![], resource_drop);
    }
}

pub(crate) fn define<T: Config>(
    linker: &mut Linker,
    store: &mut Store<'_, T>,
    module: &str,
    name: &str,
    params: Vec<ValType>,
    returns: Vec<ValType>,
    func: for<'a> fn(&mut Store<'a, T>, Vec<Value>) -> Result<Vec<Value>, HaltExecutionError>,
) {
    let func_type = FuncType {
        params: ResultType { valtypes: params },
        returns: ResultType { valtypes: returns },
    };
    let func_addr = store.func_alloc_unchecked(func_type, func);
    let _ = linker.define_unchecked(String::from(module), String::from(name), ExternVal::Func(func_addr));
}

pub(crate) fn resource_drop<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let handle = match args.get(0) {
        Some(Value::I32(v)) => *v as i32,
        _ => return Ok(vec![]),
    };
    let wasi = store.wasi_ctx.as_mut().ok_or(HaltExecutionError)?;
    wasi.resource_table.remove(&handle);
    Ok(vec![])
}

pub(crate) fn find_cabi_realloc<T: Config>(store: &Store<'_, T>) -> Option<FuncAddr> {
    let module_addr = store.caller_module?;
    if let Ok(export) = store.instance_export(unsafe { crate::wasm::execution::checked::Stored::from_bare(module_addr, store.id) }, "cabi_realloc") {
        if let Some(func) = export.as_func() {
            return Some(func.into_bare());
        }
    }
    None
}

pub(crate) fn call_cabi_realloc<T: Config>(store: &mut Store<'_, T>, new_size: u32, align: u32) -> Result<u32, HaltExecutionError> {
    let cabi_realloc_addr = find_cabi_realloc(store).ok_or(HaltExecutionError)?;
    let args = vec![Value::I32(0), Value::I32(0), Value::I32(align), Value::I32(new_size)];
    match store.invoke_unchecked(cabi_realloc_addr, args, None) {
        Ok(crate::wasm::execution::resumable::RunState::Finished { values, .. }) => {
            if let Some(Value::I32(ptr)) = values.first() {
                Ok(*ptr as u32)
            } else {
                Err(HaltExecutionError)
            }
        }
        _ => Err(HaltExecutionError),
    }
}

pub(crate) fn write_bytes<T: Config>(store: &mut Store<'_, T>, addr: u32, bytes: &[u8]) -> Result<(), ()> {
    let module_addr = store.caller_module.ok_or(())?;
    // This is a simplified lookup; assuming main memory is index 0
    let mem_addr = *store.modules.get(module_addr).mem_addrs.get(0).ok_or(())?;
    let mem = store.memories.get(mem_addr);
    mem.mem.init(addr as usize, bytes, 0, bytes.len()).map_err(|_| ())
}

pub(crate) fn write_u32<T: Config>(store: &mut Store<'_, T>, addr: u32, val: u32) -> Result<(), ()> {
    write_bytes(store, addr, &val.to_le_bytes())
}

pub(crate) fn write_u64<T: Config>(store: &mut Store<'_, T>, addr: u32, val: u64) -> Result<(), ()> {
    write_bytes(store, addr, &val.to_le_bytes())
}

pub(crate) fn read_bytes<T: Config>(store: &Store<'_, T>, addr: u32, buf: &mut [u8]) -> Result<(), ()> {
    let module_addr = store.caller_module.ok_or(())?;
    let mem_addr = *store.modules.get(module_addr).mem_addrs.get(0).ok_or(())?;
    let mem = store.memories.get(mem_addr);
    mem.mem.read_slice(addr as usize, buf).map_err(|_| ())
}

pub(crate) fn read_mem<T: Config>(store: &Store<'_, T>, addr: u32, buf: &mut [u8]) -> Result<(), ()> {
    read_bytes(store, addr, buf)
}
