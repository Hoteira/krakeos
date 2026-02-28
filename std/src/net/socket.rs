#[cfg(not(target_arch = "wasm32"))]
#[allow(unused_imports)]
use crate::sys::{syscall1, syscall6};
use crate::rust_alloc::vec::Vec;

// --- UDP socket method_export! bindings (from wasi/sockets/udp.rs) ---

method_export!("wasi:sockets/udp@0.2.0", "[method]udp-socket.create",
    pub unsafe fn create_udp_socket(address_family: i32, result_ptr: *mut u8) {
        let res = crate::sys::syscall6(41, address_family as u64, 2, 0, 0, 0, 0); // SOCK_DGRAM=2
        if res == u64::MAX {
            *result_ptr = 1; // err
        } else {
            *result_ptr = 0; // ok
            core::ptr::write_unaligned(result_ptr.add(4) as *mut i32, res as i32);
        }
    }
);

method_export!("wasi:sockets/udp@0.2.0", "[method]udp-socket.start-bind",
    pub unsafe fn udp_start_bind(socket: i32, _network: i32, ip_addr_ptr: *const u8, result_ptr: *mut u8) {
        let res = crate::sys::syscall6(49, socket as u64, ip_addr_ptr as u64, 16, 0, 0, 0);
        if res == 0 {
            *result_ptr = 0; // ok
        } else {
            *result_ptr = 1; // err
        }
    }
);

method_export!("wasi:sockets/udp@0.2.0", "[method]outgoing-datagram-stream.send",
    pub unsafe fn udp_send(stream: i32, buf_ptr: *const u8, buf_len: u32, dest_addr_ptr: *const u8, result_ptr: *mut u8) {
        let res = crate::sys::syscall6(44, stream as u64, buf_ptr as u64, buf_len as u64, 0, dest_addr_ptr as u64, 16);
        if res != u64::MAX {
            *result_ptr = 0; // ok
            core::ptr::write_unaligned(result_ptr.add(8) as *mut u64, res);
        } else {
            *result_ptr = 1; // err
        }
    }
);

method_export!("wasi:sockets/udp@0.2.0", "[method]incoming-datagram-stream.receive",
    pub unsafe fn udp_receive(stream: i32, max_results: u64, result_ptr: *mut u8) {
        let buf_ptr = result_ptr.add(32);
        let mut addr_len: u32 = 16;
        let src_addr_ptr = result_ptr.add(16);
        let res = crate::sys::syscall6(45, stream as u64, buf_ptr as u64, max_results, 0, src_addr_ptr as u64, &mut addr_len as *mut u32 as u64);
        if res != u64::MAX && res > 0 {
            *result_ptr = 0; // ok
            core::ptr::write_unaligned(result_ptr.add(8) as *mut u64, res);
        } else {
            *result_ptr = 1; // err
        }
    }
);

method_export!("wasi:sockets/udp@0.2.0", "[resource-drop]udp-socket",
    pub unsafe fn udp_socket_drop(handle: i32) {
        crate::sys::syscall1(50, handle as u64);
    }
);

// --- TCP socket method_export! bindings (from wasi/sockets/tcp.rs) ---

method_export!("wasi:sockets/tcp@0.2.0", "[method]tcp-socket.start-bind",
    pub unsafe fn tcp_start_bind(_socket: i32, _network: i32, _ip_addr_ptr: *const u8, result_ptr: *mut u8) {
        *result_ptr = 1; // Not supported natively yet
    }
);

method_export!("wasi:sockets/tcp@0.2.0", "[method]tcp-socket.finish-bind",
    pub unsafe fn tcp_finish_bind(_socket: i32, result_ptr: *mut u8) {
        *result_ptr = 1;
    }
);

method_export!("wasi:sockets/tcp@0.2.0", "[method]tcp-socket.start-connect",
    pub unsafe fn tcp_start_connect(_socket: i32, _network: i32, _ip_addr_ptr: *const u8, result_ptr: *mut u8) {
        *result_ptr = 1;
    }
);

method_export!("wasi:sockets/tcp@0.2.0", "[method]tcp-socket.finish-connect",
    pub unsafe fn tcp_finish_connect(_socket: i32, result_ptr: *mut u8) {
        *result_ptr = 1;
    }
);

method_export!("wasi:sockets/tcp@0.2.0", "[method]tcp-socket.start-listen",
    pub unsafe fn tcp_start_listen(_socket: i32, result_ptr: *mut u8) {
        *result_ptr = 1;
    }
);

method_export!("wasi:sockets/tcp@0.2.0", "[method]tcp-socket.finish-listen",
    pub unsafe fn tcp_finish_listen(_socket: i32, result_ptr: *mut u8) {
        *result_ptr = 1;
    }
);

method_export!("wasi:sockets/tcp@0.2.0", "[method]tcp-socket.accept",
    pub unsafe fn tcp_accept(_socket: i32, result_ptr: *mut u8) {
        *result_ptr = 1;
    }
);

// --- Instance network (from wasi/sockets/instance_network.rs) ---

method_export!("wasi:sockets/instance-network@0.2.0", "instance-network",
    pub unsafe fn instance_network() -> i32 {
        0 // Return a dummy network handle
    }
);

// --- IP name lookup (from wasi/sockets/ip_name_lookup.rs) ---

method_export!("wasi:sockets/ip-name-lookup@0.2.0", "resolve-addresses",
    pub unsafe fn resolve_addresses(_network: i32, _name_ptr: *const u8, _name_len: u32, result_ptr: *mut u8) {
        *result_ptr = 1; // Not supported natively yet
    }
);

// --- Socket public API ---

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketAddr {
    V4([u8; 4], u16),
}

pub struct Socket {
    pub handle: usize,
}

impl Socket {
    pub fn new() -> Option<Self> {
        let mut result = [0u8; 8];
        unsafe { create_udp_socket(2, result.as_mut_ptr()) }; // AF_INET=2
        if result[0] == 0 {
            let handle = unsafe { core::ptr::read_unaligned(result.as_ptr().add(4) as *const i32) };
            Some(Socket { handle: handle as usize })
        } else {
            None
        }
    }

    pub fn bind(&self, addr: SocketAddr) -> Result<(), i32> {
        match addr {
            SocketAddr::V4(ip, port) => {
                let mut saddr = [0u8; 16];
                saddr[0] = 2; // AF_INET
                saddr[2] = (port >> 8) as u8;
                saddr[3] = (port & 0xFF) as u8;
                saddr[4] = ip[0];
                saddr[5] = ip[1];
                saddr[6] = ip[2];
                saddr[7] = ip[3];

                let mut result = [0u8; 4];
                unsafe { udp_start_bind(self.handle as i32, 0, saddr.as_ptr(), result.as_mut_ptr()) };
                if result[0] == 0 { Ok(()) } else { Err(-1) }
            }
        }
    }

    pub fn send_to(&self, buf: &[u8], dest: SocketAddr) -> Result<usize, i32> {
        match dest {
            SocketAddr::V4(ip, port) => {
                let mut saddr = [0u8; 16];
                saddr[0] = 2; // AF_INET
                saddr[2] = (port >> 8) as u8;
                saddr[3] = (port & 0xFF) as u8;
                saddr[4] = ip[0];
                saddr[5] = ip[1];
                saddr[6] = ip[2];
                saddr[7] = ip[3];

                let mut result = [0u8; 16];
                unsafe { udp_send(self.handle as i32, buf.as_ptr(), buf.len() as u32, saddr.as_ptr(), result.as_mut_ptr()) };
                if result[0] == 0 {
                    let bytes_sent = unsafe { core::ptr::read_unaligned(result.as_ptr().add(8) as *const u64) };
                    Ok(bytes_sent as usize)
                } else {
                    Err(-1)
                }
            }
        }
    }

    pub fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, SocketAddr), i32> {
        let mut wasi_buf = crate::rust_alloc::vec![0u8; 32 + buf.len()];
        unsafe { udp_receive(self.handle as i32, buf.len() as u64, wasi_buf.as_mut_ptr()) };

        if wasi_buf[0] == 0 {
            let received_len = unsafe { core::ptr::read_unaligned(wasi_buf.as_ptr().add(8) as *const u64) } as usize;
            let mut saddr = [0u8; 16];
            saddr.copy_from_slice(&wasi_buf[16..32]);

            buf[..received_len].copy_from_slice(&wasi_buf[32..32+received_len]);

            let port = ((saddr[2] as u16) << 8) | (saddr[3] as u16);
            let ip = [saddr[4], saddr[5], saddr[6], saddr[7]];
            Ok((received_len, SocketAddr::V4(ip, port)))
        } else {
            Err(-1)
        }
    }
}

impl Drop for Socket {
    fn drop(&mut self) {
        unsafe { udp_socket_drop(self.handle as i32) };
    }
}
