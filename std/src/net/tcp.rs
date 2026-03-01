// std::net TCP wrappers — TcpStream and TcpListener
// Uses unified WASI P2 sockets interface (shimmed on native).

use crate::rust_alloc::vec::Vec;
use crate::net::host::tcp;

pub struct TcpStream { pub handle: usize }
pub struct TcpListener { pub handle: usize }

impl TcpStream {
    pub fn connect(ip: [u8; 4], port: u16) -> Option<Self> {
        crate::debugln!("CALLING TCP FN TcpStream::connect WITH ARGS: ip={:?}, port={}", ip, port);
        let mut res = [0u8; 8];
        unsafe { tcp::create_tcp_socket(2, res.as_mut_ptr()) };
        if res[0] != 0 { crate::debugln!("TCP RESULT: TcpStream::connect FAILED (socket create)"); return None; }
        let fd = unsafe { core::ptr::read_unaligned(res.as_ptr().add(4) as *const i32) as usize };
        
        let mut saddr = [0u8; 16]; saddr[0] = 2; saddr[2] = (port >> 8) as u8; saddr[3] = (port & 0xFF) as u8;
        unsafe { core::ptr::copy_nonoverlapping(ip.as_ptr(), saddr.as_mut_ptr().add(4), 4); }
        
        let mut res_start = [0u8; 4];
        unsafe { tcp::start_connect(fd as i32, 0, saddr.as_ptr(), res_start.as_mut_ptr()) };
        if res_start[0] != 0 { crate::debugln!("TCP RESULT: TcpStream::connect FAILED (start_connect)"); return None; }

        for i in 0..100 {
            let mut res_fin = [0u8; 4];
            unsafe { tcp::finish_connect(fd as i32, res_fin.as_mut_ptr()) };
            if res_fin[0] == 0 {
                crate::debugln!("TCP RESULT: TcpStream::connect SUCCESS, fd={} ({} iters)", fd, i);
                return Some(TcpStream { handle: fd });
            }
            crate::sys::yield_task();
        }
        crate::debugln!("TCP RESULT: TcpStream::connect TIMEOUT");
        None
    }

    pub fn write_all(&mut self, buf: &[u8]) -> Result<usize, i32> {
        crate::debugln!("CALLING TCP FN TcpStream::write_all WITH ARGS: fd={}, len={}", self.handle, buf.len());
        let mut res_buf = [0u8; 16];
        unsafe { tcp::send(self.handle as i32, buf.as_ptr(), buf.len() as u32, res_buf.as_mut_ptr()) };
        if res_buf[0] == 0 {
            let n = unsafe { core::ptr::read_unaligned(res_buf.as_ptr().add(8) as *const u64) } as usize;
            crate::debugln!("TCP RESULT: TcpStream::write_all SUCCESS, sent={}", n);
            Ok(n)
        } else {
            crate::debugln!("TCP RESULT: TcpStream::write_all FAILED");
            Err(-1)
        }
    }

    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize, i32> {
        crate::debugln!("CALLING TCP FN TcpStream::read WITH ARGS: fd={}, max={}", self.handle, buf.len());
        let mut wasi_buf = crate::rust_alloc::vec![0u8; 32 + buf.len()];
        unsafe { tcp::recv(self.handle as i32, buf.len() as u32, wasi_buf.as_mut_ptr()) };
        if wasi_buf[0] == 0 {
            let n = unsafe { core::ptr::read_unaligned(wasi_buf.as_ptr().add(8) as *const u64) } as usize;
            if n > 0 { buf[..n].copy_from_slice(&wasi_buf[32..32 + n]); }
            crate::debugln!("TCP RESULT: TcpStream::read SUCCESS, read={}", n);
            Ok(n)
        } else {
            crate::debugln!("TCP RESULT: TcpStream::read FAILED");
            Err(-1)
        }
    }
}

impl Drop for TcpStream {
    fn drop(&mut self) {
        crate::debugln!("CALLING TCP FN TcpStream::drop WITH ARGS: fd={}", self.handle);
        unsafe { tcp::tcp_socket_drop(self.handle as i32); }
        crate::debugln!("TCP RESULT: TcpStream::drop SUCCESS");
    }
}

impl TcpListener {
    pub fn bind(port: u16) -> Option<Self> {
        crate::debugln!("CALLING TCP FN TcpListener::bind WITH ARGS: port={}", port);
        let mut res = [0u8; 8];
        unsafe { tcp::create_tcp_socket(2, res.as_mut_ptr()) };
        if res[0] != 0 { return None; }
        let fd = unsafe { core::ptr::read_unaligned(res.as_ptr().add(4) as *const i32) as usize };
        let mut saddr = [0u8; 16]; saddr[0] = 2; saddr[2] = (port >> 8) as u8; saddr[3] = (port & 0xFF) as u8;
        let mut res_op = [0u8; 4];
        unsafe { tcp::start_bind(fd as i32, 0, saddr.as_ptr(), res_op.as_mut_ptr()); }
        if res_op[0] != 0 { return None; }
        unsafe { tcp::start_listen(fd as i32, res_op.as_mut_ptr()); }
        if res_op[0] != 0 { return None; }
        crate::debugln!("TCP RESULT: TcpListener::bind SUCCESS, fd={}", fd);
        Some(TcpListener { handle: fd })
    }

    pub fn accept(&self) -> Option<TcpStream> {
        crate::debugln!("CALLING TCP FN TcpListener::accept WITH ARGS: fd={}", self.handle);
        let mut res = [0u8; 8];
        unsafe { tcp::accept(self.handle as i32, res.as_mut_ptr()) };
        if res[0] == 0 {
            let nfd = unsafe { core::ptr::read_unaligned(res.as_ptr().add(4) as *const i32) as usize };
            crate::debugln!("TCP RESULT: TcpListener::accept SUCCESS, new_fd={}", nfd);
            Some(TcpStream { handle: nfd })
        } else {
            crate::debugln!("TCP RESULT: TcpListener::accept RESULT: NONE");
            None
        }
    }
}

impl Drop for TcpListener {
    fn drop(&mut self) {
        crate::debugln!("CALLING TCP FN TcpListener::drop WITH ARGS: fd={}", self.handle);
        unsafe { tcp::tcp_socket_drop(self.handle as i32); }
        crate::debugln!("TCP RESULT: TcpListener::drop SUCCESS");
    }
}
