use crate::rust_alloc::collections::btree_set::BTreeSet;
use crate::rust_alloc::vec::Vec;
use crate::wasm::core::error::ValidationError;
use crate::wasm::core::indices::FuncIdx;
use crate::wasm::core::reader::section_header::{SectionHeader, SectionTy};
use crate::wasm::core::reader::types::global::{Global, GlobalType};
use crate::wasm::core::reader::{WasmReadable, WasmReader};
use crate::wasm::validation::read_constant_expression::read_constant_expression;
use crate::wasm::validation::validation_stack::ValidationStack;
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
