use crate::rust_alloc::borrow::ToOwned;
use crate::rust_alloc::string::String;
use crate::rust_alloc::vec::Vec;
use crate::wasm::core::error::ValidationError;
use crate::wasm::core::reader::span::Span;
use crate::wasm::core::reader::{WasmReadable, WasmReader};

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
pub struct ComponentType {
    // TODO: Full type parsing
    pub content: Span,
}

#[derive(Debug, Clone)]
pub struct ComponentCanon {
    // TODO: Canonical definitions
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