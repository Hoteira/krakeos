extern crate alloc;
use alloc::vec::Vec;
use alloc::string::{String, ToString};
use alloc::vec;
use crate::fs::File;
use crate::io::Read;
use crate::wasm::{validate, Linker, Store};
use crate::wasm::wasi::{create_wasi_imports, create_wasi_p2_imports, WasiCtx};
use crate::debugln;

pub fn run(path: &str, root_path: &str, fds: &[(u8, u8)], aot: bool) {
    run_with_args(path, vec![path.to_string()], root_path, fds, aot);
}

pub fn run_with_args(path: &str, args: Vec<String>, root_path: &str, fds: &[(u8, u8)], aot: bool) {
    debugln!("[wasm-runner] Starting {} (AOT: {})...", path, aot);

    if let Ok(mut file) = File::open(path) {
        let size = file.size();
        let mut buffer = Vec::with_capacity(size);
        if file.read_to_end(&mut buffer).is_ok() {
            unsafe { crate::wasm::wasi::ICRNL = true; }
             match validate(&buffer) {
                Ok(validation_info) => {
                    let mut store = Store::new(());
                    store.aot_enabled = aot;
                    let mut linker = Linker::new();

                    create_wasi_imports(&mut linker, &mut store);
                    create_wasi_p2_imports(&mut linker, &mut store);
                    
                    store.wasi_ctx = Some(WasiCtx::new(args, root_path.to_string(), fds));

                    let res = if let Some(component) = &validation_info.component {
                         debugln!("[wasm-runner] [COMPONENT] Executing...");
                         crate::wasm::interpreter::component_executor::instantiate_component(
                            &mut store, &linker, component, &buffer,
                        ).map(|_| ())
                    } else {
                        linker.module_instantiate_unchecked(&mut store, &validation_info, None)
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
                                     debugln!("[wasm-runner] No entry point found.");
                                     Ok(())
                                }
                            })
                    };

                    if let Err(e) = res {
                        match e {
                            crate::wasm::RuntimeError::HostFunctionHaltedExecution(0) => {
                                // Normal exit
                            }
                            crate::wasm::RuntimeError::HostFunctionHaltedExecution(code) => {
                                debugln!("[wasm-runner] Process exited with code {}", code);
                            }
                            _ => {
                                debugln!("[wasm-runner] Execution error: {:?}", e);
                            }
                        }
                    }
                }
                Err(e) => debugln!("[wasm-runner] Validation error: {:?}", e),
             }
             unsafe { crate::wasm::wasi::ICRNL = false; }
        }
    } else {
        debugln!("[wasm-runner] Could not open {}", path);
    }
    debugln!("[wasm-runner] Finished {}.", path);
}