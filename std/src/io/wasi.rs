use crate::alloc::{vec, vec::Vec};
use crate::wasm::{
    common::{config::Config, value::Value, reader::types::{ValType, NumType}},
    interpreter::store::{HaltExecutionError, Store},
    wasi::ctx::{InputStreamSource, OutputStreamSource, PollableTarget, WasiResource},
};
use crate::wasm::wasi::preview2::{call_cabi_realloc, read_mem, write_bytes, write_u32, write_u64, read_bytes};

crate::export_method!(
    "wasi:io/streams@0.2.0", "[method]input-stream.subscribe",
    [],
    vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
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
);

crate::export_method!(
    "wasi:io/streams@0.2.0", "[method]output-stream.subscribe",
    [],
    vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
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
);

crate::export_method!(
    "wasi:io/streams@0.2.0", "[method]input-stream.read",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![],
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
                    InputStreamSource::File(fd) => Some((Some(*fd), None)),
                    InputStreamSource::Stdin => Some((Some(0), None)),
                    InputStreamSource::GuestFd(fd) => Some((None, Some(*fd))),
                    _ => None,
                },
                Some(WasiResource::Descriptor(fd)) => Some((None, Some(*fd))),
                Some(WasiResource::File(f)) => Some((None, Some(f.as_raw_fd() as i32))),
                _ => None,
            }
        };
        let buffer = if let Some((host_fd, guest_fd)) = source {
            let read_len = core::cmp::min(len_req, 1024 * 64) as usize;
            let mut buf = vec![0u8; read_len];
            
            if let Some(fd) = guest_fd {
                let mut slices = [buf.as_mut_slice()];
                match store.wasi_ctx.as_mut().unwrap().env.fd_read(fd, &mut slices) {
                    Ok(n) => {
                        buf.truncate(n);
                        buf
                    }
                    Err(_) => vec![]
                }
            } else if let Some(fd) = host_fd {
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
                                        if buf[i] == b'\n' {
                                            buf[i] = b'\r';
                                        }
                                    }
                                    // Echo input
                                    let host_stdout = store.wasi_ctx.as_ref().unwrap().env.stdio_map()[1] as usize;
                                    crate::os::file_write(host_stdout, &buf[..bytes_read]);
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
        let _ = write_u32(store, ret_ptr, 0);
        // Payload (ptr, len)
        let _ = write_u32(store, ret_ptr + 4, ptr);
        let _ = write_u32(store, ret_ptr + 8, buffer.len() as u32);
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi:io/streams@0.2.0", "[method]input-stream.skip",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![],
    pub fn stream_skip<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let handle = match args.get(0) { Some(Value::I32(v)) => *v as i32, _ => return Ok(vec![]) };
        let len_req = match args.get(1) { Some(Value::I64(v)) => *v as u64, _ => return Ok(vec![]) };
        let ret_ptr = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![]) };
        
        let source = {
            let wasi = store.wasi_ctx.as_ref().ok_or(HaltExecutionError(1))?;
            match wasi.resource_table.get(&handle) {
                Some(WasiResource::InputStream(s)) => match s {
                    InputStreamSource::File(fd) => Some((Some(*fd), None)),
                    InputStreamSource::Stdin => Some((Some(0), None)),
                    InputStreamSource::GuestFd(fd) => Some((None, Some(*fd))),
                    _ => None,
                },
                Some(WasiResource::Descriptor(fd)) => Some((None, Some(*fd))),
                Some(WasiResource::File(f)) => Some((None, Some(f.as_raw_fd() as i32))),
                _ => None,
            }
        };

        let skipped = if let Some((host_fd, guest_fd)) = source {
            let mut remaining = len_req;
            let mut buf = vec![0u8; 4096];
            let mut total_skipped = 0;
            
            while remaining > 0 {
                let to_read = core::cmp::min(remaining, buf.len() as u64) as usize;
                let bytes_read = if let Some(fd) = guest_fd {
                    let mut slices = [buf[..to_read].as_mut()];
                    match store.wasi_ctx.as_mut().unwrap().env.fd_read(fd, &mut slices) {
                        Ok(n) => n,
                        Err(_) => 0
                    }
                } else if let Some(fd) = host_fd {
                    let actual_fd = if fd == 0 { store.wasi_ctx.as_ref().unwrap().env.stdio_map()[0] as usize } else { fd };
                    crate::os::file_read(actual_fd, &mut buf[..to_read])
                } else {
                    0
                };

                if bytes_read == 0 { break; }
                if bytes_read == usize::MAX - 1 {
                     break;
                }
                
                total_skipped += bytes_read as u64;
                remaining -= bytes_read as u64;
            }
            total_skipped
        } else {
            0
        };

        let _ = write_u32(store, ret_ptr, 0); // Ok
        let _ = write_u64(store, ret_ptr + 8, skipped);
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi:io/streams@0.2.0", "[method]output-stream.check-write",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
    pub fn stream_check_write<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let ret_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![]) };
        // Return 64KB capacity for all streams for now
        let _ = write_u32(store, ret_ptr, 0); // Ok tag
        let _ = write_u64(store, ret_ptr + 8, 64 * 1024);
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi:io/streams@0.2.0", "[method]output-stream.write",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
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
                Some(WasiResource::Descriptor(fd)) => Some(OutputStreamSource::GuestFd(*fd)),
                Some(WasiResource::File(f)) => Some(OutputStreamSource::GuestFd(f.as_raw_fd() as i32)),
                _ => None,
            }
        };
        if let Some(source) = source {
            let mut buf = vec![0u8; len as usize];
            if read_mem(store, ptr, &mut buf).is_err() {
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
                OutputStreamSource::GuestFd(fd) => {
                    let slices = [&buf[..]];
                    let _ = store.wasi_ctx.as_mut().unwrap().env.fd_write(fd, &slices);
                }
                OutputStreamSource::Null => {}
            }
            let _ = write_u32(store, ret_ptr, 0);
            Ok(vec![])
        } else {
            let _ = write_u32(store, ret_ptr, 1);
            Ok(vec![])
        }
    }
);

crate::export_method!(
    "wasi:io/streams@0.2.0", "[method]output-stream.blocking-write-and-flush",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
    pub fn stream_blocking_write_and_flush<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        // Same logic as write for now, but name matches
        stream_write::<T>(store, args)
    }
);

crate::export_method!(
    "wasi:io/streams@0.2.0", "[method]output-stream.write-zeroes",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![],
    pub fn stream_write_zeroes<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let handle = match args.get(0) { Some(Value::I32(v)) => *v as i32, _ => return Ok(vec![]) };
        let len = match args.get(1) { Some(Value::I64(v)) => *v as u64, _ => return Ok(vec![]) };
        let ret_ptr = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![]) };

        let chunk_size = 4096;
        let zeros = vec![0u8; chunk_size];
        let mut remaining = len;
        
        let source = {
            let wasi = store.wasi_ctx.as_ref().ok_or(HaltExecutionError(1))?;
            match wasi.resource_table.get(&handle) {
                Some(WasiResource::OutputStream(source)) => Some(source.clone()),
                Some(WasiResource::Descriptor(fd)) => Some(OutputStreamSource::GuestFd(*fd)),
                Some(WasiResource::File(f)) => Some(OutputStreamSource::GuestFd(f.as_raw_fd() as i32)),
                _ => None,
            }
        };

        if let Some(source) = source {
            let wasi = store.wasi_ctx.as_ref().ok_or(HaltExecutionError(1))?;
            let stdio_map = wasi.env.stdio_map();
            
            while remaining > 0 {
                let to_write = core::cmp::min(remaining, chunk_size as u64) as usize;
                let slice = &zeros[..to_write];
                
                match &source {
                    OutputStreamSource::Stdout => { crate::os::file_write(stdio_map[1] as usize, slice); }
                    OutputStreamSource::Stderr => { crate::os::file_write(stdio_map[2] as usize, slice); }
                    OutputStreamSource::File(fd) => { crate::os::file_write(*fd, slice); }
                    OutputStreamSource::GuestFd(fd) => { 
                        let slices = [slice];
                        let _ = store.wasi_ctx.as_mut().unwrap().env.fd_write(*fd, &slices);
                    }
                    OutputStreamSource::Null => {}
                }
                remaining -= to_write as u64;
            }
            
            let _ = write_u32(store, ret_ptr, 0);
            Ok(vec![])
        } else {
            let _ = write_u32(store, ret_ptr, 1);
            Ok(vec![])
        }
    }
);

crate::export_method!(
    "wasi:io/streams@0.2.0", "[method]output-stream.flush",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
    pub fn stream_flush<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let ret_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![]) };
        let _ = write_u32(store, ret_ptr, 0);
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi:io/streams@0.2.0", "[method]output-stream.splice",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![],
    pub fn stream_splice<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let ret_ptr = match args.get(3) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![]) };
        let _ = write_u32(store, ret_ptr, 0);
        let _ = write_u64(store, ret_ptr + 8, 0);
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi:io/poll@0.2.0", "poll",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
    pub fn poll_poll<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let in_ptr = match args.get(0) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![]) };
        let in_len = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![]) };
        let ret_ptr = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![]) };

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
            let _ = write_bytes(store, out_ptr, &buf);
        }
        let _ = write_u32(store, ret_ptr, out_ptr);
        let _ = write_u32(store, ret_ptr + 4, count);
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi:io/poll@0.2.0", "[method]pollable.block",
    [],
    vec![ValType::NumType(NumType::I32)], vec![],
    pub fn poll_block<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let handle = match args.get(0) { Some(Value::I32(v)) => *v as i32, _ => return Ok(vec![]) };
        let mut ready = false;
        let mut deadline = None;
        {
            let wasi = store.wasi_ctx.as_ref().ok_or(HaltExecutionError(1))?;
            if let Some(WasiResource::Pollable(target)) = wasi.resource_table.get(&handle) {
                match target {
                    PollableTarget::Timer(d) => {
                        deadline = Some(*d);
                        let now = (crate::os::get_system_ticks() * 1_000_000) as u64;
                        if now >= *d { ready = true; }
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
                    if wait_ms > 0 { crate::os::sleep(wait_ms); }
                }
            }
        }
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi:io/error@0.2.0", "[method]error.to-debug-string",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
    pub fn error_to_debug_string<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let ret_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![]) };
        let msg = "WASI Error (Debug info unavailable)";
        let bytes = msg.as_bytes();
        let ptr = call_cabi_realloc(store, bytes.len() as u32, 1)?;
        let _ = write_bytes(store, ptr, bytes);
        let _ = write_u32(store, ret_ptr, ptr);
        let _ = write_u32(store, ret_ptr + 4, bytes.len() as u32);
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi_snapshot_preview1", "poll_oneoff",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn poll_oneoff_p1<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let in_ptr = match args.get(0) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let out_ptr = match args.get(1) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let nsubs = match args.get(2) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let ret_ptr = match args.get(3) { Some(Value::I32(x)) => *x as u32, _ => 0 };

        let mut in_buf = vec![0u8; nsubs as usize * 48];
        if read_bytes(store, in_ptr, &mut in_buf).is_err() { return Ok(vec![Value::I32(21)]); }
        let mut out_buf = vec![0u8; nsubs as usize * 32];
        match wasi_ctx(store).env.poll_oneoff(&in_buf, &mut out_buf, nsubs) {
            Ok(n) => {
                let _ = write_bytes(store, out_ptr, &out_buf[..n as usize * 32]);
                let _ = write_u32(store, ret_ptr, n);
                Ok(vec![Value::I32(0)])
            }
            Err(e) => Ok(vec![Value::I32(e as u32)]),
        }
    }
);

crate::export_method!(
    "wasi:io/streams@0.2.0", "[resource-drop]input-stream",
    [],
    vec![ValType::NumType(NumType::I32)], vec![],
    pub fn input_stream_drop<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        crate::wasm::wasi::preview2::resource_drop(store, args)
    }
);

crate::export_method!(
    "wasi:io/streams@0.2.0", "[resource-drop]output-stream",
    [],
    vec![ValType::NumType(NumType::I32)], vec![],
    pub fn output_stream_drop<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        crate::wasm::wasi::preview2::resource_drop(store, args)
    }
);

crate::export_method!(
    "wasi:io/poll@0.2.0", "[resource-drop]pollable",
    [],
    vec![ValType::NumType(NumType::I32)], vec![],
    pub fn pollable_drop<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        crate::wasm::wasi::preview2::resource_drop(store, args)
    }
);

crate::export_method!(
    "wasi:io/error@0.2.0", "[resource-drop]error",
    [],
    vec![ValType::NumType(NumType::I32)], vec![],
    pub fn error_drop<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        crate::wasm::wasi::preview2::resource_drop(store, args)
    }
);

pub fn register_wasi<T: Config + Clone>(linker: &mut crate::wasm::Linker, store: &mut crate::wasm::Store<'_, T>) {
    input_stream_subscribe::register(linker, store);
    output_stream_subscribe::register(linker, store);
    stream_read::register(linker, store);
    stream_skip::register(linker, store);
    stream_check_write::register(linker, store);
    stream_write::register(linker, store);
    stream_blocking_write_and_flush::register(linker, store);
    stream_write_zeroes::register(linker, store);
    stream_flush::register(linker, store);
    stream_splice::register(linker, store);
    poll_poll::register(linker, store);
    poll_block::register(linker, store);
    error_to_debug_string::register(linker, store);
    poll_oneoff_p1::register(linker, store);
    input_stream_drop::register(linker, store);
    output_stream_drop::register(linker, store);
    pollable_drop::register(linker, store);
    error_drop::register(linker, store);
}

fn wasi_ctx<'a, T: Config>(store: &'a mut Store<'_, T>) -> &'a mut crate::wasm::wasi::ctx::WasiCtx {
    if store.wasi_ctx.is_none() {
        store.wasi_ctx = Some(crate::wasm::wasi::ctx::WasiCtx::default());
    }
    store.wasi_ctx.as_mut().unwrap()
}
