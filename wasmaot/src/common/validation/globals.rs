use alloc::collections::btree_set::BTreeSet;
use alloc::vec::Vec;
use crate::common::error::ValidationError;
use crate::common::indices::FuncIdx;
use crate::common::reader::section_header::{SectionHeader, SectionTy};
use crate::common::reader::types::global::{Global, GlobalType};
use crate::common::reader::WasmReader;
use crate::common::validation::read_constant_expression::read_constant_expression;
use crate::common::validation::validation_stack::ValidationStack;
use crate::common::reader::WasmReadable;

pub(super) fn validate_global_section(
    wasm: &mut WasmReader,
    section_header: SectionHeader,
    imported_global_types: &[GlobalType],
    validation_context_refs: &mut BTreeSet<FuncIdx>,
    num_funcs: usize,
) -> Result<Vec<Global>, ValidationError> {
    assert_eq!(section_header.ty, SectionTy::Global);
    wasm.read_vec(|wasm| {
        let ty = GlobalType::read(wasm)?;
        let stack = &mut ValidationStack::new();
        let (init_expr, seen_func_idxs) =
            read_constant_expression(wasm, stack, imported_global_types, num_funcs)?;
        stack.assert_val_types(&[ty.ty], true)?;
        validation_context_refs.extend(seen_func_idxs);
        Ok(Global { ty, init_expr })
    })
}
