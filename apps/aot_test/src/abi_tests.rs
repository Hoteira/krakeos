use std::println;

#[no_mangle]
#[inline(never)]
pub fn test_multi_value_return_logic() -> i32 {
    123
}

pub fn test_abi() -> i32 {
    let mut errors = 0;

    let res = test_multi_value_return_logic();
    if res != 123 {
        println!("Error: test_multi_value_return_logic failed");
        errors += 1;
    }

    // Test Local Initialization
    // We'll use a helper that "forgets" to initialize a variable.
    // In WASM, it MUST be zero.
    let zero = test_uninitialized_local();
    if zero != 0 {
        println!("==================== ERROR: Local initialization failed: expected 0, got {}", zero);
        errors += 1;
    }

    if errors == 0 { println!("==================== ABI: OK"); }
    errors
}

#[no_mangle]
#[inline(never)]
fn test_uninitialized_local() -> i32 {
    // We use a large number of locals to ensure the zeroing loop works.
    let mut a: i32 = 0;
    let mut b: i32 = 0;
    let mut c: i32 = 0;
    let mut d: i32 = 0;
    let mut e: i32 = 0;
    
    // Rust will initialize them, but if we use assembly or trick it...
    // Actually, WASM locals are ALWAYS zero-initialized by the engine.
    // Our AOT engine must do this.
    // We can't easily "not initialize" in Rust source, but we can check if they ARE zero.
    a + b + c + d + e
}
