// std::net TCP wrappers — TcpStream and TcpListener
// Native: calls KrakeOS kernel TCP syscalls.
// WASM32: calls wasi:sockets/tcp@0.2.0 imports via wasi/sockets/tcp.rs.

use crate::rust_alloc::vec::Vec;

/// A connected TCP stream.
pub struct TcpStream {
    pub handle: usize,
}

/// A listening TCP socket.
pub struct TcpListener {
    handle: usize,
}

impl TcpStream {
    /// Active open: connect to (ip, port). Blocks until Established or timeout.
    pub fn connect(ip: [u8; 4], port: u16) -> Option<Self> {
        // socket(AF_INET=2, SOCK_STREAM=1, 0) -> fd
        #[cfg(not(target_arch = "wasm32"))]
        let sock_fd = {
            let res = unsafe { crate::sys::syscall6(41, 2, 1, 0, 0, 0, 0) };
            if res == u64::MAX {
                return None;
            }
            res as usize
        };
        #[cfg(target_arch = "wasm32")]
        let sock_fd: usize = {
            use crate::wasi::sockets::tcp;
            let mut result = [0u8; 8];
            unsafe { tcp::create_tcp_socket(2, result.as_mut_ptr()) };
            if result[0] != 0 {
                return None;
            }
            unsafe { core::ptr::read_unaligned(result.as_ptr().add(4) as *const i32) as usize }
        };

        // Build sockaddr_in: family(2) + port_be(2) + ip(4)
        let mut saddr = [0u8; 16];
        saddr[0] = 2;
        saddr[2] = (port >> 8) as u8;
        saddr[3] = (port & 0xFF) as u8;
        saddr[4] = ip[0];
        saddr[5] = ip[1];
        saddr[6] = ip[2];
        saddr[7] = ip[3];

        #[cfg(not(target_arch = "wasm32"))]
        let ok = {
            let res = unsafe {
                crate::sys::syscall6(42, sock_fd as u64, saddr.as_ptr() as u64, 16, 0, 0, 0)
            };
            res != u64::MAX
        };
        #[cfg(target_arch = "wasm32")]
        let ok = {
            use crate::wasi::sockets::tcp;
            let mut result = [0u8; 4];
            unsafe { tcp::start_connect(sock_fd as i32, 0, saddr.as_ptr(), result.as_mut_ptr()) };
            result[0] == 0
        };

        if ok {
            Some(TcpStream { handle: sock_fd })
        } else {
            None
        }
    }

    /// Write data to the connection.
    pub fn write_all(&mut self, buf: &[u8]) -> Result<usize, i32> {
        #[cfg(not(target_arch = "wasm32"))]
        let res = unsafe {
            crate::sys::syscall6(
                52,
                self.handle as u64,
                buf.as_ptr() as u64,
                buf.len() as u64,
                0,
                0,
                0,
            )
        };
        #[cfg(target_arch = "wasm32")]
        let res = {
            use crate::wasi::sockets::tcp;
            let mut result = [0u8; 8];
            unsafe {
                tcp::send(
                    self.handle as i32,
                    buf.as_ptr(),
                    buf.len() as u32,
                    result.as_mut_ptr(),
                )
            };
            if result[0] == 0 {
                unsafe { core::ptr::read_unaligned(result.as_ptr().add(4) as *const u32) as u64 }
            } else {
                u64::MAX
            }
        };
        if res == u64::MAX {
            Err(-1)
        } else {
            Ok(res as usize)
        }
    }

    /// Read data from the connection. Returns 0 if no data ready yet.
    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize, i32> {
        #[cfg(not(target_arch = "wasm32"))]
        let res = unsafe {
            crate::sys::syscall6(
                53,
                self.handle as u64,
                buf.as_mut_ptr() as u64,
                buf.len() as u64,
                0,
                0,
                0,
            )
        };
        #[cfg(target_arch = "wasm32")]
        let res = {
            use crate::wasi::sockets::tcp;
            let mut result = [0u8; 8];
            unsafe {
                tcp::recv(
                    self.handle as i32,
                    buf.as_mut_ptr(),
                    buf.len() as u32,
                    result.as_mut_ptr(),
                )
            };
            if result[0] == 0 {
                unsafe { core::ptr::read_unaligned(result.as_ptr().add(4) as *const u32) as u64 }
            } else {
                u64::MAX
            }
        };
        if res == u64::MAX {
            Err(-1)
        } else {
            Ok(res as usize)
        }
    }
}

impl Drop for TcpStream {
    fn drop(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        unsafe {
            crate::sys::syscall6(50, self.handle as u64, 0, 0, 0, 0, 0);
        }
    }
}

impl TcpListener {
    /// Bind to a port and start listening. Requires a prior bind() via socket API.
    pub fn bind(port: u16) -> Option<Self> {
        #[cfg(not(target_arch = "wasm32"))]
        let sock_fd = {
            let res = unsafe { crate::sys::syscall6(41, 2, 1, 0, 0, 0, 0) };
            if res == u64::MAX {
                return None;
            }
            res as usize
        };
        #[cfg(target_arch = "wasm32")]
        let sock_fd: usize = {
            use crate::wasi::sockets::tcp;
            let mut result = [0u8; 8];
            unsafe { tcp::create_tcp_socket(2, result.as_mut_ptr()) };
            if result[0] != 0 {
                return None;
            }
            unsafe { core::ptr::read_unaligned(result.as_ptr().add(4) as *const i32) as usize }
        };

        // Build sockaddr_in
        let mut saddr = [0u8; 16];
        saddr[0] = 2;
        saddr[2] = (port >> 8) as u8;
        saddr[3] = (port & 0xFF) as u8;

        #[cfg(not(target_arch = "wasm32"))]
        {
            let br = unsafe {
                crate::sys::syscall6(49, sock_fd as u64, saddr.as_ptr() as u64, 16, 0, 0, 0)
            };
            if br != 0 {
                return None;
            }
            let lr = unsafe { crate::sys::syscall6(51, sock_fd as u64, 10, 0, 0, 0, 0) };
            if lr == u64::MAX {
                return None;
            }
        }

        #[cfg(target_arch = "wasm32")]
        {
            use crate::wasi::sockets::tcp;
            let mut result = [0u8; 4];
            unsafe {
                tcp::start_bind(sock_fd as i32, 0, saddr.as_ptr(), result.as_mut_ptr());
            }
            if result[0] != 0 {
                return None;
            }
            unsafe {
                tcp::start_listen(sock_fd as i32, result.as_mut_ptr());
            }
            if result[0] != 0 {
                return None;
            }
        }

        Some(TcpListener { handle: sock_fd })
    }

    /// Accept the next incoming connection. Returns None if nothing pending yet.
    pub fn accept(&self) -> Option<TcpStream> {
        #[cfg(not(target_arch = "wasm32"))]
        let res = unsafe { crate::sys::syscall6(43, self.handle as u64, 0, 0, 0, 0, 0) };
        #[cfg(target_arch = "wasm32")]
        let res = {
            use crate::wasi::sockets::tcp;
            let mut result = [0u8; 8];
            unsafe { tcp::accept(self.handle as i32, result.as_mut_ptr()) };
            if result[0] != 0 {
                u64::MAX
            } else {
                unsafe { core::ptr::read_unaligned(result.as_ptr().add(4) as *const i32) as u64 }
            }
        };

        if res == u64::MAX {
            None
        } else {
            Some(TcpStream {
                handle: res as usize,
            })
        }
    }
}
