// wasi:sockets host functions

pub mod tcp {
    method_export!("wasi:sockets/tcp@0.2.0", "[constructor]tcp-socket",
        pub unsafe fn create_tcp_socket(address_family: i32, result_ptr: *mut u8) {
            crate::debugln!("CALLING TCP FN host::tcp::create_tcp_socket WITH ARGS: family={}", address_family);
            let res = crate::sys::syscall6(41, address_family as u64, 1, 0, 0, 0, 0);
            if res <= i32::MAX as u64 {
                *result_ptr = 0;
                core::ptr::write_unaligned(result_ptr.add(4) as *mut i32, res as i32);
                crate::debugln!("TCP RESULT: host::tcp::create_tcp_socket SUCCESS, fd={}", res);
            } else {
                *result_ptr = 1;
                crate::debugln!("TCP RESULT: host::tcp::create_tcp_socket FAILED");
            }
        }
    );

    method_export!("wasi:sockets/tcp@0.2.0", "[method]tcp-socket.start-bind",
        pub unsafe fn start_bind(socket: i32, _network: i32, ip_addr_ptr: *const u8, result_ptr: *mut u8) {
            crate::debugln!("CALLING TCP FN host::tcp::start_bind WITH ARGS: fd={}", socket);
            let res = crate::sys::syscall6(49, socket as u64, ip_addr_ptr as u64, 16, 0, 0, 0);
            *result_ptr = if res == 0 { 0 } else { 1 };
            crate::debugln!("TCP RESULT: host::tcp::start_bind RESULT: {}", *result_ptr);
        }
    );

    method_export!("wasi:sockets/tcp@0.2.0", "[method]tcp-socket.finish-bind",
        pub unsafe fn finish_bind(_socket: i32, result_ptr: *mut u8) {
            crate::debugln!("CALLING TCP FN host::tcp::finish_bind");
            *result_ptr = 0;
            crate::debugln!("TCP RESULT: host::tcp::finish_bind SUCCESS");
        }
    );

    method_export!("wasi:sockets/tcp@0.2.0", "[method]tcp-socket.start-connect",
        pub unsafe fn start_connect(socket: i32, _network: i32, ip_addr_ptr: *const u8, result_ptr: *mut u8) {
            crate::debugln!("CALLING TCP FN host::tcp::start_connect WITH ARGS: fd={}", socket);
            let res = crate::sys::syscall6(42, socket as u64, ip_addr_ptr as u64, 16, 0, 0, 0);
            *result_ptr = if res == 0 { 0 } else { 1 };
            crate::debugln!("TCP RESULT: host::tcp::start_connect RESULT: {}", *result_ptr);
        }
    );

    method_export!("wasi:sockets/tcp@0.2.0", "[method]tcp-socket.finish-connect",
        pub unsafe fn finish_connect(socket: i32, result_ptr: *mut u8) {
            crate::debugln!("CALLING TCP FN host::tcp::finish_connect WITH ARGS: fd={}", socket);
            let res = crate::sys::syscall6(54, socket as u64, 0, 0, 0, 0, 0);
            *result_ptr = if res == 0 { 0 } else { 1 };
            crate::debugln!("TCP RESULT: host::tcp::finish_connect RESULT: {}", *result_ptr);
        }
    );

    method_export!("wasi:sockets/tcp@0.2.0", "[method]tcp-socket.start-listen",
        pub unsafe fn start_listen(socket: i32, result_ptr: *mut u8) {
            crate::debugln!("CALLING TCP FN host::tcp::start_listen WITH ARGS: fd={}", socket);
            let res = crate::sys::syscall6(51, socket as u64, 10, 0, 0, 0, 0);
            *result_ptr = if res != u64::MAX { 0 } else { 1 };
            crate::debugln!("TCP RESULT: host::tcp::start_listen RESULT: {}", *result_ptr);
        }
    );

    method_export!("wasi:sockets/tcp@0.2.0", "[method]tcp-socket.finish-listen",
        pub unsafe fn finish_listen(_socket: i32, result_ptr: *mut u8) {
            crate::debugln!("CALLING TCP FN host::tcp::finish_listen");
            *result_ptr = 0;
            crate::debugln!("TCP RESULT: host::tcp::finish_listen SUCCESS");
        }
    );

    method_export!("wasi:sockets/tcp@0.2.0", "[method]tcp-socket.accept",
        pub unsafe fn accept(socket: i32, result_ptr: *mut u8) {
            crate::debugln!("CALLING TCP FN host::tcp::accept WITH ARGS: fd={}", socket);
            let res = crate::sys::syscall6(43, socket as u64, 0, 0, 0, 0, 0);
            if res <= i32::MAX as u64 {
                *result_ptr = 0;
                core::ptr::write_unaligned(result_ptr.add(4) as *mut i32, res as i32);
                crate::debugln!("TCP RESULT: host::tcp::accept SUCCESS, new_fd={}", res);
            } else {
                *result_ptr = 1;
                crate::debugln!("TCP RESULT: host::tcp::accept RESULT: NONE/FAILED");
            }
        }
    );

    method_export!("wasi:sockets/tcp@0.2.0", "[method]tcp-socket.send",
        pub unsafe fn send(socket: i32, buf_ptr: *const u8, buf_len: u32, result_ptr: *mut u8) {
            crate::debugln!("CALLING TCP FN host::tcp::send WITH ARGS: fd={}, len={}", socket, buf_len);
            let res = crate::sys::syscall6(52, socket as u64, buf_ptr as u64, buf_len as u64, 0, 0, 0);
            if res <= buf_len as u64 {
                *result_ptr = 0;
                core::ptr::write_unaligned(result_ptr.add(8) as *mut u64, res);
                crate::debugln!("TCP RESULT: host::tcp::send SUCCESS, sent={}", res);
            } else {
                *result_ptr = 1;
                crate::debugln!("TCP RESULT: host::tcp::send FAILED");
            }
        }
    );

    method_export!("wasi:sockets/tcp@0.2.0", "[method]tcp-socket.recv",
        pub unsafe fn recv(socket: i32, max_len: u32, result_ptr: *mut u8) {
            crate::debugln!("CALLING TCP FN host::tcp::recv WITH ARGS: fd={}, max={}", socket, max_len);
            let buf_ptr = result_ptr.add(32);
            let res = crate::sys::syscall6(53, socket as u64, buf_ptr as u64, max_len as u64, 0, 0, 0);
            if res <= max_len as u64 {
                *result_ptr = 0;
                core::ptr::write_unaligned(result_ptr.add(8) as *mut u64, res);
                crate::debugln!("TCP RESULT: host::tcp::recv SUCCESS, read={}", res);
            } else {
                *result_ptr = 1;
                crate::debugln!("TCP RESULT: host::tcp::recv RESULT: WOULDBLOCK/FAILED");
            }
        }
    );

    method_export!("wasi:sockets/tcp@0.2.0", "[resource-drop]tcp-socket",
        pub unsafe fn tcp_socket_drop(handle: i32) {
            crate::debugln!("CALLING TCP FN host::tcp::tcp_socket_drop WITH ARGS: fd={}", handle);
            crate::sys::syscall6(50, handle as u64, 0, 0, 0, 0, 0);
            crate::debugln!("TCP RESULT: host::tcp::tcp_socket_drop SUCCESS");
        }
    );
}

pub mod udp {
    method_export!("wasi:sockets/udp@0.2.0", "[method]udp-socket.create",
        pub unsafe fn create_udp_socket(address_family: i32, result_ptr: *mut u8) {
            crate::debugln!("CALLING TCP FN host::udp::create_udp_socket WITH ARGS: family={}", address_family);
            let res = crate::sys::syscall6(41, address_family as u64, 2, 0, 0, 0, 0);
            if res <= i32::MAX as u64 {
                *result_ptr = 0;
                core::ptr::write_unaligned(result_ptr.add(4) as *mut i32, res as i32);
                crate::debugln!("TCP RESULT: host::udp::create_udp_socket SUCCESS, fd={}", res);
            } else {
                *result_ptr = 1;
                crate::debugln!("TCP RESULT: host::udp::create_udp_socket FAILED");
            }
        }
    );

    method_export!("wasi:sockets/udp@0.2.0", "[resource-drop]udp-socket",
        pub unsafe fn udp_socket_drop(handle: i32) {
            crate::debugln!("CALLING TCP FN host::udp::udp_socket_drop WITH ARGS: fd={}", handle);
            crate::sys::syscall1(50, handle as u64);
            crate::debugln!("TCP RESULT: host::udp::udp_socket_drop SUCCESS");
        }
    );

    method_export!("wasi:sockets/udp@0.2.0", "[method]udp-socket.start-bind",
        pub unsafe fn start_bind(socket: i32, _network: i32, ip_addr_ptr: *const u8, result_ptr: *mut u8) {
            crate::debugln!("CALLING TCP FN host::udp::start_bind WITH ARGS: fd={}", socket);
            let res = crate::sys::syscall6(49, socket as u64, ip_addr_ptr as u64, 16, 0, 0, 0);
            *result_ptr = if res == 0 { 0 } else { 1 };
            crate::debugln!("TCP RESULT: host::udp::start_bind RESULT: {}", *result_ptr);
        }
    );

    method_export!("wasi:sockets/udp@0.2.0", "[method]outgoing-datagram-stream.send",
        pub unsafe fn send(stream: i32, buf_ptr: *const u8, buf_len: u32, dest_addr_ptr: *const u8, result_ptr: *mut u8) {
            crate::debugln!("CALLING TCP FN host::udp::send WITH ARGS: fd={}, len={}", stream, buf_len);
            let res = crate::sys::syscall6(44, stream as u64, buf_ptr as u64, buf_len as u64, 0, dest_addr_ptr as u64, 16);
            if res <= buf_len as u64 {
                *result_ptr = 0;
                core::ptr::write_unaligned(result_ptr.add(8) as *mut u64, res);
                crate::debugln!("TCP RESULT: host::udp::send SUCCESS, sent={}", res);
            } else {
                *result_ptr = 1;
                crate::debugln!("TCP RESULT: host::udp::send FAILED");
            }
        }
    );

    method_export!("wasi:sockets/udp@0.2.0", "[method]incoming-datagram-stream.receive",
        pub unsafe fn receive(stream: i32, max_results: u64, result_ptr: *mut u8) {
            crate::debugln!("CALLING TCP FN host::udp::receive WITH ARGS: fd={}, max={}", stream, max_results);
            let buf_ptr = result_ptr.add(32);
            let mut addr_len: u32 = 16;
            let src_addr_ptr = result_ptr.add(16);
            let res = crate::sys::syscall6(45, stream as u64, buf_ptr as u64, max_results, 0, src_addr_ptr as u64, &mut addr_len as *mut u32 as u64);
            if res <= max_results {
                *result_ptr = 0;
                core::ptr::write_unaligned(result_ptr.add(8) as *mut u64, res);
                crate::debugln!("TCP RESULT: host::udp::receive SUCCESS, read={}", res);
            } else {
                *result_ptr = 1;
                crate::debugln!("TCP RESULT: host::udp::receive RESULT: WOULDBLOCK/FAILED");
            }
        }
    );
}

method_export!("wasi:sockets/instance-network@0.2.0", "instance-network",
    pub unsafe fn instance_network() -> i32 {
        crate::debugln!("CALLING TCP FN host::instance_network");
        crate::debugln!("TCP RESULT: host::instance_network RESULT: 0");
        0
    }
);

method_export!("wasi:sockets/ip-name-lookup@0.2.0", "resolve-addresses",
    pub unsafe fn resolve_addresses(_network: i32, _name_ptr: *const u8, _name_len: u32, result_ptr: *mut u8) {
        crate::debugln!("CALLING TCP FN host::resolve_addresses");
        *result_ptr = 1;
        crate::debugln!("TCP RESULT: host::resolve_addresses FAILED");
    }
);
