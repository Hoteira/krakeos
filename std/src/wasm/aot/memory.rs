use crate::rust_alloc::vec::Vec;
use core::fmt;

pub struct ExecutableBuffer {
    pub buffer: Vec<u8>,
}

impl fmt::Debug for ExecutableBuffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExecutableBuffer")
            .field("len", &self.buffer.len())
            .finish()
    }
}

impl ExecutableBuffer {
    pub fn new() -> Self {
        Self { buffer: Vec::new() }
    }

    pub fn emit_u8(&mut self, byte: u8) {
        self.buffer.push(byte);
    }

    pub fn emit_u32(&mut self, val: u32) {
        self.buffer.extend_from_slice(&val.to_le_bytes());
    }

    pub fn emit_u64(&mut self, val: u64) {
        self.buffer.extend_from_slice(&val.to_le_bytes());
    }

    pub fn emit_bytes(&mut self, bytes: &[u8]) {
        self.buffer.extend_from_slice(bytes);
    }

    pub fn ptr(&self) -> *const u8 {
        self.buffer.as_ptr()
    }

    pub fn len(&self) -> usize {
        self.buffer.len()
    }
}
