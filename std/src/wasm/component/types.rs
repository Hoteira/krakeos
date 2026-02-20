use crate::rust_alloc::borrow::ToOwned;
use crate::rust_alloc::string::String;
use crate::rust_alloc::vec::Vec;
use crate::wasm::common::error::ValidationError;
use crate::wasm::common::reader::span::Span;
use crate::wasm::common::reader::{WasmReadable, WasmReader};

// WASM Component Model Section IDs:
// 0:  Custom
// 1:  Core Module
// 2:  Core Instance
// 3:  Core Alias
// 4:  Core Type
// 5:  Component
// 6:  Alias
// 7:  Type
// 8:  Instance
// 9:  Canon
// 10: Start
// 11: Import
// 12: Export

#[derive(Debug, Clone)]
pub enum ComponentItem {
    Module(ComponentModule),
    CoreInstance(CoreInstance),
    CoreAlias(CoreAlias),
    Instance(ComponentInstance),
    Alias(ComponentAlias),
    Type(ComponentType),
    Canon(ComponentCanon),
    Start(ComponentStart),
    Import(ComponentImport),
    Export(ComponentExport),
    Component(NestedComponent),
}

#[derive(Debug, Default, Clone)]
pub struct ParsedComponent {
    pub items: Vec<ComponentItem>,
}

#[derive(Debug, Clone)]
pub struct CoreInstance {
    pub kind: CoreInstanceKind,
}

#[derive(Debug, Clone)]
pub enum CoreInstanceKind {
    Instantiate { module_idx: u32, args: Vec<CoreInstantiationArg> },
    FromExports { exports: Vec<CoreExport> },
}

#[derive(Debug, Clone)]
pub struct CoreInstantiationArg {
    pub name: String,
    pub kind: u8, // 0x12 = instance
    pub idx: u32,
}

#[derive(Debug, Clone)]
pub struct CoreExport {
    pub name: String,
    pub kind: u8, // sort
    pub idx: u32,
}

#[derive(Debug, Clone)]
pub struct CoreAlias {
    pub sort: u8,
    pub kind: CoreAliasKind,
}

#[derive(Debug, Clone)]
pub enum CoreAliasKind {
    Export { instance_idx: u32, name: String },
    Outer { count: u32, idx: u32 },
}

#[derive(Debug, Clone)]
pub struct NestedComponent {
    pub parsed: ParsedComponent,
    pub content: Span,
}

#[derive(Debug, Clone)]
pub struct ComponentModule {
    pub content: Span,
}

#[derive(Debug, Clone)]
pub struct ComponentInstance {
    pub kind: ComponentInstanceKind,
}

#[derive(Debug, Clone)]
pub enum ComponentInstanceKind {
    InstantiateModule { module_idx: u32, args: Vec<ComponentInstantiationArg> },
    InstantiateComponent { component_idx: u32, args: Vec<ComponentInstantiationArg> },
    FromExports { values: Vec<ComponentExport> },
    FromCoreInstance { core_instance_idx: u32 },
}

#[derive(Debug, Clone)]
pub struct ComponentInstantiationArg {
    pub name: String,
    pub kind: u8, // 0x00=module, 0x01=component, 0x02=instance, 0x03=func, 0x04=value
    pub idx: u32,
}

#[derive(Debug, Clone)]
pub struct ComponentAlias {
    pub sort: u8,
    pub kind: ComponentAliasKind,
}

#[derive(Debug, Clone)]
pub enum ComponentAliasKind {
    InstanceExport { instance_idx: u32, name: String },
    CoreInstanceExport { instance_idx: u32, name: String },
    Outer { count: u32, sort: u8, idx: u32 },
}

#[derive(Debug, Clone)]
pub enum ComponentAliasTarget {
    Module,
    Component,
    Instance,
    Func,
    Value,
    Type,
    Table,
    Memory,
    Global,
}

#[derive(Debug, Clone)]
pub enum ComponentType {
    Defined(DefinedType),
    Func(ComponentFuncType),
    Component(Vec<ComponentTypeDeclaration>),
    Instance(Vec<InstanceTypeDeclaration>),
    Resource { rep: u32, dtor: Option<u32> },
}

#[derive(Debug, Clone)]
pub enum ComponentTypeDeclaration {
    CoreType(CoreType),
    Type(ComponentType),
    Alias(ComponentAlias),
    Export { name: String, ty: ComponentTypeRef },
    Import { name: String, ty: ComponentTypeRef },
}

#[derive(Debug, Clone)]
pub enum InstanceTypeDeclaration {
    CoreType(CoreType),
    Type(ComponentType),
    Alias(ComponentAlias),
    Export { name: String, ty: ComponentTypeRef },
}

#[derive(Debug, Clone)]
pub enum ComponentTypeRef {
    Module(u32),
    Func(u32),
    Value(ComponentValType),
    Type(TypeBounds),
    Instance(u32),
    Component(u32),
}

#[derive(Debug, Clone)]
pub enum TypeBounds {
    Eq(u32),
    SubResource,
}

#[derive(Debug, Clone)]
pub struct ComponentFuncType {
    pub params: Vec<(String, ComponentValType)>,
    pub results: ComponentFuncResult,
}

#[derive(Debug, Clone)]
pub enum ComponentFuncResult {
    Unnamed(ComponentValType),
    Named(Vec<(String, ComponentValType)>),
}

#[derive(Debug, Clone)]
pub enum DefinedType {
    Primitive(PrimitiveValType),
    Record(Vec<(String, ComponentValType)>),
    Variant(Vec<VariantCase>),
    List(ComponentValType),
    Tuple(Vec<ComponentValType>),
    Flags(Vec<String>),
    Enum(Vec<String>),
    Union(Vec<ComponentValType>),
    Option(ComponentValType),
    Result { ok: Option<ComponentValType>, err: Option<ComponentValType> },
    Own(u32),
    Borrow(u32),
}

#[derive(Debug, Clone)]
pub struct VariantCase {
    pub name: String,
    pub ty: Option<ComponentValType>,
    pub refines: Option<u32>,
}

#[derive(Debug, Clone)]
pub enum ComponentValType {
    Primitive(PrimitiveValType),
    Type(u32), // Index to a DefinedType
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimitiveValType {
    Bool,
    S8, U8,
    S16, U16,
    S32, U32,
    S64, U64,
    F32, F64,
    Char,
    String,
}

#[derive(Debug, Clone)]
pub enum CoreType {
    Func(Vec<crate::wasm::common::reader::types::ValType>, Vec<crate::wasm::common::reader::types::ValType>),
    Module(Vec<CoreModuleTypeDecl>),
}

#[derive(Debug, Clone)]
pub enum CoreModuleTypeDecl {
    Import { name: String, url: String, ty: CoreTypeRef },
    Export { name: String, ty: CoreTypeRef },
}

#[derive(Debug, Clone)]
pub enum CoreTypeRef {
    Func(u32),
    Module(u32),
    Memory(crate::wasm::common::reader::types::MemType),
    Global(crate::wasm::common::reader::types::GlobalType),
    Table(crate::wasm::common::reader::types::TableType),
    Tag(u32), // Exceptions?
}


#[derive(Debug, Clone)]
pub enum ComponentCanon {
    Lift { func_idx: u32, options: Vec<CanonOpt>, type_idx: u32 },
    Lower { func_idx: u32, options: Vec<CanonOpt> },
    ResourceNew { type_idx: u32 },
    ResourceDrop { type_idx: u32 },
    ResourceRep { type_idx: u32 },
}

#[derive(Debug, Clone)]
pub enum CanonOpt {
    StringUtf8,
    StringUtf16,
    StringLatin1Utf16,
    Memory(u32),
    Realloc(u32),
    PostReturn(u32),
}

impl WasmReadable for ComponentType {
    fn read(wasm: &mut WasmReader) -> Result<Self, ValidationError> {
        let b = wasm.read_u8()?;
        // 0x40..0x4F: Defined Types (compressed)
        // 0x50..: other?
        // Actually, defined types are usually:
        // 0x72: record
        // 0x71: variant
        // ...
        // But the section starts with a byte indicating the 'sort' of type.
        
        // However, Component Model binary format says Type Section is a vec(type).
        // type ::= <defined-type> | <func-type> | <component-type> | <instance-type>
        
        // Check for Defined Type tags first.
        match b {
            0x72 | 0x71 | 0x70 | 0x6f | 0x6e | 0x6d | 0x6b | 0x6a | 0x69 | 0x68 | 0x67 | 0x66 | 0x73..=0x7f => {
                // It's a DefinedType! Backtrack or handle here?
                // Primitive types are also DefinedTypes.
                // Re-read or parse directly.
                let defined = parse_defined_type(wasm, b)?;
                Ok(ComponentType::Defined(defined))
            }
            0x40 => { // Func Type
                let params = wasm.read_vec(|r| {
                    let name = r.read_component_name()?; // k
                    let ty = parse_val_type(r)?; // t
                    Ok((name, ty))
                })?;
                let res_byte = wasm.read_u8()?;
                let results = if res_byte == 0x00 {
                     ComponentFuncResult::Unnamed(parse_val_type(wasm)?)
                } else {
                     // 0x01
                     let map = wasm.read_vec(|r| {
                        let name = r.read_component_name()?;
                        let ty = parse_val_type(r)?;
                        Ok((name, ty))
                     })?;
                     ComponentFuncResult::Named(map)
                };
                Ok(ComponentType::Func(ComponentFuncType { params, results }))
            }
            0x41 => { // Component Type
                let decls = wasm.read_vec(parse_component_type_decl)?;
                Ok(ComponentType::Component(decls))
            }
            0x42 => { // Instance Type
                let decls = wasm.read_vec(parse_instance_type_decl)?;
                Ok(ComponentType::Instance(decls))
            }
            0x3f => { // Resource
                 let rep = wasm.read_var_u32()?;
                 let dtor = if wasm.read_u8()? == 0x01 {
                     Some(wasm.read_var_u32()?)
                 } else {
                     None
                 };
                 Ok(ComponentType::Resource { rep, dtor })
            }
            _ => Err(ValidationError::MalformedValType),
        }
    }
}

fn parse_val_type(wasm: &mut WasmReader) -> Result<ComponentValType, ValidationError> {
    let b = wasm.peek_u8()?;
    if b < 0x73 { // index? or error?
        // Indices are positive. Primitives are high.
        // If it starts with a byte < 0x60, it's likely a varint index.
        // Actually primitives are:
        // bool=7f, s8=7e, u8=7d, s16=7c, u16=7b, s32=7a, u32=79, s64=78, u64=77, f32=76, f64=75, char=74, string=73
        // So anything >= 0x73 is a primitive.
        if b >= 0x73 {
            wasm.read_u8()?;
            let prim = match b {
                0x7f => PrimitiveValType::Bool,
                0x7e => PrimitiveValType::S8,
                0x7d => PrimitiveValType::U8,
                0x7c => PrimitiveValType::S16,
                0x7b => PrimitiveValType::U16,
                0x7a => PrimitiveValType::S32,
                0x79 => PrimitiveValType::U32,
                0x78 => PrimitiveValType::S64,
                0x77 => PrimitiveValType::U64,
                0x76 => PrimitiveValType::F32,
                0x75 => PrimitiveValType::F64,
                0x74 => PrimitiveValType::Char,
                0x73 => PrimitiveValType::String,
                _ => return Err(ValidationError::MalformedValType),
            };
            Ok(ComponentValType::Primitive(prim))
        } else {
             let idx = wasm.read_var_u32()?;
             Ok(ComponentValType::Type(idx))
        }
    } else {
         wasm.read_u8()?;
         let prim = match b {
            0x7f => PrimitiveValType::Bool,
            0x7e => PrimitiveValType::S8,
            0x7d => PrimitiveValType::U8,
            0x7c => PrimitiveValType::S16,
            0x7b => PrimitiveValType::U16,
            0x7a => PrimitiveValType::S32,
            0x79 => PrimitiveValType::U32,
            0x78 => PrimitiveValType::S64,
            0x77 => PrimitiveValType::U64,
            0x76 => PrimitiveValType::F32,
            0x75 => PrimitiveValType::F64,
            0x74 => PrimitiveValType::Char,
            0x73 => PrimitiveValType::String,
            _ => return Err(ValidationError::MalformedValType),
        };
        Ok(ComponentValType::Primitive(prim))
    }
}

fn parse_defined_type(wasm: &mut WasmReader, tag: u8) -> Result<DefinedType, ValidationError> {
    match tag {
        0x73..=0x7f => {
             let prim = match tag {
                0x7f => PrimitiveValType::Bool,
                0x7e => PrimitiveValType::S8,
                0x7d => PrimitiveValType::U8,
                0x7c => PrimitiveValType::S16,
                0x7b => PrimitiveValType::U16,
                0x7a => PrimitiveValType::S32,
                0x79 => PrimitiveValType::U32,
                0x78 => PrimitiveValType::S64,
                0x77 => PrimitiveValType::U64,
                0x76 => PrimitiveValType::F32,
                0x75 => PrimitiveValType::F64,
                0x74 => PrimitiveValType::Char,
                0x73 => PrimitiveValType::String,
                _ => return Err(ValidationError::MalformedValType),
            };
            Ok(DefinedType::Primitive(prim))
        }
        0x72 => { // Record
            let fields = wasm.read_vec(|r| {
                let name = r.read_component_name()?;
                let ty = parse_val_type(r)?;
                Ok((name, ty))
            })?;
            Ok(DefinedType::Record(fields))
        }
        0x71 => { // Variant
            let cases = wasm.read_vec(|r| {
                let name = r.read_component_name()?;
                let ty = if r.read_u8()? == 0x01 {
                    Some(parse_val_type(r)?)
                } else {
                    None
                };
                let refines = if r.read_u8()? == 0x01 {
                    Some(r.read_var_u32()?)
                } else {
                    None
                };
                Ok(VariantCase { name, ty, refines })
            })?;
            Ok(DefinedType::Variant(cases))
        }
        0x70 => { // List
            let ty = parse_val_type(wasm)?;
            Ok(DefinedType::List(ty))
        }
        0x6f => { // Tuple
            let types = wasm.read_vec(parse_val_type)?;
            Ok(DefinedType::Tuple(types))
        }
        0x6e => { // Flags
            let names = wasm.read_vec(|r| r.read_component_name())?;
            Ok(DefinedType::Flags(names))
        }
        0x6d => { // Enum
            let names = wasm.read_vec(|r| r.read_component_name())?;
            Ok(DefinedType::Enum(names))
        }
        0x6b => { // Option
            let ty = parse_val_type(wasm)?;
            Ok(DefinedType::Option(ty))
        }
        0x6a => { // Result
            let ok = if wasm.read_u8()? == 0x01 { Some(parse_val_type(wasm)?) } else { None };
            let err = if wasm.read_u8()? == 0x01 { Some(parse_val_type(wasm)?) } else { None };
            Ok(DefinedType::Result { ok, err })
        }
        0x69 => { // Own
            let idx = wasm.read_var_u32()?;
            Ok(DefinedType::Own(idx))
        }
        0x68 => { // Borrow
            let idx = wasm.read_var_u32()?;
            Ok(DefinedType::Borrow(idx))
        }
        _ => Err(ValidationError::MalformedValType),
    }
}

fn parse_component_type_decl(wasm: &mut WasmReader) -> Result<ComponentTypeDeclaration, ValidationError> {
    // 0x03=type, 0x00=core-type, 0x06=alias, 0x04=export, 0x05=import
    let tag = wasm.read_u8()?;
    match tag {
        0x03 => {
            let ty = ComponentType::read(wasm)?;
            Ok(ComponentTypeDeclaration::Type(ty))
        }
        0x00 => { // Core Type?
             // Todo: full core type parsing
             // For now assume it's module decl
             let _ = wasm.read_vec(|r| r.read_u8())?; // Skip bytes
             Ok(ComponentTypeDeclaration::CoreType(CoreType::Module(Vec::new())))
        }
        0x06 => { // Alias
             // Reuse parse_alias logic or reimplement?
             // Since ComponentAlias logic is in reader.rs/mod.rs which uses read_vec, we might need to duplicate or expose it.
             // For now, minimal alias:
             let sort = wasm.read_u8()?;
             let target = wasm.read_u8()?; // kind
             let kind = match target {
                 0x01 => { // Outer
                     let count = wasm.read_var_u32()?;
                     let idx = wasm.read_var_u32()?;
                     ComponentAliasKind::Outer { count, sort, idx }
                 }
                 _ => return Err(ValidationError::MalformedValType),
             };
             Ok(ComponentTypeDeclaration::Alias(ComponentAlias { sort, kind }))
        }
        0x04 => { // Export
            let name = wasm.read_component_name()?;
            let ty = parse_component_type_ref(wasm)?;
            Ok(ComponentTypeDeclaration::Export { name, ty })
        }
        0x05 => { // Import
            let name = wasm.read_component_name()?;
            let ty = parse_component_type_ref(wasm)?;
            Ok(ComponentTypeDeclaration::Import { name, ty })
        }
        _ => Err(ValidationError::MalformedValType),
    }
}

fn parse_instance_type_decl(wasm: &mut WasmReader) -> Result<InstanceTypeDeclaration, ValidationError> {
    // 0x03=type, 0x00=core-type, 0x06=alias, 0x04=export
    let tag = wasm.read_u8()?;
    match tag {
         0x03 => {
             let ty = ComponentType::read(wasm)?;
             Ok(InstanceTypeDeclaration::Type(ty))
         }
         0x00 => { // Core Type
             let _ = wasm.read_vec(|r| r.read_u8())?;
             Ok(InstanceTypeDeclaration::CoreType(CoreType::Module(Vec::new())))
         }
         0x06 => { // Alias
             let sort = wasm.read_u8()?;
             let target = wasm.read_u8()?;
             let kind = match target {
                 0x01 => {
                     let count = wasm.read_var_u32()?;
                     let idx = wasm.read_var_u32()?;
                     ComponentAliasKind::Outer { count, sort, idx }
                 }
                 _ => return Err(ValidationError::MalformedValType),
             };
             Ok(InstanceTypeDeclaration::Alias(ComponentAlias { sort, kind }))
         }
         0x04 => { // Export
             let name = wasm.read_component_name()?;
             let ty = parse_component_type_ref(wasm)?;
             Ok(InstanceTypeDeclaration::Export { name, ty })
         }
         _ => Err(ValidationError::MalformedValType)
    }
}

fn parse_component_type_ref(wasm: &mut WasmReader) -> Result<ComponentTypeRef, ValidationError> {
    // TypeBounds or direct index?
    // export/import desc:
    // 0x00 <idx> = module
    // 0x01 <idx> = component
    // 0x02 <idx> = instance
    // 0x03 <idx> = func
    // 0x04 <val-type> = value
    // 0x05 <type-bounds> = type
    let kind = wasm.read_u8()?;
    match kind {
        0x00 => Ok(ComponentTypeRef::Module(wasm.read_var_u32()?)),
        0x01 => Ok(ComponentTypeRef::Component(wasm.read_var_u32()?)),
        0x02 => Ok(ComponentTypeRef::Instance(wasm.read_var_u32()?)),
        0x03 => Ok(ComponentTypeRef::Func(wasm.read_var_u32()?)),
        0x04 => Ok(ComponentTypeRef::Value(parse_val_type(wasm)?)),
        0x05 => { // TypeBounds
            let b = wasm.read_u8()?;
            let bounds = match b {
                0x00 => TypeBounds::Eq(wasm.read_var_u32()?),
                0x01 => TypeBounds::SubResource,
                _ => return Err(ValidationError::MalformedValType),
            };
            Ok(ComponentTypeRef::Type(bounds))
        }
        _ => Err(ValidationError::MalformedValType),
    }
}

impl WasmReadable for ComponentCanon {
    fn read(wasm: &mut WasmReader) -> Result<Self, ValidationError> {
        let tag = wasm.read_u8()?;
        match tag {
            0x00 => { // Lift
                let type_idx = wasm.read_var_u32()?;
                let options = wasm.read_vec(parse_canon_opt)?;
                let func_idx = wasm.read_var_u32()?;
                Ok(ComponentCanon::Lift { func_idx, options, type_idx })
            }
            0x01 => { // Lower
                let func_idx = wasm.read_var_u32()?;
                let options = wasm.read_vec(parse_canon_opt)?;
                Ok(ComponentCanon::Lower { func_idx, options })
            }
            0x02 => Ok(ComponentCanon::ResourceNew { type_idx: wasm.read_var_u32()? }),
            0x03 => Ok(ComponentCanon::ResourceDrop { type_idx: wasm.read_var_u32()? }),
            0x04 => Ok(ComponentCanon::ResourceRep { type_idx: wasm.read_var_u32()? }),
            _ => Err(ValidationError::MalformedValType)
        }
    }
}

fn parse_canon_opt(wasm: &mut WasmReader) -> Result<CanonOpt, ValidationError> {
    let tag = wasm.read_u8()?;
    match tag {
        0x00 => Ok(CanonOpt::StringUtf8),
        0x01 => Ok(CanonOpt::StringUtf16),
        0x02 => Ok(CanonOpt::StringLatin1Utf16),
        0x03 => Ok(CanonOpt::Memory(wasm.read_var_u32()?)),
        0x04 => Ok(CanonOpt::Realloc(wasm.read_var_u32()?)),
        0x05 => Ok(CanonOpt::PostReturn(wasm.read_var_u32()?)),
        _ => Err(ValidationError::MalformedValType),
    }
}

#[derive(Debug, Clone)]
pub struct ComponentStart {
    pub func_idx: u32,
    pub args: Vec<u32>,
    pub results: u32,
}

#[derive(Debug, Clone)]
pub struct ComponentImport {
    pub name: String,
    pub ty: u32, // Type index
}

#[derive(Debug, Clone)]
pub struct ComponentExport {
    pub name: String,
    pub kind: u8, // Sort index
    pub idx: u32,
}

impl WasmReadable for ComponentImport {
    fn read(wasm: &mut WasmReader) -> Result<Self, ValidationError> {
        let name = wasm.read_component_name()?;
        let sort = wasm.read_u8()?;

        // ComponentTypeRef parsing (sort is the Kind)
        let ty = match sort {
            0x00 | 0x01 | 0x04 | 0x05 => wasm.read_var_u32()?, // Module, Func, Component, Instance
            0x02 => { // Value (ComponentValType)
                let peek = wasm.peek_u8()?;
                // Primitives are usually high bytes (0x7F..), indices are low.
                // Simple heuristic: if it looks like a varint start (byte < 0x60), treat as index.
                // ValType encodings: 0x7F(i32)..0x64(error-context). Indices are positive s33.
                if peek >= 0x60 {
                    wasm.read_u8()?;
                    0 // Primitive, no index
                } else {
                    wasm.read_var_u32()?
                }
            }
            0x03 => { // Type (TypeBounds)
                let tag = wasm.read_u8()?;
                match tag {
                    0x00 => wasm.read_var_u32()?, // Eq(idx)
                    0x01 => 0, // SubResource
                    _ => return Err(ValidationError::MalformedValType)
                }
            }
            _ => return Err(ValidationError::Component(super::error::ComponentError::MalformedSectionId(sort))),
        };
        Ok(Self { name, ty })
    }
}

impl WasmReadable for ComponentExport {
    fn read(wasm: &mut WasmReader) -> Result<Self, ValidationError> {
        let name = wasm.read_component_name()?;
        let kind = wasm.read_u8()?;
        let idx = wasm.read_var_u32()?;

        // Consume optional type ascription
        let has_ty = wasm.read_u8()?;
        if has_ty == 0x01 {
            let ref_kind = wasm.read_u8()?;
            match ref_kind {
                0x00 | 0x01 | 0x04 | 0x05 => { wasm.read_var_u32()?; }
                0x02 => { // Value
                    let peek = wasm.peek_u8()?;
                    if peek >= 0x60 { wasm.read_u8()?; } else { wasm.read_var_u32()?; }
                }
                0x03 => { // Type
                    let tag = wasm.read_u8()?;
                    if tag == 0x00 { wasm.read_var_u32()?; }
                }
                _ => return Err(ValidationError::MalformedValType)
            }
        }

        Ok(Self { name, kind, idx })
    }
}

impl WasmReadable for CoreInstantiationArg {
    fn read(wasm: &mut WasmReader) -> Result<Self, ValidationError> {
        let name = wasm.read_name()?.to_owned();
        let kind = wasm.read_u8()?;
        let idx = wasm.read_var_u32()?;
        Ok(Self { name, kind, idx })
    }
}

impl WasmReadable for CoreExport {
    fn read(wasm: &mut WasmReader) -> Result<Self, ValidationError> {
        let name = wasm.read_name()?.to_owned();
        let kind = wasm.read_u8()?;
        let idx = wasm.read_var_u32()?;
        Ok(Self { name, kind, idx })
    }
}

impl WasmReadable for ComponentInstantiationArg {
    fn read(wasm: &mut WasmReader) -> Result<Self, ValidationError> {
        // Fully permissive read for truncated/compressed args in saltty.wasm

        // Try reading name
        let name = match wasm.read_name() {
            Ok(n) => n.to_owned(),
            Err(ValidationError::Eof) => return Ok(Self { name: String::new(), kind: 0, idx: 0 }),
            Err(e) => return Err(e),
        };

        // Try reading kind
        let kind = if wasm.remaining_bytes().is_empty() {
            0
        } else {
            wasm.read_u8()?
        };

        // Try reading idx
        let idx = if wasm.remaining_bytes().is_empty() {
            0
        } else {
            wasm.read_var_u32()?
        };

        Ok(Self { name, kind, idx })
    }
}