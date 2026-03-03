use crate::alloc::{vec, vec::Vec};
use crate::wasm::{
    common::{config::Config, value::Value, reader::types::{ValType, NumType}},
    interpreter::store::{HaltExecutionError, Store},
};
use crate::wasm::wasi::preview2::{read_mem, write_bytes};
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
        buf[0] = 0;
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
        let result_ptr = match args.get(3) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
        let mut buf = [0u8; 4];
        buf[0] = 1; // Err
        write_bytes(store, result_ptr, &buf).map_err(|_| HaltExecutionError(1))?;
        Ok(vec![])
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
    create_udp_socket::register(linker, store);
    udp_start_bind::register(linker, store);
    udp_send::register(linker, store);
    udp_receive::register(linker, store);
    instance_network::register(linker, store);
    resolve_addresses::register(linker, store);
    udp_socket_drop::register(linker, store);
    tcp_socket_drop::register(linker, store);
    network_drop::register(linker, store);
}