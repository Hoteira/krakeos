use crate::wasm::common::reader::WasmReader;
use core::ops::Index;

#[derive(Copy, Clone, Debug, Hash)]
pub struct Span {
    pub from: usize,
    pub len: usize,
}

impl Span {
    pub const fn new(from: usize, len: usize) -> Self {
        Self { from, len }
    }
    pub const fn len(&self) -> usize {
        self.len
    }
    pub const fn from(&self) -> usize {
        self.from
    }
}

impl<'a> Index<Span> for WasmReader<'a> {
    type Output = [u8];
    fn index(&self, index: Span) -> &'a Self::Output {
        &self.full_wasm_binary[index.from..(index.from + index.len)]
    }
}
