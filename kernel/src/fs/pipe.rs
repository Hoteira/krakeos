use alloc::sync::Arc;
use std::sync::Mutex;


const PIPE_SIZE: usize = 4096;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct Termios {
    pub c_iflag: u32,
    pub c_oflag: u32,
    pub c_cflag: u32,
    pub c_lflag: u32,
    pub c_line: u8,
    pub c_cc: [u8; 32],
    pub c_ispeed: u32,
    pub c_ospeed: u32,
}

impl Termios {
    pub fn new() -> Self {
        Self {
            c_iflag: 0,
            c_oflag: 0,
            c_cflag: 0,
            c_lflag: 0, // Default to non-canonical, no echo for now to stay safe
            c_line: 0,
            c_cc: [0; 32],
            c_ispeed: 0,
            c_ospeed: 0,
        }
    }
}

pub struct PipeBuffer {
    buffer: [u8; PIPE_SIZE],
    head: usize, 
    tail: usize, 
    count: usize,
    closed: bool,
    pub termios: Termios,
}

impl PipeBuffer {
    pub fn new() -> Self {
        PipeBuffer {
            buffer: [0; PIPE_SIZE],
            head: 0,
            tail: 0,
            count: 0,
            closed: false,
            termios: Termios::new(),
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
        
        let icanon = (self.termios.c_lflag & 0000002) != 0; // ICANON=0000002

        if icanon {
            // Check if there is a newline in the buffer
            let mut has_newline = false;
            let mut temp_tail = self.tail;
            for _ in 0..self.count {
                if self.buffer[temp_tail] == b'\n' || self.buffer[temp_tail] == b'\r' {
                    has_newline = true;
                    break;
                }
                temp_tail = (temp_tail + 1) % PIPE_SIZE;
            }
            if !has_newline { return 0; }
        }

        for byte in data.iter_mut() {
            if self.count == 0 {
                break; 
            }
            *byte = self.buffer[self.tail];
            self.tail = (self.tail + 1) % PIPE_SIZE;
            self.count -= 1;
            read += 1;
            
            if icanon && (*byte == b'\n' || *byte == b'\r') {
                break;
            }
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

    pub fn get_inner(&self) -> &Arc<Mutex<PipeBuffer>> {
        &self.inner
    }
}
