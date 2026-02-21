use crate::rust_alloc::vec;
use crate::rust_alloc::vec::Vec;
use crate::wasm::{
    common::{config::Config, value::Value},
    interpreter::store::{HaltExecutionError, Store},
};
use super::{read_mem, write_bytes, write_u32, write_u64, call_cabi_realloc};

pub fn adapter_close_badfd<T: Config>(_: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    Ok(vec![Value::I32(76)])
}

pub fn tcp_create_socket<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let result_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
    let mut buf = [0u8; 4];
    buf[0] = 1; 
    write_bytes(store, result_ptr, &buf).map_err(|_| HaltExecutionError(1))?;
    Ok(vec![])
}

pub fn tcp_start_bind<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let result_ptr = match args.get(3) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
    let mut buf = [0u8; 4];
    buf[0] = 1;
    write_bytes(store, result_ptr, &buf).map_err(|_| HaltExecutionError(1))?;
    Ok(vec![])
}

pub fn tcp_finish_bind<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let result_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
    let mut buf = [0u8; 4];
    buf[0] = 1;
    write_bytes(store, result_ptr, &buf).map_err(|_| HaltExecutionError(1))?;
    Ok(vec![])
}

pub fn tcp_start_connect<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let result_ptr = match args.get(3) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
    let mut buf = [0u8; 4];
    buf[0] = 1;
    write_bytes(store, result_ptr, &buf).map_err(|_| HaltExecutionError(1))?;
    Ok(vec![])
}

pub fn tcp_finish_connect<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let result_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
    let mut buf = [0u8; 4];
    buf[0] = 1;
    write_bytes(store, result_ptr, &buf).map_err(|_| HaltExecutionError(1))?;
    Ok(vec![])
}

pub fn tcp_start_listen<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let result_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
    let mut buf = [0u8; 4];
    buf[0] = 1;
    write_bytes(store, result_ptr, &buf).map_err(|_| HaltExecutionError(1))?;
    Ok(vec![])
}

pub fn tcp_finish_listen<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let result_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
    let mut buf = [0u8; 4];
    buf[0] = 1;
    write_bytes(store, result_ptr, &buf).map_err(|_| HaltExecutionError(1))?;
    Ok(vec![])
}

pub fn tcp_accept<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let result_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
    let mut buf = [0u8; 4];
    buf[0] = 1;
    write_bytes(store, result_ptr, &buf).map_err(|_| HaltExecutionError(1))?;
    Ok(vec![])
}

pub fn instance_network<T: Config>(_: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    Ok(vec![Value::I32(0)])
}

pub fn resolve_addresses<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let result_ptr = match args.get(3) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
    let mut buf = [0u8; 4];
    buf[0] = 1;
    write_bytes(store, result_ptr, &buf).map_err(|_| HaltExecutionError(1))?;
    Ok(vec![])
}

pub fn create_udp_socket<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let address_family = match args.get(0) { Some(Value::I32(v)) => *v, _ => return Err(HaltExecutionError(1)) };
    let result_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };

    #[cfg(not(target_arch = "wasm32"))]
    let res = unsafe { crate::sys::syscall6(41, address_family as u64, 2, 0, 0, 0, 0) };
    #[cfg(target_arch = "wasm32")]
    let res = u64::MAX;

    let mut buf = [0u8; 8];
    if res == u64::MAX {
        buf[0] = 1;
    } else {
        buf[0] = 0;
        buf[4..8].copy_from_slice(&(res as i32).to_le_bytes());
    }
    write_bytes(store, result_ptr, &buf).map_err(|_| HaltExecutionError(1))?;
    Ok(vec![])
}

pub fn start_bind<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let socket = match args.get(0) { Some(Value::I32(v)) => *v, _ => return Err(HaltExecutionError(1)) };
    let _network = match args.get(1) { Some(Value::I32(v)) => *v, _ => return Err(HaltExecutionError(1)) };
    let ip_addr_ptr = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
    let result_ptr = match args.get(3) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };

    let mut ip_addr = vec![0u8; 16];
    read_mem(store, ip_addr_ptr, &mut ip_addr).map_err(|_| HaltExecutionError(1))?;

    #[cfg(not(target_arch = "wasm32"))]
    let res = unsafe { crate::sys::syscall6(49, socket as u64, ip_addr.as_ptr() as u64, 16, 0, 0, 0) };
    #[cfg(target_arch = "wasm32")]
    let res = u64::MAX;

    let mut buf = [0u8; 4];
    buf[0] = if res == 0 { 0 } else { 1 };
    write_bytes(store, result_ptr, &buf).map_err(|_| HaltExecutionError(1))?;
    Ok(vec![])
}

pub fn send<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let stream = match args.get(0) { Some(Value::I32(v)) => *v, _ => return Err(HaltExecutionError(1)) };
    let buf_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
    let buf_len = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
    let dest_addr_ptr = match args.get(3) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
    let result_ptr = match args.get(4) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };

    let mut payload = vec![0u8; buf_len as usize];
    read_mem(store, buf_ptr, &mut payload).map_err(|_| HaltExecutionError(1))?;

    let mut dest_addr = vec![0u8; 16];
    read_mem(store, dest_addr_ptr, &mut dest_addr).map_err(|_| HaltExecutionError(1))?;

    #[cfg(not(target_arch = "wasm32"))]
    let res = unsafe { crate::sys::syscall6(44, stream as u64, payload.as_ptr() as u64, buf_len as u64, 0, dest_addr.as_ptr() as u64, 16) };
    #[cfg(target_arch = "wasm32")]
    let res = u64::MAX;

    let mut result_buf = [0u8; 16];
    if res != u64::MAX {
        result_buf[0] = 0;
        result_buf[8..16].copy_from_slice(&res.to_le_bytes());
    } else {
        result_buf[0] = 1;
    }
    write_bytes(store, result_ptr, &result_buf).map_err(|_| HaltExecutionError(1))?;
    Ok(vec![])
}

pub fn receive<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let stream = match args.get(0) { Some(Value::I32(v)) => *v, _ => return Err(HaltExecutionError(1)) };
    let max_results = match args.get(1) { Some(Value::I64(v)) => *v, _ => return Err(HaltExecutionError(1)) };
    let result_ptr = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };

    let mut payload = vec![0u8; max_results as usize];
    let mut src_addr = vec![0u8; 16];
    let mut addr_len: u32 = 16;

    #[cfg(not(target_arch = "wasm32"))]
    let res = unsafe {
        crate::sys::syscall6(45, stream as u64, payload.as_mut_ptr() as u64, max_results, 0, src_addr.as_mut_ptr() as u64, &mut addr_len as *mut u32 as u64)
    };
    #[cfg(target_arch = "wasm32")]
    let res = u64::MAX;

    let mut header = vec![0u8; 32 + max_results as usize];
    if res != u64::MAX && res > 0 {
        header[0] = 0; // Ok
        header[8..16].copy_from_slice(&res.to_le_bytes());
        header[16..32].copy_from_slice(&src_addr);
        header[32..32+(res as usize)].copy_from_slice(&payload[..res as usize]);
    } else {
        header[0] = 1; // Err
    }
    write_bytes(store, result_ptr, &header[..32 + if res != u64::MAX { res as usize } else { 0 }]).map_err(|_| HaltExecutionError(1))?;
    Ok(vec![])
}
