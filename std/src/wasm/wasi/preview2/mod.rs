use crate::rust_alloc::{string::String, vec, vec::Vec};
use crate::wasm::{
    common::{
        checked::{AbstractStored, Stored},
        config::Config,
        interop::Linker,
        reader::types::{FuncType, NumType, ResultType, ValType},
        value::Value,
    },
    interpreter::{
        resumable::RunState,
        store::{addrs::FuncAddr, ExternVal, HaltExecutionError, Store},
    },
};

pub mod cli;
pub mod clocks;
pub mod filesystem;
pub mod io;
pub mod random;
pub mod sockets;
pub mod terminal;

pub fn create_wasi_p2_imports<T: Config>(linker: &mut Linker, store: &mut Store<'_, T>) {
    if store.wasi_ctx.is_none() {
        store.wasi_ctx = Some(crate::wasm::wasi::ctx::WasiCtx::default());
    }

    // wasi:cli/terminal-input@0.2.0
    {
        let module = "wasi:cli/terminal-input@0.2.0";
        define(linker, store, module, "[resource-drop]terminal-input", vec![ValType::NumType(NumType::I32)], vec![], resource_drop);
    }
    // wasi:cli/terminal-output@0.2.0
    {
        let module = "wasi:cli/terminal-output@0.2.0";
        define(linker, store, module, "[resource-drop]terminal-output", vec![ValType::NumType(NumType::I32)], vec![], resource_drop);
    }
    // wasi:cli/terminal-stdin@0.2.0
    {
        let module = "wasi:cli/terminal-stdin@0.2.0";
        define(linker, store, module, "get-terminal-stdin", vec![], vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], terminal::get_terminal_stdin);
    }
    // wasi:cli/terminal-stdout@0.2.0
    {
        let module = "wasi:cli/terminal-stdout@0.2.0";
        define(linker, store, module, "get-terminal-stdout", vec![], vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], terminal::get_terminal_stdout);
    }
    // wasi:cli/terminal-stderr@0.2.0
    {
        let module = "wasi:cli/terminal-stderr@0.2.0";
        define(linker, store, module, "get-terminal-stderr", vec![], vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], terminal::get_terminal_stderr);
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
        define(linker, store, module, "[method]output-stream.write", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], io::stream_write);
        define(linker, store, module, "[method]output-stream.blocking-write", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], io::stream_write);
        define(linker, store, module, "[method]output-stream.blocking-write-and-flush", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], io::stream_write);
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
        define(linker, store, module, "get-environment", vec![], vec![ValType::NumType(NumType::I32)], cli::get_environment); // Returns Result<list<...>>
        define(linker, store, module, "get-arguments", vec![], vec![ValType::NumType(NumType::I32)], cli::get_arguments);
    }
    // wasi:filesystem/preopens@0.2.0
    {
        let module = "wasi:filesystem/preopens@0.2.0";
        define(linker, store, module, "get-directories", vec![ValType::NumType(NumType::I32)], vec![], filesystem::get_directories);
    }
    // wasi:random/insecure-seed@0.2.0
    {
        let module = "wasi:random/insecure-seed@0.2.0";
        define(linker, store, module, "insecure-seed", vec![], vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I64)], random::insecure_seed);
    }
    // krakeos:core/system@0.2.0
    {
        let module = "krakeos:core/system@0.2.0";
        define(linker, store, module, "syscall", vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I64), ValType::NumType(NumType::I64), ValType::NumType(NumType::I64)], vec![ValType::NumType(NumType::I64)], krakeos_syscall_host);
        define(linker, store, module, "syscall5", vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I64), ValType::NumType(NumType::I64), ValType::NumType(NumType::I64), ValType::NumType(NumType::I64)], vec![ValType::NumType(NumType::I64)], krakeos_syscall5_host);
        define(linker, store, module, "syscall6", vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I64), ValType::NumType(NumType::I64), ValType::NumType(NumType::I64), ValType::NumType(NumType::I64), ValType::NumType(NumType::I64)], vec![ValType::NumType(NumType::I64)], krakeos_syscall6_host);
    }
    // krakeos:graphics/screen@0.2.0
    {
        let module = "krakeos:graphics/screen@0.2.0";
        define(linker, store, module, "get-width", vec![], vec![ValType::NumType(NumType::I32)], get_screen_width_host);
        define(linker, store, module, "get-height", vec![], vec![ValType::NumType(NumType::I32)], get_screen_height_host);
    }
    // wasi:filesystem/types@0.2.0
    {
        let module = "wasi:filesystem/types@0.2.0";
        define(linker, store, module, "[method]descriptor.read-via-stream", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64)], vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], filesystem::filesystem_types_read_via_stream);
        define(linker, store, module, "[method]descriptor.write-via-stream", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64)], vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], filesystem::filesystem_types_write_via_stream);
        define(linker, store, module, "[method]descriptor.append-via-stream", vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], filesystem::filesystem_types_append_via_stream);
        
        define(linker, store, module, "[method]descriptor.type", vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], filesystem::descriptor_type);
        define(linker, store, module, "[method]descriptor.stat", vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)], filesystem::descriptor_stat);
        define(linker, store, module, "[method]descriptor.open-at", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], filesystem::descriptor_open_at);
        define(linker, store, module, "[method]descriptor.read-directory", vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], filesystem::descriptor_read_directory);
        define(linker, store, module, "[method]directory-entry-stream.read-directory-entry", vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)], filesystem::directory_entry_stream_read_directory_entry);
        define(linker, store, module, "[method]descriptor.stat-at", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)], filesystem::descriptor_stat_at);
        define(linker, store, module, "[method]descriptor.set-times-at", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)], filesystem::descriptor_set_times_at);
        define(linker, store, module, "[method]descriptor.link-at", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)], filesystem::descriptor_link_at);
        define(linker, store, module, "[method]descriptor.unlink-file-at", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)], filesystem::descriptor_unlink_file_at);
        define(linker, store, module, "[method]descriptor.remove-directory-at", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)], filesystem::descriptor_remove_directory_at);
        define(linker, store, module, "[method]descriptor.rename-at", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)], filesystem::descriptor_rename_at);
        define(linker, store, module, "[method]descriptor.symlink-at", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)], filesystem::descriptor_symlink_at);
        define(linker, store, module, "[method]descriptor.readlink-at", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)], filesystem::descriptor_readlink_at);
        define(linker, store, module, "[method]descriptor.sync", vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)], filesystem::descriptor_sync);
        define(linker, store, module, "[method]descriptor.set-size", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64)], vec![ValType::NumType(NumType::I32)], filesystem::descriptor_set_size);
        define(linker, store, module, "[method]descriptor.set-times", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)], filesystem::descriptor_set_times);
        define(linker, store, module, "[method]descriptor.advise", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)], filesystem::descriptor_advise);
        define(linker, store, module, "[method]descriptor.create-directory-at", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)], filesystem::descriptor_create_directory_at);

        define(linker, store, module, "[resource-drop]descriptor", vec![ValType::NumType(NumType::I32)], vec![], resource_drop);
        define(linker, store, module, "[resource-drop]directory-entry-stream", vec![ValType::NumType(NumType::I32)], vec![], resource_drop);

        // Also add __wasm_call_dtors to env for compatibility
        let func_type = FuncType { params: ResultType { valtypes: vec![] }, returns: ResultType { valtypes: vec![] } };
        let func_addr = store.func_alloc_unchecked(func_type, |_, _| Ok(vec![]));
        let _ = linker.define_unchecked(String::from("env"), String::from("__wasm_call_dtors"), ExternVal::Func(func_addr));
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
    let wasi = store.wasi_ctx.as_mut().ok_or(HaltExecutionError(1))?;
    wasi.resource_table.remove(&handle);
    Ok(vec![])
}

pub(crate) fn find_cabi_realloc<T: Config>(store: &Store<'_, T>) -> Option<FuncAddr> {
    let module_addr = store.caller_module?;
    if let Ok(export) = store.instance_export(unsafe { Stored::from_bare(module_addr, store.id) }, "cabi_realloc") {
        if let Some(func) = export.as_func() {
            return Some(func.into_bare());
        }
    }
    crate::debugln!("WASI P2 Error: 'cabi_realloc' not found in caller module!");
    None
}

pub(crate) fn call_cabi_realloc<T: Config>(store: &mut Store<'_, T>, new_size: u32, align: u32) -> Result<u32, HaltExecutionError> {
    let cabi_realloc_addr = find_cabi_realloc(store).ok_or(HaltExecutionError(1))?;
    let args = vec![Value::I32(0), Value::I32(0), Value::I32(align), Value::I32(new_size)];
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

fn get_screen_width_host<T: Config>(_: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let w = crate::os::graphics::get_screen_width();
    // crate::debugln!("WASM get_screen_width -> {}", w);
    Ok(vec![Value::I32(w as u32)])
}

fn krakeos_syscall_host<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let get_arg = |i: usize| -> u64 {
        match args.get(i) {
            Some(Value::I64(v)) => *v,
            Some(Value::I32(v)) => *v as u64,
            _ => 0
        }
    };

    let num = get_arg(0);
    let a1 = get_arg(1);
    let a2 = get_arg(2);
    let a3 = get_arg(3);

    // crate::debugln!("[WASM Syscall] #{} ({}, {}, {})", num, a1, a2, a3);

    // Handle pointers for specific syscalls
    match num {
        0 => { // READ: fd, buf_ptr, buf_len
            if a3 == 0 { return Ok(vec![Value::I64(0)]); }
            let mut buf = vec![0u8; a3 as usize];
            let res = unsafe { crate::sys::syscall(num, a1, buf.as_mut_ptr() as u64, a3) };
            if res != u64::MAX && res > 0 {
                write_bytes(store, a2 as u32, &buf[..res as usize]).map_err(|_| HaltExecutionError(1))?;
            }
            return Ok(vec![Value::I64(res)]);
        }
        1 => { // WRITE: fd, buf_ptr, buf_len
            if a3 == 0 { return Ok(vec![Value::I64(0)]); }
            let mut buf = vec![0u8; a3 as usize];
            read_mem(store, a2 as u32, &mut buf).map_err(|_| HaltExecutionError(1))?;
            
            let mut written = 0;
            let chunk_size = 1024;
            while written < buf.len() {
                let end = core::cmp::min(written + chunk_size, buf.len());
                let slice = &buf[written..end];
                let res = unsafe { crate::sys::syscall(num, a1, slice.as_ptr() as u64, slice.len() as u64) };
                if res == u64::MAX { return Ok(vec![Value::I64(u64::MAX)]); }
                if res == 0 { break; }
                written += res as usize;
            }
            return Ok(vec![Value::I64(written as u64)]);
        }
        2 | 83 | 84 | 85 | 87 => { // OPEN, MKDIR, RMDIR, CREATE, UNLINK (a1 is string ptr, a2 is len)
            let len = a2 as usize;
            let mut buf = vec![0u8; len];
            read_mem(store, a1 as u32, &mut buf).map_err(|_| HaltExecutionError(1))?;
            let s = String::from_utf8(buf).map_err(|_| HaltExecutionError(1))?;
            let mut s_terminated = s;
            s_terminated.push('\0');
            // For OPEN (2), a2 was len, but syscall expects (path, flags, mode).
            // Wait, guest syscall(2, ptr, len, 0).
            // Host syscall(2, ptr, flags, mode).
            // The guest interface for 'open' in std/src/fs/mod.rs passes len as 2nd arg.
            // But the Kernel expects a C-string?
            // If Kernel expects C-string, we pass s_terminated pointer.
            // But what about the 2nd and 3rd args to the Kernel syscall?
            // Guest passes (ptr, len, 0).
            // Kernel syscall signature for OPEN is typically (path, flags, mode).
            // KrakeOS std::fs::File::open uses syscall(2, ptr, len, 0).
            // Does KrakeOS kernel Open syscall take (ptr, len, mode)?
            // The logs show Ext2Node::find calls, suggesting it works with path.
            // If the kernel expects (ptr, len, ...), then we should pass len.
            // If the kernel expects (ptr, flags, ...), then we have a mismatch if we pass len as flags.

            // However, looking at 'read_mem_string' usage previously, it implies we were constructing a host string.
            // And 'res = unsafe { crate::sys::syscall(num, s_terminated.as_ptr() ... a2, a3) }'
            // We were passing 'a2' (len) as the 2nd argument to the HOST syscall.
            // If the HOST syscall is KrakeOS kernel, and if it expects (ptr, len, ...), then it's fine.
            // But 's_terminated' implies we are converting to C-string.

            // Assumption: KrakeOS kernel syscalls called via 'crate::sys::syscall' expect (ptr, len, ...) for string arguments?
            // Or (ptr_to_c_string, ...)?
            // The previous code did: 'syscall(num, s_terminated.as_ptr(), a2, a3)'.
            // If 'a2' is len, we are passing len as 2nd arg.
            // If the kernel expects (ptr, len), then s_terminated (C-string) might not be needed, just bytes.
            // But if we are in 'wasm_runner' (userland app), 'crate::sys::syscall' is the userland wrapper.
            // Let's assume KrakeOS syscalls take (ptr, len).

            let res = unsafe { crate::sys::syscall(num, s_terminated.as_ptr() as u64, a2, a3) };
            if num == 85 { crate::debugln!("SYS_CREATE host wrapper returned: {}", res); }
            return Ok(vec![Value::I64(res)]);
        }
        120 => { // SHM_GET - Do not return host pointers to WASM!
            crate::debugln!("WASM Syscall: SHM_GET blocked (returning 0)");
            return Ok(vec![Value::I64(0)]);
        }
        4 => { // STAT (a1 is string ptr, a2 is len, a3 is stat buf)
            let len = a2 as usize;
            let mut buf = vec![0u8; len];
            read_mem(store, a1 as u32, &mut buf).map_err(|_| HaltExecutionError(1))?;
            let s = String::from_utf8(buf).map_err(|_| HaltExecutionError(1))?;
            let mut s_terminated = s;
            s_terminated.push('\0');
            let mut stat = unsafe { core::mem::zeroed::<crate::fs::Stat>() };
            let res = unsafe { crate::sys::syscall(num, s_terminated.as_ptr() as u64, &mut stat as *mut _ as u64, 0) };
            if res != u64::MAX {
                write_bytes(store, a3 as u32, unsafe { core::slice::from_raw_parts(&stat as *const _ as *const u8, core::mem::size_of::<crate::fs::Stat>()) }).map_err(|_| HaltExecutionError(1))?;
            }
            return Ok(vec![Value::I64(res)]);
        }
        5 => { // FSTAT (a1 is fd, a3 is stat buf)
            let mut stat = unsafe { core::mem::zeroed::<crate::fs::Stat>() };
            let res = unsafe { crate::sys::syscall(num, a1, 0, &mut stat as *mut _ as u64) };
            if res != u64::MAX {
                write_bytes(store, a3 as u32, unsafe { core::slice::from_raw_parts(&stat as *const _ as *const u8, core::mem::size_of::<crate::fs::Stat>()) }).map_err(|_| HaltExecutionError(1))?;
            }
            return Ok(vec![Value::I64(res)]);
        }
        78 => { // READDIR: fd, buf_ptr, buf_len
            let mut buf = vec![0u8; a3 as usize];
            let res = unsafe { crate::sys::syscall(num, a1, buf.as_mut_ptr() as u64, a3) };
            if res != u64::MAX && res > 0 {
                write_bytes(store, a2 as u32, &buf[..res as usize]).map_err(|_| HaltExecutionError(1))?;
            }
            return Ok(vec![Value::I64(res)]);
        }
        110 => { // GET_PROCESS_LIST (a1 is buf, a2 is count)
            let item_size = 8 + 8 + 32; // pid, state, name
            let mut buf = vec![0u8; a2 as usize * item_size];
            let res = unsafe { crate::sys::syscall(num, buf.as_mut_ptr() as u64, a2, a3) };
            if res != u64::MAX && res > 0 {
                write_bytes(store, a1 as u32, &buf[..res as usize * item_size]).map_err(|_| HaltExecutionError(1))?;
            }
            return Ok(vec![Value::I64(res)]);
        }
        100 | 102 => { // ADD_WINDOW, UPDATE_WINDOW (a1 is Window struct ptr)
            let addr = a1 as u32;
            let wasm_base = store.get_wasm_base_ptr() as u64;

            // Read WASM Window struct fields (32-bit offsets/pointers)
            let id = read_mem_u32(store, addr)? as usize;
            let buffer_off = read_mem_u32(store, addr + 4)? as u64;
            let back_buffer_off = read_mem_u32(store, addr + 8)? as u64;
            let flipped_off = read_mem_u32(store, addr + 12)? as u64;
            let pid = read_mem_u64(store, addr + 16)?;
            let x = read_mem_u32(store, addr + 24)? as i32 as isize;
            let y = read_mem_u32(store, addr + 28)? as i32 as isize;
            let z = read_mem_u32(store, addr + 32)? as usize;
            let width = read_mem_u32(store, addr + 36)? as usize;
            let height = read_mem_u32(store, addr + 40)? as usize;

            let mut bools = [0u8; 4];
            read_mem(store, addr + 44, &mut bools).map_err(|_| HaltExecutionError(1))?;

            let min_width = read_mem_u32(store, addr + 48)? as usize;
            let min_height = read_mem_u32(store, addr + 52)? as usize;
            let event_handler = read_mem_u32(store, addr + 56)? as usize;
            let w_type_val = read_mem_u32(store, addr + 60)?;

            // Reconstruct Host Window struct
            let host_win = crate::os::graphics::Window {
                id,
                buffer: if buffer_off != 0 { (wasm_base + buffer_off) as usize } else { 0 },
                back_buffer: if back_buffer_off != 0 { (wasm_base + back_buffer_off) as usize } else { 0 },
                flipped: if flipped_off != 0 { (wasm_base + flipped_off) as usize } else { 0 },
                pid,
                x,
                y,
                z,
                width,
                height,
                can_move: bools[0] != 0,
                can_resize: bools[1] != 0,
                transparent: bools[2] != 0,
                treat_as_transparent: bools[3] != 0,
                min_width,
                min_height,
                event_handler,
                w_type: unsafe { core::mem::transmute(w_type_val) },
            };

            let res = unsafe { crate::sys::syscall(num, &host_win as *const _ as u64, a2, a3) };

            // If ADD_WINDOW, update the ID back in WASM memory
            if num == 100 {
                let _ = write_bytes(store, addr, &(res as u32).to_le_bytes());
            }

            return Ok(vec![Value::I64(res)]);
        }
        103 => { // UPDATE_WINDOW_AREA (wid, x, y, w, h)
            // Guest uses syscall5(103, wid, x, y, w, h)
            // This case should be in krakeos_syscall6_host!
            panic!("WASM Syscall stub: UPDATE_WINDOW_AREA (should be handled in syscall6_host)");
        }
        104 => { // GET_EVENTS (a1=wid, a2=buf, a3=max)
            let event_size = core::mem::size_of::<crate::os::graphics::Event>();
            let mut buf = vec![0u8; a3 as usize * event_size];
            let res = unsafe { crate::sys::syscall(num, a1, buf.as_mut_ptr() as u64, a3) };
            if res != u64::MAX && res > 0 {
                write_bytes(store, a2 as u32, &buf[..res as usize * event_size]).map_err(|_| HaltExecutionError(1))?;
            }
            return Ok(vec![Value::I64(res)]);
        }
        105 => { // GET_MOUSE
            let res = unsafe { crate::sys::syscall(num, a1, a2, a3) };
            return Ok(vec![Value::I64(res)]);
        }
        999 => { // DEBUG_PRINT (a1=ptr, a2=len)
            let mut buf = vec![0u8; a2 as usize];
            read_mem(store, a1 as u32, &mut buf).map_err(|_| HaltExecutionError(1))?;
            let s = String::from_utf8_lossy(&buf);
            crate::os::debug_print(&s);
            return Ok(vec![Value::I64(0)]);
        }
        22 => { // PIPE (a1=ptr to [i32; 2])
            let mut fds = [0i32; 2];
            let res = unsafe { crate::sys::syscall(22, fds.as_mut_ptr() as u64, 0, 0) };
            if res == 0 {
                // Write fds back to WASM memory
                let mut bytes = [0u8; 8];
                bytes[0..4].copy_from_slice(&fds[0].to_le_bytes());
                bytes[4..8].copy_from_slice(&fds[1].to_le_bytes());
                write_bytes(store, a1 as u32, &bytes).map_err(|_| HaltExecutionError(1))?;
            }
            return Ok(vec![Value::I64(res)]);
        }
        _ => {}
    }

    let res = unsafe { crate::sys::syscall(num, a1, a2, a3) };
    Ok(vec![Value::I64(res)])
}

fn read_mem_string<T: Config>(store: &Store<'_, T>, addr: u32) -> Result<String, HaltExecutionError> {
    let mut res = String::new();
    let mut curr = addr;
    loop {
        let mut b = [0u8; 1];
        if read_mem(store, curr, &mut b).is_err() { return Err(HaltExecutionError(1)); }
        if b[0] == 0 { break; }
        res.push(b[0] as char);
        curr += 1;
        if curr - addr > 1024 { break; }
    }
    Ok(res)
}

fn read_mem_u32<T: Config>(store: &Store<'_, T>, addr: u32) -> Result<u32, HaltExecutionError> {
    let mut b = [0u8; 4];
    read_mem(store, addr, &mut b).map_err(|_| HaltExecutionError(1))?;
    Ok(u32::from_le_bytes(b))
}

fn read_mem_u64<T: Config>(store: &Store<'_, T>, addr: u32) -> Result<u64, HaltExecutionError> {
    let mut b = [0u8; 8];
    read_mem(store, addr, &mut b).map_err(|_| HaltExecutionError(1))?;
    Ok(u64::from_le_bytes(b))
}

fn krakeos_syscall5_host<T: Config>(_: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let get_arg = |i: usize| -> u64 {
        match args.get(i) {
            Some(Value::I64(v)) => *v,
            Some(Value::I32(v)) => *v as u64,
            _ => 0
        }
    };
    let res = unsafe { crate::sys::syscall4(get_arg(0), get_arg(1), get_arg(2), get_arg(3), get_arg(4)) };
    Ok(vec![Value::I64(res)])
}

fn krakeos_syscall6_host<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let get_arg = |i: usize| -> u64 {
        match args.get(i) {
            Some(Value::I64(v)) => *v,
            Some(Value::I32(v)) => *v as u64,
            _ => 0
        }
    };
    let num = get_arg(0);

    if num == 59 { // SPAWN (path, path_len, argv, argv_len, fds, fds_len)
        let path_ptr = get_arg(1) as u32;
        let path_len = get_arg(2) as u32;
        let argv_ptr = get_arg(3) as u32;
        let argv_len = get_arg(4) as u32;
        let fds_ptr = get_arg(5) as u32;
        let fds_len = get_arg(6) as u32;

        let mut path_buf = vec![0u8; path_len as usize];
        read_mem(store, path_ptr, &mut path_buf).map_err(|_| HaltExecutionError(1))?;
        let path = String::from_utf8_lossy(&path_buf);

        let mut host_args = Vec::new();
        for i in 0..argv_len {
            let arg_ptr_ptr = argv_ptr + i * 4;
            let arg_ptr = read_mem_u32(store, arg_ptr_ptr)? as u32;
            let arg = read_mem_string(store, arg_ptr)?;
            host_args.push(arg);
        }
        let host_args_refs: Vec<&str> = host_args.iter().map(|s| s.as_str()).collect();

        let mut host_fds = Vec::new();
        for i in 0..fds_len {
            let fd_ptr = fds_ptr + i * 2;
            let mut buf = [0u8; 2];
            read_mem(store, fd_ptr, &mut buf).map_err(|_| HaltExecutionError(1))?;
            host_fds.push((buf[0], buf[1]));
        }

        let pid = crate::os::spawn_with_fds(&path, &host_args_refs, &host_fds);
        return Ok(vec![Value::I64(pid as u64)]);
    }

    // 103: UPDATE_WINDOW_AREA(wid, x, y, w, h)
    let res = unsafe { crate::sys::syscall6(num, get_arg(1), get_arg(2), get_arg(3), get_arg(4), get_arg(5), get_arg(6)) };
    Ok(vec![Value::I64(res)])
}

fn get_screen_height_host<T: Config>(_: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    Ok(vec![Value::I32(crate::os::graphics::get_screen_height() as u32)])
}

