use std::println;

#[cfg(target_arch = "wasm32")]
use core::arch::asm;

pub fn test_call_indirect() -> i32 {
    println!("==================== Testing Call Indirect...");
    let mut errors = 0;

    errors += test_indirect_simple();
    errors += test_indirect_dynamic();

    // We cannot easily test traps (null/sig) because they terminate the process.
    // In a real environment, we'd spawn a process and check exit code.
    // For now, we verify that valid calls work.

    if errors == 0 { println!("==================== Call Indirect: OK"); }
    errors
}

#[inline(never)]
fn add(a: i32, b: i32) -> i32 { a + b }

#[inline(never)]
fn sub(a: i32, b: i32) -> i32 { a - b }

#[inline(never)]
fn mul(a: i32, b: i32) -> i32 { a * b }

// We need a table.
// In Rust Wasm, we can't easily declare a table and put functions in it statically
// without 'wasm-bindgen' or similar.
// However, `call_indirect` is often used by Rust for `dyn Fn` calls.
// We can try to use function pointers.

fn test_indirect_simple() -> i32 {
    let f_add: fn(i32, i32) -> i32 = add;
    let f_sub: fn(i32, i32) -> i32 = sub;
    
    if f_add(10, 20) != 30 { return 1; }
    if f_sub(30, 10) != 20 { return 1; }
    0
}

fn test_indirect_dynamic() -> i32 {
    let funcs = [add, sub, mul];
    let args = [(10, 2), (20, 5), (5, 5)];
    let expected = [12, 15, 25];
    
    for i in 0..3 {
        let f = funcs[i];
        let (a, b) = args[i];
        let res = f(a, b);
        if res != expected[i] {
            println!("Error: indirect call {} returned {}, expected {}", i, res, expected[i]);
            return 1;
        }
    }
    0
}
