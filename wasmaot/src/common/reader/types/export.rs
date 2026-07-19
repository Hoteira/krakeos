use super::ExternType;
use alloc::borrow::ToOwned;
use alloc::string::String;
use crate::common::error::ValidationError;
use crate::common::indices::{FuncIdx, GlobalIdx, MemIdx, TableIdx};
use crate::common::reader::{WasmReadable, WasmReader};
use crate::common::validation::ValidationInfo;

#[derive(Debug, Clone)]
pub struct Export {
    pub name: String,
    pub desc: ExportDesc,
}

impl WasmReadable for Export {
    fn read(wasm: &mut WasmReader) -> Result<Self, ValidationError> {
        let name = wasm.read_name()?.to_owned();
        let desc = ExportDesc::read(wasm)?;
        Ok(Export { name, desc })
    }
}

#[derive(Debug, Clone)]
pub enum ExportDesc {
    FuncIdx(FuncIdx),
    TableIdx(TableIdx),
    MemIdx(MemIdx),
    GlobalIdx(GlobalIdx),
}

impl ExportDesc {
    pub fn extern_type(&self, validation_info: &ValidationInfo) -> ExternType {
        match self {
            ExportDesc::FuncIdx(func_idx) => {
                let type_idx = *validation_info
                    .functions
                    .get(*func_idx)
                    .expect("func indices to always be valid if the validation info is correct");
                let func_type = validation_info
                    .types
                    .get(type_idx)
                    .expect("type indices to always be valid if the validation info is correct");
                ExternType::Func(func_type.clone())
            }
            ExportDesc::TableIdx(table_idx) => ExternType::Table(
                *validation_info
                    .tables
                    .get(*table_idx)
                    .expect("table indices to always be valid if the validation info is correct"),
            ),
            ExportDesc::MemIdx(mem_idx) => ExternType::Mem(
                *validation_info
                    .memories
                    .get(*mem_idx)
                    .expect("mem indices to always be valid if the validation info is correct"),
            ),
            ExportDesc::GlobalIdx(global_idx) => ExternType::Global(
                validation_info
                    .globals
                    .get(*global_idx)
                    .expect("global indices to always be valid if the validation info is correct")
                    .ty,
            ),
        }
    }
    pub fn get_function_idx(&self) -> Option<FuncIdx> {
        match self {
            ExportDesc::FuncIdx(func_idx) => Some(*func_idx),
            _ => None,
        }
    }
    pub fn get_global_idx(&self) -> Option<GlobalIdx> {
        match self {
            ExportDesc::GlobalIdx(global_idx) => Some(*global_idx),
            _ => None,
        }
    }
    pub fn get_memory_idx(&self) -> Option<MemIdx> {
        match self {
            ExportDesc::MemIdx(mem_idx) => Some(*mem_idx),
            _ => None,
        }
    }
    pub fn get_table_idx(&self) -> Option<TableIdx> {
        match self {
            ExportDesc::TableIdx(table_idx) => Some(*table_idx),
            _ => None,
        }
    }
}

impl WasmReadable for ExportDesc {
    fn read(wasm: &mut WasmReader) -> Result<Self, ValidationError> {
        let desc_id = wasm.read_u8()?;
        let desc_idx = wasm.read_var_u32()? as usize;
        let desc = match desc_id {
            0x00 => ExportDesc::FuncIdx(desc_idx),
            0x01 => ExportDesc::TableIdx(desc_idx),
            0x02 => ExportDesc::MemIdx(desc_idx),
            0x03 => ExportDesc::GlobalIdx(desc_idx),
            other => return Err(ValidationError::MalformedExportDescDiscriminator(other)),
        };
        Ok(desc)
    }
}
