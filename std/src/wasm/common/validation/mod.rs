use crate::alloc::{
    collections::btree_set::{self, BTreeSet},
    vec::Vec,
};
use crate::wasm::common::error::ValidationError;
use crate::wasm::common::indices::{FuncIdx, TypeIdx};
use crate::wasm::common::reader::section_header::{SectionHeader, SectionTy};
use crate::wasm::common::reader::span::Span;
use crate::wasm::common::reader::types::data::DataSegment;
use crate::wasm::common::reader::types::element::ElemType;
use crate::wasm::common::reader::types::export::{Export, ExportDesc};
use crate::wasm::common::reader::types::global::{Global, GlobalType};
use crate::wasm::common::reader::types::import::{Import, ImportDesc};
use crate::wasm::common::reader::types::{FuncType, MemType, ResultType, TableType};
use crate::wasm::common::reader::{WasmReadable, WasmReader};
use crate::wasm::common::sidetable::Sidetable;
use crate::wasm::component::types::ParsedComponent;

pub mod code;
pub mod data;
pub mod globals;
pub mod read_constant_expression;
pub mod validation_stack;

#[derive(Clone, Debug)]
pub struct ImportsLength {
    pub imported_functions: usize,
    pub imported_globals: usize,
    pub imported_memories: usize,
    pub imported_tables: usize,
}

#[derive(Clone, Debug)]
pub struct ValidationInfo<'bytecode> {
    pub wasm: &'bytecode [u8],
    pub types: Vec<FuncType>,
    pub imports: Vec<Import>,
    pub functions: Vec<TypeIdx>,
    pub tables: Vec<TableType>,
    pub memories: Vec<MemType>,
    pub globals: Vec<Global>,
    pub functions_types: Vec<TypeIdx>,
    pub exports: Vec<Export>,
    pub func_blocks_stps: Vec<(Span, usize)>,
    pub sidetable: Sidetable,
    pub data: Vec<DataSegment>,
    pub start: Option<FuncIdx>,
    pub elements: Vec<ElemType>,
    pub imports_length: ImportsLength,
    pub component: Option<ParsedComponent>,
}

fn validate_exports(validation_info: &ValidationInfo) -> Result<(), ValidationError> {
    let mut found_export_names: btree_set::BTreeSet<&str> = btree_set::BTreeSet::new();
    for export in &validation_info.exports {
        if found_export_names.contains(export.name.as_str()) {
            return Err(ValidationError::DuplicateExportName);
        }
        found_export_names.insert(export.name.as_str());
        match export.desc {
            ExportDesc::FuncIdx(func_idx) => {
                if validation_info.functions.len()
                    + validation_info.imports_length.imported_functions
                    <= func_idx
                {
                    return Err(ValidationError::InvalidFuncIdx(func_idx));
                }
            }
            ExportDesc::TableIdx(table_idx) => {
                if validation_info.tables.len() + validation_info.imports_length.imported_tables
                    <= table_idx
                {
                    return Err(ValidationError::InvalidTableIdx(table_idx));
                }
            }
            ExportDesc::MemIdx(mem_idx) => {
                if validation_info.memories.len() + validation_info.imports_length.imported_memories
                    <= mem_idx
                {
                    return Err(ValidationError::InvalidMemIndex(mem_idx));
                }
            }
            ExportDesc::GlobalIdx(global_idx) => {
                if validation_info.globals.len() + validation_info.imports_length.imported_globals
                    <= global_idx
                {
                    return Err(ValidationError::InvalidGlobalIdx(global_idx));
                }
            }
        }
    }
    Ok(())
}

fn get_imports_length(imports: &Vec<Import>) -> ImportsLength {
    let mut imports_length = ImportsLength {
        imported_functions: 0,
        imported_globals: 0,
        imported_memories: 0,
        imported_tables: 0,
    };
    for import in imports {
        match import.desc {
            ImportDesc::Func(_) => imports_length.imported_functions += 1,
            ImportDesc::Global(_) => imports_length.imported_globals += 1,
            ImportDesc::Mem(_) => imports_length.imported_memories += 1,
            ImportDesc::Table(_) => imports_length.imported_tables += 1,
        }
    }
    imports_length
}

pub fn validate(wasm: &[u8]) -> Result<ValidationInfo<'_>, ValidationError> {
    let mut wasm_reader = WasmReader::new(wasm);
    let mut validation_context_refs: BTreeSet<FuncIdx> = BTreeSet::new();
    
    let magic = wasm_reader.strip_bytes::<4>()?;
    if magic != [0x00, 0x61, 0x73, 0x6d] {
        return Err(ValidationError::InvalidMagic);
    }
    
    let version_bytes = wasm_reader.strip_bytes::<4>()?;
    if version_bytes == [0x01, 0x00, 0x00, 0x00] {
        // Core Module
    } else if version_bytes == [0x0d, 0x00, 0x01, 0x00] {
        let parsed_component = crate::wasm::component::reader::parse_component(&mut wasm_reader)
            .map_err(ValidationError::Component)?;
        return Ok(ValidationInfo {
            wasm: wasm_reader.into_inner(),
            types: Vec::new(),
            imports: Vec::new(),
            functions: Vec::new(),
            tables: Vec::new(),
            memories: Vec::new(),
            globals: Vec::new(),
            functions_types: Vec::new(),
            exports: Vec::new(),
            func_blocks_stps: Vec::new(),
            sidetable: Vec::new(),
            data: Vec::new(),
            start: None,
            elements: Vec::new(),
            imports_length: ImportsLength { imported_functions: 0, imported_globals: 0, imported_memories: 0, imported_tables: 0 },
            component: Some(parsed_component),
        });
    } else {
        return Err(ValidationError::InvalidBinaryFormatVersion);
    };

    let mut header = None;
    read_next_header(&mut wasm_reader, &mut header)?;
    let mut skip_section = |wasm: &mut WasmReader, section_header: &mut Option<SectionHeader>| {
        handle_section(wasm, section_header, SectionTy::Custom, |wasm, h| {
            let _name = wasm.read_name()?;
            let remaining_bytes = h
                .contents
                .from()
                .checked_add(h.contents.len())
                .and_then(|res| res.checked_sub(wasm.pc))
                .ok_or(ValidationError::InvalidCustomSectionLength)?;
            wasm.skip(remaining_bytes)?;
            Ok(())
        })
    };
    while (skip_section(&mut wasm_reader, &mut header)?).is_some() {}
    let types = handle_section(&mut wasm_reader, &mut header, SectionTy::Type, |wasm, _| {
        wasm.read_vec(FuncType::read)
    })?.unwrap_or_default();
    
    while (skip_section(&mut wasm_reader, &mut header)?).is_some() {}
    let imports = handle_section(&mut wasm_reader, &mut header, SectionTy::Import, |wasm, _| {
        wasm.read_vec(|wasm| {
            let import = Import::read(wasm)?;
            match import.desc {
                ImportDesc::Func(type_idx) => {
                    types.get(type_idx).ok_or(ValidationError::InvalidTypeIdx(type_idx))?;
                }
                _ => {}
            }
            Ok(import)
        })
    })?.unwrap_or_default();
    
    let imports_length = get_imports_length(&imports);
    while (skip_section(&mut wasm_reader, &mut header)?).is_some() {}
    let local_functions =
        handle_section(&mut wasm_reader, &mut header, SectionTy::Function, |wasm, _| {
            wasm.read_vec(|wasm| {
                let type_idx = wasm.read_var_u32()? as usize;
                types.get(type_idx).ok_or(ValidationError::InvalidTypeIdx(type_idx))?;
                Ok(type_idx)
            })
        })?.unwrap_or_default();
        
    let all_functions_types = imports.iter().filter_map(|import| match &import.desc {
        ImportDesc::Func(type_idx) => Some(*type_idx),
        _ => None,
    }).chain(local_functions.iter().cloned()).collect::<Vec<TypeIdx>>();
    
    while (skip_section(&mut wasm_reader, &mut header)?).is_some() {}
    let imported_tables = imports.iter().filter_map(|m| match m.desc {
            ImportDesc::Table(table) => Some(table),
            _ => None,
        }).collect::<Vec<TableType>>();
    let tables = handle_section(&mut wasm_reader, &mut header, SectionTy::Table, |wasm, _| {
        wasm.read_vec(TableType::read)
    })?.unwrap_or_default();
    let all_tables = {
        let mut temp = imported_tables;
        temp.extend(tables.clone());
        temp
    };
    
    while (skip_section(&mut wasm_reader, &mut header)?).is_some() {}
    let imported_memories = imports.iter().filter_map(|m| match m.desc {
            ImportDesc::Mem(mem) => Some(mem),
            _ => None,
        }).collect::<Vec<MemType>>();
    let memories = handle_section(&mut wasm_reader, &mut header, SectionTy::Memory, |wasm, _| {
        wasm.read_vec(MemType::read)
    })?.unwrap_or_default();
    let all_memories = {
        let mut temp = imported_memories;
        temp.extend(memories.clone());
        temp
    };
    if all_memories.len() > 1 {
        return Err(ValidationError::UnsupportedMultipleMemoriesProposal);
    }
    
    while (skip_section(&mut wasm_reader, &mut header)?).is_some() {}
    let imported_global_types = imports.iter().filter_map(|m| match m.desc {
            ImportDesc::Global(global) => Some(global),
            _ => None,
        }).collect::<Vec<GlobalType>>();
    let globals = handle_section(&mut wasm_reader, &mut header, SectionTy::Global, |wasm, h| {
        self::globals::validate_global_section(
            wasm,
            h,
            &imported_global_types,
            &mut validation_context_refs,
            all_functions_types.len(),
        )
    })?.unwrap_or_default();
    
    let mut all_globals = Vec::new();
    for item in &imported_global_types {
        all_globals.push(Global { init_expr: Span::new(usize::MAX, 0), ty: *item });
    }
    for item in &globals {
        all_globals.push(*item)
    }
    
    while (skip_section(&mut wasm_reader, &mut header)?).is_some() {}
    let exports = handle_section(&mut wasm_reader, &mut header, SectionTy::Export, |wasm, _| {
        wasm.read_vec(Export::read)
    })?.unwrap_or_default();
    
    validation_context_refs.extend(exports.iter().filter_map(|e| match e.desc {
            ExportDesc::FuncIdx(idx) => Some(idx),
            _ => None,
        }));
        
    while (skip_section(&mut wasm_reader, &mut header)?).is_some() {}
    let start = handle_section(&mut wasm_reader, &mut header, SectionTy::Start, |wasm, _| {
        let func_idx = wasm.read_var_u32().map(|idx| idx as FuncIdx)?;
        let type_idx = *all_functions_types.get(func_idx).ok_or(ValidationError::InvalidFuncIdx(func_idx))?;
        if types[type_idx] != (FuncType {
            params: ResultType { valtypes: Vec::new() },
            returns: ResultType { valtypes: Vec::new() },
        }) {
            Err(ValidationError::InvalidStartFunctionSignature)
        } else {
            Ok(func_idx)
        }
    })?;
    
    while (skip_section(&mut wasm_reader, &mut header)?).is_some() {}
    let elements = handle_section(&mut wasm_reader, &mut header, SectionTy::Element, |wasm, _| {
            ElemType::read_from_wasm(
                wasm,
                &all_functions_types,
                &mut validation_context_refs,
                &all_tables,
                &imported_global_types,
            )
        })?.unwrap_or_default();
        
    while (skip_section(&mut wasm_reader, &mut header)?).is_some() {}
    let data_count = handle_section(&mut wasm_reader, &mut header, SectionTy::DataCount, |wasm, _| {
            wasm.read_var_u32()
        })?;
        
    while (skip_section(&mut wasm_reader, &mut header)?).is_some() {}
    let mut sidetable = Sidetable::new();
    let num_imported_funcs = imports_length.imported_functions;
    let func_blocks_stps = handle_section(&mut wasm_reader, &mut header, SectionTy::Code, |wasm, h| {
        self::code::validate_code_section(
            wasm,
            h,
            &types,
            &all_functions_types,
            num_imported_funcs,
            &all_globals,
            &all_memories,
            &data_count,
            &all_tables,
            &elements,
            &validation_context_refs,
            &mut sidetable,
        )
    })?.unwrap_or_default();
    
    if func_blocks_stps.len() != local_functions.len() {
        return Err(ValidationError::FunctionAndCodeSectionsHaveDifferentLengths);
    }
    
    while (skip_section(&mut wasm_reader, &mut header)?).is_some() {}
    let data_section = handle_section(&mut wasm_reader, &mut header, SectionTy::Data, |wasm, h| {
        self::data::validate_data_section(
            wasm,
            h,
            &imported_global_types,
            all_memories.len(),
            all_functions_types.len(),
        )
    })?.unwrap_or_default();
    
    if let (Some(dc), dl) = (data_count, data_section.len()) {
        if dc as usize != dl {
            return Err(ValidationError::DataCountAndDataSectionsLengthAreDifferent);
        }
    }
    
    while (skip_section(&mut wasm_reader, &mut header)?).is_some() {}
    if let Some(h) = header {
        return Err(ValidationError::SectionOutOfOrder(h.ty));
    }
    
    let validation_info = ValidationInfo {
        wasm: wasm_reader.into_inner(),
        types,
        imports,
        functions: local_functions,
        tables,
        memories,
        globals,
        functions_types: all_functions_types,
        exports,
        func_blocks_stps,
        sidetable,
        data: data_section,
        start,
        elements,
        imports_length,
        component: None,
    };
    validate_exports(&validation_info)?;
    Ok(validation_info)
}

fn read_next_header(wasm: &mut WasmReader, header: &mut Option<SectionHeader>) -> Result<(), ValidationError> {
    if header.is_none() && !wasm.remaining_bytes().is_empty() {
        *header = Some(SectionHeader::read(wasm)?);
    }
    Ok(())
}

fn handle_section<T, F: FnOnce(&mut WasmReader, SectionHeader) -> Result<T, ValidationError>>(
    wasm: &mut WasmReader,
    header: &mut Option<SectionHeader>,
    section_ty: SectionTy,
    handler: F,
) -> Result<Option<T>, ValidationError> {
    match header {
        Some(h) if h.ty == section_ty => {
            let h = header.take().unwrap();
            let start_pc = wasm.pc;
            let contents_len = h.contents.len();
            let ret = handler(wasm, h)?;
            let consumed = wasm.pc - start_pc;
            if consumed < contents_len {
                wasm.skip(contents_len - consumed)?;
            }
            read_next_header(wasm, header)?;
            Ok(Some(ret))
        }
        _ => Ok(None),
    }
}
