use super::{call_cabi_realloc, read_bytes, read_mem, write_bytes, write_u32};
use crate::rust_alloc::{vec, vec::Vec};
use crate::wasm::{
    common::{config::Config, value::Value},
    interpreter::store::{HaltExecutionError, Store},
    wasi::ctx::{InputStreamSource, OutputStreamSource, PollableTarget, WasiResource},
};

pub fn input_stream_subscribe<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let handle = match args.get(0) {
        Some(Value::I32(v)) => *v as i32,
        _ => return Ok(vec![Value::I32(0)]),
    };
    let wasi = store.wasi_ctx.as_mut().ok_or(HaltExecutionError(1))?;
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
    let wasi = store.wasi_ctx.as_mut().ok_or(HaltExecutionError(1))?;
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
        let wasi = store.wasi_ctx.as_ref().ok_or(HaltExecutionError(1))?;
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
        if fd == 0 {
            let host_fd = store.wasi_ctx.as_ref().unwrap().env.stdio_map()[0] as usize;
            loop {
                let bytes_read = crate::os::file_read(host_fd, &mut buf);
                if bytes_read == usize::MAX - 1 { // EWOULDBLOCK
                    crate::os::yield_task();
                    continue;
                }
                if bytes_read > buf.len() {
                    return Ok(vec![]);
                }
                if bytes_read > 0 {
                    unsafe {
                        if crate::wasm::wasi::ICRNL {
                            for i in 0..bytes_read {
                                if buf[i] == b'\r' {
                                    buf[i] = b'\n';
                                }
                            }
                        }
                    }
                    buf.truncate(bytes_read);
                    break buf;
                }
                crate::os::yield_task();
            }
        } else {
            loop {
                let bytes_read = crate::os::file_read(fd, &mut buf);
                if bytes_read == usize::MAX - 1 {
                    crate::os::yield_task();
                    continue;
                }
                buf.truncate(bytes_read);
                break buf;
            }
        }
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
    write_u32(store, ret_ptr, 0).map_err(|_| HaltExecutionError(1))?;
    // Payload (ptr, len)
    write_u32(store, ret_ptr + 4, ptr).map_err(|_| HaltExecutionError(1))?;
    write_u32(store, ret_ptr + 8, buffer.len() as u32).map_err(|_| HaltExecutionError(1))?;
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

    let source = {
        let wasi = store.wasi_ctx.as_ref().ok_or(HaltExecutionError(1))?;
        match wasi.resource_table.get(&handle) {
            Some(WasiResource::OutputStream(source)) => Some(source.clone()),
            _ => None,
        }
    };
    if let Some(source) = source {
        let mut buf = vec![0u8; len as usize];
        if read_mem(store, ptr, &mut buf).is_err() {
            // Write Error (Tag 1)
            let _ = write_u32(store, ret_ptr, 1);
            return Ok(vec![]);
        }
        let wasi = store.wasi_ctx.as_ref().ok_or(HaltExecutionError(1))?;
        let stdio_map = wasi.env.stdio_map();
        match source {
            OutputStreamSource::Stdout => {
                let mut written = 0;
                while written < buf.len() {
                    let n = crate::os::file_write(stdio_map[1] as usize, &buf[written..]);
                    if n == usize::MAX - 1 {
                        crate::os::yield_task();
                        continue;
                    }
                    if n == 0 { break; }
                    written += n;
                }
            }
            OutputStreamSource::Stderr => {
                let mut written = 0;
                while written < buf.len() {
                    let n = crate::os::file_write(stdio_map[2] as usize, &buf[written..]);
                    if n == usize::MAX - 1 {
                        crate::os::yield_task();
                        continue;
                    }
                    if n == 0 { break; }
                    written += n;
                }
            }
            OutputStreamSource::File(fd) => {
                loop {
                    let n = crate::os::file_write(fd, &buf);
                    if n == usize::MAX - 1 {
                        crate::os::yield_task();
                        continue;
                    }
                    break;
                }
            }
            OutputStreamSource::Null => {}
        }
        // Write Success (Tag 0)
        write_u32(store, ret_ptr, 0).map_err(|_| HaltExecutionError(1))?;
        Ok(vec![])
    } else {
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

    let mut poll_fds = Vec::new();
    let mut clock_deadline_ns: Option<u64> = None;
    let mut handles = Vec::new();

    {
        let wasi = store.wasi_ctx.as_ref().ok_or(HaltExecutionError(1))?;
        for i in 0..in_len {
            let mut bytes = [0u8; 4];
            if read_bytes(store, in_ptr + i * 4, &mut bytes).is_err() {
                return Ok(vec![]);
            }
            let handle = i32::from_le_bytes(bytes);
            handles.push(handle);

            if let Some(WasiResource::Pollable(target)) = wasi.resource_table.get(&handle) {
                let stdio_map = wasi.env.stdio_map();
                match target {
                    PollableTarget::Timer(deadline) => {
                        if clock_deadline_ns.map(|d| *deadline < d).unwrap_or(true) {
                            clock_deadline_ns = Some(*deadline);
                        }
                    }
                    PollableTarget::Read(h) => {
                        let host_fd = if *h < 3 { stdio_map[*h as usize] } else { *h };
                        poll_fds.push(crate::os::PollFd { fd: host_fd, events: crate::os::POLLIN, revents: 0 });
                    }
                    PollableTarget::Write(h) => {
                        let host_fd = if *h < 3 { stdio_map[*h as usize] } else { *h };
                        poll_fds.push(crate::os::PollFd { fd: host_fd, events: crate::os::POLLOUT, revents: 0 });
                    }
                }
            }
        }
    }

    let timeout_ms = if let Some(deadline) = clock_deadline_ns {
        let now = crate::os::get_system_ticks() * 1_000_000;
        if deadline > now { ((deadline - now) / 1_000_000) as i32 } else { 0 }
    } else {
        -1
    };

    if !poll_fds.is_empty() || timeout_ms >= 0 {
        let _ = crate::os::poll(&mut poll_fds, timeout_ms);
    }

    let mut ready_indices = Vec::new();
    {
        let wasi = store.wasi_ctx.as_ref().ok_or(HaltExecutionError(1))?;
        let now = crate::os::get_system_ticks() * 1_000_000;
        for (idx, handle) in handles.iter().enumerate() {
            if let Some(WasiResource::Pollable(target)) = wasi.resource_table.get(handle) {
                match target {
                    PollableTarget::Timer(d) => {
                        if now >= *d { ready_indices.push(idx as u32); }
                    }
                    PollableTarget::Read(h) => {
                        if let Some(pfd) = poll_fds.iter().find(|p| p.fd == *h) {
                            if (pfd.revents & crate::os::POLLIN) != 0 { ready_indices.push(idx as u32); }
                        }
                    }
                    PollableTarget::Write(h) => {
                        if let Some(pfd) = poll_fds.iter().find(|p| p.fd == *h) {
                            if (pfd.revents & crate::os::POLLOUT) != 0 { ready_indices.push(idx as u32); }
                        }
                    }
                }
            }
        }
    }

    let count = ready_indices.len() as u32;
    let out_ptr = if count > 0 {
        match call_cabi_realloc(store, count * 4, 4) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        }
    } else { 0 };

    if count > 0 {
        let mut buf = Vec::with_capacity((count * 4) as usize);
        for idx in ready_indices {
            buf.extend_from_slice(&idx.to_le_bytes());
        }
        if write_bytes(store, out_ptr, &buf).is_err() {
            return Ok(vec![]);
        }
    }
    write_u32(store, ret_ptr, out_ptr).map_err(|_| HaltExecutionError(1))?;
    write_u32(store, ret_ptr + 4, count).map_err(|_| HaltExecutionError(1))?;
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
        let wasi = store.wasi_ctx.as_ref().ok_or(HaltExecutionError(1))?;
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
            return Err(HaltExecutionError(1));
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
