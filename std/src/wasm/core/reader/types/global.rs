use crate::wasm::core::error::ValidationError;
use crate::wasm::core::reader::span::Span;
use crate::wasm::core::reader::types::ValType;
use crate::wasm::core::reader::{WasmReadable, WasmReader};
#[derive(Debug, Copy, Clone)]
pub struct Global {
    pub ty: GlobalType,
    pub init_expr: Span,
}
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct GlobalType {
    pub ty: ValType,
    pub is_mut: bool,
}
impl WasmReadable for GlobalType {
    fn read(wasm: &mut WasmReader) -> Result<Self, ValidationError> {
        let ty = ValType::read(wasm)?;
        let is_mut = match wasm.read_u8()? {
            0x00 => false,
            0x01 => true,
            other => return Err(ValidationError::MalformedMutDiscriminator(other)),
        };
        Ok(Self { ty, is_mut })
    }
}
