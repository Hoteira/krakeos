use crate::rust_alloc::string::String;
use crate::rust_alloc::vec::Vec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValType {
    I32,
    I64,
    F32,
    F64,
    V128,
    FuncRef,
    ExternRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FuncType {
    pub params: Vec<ValType>,
    pub results: Vec<ValType>,
}

#[derive(Debug, Clone)]
pub struct Module {
    pub types: Vec<FuncType>,
    pub imports: Vec<Import>,
    pub functions: Vec<u32>, // Index into types
    pub tables: Vec<TableType>,
    pub memories: Vec<MemoryType>,
    pub globals: Vec<Global>,
    pub exports: Vec<Export>,
    pub start: Option<u32>,
    pub elements: Vec<Element>,
    pub code: Vec<FunctionBody>,
    pub data: Vec<DataSegment>,
}

#[derive(Debug, Clone)]
pub struct Import {
    pub module: String,
    pub name: String,
    pub desc: ImportDesc,
}

#[derive(Debug, Clone)]
pub enum ImportDesc {
    Func(u32), // Type index
    Table(TableType),
    Memory(MemoryType),
    Global(GlobalType),
}

#[derive(Debug, Clone, Copy)]
pub struct TableType {
    pub element_type: ValType,
    pub limits: Limits,
}

#[derive(Debug, Clone, Copy)]
pub struct MemoryType {
    pub limits: Limits,
}

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub min: u32,
    pub max: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct Global {
    pub ty: GlobalType,
    pub init: Expr,
}

#[derive(Debug, Clone, Copy)]
pub struct GlobalType {
    pub content_type: ValType,
    pub mutability: bool,
}

#[derive(Debug, Clone)]
pub struct Export {
    pub name: String,
    pub desc: ExportDesc,
}

#[derive(Debug, Clone, Copy)]
pub enum ExportDesc {
    Func(u32),
    Table(u32),
    Memory(u32),
    Global(u32),
}

#[derive(Debug, Clone)]
pub struct Element {
    pub table_index: u32,
    pub offset: Expr,
    pub init: Vec<u32>, // Func indices
}

#[derive(Debug, Clone)]
pub struct FunctionBody {
    pub locals: Vec<Local>,
    pub code: Vec<u8>, // Raw bytecode for now, or parsed instructions
}

#[derive(Debug, Clone)]
pub struct Local {
    pub count: u32,
    pub ty: ValType,
}

#[derive(Debug, Clone)]
pub struct DataSegment {
    pub memory_index: u32,
    pub offset: Option<Expr>, // None for passive
    pub init: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct Expr {
    pub instructions: Vec<u8>, // Simplification for initialization expressions
}