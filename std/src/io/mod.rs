pub mod serial;

pub type Result<T> = core::result::Result<T, Error>;

#[derive(Debug)]
pub struct Error {
    repr: i32,
}

impl Error {
    pub fn from_raw_os_error(code: i32) -> Error {
        Error { repr: code }
    }
}

pub trait Read {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize>;
    
    fn read_to_end(&mut self, buf: &mut rust_alloc::vec::Vec<u8>) -> Result<usize> {
        let mut temp = [0u8; 1024];
        let mut total = 0;
        loop {
            match self.read(&mut temp) {
                Ok(0) => break,
                Ok(n) => {
                    buf.extend_from_slice(&temp[..n]);
                    total += n;
                }
                Err(e) => return Err(e),
            }
        }
        Ok(total)
    }

    fn read_to_string(&mut self, buf: &mut rust_alloc::string::String) -> Result<usize> {
        let mut bytes = rust_alloc::vec::Vec::new();
        let len = self.read_to_end(&mut bytes)?;
        if let Ok(s) = rust_alloc::string::String::from_utf8(bytes) {
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
