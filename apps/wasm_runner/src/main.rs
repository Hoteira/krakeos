#![no_std]
#![no_main]

extern crate alloc;
use alloc::vec::Vec;
use std::io::Read;
use std::println;
use std::wasm::{validate, Linker, Store};
// Removed Value

#[unsafe(no_mangle)]
pub extern "C" fn main() -> i32 {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() < 2 {
        println!("Usage: wasm_runner <file.wasm>");
        return 1;
    }

    let filename = &args[1];
    let mut file = match std::fs::File::open(filename) {
        Ok(f) => f,
        Err(_) => {
            println!("Error: Could not open file '{}'", filename);
            return 1;
        }
    };

    let mut buffer = Vec::new();
    if file.read_to_end(&mut buffer).is_err() {
        println!("Error: Could not read file '{}'", filename);
        return 1;
    }

    println!("Validating WASM...");
    let validation_info = match validate(&buffer) {
        Ok(info) => info,
        Err(e) => {
            println!("Validation failed: {:?}", e);
            return 1;
        }
    };
    println!("Validation successful.");

    let mut store = Store::new(()); // Empty user data
    let mut linker = Linker::new();

    // We can't easily implement WASI here without significant work.
    // So we just try to instantiate with empty imports (or fail if imports are needed).

    println!("Instantiating...");
    let instance_res = linker.module_instantiate(&mut store, &validation_info, None);

    let instance_outcome = match instance_res {
        Ok(outcome) => outcome,
        Err(e) => {
            println!("Instantiation failed: {:?}", e);
            // If it failed because of missing imports, listing them would be nice, but validation_info has them.
            if !validation_info.imports.is_empty() {
                println!("Module requires imports which are not provided.");
                for import in &validation_info.imports {
                    println!("  Import: {}.{}", import.module_name, import.name);
                }
            }
            return 1;
        }
    };

    println!("Instantiation successful. Module Address: {:?}", instance_outcome.module_addr);

    // Look for _start
    let start_export = store.instance_export(instance_outcome.module_addr, "_start");
    if let Ok(export) = start_export {
        if let Some(func_addr) = export.as_func() {
            println!("Found _start. Invoking...");
            match store.invoke(func_addr, Vec::new(), None) {
                Ok(_) => println!("Execution finished successfully."),
                Err(e) => println!("Execution failed: {:?}", e),
            }
        }
    } else {
        println!("No _start function found.");
    }

    0
}