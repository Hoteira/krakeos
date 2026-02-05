use crate::rust_alloc::{
    collections::btree_set::BTreeSet,
    vec::Vec,
};
use crate::wasm::common::error::ValidationError;
use crate::wasm::common::indices::{
    DataIdx, ElemIdx, FuncIdx, GlobalIdx, LabelIdx, LocalIdx, MemIdx, TableIdx, TypeIdx,
};
use crate::wasm::common::reader::section_header::{SectionHeader, SectionTy};
use crate::wasm::common::reader::span::Span;
use crate::wasm::common::reader::types::element::ElemType;
use crate::wasm::common::reader::types::global::Global;
use crate::wasm::common::reader::types::memarg::MemArg;
use crate::wasm::common::reader::types::{BlockType, FuncType, MemType, NumType, TableType, ValType, RefType};
use crate::wasm::common::reader::{WasmReadable, WasmReader};
use crate::wasm::common::sidetable::{Sidetable, SidetableEntry};
use crate::wasm::common::validation::validation_stack::{LabelInfo, ValidationStack};
use core::iter;

pub fn validate_code_section(
    wasm: &mut WasmReader,
    _section_header: SectionHeader,
    fn_types: &[FuncType],
    type_idx_of_fn: &[usize],
    num_imported_funcs: usize,
    globals: &[Global],
    memories: &[MemType],
    data_count: &Option<u32>,
    tables: &[TableType],
    elements: &[ElemType],
    validation_context_refs: &BTreeSet<FuncIdx>,
    sidetable: &mut Sidetable,
) -> Result<Vec<(Span, usize)>, ValidationError> {
    let code_block_spans_stps = wasm.read_vec_enumerated(|wasm, idx| {
        let ty_idx = *type_idx_of_fn
            .get(idx + num_imported_funcs)
            .ok_or(ValidationError::FunctionAndCodeSectionsHaveDifferentLengths)?;
        let func_ty = fn_types.get(ty_idx).ok_or(ValidationError::InvalidTypeIdx(ty_idx))?.clone();
        let func_size = wasm.read_var_u32()?;
        let func_block = wasm.make_span(func_size as usize)?;
        let previous_pc = wasm.pc;
        let locals = {
            let params = func_ty.params.valtypes.iter().cloned();
            let declared_locals = read_declared_locals(wasm)?;
            params.chain(declared_locals).collect::<Vec<ValType>>()
        };
        let mut stack = ValidationStack::new_for_func(func_ty);
        let stp = sidetable.len();
        read_instructions(
            wasm,
            &mut stack,
            sidetable,
            &locals,
            globals,
            fn_types,
            type_idx_of_fn,
            memories,
            data_count,
            tables,
            elements,
            validation_context_refs,
        )?;
        if previous_pc + func_size as usize != wasm.pc {
            return Err(ValidationError::CodeExprHasTrailingInstructions);
        }
        Ok((func_block, stp))
    })?;
    Ok(code_block_spans_stps)
}

pub fn read_declared_locals(wasm: &mut WasmReader) -> Result<Vec<ValType>, ValidationError> {
    let locals = wasm.read_vec(|wasm| {
        let n = wasm.read_var_u32()?;
        let valtype = ValType::read(wasm)?;
        Ok((n as usize, valtype))
    })?;
    let mut total_no_of_locals: u64 = 0;
    for local in &locals {
        let temp = local.0 as u64;
        total_no_of_locals = total_no_of_locals.checked_add(temp).ok_or(ValidationError::TooManyLocals(total_no_of_locals))?;
    }
    if total_no_of_locals > u32::MAX as u64 {
        return Err(ValidationError::TooManyLocals(total_no_of_locals));
    }
    let locals = locals
        .into_iter()
        .flat_map(|entry| iter::repeat(entry.1).take(entry.0))
        .collect::<Vec<ValType>>();
    Ok(locals)
}

fn validate_branch_and_generate_sidetable_entry(
    wasm: &WasmReader,
    label_idx: usize,
    stack: &mut ValidationStack,
    sidetable: &mut Sidetable,
    unify_to_expected_types: bool,
) -> Result<(), ValidationError> {
    stack.assert_val_types_of_label_jump_types_on_top(label_idx, unify_to_expected_types)?;
    let stack_len = stack.len();
    let index_of_label_in_ctrl_stack = stack
        .ctrl_stack
        .len()
        .checked_sub(label_idx)
        .and_then(|i| i.checked_sub(1))
        .ok_or(ValidationError::InvalidLabelIdx(label_idx))?;
    let targeted_ctrl_block_entry = stack.ctrl_stack.get_mut(index_of_label_in_ctrl_stack).unwrap();
    let valcnt = targeted_ctrl_block_entry.label_types().len();
    let popcnt = stack_len - targeted_ctrl_block_entry.height - valcnt;
    let stp_here = sidetable.len();
    sidetable.push(SidetableEntry {
        delta_pc: wasm.pc as isize,
        delta_stp: stp_here as isize,
        popcnt,
        valcnt,
    });
    match &mut targeted_ctrl_block_entry.label_info {
        LabelInfo::Block { stps_to_backpatch } => stps_to_backpatch.push(stp_here),
        LabelInfo::Loop { ip, stp } => {
            sidetable[stp_here].delta_pc = *ip as isize - wasm.pc as isize;
            sidetable[stp_here].delta_stp = *stp as isize - stp_here as isize;
        }
        LabelInfo::If {
            stps_to_backpatch, ..
        } => stps_to_backpatch.push(stp_here),
        LabelInfo::Func { stps_to_backpatch } => stps_to_backpatch.push(stp_here),
        LabelInfo::Untyped => unreachable!(),
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn read_instructions(
    wasm: &mut WasmReader,
    stack: &mut ValidationStack,
    sidetable: &mut Sidetable,
    locals: &[ValType],
    globals: &[Global],
    fn_types: &[FuncType],
    type_idx_of_fn: &[usize],
    memories: &[MemType],
    data_count: &Option<u32>,
    tables: &[TableType],
    elements: &[ElemType],
    validation_context_refs: &BTreeSet<FuncIdx>,
) -> Result<(), ValidationError> {
    use crate::wasm::common::reader::types::opcode::*;
    loop {
        let Ok(first_instr_byte) = wasm.read_u8() else {
            return Err(ValidationError::ExprMissingEnd);
        };
        //crate::debugln!("{:#x} ", first_instr_byte);
        match first_instr_byte {
            NOP => {}
            BLOCK => {
                let block_ty = BlockType::read(wasm)?.as_func_type(fn_types)?;
                let label_info = LabelInfo::Block {
                    stps_to_backpatch: Vec::new(),
                };
                stack.assert_push_ctrl(label_info, block_ty, true)?;
            }
            LOOP => {
                let block_ty = BlockType::read(wasm)?.as_func_type(fn_types)?;
                let label_info = LabelInfo::Loop {
                    ip: wasm.pc,
                    stp: sidetable.len(),
                };
                stack.assert_push_ctrl(label_info, block_ty, true)?;
            }
            IF => {
                let block_ty = BlockType::read(wasm)?.as_func_type(fn_types)?;
                stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                let stp_here = sidetable.len();
                sidetable.push(SidetableEntry {
                    delta_pc: wasm.pc as isize,
                    delta_stp: stp_here as isize,
                    popcnt: 0,
                    valcnt: block_ty.params.valtypes.len(),
                });
                let label_info = LabelInfo::If {
                    stp: stp_here,
                    stps_to_backpatch: Vec::new(),
                };
                stack.assert_push_ctrl(label_info, block_ty, true)?;
            }
            ELSE => {
                let (mut label_info, block_ty) = stack.assert_pop_ctrl(true)?;
                if let LabelInfo::If {
                    stp,
                    stps_to_backpatch,
                } = &mut label_info
                {
                    if *stp == usize::MAX {
                        return Err(ValidationError::ElseWithoutMatchingIf);
                    }
                    let stp_here = sidetable.len();
                    sidetable.push(SidetableEntry {
                        delta_pc: wasm.pc as isize,
                        delta_stp: stp_here as isize,
                        popcnt: 0,
                        valcnt: block_ty.returns.valtypes.len(),
                    });
                    stps_to_backpatch.push(stp_here);
                    sidetable[*stp].delta_pc = wasm.pc as isize - sidetable[*stp].delta_pc;
                    sidetable[*stp].delta_stp =
                        sidetable.len() as isize - sidetable[*stp].delta_stp;
                    *stp = usize::MAX;
                    for valtype in block_ty.returns.valtypes.iter().rev() {
                        stack.assert_pop_val_type(*valtype)?;
                    }
                    for valtype in block_ty.params.valtypes.iter() {
                        stack.push_valtype(*valtype);
                    }
                    stack.assert_push_ctrl(label_info, block_ty, true)?;
                } else {
                    return Err(ValidationError::ElseWithoutMatchingIf);
                }
            }
            BR => {
                let label_idx = wasm.read_var_u32()? as LabelIdx;
                validate_branch_and_generate_sidetable_entry(
                    wasm, label_idx, stack, sidetable, false,
                )?;
                stack.make_unspecified()?;
            }
            BR_IF => {
                let label_idx = wasm.read_var_u32()? as LabelIdx;
                stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                validate_branch_and_generate_sidetable_entry(
                    wasm, label_idx, stack, sidetable, true,
                )?;
            }
            BR_TABLE => {
                let label_vec = wasm.read_vec(|wasm| wasm.read_var_u32().map(|v| v as LabelIdx))?;
                let max_label_idx = wasm.read_var_u32()? as LabelIdx;
                stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                for label_idx in &label_vec {
                    validate_branch_and_generate_sidetable_entry(
                        wasm, *label_idx, stack, sidetable, false,
                    )?;
                }
                validate_branch_and_generate_sidetable_entry(
                    wasm,
                    max_label_idx,
                    stack,
                    sidetable,
                    false,
                )?;
                stack.make_unspecified()?;
            }
            END => {
                let (label_info, block_ty) = stack.assert_pop_ctrl(true)?;
                let stp_here = sidetable.len();
                match label_info {
                    LabelInfo::Block { ref stps_to_backpatch } | LabelInfo::If { ref stps_to_backpatch, .. } | LabelInfo::Func { ref stps_to_backpatch } => {
                        if let LabelInfo::If { stp, .. } = label_info {
                            if stp != usize::MAX {
                                if !(block_ty.params == block_ty.returns) {
                                    return Err(ValidationError::IfWithoutMatchingElse);
                                }
                                sidetable[stp].delta_pc = (wasm.pc as isize) - sidetable[stp].delta_pc;
                                sidetable[stp].delta_stp =
                                    (stp_here as isize) - sidetable[stp].delta_stp;
                            }
                        }
                        stps_to_backpatch.iter().for_each(|i| {
                            sidetable[*i].delta_pc = (wasm.pc as isize) - sidetable[*i].delta_pc;
                            if let LabelInfo::Func { .. } = label_info {
                                sidetable[*i].delta_pc -= 1;
                            }
                            sidetable[*i].delta_stp = (stp_here as isize) - sidetable[*i].delta_stp;
                        });
                    }
                    _ => {}
                }
                if stack.ctrl_stack.is_empty() {
                    return Ok(());
                }
            }
            RETURN => {
                let label_idx = stack.ctrl_stack.len() - 1;
                validate_branch_and_generate_sidetable_entry(
                    wasm, label_idx, stack, sidetable, false,
                )?;
                stack.make_unspecified()?;
            }
            CALL => {
                let func_idx = wasm.read_var_u32()? as FuncIdx;
                let type_idx = *type_idx_of_fn
                    .get(func_idx)
                    .ok_or(ValidationError::InvalidFuncIdx(func_idx))?;
                let func_ty = &fn_types[type_idx];
                for typ in func_ty.params.valtypes.iter().rev() {
                    stack.assert_pop_val_type(*typ)?;
                }
                for typ in func_ty.returns.valtypes.iter() {
                    stack.push_valtype(*typ);
                }
            }
            CALL_INDIRECT => {
                let type_idx = wasm.read_var_u32()? as TypeIdx;
                let table_idx = wasm.read_var_u32()? as TableIdx;
                let tab = tables
                    .get(table_idx)
                    .ok_or(ValidationError::InvalidTableIdx(table_idx))?;
                if tab.et != RefType::FuncRef {
                    return Err(ValidationError::IndirectCallToNonFuncRefTable(tab.et));
                }
                let func_ty = fn_types
                    .get(type_idx)
                    .ok_or(ValidationError::InvalidTypeIdx(type_idx))?;
                stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                for typ in func_ty.params.valtypes.iter().rev() {
                    stack.assert_pop_val_type(*typ)?;
                }
                for typ in func_ty.returns.valtypes.iter() {
                    stack.push_valtype(*typ);
                }
            }
            UNREACHABLE => {
                stack.make_unspecified()?;
            }
            DROP => {
                stack.drop_val()?;
            }
            SELECT => {
                stack.validate_polymorphic_select()?;
            }
            SELECT_T => {
                let type_vec = wasm.read_vec(ValType::read)?;
                if type_vec.len() != 1 {
                    return Err(ValidationError::InvalidSelectTypeVectorLength(
                        type_vec.len(),
                    ));
                }
                stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                stack.assert_pop_val_type(type_vec[0])?;
                stack.assert_pop_val_type(type_vec[0])?;
                stack.push_valtype(type_vec[0]);
            }
            LOCAL_GET => {
                let local_idx = wasm.read_var_u32()? as LocalIdx;
                let local_ty = locals
                    .get(local_idx)
                    .ok_or(ValidationError::InvalidLocalIdx(local_idx))?;
                stack.push_valtype(*local_ty);
            }
            LOCAL_SET => {
                let local_idx = wasm.read_var_u32()? as LocalIdx;
                let local_ty = locals
                    .get(local_idx)
                    .ok_or(ValidationError::InvalidLocalIdx(local_idx))?;
                stack.assert_pop_val_type(*local_ty)?;
            }
            LOCAL_TEE => {
                let local_idx = wasm.read_var_u32()? as LocalIdx;
                let local_ty = locals
                    .get(local_idx)
                    .ok_or(ValidationError::InvalidLocalIdx(local_idx))?;
                stack.assert_val_types_on_top(&[*local_ty], true)?;
            }
            GLOBAL_GET => {
                let global_idx = wasm.read_var_u32()? as GlobalIdx;
                let global = globals
                    .get(global_idx)
                    .ok_or(ValidationError::InvalidGlobalIdx(global_idx))?;
                stack.push_valtype(global.ty.ty);
            }
            GLOBAL_SET => {
                let global_idx = wasm.read_var_u32()? as GlobalIdx;
                let global = globals
                    .get(global_idx)
                    .ok_or(ValidationError::InvalidGlobalIdx(global_idx))?;
                if !global.ty.is_mut {
                    return Err(ValidationError::MutationOfConstGlobal);
                }
                stack.assert_pop_val_type(global.ty.ty)?;
            }
            TABLE_GET => {
                let table_idx = wasm.read_var_u32()? as TableIdx;
                if tables.len() <= table_idx {
                    return Err(ValidationError::InvalidTableIdx(table_idx));
                }
                let t = tables.get(table_idx).unwrap().et;
                stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                stack.push_valtype(ValType::RefType(t));
            }
            TABLE_SET => {
                let table_idx = wasm.read_var_u32()? as TableIdx;
                if tables.len() <= table_idx {
                    return Err(ValidationError::InvalidTableIdx(table_idx));
                }
                let t = tables.get(table_idx).unwrap().et;
                stack.assert_pop_ref_type(Some(t))?;
                stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
            }
            I32_LOAD => {
                if memories.is_empty() {
                    return Err(ValidationError::InvalidMemIndex(0));
                }
                let _arg = MemArg::read(wasm)?;
                stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                stack.push_valtype(ValType::NumType(NumType::I32));
            }
            I64_LOAD => {
                if memories.is_empty() {
                    return Err(ValidationError::InvalidMemIndex(0));
                }
                let _arg = MemArg::read(wasm)?;
                stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                stack.push_valtype(ValType::NumType(NumType::I64));
            }
            F32_LOAD => {
                if memories.is_empty() {
                    return Err(ValidationError::InvalidMemIndex(0));
                }
                let _arg = MemArg::read(wasm)?;
                stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                stack.push_valtype(ValType::NumType(NumType::F32));
            }
            F64_LOAD => {
                if memories.is_empty() {
                    return Err(ValidationError::InvalidMemIndex(0));
                }
                let _arg = MemArg::read(wasm)?;
                stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                stack.push_valtype(ValType::NumType(NumType::F64));
            }
            I32_LOAD8_S | I32_LOAD8_U | I32_LOAD16_S | I32_LOAD16_U => {
                if memories.is_empty() {
                    return Err(ValidationError::InvalidMemIndex(0));
                }
                let _arg = MemArg::read(wasm)?;
                stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                stack.push_valtype(ValType::NumType(NumType::I32));
            }
            I64_LOAD8_S | I64_LOAD8_U | I64_LOAD16_S | I64_LOAD16_U | I64_LOAD32_S | I64_LOAD32_U => {
                if memories.is_empty() {
                    return Err(ValidationError::InvalidMemIndex(0));
                }
                let _arg = MemArg::read(wasm)?;
                stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                stack.push_valtype(ValType::NumType(NumType::I64));
            }
            I32_STORE | I32_STORE8 | I32_STORE16 => {
                if memories.is_empty() {
                    return Err(ValidationError::InvalidMemIndex(0));
                }
                let _arg = MemArg::read(wasm)?;
                stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
            }
            I64_STORE | I64_STORE8 | I64_STORE16 | I64_STORE32 => {
                if memories.is_empty() {
                    return Err(ValidationError::InvalidMemIndex(0));
                }
                let _arg = MemArg::read(wasm)?;
                stack.assert_pop_val_type(ValType::NumType(NumType::I64))?;
                stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
            }
            F32_STORE => {
                if memories.is_empty() {
                    return Err(ValidationError::InvalidMemIndex(0));
                }
                let _arg = MemArg::read(wasm)?;
                stack.assert_pop_val_type(ValType::NumType(NumType::F32))?;
                stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
            }
            F64_STORE => {
                if memories.is_empty() {
                    return Err(ValidationError::InvalidMemIndex(0));
                }
                let _arg = MemArg::read(wasm)?;
                stack.assert_pop_val_type(ValType::NumType(NumType::F64))?;
                stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
            }
            MEMORY_SIZE | MEMORY_GROW => {
                let _idx = wasm.read_u8()?;
                if first_instr_byte == MEMORY_GROW {
                    stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                }
                stack.push_valtype(ValType::NumType(NumType::I32));
            }
            REF_NULL => {
                let reftype = RefType::read(wasm)?;
                stack.push_valtype(ValType::RefType(reftype));
            }
            REF_IS_NULL => {
                stack.assert_pop_ref_type(None)?;
                stack.push_valtype(ValType::NumType(NumType::I32));
            }
            REF_FUNC => {
                let func_idx = wasm.read_var_u32()? as FuncIdx;
                if type_idx_of_fn.len() <= func_idx {
                    return Err(ValidationError::InvalidFuncIdx(func_idx));
                }
                if !validation_context_refs.contains(&func_idx) {
                    return Err(ValidationError::ReferencingAnUnreferencedFunction(func_idx));
                }
                stack.push_valtype(ValType::RefType(RefType::FuncRef));
            }
            I32_CONST => {
                wasm.read_var_i32()?;
                stack.push_valtype(ValType::NumType(NumType::I32));
            }
            I64_CONST => {
                wasm.read_var_i64()?;
                stack.push_valtype(ValType::NumType(NumType::I64));
            }
            F32_CONST => {
                wasm.read_f32()?;
                stack.push_valtype(ValType::NumType(NumType::F32));
            }
            F64_CONST => {
                wasm.read_f64()?;
                stack.push_valtype(ValType::NumType(NumType::F64));
            }
            I32_EQZ | I32_EQ | I32_NE | I32_LT_S | I32_LT_U | I32_GT_S | I32_GT_U | I32_LE_S | I32_LE_U | I32_GE_S | I32_GE_U => {
                stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                if first_instr_byte != I32_EQZ {
                    stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                }
                stack.push_valtype(ValType::NumType(NumType::I32));
            }
            I64_EQZ | I64_EQ | I64_NE | I64_LT_S | I64_LT_U | I64_GT_S | I64_GT_U | I64_LE_S | I64_LE_U | I64_GE_S | I64_GE_U => {
                stack.assert_pop_val_type(ValType::NumType(NumType::I64))?;
                if first_instr_byte != I64_EQZ {
                    stack.assert_pop_val_type(ValType::NumType(NumType::I64))?;
                }
                stack.push_valtype(ValType::NumType(NumType::I32));
            }
            F32_EQ | F32_NE | F32_LT | F32_GT | F32_LE | F32_GE => {
                stack.assert_pop_val_type(ValType::NumType(NumType::F32))?;
                stack.assert_pop_val_type(ValType::NumType(NumType::F32))?;
                stack.push_valtype(ValType::NumType(NumType::I32));
            }
            F64_EQ | F64_NE | F64_LT | F64_GT | F64_LE | F64_GE => {
                stack.assert_pop_val_type(ValType::NumType(NumType::F64))?;
                stack.assert_pop_val_type(ValType::NumType(NumType::F64))?;
                stack.push_valtype(ValType::NumType(NumType::I32));
            }
            I32_CLZ | I32_CTZ | I32_POPCNT => {
                stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                stack.push_valtype(ValType::NumType(NumType::I32));
            }
            I32_ADD | I32_SUB | I32_MUL | I32_DIV_S | I32_DIV_U | I32_REM_S | I32_REM_U | I32_AND | I32_OR | I32_XOR | I32_SHL | I32_SHR_S | I32_SHR_U | I32_ROTL | I32_ROTR => {
                stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                stack.push_valtype(ValType::NumType(NumType::I32));
            }
            I64_CLZ | I64_CTZ | I64_POPCNT => {
                stack.assert_pop_val_type(ValType::NumType(NumType::I64))?;
                stack.push_valtype(ValType::NumType(NumType::I64));
            }
            I64_ADD | I64_SUB | I64_MUL | I64_DIV_S | I64_DIV_U | I64_REM_S | I64_REM_U | I64_AND | I64_OR | I64_XOR => {
                stack.assert_pop_val_type(ValType::NumType(NumType::I64))?;
                stack.assert_pop_val_type(ValType::NumType(NumType::I64))?;
                stack.push_valtype(ValType::NumType(NumType::I64));
            }
            I64_SHL | I64_SHR_S | I64_SHR_U | I64_ROTL | I64_ROTR => {
                stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                stack.assert_pop_val_type(ValType::NumType(NumType::I64))?;
                stack.push_valtype(ValType::NumType(NumType::I64));
            }
            F32_ABS | F32_NEG | F32_CEIL | F32_FLOOR | F32_TRUNC | F32_NEAREST | F32_SQRT => {
                stack.assert_pop_val_type(ValType::NumType(NumType::F32))?;
                stack.push_valtype(ValType::NumType(NumType::F32));
            }
            F32_ADD | F32_SUB | F32_MUL | F32_DIV | F32_MIN | F32_MAX | F32_COPYSIGN => {
                stack.assert_pop_val_type(ValType::NumType(NumType::F32))?;
                stack.assert_pop_val_type(ValType::NumType(NumType::F32))?;
                stack.push_valtype(ValType::NumType(NumType::F32));
            }
            F64_ABS | F64_NEG | F64_CEIL | F64_FLOOR | F64_TRUNC | F64_NEAREST | F64_SQRT => {
                stack.assert_pop_val_type(ValType::NumType(NumType::F64))?;
                stack.push_valtype(ValType::NumType(NumType::F64));
            }
            F64_ADD | F64_SUB | F64_MUL | F64_DIV | F64_MIN | F64_MAX | F64_COPYSIGN => {
                stack.assert_pop_val_type(ValType::NumType(NumType::F64))?;
                stack.assert_pop_val_type(ValType::NumType(NumType::F64))?;
                stack.push_valtype(ValType::NumType(NumType::F64));
            }
            I32_WRAP_I64 => {
                stack.assert_pop_val_type(ValType::NumType(NumType::I64))?;
                stack.push_valtype(ValType::NumType(NumType::I32));
            }
            I32_TRUNC_F32_S | I32_TRUNC_F32_U | I32_REINTERPRET_F32 => {
                stack.assert_pop_val_type(ValType::NumType(NumType::F32))?;
                stack.push_valtype(ValType::NumType(NumType::I32));
            }
            I32_TRUNC_F64_S | I32_TRUNC_F64_U => {
                stack.assert_pop_val_type(ValType::NumType(NumType::F64))?;
                stack.push_valtype(ValType::NumType(NumType::I32));
            }
            I64_EXTEND_I32_S | I64_EXTEND_I32_U => {
                stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                stack.push_valtype(ValType::NumType(NumType::I64));
            }
            I64_TRUNC_F32_S | I64_TRUNC_F32_U => {
                stack.assert_pop_val_type(ValType::NumType(NumType::F32))?;
                stack.push_valtype(ValType::NumType(NumType::I64));
            }
            I64_TRUNC_F64_S | I64_TRUNC_F64_U | I64_REINTERPRET_F64 => {
                stack.assert_pop_val_type(ValType::NumType(NumType::F64))?;
                stack.push_valtype(ValType::NumType(NumType::I64));
            }
            F32_CONVERT_I32_S | F32_CONVERT_I32_U | F32_REINTERPRET_I32 => {
                stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                stack.push_valtype(ValType::NumType(NumType::F32));
            }
            F32_CONVERT_I64_S | F32_CONVERT_I64_U => {
                stack.assert_pop_val_type(ValType::NumType(NumType::I64))?;
                stack.push_valtype(ValType::NumType(NumType::F32));
            }
            F32_DEMOTE_F64 => {
                stack.assert_pop_val_type(ValType::NumType(NumType::F64))?;
                stack.push_valtype(ValType::NumType(NumType::F32));
            }
            F64_CONVERT_I32_S | F64_CONVERT_I32_U => {
                stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                stack.push_valtype(ValType::NumType(NumType::F64));
            }
            F64_CONVERT_I64_S | F64_CONVERT_I64_U | F64_REINTERPRET_I64 => {
                stack.assert_pop_val_type(ValType::NumType(NumType::I64))?;
                stack.push_valtype(ValType::NumType(NumType::F64));
            }
            F64_PROMOTE_F32 => {
                stack.assert_pop_val_type(ValType::NumType(NumType::F32))?;
                stack.push_valtype(ValType::NumType(NumType::F64));
            }
            I32_EXTEND8_S | I32_EXTEND16_S => {
                stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                stack.push_valtype(ValType::NumType(NumType::I32));
            }
            I64_EXTEND8_S | I64_EXTEND16_S | I64_EXTEND32_S => {
                stack.assert_pop_val_type(ValType::NumType(NumType::I64))?;
                stack.push_valtype(ValType::NumType(NumType::I64));
            }
            FC_EXTENSIONS => {
                let second = wasm.read_var_u32()?;
                use crate::wasm::common::reader::types::opcode::fc_extensions::*;
                match second {
                    I32_TRUNC_SAT_F32_S | I32_TRUNC_SAT_F32_U => {
                        stack.assert_pop_val_type(ValType::NumType(NumType::F32))?;
                        stack.push_valtype(ValType::NumType(NumType::I32));
                    }
                    I32_TRUNC_SAT_F64_S | I32_TRUNC_SAT_F64_U => {
                        stack.assert_pop_val_type(ValType::NumType(NumType::F64))?;
                        stack.push_valtype(ValType::NumType(NumType::I32));
                    }
                    I64_TRUNC_SAT_F32_S | I64_TRUNC_SAT_F32_U => {
                        stack.assert_pop_val_type(ValType::NumType(NumType::F32))?;
                        stack.push_valtype(ValType::NumType(NumType::I64));
                    }
                    I64_TRUNC_SAT_F64_S | I64_TRUNC_SAT_F64_U => {
                        stack.assert_pop_val_type(ValType::NumType(NumType::F64))?;
                        stack.push_valtype(ValType::NumType(NumType::I64));
                    }
                    MEMORY_INIT => {
                        let _data_idx = wasm.read_var_u32()?;
                        let mem_idx = wasm.read_u8()? as MemIdx;
                        if mem_idx != 0 { return Err(ValidationError::UnsupportedMultipleMemoriesProposal); }
                        stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                        stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                        stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                    }
                    DATA_DROP => { let _data_idx = wasm.read_var_u32()?; }
                    MEMORY_COPY => {
                        let (dst, src) = (wasm.read_u8()?, wasm.read_u8()?);
                        if dst != 0 || src != 0 { return Err(ValidationError::UnsupportedMultipleMemoriesProposal); }
                        stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                        stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                        stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                    }
                    MEMORY_FILL => {
                        let mem_idx = wasm.read_u8()? as MemIdx;
                        if mem_idx != 0 { return Err(ValidationError::UnsupportedMultipleMemoriesProposal); }
                        stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                        stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                        stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                    }
                    TABLE_INIT => {
                        let _elem_idx = wasm.read_var_u32()?;
                        let _table_idx = wasm.read_var_u32()?;
                        stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                        stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                        stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                    }
                    ELEM_DROP => { let _elem_idx = wasm.read_var_u32()?; }
                    TABLE_COPY => {
                        let _table_x = wasm.read_var_u32()?;
                        let _table_y = wasm.read_var_u32()?;
                        stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                        stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                        stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                    }
                    TABLE_GROW => {
                        let table_idx = wasm.read_var_u32()? as TableIdx;
                        let t = tables.get(table_idx).ok_or(ValidationError::InvalidTableIdx(table_idx))?.et;
                        stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                        stack.assert_pop_ref_type(Some(t))?;
                        stack.push_valtype(ValType::NumType(NumType::I32));
                    }
                    TABLE_SIZE => {
                        let _table_idx = wasm.read_var_u32()?;
                        stack.push_valtype(ValType::NumType(NumType::I32));
                    }
                    TABLE_FILL => {
                        let table_idx = wasm.read_var_u32()? as TableIdx;
                        let t = tables.get(table_idx).ok_or(ValidationError::InvalidTableIdx(table_idx))?.et;
                        stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                        stack.assert_pop_ref_type(Some(t))?;
                        stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                    }
                    _ => return Err(ValidationError::InvalidMultiByteInstr(first_instr_byte, second))
                }
            }
            FD_EXTENSIONS => {
                let second = wasm.read_var_u32()?;
                use crate::wasm::common::reader::types::opcode::fd_extensions::*;
                match second {
                    V128_LOAD | V128_LOAD8X8_S | V128_LOAD8X8_U | V128_LOAD16X4_S | V128_LOAD16X4_U | V128_LOAD32X2_S | V128_LOAD32X2_U | V128_LOAD8_SPLAT | V128_LOAD16_SPLAT | V128_LOAD32_SPLAT | V128_LOAD64_SPLAT | V128_LOAD32_ZERO | V128_LOAD64_ZERO => {
                        let _arg = MemArg::read(wasm)?;
                        stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                        stack.push_valtype(ValType::VecType);
                    }
                    V128_STORE => {
                        let _arg = MemArg::read(wasm)?;
                        stack.assert_pop_val_type(ValType::VecType)?;
                        stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                    }
                    V128_LOAD8_LANE | V128_LOAD16_LANE | V128_LOAD32_LANE | V128_LOAD64_LANE => {
                        let _arg = MemArg::read(wasm)?;
                        let _lane = wasm.read_u8()?;
                        stack.assert_pop_val_type(ValType::VecType)?;
                        stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                        stack.push_valtype(ValType::VecType);
                    }
                    V128_STORE8_LANE | V128_STORE16_LANE | V128_STORE32_LANE | V128_STORE64_LANE => {
                        let _arg = MemArg::read(wasm)?;
                        let _lane = wasm.read_u8()?;
                        stack.assert_pop_val_type(ValType::VecType)?;
                        stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                    }
                    V128_CONST => {
                        for _ in 0..16 { wasm.read_u8()?; }
                        stack.push_valtype(ValType::VecType);
                    }
                    I8X16_SHUFFLE => {
                        for _ in 0..16 { wasm.read_u8()?; }
                        stack.assert_pop_val_type(ValType::VecType)?;
                        stack.assert_pop_val_type(ValType::VecType)?;
                        stack.push_valtype(ValType::VecType);
                    }
                    I8X16_EXTRACT_LANE_S | I8X16_EXTRACT_LANE_U | I16X8_EXTRACT_LANE_S | I16X8_EXTRACT_LANE_U | I32X4_EXTRACT_LANE => {
                        let _lane = wasm.read_u8()?;
                        stack.assert_pop_val_type(ValType::VecType)?;
                        stack.push_valtype(ValType::NumType(NumType::I32));
                    }
                    I64X2_EXTRACT_LANE => {
                        let _lane = wasm.read_u8()?;
                        stack.assert_pop_val_type(ValType::VecType)?;
                        stack.push_valtype(ValType::NumType(NumType::I64));
                    }
                    F32X4_EXTRACT_LANE => {
                        let _lane = wasm.read_u8()?;
                        stack.assert_pop_val_type(ValType::VecType)?;
                        stack.push_valtype(ValType::NumType(NumType::F32));
                    }
                    F64X2_EXTRACT_LANE => {
                        let _lane = wasm.read_u8()?;
                        stack.assert_pop_val_type(ValType::VecType)?;
                        stack.push_valtype(ValType::NumType(NumType::F64));
                    }
                    I8X16_REPLACE_LANE | I16X8_REPLACE_LANE | I32X4_REPLACE_LANE => {
                        let _lane = wasm.read_u8()?;
                        stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                        stack.assert_pop_val_type(ValType::VecType)?;
                        stack.push_valtype(ValType::VecType);
                    }
                    I64X2_REPLACE_LANE => {
                        let _lane = wasm.read_u8()?;
                        stack.assert_pop_val_type(ValType::NumType(NumType::I64))?;
                        stack.assert_pop_val_type(ValType::VecType)?;
                        stack.push_valtype(ValType::VecType);
                    }
                    F32X4_REPLACE_LANE => {
                        let _lane = wasm.read_u8()?;
                        stack.assert_pop_val_type(ValType::NumType(NumType::F32))?;
                        stack.assert_pop_val_type(ValType::VecType)?;
                        stack.push_valtype(ValType::VecType);
                    }
                    F64X2_REPLACE_LANE => {
                        let _lane = wasm.read_u8()?;
                        stack.assert_pop_val_type(ValType::NumType(NumType::F64))?;
                        stack.assert_pop_val_type(ValType::VecType)?;
                        stack.push_valtype(ValType::VecType);
                    }
                    I8X16_SWIZZLE | I8X16_RELAXED_SWIZZLE => {
                        stack.assert_pop_val_type(ValType::VecType)?;
                        stack.assert_pop_val_type(ValType::VecType)?;
                        stack.push_valtype(ValType::VecType);
                    }
                    I8X16_SPLAT | I16X8_SPLAT | I32X4_SPLAT => {
                        stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                        stack.push_valtype(ValType::VecType);
                    }
                    I64X2_SPLAT => {
                        stack.assert_pop_val_type(ValType::NumType(NumType::I64))?;
                        stack.push_valtype(ValType::VecType);
                    }
                    F32X4_SPLAT => {
                        stack.assert_pop_val_type(ValType::NumType(NumType::F32))?;
                        stack.push_valtype(ValType::VecType);
                    }
                    F64X2_SPLAT => {
                        stack.assert_pop_val_type(ValType::NumType(NumType::F64))?;
                        stack.push_valtype(ValType::VecType);
                    }
                    I8X16_EQ | I8X16_NE | I8X16_LT_S | I8X16_LT_U | I8X16_GT_S | I8X16_GT_U | I8X16_LE_S | I8X16_LE_U | I8X16_GE_S | I8X16_GE_U
                    | I16X8_EQ | I16X8_NE | I16X8_LT_S | I16X8_LT_U | I16X8_GT_S | I16X8_GT_U | I16X8_LE_S | I16X8_LE_U | I16X8_GE_S | I16X8_GE_U
                    | I32X4_EQ | I32X4_NE | I32X4_LT_S | I32X4_LT_U | I32X4_GT_S | I32X4_GT_U | I32X4_LE_S | I32X4_LE_U | I32X4_GE_S | I32X4_GE_U
                    | I64X2_EQ | I64X2_NE | I64X2_LT_S | I64X2_GT_S | I64X2_LE_S | I64X2_GE_S
                    | F32X4_EQ | F32X4_NE | F32X4_LT | F32X4_GT | F32X4_LE | F32X4_GE
                    | F64X2_EQ | F64X2_NE | F64X2_LT | F64X2_GT | F64X2_LE | F64X2_GE
                    | V128_AND | V128_ANDNOT | V128_OR | V128_XOR
                    | I8X16_ADD | I8X16_SUB | I8X16_MIN_S | I8X16_MIN_U | I8X16_MAX_S | I8X16_MAX_U | I8X16_AVGR_U | I8X16_ADD_SAT_S | I8X16_ADD_SAT_U | I8X16_SUB_SAT_S | I8X16_SUB_SAT_U
                    | I16X8_ADD | I16X8_SUB | I16X8_MUL | I16X8_MIN_S | I16X8_MIN_U | I16X8_MAX_S | I16X8_MAX_U | I16X8_AVGR_U | I16X8_ADD_SAT_S | I16X8_ADD_SAT_U | I16X8_SUB_SAT_S | I16X8_SUB_SAT_U | I16X8_Q15MULRSAT_S
                    | I32X4_ADD | I32X4_SUB | I32X4_MUL | I32X4_MIN_S | I32X4_MIN_U | I32X4_MAX_S | I32X4_MAX_U | I32X4_DOT_I16X8_S
                    | I64X2_ADD | I64X2_SUB | I64X2_MUL
                    | F32X4_ADD | F32X4_SUB | F32X4_MUL | F32X4_DIV | F32X4_MIN | F32X4_MAX | F32X4_PMIN | F32X4_PMAX
                    | F64X2_ADD | F64X2_SUB | F64X2_MUL | F64X2_DIV | F64X2_MIN | F64X2_MAX | F64X2_PMIN | F64X2_PMAX
                    | I8X16_NARROW_I16X8_S | I8X16_NARROW_I16X8_U | I16X8_NARROW_I32X4_S | I16X8_NARROW_I32X4_U
                    | I16X8_EXTMUL_LOW_I8X16_S | I16X8_EXTMUL_HIGH_I8X16_S | I16X8_EXTMUL_LOW_I8X16_U | I16X8_EXTMUL_HIGH_I8X16_U
                    | I32X4_EXTMUL_LOW_I16X8_S | I32X4_EXTMUL_HIGH_I16X8_S | I32X4_EXTMUL_LOW_I16X8_U | I32X4_EXTMUL_HIGH_I16X8_U
                    | I64X2_EXTMUL_LOW_I32X4_S | I64X2_EXTMUL_HIGH_I32X4_S | I64X2_EXTMUL_LOW_I32X4_U | I64X2_EXTMUL_HIGH_I32X4_U
                    | F32X4_RELAXED_MAX | F32X4_RELAXED_MIN | F64X2_RELAXED_MAX | F64X2_RELAXED_MIN
                    => {
                        stack.assert_pop_val_type(ValType::VecType)?;
                        stack.assert_pop_val_type(ValType::VecType)?;
                        stack.push_valtype(ValType::VecType);
                    }
                    V128_NOT | I8X16_ABS | I8X16_NEG | I8X16_POPCNT
                    | I16X8_ABS | I16X8_NEG | I32X4_ABS | I32X4_NEG | I64X2_ABS | I64X2_NEG
                    | F32X4_ABS | F32X4_NEG | F32X4_SQRT | F32X4_CEIL | F32X4_FLOOR | F32X4_TRUNC | F32X4_NEAREST
                    | F64X2_ABS | F64X2_NEG | F64X2_SQRT | F64X2_CEIL | F64X2_FLOOR | F64X2_TRUNC | F64X2_NEAREST
                    | I16X8_EXTEND_LOW_I8X16_S | I16X8_EXTEND_HIGH_I8X16_S | I16X8_EXTEND_LOW_I8X16_U | I16X8_EXTEND_HIGH_I8X16_U
                    | I32X4_EXTEND_LOW_I16X8_S | I32X4_EXTEND_HIGH_I16X8_S | I32X4_EXTEND_LOW_I16X8_U | I32X4_EXTEND_HIGH_I16X8_U
                    | I64X2_EXTEND_LOW_I32X4_S | I64X2_EXTEND_HIGH_I32X4_S | I64X2_EXTEND_LOW_I32X4_U | I64X2_EXTEND_HIGH_I32X4_U
                    | I16X8_EXTADD_PAIRWISE_I8X16_S | I16X8_EXTADD_PAIRWISE_I8X16_U | I32X4_EXTADD_PAIRWISE_I16X8_S | I32X4_EXTADD_PAIRWISE_I16X8_U
                    | I32X4_TRUNC_SAT_F32X4_S | I32X4_TRUNC_SAT_F32X4_U | F32X4_CONVERT_I32X4_S | F32X4_CONVERT_I32X4_U
                    | I32X4_TRUNC_SAT_F64X2_S_ZERO | I32X4_TRUNC_SAT_F64X2_U_ZERO | F64X2_CONVERT_LOW_I32X4_S | F64X2_CONVERT_LOW_I32X4_U
                    | I32X4_RELAXED_TRUNC_F32X4_S | I32X4_RELAXED_TRUNC_F32X4_U | I32X4_RELAXED_TRUNC_F64X2_S_ZERO | I32X4_RELAXED_TRUNC_F64X2_U_ZERO
                    => {
                        stack.assert_pop_val_type(ValType::VecType)?;
                        stack.push_valtype(ValType::VecType);
                    }
                    V128_BITSELECT | I8X16_RELAXED_LANESELECT | I16X8_RELAXED_LANESELECT | I32X4_RELAXED_LANESELECT | I64X2_RELAXED_LANESELECT
                    | F32X4_RELAXED_MADD | F32X4_RELAXED_NMADD | F64X2_RELAXED_MADD | F64X2_RELAXED_NMADD
                    => {
                        stack.assert_pop_val_type(ValType::VecType)?;
                        stack.assert_pop_val_type(ValType::VecType)?;
                        stack.assert_pop_val_type(ValType::VecType)?;
                        stack.push_valtype(ValType::VecType);
                    }
                    V128_ANY_TRUE | I8X16_ALL_TRUE | I8X16_BITMASK | I16X8_ALL_TRUE | I16X8_BITMASK | I32X4_ALL_TRUE | I32X4_BITMASK | I64X2_ALL_TRUE | I64X2_BITMASK => {
                        stack.assert_pop_val_type(ValType::VecType)?;
                        stack.push_valtype(ValType::NumType(NumType::I32));
                    }
                    I8X16_SHL | I8X16_SHR_S | I8X16_SHR_U | I16X8_SHL | I16X8_SHR_S | I16X8_SHR_U | I32X4_SHL | I32X4_SHR_S | I32X4_SHR_U | I64X2_SHL | I64X2_SHR_S | I64X2_SHR_U => {
                        stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                        stack.assert_pop_val_type(ValType::VecType)?;
                        stack.push_valtype(ValType::VecType);
                    }
                    _ => return Err(ValidationError::InvalidMultiByteInstr(first_instr_byte, second))
                }
            }
            ATOMIC_PREFIX => {
                let second = wasm.read_var_u32()?;
                // crate::debugln!("Validating Atomic opcode {:#x} at PC {:#x}", second, wasm.pc);
                match second {
                    0x03 => { // atomic.fence
                        wasm.read_u8()?; // Reserved byte
                    }
                    0x00..=0x02 => { // notify/wait
                        let _arg = MemArg::read(wasm)?;
                        if second == 0x00 { // notify
                            stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                            stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                            stack.push_valtype(ValType::NumType(NumType::I32));
                        } else { // wait
                            // Order: timeout (I64), expected (I32/I64), address (I32)
                            stack.assert_pop_val_type(ValType::NumType(NumType::I64))?;
                            if second == 0x01 { stack.assert_pop_val_type(ValType::NumType(NumType::I32))?; }
                            else { stack.assert_pop_val_type(ValType::NumType(NumType::I64))?; }
                            stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                            stack.push_valtype(ValType::NumType(NumType::I32));
                        }
                    }
                    0x10..=0x16 => { // load
                        let _arg = MemArg::read(wasm)?;
                        stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                        let returns_i64 = match second {
                            0x11 | 0x14 | 0x15 | 0x16 => true,
                            _ => false,
                        };
                        if returns_i64 { stack.push_valtype(ValType::NumType(NumType::I64)); }
                        else { stack.push_valtype(ValType::NumType(NumType::I32)); }
                    }
                    0x17..=0x1D => { // store
                        let _arg = MemArg::read(wasm)?;
                        let pops_i64 = match second {
                            0x18 | 0x1b | 0x1c | 0x1d => true,
                            _ => false,
                        };
                        if pops_i64 { stack.assert_pop_val_type(ValType::NumType(NumType::I64))?; }
                        else { stack.assert_pop_val_type(ValType::NumType(NumType::I32))?; }
                        stack.assert_pop_val_type(ValType::NumType(NumType::I32))?;
                    }
                    _ => { // RMW / cmpxchg
                        let _arg = MemArg::read(wasm)?;
                        let is_64 = match second {
                            0x1f | 0x22 | 0x23 | 0x24 | // Add
                            0x26 | 0x29 | 0x2a | 0x2b | // Sub
                            0x2d | 0x30 | 0x31 | 0x32 | // And
                            0x34 | 0x37 | 0x38 | 0x39 | // Or
                            0x3b | 0x3e | 0x3f | 0x40 | // Xor
                            0x42 | 0x45 | 0x46 | 0x47 | // Xchg
                            0x49 | 0x4c | 0x4d | 0x4e    // Cmpxchg
                            => true,
                            _ => false,
                        };
                        
                        if second >= 0x48 { // cmpxchg
                            if is_64 {
                                stack.assert_pop_val_type(ValType::NumType(NumType::I64))?; // replacement
                                stack.assert_pop_val_type(ValType::NumType(NumType::I64))?; // expected
                                stack.assert_pop_val_type(ValType::NumType(NumType::I32))?; // address
                                stack.push_valtype(ValType::NumType(NumType::I64));
                            } else {
                                stack.assert_pop_val_type(ValType::NumType(NumType::I32))?; // replacement
                                stack.assert_pop_val_type(ValType::NumType(NumType::I32))?; // expected
                                stack.assert_pop_val_type(ValType::NumType(NumType::I32))?; // address
                                stack.push_valtype(ValType::NumType(NumType::I32));
                            }
                        } else { // RMW
                            if is_64 {
                                stack.assert_pop_val_type(ValType::NumType(NumType::I64))?; // value
                                stack.assert_pop_val_type(ValType::NumType(NumType::I32))?; // address
                                stack.push_valtype(ValType::NumType(NumType::I64));
                            } else {
                                stack.assert_pop_val_type(ValType::NumType(NumType::I32))?; // value
                                stack.assert_pop_val_type(ValType::NumType(NumType::I32))?; // address
                                stack.push_valtype(ValType::NumType(NumType::I32));
                            }
                        }
                    }
                }
            }
            _ => return Err(ValidationError::InvalidInstr(first_instr_byte))
        }
    }
}