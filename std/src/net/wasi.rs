use crate::alloc::{vec, vec::Vec};
use crate::wasm::{
    common::{config::Config, value::Value, reader::types::{ValType, NumType}},
    interpreter::store::{HaltExecutionError, Store},
};
use crate::wasm::wasi::preview2::{read_mem, write_bytes, write_u32, write_u64};
use crate::os::krakeos as host;

crate::export_method!(
    "wasi:sockets/tcp@0.2.0", "[constructor]tcp-socket",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
    pub fn tcp_create_socket<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let address_family = match args.get(0) { Some(Value::I32(v)) => *v, _ => return Err(HaltExecutionError(1)) };
        let result_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
        
        let res = host::socket_create(address_family as u64, 1); // 1 = SOCK_STREAM
        
        let mut buf = [0u8; 8];
        if res <= i32::MAX as u64 {
            buf[0] = 0;
            buf[4..8].copy_from_slice(&(res as i32).to_le_bytes());
        } else {
            buf[0] = 1;
        }
        write_bytes(store, result_ptr, &buf).map_err(|_| HaltExecutionError(1))?;
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi:sockets/tcp@0.2.0", "[method]tcp-socket.start-bind",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
    pub fn tcp_start_bind<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let socket = match args.get(0) { Some(Value::I32(v)) => *v, _ => return Err(HaltExecutionError(1)) };
        let ip_addr_ptr = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
        let result_ptr = match args.get(3) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
        let mut ip_addr = vec![0u8; 16];
        if read_mem(store, ip_addr_ptr, &mut ip_addr).is_err() { return Err(HaltExecutionError(1)); }
        
        let res = host::socket_bind(socket as u64, ip_addr.as_ptr(), 16);
        
        let mut buf = [0u8; 4];
        buf[0] = if res == 0 { 0 } else { 1 };
        write_bytes(store, result_ptr, &buf).map_err(|_| HaltExecutionError(1))?;
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi:sockets/tcp@0.2.0", "[method]tcp-socket.finish-bind",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
    pub fn tcp_finish_bind<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let result_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
        let mut buf = [0u8; 4];
        buf[0] = 0; // Ok
        write_bytes(store, result_ptr, &buf).map_err(|_| HaltExecutionError(1))?;
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi:sockets/tcp@0.2.0", "[method]tcp-socket.start-connect",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
    pub fn tcp_start_connect<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let socket = match args.get(0) { Some(Value::I32(v)) => *v, _ => return Err(HaltExecutionError(1)) };
        let ip_addr_ptr = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
        let result_ptr = match args.get(3) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
        let mut ip_addr = vec![0u8; 16];
        if read_mem(store, ip_addr_ptr, &mut ip_addr).is_err() { return Err(HaltExecutionError(1)); }
        
        let res = host::socket_connect(socket as u64, ip_addr.as_ptr(), 16);
        
        let mut buf = [0u8; 4];
        buf[0] = if res == 0 { 0 } else { 1 };
        write_bytes(store, result_ptr, &buf).map_err(|_| HaltExecutionError(1))?;
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi:sockets/tcp@0.2.0", "[method]tcp-socket.finish-connect",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
    pub fn tcp_finish_connect<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let socket = match args.get(0) { Some(Value::I32(v)) => *v, _ => return Err(HaltExecutionError(1)) };
        let result_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
        
        let res = host::socket_finish_connect(socket as u64);
        
        let mut buf = [0u8; 4];
        buf[0] = if res == 0 { 0 } else { 1 };
        write_bytes(store, result_ptr, &buf).map_err(|_| HaltExecutionError(1))?;
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi:sockets/tcp@0.2.0", "[method]tcp-socket.start-listen",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
    pub fn tcp_start_listen<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let socket = match args.get(0) { Some(Value::I32(v)) => *v, _ => return Err(HaltExecutionError(1)) };
        let result_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
        
        let res = host::socket_listen(socket as u64, 10);
        
        let mut buf = [0u8; 4];
        buf[0] = if res != u64::MAX { 0 } else { 1 };
        write_bytes(store, result_ptr, &buf).map_err(|_| HaltExecutionError(1))?;
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi:sockets/tcp@0.2.0", "[method]tcp-socket.finish-listen",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
    pub fn tcp_finish_listen<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let result_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
        let mut buf = [0u8; 4];
        buf[0] = 0;
        write_bytes(store, result_ptr, &buf).map_err(|_| HaltExecutionError(1))?;
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi:sockets/tcp@0.2.0", "[method]tcp-socket.accept",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
    pub fn tcp_accept<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let socket = match args.get(0) { Some(Value::I32(v)) => *v, _ => return Err(HaltExecutionError(1)) };
        let result_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
        
        let res = host::socket_accept(socket as u64);
        
        let mut buf = [0u8; 8];
        if res <= i32::MAX as u64 {
            buf[0] = 0;
            buf[4..8].copy_from_slice(&(res as i32).to_le_bytes());
        } else {
            buf[0] = 1;
        }
        write_bytes(store, result_ptr, &buf).map_err(|_| HaltExecutionError(1))?;
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi:sockets/tcp@0.2.0", "[method]tcp-socket.send",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
    pub fn tcp_send<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let socket = match args.get(0) { Some(Value::I32(v)) => *v, _ => return Err(HaltExecutionError(1)) };
        let buf_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
        let buf_len = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
        let result_ptr = match args.get(3) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
        let mut payload = vec![0u8; buf_len as usize];
        if read_mem(store, buf_ptr, &mut payload).is_err() { return Err(HaltExecutionError(1)); }
        
        let res = host::socket_send(socket as u64, payload.as_ptr(), buf_len as u64);
        
        let mut out_buf = [0u8; 16];
        if res <= buf_len as u64 {
            out_buf[0] = 0;
            out_buf[8..16].copy_from_slice(&res.to_le_bytes());
        } else { out_buf[0] = 1; }
        write_bytes(store, result_ptr, &out_buf).map_err(|_| HaltExecutionError(1))?;
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi:sockets/tcp@0.2.0", "[method]tcp-socket.recv",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
    pub fn tcp_recv<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let socket = match args.get(0) { Some(Value::I32(v)) => *v, _ => return Err(HaltExecutionError(1)) };
        let max_len = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
        let result_ptr = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
        let mut payload = vec![0u8; max_len as usize];
        
        let res = host::socket_recv(socket as u64, payload.as_mut_ptr(), max_len as u64);
        
        if res <= max_len as u64 {
            let res_usize = res as usize;
            let mut header = vec![0u8; 32];
            header[0] = 0;
            header[8..16].copy_from_slice(&res.to_le_bytes());
            if res_usize > 0 {
                header.extend_from_slice(&payload[..res_usize]);
            }
            write_bytes(store, result_ptr, &header).map_err(|_| HaltExecutionError(1))?;
        } else {
            let mut header = [0u8; 32];
            header[0] = 1;
            write_bytes(store, result_ptr, &header).map_err(|_| HaltExecutionError(1))?;
        }
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi:sockets/tcp@0.2.0", "[method]tcp-socket.local-address",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
    pub fn tcp_local_address<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let socket = match args.get(0) { Some(Value::I32(v)) => *v, _ => return Err(HaltExecutionError(1)) };
        let result_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
        
        let mut addr = [0u8; 16];
        let res = host::socket_get_local_addr(socket as u64, addr.as_mut_ptr());
        
        let mut buf = [0u8; 32];
        if res == 0 {
            buf[0] = 0; // Ok
            buf[8..24].copy_from_slice(&addr);
        } else {
            buf[0] = 1; // Err
        }
        write_bytes(store, result_ptr, &buf).map_err(|_| HaltExecutionError(1))?;
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi:sockets/tcp@0.2.0", "[method]tcp-socket.remote-address",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
    pub fn tcp_remote_address<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let socket = match args.get(0) { Some(Value::I32(v)) => *v, _ => return Err(HaltExecutionError(1)) };
        let result_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
        
        let mut addr = [0u8; 16];
        let res = host::socket_get_remote_addr(socket as u64, addr.as_mut_ptr());
        
        let mut buf = [0u8; 32];
        if res == 0 {
            buf[0] = 0; // Ok
            buf[8..24].copy_from_slice(&addr);
        } else {
            buf[0] = 1; // Err
        }
        write_bytes(store, result_ptr, &buf).map_err(|_| HaltExecutionError(1))?;
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi:sockets/tcp@0.2.0", "[method]tcp-socket.is-listening",
    [],
    vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn tcp_is_listening<T: Config>(_: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        Ok(vec![Value::I32(0)]) // Stub: default false
    }
);

crate::export_method!(
    "wasi:sockets/tcp@0.2.0", "[method]tcp-socket.address-family",
    [],
    vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn tcp_address_family<T: Config>(_: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        Ok(vec![Value::I32(0)]) // Stub: default ipv4
    }
);

crate::export_method!(
    "wasi:sockets/tcp@0.2.0", "[method]tcp-socket.shutdown",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
    pub fn tcp_shutdown<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let socket = match args.get(0) { Some(Value::I32(v)) => *v, _ => return Err(HaltExecutionError(1)) };
        let how = match args.get(1) { Some(Value::I32(v)) => *v, _ => return Err(HaltExecutionError(1)) };
        let result_ptr = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
        
        let res = host::socket_shutdown(socket as u64, how as u64);
        
        let mut buf = [0u8; 4];
        buf[0] = if res == 0 { 0 } else { 1 };
        write_bytes(store, result_ptr, &buf).map_err(|_| HaltExecutionError(1))?;
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi:sockets/tcp@0.2.0", "[method]tcp-socket.set-listen-backlog-size",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![],
    pub fn tcp_set_listen_backlog_size<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let socket = match args.get(0) { Some(Value::I32(v)) => *v, _ => return Err(HaltExecutionError(1)) };
        let backlog = match args.get(1) { Some(Value::I64(v)) => *v, _ => return Err(HaltExecutionError(1)) };
        let result_ptr = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
        
        let res = host::socket_listen(socket as u64, backlog);
        
        let mut buf = [0u8; 4];
        buf[0] = if res != u64::MAX { 0 } else { 1 };
        write_bytes(store, result_ptr, &buf).map_err(|_| HaltExecutionError(1))?;
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi:sockets/tcp@0.2.0", "[method]tcp-socket.subscribe",
    [],
    vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn tcp_subscribe<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let handle = match args.get(0) { Some(Value::I32(v)) => *v as i32, _ => return Err(HaltExecutionError(1)) };
        let wasi = store.wasi_ctx.as_mut().ok_or(HaltExecutionError(1))?;
        let id = wasi.next_resource_id;
        wasi.next_resource_id += 1;
        // Map to a Read pollable for now
        wasi.resource_table.insert(id, crate::wasm::wasi::ctx::WasiResource::Pollable(crate::wasm::wasi::ctx::PollableTarget::Read(handle)));
        Ok(vec![Value::I32(id as u32)])
    }
);

// TCP Options Stubs

crate::export_method!(
    "wasi:sockets/tcp@0.2.0", "[method]tcp-socket.keep-alive-enabled",
    [], vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn tcp_get_keep_alive_enabled<T: Config>(_: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> { Ok(vec![Value::I32(0)]) }
);
crate::export_method!(
    "wasi:sockets/tcp@0.2.0", "[method]tcp-socket.set-keep-alive-enabled",
    [], vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
    pub fn tcp_set_keep_alive_enabled<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> { 
        let result_ptr = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
        let _ = write_bytes(store, result_ptr, &[0]); Ok(vec![]) 
    }
);
crate::export_method!(
    "wasi:sockets/tcp@0.2.0", "[method]tcp-socket.keep-alive-idle-time",
    [], vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I64)],
    pub fn tcp_get_keep_alive_idle_time<T: Config>(_: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> { Ok(vec![Value::I64(7200000000000)]) }
);
crate::export_method!(
    "wasi:sockets/tcp@0.2.0", "[method]tcp-socket.set-keep-alive-idle-time",
    [], vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![],
    pub fn tcp_set_keep_alive_idle_time<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> { 
        let result_ptr = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
        let _ = write_bytes(store, result_ptr, &[0]); Ok(vec![]) 
    }
);
crate::export_method!(
    "wasi:sockets/tcp@0.2.0", "[method]tcp-socket.keep-alive-interval",
    [], vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I64)],
    pub fn tcp_get_keep_alive_interval<T: Config>(_: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> { Ok(vec![Value::I64(75000000000)]) }
);
crate::export_method!(
    "wasi:sockets/tcp@0.2.0", "[method]tcp-socket.set-keep-alive-interval",
    [], vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![],
    pub fn tcp_set_keep_alive_interval<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> { 
        let result_ptr = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
        let _ = write_bytes(store, result_ptr, &[0]); Ok(vec![]) 
    }
);
crate::export_method!(
    "wasi:sockets/tcp@0.2.0", "[method]tcp-socket.keep-alive-count",
    [], vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn tcp_get_keep_alive_count<T: Config>(_: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> { Ok(vec![Value::I32(9)]) }
);
crate::export_method!(
    "wasi:sockets/tcp@0.2.0", "[method]tcp-socket.set-keep-alive-count",
    [], vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
    pub fn tcp_set_keep_alive_count<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> { 
        let result_ptr = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
        let _ = write_bytes(store, result_ptr, &[0]); Ok(vec![]) 
    }
);
crate::export_method!(
    "wasi:sockets/tcp@0.2.0", "[method]tcp-socket.hop-limit",
    [], vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn tcp_get_hop_limit<T: Config>(_: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> { Ok(vec![Value::I32(64)]) }
);
crate::export_method!(
    "wasi:sockets/tcp@0.2.0", "[method]tcp-socket.set-hop-limit",
    [], vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
    pub fn tcp_set_hop_limit<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> { 
        let result_ptr = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
        let _ = write_bytes(store, result_ptr, &[0]); Ok(vec![]) 
    }
);
crate::export_method!(
    "wasi:sockets/tcp@0.2.0", "[method]tcp-socket.receive-buffer-size",
    [], vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I64)],
    pub fn tcp_get_receive_buffer_size<T: Config>(_: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> { Ok(vec![Value::I64(65536)]) }
);
crate::export_method!(
    "wasi:sockets/tcp@0.2.0", "[method]tcp-socket.set-receive-buffer-size",
    [], vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![],
    pub fn tcp_set_receive_buffer_size<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> { 
        let result_ptr = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
        let _ = write_bytes(store, result_ptr, &[0]); Ok(vec![]) 
    }
);
crate::export_method!(
    "wasi:sockets/tcp@0.2.0", "[method]tcp-socket.send-buffer-size",
    [], vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I64)],
    pub fn tcp_get_send_buffer_size<T: Config>(_: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> { Ok(vec![Value::I64(65536)]) }
);
crate::export_method!(
    "wasi:sockets/tcp@0.2.0", "[method]tcp-socket.set-send-buffer-size",
    [], vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![],
    pub fn tcp_set_send_buffer_size<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> { 
        let result_ptr = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
        let _ = write_bytes(store, result_ptr, &[0]); Ok(vec![]) 
    }
);

crate::export_method!(
    "wasi:sockets/udp@0.2.0", "[method]udp-socket.create",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
    pub fn create_udp_socket<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let address_family = match args.get(0) { Some(Value::I32(v)) => *v, _ => return Err(HaltExecutionError(1)) };
        let result_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
        
        let res = host::socket_create(address_family as u64, 2); // 2 = SOCK_DGRAM
        
        let mut buf = [0u8; 8];
        if res <= i32::MAX as u64 {
            buf[0] = 0;
            buf[4..8].copy_from_slice(&(res as i32).to_le_bytes());
        } else { buf[0] = 1; }
        write_bytes(store, result_ptr, &buf).map_err(|_| HaltExecutionError(1))?;
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi:sockets/udp@0.2.0", "[method]udp-socket.start-bind",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
    pub fn udp_start_bind<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let socket = match args.get(0) { Some(Value::I32(v)) => *v, _ => return Err(HaltExecutionError(1)) };
        let ip_addr_ptr = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
        let result_ptr = match args.get(3) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
        let mut ip_addr = vec![0u8; 16];
        if read_mem(store, ip_addr_ptr, &mut ip_addr).is_err() { return Err(HaltExecutionError(1)); }
        
        let res = host::socket_bind(socket as u64, ip_addr.as_ptr(), 16);
        
        let mut buf = [0u8; 4];
        buf[0] = if res == 0 { 0 } else { 1 };
        write_bytes(store, result_ptr, &buf).map_err(|_| HaltExecutionError(1))?;
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi:sockets/udp@0.2.0", "[method]outgoing-datagram-stream.send",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
    pub fn udp_send<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let stream = match args.get(0) { Some(Value::I32(v)) => *v, _ => return Err(HaltExecutionError(1)) };
        let buf_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
        let buf_len = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
        let dest_addr_ptr = match args.get(3) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
        let result_ptr = match args.get(4) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
        let mut payload = vec![0u8; buf_len as usize];
        if read_mem(store, buf_ptr, &mut payload).is_err() { return Err(HaltExecutionError(1)); }
        let mut dest_addr = [0u8; 16];
        if read_mem(store, dest_addr_ptr, &mut dest_addr).is_err() { return Err(HaltExecutionError(1)); }
        
        let res = host::socket_udp_send(stream as u64, payload.as_ptr(), buf_len as u64, dest_addr.as_ptr(), 16);
        
        let mut result_buf = [0u8; 16];
        if res <= buf_len as u64 {
            result_buf[0] = 0;
            result_buf[8..16].copy_from_slice(&res.to_le_bytes());
        } else { result_buf[0] = 1; }
        write_bytes(store, result_ptr, &result_buf).map_err(|_| HaltExecutionError(1))?;
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi:sockets/udp@0.2.0", "[method]incoming-datagram-stream.receive",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![],
    pub fn udp_receive<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let stream = match args.get(0) { Some(Value::I32(v)) => *v, _ => return Err(HaltExecutionError(1)) };
        let max_results = match args.get(1) { Some(Value::I64(v)) => *v as u64, _ => return Err(HaltExecutionError(1)) };
        let result_ptr = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
        
        let mut payload = vec![0u8; max_results as usize];
        let mut src_addr = [0u8; 16];
        let mut addr_len: u32 = 16;

        let res = host::socket_udp_recv(stream as u64, payload.as_mut_ptr(), max_results, src_addr.as_mut_ptr(), &mut addr_len);

        if res <= max_results {
            let res_usize = res as usize;
            let mut header = vec![0u8; 32];
            header[0] = 0;
            header[8..16].copy_from_slice(&res.to_le_bytes());
            header[16..32].copy_from_slice(&src_addr);
            if res_usize > 0 {
                header.extend_from_slice(&payload[..res_usize]);
            }
            write_bytes(store, result_ptr, &header).map_err(|_| HaltExecutionError(1))?;
        } else {
            let mut header = [0u8; 32];
            header[0] = 1;
            write_bytes(store, result_ptr, &header).map_err(|_| HaltExecutionError(1))?;
        }
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi:sockets/udp@0.2.0", "[method]udp-socket.finish-bind",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
    pub fn udp_finish_bind<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let result_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
        let mut buf = [0u8; 4];
        buf[0] = 0; // Ok
        write_bytes(store, result_ptr, &buf).map_err(|_| HaltExecutionError(1))?;
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi:sockets/udp@0.2.0", "[method]udp-socket.stream",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
    pub fn udp_stream<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let socket = match args.get(0) { Some(Value::I32(v)) => *v, _ => return Err(HaltExecutionError(1)) };
        let remote_addr_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
        let result_ptr = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
        
        let mut remote_addr = [0u8; 16];
        read_mem(store, remote_addr_ptr, &mut remote_addr).map_err(|_| HaltExecutionError(1))?;
        
        let res = host::socket_connect(socket as u64, remote_addr.as_ptr(), 16);
        
        let mut buf = [0u8; 12];
        if res == 0 {
            buf[0] = 0; // Ok
            let wasi = store.wasi_ctx.as_mut().ok_or(HaltExecutionError(1))?;
            let in_id = wasi.next_resource_id; wasi.next_resource_id += 1;
            wasi.resource_table.insert(in_id, crate::wasm::wasi::ctx::WasiResource::InputStream(crate::wasm::wasi::ctx::InputStreamSource::GuestFd(socket as i32)));
            let out_id = wasi.next_resource_id; wasi.next_resource_id += 1;
            wasi.resource_table.insert(out_id, crate::wasm::wasi::ctx::WasiResource::OutputStream(crate::wasm::wasi::ctx::OutputStreamSource::GuestFd(socket as i32)));
            
            buf[4..8].copy_from_slice(&(in_id as i32).to_le_bytes());
            buf[8..12].copy_from_slice(&(out_id as i32).to_le_bytes());
        } else {
            buf[0] = 1; // Err
        }
        write_bytes(store, result_ptr, &buf).map_err(|_| HaltExecutionError(1))?;
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi:sockets/udp@0.2.0", "[method]udp-socket.local-address",
    [], vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
    pub fn udp_local_address<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let socket = match args.get(0) { Some(Value::I32(v)) => *v, _ => return Err(HaltExecutionError(1)) };
        let result_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
        let mut addr = [0u8; 16];
        let res = host::socket_get_local_addr(socket as u64, addr.as_mut_ptr());
        let mut buf = [0u8; 32];
        if res == 0 { buf[0] = 0; buf[8..24].copy_from_slice(&addr); } else { buf[0] = 1; }
        write_bytes(store, result_ptr, &buf).map_err(|_| HaltExecutionError(1))?;
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi:sockets/udp@0.2.0", "[method]udp-socket.remote-address",
    [], vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
    pub fn udp_remote_address<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let socket = match args.get(0) { Some(Value::I32(v)) => *v, _ => return Err(HaltExecutionError(1)) };
        let result_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
        let mut addr = [0u8; 16];
        let res = host::socket_get_remote_addr(socket as u64, addr.as_mut_ptr());
        let mut buf = [0u8; 32];
        if res == 0 { buf[0] = 0; buf[8..24].copy_from_slice(&addr); } else { buf[0] = 1; }
        write_bytes(store, result_ptr, &buf).map_err(|_| HaltExecutionError(1))?;
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi:sockets/udp@0.2.0", "[method]udp-socket.address-family",
    [], vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn udp_address_family<T: Config>(_: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> { Ok(vec![Value::I32(0)]) }
);

crate::export_method!(
    "wasi:sockets/udp@0.2.0", "[method]udp-socket.subscribe",
    [], vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn udp_subscribe<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let handle = match args.get(0) { Some(Value::I32(v)) => *v as i32, _ => return Err(HaltExecutionError(1)) };
        let wasi = store.wasi_ctx.as_mut().ok_or(HaltExecutionError(1))?;
        let id = wasi.next_resource_id; wasi.next_resource_id += 1;
        wasi.resource_table.insert(id, crate::wasm::wasi::ctx::WasiResource::Pollable(crate::wasm::wasi::ctx::PollableTarget::Read(handle)));
        Ok(vec![Value::I32(id as u32)])
    }
);

crate::export_method!(
    "wasi:sockets/udp@0.2.0", "[method]incoming-datagram-stream.subscribe",
    [], vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn udp_incoming_subscribe<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        udp_subscribe(store, args)
    }
);

crate::export_method!(
    "wasi:sockets/udp@0.2.0", "[method]outgoing-datagram-stream.subscribe",
    [], vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn udp_outgoing_subscribe<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let handle = match args.get(0) { Some(Value::I32(v)) => *v as i32, _ => return Err(HaltExecutionError(1)) };
        let wasi = store.wasi_ctx.as_mut().ok_or(HaltExecutionError(1))?;
        let id = wasi.next_resource_id; wasi.next_resource_id += 1;
        wasi.resource_table.insert(id, crate::wasm::wasi::ctx::WasiResource::Pollable(crate::wasm::wasi::ctx::PollableTarget::Write(handle)));
        Ok(vec![Value::I32(id as u32)])
    }
);

crate::export_method!(
    "wasi:sockets/udp@0.2.0", "[method]outgoing-datagram-stream.check-send",
    [], vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
    pub fn udp_check_send<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let result_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
        let mut buf = [0u8; 16];
        buf[0] = 0; // Ok
        let _ = write_u64(store, result_ptr + 8, 1); // Can send at least 1 datagram
        write_bytes(store, result_ptr, &buf).map_err(|_| HaltExecutionError(1))?;
        Ok(vec![])
    }
);

// UDP Options Stubs
crate::export_method!("wasi:sockets/udp@0.2.0", "[method]udp-socket.unicast-hop-limit", [], vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)], pub fn udp_get_unicast_hop_limit<T: Config>(_: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> { Ok(vec![Value::I32(64)]) });
crate::export_method!("wasi:sockets/udp@0.2.0", "[method]udp-socket.set-unicast-hop-limit", [], vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], pub fn udp_set_unicast_hop_limit<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> { let result_ptr = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) }; let _ = write_bytes(store, result_ptr, &[0]); Ok(vec![]) });
crate::export_method!("wasi:sockets/udp@0.2.0", "[method]udp-socket.receive-buffer-size", [], vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I64)], pub fn udp_get_receive_buffer_size<T: Config>(_: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> { Ok(vec![Value::I64(65536)]) });
crate::export_method!("wasi:sockets/udp@0.2.0", "[method]udp-socket.set-receive-buffer-size", [], vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![], pub fn udp_set_receive_buffer_size<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> { let result_ptr = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) }; let _ = write_bytes(store, result_ptr, &[0]); Ok(vec![]) });
crate::export_method!("wasi:sockets/udp@0.2.0", "[method]udp-socket.send-buffer-size", [], vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I64)], pub fn udp_get_send_buffer_size<T: Config>(_: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> { Ok(vec![Value::I64(65536)]) });
crate::export_method!("wasi:sockets/udp@0.2.0", "[method]udp-socket.set-send-buffer-size", [], vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![], pub fn udp_set_send_buffer_size<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> { let result_ptr = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) }; let _ = write_bytes(store, result_ptr, &[0]); Ok(vec![]) });

crate::export_method!(
    "wasi:sockets/instance-network@0.2.0", "instance-network",
    [],
    vec![], vec![ValType::NumType(NumType::I32)],
    pub fn instance_network<T: Config>(_: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        Ok(vec![Value::I32(0)])
    }
);

crate::export_method!(
    "wasi:sockets/ip-name-lookup@0.2.0", "resolve-addresses",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
    pub fn resolve_addresses<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let name_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
        let name_len = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
        let result_ptr = match args.get(3) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
        
        let mut name_buf = vec![0u8; name_len as usize];
        read_mem(store, name_ptr, &mut name_buf).map_err(|_| HaltExecutionError(1))?;
        let name = crate::alloc::string::String::from_utf8_lossy(&name_buf);

        let mut buf = [0u8; 8];
        if name == "localhost" {
            buf[0] = 0; // Ok
            let wasi = store.wasi_ctx.as_mut().ok_or(HaltExecutionError(1))?;
            let id = wasi.next_resource_id; wasi.next_resource_id += 1;
            wasi.resource_table.insert(id, crate::wasm::wasi::ctx::WasiResource::ResolveAddressStream(vec![[127, 0, 0, 1]]));
            buf[4..8].copy_from_slice(&(id as i32).to_le_bytes());
        } else {
            buf[0] = 1; // Err
        }
        write_bytes(store, result_ptr, &buf).map_err(|_| HaltExecutionError(1))?;
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi:sockets/ip-name-lookup@0.2.0", "[method]resolve-address-stream.resolve-next-address",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![],
    pub fn resolve_next_address<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let handle = match args.get(0) { Some(Value::I32(v)) => *v as i32, _ => return Err(HaltExecutionError(1)) };
        let result_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
        
        let mut addr = None;
        {
            let wasi = store.wasi_ctx.as_mut().ok_or(HaltExecutionError(1))?;
            if let Some(crate::wasm::wasi::ctx::WasiResource::ResolveAddressStream(list)) = wasi.resource_table.get_mut(&handle) {
                if !list.is_empty() {
                    addr = Some(list.remove(0));
                }
            }
        }

        let mut buf = [0u8; 32];
        buf[0] = 0; // Ok tag
        if let Some(a) = addr {
            buf[4] = 1; // Some
            buf[8] = 0; // ipv4 tag
            buf[12..16].copy_from_slice(&a);
        } else {
            buf[4] = 0; // None
        }
        write_bytes(store, result_ptr, &buf).map_err(|_| HaltExecutionError(1))?;
        Ok(vec![])
    }
);

crate::export_method!(
    "wasi:sockets/ip-name-lookup@0.2.0", "[method]resolve-address-stream.subscribe",
    [], vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn resolve_subscribe<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let wasi = store.wasi_ctx.as_mut().ok_or(HaltExecutionError(1))?;
        let id = wasi.next_resource_id; wasi.next_resource_id += 1;
        wasi.resource_table.insert(id, crate::wasm::wasi::ctx::WasiResource::Pollable(crate::wasm::wasi::ctx::PollableTarget::Timer(0)));
        Ok(vec![Value::I32(id as u32)])
    }
);

crate::export_method!(
    "wasi:sockets/ip-name-lookup@0.2.0", "[resource-drop]resolve-address-stream",
    [],
    vec![ValType::NumType(NumType::I32)], vec![],
    pub fn resolve_address_stream_drop<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        crate::wasm::wasi::preview2::resource_drop(store, args)
    }
);

crate::export_method!(
    "wasi:sockets/udp@0.2.0", "[resource-drop]udp-socket",
    [],
    vec![ValType::NumType(NumType::I32)], vec![],
    pub fn udp_socket_drop<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        crate::wasm::wasi::preview2::resource_drop(store, args)
    }
);

crate::export_method!(
    "wasi:sockets/tcp@0.2.0", "[resource-drop]tcp-socket",
    [],
    vec![ValType::NumType(NumType::I32)], vec![],
    pub fn tcp_socket_drop<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        crate::wasm::wasi::preview2::resource_drop(store, args)
    }
);

crate::export_method!(
    "wasi:sockets/network@0.2.0", "[resource-drop]network",
    [],
    vec![ValType::NumType(NumType::I32)], vec![],
    pub fn network_drop<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        crate::wasm::wasi::preview2::resource_drop(store, args)
    }
);

pub fn register_wasi<T: Config + Clone>(linker: &mut crate::wasm::Linker, store: &mut crate::wasm::Store<'_, T>) {
    tcp_create_socket::register(linker, store);
    tcp_start_bind::register(linker, store);
    tcp_finish_bind::register(linker, store);
    tcp_start_connect::register(linker, store);
    tcp_finish_connect::register(linker, store);
    tcp_start_listen::register(linker, store);
    tcp_finish_listen::register(linker, store);
    tcp_accept::register(linker, store);
    tcp_send::register(linker, store);
    tcp_recv::register(linker, store);
    tcp_local_address::register(linker, store);
    tcp_remote_address::register(linker, store);
    tcp_is_listening::register(linker, store);
    tcp_address_family::register(linker, store);
    tcp_shutdown::register(linker, store);
    tcp_set_listen_backlog_size::register(linker, store);
    tcp_subscribe::register(linker, store);
    tcp_get_keep_alive_enabled::register(linker, store);
    tcp_set_keep_alive_enabled::register(linker, store);
    tcp_get_keep_alive_idle_time::register(linker, store);
    tcp_set_keep_alive_idle_time::register(linker, store);
    tcp_get_keep_alive_interval::register(linker, store);
    tcp_set_keep_alive_interval::register(linker, store);
    tcp_get_keep_alive_count::register(linker, store);
    tcp_set_keep_alive_count::register(linker, store);
    tcp_get_hop_limit::register(linker, store);
    tcp_set_hop_limit::register(linker, store);
    tcp_get_receive_buffer_size::register(linker, store);
    tcp_set_receive_buffer_size::register(linker, store);
    tcp_get_send_buffer_size::register(linker, store);
    tcp_set_send_buffer_size::register(linker, store);
    
    create_udp_socket::register(linker, store);
    udp_start_bind::register(linker, store);
    udp_finish_bind::register(linker, store);
    udp_stream::register(linker, store);
    udp_local_address::register(linker, store);
    udp_remote_address::register(linker, store);
    udp_address_family::register(linker, store);
    udp_subscribe::register(linker, store);
    udp_incoming_subscribe::register(linker, store);
    udp_outgoing_subscribe::register(linker, store);
    udp_check_send::register(linker, store);
    udp_send::register(linker, store);
    udp_receive::register(linker, store);
    udp_get_unicast_hop_limit::register(linker, store);
    udp_set_unicast_hop_limit::register(linker, store);
    udp_get_receive_buffer_size::register(linker, store);
    udp_set_receive_buffer_size::register(linker, store);
    udp_get_send_buffer_size::register(linker, store);
    udp_set_send_buffer_size::register(linker, store);
    
    instance_network::register(linker, store);
    resolve_addresses::register(linker, store);
    resolve_next_address::register(linker, store);
    resolve_subscribe::register(linker, store);
    resolve_address_stream_drop::register(linker, store);
    udp_socket_drop::register(linker, store);
    tcp_socket_drop::register(linker, store);
    network_drop::register(linker, store);
    krakeos_socket_create_host::register(linker, store);
    krakeos_socket_connect_host::register(linker, store);
    krakeos_socket_finish_connect_host::register(linker, store);
    krakeos_socket_bind_host::register(linker, store);
    krakeos_socket_listen_host::register(linker, store);
    krakeos_socket_accept_host::register(linker, store);
    krakeos_socket_send_host::register(linker, store);
    krakeos_socket_recv_host::register(linker, store);
    krakeos_socket_udp_send_host::register(linker, store);
    krakeos_socket_udp_recv_host::register(linker, store);
    krakeos_socket_get_local_addr_host::register(linker, store);
    krakeos_socket_get_remote_addr_host::register(linker, store);
    krakeos_socket_shutdown_host::register(linker, store);
    sock_recv_p1::register(linker, store);
    sock_send_p1::register(linker, store);
    sock_shutdown_p1::register(linker, store);
    }

    crate::export_method!(
    "wasi_snapshot_preview1", "sock_recv",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn sock_recv_p1<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let fd = match args.get(0) { Some(Value::I32(x)) => *x as i32, _ => -1 };
        let ri_data_ptr = match args.get(1) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let ri_data_len = match args.get(2) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let ri_flags = match args.get(3) { Some(Value::I32(x)) => *x as u16, _ => 0 };
        let ro_datalen_ptr = match args.get(4) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let ro_flags_ptr = match args.get(5) { Some(Value::I32(x)) => *x as u32, _ => 0 };

        let mut iovs = Vec::new();
        for i in 0..ri_data_len {
            let mut iov = [0u8; 8];
            if read_mem(store, ri_data_ptr + i * 8, &mut iov).is_err() { return Ok(vec![Value::I32(21)]); }
            let b_ptr = u32::from_le_bytes(iov[0..4].try_into().unwrap());
            let b_len = u32::from_le_bytes(iov[4..8].try_into().unwrap());
            iovs.push((b_ptr, b_len));
        }

        let mut buffers = Vec::new();
        for (_, len) in &iovs { buffers.push(vec![0u8; *len as usize]); }
        let mut slices: Vec<&mut [u8]> = buffers.iter_mut().map(|v| v.as_mut_slice()).collect();

        match wasi_ctx(store).env.sock_recv(fd, &mut slices, ri_flags) {
            Ok((n, flags)) => {
                let mut remaining = n;
                for ((ptr, _), buf) in iovs.iter().zip(buffers.iter()) {
                    let to_write = core::cmp::min(remaining, buf.len());
                    if to_write > 0 {
                        let _ = write_bytes(store, *ptr, &buf[..to_write]);
                        remaining -= to_write;
                    }
                }
                let _ = write_u32(store, ro_datalen_ptr, n as u32);
                let _ = write_u32(store, ro_flags_ptr, flags as u32);
                Ok(vec![Value::I32(0)])
            }
            Err(e) => Ok(vec![Value::I32(e as u32)]),
        }
    }
    );

    crate::export_method!(
    "wasi_snapshot_preview1", "sock_send",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn sock_send_p1<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let fd = match args.get(0) { Some(Value::I32(x)) => *x as i32, _ => -1 };
        let si_data_ptr = match args.get(1) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let si_data_len = match args.get(2) { Some(Value::I32(x)) => *x as u32, _ => 0 };
        let si_flags = match args.get(3) { Some(Value::I32(x)) => *x as u16, _ => 0 };
        let so_datalen_ptr = match args.get(4) { Some(Value::I32(x)) => *x as u32, _ => 0 };

        let mut buffers = Vec::new();
        for i in 0..si_data_len {
            let mut iov = [0u8; 8];
            if read_mem(store, si_data_ptr + i * 8, &mut iov).is_err() { return Ok(vec![Value::I32(21)]); }
            let b_ptr = u32::from_le_bytes(iov[0..4].try_into().unwrap());
            let b_len = u32::from_le_bytes(iov[4..8].try_into().unwrap());
            let mut b = vec![0u8; b_len as usize];
            if read_mem(store, b_ptr, &mut b).is_err() { return Ok(vec![Value::I32(21)]); }
            buffers.push(b);
        }

        let slices: Vec<&[u8]> = buffers.iter().map(|v| v.as_slice()).collect();
        match wasi_ctx(store).env.sock_send(fd, &slices, si_flags) {
            Ok(n) => {
                let _ = write_u32(store, so_datalen_ptr, n as u32);
                Ok(vec![Value::I32(0)])
            }
            Err(e) => Ok(vec![Value::I32(e as u32)]),
        }
    }
    );

    crate::export_method!(
    "wasi_snapshot_preview1", "sock_shutdown",
    [],
    vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn sock_shutdown_p1<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let fd = match args.get(0) { Some(Value::I32(x)) => *x as i32, _ => -1 };
        let how = match args.get(1) { Some(Value::I32(x)) => *x as u8, _ => 0 };
        match wasi_ctx(store).env.sock_shutdown(fd, how) {
            Ok(_) => Ok(vec![Value::I32(0)]),
            Err(e) => Ok(vec![Value::I32(e as u32)]),
        }
    }
    );

    fn wasi_ctx<'a, T: Config>(store: &'a mut Store<'_, T>) -> &'a mut crate::wasm::wasi::ctx::WasiCtx {
    if store.wasi_ctx.is_none() {
        store.wasi_ctx = Some(crate::wasm::wasi::ctx::WasiCtx::default());
    }
    store.wasi_ctx.as_mut().unwrap()
    }

    crate::export_method!(
    "krakeos:system/network@0.2.0", "socket-create",
    [],
    vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I64)], vec![ValType::NumType(NumType::I64)],
    pub fn krakeos_socket_create_host<T: Config>(_: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let family = match args.get(0) { Some(Value::I64(v)) => *v, _ => 0 };
        let ty = match args.get(1) { Some(Value::I64(v)) => *v, _ => 0 };
        Ok(vec![Value::I64(host::socket_create(family, ty))])
    }
);

crate::export_method!(
    "krakeos:system/network@0.2.0", "socket-connect",
    [],
    vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I32), ValType::NumType(NumType::I64)], vec![ValType::NumType(NumType::I64)],
    pub fn krakeos_socket_connect_host<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let fd = match args.get(0) { Some(Value::I64(v)) => *v, _ => 0 };
        let addr_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => 0 };
        let addr_len = match args.get(2) { Some(Value::I64(v)) => *v, _ => 0 };
        let mut addr = vec![0u8; addr_len as usize];
        if read_mem(store, addr_ptr, &mut addr).is_err() { return Err(HaltExecutionError(1)); }
        Ok(vec![Value::I64(host::socket_connect(fd, addr.as_ptr(), addr_len))])
    }
);

crate::export_method!(
    "krakeos:system/network@0.2.0", "socket-finish-connect",
    [],
    vec![ValType::NumType(NumType::I64)], vec![ValType::NumType(NumType::I64)],
    pub fn krakeos_socket_finish_connect_host<T: Config>(_: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let fd = match args.get(0) { Some(Value::I64(v)) => *v, _ => 0 };
        Ok(vec![Value::I64(host::socket_finish_connect(fd))])
    }
);

crate::export_method!(
    "krakeos:system/network@0.2.0", "socket-bind",
    [],
    vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I32), ValType::NumType(NumType::I64)], vec![ValType::NumType(NumType::I64)],
    pub fn krakeos_socket_bind_host<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let fd = match args.get(0) { Some(Value::I64(v)) => *v, _ => 0 };
        let addr_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => 0 };
        let addr_len = match args.get(2) { Some(Value::I64(v)) => *v, _ => 0 };
        let mut addr = vec![0u8; addr_len as usize];
        if read_mem(store, addr_ptr, &mut addr).is_err() { return Err(HaltExecutionError(1)); }
        Ok(vec![Value::I64(host::socket_bind(fd, addr.as_ptr(), addr_len))])
    }
);

crate::export_method!(
    "krakeos:system/network@0.2.0", "socket-listen",
    [],
    vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I64)], vec![ValType::NumType(NumType::I64)],
    pub fn krakeos_socket_listen_host<T: Config>(_: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let fd = match args.get(0) { Some(Value::I64(v)) => *v, _ => 0 };
        let backlog = match args.get(1) { Some(Value::I64(v)) => *v, _ => 0 };
        Ok(vec![Value::I64(host::socket_listen(fd, backlog))])
    }
);

crate::export_method!(
    "krakeos:system/network@0.2.0", "socket-accept",
    [],
    vec![ValType::NumType(NumType::I64)], vec![ValType::NumType(NumType::I64)],
    pub fn krakeos_socket_accept_host<T: Config>(_: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let fd = match args.get(0) { Some(Value::I64(v)) => *v, _ => 0 };
        Ok(vec![Value::I64(host::socket_accept(fd))])
    }
);

crate::export_method!(
    "krakeos:system/network@0.2.0", "socket-send",
    [],
    vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I32), ValType::NumType(NumType::I64)], vec![ValType::NumType(NumType::I64)],
    pub fn krakeos_socket_send_host<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let fd = match args.get(0) { Some(Value::I64(v)) => *v, _ => 0 };
        let buf_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => 0 };
        let buf_len = match args.get(2) { Some(Value::I64(v)) => *v, _ => 0 };
        let mut buf = vec![0u8; buf_len as usize];
        if read_mem(store, buf_ptr, &mut buf).is_err() { return Err(HaltExecutionError(1)); }
        Ok(vec![Value::I64(host::socket_send(fd, buf.as_ptr(), buf_len))])
    }
);

crate::export_method!(
    "krakeos:system/network@0.2.0", "socket-recv",
    [],
    vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I32), ValType::NumType(NumType::I64)], vec![ValType::NumType(NumType::I64)],
    pub fn krakeos_socket_recv_host<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let fd = match args.get(0) { Some(Value::I64(v)) => *v, _ => 0 };
        let buf_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => 0 };
        let buf_len = match args.get(2) { Some(Value::I64(v)) => *v, _ => 0 };
        let mut buf = vec![0u8; buf_len as usize];
        let res = host::socket_recv(fd, buf.as_mut_ptr(), buf_len);
        if res <= buf_len {
            let _ = write_bytes(store, buf_ptr, &buf[..res as usize]);
        }
        Ok(vec![Value::I64(res)])
    }
);

crate::export_method!(
    "krakeos:system/network@0.2.0", "socket-udp-send",
    [],
    vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32), ValType::NumType(NumType::I64)], vec![ValType::NumType(NumType::I64)],
    pub fn krakeos_socket_udp_send_host<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let fd = match args.get(0) { Some(Value::I64(v)) => *v, _ => 0 };
        let buf_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => 0 };
        let buf_len = match args.get(2) { Some(Value::I64(v)) => *v, _ => 0 };
        let addr_ptr = match args.get(3) { Some(Value::I32(v)) => *v as u32, _ => 0 };
        let addr_len = match args.get(4) { Some(Value::I64(v)) => *v, _ => 0 };
        let mut buf = vec![0u8; buf_len as usize];
        if read_mem(store, buf_ptr, &mut buf).is_err() { return Err(HaltExecutionError(1)); }
        let mut addr = vec![0u8; addr_len as usize];
        if read_mem(store, addr_ptr, &mut addr).is_err() { return Err(HaltExecutionError(1)); }
        Ok(vec![Value::I64(host::socket_udp_send(fd, buf.as_ptr(), buf_len, addr.as_ptr(), addr_len))])
    }
);

crate::export_method!(
    "krakeos:system/network@0.2.0", "socket-udp-recv",
    [],
    vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I64)],
    pub fn krakeos_socket_udp_recv_host<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let fd = match args.get(0) { Some(Value::I64(v)) => *v, _ => 0 };
        let buf_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => 0 };
        let buf_len = match args.get(2) { Some(Value::I64(v)) => *v, _ => 0 };
        let addr_ptr = match args.get(3) { Some(Value::I32(v)) => *v as u32, _ => 0 };
        let addr_len_ptr = match args.get(4) { Some(Value::I32(v)) => *v as u32, _ => 0 };
        let mut buf = vec![0u8; buf_len as usize];
        let mut addr = [0u8; 16];
        let mut addr_len = 16u32;
        let res = host::socket_udp_recv(fd, buf.as_mut_ptr(), buf_len, addr.as_mut_ptr(), &mut addr_len);
        if res <= buf_len {
            let _ = write_bytes(store, buf_ptr, &buf[..res as usize]);
            let _ = write_bytes(store, addr_ptr, &addr[..addr_len as usize]);
            let _ = write_u32(store, addr_len_ptr, addr_len);
        }
        Ok(vec![Value::I64(res)])
    }
);

crate::export_method!(
    "krakeos:system/network@0.2.0", "socket-get-local-addr",
    [],
    vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn krakeos_socket_get_local_addr_host<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let fd = match args.get(0) { Some(Value::I64(v)) => *v, _ => 0 };
        let addr_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => 0 };
        let mut addr = [0u8; 16];
        let res = host::socket_get_local_addr(fd, addr.as_mut_ptr());
        if res == 0 {
            let _ = write_bytes(store, addr_ptr, &addr);
        }
        Ok(vec![Value::I32(res as u32)])
    }
);

crate::export_method!(
    "krakeos:system/network@0.2.0", "socket-get-remote-addr",
    [],
    vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)],
    pub fn krakeos_socket_get_remote_addr_host<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let fd = match args.get(0) { Some(Value::I64(v)) => *v, _ => 0 };
        let addr_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => 0 };
        let mut addr = [0u8; 16];
        let res = host::socket_get_remote_addr(fd, addr.as_mut_ptr());
        if res == 0 {
            let _ = write_bytes(store, addr_ptr, &addr);
        }
        Ok(vec![Value::I32(res as u32)])
    }
);

crate::export_method!(
    "krakeos:system/network@0.2.0", "socket-shutdown",
    [],
    vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I64)], vec![ValType::NumType(NumType::I32)],
    pub fn krakeos_socket_shutdown_host<T: Config>(_: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
        let fd = match args.get(0) { Some(Value::I64(v)) => *v, _ => 0 };
        let how = match args.get(1) { Some(Value::I64(v)) => *v, _ => 0 };
        Ok(vec![Value::I32(host::socket_shutdown(fd, how) as u32)])
    }
);
