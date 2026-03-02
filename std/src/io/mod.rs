pub mod serial;
pub mod async_io;
pub mod host;
#[cfg(feature = "userland")]
pub mod wasi;

pub use async_io::{AsyncRead, AsyncWrite};

pub type Result<T> = core::result::Result<T, Error>;

#[derive(Debug)]
pub struct Error {
    repr: i32,
}

impl Error {
    pub fn from_raw_os_error(code: i32) -> Self {
        Self { repr: code }
    }

    pub fn is_would_block(&self) -> bool {
        self.repr == -2
    }
}

pub trait Read {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize>;

    fn read_to_end(&mut self, buf: &mut alloc::vec::Vec<u8>) -> Result<usize> {
        let mut total_read = 0;
        loop {
            if buf.len() == buf.capacity() {
                buf.reserve(32); // Reserve at least some bytes
            }
            
            let len = buf.len();
            let capacity = buf.capacity();
            let unused_space = unsafe {
                core::slice::from_raw_parts_mut(
                    buf.as_mut_ptr().add(len),
                    capacity - len,
                )
            };
            
            match self.read(unused_space) {
                Ok(0) => break,
                Ok(n) => {
                    unsafe { buf.set_len(len + n); }
                    total_read += n;
                }
                Err(e) => return Err(e),
            }
        }
        Ok(total_read)
    }

    fn read_to_string(&mut self, buf: &mut alloc::string::String) -> Result<usize> {
        let mut bytes = alloc::vec::Vec::new();
        let len = self.read_to_end(&mut bytes)?;
        if let Ok(s) = alloc::string::String::from_utf8(bytes) {
            buf.push_str(&s);
            Ok(len)
        } else {
            Err(Error::from_raw_os_error(-1)) // Invalid UTF-8
        }
    }
}

pub trait Write {
    fn write(&mut self, buf: &[u8]) -> Result<usize>;
    fn flush(&mut self) -> Result<()>;

    fn write_all(&mut self, mut buf: &[u8]) -> Result<()> {
        while !buf.is_empty() {
            match self.write(buf) {
                Ok(0) => return Err(Error::from_raw_os_error(-1)),
                Ok(n) => buf = &buf[n..],
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}

pub trait Seek {
    fn seek(&mut self, pos: SeekFrom) -> Result<u64>;
}

pub enum SeekFrom {
    Start(u64),
    End(i64),
    Current(i64),
}
