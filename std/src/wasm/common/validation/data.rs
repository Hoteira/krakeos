use crate::rust_alloc::vec::Vec;
use crate::wasm::common::error::ValidationError;
use crate::wasm::common::indices::MemIdx;
use crate::wasm::common::reader::section_header::{SectionHeader, SectionTy};
use crate::wasm::common::reader::types::data::{DataMode, DataModeActive, DataSegment};
use crate::wasm::common::reader::types::global::GlobalType;
use crate::wasm::common::reader::WasmReader;
use crate::wasm::common::validation::read_constant_expression::read_constant_expression;
use crate::wasm::common::validation::validation_stack::ValidationStack;
use crate::wasm::common::reader::types::{NumType, ValType};

pub(super) fn validate_data_section(
    wasm: &mut WasmReader,
    section_header: SectionHeader,
    imported_global_types: &[GlobalType],
    no_of_total_memories: usize,
    num_funcs: usize,
) -> Result<Vec<DataSegment>, ValidationError> {
    assert_eq!(section_header.ty, SectionTy::Data);
    wasm.read_vec(|wasm| {
        let mode = wasm.read_var_u32()?;
        let data_sec: DataSegment = match mode {
            0 => {
                if no_of_total_memories == 0 {
                    return Err(ValidationError::InvalidMemIndex(0));
                }
                let mut valid_stack = ValidationStack::new();
                let (offset, _) = {
                    read_constant_expression(
                        wasm,
                        &mut valid_stack,
                        imported_global_types,
                        num_funcs,
                    )?
                };
                valid_stack.assert_val_types(&[ValType::NumType(NumType::I32)], true)?;
                let byte_vec = wasm.read_vec(|el| el.read_u8())?;
                DataSegment {
                    mode: DataMode::Active(DataModeActive {
                        memory_idx: 0,
                        offset,
                    }),
                    init: byte_vec,
                }
            }
            1 => {
                DataSegment {
                    mode: DataMode::Passive,
                    init: wasm.read_vec(|el| el.read_u8())?,
                }
            }
            2 => {
                let mem_idx = wasm.read_var_u32()? as MemIdx;
                if mem_idx >= no_of_total_memories {
                    return Err(ValidationError::InvalidMemIndex(mem_idx));
                }
                if mem_idx != 0 {
                    return Err(ValidationError::UnsupportedMultipleMemoriesProposal);
                }
                let mut valid_stack = ValidationStack::new();
                let (offset, _) = {
                    read_constant_expression(
                        wasm,
                        &mut valid_stack,
                        imported_global_types,
                        num_funcs,
                    )?
                };
                valid_stack.assert_val_types(&[ValType::NumType(NumType::I32)], true)?;
                let byte_vec = wasm.read_vec(|el| el.read_u8())?;
                DataSegment {
                    mode: DataMode::Active(DataModeActive {
                        memory_idx: 0,
                        offset,
                    }),
                    init: byte_vec,
                }
            }
            _ => { return Err(ValidationError::MalformedDataSegmentMode(mode)); },
        };
        Ok(data_sec)
    })
}