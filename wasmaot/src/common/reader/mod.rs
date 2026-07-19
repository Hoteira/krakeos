use crate::common::error::ValidationError;
use crate::common::reader::span::Span;

pub mod section_header;
pub mod types;
pub mod span;

#[derive(Clone)]
pub struct WasmReader<'a> {
    pub full_wasm_binary: &'a [u8],
    pub pc: usize,
}

impl<'a> WasmReader<'a> {
    pub const fn new(wasm: &'a [u8]) -> Self {
        Self {
            full_wasm_binary: wasm,
            pc: 0,
        }
    }

    pub fn move_start_to(&mut self, span: Span) -> Result<(), ValidationError> {
        if span.from + span.len > self.full_wasm_binary.len() {
            return Err(ValidationError::Eof);
        }
        self.pc = span.from;
        Ok(())
    }

    pub fn remaining_bytes(&self) -> &[u8] {
        &self.full_wasm_binary[self.pc..]
    }

    pub fn make_span(&self, len: usize) -> Result<Span, ValidationError> {
        if self.pc + len > self.full_wasm_binary.len() {
            return Err(ValidationError::Eof);
        }
        Ok(Span::new(self.pc, len))
    }

    pub fn strip_bytes<const N: usize>(&mut self) -> Result<[u8; N], ValidationError> {
        if N > self.full_wasm_binary.len() - self.pc {
            return Err(ValidationError::Eof);
        }
        let bytes = &self.full_wasm_binary[self.pc..(self.pc + N)];
        self.pc += N;
        Ok(bytes.try_into().expect("the slice length to be exactly N"))
    }

    pub fn strip_bytes_dynamic(&mut self, len: usize) -> Result<&'a [u8], ValidationError> {
        if len > self.full_wasm_binary.len() - self.pc {
            return Err(ValidationError::Eof);
        }
        let bytes = &self.full_wasm_binary[self.pc..(self.pc + len)];
        self.pc += len;
        Ok(bytes)
    }

    pub fn peek_u8(&self) -> Result<u8, ValidationError> {
        self.full_wasm_binary
            .get(self.pc)
            .copied()
            .ok_or(ValidationError::Eof)
    }

    pub fn measure_num_read_bytes<T>(
        &mut self,
        f: impl FnOnce(&mut WasmReader) -> Result<T, ValidationError>,
    ) -> Result<(T, usize), ValidationError> {
        let before = self.pc;
        let ret = f(self)?;
        debug_assert!(
            self.pc >= before,
            "pc was advanced backwards towards the start"
        );
        let num_read_bytes = self.pc - before;
        Ok((ret, num_read_bytes))
    }

    pub fn skip(&mut self, num_bytes: usize) -> Result<(), ValidationError> {
        if num_bytes > self.full_wasm_binary.len() - self.pc {
            return Err(ValidationError::Eof);
        }
        self.pc += num_bytes;
        Ok(())
    }

    pub fn into_inner(self) -> &'a [u8] {
        self.full_wasm_binary
    }

    pub fn handle_transaction<T, E>(
        &mut self,
        f: impl FnOnce(&mut WasmReader<'a>) -> Result<T, E>,
    ) -> Result<T, E> {
        let original = self.clone();
        f(self).inspect_err(|_| {
            *self = original;
        })
    }
}

pub trait WasmReadable: Sized {
    fn read(wasm: &mut WasmReader) -> Result<Self, ValidationError>;
}
