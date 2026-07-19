use alloc::vec::Vec;
use crate::common::error::ValidationError;
use crate::common::indices::MemIdx;
use crate::common::reader::section_header::{SectionHeader, SectionTy};
use crate::common::reader::types::data::{DataMode, DataModeActive, DataSegment};
use crate::common::reader::types::global::GlobalType;
use crate::common::reader::WasmReader;
use crate::common::validation::read_constant_expression::read_constant_expression;
use crate::common::validation::validation_stack::ValidationStack;
use crate::common::reader::types::{NumType, ValType};

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
                let wasm_init_offset = wasm.pc - byte_vec.len();
                DataSegment {
                    mode: DataMode::Active(DataModeActive {
                        memory_idx: 0,
                        offset,
                    }),
                    init: byte_vec,
                    wasm_init_offset,
                }
            }
            1 => {
                let byte_vec = wasm.read_vec(|el| el.read_u8())?;
                let wasm_init_offset = wasm.pc - byte_vec.len();
                DataSegment {
                    mode: DataMode::Passive,
                    init: byte_vec,
                    wasm_init_offset,
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
                let wasm_init_offset = wasm.pc - byte_vec.len();
                DataSegment {
                    mode: DataMode::Active(DataModeActive {
                        memory_idx: 0,
                        offset,
                    }),
                    init: byte_vec,
                    wasm_init_offset,
                }
            }
            _ => { return Err(ValidationError::MalformedDataSegmentMode(mode)); },
        };
        Ok(data_sec)
    })
}
