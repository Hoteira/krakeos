use crate::rust_alloc::vec::Vec;
use crate::wasi::sockets::udp;

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
        unsafe { udp::create_udp_socket(2, result.as_mut_ptr()) }; // AF_INET=2
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
                unsafe { udp::start_bind(self.handle as i32, 0, saddr.as_ptr(), result.as_mut_ptr()) };
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
                unsafe { udp::send(self.handle as i32, buf.as_ptr(), buf.len() as u32, saddr.as_ptr(), result.as_mut_ptr()) };
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
        unsafe { udp::receive(self.handle as i32, buf.len() as u64, wasi_buf.as_mut_ptr()) };
        
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
        unsafe { udp::udp_socket_drop(self.handle as i32) };
    }
}
