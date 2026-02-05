use std::println;

#[cfg(target_arch = "wasm32")]
use core::arch::asm;

pub fn test_tables_globals() -> i32 {
    println!("==================== Testing Tables & Globals...");
    let mut errors = 0;

    errors += test_globals();
    errors += test_tables();

    if errors == 0 { println!("==================== Tables & Globals: OK"); }
    errors
}

#[cfg(target_arch = "wasm32")]
fn test_globals() -> i32 {
    let mut errors = 0;
    
    // LLVM WASM backend requires symbolic access to globals.
    // __stack_pointer is a standard global in many WASM environments.
    unsafe {
        let mut sp: i32;
        asm!(
            "global.get __stack_pointer",
            "local.set {0}",
            out(local) sp,
        );
        
        let original_sp = sp;
        let new_sp = sp - 64;
        
        asm!(
            "local.get {0}",
            "global.set __stack_pointer",
            in(local) new_sp,
        );
        
        asm!(
            "global.get __stack_pointer",
            "local.set {0}",
            out(local) sp,
        );
        
        if sp != new_sp {
            println!("Error: global.get/set failed: expected {}, got {}", new_sp, sp);
            errors += 1;
        }
        
        asm!(
            "local.get {0}",
            "global.set __stack_pointer",
            in(local) original_sp,
        ); // Restore
    }
    
    errors
}

#[cfg(not(target_arch = "wasm32"))]
fn test_globals() -> i32 { 0 }

#[cfg(target_arch = "wasm32")]
fn test_tables() -> i32 {
    let mut errors = 0;
    
    unsafe {
        // Table 0 is the default function table.
        let mut size: i32;
        asm!("table.size 0", "local.set {0}", out(local) size);
        
        // Grow table
        let mut prev: i32;
        // table.grow pops [ref, i32] and pushes [i32]
        // ref.null func is opcode 0xD0 0x70
        asm!(
            ".byte 0xd0, 0x70", // ref.null func
            "local.get {0}",
            "table.grow 0",
            "local.set {1}",
            in(local) 10,
            out(local) prev,
        );
        
        if prev == -1 {
            println!("Warning: table.grow(10) failed");
        } else {
            let mut new_size: i32;
            asm!("table.size 0", "local.set {0}", out(local) new_size);
            if new_size != prev + 10 {
                println!("Error: table.size mismatch after grow: expected {}, got {}", prev + 10, new_size);
                errors += 1;
            }
            
            // table.fill(offset, ref, count) pops [i32 (offset), ref, i32 (count)]
            asm!(
                "local.get {0}", // offset
                ".byte 0xd0, 0x70", // ref.null func
                "local.get {1}", // count
                "table.fill 0",
                in(local) prev,
                in(local) 5,
            );

            // table.copy(dest, src, count) pops [i32, i32, i32]
            asm!(
                "local.get {0}", // dest
                "local.get {1}", // src
                "local.get {2}", // count
                "table.copy 0, 0",
                in(local) prev,
                in(local) 0,
                in(local) 2,
            );
        }
    }
    
    errors
}

#[cfg(not(target_arch = "wasm32"))]
fn test_tables() -> i32 { 0 }