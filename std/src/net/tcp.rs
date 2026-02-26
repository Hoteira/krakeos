// std::net TCP wrappers — TcpStream and TcpListener
// Uses unified WASI P2 sockets interface (shimmed on native).

use crate::rust_alloc::vec::Vec;
use crate::wasi::sockets::tcp;

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
        let mut result = [0u8; 8];
        unsafe { tcp::create_tcp_socket(2, result.as_mut_ptr()) };
        if result[0] != 0 {
            return None;
        }
        let sock_fd = unsafe { core::ptr::read_unaligned(result.as_ptr().add(4) as *const i32) as usize };

        // Build sockaddr_in: family(2) + port_be(2) + ip(4)
        let mut saddr = [0u8; 16];
        saddr[0] = 2;
        saddr[2] = (port >> 8) as u8;
        saddr[3] = (port & 0xFF) as u8;
        saddr[4] = ip[0];
        saddr[5] = ip[1];
        saddr[6] = ip[2];
        saddr[7] = ip[3];

        let mut result = [0u8; 4];
        unsafe { tcp::start_connect(sock_fd as i32, 0, saddr.as_ptr(), result.as_mut_ptr()) };
        let ok = result[0] == 0;

        if ok {
            Some(TcpStream { handle: sock_fd })
        } else {
            None
        }
    }

    /// Write data to the connection.
    pub fn write_all(&mut self, buf: &[u8]) -> Result<usize, i32> {
        let mut result = [0u8; 8];
        unsafe {
            tcp::send(
                self.handle as i32,
                buf.as_ptr(),
                buf.len() as u32,
                result.as_mut_ptr(),
            )
        };
        let res = if result[0] == 0 {
            unsafe { core::ptr::read_unaligned(result.as_ptr().add(4) as *const u32) as u64 }
        } else {
            u64::MAX
        };

        if res == u64::MAX {
            Err(-1)
        } else {
            Ok(res as usize)
        }
    }

    /// Read data from the connection. Returns 0 if no data ready yet.
    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize, i32> {
        let mut result = [0u8; 8];
        unsafe {
            tcp::recv(
                self.handle as i32,
                buf.as_mut_ptr(),
                buf.len() as u32,
                result.as_mut_ptr(),
            )
        };
        let res = if result[0] == 0 {
            unsafe { core::ptr::read_unaligned(result.as_ptr().add(4) as *const u32) as u64 }
        } else {
            u64::MAX
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
        unsafe {
            tcp::tcp_socket_drop(self.handle as i32);
        }
        #[cfg(target_arch = "wasm32")]
        unsafe {
            crate::wasi::sockets::tcp::tcp_socket_drop(self.handle as i32);
        }
    }
}

impl TcpListener {
    /// Bind to a port and start listening. Requires a prior bind() via socket API.
    pub fn bind(port: u16) -> Option<Self> {
        let mut result = [0u8; 8];
        unsafe { tcp::create_tcp_socket(2, result.as_mut_ptr()) };
        if result[0] != 0 {
            return None;
        }
        let sock_fd = unsafe { core::ptr::read_unaligned(result.as_ptr().add(4) as *const i32) as usize };

        // Build sockaddr_in
        let mut saddr = [0u8; 16];
        saddr[0] = 2;
        saddr[2] = (port >> 8) as u8;
        saddr[3] = (port & 0xFF) as u8;

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

        Some(TcpListener { handle: sock_fd })
    }

    /// Accept the next incoming connection. Returns None if nothing pending yet.
    pub fn accept(&self) -> Option<TcpStream> {
        let mut result = [0u8; 8];
        unsafe { tcp::accept(self.handle as i32, result.as_mut_ptr()) };
        let res = if result[0] != 0 {
            u64::MAX
        } else {
            unsafe { core::ptr::read_unaligned(result.as_ptr().add(4) as *const i32) as u64 }
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

impl Drop for TcpListener {
    fn drop(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        unsafe {
            crate::sys::syscall6(50, self.handle as u64, 0, 0, 0, 0, 0);
        }
        #[cfg(target_arch = "wasm32")]
        unsafe {
            crate::wasi::sockets::tcp::tcp_socket_drop(self.handle as i32);
        }
    }
}
