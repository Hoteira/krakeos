use super::error::ComponentError;
use super::types::*;
use crate::alloc::borrow::ToOwned;
use crate::wasm::common::error::ValidationError;
use crate::wasm::common::reader::{WasmReadable, WasmReader};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentSectionTy {
    Custom,       // 0
    CoreModule,   // 1
    CoreInstance, // 2
    CoreAlias,    // 3
    CoreType,     // 4
    Component,    // 5
    Alias,        // 6
    Type,         // 7
    Instance,     // 8
    Canon,        // 9
    Start,        // 10
    Import,       // 11
    Export,       // 12
    Unknown(u8),
}

impl ComponentSectionTy {
    pub fn from_u8(v: u8) -> Result<Self, ComponentError> {
        match v {
            0 => Ok(Self::Custom),
            1 => Ok(Self::CoreModule),
            2 => Ok(Self::CoreInstance),
            3 => Ok(Self::CoreAlias),
            4 => Ok(Self::CoreType),
            5 => Ok(Self::Component),
            6 => Ok(Self::Alias),
            7 => Ok(Self::Type),
            8 => Ok(Self::Instance),
            9 => Ok(Self::Canon),
            10 => Ok(Self::Import), // Was Start
            11 => Ok(Self::Export), // Was Import
            12 => Ok(Self::Start),  // Was Export
            _ => Ok(Self::Unknown(v)),
        }
    }
}

pub fn parse_component(wasm: &mut WasmReader) -> Result<ParsedComponent, ComponentError> {
    // crate::debugln!("Parsing Component...");
    let mut component = ParsedComponent::default();

    while !wasm.remaining_bytes().is_empty() {
        let (ty, size) = read_section_header(wasm)?;
        // crate::debugln!("Parsing Section: {:?} (Size: {}) at pc {:#x}", ty, size, wasm.pc);

        // Handle sections that need absolute spans or skipping logic on the main reader
        match ty {
            ComponentSectionTy::Custom |
            ComponentSectionTy::CoreType |
            ComponentSectionTy::Unknown(_) => {
                wasm.skip(size as usize).map_err(|_| ComponentError::UnexpectedEof)?;
                continue;
            }
            ComponentSectionTy::CoreModule => {
                let content = wasm.make_span(size as usize).map_err(|_| ComponentError::UnexpectedEof)?;
                component.items.push(ComponentItem::Module(ComponentModule { content }));
                wasm.skip(size as usize).map_err(|_| ComponentError::UnexpectedEof)?;
                continue;
            }
            ComponentSectionTy::Component => {
                // crate::debugln!("Parsing Nested Component Section...");
                let nested_len = size as usize;
                let nested_bytes = wasm.make_span(nested_len).map_err(|_| ComponentError::UnexpectedEof)?;

                // Check for valid component header before recursing
                let sub_binary = &wasm.full_wasm_binary[nested_bytes.from..nested_bytes.from + nested_bytes.len];
                let is_binary_component = if sub_binary.len() >= 8 {
                    &sub_binary[0..4] == [0x00, 0x61, 0x73, 0x6d] && &sub_binary[4..8] == [0x0d, 0x00, 0x01, 0x00]
                } else {
                    false
                };

                if is_binary_component {
                    let mut sub_reader = WasmReader::new(sub_binary);
                    // Skip header
                    let _ = sub_reader.skip(8);

                    match parse_component(&mut sub_reader) {
                        Ok(nested_component) => {
                            component.items.push(ComponentItem::Component(NestedComponent {
                                parsed: nested_component,
                                content: nested_bytes,
                            }));
                        }
                        Err(e) => {
                            crate::debugln!("Error parsing nested component: {:?}", e);
                            return Err(e);
                        }
                    }
                } else {
                    crate::debugln!("  Skipping non-binary/unknown nested component");
                    // Push a placeholder so the index space is maintained
                    component.items.push(ComponentItem::Component(NestedComponent {
                        parsed: ParsedComponent::default(),
                        content: nested_bytes,
                    }));
                }

                // Ensure we advance the main reader past this section
                wasm.skip(nested_len).map_err(|_| ComponentError::UnexpectedEof)?;
                continue;
            }
            _ => {} // Fall through to structured parsing with slice
        }

        // For structured sections, we slice the data to strict bounds to prevent over-reading
        let section_data = wasm.strip_bytes_dynamic(size as usize).map_err(|_| ComponentError::UnexpectedEof)?;
        // crate::debugln!("  Section Data: {:02x?}", section_data);
        let mut reader = WasmReader::new(section_data);

        match ty {
            ComponentSectionTy::CoreInstance => {
                let core_instances = reader.read_vec(|r| {
                    let kind = r.read_u8()?;
                    match kind {
                        0x00 => { // Instantiate
                            let module_idx = r.read_var_u32()?;
                            let args = r.read_vec(CoreInstantiationArg::read)?;
                            Ok(CoreInstance { kind: CoreInstanceKind::Instantiate { module_idx, args } })
                        }
                        0x01 => { // From Exports
                            let exports = r.read_vec(CoreExport::read)?;
                            Ok(CoreInstance { kind: CoreInstanceKind::FromExports { exports } })
                        }
                        _ => Err(ValidationError::Component(ComponentError::UnimplementedSection(kind))),
                    }
                }).map_err(|_| ComponentError::MalformedVarU32)?;
                for (i, inst) in core_instances.iter().enumerate() {
                    // crate::debugln!("  Core Instance {}: {:?}", i, inst.kind);
                }
                component.items.extend(core_instances.into_iter().map(ComponentItem::CoreInstance));
            }
            ComponentSectionTy::CoreAlias => {
                let aliases = reader.read_vec(|r| {
                    let sort = r.read_u8()?;
                    let kind = r.read_u8()?;
                    let target = match kind {
                        0x00 => { // Export
                            let instance_idx = r.read_var_u32()?;
                            let name = r.read_name()?.to_owned();
                            CoreAliasKind::Export { instance_idx, name }
                        }
                        0x01 => { // Outer
                            let count = r.read_var_u32()?;
                            let idx = r.read_var_u32()?;
                            CoreAliasKind::Outer { count, idx }
                        }
                        _ => return Err(ValidationError::Component(ComponentError::UnimplementedSection(kind))),
                    };
                    Ok(CoreAlias { sort, kind: target })
                }).map_err(|_| ComponentError::MalformedVarU32)?;
                component.items.extend(aliases.into_iter().map(ComponentItem::CoreAlias));
            }
            ComponentSectionTy::Instance => {
                let instances = reader.read_vec(|r| {
                    let kind_byte = r.read_u8()?;
                    match kind_byte {
                        0x00 => { // Instantiate Module
                            let module_idx = r.read_var_u32()?;
                            let args = r.read_vec(ComponentInstantiationArg::read)?;
                            Ok(ComponentInstance {
                                kind: ComponentInstanceKind::InstantiateModule { module_idx, args }
                            })
                        }
                        0x01 => { // Instantiate Component
                            let component_idx = r.read_var_u32()?;
                            let args = r.read_vec(ComponentInstantiationArg::read)?;
                            Ok(ComponentInstance {
                                kind: ComponentInstanceKind::InstantiateComponent { component_idx, args }
                            })
                        }
                        0x02 => { // Instance From Exports
                            let values = r.read_vec(|r2| {
                                let name = r2.read_name()?.to_owned(); // Plain name
                                let kind = r2.read_u8()?;
                                let idx = r2.read_var_u32()?;
                                Ok(ComponentExport { name, kind, idx })
                            })?;
                            Ok(ComponentInstance {
                                kind: ComponentInstanceKind::FromExports { values }
                            })
                        }
                        0x03 => { // Instance From Core Instance
                            let core_instance_idx = r.read_var_u32()?;
                            Ok(ComponentInstance {
                                kind: ComponentInstanceKind::FromCoreInstance { core_instance_idx }
                            })
                        }
                        _ => Err(ValidationError::Component(ComponentError::UnimplementedSection(kind_byte))),
                    }
                }).map_err(|e| {
                    crate::debugln!("Instance section parsing failed: {:?}", e);
                    if let ValidationError::Component(ce) = e { return ce; }
                    ComponentError::MalformedVarU32
                })?;
                for (i, inst) in instances.iter().enumerate() {
                    // crate::debugln!("  Instance {}: {:?}", i, inst.kind);
                }
                component.items.extend(instances.into_iter().map(ComponentItem::Instance));
            }
            ComponentSectionTy::Alias => {
                let aliases = reader.read_vec(|r| {
                    let sort = r.read_u8()?;
                    let _byte2 = if sort == 0x00 {
                        Some(r.read_u8()?)
                    } else {
                        None
                    };

                    let kind_byte = r.read_u8()?;

                    let res = match kind_byte {
                        0x00 => { // InstanceExport
                            // Format: [instance_idx] [name] - Aliases use plain strings for names!
                            let instance_idx = r.read_var_u32()?;
                            let name = r.read_name()?.to_owned();
                            Ok(ComponentAlias {
                                sort,
                                kind: ComponentAliasKind::InstanceExport { instance_idx, name },
                            })
                        }
                        0x01 => { // CoreInstanceExport
                            // Format: [instance_idx] [plain_name]
                            let instance_idx = r.read_var_u32()?;
                            let name = r.read_name()?.to_owned(); // Core exports are plain strings
                            Ok(ComponentAlias {
                                sort,
                                kind: ComponentAliasKind::CoreInstanceExport { instance_idx, name },
                            })
                        }
                        0x02 => { // Outer
                            // Format: [count] [index]
                            let count = r.read_var_u32()?;
                            let index = r.read_var_u32()?;
                            Ok(ComponentAlias {
                                sort,
                                kind: ComponentAliasKind::Outer { count, sort: sort, idx: index },
                            })
                        }
                        _ => {
                            Err(ValidationError::Component(ComponentError::UnimplementedSection(kind_byte)))
                        }
                    };
                    if let Ok(ref a) = res {
                        // crate::debugln!("  Parsed Alias: sort={}, kind_byte={:#x}", a.sort, kind_byte);
                    }
                    res
                }).map_err(|e| {
                    crate::debugln!("Alias section parsing failed: {:?}", e);
                    if let ValidationError::Component(ce) = e { return ce; }
                    ComponentError::MalformedVarU32
                })?;
                component.items.extend(aliases.into_iter().map(ComponentItem::Alias));
            }
            ComponentSectionTy::Type => {
                let types = reader.read_vec(ComponentType::read).map_err(|_| ComponentError::MalformedVarU32)?;
                component.items.extend(types.into_iter().map(ComponentItem::Type));
            }
            ComponentSectionTy::Canon => {
                let canons = reader.read_vec(ComponentCanon::read).map_err(|_| ComponentError::MalformedVarU32)?;
                component.items.extend(canons.into_iter().map(ComponentItem::Canon));
            }
            ComponentSectionTy::Start => {
                // crate::debugln!("Parsing Start Section...");
                let func_idx = reader.read_var_u32().map_err(|_| ComponentError::MalformedVarU32)?;
                let args = reader.read_vec(|w| w.read_var_u32()).map_err(|_| ComponentError::MalformedVarU32)?;
                let results = reader.read_var_u32().map_err(|_| ComponentError::MalformedVarU32)?;
                component.items.push(ComponentItem::Start(ComponentStart { func_idx, args, results }));
            }
            ComponentSectionTy::Import => {
                let imports = reader.read_vec(ComponentImport::read).map_err(|_| ComponentError::MalformedVarU32)?;
                for import in &imports {
                    // crate::debugln!("  Import: {}", import.name);
                }
                component.items.extend(imports.into_iter().map(ComponentItem::Import));
            }
            ComponentSectionTy::Export => {
                let exports = reader.read_vec(ComponentExport::read).map_err(|_| ComponentError::MalformedVarU32)?;
                for export in &exports {
                    // crate::debugln!("  Export: {} (kind: {}, idx: {})", export.name, export.kind, export.idx);
                }
                component.items.extend(exports.into_iter().map(ComponentItem::Export));
            }
            _ => {
                // Should be handled in first match or skipped
            }
        }
    }

    // crate::debugln!("Component Parsing Finished.");
    Ok(component)
}

pub(crate) fn read_section_header(reader: &mut WasmReader) -> Result<(ComponentSectionTy, u32), ComponentError> {
    let id = reader.read_u8().map_err(|_| ComponentError::UnexpectedEof)?;
    let size = reader.read_var_u32().map_err(|_| ComponentError::MalformedVarU32)?;
    let ty = ComponentSectionTy::from_u8(id)?;
    Ok((ty, size))
}
