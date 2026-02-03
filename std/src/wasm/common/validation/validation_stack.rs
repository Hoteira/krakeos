use crate::rust_alloc::vec;
use crate::rust_alloc::vec::Vec;
use crate::wasm::common::error::ValidationError;
use crate::wasm::common::reader::types::{FuncType, ResultType};
use crate::wasm::common::reader::types::{NumType, RefType, ValType};
use core::iter;

#[derive(Debug, PartialEq, Eq)]
pub struct ValidationStack {
    stack: Vec<ValidationStackEntry>,
    pub ctrl_stack: Vec<CtrlStackEntry>,
}

impl ValidationStack {
    pub fn new() -> Self {
        Self {
            stack: Vec::new(),
            ctrl_stack: vec![CtrlStackEntry {
                label_info: LabelInfo::Untyped,
                block_ty: FuncType {
                    params: ResultType {
                        valtypes: Vec::new(),
                    },
                    returns: ResultType {
                        valtypes: Vec::new(),
                    },
                },
                height: 0,
                unreachable: false,
            }],
        }
    }
    pub(super) fn new_for_func(block_ty: FuncType) -> Self {
        Self {
            stack: Vec::new(),
            ctrl_stack: vec![CtrlStackEntry {
                label_info: LabelInfo::Func {
                    stps_to_backpatch: Vec::new(),
                },
                block_ty,
                height: 0,
                unreachable: false,
            }],
        }
    }
    pub fn len(&self) -> usize {
        self.stack.len()
    }
    pub fn push_valtype(&mut self, valtype: ValType) {
        self.stack.push(ValidationStackEntry::Val(valtype));
    }
    pub(super) fn drop_val(&mut self) -> Result<(), ValidationError> {
        self.pop_valtype()
            .map_err(|_| ValidationError::ExpectedAnOperand)?;
        Ok(())
    }
    pub(super) fn make_unspecified(&mut self) -> Result<(), ValidationError> {
        let last_ctrl_stack_entry = self
            .ctrl_stack
            .last_mut()
            .ok_or(ValidationError::ValidationCtrlStackEmpty)?;
        last_ctrl_stack_entry.unreachable = true;
        self.stack.truncate(last_ctrl_stack_entry.height);
        Ok(())
    }
    fn pop_valtype(&mut self) -> Result<ValidationStackEntry, ValidationError> {
        let last_ctrl_stack_entry = self.ctrl_stack.last().unwrap();
        assert!(self.stack.len() >= last_ctrl_stack_entry.height);
        if last_ctrl_stack_entry.height == self.stack.len() {
            if last_ctrl_stack_entry.unreachable {
                Ok(ValidationStackEntry::Bottom)
            } else {
                Err(ValidationError::EndInvalidValueStack)
            }
        } else {
            self.stack
                .pop()
                .ok_or(ValidationError::EndInvalidValueStack)
        }
    }
    pub fn assert_pop_ref_type(
        &mut self,
        expected_ty: Option<RefType>,
    ) -> Result<(), ValidationError> {
        match self.pop_valtype()? {
            ValidationStackEntry::Val(ValType::RefType(ref_type)) => {
                expected_ty.map_or(Ok(()), |ty| {
                    (ty == ref_type).then_some(()).ok_or(
                        ValidationError::MismatchedRefTypesOnValidationStack {
                            expected: ty,
                            actual: ref_type,
                        },
                    )
                })
            }
            ValidationStackEntry::Val(v) => Err(ValidationError::ExpectedReferenceTypeOnStack(v)),
            ValidationStackEntry::Bottom => Ok(()),
        }
    }
    pub fn assert_pop_val_type(&mut self, expected_ty: ValType) -> Result<(), ValidationError> {
        let actual = self.pop_valtype()?;
        if actual.unifies_to(&ValidationStackEntry::Val(expected_ty)) {
            Ok(())
        } else {
            match actual {
                ValidationStackEntry::Val(ty) => {
                    crate::debugln!("Validation type mismatch: expected {:?}, found {:?}", expected_ty, ty);
                    Err(ValidationError::InvalidValidationStackValType(Some(ty)))
                }
                ValidationStackEntry::Bottom => unreachable!(),
            }
        }
    }
    fn assert_val_types_on_top_with_custom_stacks(
        stack: &mut Vec<ValidationStackEntry>,
        ctrl_stack: &[CtrlStackEntry],
        expected_val_types: &[ValType],
        unify_to_expected_types: bool,
    ) -> Result<(), ValidationError> {
        let last_ctrl_stack_entry = ctrl_stack
            .last()
            .ok_or(ValidationError::ValidationCtrlStackEmpty)?;
        let stack_len = stack.len();
        let rev_iterator = expected_val_types.iter().rev().enumerate();
        for (i, expected_ty) in rev_iterator {
            if stack_len - last_ctrl_stack_entry.height <= i {
                if last_ctrl_stack_entry.unreachable {
                    if unify_to_expected_types {
                        stack.splice(
                            stack_len - i..stack_len - i,
                            expected_val_types[..expected_val_types.len() - i]
                                .iter()
                                .map(|ty| ValidationStackEntry::Val(*ty)),
                        );
                    } else {
                        stack.splice(
                            stack_len - i..stack_len - i,
                            iter::repeat(ValidationStackEntry::Bottom)
                                .take(expected_val_types.len() - i),
                        );
                    }
                    return Ok(());
                } else {
                    crate::debugln!("EndInvalidValueStack: stack_len={}, height={}, i={}", stack_len, last_ctrl_stack_entry.height, i);
                    crate::debugln!("Stack top: {:?}", &stack[last_ctrl_stack_entry.height..]);
                    return Err(ValidationError::EndInvalidValueStack);
                }
            }
            let actual_entry = &mut stack[stack_len - i - 1];
            if !actual_entry.unifies_to(&ValidationStackEntry::Val(*expected_ty)) {
                return Err(ValidationError::EndInvalidValueStack);
            }
            if unify_to_expected_types && matches!(actual_entry, ValidationStackEntry::Bottom) {
                *actual_entry = ValidationStackEntry::Val(*expected_ty);
            }
        }
        Ok(())
    }
    fn assert_val_types_with_custom_stacks(
        stack: &mut Vec<ValidationStackEntry>,
        ctrl_stack: &[CtrlStackEntry],
        expected_val_types: &[ValType],
        unify_to_expected_types: bool,
    ) -> Result<(), ValidationError> {
        ValidationStack::assert_val_types_on_top_with_custom_stacks(
            stack,
            ctrl_stack,
            expected_val_types,
            unify_to_expected_types,
        )?;
        let last_ctrl_stack_entry = &ctrl_stack[ctrl_stack.len() - 1];
        if stack.len() == last_ctrl_stack_entry.height + expected_val_types.len() {
            Ok(())
        } else {
            Err(ValidationError::EndInvalidValueStack)
        }
    }
    pub(super) fn assert_val_types_on_top(
        &mut self,
        expected_val_types: &[ValType],
        unify_to_expected_types: bool,
    ) -> Result<(), ValidationError> {
        ValidationStack::assert_val_types_on_top_with_custom_stacks(
            &mut self.stack,
            &self.ctrl_stack,
            expected_val_types,
            unify_to_expected_types,
        )
    }
    pub fn assert_val_types(
        &mut self,
        expected_val_types: &[ValType],
        unify_to_expected_types: bool,
    ) -> Result<(), ValidationError> {
        ValidationStack::assert_val_types_with_custom_stacks(
            &mut self.stack,
            &self.ctrl_stack,
            expected_val_types,
            unify_to_expected_types,
        )
    }
    pub fn assert_val_types_of_label_jump_types_on_top(
        &mut self,
        label_idx: usize,
        unify_to_expected_types: bool,
    ) -> Result<(), ValidationError> {
        let index_of_label_in_ctrl_stack = self
            .ctrl_stack
            .len()
            .checked_sub(label_idx)
            .and_then(|i| i.checked_sub(1));
        let label_types = index_of_label_in_ctrl_stack
            .and_then(|index_of_label_in_ctrl_stack| {
                self.ctrl_stack.get(index_of_label_in_ctrl_stack)
            })
            .ok_or(ValidationError::InvalidLabelIdx(label_idx))?
            .label_types();
        ValidationStack::assert_val_types_on_top_with_custom_stacks(
            &mut self.stack,
            &self.ctrl_stack,
            label_types,
            unify_to_expected_types,
        )
    }
    pub fn assert_push_ctrl(
        &mut self,
        label_info: LabelInfo,
        block_ty: FuncType,
        unify_to_expected_types: bool,
    ) -> Result<(), ValidationError> {
        self.assert_val_types_on_top(&block_ty.params.valtypes, unify_to_expected_types)?;
        let height = self.stack.len() - block_ty.params.valtypes.len();
        self.ctrl_stack.push(CtrlStackEntry {
            label_info,
            block_ty,
            height,
            unreachable: false,
        });
        Ok(())
    }
    pub fn assert_pop_ctrl(
        &mut self,
        unify_to_expected_types: bool,
    ) -> Result<(LabelInfo, FuncType), ValidationError> {
        let return_types = &self
            .ctrl_stack
            .last()
            .ok_or(ValidationError::ValidationCtrlStackEmpty)?
            .block_ty
            .returns
            .valtypes;
        ValidationStack::assert_val_types_with_custom_stacks(
            &mut self.stack,
            &self.ctrl_stack,
            return_types,
            unify_to_expected_types,
        )?;
        let last_ctrl_stack_entry = self.ctrl_stack.pop().unwrap();
        Ok((
            last_ctrl_stack_entry.label_info,
            last_ctrl_stack_entry.block_ty,
        ))
    }
    pub fn validate_polymorphic_select(&mut self) -> Result<(), ValidationError> {
        self.assert_pop_val_type(ValType::NumType(NumType::I32))?;
        let first_arg = self.pop_valtype()?;
        let second_arg = self.pop_valtype()?;
        let unified_type = second_arg
            .unify(&first_arg)
            .ok_or(ValidationError::InvalidValidationStackValType(None))?;
        if !(unified_type.unifies_to(&ValidationStackEntry::Val(ValType::NumType(NumType::I32)))
            || unified_type.unifies_to(&ValidationStackEntry::Val(ValType::NumType(NumType::F32)))
            || unified_type.unifies_to(&ValidationStackEntry::Val(ValType::NumType(NumType::I64)))
            || unified_type.unifies_to(&ValidationStackEntry::Val(ValType::NumType(NumType::F64)))
            || unified_type.unifies_to(&ValidationStackEntry::Val(ValType::VecType)))
        {
            return Err(ValidationError::InvalidValidationStackValType(None));
        }
        self.stack.push(unified_type);
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValidationStackEntry {
    Val(ValType),
    Bottom,
}

impl ValidationStackEntry {
    fn unifies_to(&self, other: &ValidationStackEntry) -> bool {
        match (self, other) {
            (ValidationStackEntry::Bottom, _) => true,
            (_, ValidationStackEntry::Bottom) => true,
            (ValidationStackEntry::Val(v1), ValidationStackEntry::Val(v2)) => {
                if v1 == v2 { return true; }
                // Treat all numeric types as interchangeable for our unified slot ABI
                match (v1, v2) {
                    (ValType::NumType(_), ValType::NumType(_)) => true,
                    _ => false,
                }
            }
        }
    }
    fn unify(&self, other: &ValidationStackEntry) -> Option<Self> {
        if self.unifies_to(other) {
            if matches!(self, ValidationStackEntry::Bottom) { Some(other.clone()) }
            else { Some(self.clone()) }
        } else {
            None
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtrlStackEntry {
    pub label_info: LabelInfo,
    pub block_ty: FuncType,
    pub height: usize,
    pub unreachable: bool,
}

impl CtrlStackEntry {
    pub fn label_types(&self) -> &[ValType] {
        if matches!(self.label_info, LabelInfo::Loop { .. }) {
            &self.block_ty.params.valtypes
        } else {
            &self.block_ty.returns.valtypes
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LabelInfo {
    Block {
        stps_to_backpatch: Vec<usize>,
    },
    Loop {
        ip: usize,
        stp: usize,
    },
    If {
        stps_to_backpatch: Vec<usize>,
        stp: usize,
    },
    Func {
        stps_to_backpatch: Vec<usize>,
    },
    Untyped,
}