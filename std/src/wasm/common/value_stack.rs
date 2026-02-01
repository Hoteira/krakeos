use crate::rust_alloc::vec::{Drain, Vec};
use crate::wasm::common::indices::LocalIdx;
use crate::wasm::common::reader::types::{FuncType, ValType};
use crate::wasm::common::assert_validated::UnwrapValidatedExt;
use crate::wasm::common::config::Config;
use crate::wasm::interpreter::store::addrs::FuncAddr;
use crate::wasm::common::value::Value;
use crate::wasm::common::runtime_error::RuntimeError;

#[derive(Default, Debug)]
pub struct Stack {
    values: Vec<Value>,
    pub(crate) frames: Vec<CallFrame>,
}

impl Stack {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn new_with_values(values: Vec<Value>) -> Self {
        Self {
            values,
            ..Self::default()
        }
    }
    pub(crate) fn into_values(self) -> Vec<Value> {
        self.values
    }
    pub fn len(&self) -> usize {
        self.values.len()
    }
    pub fn pop_value(&mut self) -> Value {
        debug_assert!(
            if !self.frames.is_empty() {
                self.values.len() > self.current_call_frame().value_stack_base_idx
            } else {
                true
            },
            "can not pop values past the current call frame"
        );
        self.values.pop().unwrap_validated()
    }
    pub fn peek_value(&self) -> Option<Value> {
        self.values.last().copied()
    }
    pub fn push_value<C: Config>(&mut self, value: Value) -> Result<(), RuntimeError> {
        if self.values.len() > C::MAX_VALUE_STACK_SIZE {
            return Err(RuntimeError::StackExhaustion);
        }
        self.values.push(value);
        Ok(())
    }
    pub fn get_local(&self, idx: LocalIdx) -> &Value {
        let call_frame_base_idx = self.current_call_frame().call_frame_base_idx;
        self.values
            .get(call_frame_base_idx + idx)
            .unwrap_validated()
    }
    pub fn get_local_mut(&mut self, idx: LocalIdx) -> &mut Value {
        let call_frame_base_idx = self.current_call_frame().call_frame_base_idx;
        self.values
            .get_mut(call_frame_base_idx + idx)
            .unwrap_validated()
    }
    pub fn current_call_frame(&self) -> &CallFrame {
        self.frames.last().unwrap_validated()
    }
    pub fn pop_call_frame(&mut self) -> (FuncAddr, usize, usize) {
        let CallFrame {
            return_func_addr,
            return_addr,
            call_frame_base_idx,
            return_value_count,
            return_stp,
            ..
        } = self.frames.pop().unwrap_validated();
        let remove_count = self.values.len() - call_frame_base_idx - return_value_count;
        self.remove_in_between(remove_count, return_value_count);
        debug_assert_eq!(
            self.values.len(),
            call_frame_base_idx + return_value_count,
            "after a function call finished, the stack must have exactly as many values as it had before calling the function plus the number of function return values"
        );
        (return_func_addr, return_addr, return_stp)
    }
    pub fn push_call_frame<C: Config>(
        &mut self,
        return_func_addr: FuncAddr,
        func_ty: &FuncType,
        remaining_locals: &[ValType],
        return_addr: usize,
        return_stp: usize,
    ) -> Result<(), RuntimeError> {
        if self.call_frame_count() > C::MAX_CALL_STACK_SIZE {
            return Err(RuntimeError::StackExhaustion);
        }
        debug_assert!(
            self.values.len() >= func_ty.params.valtypes.len(),
            "when pushing a new call frame, at least as many values need to be on the stack as required by the new call frames's function"
        );
        let param_count = func_ty.params.valtypes.len();
        let call_frame_base_idx = self.values.len() - param_count;
        for local in remaining_locals {
            self.values.push(Value::default_from_ty(*local));
        }
        let value_stack_base_idx = self.values.len();
        self.frames.push(CallFrame {
            return_func_addr,
            return_addr,
            value_stack_base_idx,
            call_frame_base_idx,
            return_value_count: func_ty.returns.valtypes.len(),
            return_stp,
        });
        Ok(())
    }
    pub fn call_frame_count(&self) -> usize {
        self.frames.len()
    }
    pub fn pop_tail_iter(&mut self, n: usize) -> Drain<'_, Value> {
        let start = self.values.len() - n;
        self.values.drain(start..)
    }
    pub fn remove_in_between(&mut self, remove_count: usize, keep_count: usize) {
        let len = self.values.len();
        self.values
            .copy_within(len - keep_count.., len - keep_count - remove_count);
        self.values.truncate(len - remove_count);
    }
}

#[derive(Debug)]
pub(crate) struct CallFrame {
    pub return_func_addr: FuncAddr,
    pub return_addr: usize,
    pub value_stack_base_idx: usize,
    pub call_frame_base_idx: usize,
    pub return_value_count: usize,
    pub return_stp: usize,
}