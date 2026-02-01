use crate::rust_alloc::string::String;
use core::fmt::{Display, Formatter};

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum RuntimeError {
    Trap(TrapError),
    ModuleNotFound,
    FunctionNotFound,
    ResumableNotFound,
    StackExhaustion,
    HostFunctionSignatureMismatch,
    WriteOnImmutableGlobal,
    GlobalTypeMismatch,
    HostFunctionHaltedExecution(i32),
    TableAccessOutOfBounds,
    UnknownExport,
    TableTypeMismatch,
    StoreIdMismatch,
    InvalidImportType,
    UnknownImport,
    RegistrySymbolAlreadyExists,
    MoreThanOneMemory,
    OutOfFuel,
    ExternValsLenMismatch,
    DuplicateExternDefinition,
    UnableToResolveExternLookup { module: String, name: String },
    ValidationError,
    FunctionInvocationSignatureMismatch,
    LinkerNotYetAssociatedWithStoreId,
}

impl Display for RuntimeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            RuntimeError::Trap(trap_error) => write!(f, "{trap_error}"),
            RuntimeError::FunctionNotFound => f.write_str("Function not found"),
            RuntimeError::ModuleNotFound => f.write_str("No such module exists"),
            RuntimeError::ResumableNotFound => f.write_str("No such resumable exists"),
            RuntimeError::StackExhaustion => {
                f.write_str("either the call stack or the value stack overflowed")
            }
            RuntimeError::HostFunctionSignatureMismatch => {
                f.write_str("host function call did not respect its type signature")
            }
            RuntimeError::HostFunctionHaltedExecution(code) => {
                write!(f, "A host function requested execution to be halted with code {}.", code)
            }
            RuntimeError::InvalidImportType => f.write_str("Invalid import type"),
            RuntimeError::TableAccessOutOfBounds => f.write_str("A table access was out of bounds"),
            RuntimeError::RegistrySymbolAlreadyExists => f.write_str(
                "It was attempted to register a symbol under a name for which a symbol already exists.",
            ),
            RuntimeError::UnknownExport => {
                f.write_str("An unknown export was referenced by its name.")
            }
            RuntimeError::TableTypeMismatch => {
                f.write_str("An alloc/write operation failed on a table due to a type mismatch.")
            }
            RuntimeError::UnknownImport => f.write_str("Unknown Import"),
            RuntimeError::MoreThanOneMemory => {
                f.write_str("As of not only one memory is allowed per module.")
            }
            RuntimeError::WriteOnImmutableGlobal => f.write_str(
                "A write operation on a global failed due to the global being immutable",
            ),
            RuntimeError::GlobalTypeMismatch => {
                f.write_str("An alloc/write operation on a global failed due to a type mismatch")
            }
            RuntimeError::OutOfFuel => {
                f.write_str("Fueled execution that is not resumable has ran out of fuel")
            }
            RuntimeError::ExternValsLenMismatch => {
                f.write_str("The number of module exports did not match the number of extern values provided for instantiation.")
            }
            RuntimeError::DuplicateExternDefinition => {
                f.write_str("Linking failed because of a duplicate definition of some extern value")
            }
            RuntimeError::UnableToResolveExternLookup { module, name } => {
                write!(f, "An extern lookup could not be resolved because no matching extern value existed for it: {module}::{name}")
            }
            RuntimeError::FunctionInvocationSignatureMismatch => {
                f.write_str("A function was invoked with incorrect parameters or return types")
            }
            RuntimeError::StoreIdMismatch => f.write_str("The identifier of a stored object did not match the store it was used with"),
            RuntimeError::LinkerNotYetAssociatedWithStoreId => f.write_str("A checked method of a linker was used, even though that linker has not yet been associated to any store through its id"),
            RuntimeError::ValidationError => f.write_str("Validation Error"),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum TrapError {
    DivideBy0,
    UnrepresentableResult,
    BadConversionToInteger,
    MemoryOrDataAccessOutOfBounds,
    TableOrElementAccessOutOfBounds,
    UninitializedElement,
    SignatureMismatch,
    IndirectCallNullFuncRef,
    TableAccessOutOfBounds,
    ReachedUnreachable,
}

impl Display for TrapError {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            TrapError::DivideBy0 => f.write_str("Divide by zero is not permitted"),
            TrapError::UnrepresentableResult => f.write_str("Result is unrepresentable"),
            TrapError::BadConversionToInteger => f.write_str("Bad conversion to integer"),
            TrapError::MemoryOrDataAccessOutOfBounds => {
                f.write_str("Memory or data access out of bounds")
            }
            TrapError::TableOrElementAccessOutOfBounds => {
                f.write_str("Table or element access out of bounds")
            }
            TrapError::UninitializedElement => f.write_str("Uninitialized element"),
            TrapError::SignatureMismatch => f.write_str("Indirect call signature mismatch"),
            TrapError::IndirectCallNullFuncRef => {
                f.write_str("Indirect call targeted null reference")
            }
            TrapError::TableAccessOutOfBounds => {
                f.write_str("Indirect call: table index out of bounds")
            }
            TrapError::ReachedUnreachable => {
                f.write_str("an unreachable statement was reached, triggered a trap")
            }
        }
    }
}

impl From<TrapError> for RuntimeError {
    fn from(value: TrapError) -> Self {
        Self::Trap(value)
    }
}
