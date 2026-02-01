use crate::wasm::common::error::ValidationError;
use crate::wasm::common::reader::{WasmReadable, WasmReader};
use core::fmt::Debug;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemArg {
    pub align: u32,
    pub offset: u32,
}

impl WasmReadable for MemArg {
    fn read(wasm: &mut WasmReader) -> Result<Self, ValidationError> {
        let align = wasm.read_var_u32()?;
        let offset = wasm.read_var_u32()?;
        Ok(Self { offset, align })
    }
}