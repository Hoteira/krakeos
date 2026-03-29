extern crate alloc;
use crate::debugln;
use crate::fs::File;
use crate::io::Read;
use crate::wasm::wasi::{WasiCtx, create_wasi_imports, create_wasi_p2_imports};
use crate::wasm::container::{register_container, unregister_container};
use crate::process::get_pid;
use crate::wasm::common::config::Config;
use crate::wasm::interpreter::resumable::RunState;
use crate::wasm::{Linker, Store, validate, Value};
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

pub struct Ring3AotInfo {
    pub entry_addr: u64,
    pub ctx_ptr: u64,
    pub stack_base: u64,
    pub stack_limit: u64,
    pub module_addr: usize,
}

pub enum WasmRunResult {
    Finished(i32),
    AotReady(Ring3AotInfo),
}

pub fn run(path: &str, root_path: &str, fds: &[(u8, u8)], aot: bool) -> WasmRunResult {
    run_with_args(path, vec![path.to_string()], root_path, fds, aot)
}

pub fn run_with_args(
    path: &str,
    args: Vec<String>,
    root_path: &str,
    fds: &[(u8, u8)],
    aot: bool,
) -> WasmRunResult {
    run_with_env(path, args, root_path, fds, Vec::new(), aot)
}

pub fn run_in_container<'a, T: Config>(
    name: &str,
    buffer: &'a [u8],
    linker: &mut Linker,
    store: &mut Store<'a, T>,
) -> WasmRunResult {
    unsafe {
        crate::wasm::wasi::ICRNL = true;
    }
    let res: Result<WasmRunResult, crate::wasm::RuntimeError> = match validate(buffer) {
        Ok(validation_info) => {
            if let Some(component) = &validation_info.component {
                crate::wasm::interpreter::component_executor::instantiate_component(
                    store, linker, component, buffer,
                )
                .map(|_| WasmRunResult::Finished(0))
            } else {
                linker
                    .module_instantiate_unchecked(store, &validation_info, None, 0)
                    .and_then(|instance| handle_instantiation_result(store, instance))
            }
        }
        Err(e) => {
            crate::debugln!("[wasm-runner] Validation error: {:?}", e);
            Ok(WasmRunResult::Finished(1))
        }
    };

    let final_res = match res {
        Ok(r) => r,
        Err(crate::wasm::RuntimeError::HostFunctionHaltedExecution(code)) => WasmRunResult::Finished(code),
        Err(e) => {
            crate::debugln!("[wasm-runner] Execution error in {}: {:?}", name, e);
            WasmRunResult::Finished(1)
        }
    };
    unsafe {
        crate::wasm::wasi::ICRNL = false;
    }
    final_res
}

fn handle_instantiation_result<'a, T: Config>(
    store: &mut Store<'a, T>,
    instance: crate::wasm::interpreter::store::InstantiationOutcome,
) -> Result<WasmRunResult, crate::wasm::RuntimeError> {
    if store.aot_enabled && instance.maybe_ctx_ptr.is_some() {
        crate::debugln!("[AOT-Runner] AOT enabled, looking for entry point...");
        let entry_point = store
            .instance_export_unchecked(instance.module_addr, "wasi:cli/run@0.2.0#run")
            .ok()
            .and_then(|e| e.as_func())
            .or_else(|| {
                store
                    .instance_export_unchecked(instance.module_addr, "__main_void")
                    .ok()
                    .and_then(|e| e.as_func())
            })
            .or_else(|| {
                store
                    .instance_export_unchecked(instance.module_addr, "run")
                    .ok()
                    .and_then(|e| e.as_func())
            })
            .or_else(|| {
                store
                    .instance_export_unchecked(instance.module_addr, "_start")
                    .ok()
                    .and_then(|e| e.as_func())
            });

        if let Some(func_addr) = entry_point {
            crate::debugln!("[AOT-Runner] Found entry point at addr {}", func_addr);
            let wasm_func = match store.functions.get(func_addr) {
                crate::wasm::interpreter::store::FuncInst::WasmFunc(f) => f,
                _ => {
                    return Err(crate::wasm::RuntimeError::Trap(
                        crate::wasm::TrapError::ReachedUnreachable,
                    ))
                }
            };
            crate::debugln!("[AOT-Runner] AOT ptr: {:#x}", wasm_func.aot_ptr.unwrap_or(0));
            return Ok(WasmRunResult::AotReady(Ring3AotInfo {
                entry_addr: wasm_func.aot_ptr.unwrap_or(0) as u64,
                ctx_ptr: instance.maybe_ctx_ptr.unwrap(),
                stack_base: store.stack_base,
                stack_limit: store.stack_limit,
                module_addr: instance.module_addr,
            }));
        } else {
            crate::debugln!("[AOT-Runner] No AOT entry point found.");
        }
    }

    let entry_point = store
        .instance_export_unchecked(instance.module_addr, "wasi:cli/run@0.2.0#run")
        .ok()
        .and_then(|e| e.as_func())
        .or_else(|| {
            store
                .instance_export_unchecked(instance.module_addr, "__main_void")
                .ok()
                .and_then(|e| e.as_func())
        })
        .or_else(|| {
            store
                .instance_export_unchecked(instance.module_addr, "run")
                .ok()
                .and_then(|e| e.as_func())
        })
        .or_else(|| {
            store
                .instance_export_unchecked(instance.module_addr, "_start")
                .ok()
                .and_then(|e| e.as_func())
        });

    if let Some(func_addr) = entry_point {
        store.invoke_unchecked(func_addr, Vec::new(), None).map(|run_res| {
            if let RunState::Finished { values, .. } = run_res {
                if let Some(Value::I32(val)) = values.first() {
                    return WasmRunResult::Finished(*val as i32);
                }
            }
            WasmRunResult::Finished(0)
        })
    } else {
        Ok(WasmRunResult::Finished(0))
    }
}

/// Derive the `.wacc` cache path from a `.wasm` path.
fn wacc_path(wasm_path: &str) -> String {
    if wasm_path.ends_with(".wasm") {
        let mut s = String::from(&wasm_path[..wasm_path.len() - 5]);
        s.push_str(".wacc");
        s
    } else {
        let mut s = String::from(wasm_path);
        s.push_str(".wacc");
        s
    }
}

/// Try to load a `.wacc` cache.  Returns `Some(WaccInfo)` on cache hit.
/// `wasm_bytes` is the original `.wasm` file content, needed to reconstruct data segments.
fn try_load_wacc(wasm_path: &str, wasm_mtime: u64, wasm_size: u64, wasm_bytes: &[u8]) -> Option<crate::wasm::aot::wacc::WaccInfo> {
    let wp = wacc_path(wasm_path);
    let mut file = crate::fs::File::open(&wp).ok()?;
    let size = file.size();
    if size < 40 { return None; } // smaller than header
    let mut buf = vec![0u8; size];
    crate::io::Read::read(&mut file, &mut buf).ok()?;
    if !crate::wasm::aot::wacc::wacc_header_matches(&buf, wasm_mtime, wasm_size) {
        debugln!("[WACC] Cache stale for {}", wasm_path);
        return None;
    }
    debugln!("[WACC] Cache hit for {}", wasm_path);
    crate::wasm::aot::wacc::deserialize_wacc(&buf, wasm_bytes)
}

/// Save a `.wacc` cache file next to the `.wasm` source.
fn save_wacc(
    wasm_path: &str,
    aot: &crate::wasm::aot::runtime::AotModule,
    vi: &crate::wasm::common::validation::ValidationInfo,
    wasm_mtime: u64,
    wasm_size: u64,
    global_init_vals: &[crate::wasm::Value],
    data_offsets: &[Option<i32>],
    elem_offsets: &[Option<i32>],
) {
    crate::debugln!("save_wacc: Start serialization...");
    let bytes = crate::wasm::aot::wacc::WaccSerializer::serialize(
        aot, vi, wasm_mtime, wasm_size, global_init_vals, data_offsets, elem_offsets,
    );
    let wp = wacc_path(wasm_path);
    crate::debugln!("save_wacc: Finished serialization. About to create file {}...", wp);
    if let Ok(mut file) = crate::fs::File::create(&wp) {
        crate::debugln!("save_wacc: File::create returned. About to start Write::write of {} bytes...", bytes.len());
        if crate::io::Write::write(&mut file, &bytes).is_ok() {
            debugln!("[WACC] Saved cache {} ({} KB)", wp, bytes.len() / 1024);
        } else {
            crate::debugln!("save_wacc: Write::write failed!");
        }
    } else {
        crate::debugln!("save_wacc: File::create failed!");
    }
}

/// Get mtime and size for a wasm file (from a buffer and path).
fn wasm_file_stat(path: &str) -> (u64, u64) {
    if let Ok(file) = crate::fs::File::open(path) {
        if let Ok(st) = file.stat() {
            return (st.mtime, st.size);
        }
    }
    (0, 0)
}

pub fn run_with_buffer(
    name: &str,
    buffer: &[u8],
    mut args: Vec<String>,
    root_path: &str,
    fds: &[(u8, u8)],
    env_vars: Vec<(String, String)>,
    aot: bool,
    container_id: u64,
    slot_id: u16,
) -> WasmRunResult {
    debugln!("[wasm-runner] Starting buffer {} (AOT: {})...", name, aot);

    // Parse runtime-specific arguments
    let mut actual_root_path = root_path.to_string();
    let mut overrides: Vec<(usize, String)> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--dir" && i + 1 < args.len() {
            args.remove(i);
            actual_root_path = args.remove(i);
        } else if args[i].starts_with("--arg") && args[i].contains('=') {
            let eq_idx = args[i].find('=').unwrap();
            if let Ok(n) = args[i][5..eq_idx].parse::<usize>() {
                let val = args[i][eq_idx + 1..].to_string();
                args.remove(i);
                overrides.push((n, val));
            } else {
                i += 1;
            }
        } else if args[i].starts_with("--arg") {
            if let Ok(n) = args[i][5..].parse::<usize>() {
                if i + 1 < args.len() {
                    args.remove(i);
                    let val = args.remove(i);
                    overrides.push((n, val));
                } else {
                    i += 1;
                }
            } else {
                i += 1;
            }
        } else {
            i += 1;
        }
    }

    for (n, val) in overrides {
        while args.len() <= n {
            args.push(String::new());
        }
        args[n] = val;
    }

    unsafe {
        crate::wasm::wasi::ICRNL = true;
    }

    // --- .wacc cache fast path ---
    let (wasm_mtime, wasm_size) = wasm_file_stat(name);
    let wacc_info = if aot && wasm_mtime != 0 {
        try_load_wacc(name, wasm_mtime, wasm_size, buffer)
    } else {
        None
    };

    let res: Result<WasmRunResult, crate::wasm::RuntimeError> = if let Some(wacc) = wacc_info {
        // ── Cache hit: skip validate + compile ──────────────────
        let mut store = Store::new(());
        store.aot_enabled = true;

        let mut slot_info = crate::os::SlotInfo {
            slot_id, linear_memory_base: 0, linear_memory_size: 0, code_base: 0, stack_base: 0,
        };
        #[cfg(not(feature = "userland"))]
        {
            const CODE_REGION_BASE: u64  = 0x0000_0001_0000_0000;
            const CODE_SLOT_SIZE: u64    = 64 * 1024 * 1024;
            const STACK_REGION_BASE: u64 = 0x0000_0041_0000_0000;
            const STACK_SLOT_SIZE: u64   = 16 * 1024 * 1024;
            const LINEAR_MEMORY_REGION_BASE: u64 = 0x0000_0051_2000_0000;
            const LINEAR_MEMORY_SLOT_SIZE: u64   = 31 * 1024 * 1024 * 1024;

            slot_info.linear_memory_base = LINEAR_MEMORY_REGION_BASE + (slot_id as u64) * LINEAR_MEMORY_SLOT_SIZE;
            slot_info.code_base = CODE_REGION_BASE + (slot_id as u64) * CODE_SLOT_SIZE;
            slot_info.stack_base = STACK_REGION_BASE + (slot_id as u64) * STACK_SLOT_SIZE;
        }
        #[cfg(feature = "userland")]
        crate::os::process_get_slot_info(&mut slot_info as *mut _ as *mut u8);

        let sas_base = Some(slot_info.linear_memory_base);
        store.sas_memory_base = sas_base;
        store.code_base = Some(slot_info.code_base);
        store.stack_base = slot_info.stack_base + 16 * 1024 * 1024;
        store.stack_limit = slot_info.stack_base;

        let initial_mem_size = wacc.memories.get(0).map(|m| m.limits.min * 65536).unwrap_or(0);
        register_container(
            container_id, slot_info.slot_id, None,
            sas_base.unwrap_or(0), initial_mem_size as u64,
            4 * 1024 * 1024 * 1024, slot_info.code_base, slot_info.stack_base,
        );
        store.container_id = Some(container_id);

        let mut linker = Linker::new();
        create_wasi_imports(&mut linker, &mut store);
        create_wasi_p2_imports(&mut linker, &mut store);
        store.wasi_ctx = Some(WasiCtx::new_with_env(args, actual_root_path, fds, env_vars));

        // Resolve imports through the linker
        let extern_vals: Vec<crate::wasm::interpreter::store::ExternVal> = wacc.imports.iter()
            .filter_map(|imp| linker.get_unchecked(imp.module_name.clone(), imp.name.clone()))
            .collect();

        let result = store.module_instantiate_from_wacc(&wacc, buffer, extern_vals, slot_id)
            .and_then(|instance| handle_instantiation_result(&mut store, instance));

        if let Ok(WasmRunResult::AotReady(ref info)) = result {
            let ctx = unsafe { &mut *(info.ctx_ptr as *mut crate::wasm::aot::runtime::Ring3Context) };
            ctx.module_addr = info.module_addr;
            let store_raw = alloc::boxed::Box::into_raw(alloc::boxed::Box::new(store));
            ctx.store = store_raw as *mut usize;
        }

        result
    } else {
        // ── Cache miss: normal validate + compile path ──────────
        match validate(buffer) {
        Ok(validation_info) => {
            let mut store = Store::new(());
            store.aot_enabled = aot;

            let mut slot_info = crate::os::SlotInfo {
                slot_id, linear_memory_base: 0, linear_memory_size: 0, code_base: 0, stack_base: 0,
            };
            #[cfg(not(feature = "userland"))]
            {
                const CODE_REGION_BASE: u64  = 0x0000_0001_0000_0000;
                const CODE_SLOT_SIZE: u64    = 64 * 1024 * 1024;
                const STACK_REGION_BASE: u64 = 0x0000_0041_0000_0000;
                const STACK_SLOT_SIZE: u64   = 16 * 1024 * 1024;
                const LINEAR_MEMORY_REGION_BASE: u64 = 0x0000_0051_2000_0000;
                const LINEAR_MEMORY_SLOT_SIZE: u64   = 31 * 1024 * 1024 * 1024;

                slot_info.linear_memory_base = LINEAR_MEMORY_REGION_BASE + (slot_id as u64) * LINEAR_MEMORY_SLOT_SIZE;
                slot_info.code_base = CODE_REGION_BASE + (slot_id as u64) * CODE_SLOT_SIZE;
                slot_info.stack_base = STACK_REGION_BASE + (slot_id as u64) * STACK_SLOT_SIZE;
            }
            #[cfg(feature = "userland")]
            crate::os::process_get_slot_info(&mut slot_info as *mut _ as *mut u8);

            let sas_base = Some(slot_info.linear_memory_base);
            store.sas_memory_base = sas_base;
            store.code_base = Some(slot_info.code_base);
            store.stack_base = slot_info.stack_base + 16 * 1024 * 1024;
            store.stack_limit = slot_info.stack_base;

            let initial_mem_size = validation_info.memories.get(0).map(|m| m.limits.min * 65536).unwrap_or(0);
            register_container(
                container_id, slot_info.slot_id, None,
                sas_base.unwrap_or(0), initial_mem_size as u64,
                4 * 1024 * 1024 * 1024, slot_info.code_base, slot_info.stack_base,
            );
            store.container_id = Some(container_id);

            let mut linker = Linker::new();
            create_wasi_imports(&mut linker, &mut store);
            create_wasi_p2_imports(&mut linker, &mut store);
            store.wasi_ctx = Some(WasiCtx::new_with_env(args, actual_root_path, fds, env_vars));

            // Component handling
            let effective_validation;
            let effective_buffer;
            if let Some(component) = &validation_info.component {
                use crate::wasm::component::types::ComponentItem;
                let first_module = component.items.iter().find_map(|item| {
                    if let ComponentItem::Module(m) = item { Some(m) } else { None }
                });
                if let Some(core_mod) = first_module {
                    let core_bytes = &buffer[core_mod.content.from
                        ..core_mod.content.from + core_mod.content.len];
                    debugln!("[wasm-runner] [COMPONENT] Extracting core module ({} bytes) for AOT...",
                        core_bytes.len());
                    match validate(core_bytes) {
                        Ok(vi) => {
                            effective_validation = vi;
                            effective_buffer = core_bytes;
                        }
                        Err(e) => {
                            debugln!("[wasm-runner] Core module validation error: {:?}", e);
                            return WasmRunResult::Finished(1);
                        }
                    }
                } else {
                    debugln!("[wasm-runner] [COMPONENT] No core module found, falling back to component executor...");
                    return match crate::wasm::interpreter::component_executor::instantiate_component(
                        &mut store, &linker, component, buffer,
                    ) {
                        Ok(_) => WasmRunResult::Finished(0),
                        Err(e) => {
                            debugln!("[wasm-runner] Component error: {:?}", e);
                            WasmRunResult::Finished(1)
                        }
                    };
                }
            } else {
                effective_validation = validation_info;
                effective_buffer = buffer;
            }
            let _ = effective_buffer;

            let result = linker
                .module_instantiate_unchecked(&mut store, &effective_validation, None, slot_id)
                .and_then(|instance| {
                    // Save .wacc cache on first successful AOT compilation
                    if aot && wasm_mtime != 0 && !store.aot_modules.is_empty() {
                        let aot_mod = store.aot_modules.last().unwrap();
                        save_wacc(
                            name, aot_mod, &effective_validation,
                            wasm_mtime, wasm_size,
                            &instance.global_init_vals,
                            &instance.data_offsets,
                            &instance.elem_offsets,
                        );
                    }
                    handle_instantiation_result(&mut store, instance)
                });

            if let Ok(WasmRunResult::AotReady(ref info)) = result {
                let ctx = unsafe { &mut *(info.ctx_ptr as *mut crate::wasm::aot::runtime::Ring3Context) };
                ctx.module_addr = info.module_addr;
                let store_raw = alloc::boxed::Box::into_raw(alloc::boxed::Box::new(store));
                ctx.store = store_raw as *mut usize;
            }

            result
        }
        Err(e) => {
            debugln!("[wasm-runner] Validation error: {:?}", e);
            Ok(WasmRunResult::Finished(1))
        }
    }
    };

    let final_res = match res {
        Ok(r) => r,
        Err(crate::wasm::RuntimeError::HostFunctionHaltedExecution(code)) => {
            if code != 0 {
                debugln!("[wasm-runner] Process exited with code {}", code);
            }
            WasmRunResult::Finished(code)
        }
        Err(e) => {
            debugln!("[wasm-runner] Execution error in {}: {:?}", name, e);
            WasmRunResult::Finished(1)
        }
    };
    unsafe {
        crate::wasm::wasi::ICRNL = false;
    }
    debugln!("[wasm-runner] Finished buffer {}.", name);
    if let WasmRunResult::Finished(exit_code) = &final_res {
        unregister_container(container_id, *exit_code);
    }
    final_res
}

pub fn run_with_env(
    path: &str,
    mut args: Vec<String>,
    root_path: &str,
    fds: &[(u8, u8)],
    env_vars: Vec<(String, String)>,
    aot: bool,
) -> WasmRunResult {
    debugln!("[wasm-runner] Starting {} (AOT: {})...", path, aot);

    // Parse runtime-specific arguments
    let mut actual_root_path = root_path.to_string();
    let mut overrides: Vec<(usize, String)> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--dir" && i + 1 < args.len() {
            args.remove(i);
            actual_root_path = args.remove(i);
        } else if args[i].starts_with("--arg") && args[i].contains('=') {
            let eq_idx = args[i].find('=').unwrap();
            if let Ok(n) = args[i][5..eq_idx].parse::<usize>() {
                let val = args[i][eq_idx + 1..].to_string();
                args.remove(i);
                overrides.push((n, val));
            } else {
                i += 1;
            }
        } else if args[i].starts_with("--arg") {
            if let Ok(n) = args[i][5..].parse::<usize>() {
                if i + 1 < args.len() {
                    args.remove(i);
                    let val = args.remove(i);
                    overrides.push((n, val));
                } else {
                    i += 1;
                }
            } else {
                i += 1;
            }
        } else {
            i += 1;
        }
    }

    for (n, val) in overrides {
        while args.len() <= n {
            args.push(String::new());
        }
        args[n] = val;
    }

    if let Ok(mut file) = File::open(path) {
        let (wasm_mtime, wasm_size) = file.stat().map(|s| (s.mtime, s.size)).unwrap_or((0, 0));
        let size = wasm_size as usize;
        let mut buffer = Vec::with_capacity(size);
        if file.read_to_end(&mut buffer).is_ok() {
            unsafe {
                crate::wasm::wasi::ICRNL = true;
            }

            // .wacc cache fast path
            let wacc_info = if aot && wasm_mtime != 0 {
                try_load_wacc(path, wasm_mtime, wasm_size, &buffer)
            } else {
                None
            };

            if let Some(wacc) = wacc_info {
                // ── Cache hit ──
                let mut store = Store::new(());
                store.aot_enabled = true;
                let mut slot_info = crate::os::SlotInfo {
                    slot_id: 0, linear_memory_base: 0, linear_memory_size: 0, code_base: 0, stack_base: 0,
                };
                crate::os::process_get_slot_info(&mut slot_info as *mut _ as *mut u8);
                let sas_base = Some(slot_info.linear_memory_base);
                store.sas_memory_base = sas_base;
                store.code_base = Some(slot_info.code_base);
                store.stack_base = slot_info.stack_base + 16 * 1024 * 1024;
                store.stack_limit = slot_info.stack_base;

                let initial_mem_size = wacc.memories.get(0).map(|m| m.limits.min * 65536).unwrap_or(0);
                let container_id = get_pid();
                register_container(
                    container_id, slot_info.slot_id, None,
                    sas_base.unwrap_or(0), initial_mem_size as u64,
                    4 * 1024 * 1024 * 1024, slot_info.code_base, slot_info.stack_base,
                );
                store.container_id = Some(container_id);

                let mut linker = Linker::new();
                create_wasi_imports(&mut linker, &mut store);
                create_wasi_p2_imports(&mut linker, &mut store);
                store.wasi_ctx = Some(WasiCtx::new_with_env(args, actual_root_path, fds, env_vars));

                let extern_vals: Vec<crate::wasm::interpreter::store::ExternVal> = wacc.imports.iter()
                    .filter_map(|imp| linker.get_unchecked(imp.module_name.clone(), imp.name.clone()))
                    .collect();

                let res = store.module_instantiate_from_wacc(&wacc, &buffer, extern_vals, slot_info.slot_id)
                    .and_then(|instance| handle_instantiation_result(&mut store, instance));

                let final_res = match res {
                    Ok(r) => r,
                    Err(crate::wasm::RuntimeError::HostFunctionHaltedExecution(code)) => WasmRunResult::Finished(code),
                    Err(e) => {
                        debugln!("[wasm-runner] Execution error in {}: {:?}", path, e);
                        WasmRunResult::Finished(1)
                    }
                };
                unsafe { crate::wasm::wasi::ICRNL = false; }
                debugln!("[wasm-runner] Finished {}.", path);
                if let WasmRunResult::Finished(exit_code) = &final_res {
                    unregister_container(container_id, *exit_code);
                }
                return final_res;
            }

            // ── Cache miss: normal path ──
            match validate(&buffer) {
                Ok(validation_info) => {
                    let mut store = Store::new(());
                    store.aot_enabled = aot;

                    let mut slot_info = crate::os::SlotInfo {
                        slot_id: 0, linear_memory_base: 0, linear_memory_size: 0, code_base: 0, stack_base: 0,
                    };
                    crate::os::process_get_slot_info(&mut slot_info as *mut _ as *mut u8);

                    let sas_base = Some(slot_info.linear_memory_base);
                    store.sas_memory_base = sas_base;
                    store.code_base = Some(slot_info.code_base);
                    store.stack_base = slot_info.stack_base + 16 * 1024 * 1024;
                    store.stack_limit = slot_info.stack_base;

                    let initial_mem_size = validation_info.memories.get(0).map(|m| m.limits.min * 65536).unwrap_or(0);
                    let container_id = get_pid();
                    register_container(
                        container_id, slot_info.slot_id, None,
                        sas_base.unwrap_or(0), initial_mem_size as u64,
                        4 * 1024 * 1024 * 1024, slot_info.code_base, slot_info.stack_base,
                    );
                    store.container_id = Some(container_id);

                    let mut linker = Linker::new();
                    create_wasi_imports(&mut linker, &mut store);
                    create_wasi_p2_imports(&mut linker, &mut store);
                    store.wasi_ctx = Some(WasiCtx::new_with_env(args, actual_root_path, fds, env_vars));

                    let res: Result<WasmRunResult, crate::wasm::RuntimeError> = {
                        let effective_vi;
                        if let Some(component) = &validation_info.component {
                            use crate::wasm::component::types::ComponentItem;
                            let first_module = component.items.iter().find_map(|item| {
                                if let ComponentItem::Module(m) = item { Some(m) } else { None }
                            });
                            if let Some(core_mod) = first_module {
                                let core_bytes = &buffer[core_mod.content.from
                                    ..core_mod.content.from + core_mod.content.len];
                                match validate(core_bytes) {
                                    Ok(vi) => { effective_vi = vi; }
                                    Err(e) => {
                                        debugln!("[wasm-runner] Core module validation error: {:?}", e);
                                        return WasmRunResult::Finished(1);
                                    }
                                }
                            } else {
                                return match crate::wasm::interpreter::component_executor::instantiate_component(
                                    &mut store, &linker, component, &buffer,
                                ) {
                                    Ok(_) => WasmRunResult::Finished(0),
                                    Err(e) => {
                                        debugln!("[wasm-runner] Component error: {:?}", e);
                                        WasmRunResult::Finished(1)
                                    }
                                };
                            }
                        } else {
                            effective_vi = validation_info;
                        }
                        linker
                            .module_instantiate_unchecked(&mut store, &effective_vi, None, slot_info.slot_id)
                            .and_then(|instance| {
                                if aot && wasm_mtime != 0 && !store.aot_modules.is_empty() {
                                    let aot_mod = store.aot_modules.last().unwrap();
                                    save_wacc(
                                        path, aot_mod, &effective_vi,
                                        wasm_mtime, wasm_size,
                                        &instance.global_init_vals,
                                        &instance.data_offsets,
                                        &instance.elem_offsets,
                                    );
                                }
                                handle_instantiation_result(&mut store, instance)
                            })
                    };

                    let final_res = match res {
                        Ok(r) => r,
                        Err(crate::wasm::RuntimeError::HostFunctionHaltedExecution(code)) => WasmRunResult::Finished(code),
                        Err(e) => {
                            debugln!("[wasm-runner] Execution error in {}: {:?}", path, e);
                            WasmRunResult::Finished(1)
                        }
                    };
                    unsafe { crate::wasm::wasi::ICRNL = false; }
                    debugln!("[wasm-runner] Finished {}.", path);
                    if let WasmRunResult::Finished(exit_code) = &final_res {
                        unregister_container(container_id, *exit_code);
                    }
                    return final_res;
                }
                Err(e) => {
                    debugln!("[wasm-runner] Validation error: {:?}", e);
                    return WasmRunResult::Finished(1);
                }
            }
        }
    } else {
        debugln!("[wasm-runner] Could not open {}", path);
    }
    debugln!("[wasm-runner] Finished {}.", path);
    WasmRunResult::Finished(1)
}
