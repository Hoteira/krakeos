use alloc::sync::Arc;
use std::sync::Mutex;


const PIPE_SIZE: usize = 4096;

pub struct PipeBuffer {
    buffer: [u8; PIPE_SIZE],
    head: usize, 
    tail: usize, 
    count: usize,
    closed: bool,
}

impl PipeBuffer {
    pub fn new() -> Self {
        PipeBuffer {
            buffer: [0; PIPE_SIZE],
            head: 0,
            tail: 0,
            count: 0,
            closed: false,
        }
    }

    pub fn write(&mut self, data: &[u8]) -> usize {
        if self.closed { return 0; }
        
        let mut written = 0;
        for &byte in data {
            if self.count == PIPE_SIZE {
                break; 
            }
            self.buffer[self.head] = byte;
            self.head = (self.head + 1) % PIPE_SIZE;
            self.count += 1;
            written += 1;
        }
        written
    }

    pub fn read(&mut self, data: &mut [u8]) -> usize {
        let mut read = 0;
        for byte in data.iter_mut() {
            if self.count == 0 {
                break; 
            }
            *byte = self.buffer[self.tail];
            self.tail = (self.tail + 1) % PIPE_SIZE;
            self.count -= 1;
            read += 1;
        }
        read
    }
}



#[derive(Clone)]
pub struct Pipe {
    inner: Arc<Mutex<PipeBuffer>>,
}

impl Pipe {
    pub fn new() -> Self {
        Pipe {
            inner: Arc::new(Mutex::new(PipeBuffer::new())),
        }
    }

    pub fn read(&self, buf: &mut [u8]) -> usize {
        let mut inner = self.inner.lock();
        inner.read(buf)
    }

    pub fn write(&self, buf: &[u8]) -> usize {
        let mut inner = self.inner.lock();
        inner.write(buf)
    }

    pub fn close(&self) {
        let mut inner = self.inner.lock();
        inner.closed = true;
    }

    pub fn available(&self) -> usize {
        let inner = self.inner.lock();
        inner.count
    }
}
