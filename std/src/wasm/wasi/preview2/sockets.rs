use crate::rust_alloc::{vec, vec::Vec};
use crate::wasm::execution::{
    config::Config,
    store::{HaltExecutionError, Store},
    value::Value,
};

pub fn adapter_close_badfd<T: Config>(_: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    Ok(vec![Value::I32(0)])
}
