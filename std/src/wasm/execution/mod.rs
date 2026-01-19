use crate::rust_alloc::vec::Vec;
use crate::wasm::execution::assert_validated::UnwrapValidatedExt;
use crate::wasm::execution::interop::InteropValueList;
use crate::wasm::execution::value::Value;
use const_interpreter_loop::run_const_span;
use store::HaltExecutionError;
pub(crate) mod assert_validated;
pub mod checked;
pub mod config;
pub mod const_interpreter_loop;
pub mod error;
pub mod interop;
mod interpreter_loop;
pub mod linker;
pub(crate) mod little_endian;
pub mod resumable;
pub mod store;
pub mod value;
pub mod component_executor;
pub mod simd_utils;
pub mod simd_instructions;
pub use self::value_stack::Stack;
pub use interpreter_loop::run;
pub mod value_stack;
pub fn host_function_wrapper<Params: InteropValueList, Results: InteropValueList>(
    params: Vec<Value>,
    f: impl FnOnce(Params) -> Result<Results, HaltExecutionError>,
) -> Result<Vec<Value>, HaltExecutionError> {
    let params =
        Params::try_from_values(params.into_iter()).expect("Params match the actual parameters");
    f(params).map(Results::into_values)
}
