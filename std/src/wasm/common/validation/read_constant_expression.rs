use crate::rust_alloc::vec::Vec;
use crate::wasm::common::error::ValidationError;
use crate::wasm::common::indices::{FuncIdx, GlobalIdx};
use crate::wasm::common::reader::span::Span;
use crate::wasm::common::reader::types::global::GlobalType;
use crate::wasm::common::reader::{WasmReadable, WasmReader};
use crate::wasm::common::validation::validation_stack::ValidationStack;
use crate::wasm::common::reader::types::{NumType, RefType, ValType};

pub fn read_constant_expression(
    wasm: &mut WasmReader,
    stack: &mut ValidationStack,
    globals_ty: &[GlobalType],
    num_funcs: usize,
) -> Result<(Span, Vec<FuncIdx>), ValidationError> {
    let start_pc = wasm.pc;
    let mut seen_func_idxs: Vec<FuncIdx> = Vec::new();
    loop {
        let Ok(first_instr_byte) = wasm.read_u8() else {
            return Err(ValidationError::ExprMissingEnd);
        };
        use crate::wasm::common::reader::types::opcode::*;
        match first_instr_byte {
            END => {
                return Ok((Span::new(start_pc, wasm.pc - start_pc), seen_func_idxs));
            }
            GLOBAL_GET => {
                let global_idx = wasm.read_var_u32()? as GlobalIdx;
                let global = globals_ty
                    .get(global_idx)
                    .ok_or(ValidationError::InvalidGlobalIdx(global_idx))?;
                stack.push_valtype(global.ty);
            }
            I32_CONST => {
                let _num = wasm.read_var_i32()?;
                stack.push_valtype(ValType::NumType(NumType::I32));
            }
            F32_CONST => {
                let _num = wasm.read_f32()?;
                stack.push_valtype(ValType::NumType(NumType::F32));
            }
            F64_CONST => {
                let _num = wasm.read_f64()?;
                stack.push_valtype(ValType::NumType(NumType::F64));
            }
            I64_CONST => {
                let _num = wasm.read_var_i64()?;
                stack.push_valtype(ValType::NumType(NumType::I64));
            }
            REF_NULL => {
                stack.push_valtype(ValType::RefType(RefType::read(wasm)?));
            }
            REF_FUNC => {
                let func_idx = wasm.read_var_u32()? as usize;
                if num_funcs <= func_idx {
                    return Err(ValidationError::InvalidFuncIdx(func_idx));
                }
                seen_func_idxs.push(func_idx);
                stack.push_valtype(ValType::RefType(RefType::FuncRef));
            }
            FD_EXTENSIONS => {
                use crate::wasm::common::reader::types::opcode::fd_extensions::*;
                let Ok(second_instr) = wasm.read_var_u32() else {
                    return Err(ValidationError::ExprMissingEnd);
                };
                match second_instr {
                    V128_CONST => {
                        for _ in 0..16 {
                            let _data = wasm.read_u8()?;
                        }
                        stack.push_valtype(ValType::VecType);
                    }
                    _ => {
                        return Err(ValidationError::InvalidInstr(first_instr_byte));
                    }
                }
            }
            _ => {
                return Err(ValidationError::InvalidInstr(first_instr_byte));
            }
        }
    }
}