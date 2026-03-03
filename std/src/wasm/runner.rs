extern crate alloc;
use crate::debugln;
use crate::fs::File;
use crate::io::Read;
use crate::wasm::wasi::{WasiCtx, create_wasi_imports, create_wasi_p2_imports};
use crate::wasm::container::{register_container, unregister_container};
use crate::wasm::common::config::Config;
use crate::wasm::{Linker, Store, validate};
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
            let res = if let Some(component) = &validation_info.component {
                crate::wasm::interpreter::component_executor::instantiate_component(
                    store, linker, component, buffer,
                )
                .map(|_| ())
            } else {
                linker
                    .module_instantiate_unchecked(store, &validation_info, None)
                    .and_then(|instance| {
                        let entry_point = store
                            .instance_export_unchecked(instance.module_addr, "run")
                            .ok()
                            .and_then(|e| e.as_func())
                            .or_else(|| {
                                store
                                    .instance_export_unchecked(instance.module_addr, "_start")
                                    .ok()
                                    .and_then(|e| e.as_func())
                            });

                        if let Some(func_addr) = entry_point {
                            store.invoke_unchecked(func_addr, Vec::new(), None).map(|_| ())
                        } else {
                            Ok(())
                        }
                    })
            };

            let exit_code = match res {
                Ok(_) => 0,
                Err(crate::wasm::RuntimeError::HostFunctionHaltedExecution(code)) => code,
                Err(_) => 1,
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

pub fn run_with_env(
    path: &str,
    args: Vec<String>,
    root_path: &str,
    fds: &[(u8, u8)],
    env_vars: Vec<(String, String)>,
    aot: bool,
) -> i32 {
    debugln!("[wasm-runner] Starting {} (AOT: {})...", path, aot);

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
                    
                    // Allocate SAS memory base (256MB chunk within the process slot)
                    let sas_base = crate::memory::allocate_sas_region(256 * 1024 * 1024);
                    store.sas_memory_base = sas_base;

                    // Register container
                    let container_id = register_container(None, 0, sas_base.unwrap_or(0), 0, 0);
                    store.container_id = Some(container_id);

                    let mut linker = Linker::new();

                    create_wasi_imports(&mut linker, &mut store);
                    create_wasi_p2_imports(&mut linker, &mut store);

                    store.wasi_ctx = Some(WasiCtx::new_with_env(
                        args,
                        root_path.to_string(),
                        fds,
                        env_vars,
                    ));

                    let res = if let Some(component) = &validation_info.component {
                        debugln!("[wasm-runner] [COMPONENT] Executing...");
                        crate::wasm::interpreter::component_executor::instantiate_component(
                            &mut store, &linker, component, &buffer,
                        )
                        .map(|_| ())
                    } else {
                        linker
                            .module_instantiate_unchecked(&mut store, &validation_info, None)
                            .and_then(|instance| {
                                let entry_point = store
                                    .instance_export_unchecked(instance.module_addr, "run")
                                    .ok()
                                    .and_then(|e| e.as_func())
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
                                        .map(|_| ())
                                } else {
                                    debugln!("[wasm-runner] No entry point found.");
                                    Ok(())
                                }
                            })
                    };

                    let exit_code = match res {
                        Ok(_) => 0,
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
