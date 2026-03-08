with open("std/src/net/wasi.rs", "a") as f:
    f.write("""

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
""")
