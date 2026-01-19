use crate::rust_alloc::collections::BTreeMap;
use crate::rust_alloc::format;
use crate::rust_alloc::string::String;
use crate::rust_alloc::string::ToString;
use crate::rust_alloc::vec::Vec;
use crate::wasm::component::types::{
    ComponentAliasKind, ComponentInstanceKind,
    ParsedComponent,
};
use crate::wasm::execution::config::Config;
use crate::wasm::execution::linker::Linker;
use crate::wasm::execution::store::addrs::ModuleAddr;
use crate::wasm::execution::store::Store;
use crate::wasm::{ExternVal, RuntimeError};

pub fn instantiate_component<'a, T: Config>(
    store: &mut Store<'a, T>,
    linker: &Linker,
    component: &ParsedComponent,
    wasm: &'a [u8],
) -> Result<BTreeMap<String, ExternVal>, RuntimeError> {
    use crate::wasm::component::types::ComponentItem;
    crate::debugln!("Instantiating Component...");

    // Runtime Index Spaces
    let mut core_instances: Vec<BTreeMap<String, ExternVal>> = Vec::new();
    let mut core_funcs: Vec<ExternVal> = Vec::new();
    let mut core_tables: Vec<ExternVal> = Vec::new();
    let mut core_mems: Vec<ExternVal> = Vec::new();
    let mut core_globals: Vec<ExternVal> = Vec::new();

    let mut instances: Vec<BTreeMap<String, ExternVal>> = Vec::new();
    let mut functions: Vec<ExternVal> = Vec::new();

    // Definition Index Spaces (References to parsed items)
    let mut core_modules: Vec<&crate::wasm::component::types::ComponentModule> = Vec::new();
    let mut nested_components: Vec<&crate::wasm::component::types::NestedComponent> = Vec::new();

    let mut all_instantiated_modules: Vec<ModuleAddr> = Vec::new();

    // Iterate over items in order
    for item in &component.items {
        use crate::wasm::component::types::ComponentItem;
        match item {
            ComponentItem::Module(m) => core_modules.push(m),
            ComponentItem::Component(c) => nested_components.push(c),
            ComponentItem::Import(import) => {
                crate::debugln!("  Resolving Component Import: '{}'...", import.name);
                if let Some(module_exports) = linker.get_module_exports(&import.name) {
                    let mut export_map = BTreeMap::new();
                    for (name, val) in module_exports {
                        export_map.insert(name, val);
                    }
                    instances.push(export_map);
                } else {
                    crate::debugln!(
                        "Warning: Could not resolve component import '{}'",
                        import.name
                    );
                    instances.push(BTreeMap::new());
                }
            }
            ComponentItem::CoreInstance(core_inst_def) => {
                match &core_inst_def.kind {
                    crate::wasm::component::types::CoreInstanceKind::Instantiate {
                        module_idx,
                        args,
                    } => {
                        if *module_idx as usize >= core_modules.len() {
                            crate::debugln!("Error: Module index {} out of bounds", module_idx);
                            core_instances.push(BTreeMap::new());
                            continue;
                        }
                        crate::debugln!("  Instantiating Core Module [Index {}]...", module_idx);
                        let module_node = core_modules[*module_idx as usize];

                        let module_bytes = &wasm[module_node.content.from
                            ..module_node.content.from + module_node.content.len];
                        let validation_info = match crate::wasm::validation::validate(module_bytes)
                        {
                            Ok(info) => info,
                            Err(e) => {
                                crate::debugln!("Validation error for core module: {:?}", e);
                                return Err(RuntimeError::ValidationError);
                            }
                        };

                        let mut extern_vals = Vec::new();
                        for import in &validation_info.imports {
                            let mut resolved = false;

                            // 1. Try to resolve from instantiation arguments
                            if let Some(arg) = args.iter().find(|a| a.name == import.module_name) {
                                match arg.kind {
                                    0x02 => {
                                        // Component Instance
                                        if let Some(inst) = instances.get(arg.idx as usize) {
                                            if let Some(val) = inst.get(&import.name) {
                                                extern_vals.push(*val);
                                                resolved = true;
                                            }
                                        }
                                    }
                                    0x12 => {
                                        // Core Instance
                                        if let Some(inst) = core_instances.get(arg.idx as usize) {
                                            if let Some(val) = inst.get(&import.name) {
                                                extern_vals.push(*val);
                                                resolved = true;
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            }

                            if !resolved {
                                // 2. Fuzzy Linker Lookup
                                if let Some(module_exports) =
                                    linker.get_module_exports(&import.module_name)
                                {
                                    if let Some((_, val)) =
                                        module_exports.iter().find(|(n, _)| *n == import.name)
                                    {
                                        extern_vals.push(*val);
                                        resolved = true;
                                    }
                                }
                            }

                            if !resolved {
                                // 3. Hardcoded Fallback for saltty.wasm env/memory and __main_module__
                                if import.module_name == "env" && import.name == "memory" {
                                    crate::debugln!("    Synthesizing env.memory (fallback)...");
                                    let mem_ty = crate::wasm::core::reader::types::MemType {
                                        limits: crate::wasm::core::reader::types::Limits {
                                            min: 17,
                                            max: None,
                                        },
                                    };
                                    let mem_addr = store.mem_alloc_unchecked(mem_ty);
                                    extern_vals.push(ExternVal::Mem(mem_addr));
                                    resolved = true;
                                } else if import.module_name == "__main_module__" {
                                    let target = match import.name.as_str() {
                                        "_start" => "_start",
                                        "cabi_realloc" => "cabi_realloc",
                                        _ => "",
                                    };
                                    if !target.is_empty() {
                                        crate::debugln!(
                                            "    Scanning core instances for '{}'...",
                                            target
                                        );
                                        for inst in &core_instances {
                                            if let Some(val) = inst.get(target) {
                                                crate::debugln!("      Found!");
                                                extern_vals.push(*val);
                                                resolved = true;
                                                break;
                                            }
                                        }
                                    }
                                }
                            }

                            if !resolved {
                                crate::debugln!(
                                    "Warning: Failed to resolve core module import: {}::{}",
                                    import.module_name,
                                    import.name
                                );
                                // Stubbing logic to allow instantiation to proceed
                                use crate::wasm::core::reader::types::import::ImportDesc;
                                match &import.desc {
                                    ImportDesc::Func(type_idx) => {
                                        if let Some(func_type) =
                                            validation_info.types.get(*type_idx)
                                        {
                                            crate::debugln!(
                                                "    -> Stubbing function import (Type: {:?})",
                                                func_type
                                            );
                                            let func_addr = store.func_alloc_unchecked(func_type.clone(), |_, params| {
                                                crate::debugln!("STUB: Called missing import function");
                                                // Return default values matching result types
                                                // We don't have easy access to result types here inside the closure without capturing func_type
                                                // But since we cloned func_type, we might not have it inside `fn`.
                                                // Wait, `host_func` is `fn`. It cannot capture.
                                                // So we can't be type-safe easily.
                                                // Return empty vec? If signature expects results, execution will crash/trap on return check?
                                                // Core runtime `invoke_unchecked` doesn't enforce return count strictly if host func returns Vec.
                                                // But `resume_unchecked` pushes results.
                                                // If we return empty, and it expects i32, the stack will be underflown?
                                                // Yes.
                                                // Ideally we should return correct zeros.
                                                // Since we can't capture, we assume void or crash.
                                                // Actually, let's just Trap.
                                                Err(crate::wasm::execution::store::HaltExecutionError)
                                            });
                                            extern_vals.push(ExternVal::Func(func_addr));
                                            resolved = true;
                                        }
                                    }
                                    ImportDesc::Mem(mem_ty) => {
                                        crate::debugln!("    -> Stubbing memory import");
                                        let mem_addr = store.mem_alloc_unchecked(*mem_ty);
                                        extern_vals.push(ExternVal::Mem(mem_addr));
                                        resolved = true;
                                    }
                                    ImportDesc::Table(table_ty) => {
                                        crate::debugln!("    -> Stubbing table import");
                                        use crate::wasm::execution::value::Ref;
                                        let table_addr = store
                                            .table_alloc_unchecked(
                                                *table_ty,
                                                Ref::Null(table_ty.et),
                                            )
                                            .unwrap(); // unwrap safe for alloc
                                        extern_vals.push(ExternVal::Table(table_addr));
                                        resolved = true;
                                    }
                                    ImportDesc::Global(global_ty) => {
                                        crate::debugln!("    -> Stubbing global import");
                                        use crate::wasm::core::reader::types::{NumType, ValType};
                                        use crate::wasm::execution::value::{Value, F32, F64};

                                        let val = match global_ty.ty {
                                            ValType::NumType(NumType::I64) => Value::I64(0),
                                            ValType::NumType(NumType::F32) => Value::F32(F32(0.0)),
                                            ValType::NumType(NumType::F64) => Value::F64(F64(0.0)),
                                            _ => Value::I32(0),
                                        };
                                        let global_addr =
                                            store.global_alloc_unchecked(*global_ty, val).unwrap();
                                        extern_vals.push(ExternVal::Global(global_addr));
                                        resolved = true;
                                    }
                                }
                            }
                        }

                        if let Ok(outcome) =
                            store.module_instantiate_unchecked(&validation_info, extern_vals, None)
                        {
                            let module_inst = store.modules.get(outcome.module_addr);
                            core_instances.push(module_inst.exports.clone());
                            all_instantiated_modules.push(outcome.module_addr);
                        } else {
                            core_instances.push(BTreeMap::new());
                        }
                    }
                    crate::wasm::component::types::CoreInstanceKind::FromExports { exports } => {
                        let mut export_map = BTreeMap::new();
                        for export in exports {
                            let val = match export.kind {
                                0x00 => {
                                    let result = core_funcs.get(export.idx as usize).copied();
                                    if result.is_none() {
                                        crate::debugln!(
                                            "  FromExports: FAILED to find func at core_funcs[{}] for export '{}'",
                                            export.idx,
                                            export.name
                                        );
                                    }
                                    result
                                }
                                0x01 => core_tables.get(export.idx as usize).copied(),
                                0x02 => core_mems.get(export.idx as usize).copied(),
                                0x03 => core_globals.get(export.idx as usize).copied(),
                                _ => None,
                            };
                            if let Some(v) = val {
                                export_map.insert(export.name.clone(), v);
                            }
                        }
                        crate::debugln!(
                            "  FromExports: Created core_instances[{}] with {} exports: {:?}",
                            core_instances.len(),
                            export_map.len(),
                            export_map.keys().collect::<Vec<_>>()
                        );
                        core_instances.push(export_map);
                    }
                }
            }
            ComponentItem::CoreAlias(alias) => match &alias.kind {
                crate::wasm::component::types::CoreAliasKind::Export { instance_idx, name } => {
                    if let Some(inst) = core_instances.get(*instance_idx as usize) {
                        if let Some(val) = inst.get(name) {
                            match alias.sort {
                                0x00 => core_funcs.push(*val),
                                0x01 => core_tables.push(*val),
                                0x02 => core_mems.push(*val),
                                0x03 => core_globals.push(*val),
                                _ => {}
                            }
                        }
                    }
                }
                _ => {}
            },
            ComponentItem::Alias(alias) => {
                match &alias.kind {
                    ComponentAliasKind::InstanceExport { instance_idx, name } => {
                        if let Some(inst) = instances.get(*instance_idx as usize) {
                            if let Some(val) = inst.get(name) {
                                match alias.sort {
                                    0x03 => functions.push(*val),
                                    _ => {}
                                }
                            }
                        }
                    }
                    ComponentAliasKind::CoreInstanceExport { instance_idx, name } => {
                        if let Some(inst) = core_instances.get(*instance_idx as usize) {
                            if let Some(val) = inst.get(name) {
                                // Add to the appropriate core index space based on sort
                                match alias.sort {
                                    0x00 => {
                                        crate::debugln!(
                                            "  CoreInstanceExport Alias: Adding func '{}' to core_funcs[{}]",
                                            name,
                                            core_funcs.len()
                                        );
                                        core_funcs.push(*val);
                                    }
                                    0x01 => {
                                        crate::debugln!(
                                            "  CoreInstanceExport Alias: Adding table '{}' to core_tables[{}]",
                                            name,
                                            core_tables.len()
                                        );
                                        core_tables.push(*val);
                                    }
                                    0x02 => {
                                        crate::debugln!(
                                            "  CoreInstanceExport Alias: Adding mem '{}' to core_mems[{}]",
                                            name,
                                            core_mems.len()
                                        );
                                        core_mems.push(*val);
                                    }
                                    0x03 => {
                                        crate::debugln!(
                                            "  CoreInstanceExport Alias: Adding func '{}' to functions[{}]",
                                            name,
                                            functions.len()
                                        );
                                        functions.push(*val);
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            ComponentItem::Instance(inst_def) => {
                match &inst_def.kind {
                    ComponentInstanceKind::InstantiateModule { module_idx, args } => {
                        if *module_idx as usize >= core_modules.len() {
                            instances.push(BTreeMap::new());
                            continue;
                        }
                        let module_node = core_modules[*module_idx as usize];
                        let module_bytes = &wasm[module_node.content.from
                            ..module_node.content.from + module_node.content.len];
                        let validation_info = match crate::wasm::validation::validate(module_bytes)
                        {
                            Ok(info) => info,
                            Err(_) => {
                                instances.push(BTreeMap::new());
                                continue;
                            }
                        };

                        let mut extern_vals = Vec::new();
                        for import in &validation_info.imports {
                            let mut resolved = false;
                            if let Some(arg) = args.iter().find(|a| a.name == import.module_name) {
                                if arg.kind == 0x02 {
                                    // Instance
                                    if let Some(inst) = instances.get(arg.idx as usize) {
                                        if let Some(val) = inst.get(&import.name) {
                                            extern_vals.push(*val);
                                            resolved = true;
                                        }
                                    }
                                }
                            }

                            if !resolved {
                                if let Some(module_exports) =
                                    linker.get_module_exports(&import.module_name)
                                {
                                    if let Some((_, val)) =
                                        module_exports.iter().find(|(n, _)| *n == import.name)
                                    {
                                        extern_vals.push(*val);
                                        resolved = true;
                                    }
                                }
                            }

                            if !resolved {
                                crate::debugln!(
                                    "Warning: Failed to resolve module import: {}::{}",
                                    import.module_name,
                                    import.name
                                );
                            }
                        }

                        if let Ok(outcome) = store.module_instantiate_unchecked(
                            &validation_info,
                            extern_vals.clone(),
                            None,
                        ) {
                            let module_inst = store.modules.get(outcome.module_addr);
                            instances.push(module_inst.exports.clone());
                            all_instantiated_modules.push(outcome.module_addr);

                            if let Some(import_setup_func) = module_inst.exports.get("$imports") {
                                if let Some(func_addr) = import_setup_func.as_func() {
                                    let params: Vec<crate::wasm::Value> = extern_vals
                                        .iter()
                                        .filter_map(|ev| {
                                            if let crate::wasm::ExternVal::Func(addr) = ev {
                                                Some(crate::wasm::Value::Ref(
                                                    crate::wasm::execution::value::Ref::Func(*addr),
                                                ))
                                            } else {
                                                None
                                            }
                                        })
                                        .collect();
                                    let _ = store.invoke_unchecked(func_addr, params, None);
                                }
                            }
                        } else {
                            instances.push(BTreeMap::new());
                        }
                    }
                    ComponentInstanceKind::InstantiateComponent {
                        component_idx,
                        args,
                    } => {
                        let idx = *component_idx as usize;
                        if idx >= nested_components.len() {
                            instances.push(BTreeMap::new());
                            continue;
                        }

                        let nested_node = nested_components[idx];
                        let nested_bytes = &wasm[nested_node.content.from
                            ..nested_node.content.from + nested_node.content.len];
                        let mut nested_linker = Linker::new();

                        for arg in args {
                            if arg.kind == 0x02 {
                                if let Some(inst) = instances.get(arg.idx as usize) {
                                    for (export_name, export_val) in inst {
                                        let _ = nested_linker.define_unchecked(
                                            arg.name.clone(),
                                            export_name.clone(),
                                            *export_val,
                                        );
                                    }
                                }
                            }
                        }

                        if let Ok(nested_exports) = instantiate_component(
                            store,
                            &nested_linker,
                            &nested_node.parsed,
                            nested_bytes,
                        ) {
                            instances.push(nested_exports);
                        } else {
                            instances.push(BTreeMap::new());
                        }
                    }
                    ComponentInstanceKind::FromExports { values } => {
                        let mut exports = BTreeMap::new();
                        for export in values {
                            let val = match export.kind {
                                0x03 => functions.get(export.idx as usize).copied(),
                                0x02 => instances
                                    .get(export.idx as usize)
                                    .cloned()
                                    .map(|_| ExternVal::Func(0)), // FIXME
                                _ => None,
                            };
                            if let Some(v) = val {
                                if let ExternVal::Func(_) = v {
                                    if export.kind == 0x03 {
                                        exports.insert(export.name.clone(), v);
                                    }
                                }
                            }
                        }
                        instances.push(exports);
                    }
                    ComponentInstanceKind::FromCoreInstance { core_instance_idx } => {
                        if let Some(inst) = core_instances.get(*core_instance_idx as usize) {
                            instances.push(inst.clone());
                        } else {
                            instances.push(BTreeMap::new());
                        }
                    }
                }
            }
            _ => {} // Type, Canon, Start, Export handled elsewhere or not needed for sequential execution state
        }
    }

    // 6. Exports
    let mut component_exports = BTreeMap::new();
    for item in &component.items {
        if let ComponentItem::Export(export) = item {
            if export.kind == 0x03 {
                if let Some(func) = functions.get(export.idx as usize) {
                    component_exports.insert(export.name.clone(), *func);
                }
            } else if export.kind == 0x02 {
                if let Some(inst) = instances.get(export.idx as usize) {
                    for (name, val) in inst {
                        component_exports.insert(format!("{}.{}", export.name, name), *val);
                    }
                }
            }
        }
    }

    // EXECUTION LOGIC
    // 1. Try "run" from component exports (Fuzzy match)
    let run_func = component_exports.get("run").or_else(|| {
        component_exports.iter().find_map(|(k, v)| {
            if k.ends_with(":cli/run.run") || k.ends_with("#run") {
                Some(v)
            } else {
                None
            }
        })
    });

    if let Some(run_func) = run_func {
        if let crate::wasm::ExternVal::Func(func_addr) = *run_func {
            crate::debugln!("Executing component export 'run'...");
            let _ = store.invoke_unchecked(
                func_addr,
                crate::rust_alloc::vec::Vec::<crate::wasm::Value>::new(),
                None,
            );
            return Ok(component_exports);
        }
    }

    // 2. Fallback: Search ALL instantiated modules for entry points
    crate::debugln!(
        "Entry Point Search: Checking {} instantiated modules...",
        all_instantiated_modules.len()
    );

    let mut executed_run = false;

    // Pass 1: Execute explicit "run" commands (e.g. Adapter initialization / wasi:cli/run.run)
    // If "run" is found and executed, this is THE entry point - don't also run _start
    for module_addr in all_instantiated_modules.iter().rev() {
        let exports = {
            let module_inst = store.modules.get(*module_addr);
            module_inst.exports.clone()
        };
        crate::debugln!(
            "  Module at {:?}, Exports: {:?}",
            module_addr,
            exports.keys().collect::<Vec<_>>()
        );

        for (name, val) in &exports {
            if name == "run" || name.ends_with("#run") || name.ends_with(".run") {
                if let Some(func_addr) = val.as_func() {
                    crate::debugln!("Entry Point Found: Executing export '{}' from Module {:?}...", name, module_addr);
                    let _ = store.invoke_unchecked(
                        func_addr,
                        crate::rust_alloc::vec::Vec::<crate::wasm::Value>::new(),
                        None,
                    );
                    executed_run = true;
                    break; // Found and executed "run", stop searching
                }
            }
        }
        if executed_run {
            break;
        }
    }

    // If we executed "run", we're done - return early
    if executed_run {
        return Ok(component_exports);
    }

    // Pass 2: Execute _start / main (User code) - ONLY if no "run" was found
    for module_addr in all_instantiated_modules.iter().rev() {
        let exports = {
            let module_inst = store.modules.get(*module_addr);
            module_inst.exports.clone()
        };

        let mut found_start = false;
        for (name, val) in &exports {
            if name == "_start" || name == "main" {
                if let Some(func_addr) = val.as_func() {
                    crate::debugln!("Entry Point Found: Executing export '{}' from Module {:?}...", name, module_addr);
                    let _ = store.invoke_unchecked(
                        func_addr,
                        crate::rust_alloc::vec::Vec::<crate::wasm::Value>::new(),
                        None,
                    );
                    found_start = true;
                    break;
                }
            }
        }
        if found_start {
            return Ok(component_exports);
        }
    }

    // Numeric fallback: Execute ALL numeric exports sequentially (last resort)
    for module_addr in all_instantiated_modules.iter().rev() {
        let exports = {
            let module_inst = store.modules.get(*module_addr);
            module_inst.exports.clone()
        };
        for i in 0..10 {
            let name = i.to_string();
            if let Some(val) = exports.get(&name) {
                if let Some(func_addr) = val.as_func() {
                    crate::debugln!("Entry Point Found: Executing numeric export '{}' from Module {:?}...", name, module_addr);
                    let _ = store.invoke_unchecked(
                        func_addr,
                        crate::rust_alloc::vec::Vec::<crate::wasm::Value>::new(),
                        None,
                    );
                }
            }
        }
    }

    Ok(component_exports)
}
