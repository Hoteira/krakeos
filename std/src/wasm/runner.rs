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
    let res: Result<WasmRunResult, crate::wasm::RuntimeError> = match validate(buffer) {
        Ok(validation_info) => {
            let mut store = Store::new(());
            store.aot_enabled = aot;
            
            // Get slot info
            let mut slot_info = crate::os::SlotInfo {
                slot_id,
                linear_memory_base: 0,
                linear_memory_size: 0,
                code_base: 0,
                stack_base: 0,
            };
            
            // If in kernel, we need to calculate bases based on slot_id
            #[cfg(not(feature = "userland"))]
            {
                // In kernel, we use the constants defined in the SAS memory model
                const CODE_REGION_BASE: u64  = 0x0000_0001_0000_0000; // 4 GiB
                const CODE_SLOT_SIZE: u64    = 64 * 1024 * 1024;     // 64 MiB
                const STACK_REGION_BASE: u64 = 0x0000_0041_0000_0000; // 260 GiB
                const STACK_SLOT_SIZE: u64   = 2 * 1024 * 1024;      // 2 MiB
                const LINEAR_MEMORY_REGION_BASE: u64 = 0x0000_0043_2000_0000; // 268.5 GiB
                const LINEAR_MEMORY_SLOT_SIZE: u64   = 31 * 1024 * 1024 * 1024; // 31 GiB

                slot_info.linear_memory_base = LINEAR_MEMORY_REGION_BASE + (slot_id as u64) * LINEAR_MEMORY_SLOT_SIZE;
                slot_info.code_base = CODE_REGION_BASE + (slot_id as u64) * CODE_SLOT_SIZE;
                slot_info.stack_base = STACK_REGION_BASE + (slot_id as u64) * STACK_SLOT_SIZE;
            }
            #[cfg(feature = "userland")]
            crate::os::process_get_slot_info(&mut slot_info as *mut _ as *mut u8);

            // Allocate SAS memory base (1GB chunk within the process slot)
            let sas_base = Some(slot_info.linear_memory_base);
            store.sas_memory_base = sas_base;
            store.code_base = Some(slot_info.code_base);
            store.stack_base = slot_info.stack_base + 2 * 1024 * 1024; // 2MB stack top
            store.stack_limit = slot_info.stack_base; // Bottom

            // Register container
            let initial_mem_size = validation_info.memories.get(0).map(|m| m.limits.min * 65536).unwrap_or(0);
            register_container(
                container_id,
                slot_info.slot_id,
                None,
                sas_base.unwrap_or(0),
                initial_mem_size as u64,
                4 * 1024 * 1024 * 1024, // 4 GiB max
                slot_info.code_base,
                slot_info.stack_base,
            );
            store.container_id = Some(container_id);

            let mut linker = Linker::new();

            create_wasi_imports(&mut linker, &mut store);
            create_wasi_p2_imports(&mut linker, &mut store);

            store.wasi_ctx = Some(WasiCtx::new_with_env(
                args,
                actual_root_path,
                fds,
                env_vars,
            ));

            if let Some(component) = &validation_info.component {
                debugln!("[wasm-runner] [COMPONENT] Executing...");
                crate::wasm::interpreter::component_executor::instantiate_component(
                    &mut store, &linker, component, buffer,
                )
                .map(|_| WasmRunResult::Finished(0))
            } else {
                let result = linker
                    .module_instantiate_unchecked(&mut store, &validation_info, None, slot_id)
                    .and_then(|instance| handle_instantiation_result(&mut store, instance));

                // Persist the Store for Ring3 host call dispatch
                if let Ok(WasmRunResult::AotReady(ref info)) = result {
                    let ctx = unsafe { &mut *(info.ctx_ptr as *mut crate::wasm::aot::runtime::Ring3Context) };
                    ctx.module_addr = info.module_addr;
                    let store_raw = alloc::boxed::Box::into_raw(alloc::boxed::Box::new(store));
                    ctx.store = store_raw as *mut usize;
                }

                result
            }
        }
        Err(e) => {
            debugln!("[wasm-runner] Validation error: {:?}", e);
            Ok(WasmRunResult::Finished(1))
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
        let size = file.size();
        let mut buffer = Vec::with_capacity(size);
        if file.read_to_end(&mut buffer).is_ok() {
            unsafe {
                crate::wasm::wasi::ICRNL = true;
            }
            match validate(&buffer) {
                Ok(validation_info) => {
                    let mut store = Store::new(());
                    store.aot_enabled = aot;
                    
                    // Get slot info
                    let mut slot_info = crate::os::SlotInfo {
                        slot_id: 0,
                        linear_memory_base: 0,
                        linear_memory_size: 0,
                        code_base: 0,
                        stack_base: 0,
                    };
                    crate::os::process_get_slot_info(&mut slot_info as *mut _ as *mut u8);

                    // Use the slot's linear memory base directly
                    let sas_base = Some(slot_info.linear_memory_base);
                    store.sas_memory_base = sas_base;
                    store.code_base = Some(slot_info.code_base);
                    store.stack_base = slot_info.stack_base + 2 * 1024 * 1024;
                    store.stack_limit = slot_info.stack_base;

                    // Register container
                    let initial_mem_size = validation_info.memories.get(0).map(|m| m.limits.min * 65536).unwrap_or(0);
                    let container_id = get_pid(); // Use PID as container ID for top-level
                    register_container(
                        container_id,
                        slot_info.slot_id,
                        None,
                        sas_base.unwrap_or(0),
                        initial_mem_size as u64,
                        4 * 1024 * 1024 * 1024, // 4 GiB max
                        slot_info.code_base,
                        slot_info.stack_base,
                    );
                    store.container_id = Some(container_id);

                    let mut linker = Linker::new();

                    create_wasi_imports(&mut linker, &mut store);
                    create_wasi_p2_imports(&mut linker, &mut store);

                    store.wasi_ctx = Some(WasiCtx::new_with_env(
                        args,
                        actual_root_path,
                        fds,
                        env_vars,
                    ));

                    let res: Result<WasmRunResult, crate::wasm::RuntimeError> = if let Some(component) = &validation_info.component {
                        crate::wasm::interpreter::component_executor::instantiate_component(
                            &mut store, &linker, component, &buffer,
                        )
                        .map(|_| WasmRunResult::Finished(0))
                    } else {
                        linker
                            .module_instantiate_unchecked(&mut store, &validation_info, None, slot_info.slot_id)
                            .and_then(|instance| handle_instantiation_result(&mut store, instance))
                    };

                    let final_res = match res {
                        Ok(r) => r,
                        Err(crate::wasm::RuntimeError::HostFunctionHaltedExecution(code)) => WasmRunResult::Finished(code),
                        Err(e) => {
                            debugln!("[wasm-runner] Execution error in {}: {:?}", path, e);
                            WasmRunResult::Finished(1)
                        }
                    };
                    unsafe {
                        crate::wasm::wasi::ICRNL = false;
                    }
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
            unsafe {
                crate::wasm::wasi::ICRNL = false;
            }
        }
    } else {
        debugln!("[wasm-runner] Could not open {}", path);
    }
    debugln!("[wasm-runner] Finished {}.", path);
    WasmRunResult::Finished(1)
}
