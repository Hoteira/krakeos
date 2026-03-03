#![no_std]

extern crate alloc;
use std::println;

pub fn main() {
    println!("Container Test: Starting...");

    // Simple WASM module that returns 42
    let wasm_bytes: &[u8] = &[
        0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 
        0x01, 0x05, 0x01, 0x60, 0x00, 0x01, 0x7f, 
        0x03, 0x02, 0x01, 0x00, 
        0x07, 0x07, 0x01, 0x03, 0x72, 0x75, 0x6e, 0x00, 0x00, 
        0x0a, 0x06, 0x01, 0x04, 0x00, 0x41, 0x2a, 0x0b
    ];

    println!("Container Test: Planting child WASM (returns 42)...");
    
    // Plant at 0x10000 (64KB) offset, 64KB size
    match std::os::container_plant(wasm_bytes, 0x10000, 64 * 1024) {
        Ok(id) => {
            println!("Container Test: Child planted with ID {}", id);
            
            println!("Container Test: Harvesting child...");
            match std::os::container_harvest(id) {
                Ok(res) => {
                    println!("Container Test: Child returned {}", res);
                    if res == 42 {
                        println!("Container Test: SUCCESS");
                    } else {
                        println!("Container Test: FAILURE (expected 42)");
                    }
                }
                Err(e) => {
                    println!("Container Test: Harvest failed: {}", e);
                }
            }
        }
        Err(e) => {
            println!("Container Test: Plant failed: {}", e);
        }
    }
}
