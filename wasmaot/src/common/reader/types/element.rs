use super::global::GlobalType;
use crate::common::reader::types::RefType;
use alloc::collections::btree_set::BTreeSet;
use alloc::vec::Vec;
use crate::common::error::ValidationError;
use crate::common::indices::{FuncIdx, TableIdx};
use crate::common::reader::span::Span;
use crate::common::reader::types::TableType;
use crate::common::reader::{WasmReadable, WasmReader};
use crate::common::validation::read_constant_expression::read_constant_expression;
use crate::common::validation::validation_stack::ValidationStack;
use crate::common::reader::types::ValType;
use core::fmt::Debug;

#[derive(Clone)]
pub struct ElemType {
    pub init: ElemItems,
    pub mode: ElemMode,
}

impl Debug for ElemType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ElemType")
            .field("init_len", &self.init.len())
            .field("mode", &self.mode)
            .field("ty", &self.init.ty())
            .finish()
    }
}

impl ElemType {
    pub fn ty(&self) -> RefType {
        self.init.ty()
    }
    pub fn to_ref_type(&self) -> RefType {
        match self.init {
            ElemItems::Exprs(rref, _) => rref,
            ElemItems::RefFuncs(_) => RefType::FuncRef,
        }
    }
    pub fn read_from_wasm(
        wasm: &mut WasmReader,
        functions: &[usize],
        validation_context_refs: &mut BTreeSet<FuncIdx>,
        tables: &[TableType],
        imported_global_types: &[GlobalType],
    ) -> Result<Vec<Self>, ValidationError> {
        wasm.read_vec(|wasm| {
            let prop = wasm.read_var_u32()?;
            let num_funcs = functions.len();
            let elem = match prop {
                0 => {
                    let e = parse_validate_active_segment_offset_expr(
                        wasm,
                        imported_global_types,
                        num_funcs,
                        validation_context_refs,
                    )?;
                    let init = parse_validate_shortened_initializer_list(
                        wasm,
                        num_funcs,
                        validation_context_refs,
                    )?;
                    let mode = ElemMode::Active(ActiveElem {
                        table_idx: 0,
                        init_expr: e,
                    });
                    ElemType { init, mode }
                }
                1 => {
                    let _et = parse_elemkind(wasm)?;
                    let init = parse_validate_shortened_initializer_list(
                        wasm,
                        num_funcs,
                        validation_context_refs,
                    )?;
                    let mode = ElemMode::Passive;
                    ElemType { init, mode }
                }
                2 => {
                    let x = wasm.read_var_u32()?;
                    let e = parse_validate_active_segment_offset_expr(
                        wasm,
                        imported_global_types,
                        num_funcs,
                        validation_context_refs,
                    )?;
                    let _et = parse_elemkind(wasm)?;
                    let init = parse_validate_shortened_initializer_list(
                        wasm,
                        num_funcs,
                        validation_context_refs,
                    )?;
                    let mode = ElemMode::Active(ActiveElem {
                        table_idx: x,
                        init_expr: e,
                    });
                    ElemType { init, mode }
                }
                3 => {
                    let _et = parse_elemkind(wasm)?;
                    let init = parse_validate_shortened_initializer_list(
                        wasm,
                        num_funcs,
                        validation_context_refs,
                    )?;
                    let mode = ElemMode::Declarative;
                    ElemType { init, mode }
                }
                4 => {
                    let e = parse_validate_active_segment_offset_expr(
                        wasm,
                        imported_global_types,
                        num_funcs,
                        validation_context_refs,
                    )?;
                    let init = parse_validate_generic_initializer_list(
                        wasm,
                        RefType::FuncRef,
                        imported_global_types,
                        num_funcs,
                        validation_context_refs,
                    )?;
                    let mode = ElemMode::Active(ActiveElem {
                        table_idx: 0,
                        init_expr: e,
                    });
                    ElemType { init, mode }
                }
                5 => {
                    let et = RefType::read(wasm)?;
                    let init = parse_validate_generic_initializer_list(
                        wasm,
                        et,
                        imported_global_types,
                        num_funcs,
                        validation_context_refs,
                    )?;
                    let mode = ElemMode::Passive;
                    ElemType { init, mode }
                }
                6 => {
                    let x = wasm.read_var_u32()?;
                    let e = parse_validate_active_segment_offset_expr(
                        wasm,
                        imported_global_types,
                        num_funcs,
                        validation_context_refs,
                    )?;
                    let et = RefType::read(wasm)?;
                    let init = parse_validate_generic_initializer_list(
                        wasm,
                        et,
                        imported_global_types,
                        num_funcs,
                        validation_context_refs,
                    )?;
                    let mode = ElemMode::Active(ActiveElem {
                        table_idx: x,
                        init_expr: e,
                    });
                    ElemType { init, mode }
                }
                7 => {
                    let et = RefType::read(wasm)?;
                    let init = parse_validate_generic_initializer_list(
                        wasm,
                        et,
                        imported_global_types,
                        num_funcs,
                        validation_context_refs,
                    )?;
                    let mode = ElemMode::Declarative;
                    ElemType { init, mode }
                }
                8.. => {
                    return Err(ValidationError::InvalidBinaryFormatVersion);
                }
            };
            let t = elem.ty();
            match elem.mode {
                ElemMode::Active(ActiveElem {
                                     table_idx: x,
                                     init_expr: _expr,
                                 }) => {
                    let table_type = tables
                        .get(x as usize)
                        .ok_or(ValidationError::InvalidTableIdx(x as TableIdx))?;
                    if table_type.et != t {
                        return Err(ValidationError::ActiveElementSegmentTypeMismatch);
                    }
                }
                ElemMode::Declarative | ElemMode::Passive => (),
            }
            Ok(elem)
        })
    }
}

#[derive(Debug, Clone)]
pub enum ElemItems {
    RefFuncs(Vec<u32>),
    Exprs(RefType, Vec<Span>),
}

impl ElemItems {
    pub fn ty(&self) -> RefType {
        match self {
            Self::RefFuncs(_) => RefType::FuncRef,
            Self::Exprs(rty, _) => *rty,
        }
    }
    pub fn len(&self) -> usize {
        match self {
            Self::RefFuncs(ref_funcs) => ref_funcs.len(),
            Self::Exprs(_, exprs) => exprs.len(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum ElemMode {
    Passive,
    Active(ActiveElem),
    Declarative,
}

#[derive(Debug, Clone)]
pub struct ActiveElem {
    pub table_idx: u32,
    pub init_expr: Span,
}

fn parse_validate_active_segment_offset_expr(
    wasm: &mut WasmReader,
    imported_global_types: &[GlobalType],
    num_funcs: usize,
    validation_context_refs: &mut BTreeSet<FuncIdx>,
) -> Result<Span, ValidationError> {
    let mut valid_stack = ValidationStack::new();
    let (span, seen_func_refs) =
        read_constant_expression(wasm, &mut valid_stack, imported_global_types, num_funcs)?;
    validation_context_refs.extend(seen_func_refs);
    valid_stack.assert_val_types(&[ValType::NumType(crate::common::reader::types::NumType::I32)], true)?;
    Ok(span)
}

fn parse_validate_shortened_initializer_list(
    wasm: &mut WasmReader,
    num_funcs: usize,
    validation_context_refs: &mut BTreeSet<FuncIdx>,
) -> Result<ElemItems, ValidationError> {
    wasm.read_vec(|w| {
        let func_idx = w.read_var_u32()?;
        if num_funcs <= func_idx as usize {
            return Err(ValidationError::InvalidFuncIdx(func_idx as usize));
        }
        validation_context_refs.insert(func_idx as FuncIdx);
        Ok(func_idx)
    })
        .map(ElemItems::RefFuncs)
}

fn parse_validate_generic_initializer_list(
    wasm: &mut WasmReader,
    expected_type: RefType,
    imported_global_types: &[GlobalType],
    num_funcs: usize,
    validation_context_refs: &mut BTreeSet<FuncIdx>,
) -> Result<ElemItems, ValidationError> {
    wasm.read_vec(|w| {
        let mut valid_stack = ValidationStack::new();
        let (span, seen_func_refs) =
            read_constant_expression(w, &mut valid_stack, imported_global_types, num_funcs)?;
        validation_context_refs.extend(seen_func_refs);
        valid_stack.assert_val_types(&[ValType::RefType(expected_type)], true)?;
        Ok(span)
    })
        .map(|v| ElemItems::Exprs(expected_type, v))
}

fn parse_elemkind(wasm: &mut WasmReader) -> Result<u8, ValidationError> {
    let et = wasm.read_u8()?;
    if et != 0x00 {
        Err(ValidationError::MalformedElemKindDiscriminator(et))
    } else {
        Ok(et)
    }
}
