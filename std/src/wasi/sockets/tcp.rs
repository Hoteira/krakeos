// WASI P2 sockets/tcp.rs
// Cross-platform TCP socket layer.
// When compiled to WASM, these are external WASI imports.
// When compiled to Native, these wrap KrakeOS kernel syscalls.

#[cfg(target_arch = "wasm32")]
mod wasi_imports {
    #[link(wasm_import_module = "wasi:sockets/tcp@0.2.0")]
    unsafe extern "C" {
        #[link_name = "[constructor]tcp-socket"]
        pub fn create_tcp_socket(address_family: i32, result_ptr: *mut u8);

        #[link_name = "[method]tcp-socket.start-bind"]
        pub fn start_bind(socket: i32, network: i32, ip_addr_ptr: *const u8, result_ptr: *mut u8);

        #[link_name = "[method]tcp-socket.finish-bind"]
        pub fn finish_bind(socket: i32, result_ptr: *mut u8);

        #[link_name = "[method]tcp-socket.start-connect"]
        pub fn start_connect(socket: i32, network: i32, ip_addr_ptr: *const u8, result_ptr: *mut u8);

        #[link_name = "[method]tcp-socket.finish-connect"]
        pub fn finish_connect(socket: i32, result_ptr: *mut u8);

        #[link_name = "[method]tcp-socket.start-listen"]
        pub fn start_listen(socket: i32, result_ptr: *mut u8);

        #[link_name = "[method]tcp-socket.finish-listen"]
        pub fn finish_listen(socket: i32, result_ptr: *mut u8);

        #[link_name = "[method]tcp-socket.accept"]
        pub fn accept(socket: i32, result_ptr: *mut u8);

        #[link_name = "[method]tcp-socket.send"]
        pub fn send(socket: i32, buf_ptr: *const u8, buf_len: u32, result_ptr: *mut u8);

        #[link_name = "[method]tcp-socket.recv"]
        pub fn recv(socket: i32, max_len: u32, result_ptr: *mut u8);

        #[link_name = "[resource-drop]tcp-socket"]
        pub fn tcp_socket_drop(handle: i32);
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasi_imports::*;

// ── Native (non-WASM) implementations ────────────────────────────────────────

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn create_tcp_socket(address_family: i32, result_ptr: *mut u8) {
    // socket(AF_INET=2, SOCK_STREAM=1, 0)
    let res = crate::sys::syscall6(41, address_family as u64, 1, 0, 0, 0, 0);
    if res <= i32::MAX as u64 {
        *result_ptr = 0; // ok
        core::ptr::write_unaligned(result_ptr.add(4) as *mut i32, res as i32);
    } else {
        *result_ptr = 1; // err
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn start_bind(socket: i32, _network: i32, ip_addr_ptr: *const u8, result_ptr: *mut u8) {
    let res = crate::sys::syscall6(49, socket as u64, ip_addr_ptr as u64, 16, 0, 0, 0);
    *result_ptr = if res == 0 { 0 } else { 1 };
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn finish_bind(_socket: i32, result_ptr: *mut u8) {
    *result_ptr = 0; // bind is synchronous on KrakeOS
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn start_connect(
    socket: i32,
    _network: i32,
    ip_addr_ptr: *const u8,
    result_ptr: *mut u8,
) {
    let res = crate::sys::syscall6(42, socket as u64, ip_addr_ptr as u64, 16, 0, 0, 0);
    *result_ptr = if res == 0 { 0 } else { 1 };
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn finish_connect(_socket: i32, result_ptr: *mut u8) {
    *result_ptr = 0; // connect is blocking on KrakeOS
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn start_listen(socket: i32, result_ptr: *mut u8) {
    let res = crate::sys::syscall6(51, socket as u64, 10, 0, 0, 0, 0);
    *result_ptr = if res != u64::MAX { 0 } else { 1 };
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn finish_listen(_socket: i32, result_ptr: *mut u8) {
    *result_ptr = 0;
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn accept(socket: i32, result_ptr: *mut u8) {
    let res = crate::sys::syscall6(43, socket as u64, 0, 0, 0, 0, 0);
    if res <= i32::MAX as u64 {
        *result_ptr = 0;
        core::ptr::write_unaligned(result_ptr.add(4) as *mut i32, res as i32);
    } else {
        *result_ptr = 1;
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn send(socket: i32, buf_ptr: *const u8, buf_len: u32, result_ptr: *mut u8) {
    let res = crate::sys::syscall6(52, socket as u64, buf_ptr as u64, buf_len as u64, 0, 0, 0);
    if res <= buf_len as u64 {
        *result_ptr = 0;
        core::ptr::write_unaligned(result_ptr.add(8) as *mut u64, res);
    } else {
        *result_ptr = 1;
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn recv(socket: i32, max_len: u32, result_ptr: *mut u8) {
    let buf_ptr = result_ptr.add(32);
    let res = crate::sys::syscall6(53, socket as u64, buf_ptr as u64, max_len as u64, 0, 0, 0);
    if res <= max_len as u64 {
        *result_ptr = 0; // ok
        core::ptr::write_unaligned(result_ptr.add(8) as *mut u64, res);
    } else {
        *result_ptr = 1; // err
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn tcp_socket_drop(handle: i32) {
    crate::sys::syscall6(50, handle as u64, 0, 0, 0, 0, 0);
}
