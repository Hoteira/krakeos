use rust_alloc::vec;
use crate::rust_alloc::vec::Vec;
use crate::wasm::{
    common::{config::Config, value::Value},
    interpreter::store::{HaltExecutionError, Store},
};

pub fn adapter_close_badfd<T: Config>(_: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    // Return EBADF (76)
    Ok(vec![Value::I32(76)])
}
