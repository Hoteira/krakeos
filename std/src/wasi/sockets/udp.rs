// Cross-platform WASI P2 sockets implementation layer.
// When compiled to WASM, these are external imports.
// When compiled to Native, these directly wrap KrakeOS syscalls.

#[cfg(target_arch = "wasm32")]
mod wasi_imports {
    #[link(wasm_import_module = "wasi:sockets/udp@0.2.0")]
    unsafe extern "C" {
        #[link_name = "[method]udp-socket.create"]
        pub fn create_udp_socket(address_family: i32, result_ptr: *mut u8);

        #[link_name = "[method]udp-socket.start-bind"]
        pub fn start_bind(socket: i32, network: i32, ip_addr_ptr: *const u8, result_ptr: *mut u8);

        #[link_name = "[method]outgoing-datagram-stream.send"]
        pub fn send(stream: i32, buf_ptr: *const u8, buf_len: u32, dest_addr_ptr: *const u8, result_ptr: *mut u8);

        #[link_name = "[method]incoming-datagram-stream.receive"]
        pub fn receive(stream: i32, max_results: u64, result_ptr: *mut u8);

        #[link_name = "[resource-drop]udp-socket"]
        pub fn udp_socket_drop(handle: i32);
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasi_imports::*;

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn create_udp_socket(address_family: i32, result_ptr: *mut u8) {
    let res = crate::sys::syscall6(41, address_family as u64, 2, 0, 0, 0, 0); // SOCK_DGRAM=2
    if res <= i32::MAX as u64 {
        *result_ptr = 0; // ok
        core::ptr::write_unaligned(result_ptr.add(4) as *mut i32, res as i32);
    } else {
        *result_ptr = 1; // err
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn udp_socket_drop(handle: i32) {
    crate::sys::syscall1(50, handle as u64);
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn start_bind(socket: i32, _network: i32, ip_addr_ptr: *const u8, result_ptr: *mut u8) {
    let res = crate::sys::syscall6(49, socket as u64, ip_addr_ptr as u64, 16, 0, 0, 0);
    if res == 0 {
        *result_ptr = 0; // ok
    } else {
        *result_ptr = 1; // err
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn send(stream: i32, buf_ptr: *const u8, buf_len: u32, dest_addr_ptr: *const u8, result_ptr: *mut u8) {
    let res = crate::sys::syscall6(44, stream as u64, buf_ptr as u64, buf_len as u64, 0, dest_addr_ptr as u64, 16);
    if res <= buf_len as u64 {
        *result_ptr = 0; // ok
        core::ptr::write_unaligned(result_ptr.add(8) as *mut u64, res);
    } else {
        *result_ptr = 1; // err
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn receive(stream: i32, max_results: u64, result_ptr: *mut u8) {
    let buf_ptr = result_ptr.add(32);
    let mut addr_len: u32 = 16;
    let src_addr_ptr = result_ptr.add(16);
    let res = crate::sys::syscall6(45, stream as u64, buf_ptr as u64, max_results, 0, src_addr_ptr as u64, &mut addr_len as *mut u32 as u64);
    if res <= max_results {
        *result_ptr = 0; // ok
        core::ptr::write_unaligned(result_ptr.add(8) as *mut u64, res);
    } else {
        *result_ptr = 1; // err
    }
}
