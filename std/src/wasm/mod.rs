#[macro_use]
mod log_wrapper;
pub mod common;
pub mod interpreter;
pub mod aot;
pub mod wasi;
pub mod component;
pub mod runner;

pub use runner::{run, run_with_args};

pub use common::validation::{ValidationInfo, validate};
pub use common::error::ValidationError;
pub use common::interop::Linker;
pub use common::runtime_error::{RuntimeError, TrapError};
pub use common::value::Value;
pub use common::reader::span::Span;
pub use common::reader::types::{Limits, NumType, RefType, ValType};
pub use interpreter::store::{Store, ExternVal};
pub use interpreter::resumable::ResumableRef;