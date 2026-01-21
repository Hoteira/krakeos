use super::{call_cabi_realloc, read_bytes, read_mem, write_bytes, write_u32};
use crate::rust_alloc::{string::String, vec, vec::Vec};
use crate::wasm::{
    execution::{
        config::Config,
        store::{HaltExecutionError, Store},
        value::Value,
    },
    wasi::ctx::{InputStreamSource, OutputStreamSource, PollableTarget, WasiResource},
};

pub fn input_stream_subscribe<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let handle = match args.get(0) {
        Some(Value::I32(v)) => *v as i32,
        _ => return Ok(vec![Value::I32(0)]),
    };
    let wasi = store.wasi_ctx.as_mut().ok_or(HaltExecutionError)?;
    let id = wasi.next_resource_id;
    wasi.next_resource_id += 1;
    wasi.resource_table.insert(id, WasiResource::Pollable(PollableTarget::Read(handle)));
    Ok(vec![Value::I32(id as u32)])
}

pub fn output_stream_subscribe<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let handle = match args.get(0) {
        Some(Value::I32(v)) => *v as i32,
        _ => return Ok(vec![Value::I32(0)]),
    };
    let wasi = store.wasi_ctx.as_mut().ok_or(HaltExecutionError)?;
    let id = wasi.next_resource_id;
    wasi.next_resource_id += 1;
    wasi.resource_table.insert(id, WasiResource::Pollable(PollableTarget::Write(handle)));
    Ok(vec![Value::I32(id as u32)])
}

pub fn stream_read<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let handle = match args.get(0) {
        Some(Value::I32(v)) => *v as i32,
        _ => return Ok(vec![]),
    };
    let len_req = match args.get(1) {
        Some(Value::I64(v)) => *v as u64,
        _ => return Ok(vec![]),
    };
    let ret_ptr = match args.get(2) {
        Some(Value::I32(v)) => *v as u32,
        _ => return Ok(vec![]),
    };
    let source = {
        let wasi = store.wasi_ctx.as_ref().ok_or(HaltExecutionError)?;
        match wasi.resource_table.get(&handle) {
            Some(WasiResource::InputStream(s)) => match s {
                InputStreamSource::File(fd) => Some(*fd),
                InputStreamSource::Stdin => Some(0),
                _ => None,
            },
            _ => None,
        }
    };
    let buffer = if let Some(fd) = source {
        let read_len = core::cmp::min(len_req, 1024 * 64) as usize;
        let mut buf = vec![0u8; read_len];
        let bytes_read = crate::os::file_read(fd, &mut buf);
        buf.truncate(bytes_read);
        buf
    } else {
        Vec::new()
    };
    let ptr = if !buffer.is_empty() {
        match call_cabi_realloc(store, buffer.len() as u32, 1) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        }
    } else {
        0
    };
    if !buffer.is_empty() {
        if write_bytes(store, ptr, &buffer).is_err() {
            return Ok(vec![]);
        }
    }
    // Result<list<u8>, stream-error>
    // Tag 0 (OK)
    write_u32(store, ret_ptr, 0).map_err(|_| HaltExecutionError)?;
    // Payload (ptr, len)
    write_u32(store, ret_ptr + 4, ptr).map_err(|_| HaltExecutionError)?;
    write_u32(store, ret_ptr + 8, buffer.len() as u32).map_err(|_| HaltExecutionError)?;
    Ok(vec![])
}

pub fn stream_write<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let handle = match args.get(0) {
        Some(Value::I32(v)) => *v as i32,
        _ => return Ok(vec![]),
    };
    let ptr = match args.get(1) {
        Some(Value::I32(v)) => *v as u32,
        _ => return Ok(vec![]),
    };
    let len = match args.get(2) {
        Some(Value::I32(v)) => *v as u32,
        _ => return Ok(vec![]),
    };
    let ret_ptr = match args.get(3) {
        Some(Value::I32(v)) => *v as u32,
        _ => return Ok(vec![]),
    };

    crate::debugln!(
        "WASI P2: stream_write(handle: {}, ptr: {:#x}, len: {})",
        handle,
        ptr,
        len
    );
    let source = {
        let wasi = store.wasi_ctx.as_ref().ok_or(HaltExecutionError)?;
        match wasi.resource_table.get(&handle) {
            Some(WasiResource::OutputStream(source)) => Some(source.clone()),
            _ => None,
        }
    };
    if let Some(source) = source {
        let mut buf = vec![0u8; len as usize];
        if read_mem(store, ptr, &mut buf).is_err() {
            crate::debugln!("  Error: Failed to read guest memory at {:#x}", ptr);
            // Write Error (Tag 1)
            let _ = write_u32(store, ret_ptr, 1);
            return Ok(vec![]);
        }
        match source {
            OutputStreamSource::Stdout => {
                crate::os::file_write(1, &buf);
            }
            OutputStreamSource::Stderr => {
                crate::os::file_write(2, &buf);
            }
            OutputStreamSource::File(fd) => {
                crate::os::file_write(fd, &buf);
            }
            OutputStreamSource::Null => {}
        }
        // Write Success (Tag 0)
        write_u32(store, ret_ptr, 0).map_err(|_| HaltExecutionError)?;
        Ok(vec![])
    } else {
        crate::debugln!("  Error: Invalid output stream handle {}", handle);
        // Write Error (Tag 1)
        let _ = write_u32(store, ret_ptr, 1);
        Ok(vec![])
    }
}

pub fn poll_poll<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let in_ptr = match args.get(0) {
        Some(Value::I32(v)) => *v as u32,
        _ => return Ok(vec![]),
    };
    let in_len = match args.get(1) {
        Some(Value::I32(v)) => *v as u32,
        _ => return Ok(vec![]),
    };
    let ret_ptr = match args.get(2) {
        Some(Value::I32(v)) => *v as u32,
        _ => return Ok(vec![]),
    };
    let mut pollables = Vec::new();
    for i in 0..in_len {
        let mut bytes = [0u8; 4];
        if read_bytes(store, in_ptr + i * 4, &mut bytes).is_err() {
            return Ok(vec![]);
        }
        let handle = i32::from_le_bytes(bytes);
        pollables.push(handle);
    }
    let mut ready_indices = Vec::new();
    let mut min_deadline: Option<u64> = None;
    // Check readiness immediately
    {
        let wasi = store.wasi_ctx.as_ref().ok_or(HaltExecutionError)?;
        let now = (crate::os::get_system_ticks() * 1_000_000) as u64;
        for (idx, handle) in pollables.iter().enumerate() {
            if let Some(WasiResource::Pollable(target)) = wasi.resource_table.get(handle) {
                match target {
                    PollableTarget::Timer(deadline) => {
                        if now >= *deadline {
                            ready_indices.push(idx as u32);
                        } else {
                            if min_deadline.map(|d| *deadline < d).unwrap_or(true) {
                                min_deadline = Some(*deadline);
                            }
                        }
                    }
                    PollableTarget::Read(_stream) => {
                        ready_indices.push(idx as u32);
                    }
                    PollableTarget::Write(_stream) => {
                        ready_indices.push(idx as u32);
                    }
                }
            }
        }
    }
    // If nothing ready and we have a timer, wait
    if ready_indices.is_empty() {
        if let Some(deadline) = min_deadline {
            let now = (crate::os::get_system_ticks() * 1_000_000) as u64;
            if deadline > now {
                let wait_ns = deadline - now;
                let wait_ms = wait_ns / 1_000_000;
                if wait_ms > 0 {
                    crate::os::sleep(wait_ms);
                }
            }
            // Re-check timers
            let wasi = store.wasi_ctx.as_ref().ok_or(HaltExecutionError)?;
            let now = (crate::os::get_system_ticks() * 1_000_000) as u64;
            for (idx, handle) in pollables.iter().enumerate() {
                if let Some(WasiResource::Pollable(target)) = wasi.resource_table.get(handle) {
                    if let PollableTarget::Timer(d) = target {
                        if now >= *d {
                            ready_indices.push(idx as u32);
                        }
                    }
                }
            }
        }
    }
    // Write result
    let count = ready_indices.len() as u32;
    let out_ptr = if count > 0 {
        match call_cabi_realloc(store, count * 4, 4) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        }
    } else {
        0
    };
    if count > 0 {
        let mut buf = Vec::with_capacity((count * 4) as usize);
        for idx in ready_indices {
            buf.extend_from_slice(&idx.to_le_bytes());
        }
        if write_bytes(store, out_ptr, &buf).is_err() {
            return Ok(vec![]);
        }
    }
    write_u32(store, ret_ptr, out_ptr).map_err(|_| HaltExecutionError)?;
    write_u32(store, ret_ptr + 4, count).map_err(|_| HaltExecutionError)?;
    Ok(vec![])
}

pub fn poll_block<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let handle = match args.get(0) {
        Some(Value::I32(v)) => *v as i32,
        _ => return Ok(vec![]),
    };
    let mut ready = false;
    let mut deadline = None;
    {
        let wasi = store.wasi_ctx.as_ref().ok_or(HaltExecutionError)?;
        if let Some(WasiResource::Pollable(target)) = wasi.resource_table.get(&handle) {
            match target {
                PollableTarget::Timer(d) => {
                    deadline = Some(*d);
                    let now = (crate::os::get_system_ticks() * 1_000_000) as u64;
                    if now >= *d {
                        ready = true;
                    }
                }
                _ => ready = true,
            }
        } else {
            return Err(HaltExecutionError);
        }
    }
    if !ready {
        if let Some(d) = deadline {
            let now = (crate::os::get_system_ticks() * 1_000_000) as u64;
            if d > now {
                let wait_ms = (d - now) / 1_000_000;
                if wait_ms > 0 {
                    crate::os::sleep(wait_ms);
                }
            }
        }
    }
    Ok(vec![])
}

pub fn error_to_debug_string<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    panic!("WASI P2 stub: error_to_debug_string");
}
