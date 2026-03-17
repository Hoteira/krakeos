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

pub fn run(path: &str, root_path: &str, fds: &[(u8, u8)], aot: bool) -> i32 {
    run_with_args(path, vec![path.to_string()], root_path, fds, aot)
}

pub fn run_with_args(
    path: &str,
    args: Vec<String>,
    root_path: &str,
    fds: &[(u8, u8)],
    aot: bool,
) -> i32 {
    run_with_env(path, args, root_path, fds, Vec::new(), aot)
}

pub fn run_in_container<'a, T: Config>(
    name: &str,
    buffer: &'a [u8],
    linker: &mut Linker,
    store: &mut Store<'a, T>,
) -> i32 {
    unsafe {
        crate::wasm::wasi::ICRNL = true;
    }
    match validate(buffer) {
        Ok(validation_info) => {
            let res: Result<i32, crate::wasm::RuntimeError> = if let Some(component) = &validation_info.component {
                crate::wasm::interpreter::component_executor::instantiate_component(
                    store, linker, component, buffer,
                )
                .map(|_| 0)
            } else {
                linker
                    .module_instantiate_unchecked(store, &validation_info, None)
                    .and_then(|instance| {
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
                                        return *val as i32;
                                    }
                                }
                                0
                            })
                        } else {
                            Ok(0)
                        }
                    })
            };

            let exit_code = match res {
                Ok(code) => code,
                Err(crate::wasm::RuntimeError::HostFunctionHaltedExecution(code)) => code,
                Err(e) => {
                    crate::debugln!("[wasm-runner] Execution error in {}: {:?}", name, e);
                    1
                }
            };
            unsafe {
                crate::wasm::wasi::ICRNL = false;
            }
            exit_code
        }
        Err(_) => {
            unsafe {
                crate::wasm::wasi::ICRNL = false;
            }
            1
        }
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
) -> i32 {
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
    match validate(buffer) {
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
                const KERNEL_STACK_REGION_BASE: u64 = 0x0000_0043_0000_0000; // 268 GiB
                const KERNEL_STACK_SLOT_SIZE: u64   = 128 * 1024;    // 128 KiB
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

            let res: Result<i32, crate::wasm::RuntimeError> = if let Some(component) = &validation_info.component {
                debugln!("[wasm-runner] [COMPONENT] Executing...");
                crate::wasm::interpreter::component_executor::instantiate_component(
                    &mut store, &linker, component, buffer,
                )
                .map(|_| 0)
            } else {
                linker
                    .module_instantiate_unchecked(&mut store, &validation_info, None)
                    .and_then(|instance| {
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
                                    .instance_export_unchecked(
                                        instance.module_addr,
                                        "_start",
                                    )
                                    .ok()
                                    .and_then(|e| e.as_func())
                            });

                        if let Some(func_addr) = entry_point {
                            store
                                .invoke_unchecked(func_addr, Vec::new(), None)
                                .map(|run_res| {
                                    if let RunState::Finished { values, .. } = run_res {
                                        if let Some(Value::I32(val)) = values.first() {
                                            return *val as i32;
                                        }
                                    }
                                    0
                                })
                        } else {
                            debugln!("[wasm-runner] No entry point found.");
                            Ok(0)
                        }
                    })
            };

            let exit_code = match res {
                Ok(code) => code,
                Err(crate::wasm::RuntimeError::HostFunctionHaltedExecution(code)) => {
                    if code != 0 {
                        debugln!("[wasm-runner] Process exited with code {}", code);
                    }
                    code
                }
                Err(e) => {
                    debugln!("[wasm-runner] Execution error: {:?}", e);
                    1
                }
            };
            unsafe {
                crate::wasm::wasi::ICRNL = false;
            }
            debugln!("[wasm-runner] Finished buffer {}.", name);
            unregister_container(container_id, exit_code);
            return exit_code;
        }
        Err(e) => debugln!("[wasm-runner] Validation error: {:?}", e),
    }
    unsafe {
        crate::wasm::wasi::ICRNL = false;
    }
    debugln!("[wasm-runner] Finished buffer {}.", name);
    1
}

pub fn run_with_env(
    path: &str,
    mut args: Vec<String>,
    root_path: &str,
    fds: &[(u8, u8)],
    env_vars: Vec<(String, String)>,
    aot: bool,
) -> i32 {
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

                    let res: Result<i32, crate::wasm::RuntimeError> = if let Some(component) = &validation_info.component {
                        debugln!("[wasm-runner] [COMPONENT] Executing...");
                        crate::wasm::interpreter::component_executor::instantiate_component(
                            &mut store, &linker, component, &buffer,
                        )
                        .map(|_| 0)
                    } else {
                        linker
                            .module_instantiate_unchecked(&mut store, &validation_info, None)
                            .and_then(|instance| {
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
                                            .instance_export_unchecked(
                                                instance.module_addr,
                                                "_start",
                                            )
                                            .ok()
                                            .and_then(|e| e.as_func())
                                    });

                                if let Some(func_addr) = entry_point {
                                    store
                                        .invoke_unchecked(func_addr, Vec::new(), None)
                                        .map(|run_res| {
                                            if let RunState::Finished { values, .. } = run_res {
                                                if let Some(Value::I32(val)) = values.first() {
                                                    return *val as i32;
                                                }
                                            }
                                            0
                                        })
                                } else {
                                    debugln!("[wasm-runner] No entry point found.");
                                    Ok(0)
                                }
                            })
                    };

                    let exit_code = match res {
                        Ok(code) => code,
                        Err(crate::wasm::RuntimeError::HostFunctionHaltedExecution(code)) => {
                            if code != 0 {
                                debugln!("[wasm-runner] Process exited with code {}", code);
                            }
                            code
                        }
                        Err(e) => {
                            debugln!("[wasm-runner] Execution error: {:?}", e);
                            1
                        }
                    };
                    unsafe {
                        crate::wasm::wasi::ICRNL = false;
                    }
                    debugln!("[wasm-runner] Finished {}.", path);
                    unregister_container(container_id, exit_code);
                    return exit_code;
                }
                Err(e) => debugln!("[wasm-runner] Validation error: {:?}", e),
            }
            unsafe {
                crate::wasm::wasi::ICRNL = false;
            }
        }
    } else {
        debugln!("[wasm-runner] Could not open {}", path);
    }
    debugln!("[wasm-runner] Finished {}.", path);
    1
}
