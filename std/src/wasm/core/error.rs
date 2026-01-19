use super::indices::{DataIdx, ElemIdx, FuncIdx, LabelIdx, LocalIdx, MemIdx, TableIdx, TypeIdx};
use crate::wasm::core::indices::GlobalIdx;
use crate::wasm::core::reader::section_header::SectionTy;
use crate::wasm::core::reader::types::ValType;
use crate::wasm::validation::validation_stack::ValidationStackEntry;
use crate::wasm::RefType;
use core::fmt::{Display, Formatter};
use core::str::Utf8Error;
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ValidationError {
    InvalidMagic,
    InvalidBinaryFormatVersion,
    Eof,
    MalformedUtf8(Utf8Error),
    MalformedSectionTypeDiscriminator(u8),
    MalformedNumTypeDiscriminator(u8),
    MalformedVecTypeDiscriminator(u8),
    MalformedFuncTypeDiscriminator(u8),
    MalformedRefTypeDiscriminator(u8),
    MalformedValType,
    MalformedExportDescDiscriminator(u8),
    MalformedImportDescDiscriminator(u8),
    MalformedLimitsDiscriminator(u8),
    MalformedLimitsMinLargerThanMax {
        min: u32,
        max: u32,
    },
    MalformedMutDiscriminator(u8),
    MalformedBlockTypeTypeIdx(i64),
    MalformedVariableLengthInteger,
    MalformedElemKindDiscriminator(u8),
    InvalidTypeIdx(TypeIdx),
    InvalidFuncIdx(FuncIdx),
    InvalidTableIdx(TableIdx),
    InvalidMemIndex(MemIdx),
    InvalidGlobalIdx(GlobalIdx),
    InvalidElemIdx(ElemIdx),
    InvalidDataIdx(DataIdx),
    InvalidLocalIdx(LocalIdx),
    InvalidLabelIdx(LabelIdx),
    InvalidLaneIdx(u8),
    SectionOutOfOrder(SectionTy),
    InvalidCustomSectionLength,
    ExprMissingEnd,
    InvalidInstr(u8),
    InvalidMultiByteInstr(u8, u32),
    EndInvalidValueStack,
    InvalidValidationStackValType(Option<ValType>),
    InvalidValidationStackType(ValidationStackEntry),
    ExpectedAnOperand,
    MemoryTooLarge,
    MutationOfConstGlobal,
    ErroneousAlignment {
        alignment: u32,
        minimum_required_alignment: u32,
    },
    ValidationCtrlStackEmpty,
    ElseWithoutMatchingIf,
    IfWithoutMatchingElse,
    MismatchedRefTypesDuringTableInit {
        table_ty: RefType,
        elem_ty: RefType,
    },
    MismatchedRefTypesDuringTableCopy {
        source_table_ty: RefType,
        destination_table_ty: RefType,
    },
    MismatchedRefTypesOnValidationStack {
        expected: RefType,
        actual: RefType,
    },
    IndirectCallToNonFuncRefTable(RefType),
    ExpectedReferenceTypeOnStack(ValType),
    ReferencingAnUnreferencedFunction(FuncIdx),
    InvalidSelectTypeVectorLength(usize),
    TooManyLocals(u64),
    DuplicateExportName,
    UnsupportedMultipleMemoriesProposal,
    CodeExprHasTrailingInstructions,
    FunctionAndCodeSectionsHaveDifferentLengths,
    DataCountAndDataSectionsLengthAreDifferent,
    InvalidImportType,
    InvalidStartFunctionSignature,
    ActiveElementSegmentTypeMismatch,
    I33IsNegative,
    Component(crate::wasm::component::error::ComponentError),
}
impl Display for ValidationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            ValidationError::Component(e) => write!(f, "Component Error: {:?}", e),
            ValidationError::InvalidMagic => write!(f, "The magic number is invalid"),
            ValidationError::InvalidBinaryFormatVersion => write!(f, "The Wasm binary format version is invalid"),
            ValidationError::Eof => write!(f, "The end of the Wasm bytecode was reached unexpectedly"),
            ValidationError::MalformedUtf8(utf8_error) => write!(f, "Failed to parse a UTF-8 string: {utf8_error}"),
            ValidationError::MalformedSectionTypeDiscriminator(byte) => write!(f, "Failed to parse {byte:#x} as a section type discriminator"),
            ValidationError::MalformedNumTypeDiscriminator(byte) => write!(f, "Failed to parse {byte:#x} as a number type discriminator"),
            ValidationError::MalformedVecTypeDiscriminator(byte) => write!(f, "Failed to parse {byte:#x} as a vector type discriminator"),
            ValidationError::MalformedFuncTypeDiscriminator(byte) => write!(f, "Failed to parse {byte:#x} as a function type discriminator"),
            ValidationError::MalformedRefTypeDiscriminator(byte) => write!(f, "Failed to parse {byte:#x} as a reference type discriminator"),
            ValidationError::MalformedValType => write!(f, "Failed to read a value type because it is neither a number, reference or vector type"),
            ValidationError::MalformedExportDescDiscriminator(byte) => write!(f, "Failed to parse {byte:#x} as an export description discriminator"),
            ValidationError::MalformedImportDescDiscriminator(byte) => write!(f, "Failed to parse {byte:#x} as an import description discriminator"),
            ValidationError::MalformedLimitsDiscriminator(byte) => write!(f, "Failed to parse {byte:#x} as a limits type discriminator"),
            ValidationError::MalformedLimitsMinLargerThanMax { min, max } => write!(f, "Limits are malformed because min={min} is larger than max={max}"),
            ValidationError::MalformedMutDiscriminator(byte) => write!(f, "Failed to parse {byte:#x} as a mute type discriminator"),
            ValidationError::MalformedBlockTypeTypeIdx(idx) => write!(f, "The type index {idx} which is encoded as a singed 33-bit integer inside a block type is malformed"),
            ValidationError::MalformedVariableLengthInteger => write!(f, "Reading a variable-length integer overflowed"),
            ValidationError::MalformedElemKindDiscriminator(byte) => write!(f, "Failed to parse {byte:#x} as an element kind discriminator"),
            ValidationError::InvalidTypeIdx(idx) => write!(f, "The type index {idx} is invalid"),
            ValidationError::InvalidFuncIdx(idx) => write!(f, "The function index {idx} is invalid"),
            ValidationError::InvalidTableIdx(idx) => write!(f, "The table index {idx} is invalid"),
            ValidationError::InvalidMemIndex(idx) => write!(f, "The memory index {idx} is invalid"),
            ValidationError::InvalidGlobalIdx(idx) => write!(f, "The global index {idx} is invalid"),
            ValidationError::InvalidElemIdx(idx) => write!(f, "The element segment index {idx} is invalid"),
            ValidationError::InvalidDataIdx(idx) => write!(f, "The data segment index {idx} is invalid"),
            ValidationError::InvalidLocalIdx(idx) => write!(f, "The local index {idx} is invalid"),
            ValidationError::InvalidLabelIdx(idx) => write!(f, "The label index {idx} is invalid"),
            ValidationError::InvalidLaneIdx(idx) => write!(f, "The lane index {idx} is invalid"),
            ValidationError::SectionOutOfOrder(ty) => write!(f, "A section of type `{ty:?}` is defined out of order"),
            ValidationError::InvalidCustomSectionLength => write!(f, "A custom section contains more bytes than its section header specifies"),
            ValidationError::ExprMissingEnd => write!(f, "An expr type is missing an end byte"),
            ValidationError::InvalidInstr(byte) => write!(f, "The instruction opcode {byte:#x} is invalid"),
            ValidationError::InvalidMultiByteInstr(first_byte, second_instr) => write!(f, "The multi-byte instruction opcode {first_byte:#x} {second_instr} is invalid"),
            ValidationError::ActiveElementSegmentTypeMismatch => write!(f, "an element segment's type and its table's type are different"),
            ValidationError::EndInvalidValueStack => write!(f, "Different value stack types were expected at the end of a block/function"),
            ValidationError::InvalidValidationStackValType(ty) => write!(f, "An unexpected type `{ty:?}` was found on the stack when trying to pop another"),
            ValidationError::InvalidValidationStackType(ty) => write!(f, "An unexpected type `{ty:?}` was found on the stack"),
            ValidationError::ExpectedAnOperand => write!(f, "Expected a value type operand on the stack"),
            ValidationError::MemoryTooLarge => write!(f, "The size specified by a memory type exceeds the maximum size"),
            ValidationError::MutationOfConstGlobal => write!(f, "An attempt has been made to mutate a const global"),
            ValidationError::ErroneousAlignment { alignment, minimum_required_alignment } => write!(f, "The alignment 2^{alignment} is not less or equal to the required alignment 2^{minimum_required_alignment}"),
            ValidationError::ValidationCtrlStackEmpty => write!(f, "Failed to retrieve last ctrl block because validation ctrl stack is empty"),
            ValidationError::ElseWithoutMatchingIf => write!(f, "Found `else` without a previous matching `if` instruction"),
            ValidationError::IfWithoutMatchingElse => write!(f, "Found `end` without a previous matching `else` to an `if` instruction"),
            ValidationError::MismatchedRefTypesDuringTableInit { table_ty, elem_ty } => write!(f, "Mismatch of table type `{table_ty:?}` and element segment type `{elem_ty:?}` for `table.init` instruction"),
            ValidationError::MismatchedRefTypesDuringTableCopy { source_table_ty, destination_table_ty } => write!(f, "Mismatch of source table type `{source_table_ty:?}` and destination table type `{destination_table_ty:?}` for `table.copy` instruction"),
            ValidationError::MismatchedRefTypesOnValidationStack { expected, actual } => write!(f, "Mismatch of reference types on the value stack: Expected `{expected:?}` but got `{actual:?}`"),
            ValidationError::IndirectCallToNonFuncRefTable(table_ty) => write!(f, "An indirect call to a table which does not store function references but instead `{table_ty:?}` was made"),
            ValidationError::ExpectedReferenceTypeOnStack(found_valtype) => write!(f, "Expected a reference type but instead found a `{found_valtype:?}` on the stack"),
            ValidationError::ReferencingAnUnreferencedFunction(func_idx) => write!(f, "Referenced a function with index {func_idx} that was not referenced in prior validation"),
            ValidationError::InvalidSelectTypeVectorLength(len) => write!(f, "The type vector of a `select` instruction must be of length 1 as of now but it is of length {len} instead"),
            ValidationError::TooManyLocals(n) => write!(f, "There are {n} locals and this exceeds the maximum allowed number of 2^32-1"),
            ValidationError::DuplicateExportName => write!(f, "Multiple exports share the same name"),
            ValidationError::UnsupportedMultipleMemoriesProposal => write!(f, "A memory index other than 1 was used, but the proposal for multiple memories is not yet supported"),
            ValidationError::CodeExprHasTrailingInstructions => write!(f, "A code expression has invalid trailing instructions following its `end` instruction"),
            ValidationError::FunctionAndCodeSectionsHaveDifferentLengths => write!(f, "The function and code sections have different lengths"),
            ValidationError::DataCountAndDataSectionsLengthAreDifferent => write!(f, "The data count section specifies a different length than there are data segments in the data section"),
            ValidationError::InvalidImportType => f.write_str("Invalid import type"),
            ValidationError::InvalidStartFunctionSignature => write!(f, "The start function has parameters or return types which it is not allowed to have"),
            ValidationError::I33IsNegative => f.write_str("An i33 type is negative which is not allowed")
        }
    }
}
impl ValidationError {
    /// Convert this error to a message that is compatible with the error messages used by the official Wasm testsuite.
    pub fn to_message(&self) -> &'static str {
        todo!("convert validation error to testsuite message");
    }
}
#[cfg(test)]
mod test {
    use crate::rust_alloc::string::ToString;
    use crate::wasm::core::error::ValidationError;
    #[test]
    fn fmt_invalid_magic() {
        assert!(ValidationError::InvalidMagic
            .to_string()
            .contains("magic number"));
    }
    #[test]
    fn fmt_invalid_version() {
        assert!(ValidationError::InvalidBinaryFormatVersion
            .to_string()
            .contains("version"));
    }
}