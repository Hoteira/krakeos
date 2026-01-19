#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComponentError {
    InvalidMagic,
    InvalidVersion,
    UnexpectedEof,
    MalformedSectionId(u8),
    MalformedVarU32,
    MalformedUtf8,
    UnimplementedSection(u8),
}
