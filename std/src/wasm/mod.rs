#![no_std]

#[macro_use]
pub(crate) mod log_wrapper;

pub use self::core::error::ValidationError;
pub use self::core::reader::types::{
    export::ExportDesc, global::GlobalType, Limits, NumType, RefType, ValType,
};
pub use self::core::rw_spinlock;
pub use execution::error::{RuntimeError, TrapError};

pub use execution::linker::Linker;
pub use execution::store::*;
pub use execution::value::Value;
// Export Linker
pub use execution::*;
pub use validation::*;

pub(crate) mod core;
pub(crate) mod execution;
pub(crate) mod validation;

pub mod wasi;

/// A definition for a [`Result`] using the optional [`Error`] type.
pub type Result<T> = ::core::result::Result<T, Error>;

/// An opt-in error type useful for merging all error types of this crate into a single type.
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Validation(ValidationError),
    RuntimeError(RuntimeError),
}

impl From<ValidationError> for Error {
    fn from(value: ValidationError) -> Self {
        Self::Validation(value)
    }
}

impl From<RuntimeError> for Error {
    fn from(value: RuntimeError) -> Self {
        Self::RuntimeError(value)
    }
}
