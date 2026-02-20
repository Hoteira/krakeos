use crate::sys::{syscall1, syscall6};
use crate::rust_alloc::vec::Vec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketAddr {
    V4([u8; 4], u16),
}

pub struct Socket {
    pub handle: usize,
}

impl Socket {
    pub fn new() -> Option<Self> {
        // SYS_SOCKET = 41
        let res = unsafe { syscall6(41, 2, 2, 0, 0, 0, 0) }; // AF_INET=2, SOCK_DGRAM=2
        if res == u64::MAX { None } else { Some(Socket { handle: res as usize }) }
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
                
                // SYS_BIND = 49
                let res = unsafe { syscall6(49, self.handle as u64, saddr.as_ptr() as u64, 16, 0, 0, 0) };
                if res == 0 { Ok(()) } else { Err(res as i32) }
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

                // SYS_SENDTO = 44
                let res = unsafe { 
                    syscall6(44, self.handle as u64, buf.as_ptr() as u64, buf.len() as u64, 0, saddr.as_ptr() as u64, 16) 
                };
                if res == u64::MAX { Err(-1) } else { Ok(res as usize) }
            }
        }
    }

    pub fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, SocketAddr), i32> {
        let mut saddr = [0u8; 16];
        let mut addr_len = 16u32;
        // SYS_RECVFROM = 45
        let res = unsafe {
            syscall6(45, self.handle as u64, buf.as_mut_ptr() as u64, buf.len() as u64, 0, saddr.as_mut_ptr() as u64, &mut addr_len as *mut u32 as u64)
        };
        
        if res == u64::MAX { return Err(-1); }
        
        let port = ((saddr[2] as u16) << 8) | (saddr[3] as u16);
        let ip = [saddr[4], saddr[5], saddr[6], saddr[7]];
        
        Ok((res as usize, SocketAddr::V4(ip, port)))
    }
}

impl Drop for Socket {
    fn drop(&mut self) {
        // SYS_SOCKET_CLOSE = 50
        unsafe { syscall1(50, self.handle as u64) };
    }
}
