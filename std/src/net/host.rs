// wasi:sockets host functions

pub mod tcp {
    method_export!("wasi:sockets/tcp@0.2.0", "[constructor]tcp-socket",
        pub fn create_tcp_socket(address_family: i32, result_ptr: *mut u8) {
            let res = crate::sys::syscall6(41, address_family as u64, 1, 0, 0, 0, 0);
            if res <= i32::MAX as u64 {
                core::ptr::write_unaligned(result_ptr as *mut u32, 0);
                core::ptr::write_unaligned(result_ptr.add(4) as *mut i32, res as i32);
            } else {
                core::ptr::write_unaligned(result_ptr as *mut u32, 1);
            }
        }
    );

    method_export!("wasi:sockets/tcp@0.2.0", "[method]tcp-socket.start-bind",
        pub fn start_bind(socket: i32, _network: i32, ip_addr_ptr: *const u8, result_ptr: *mut u8) {
            let res = crate::sys::syscall6(49, socket as u64, ip_addr_ptr as u64, 16, 0, 0, 0);
            *result_ptr = if res == 0 { 0 } else { 1 };
        }
    );

    method_export!("wasi:sockets/tcp@0.2.0", "[method]tcp-socket.finish-bind",
        pub fn finish_bind(_socket: i32, result_ptr: *mut u8) {
            *result_ptr = 0;
        }
    );

    method_export!("wasi:sockets/tcp@0.2.0", "[method]tcp-socket.start-connect",
        pub fn start_connect(socket: i32, _network: i32, ip_addr_ptr: *const u8, result_ptr: *mut u8) {
            let res = crate::sys::syscall6(42, socket as u64, ip_addr_ptr as u64, 16, 0, 0, 0);
            *result_ptr = if res == 0 { 0 } else { 1 };
        }
    );

    method_export!("wasi:sockets/tcp@0.2.0", "[method]tcp-socket.finish-connect",
        pub fn finish_connect(socket: i32, result_ptr: *mut u8) {
            let res = crate::sys::syscall6(54, socket as u64, 0, 0, 0, 0, 0);
            *result_ptr = if res == 0 { 0 } else { 1 };
        }
    );

    method_export!("wasi:sockets/tcp@0.2.0", "[method]tcp-socket.start-listen",
        pub fn start_listen(socket: i32, result_ptr: *mut u8) {
            let res = crate::sys::syscall6(51, socket as u64, 10, 0, 0, 0, 0);
            *result_ptr = if res != u64::MAX { 0 } else { 1 };
        }
    );

    method_export!("wasi:sockets/tcp@0.2.0", "[method]tcp-socket.finish-listen",
        pub fn finish_listen(_socket: i32, result_ptr: *mut u8) {
            *result_ptr = 0;
        }
    );

    method_export!("wasi:sockets/tcp@0.2.0", "[method]tcp-socket.accept",
        pub fn accept(socket: i32, result_ptr: *mut u8) {
            let res = crate::sys::syscall6(43, socket as u64, 0, 0, 0, 0, 0);
            if res <= i32::MAX as u64 {
                core::ptr::write_unaligned(result_ptr as *mut u32, 0);
                core::ptr::write_unaligned(result_ptr.add(4) as *mut i32, res as i32);
            } else {
                core::ptr::write_unaligned(result_ptr as *mut u32, 1);
            }
        }
    );

    method_export!("wasi:sockets/tcp@0.2.0", "[method]tcp-socket.send",
        pub fn send(socket: i32, buf_ptr: *const u8, buf_len: u32, result_ptr: *mut u8) {
            let res = crate::sys::syscall6(52, socket as u64, buf_ptr as u64, buf_len as u64, 0, 0, 0);
            if res <= buf_len as u64 {
                *result_ptr = 0;
                core::ptr::write_unaligned(result_ptr.add(8) as *mut u64, res);
            } else {
                *result_ptr = 1;
            }
        }
    );

    method_export!("wasi:sockets/tcp@0.2.0", "[method]tcp-socket.recv",
        pub fn recv(socket: i32, max_len: u32, result_ptr: *mut u8) {
            let buf_ptr = result_ptr.add(32);
            let res = crate::sys::syscall6(53, socket as u64, buf_ptr as u64, max_len as u64, 0, 0, 0);
            if res <= max_len as u64 {
                *result_ptr = 0;
                core::ptr::write_unaligned(result_ptr.add(8) as *mut u64, res);
            } else {
                *result_ptr = 1;
            }
        }
    );

    method_export!("wasi:sockets/tcp@0.2.0", "[resource-drop]tcp-socket",
        pub fn tcp_socket_drop(handle: i32) {
            crate::sys::syscall6(50, handle as u64, 0, 0, 0, 0, 0);
        }
    );
}

pub mod udp {
    method_export!("wasi:sockets/udp@0.2.0", "[method]udp-socket.create",
        pub fn create_udp_socket(address_family: i32, result_ptr: *mut u8) {
            let res = crate::sys::syscall6(41, address_family as u64, 2, 0, 0, 0, 0);
            if res <= i32::MAX as u64 {
                core::ptr::write_unaligned(result_ptr as *mut u32, 0);
                core::ptr::write_unaligned(result_ptr.add(4) as *mut i32, res as i32);
            } else {
                core::ptr::write_unaligned(result_ptr as *mut u32, 1);
            }
        }
    );

    method_export!("wasi:sockets/udp@0.2.0", "[resource-drop]udp-socket",
        pub fn udp_socket_drop(handle: i32) {
            crate::sys::syscall1(50, handle as u64);
        }
    );

    method_export!("wasi:sockets/udp@0.2.0", "[method]udp-socket.start-bind",
        pub fn start_bind(socket: i32, _network: i32, ip_addr_ptr: *const u8, result_ptr: *mut u8) {
            let res = crate::sys::syscall6(49, socket as u64, ip_addr_ptr as u64, 16, 0, 0, 0);
            *result_ptr = if res == 0 { 0 } else { 1 };
        }
    );

    method_export!("wasi:sockets/udp@0.2.0", "[method]outgoing-datagram-stream.send",
        pub fn send(stream: i32, buf_ptr: *const u8, buf_len: u32, dest_addr_ptr: *const u8, result_ptr: *mut u8) {
            let res = crate::sys::syscall6(44, stream as u64, buf_ptr as u64, buf_len as u64, 0, dest_addr_ptr as u64, 16);
            if res <= buf_len as u64 {
                *result_ptr = 0;
                core::ptr::write_unaligned(result_ptr.add(8) as *mut u64, res);
            } else {
                *result_ptr = 1;
            }
        }
    );

    method_export!("wasi:sockets/udp@0.2.0", "[method]incoming-datagram-stream.receive",
        pub fn receive(stream: i32, max_results: u64, result_ptr: *mut u8) {
            let buf_ptr = result_ptr.add(32);
            let mut addr_len: u32 = 16;
            let src_addr_ptr = result_ptr.add(16);
            let res = crate::sys::syscall6(45, stream as u64, buf_ptr as u64, max_results, 0, src_addr_ptr as u64, &mut addr_len as *mut u32 as u64);
            if res <= max_results {
                *result_ptr = 0;
                core::ptr::write_unaligned(result_ptr.add(8) as *mut u64, res);
            } else {
                *result_ptr = 1;
            }
        }
    );
}

method_export!("wasi:sockets/instance-network@0.2.0", "instance-network",
    pub fn instance_network() -> i32 {
        0
    }
);

method_export!("wasi:sockets/ip-name-lookup@0.2.0", "resolve-addresses",
    pub fn resolve_addresses(_network: i32, _name_ptr: *const u8, _name_len: u32, result_ptr: *mut u8) {
        *result_ptr = 1;
    }
);
