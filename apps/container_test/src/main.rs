#![no_std]
extern crate alloc;
use std::println;

pub fn main() {
    println!("[Container Test] Starting...");
    
    // A tiny WASM module that just returns 42
    let wasm_bytes: [u8; 36] = [
        0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 
        0x01, 0x05, 0x01, 0x60, 0x00, 0x01, 0x7f, 
        0x03, 0x02, 0x01, 0x00, 
        0x07, 0x07, 0x01, 0x03, 0x72, 0x75, 0x6e, 0x00, 0x00, 
        0x0a, 0x06, 0x01, 0x04, 0x00, 0x41, 0x2a, 0x0b
    ];

    println!("[Container Test] Planting child container at offset 0x10000...");
    match std::os::container_plant(&wasm_bytes, 0x10000, 65536) {
        Ok(id) => {
            println!("[Container Test] Child planted with ID: {}. Harvesting...", id);
            // In a real scenario we might need to wait or poll.
            // For this test, plant() starts the thread.
            
            for _ in 0..10 {
                if let Ok(val) = std::os::container_harvest(id) {
                    println!("[Container Test] Child returned: {}", val);
                    return;
                }
                std::os::sleep(100);
            }
            println!("[Container Test] Failed to harvest child (timeout).");
        }
        Err(e) => {
            println!("[Container Test] Plant failed: {}", e);
        }
    }
}
