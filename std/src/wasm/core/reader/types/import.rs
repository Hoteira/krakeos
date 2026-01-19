use super::global::GlobalType;
use super::{ExternType, MemType, TableType};
use crate::rust_alloc::borrow::ToOwned;
use crate::rust_alloc::string::String;
use crate::wasm::core::error::ValidationError;
use crate::wasm::core::indices::TypeIdx;
use crate::wasm::core::reader::{WasmReadable, WasmReader};
use crate::wasm::ValidationInfo;
#[derive(Debug, Clone)]
pub struct Import {
    #[allow(warnings)]
    pub module_name: String,
    #[allow(warnings)]
    pub name: String,
    #[allow(warnings)]
    pub desc: ImportDesc,
}
impl WasmReadable for Import {
    fn read(wasm: &mut WasmReader) -> Result<Self, ValidationError> {
        let module_name = wasm.read_name()?.to_owned();
        let name = wasm.read_name()?.to_owned();
        let desc = ImportDesc::read(wasm)?;
        Ok(Self {
            module_name,
            name,
            desc,
        })
    }
}
#[derive(Debug, Clone)]
pub enum ImportDesc {
    Func(TypeIdx),
    Table(TableType),
    Mem(MemType),
    Global(GlobalType),
}
impl WasmReadable for ImportDesc {
    fn read(wasm: &mut WasmReader) -> Result<Self, ValidationError> {
        let desc = match wasm.read_u8()? {
            0x00 => Self::Func(wasm.read_var_u32()? as TypeIdx),
            0x01 => Self::Table(TableType::read(wasm)?),
            0x02 => Self::Mem(MemType::read(wasm)?),
            0x03 => Self::Global(GlobalType::read(wasm)?),
            other => return Err(ValidationError::MalformedImportDescDiscriminator(other)),
        };
        Ok(desc)
    }
}
impl ImportDesc {
    pub fn extern_type(&self, validation_info: &ValidationInfo) -> ExternType {
        match self {
            ImportDesc::Func(type_idx) => {
                let func_type = validation_info
                    .types
                    .get(*type_idx)
                    .expect("type index of import descs to always be valid if the validation info is correct");
                ExternType::Func(func_type.clone())
            }
            ImportDesc::Table(ty) => ExternType::Table(*ty),
            ImportDesc::Mem(ty) => ExternType::Mem(*ty),
            ImportDesc::Global(ty) => ExternType::Global(*ty),
        }
    }
}
