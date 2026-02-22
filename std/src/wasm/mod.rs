#[macro_use]
mod log_wrapper;
pub mod aot;
pub mod common;
pub mod component;
pub mod interpreter;
pub mod runner;
pub mod wasi;

pub use runner::{run, run_with_args, run_with_env};

pub use common::error::ValidationError;
pub use common::interop::Linker;
pub use common::reader::span::Span;
pub use common::reader::types::{Limits, NumType, RefType, ValType};
pub use common::runtime_error::{RuntimeError, TrapError};
pub use common::validation::{ValidationInfo, validate};
pub use common::value::Value;
pub use interpreter::resumable::ResumableRef;
pub use interpreter::store::{ExternVal, Store};
