#![no_std]
#![feature(wasm_target_feature)]
#![feature(asm_experimental_arch)]

use std::println;
use core::num::Wrapping;
use std::math::FloatMath;

mod simd_tests;
mod atomic_tests;
mod abi_tests;
mod int_tests;
mod float_tests;
mod control_flow_tests;
mod memory_bulk_tests;
mod table_global_tests;

pub fn main() {
    println!("==================== Starting Comprehensive AOT Compiler Test Suite...");

    let mut failed = 0;

    failed += int_tests::test_int_edge_cases();
    failed += float_tests::test_float_precision_rounding();
    failed += test_i32_arithmetic();
    failed += test_i64_arithmetic();
    failed += test_f32_math();
    failed += test_f64_math();
    failed += control_flow_tests::test_complex_control_flow();
    failed += memory_bulk_tests::test_memory_bulk();
    failed += table_global_tests::test_tables_globals();
    failed += test_memory_bounds();
    failed += test_conversions();
    failed += test_simd_basic();
    failed += abi_tests::test_abi();
    
    unsafe {
        failed += simd_tests::test_simd();
        failed += atomic_tests::test_atomics();
    }

    println!("====================");
    if failed == 0 {
        println!("==================== ALL AOT TESTS PASSED SUCCESSFULLY!");
    } else {
        println!("==================== AOT TEST SUITE FAILED WITH {} ERRORS", failed);
    }
}

#[inline(never)]
fn test_i32_arithmetic() -> i32 {
    println!("==================== Testing i32 Arithmetic...");
    let mut errors = 0;

    // Wraparound
    let a = Wrapping(i32::MAX);
    let b = Wrapping(1);
    if (a + b).0 != i32::MIN {
        println!("==================== ERROR: i32 overflow addition failed");
        errors += 1;
    }

    // Signed Division & Remainder
    let c: i32 = -10;
    let d: i32 = 3;
    if c / d != -3 {
        println!("==================== ERROR: i32 signed division failed: {} / {} = {}", c, d, c / d);
        errors += 1;
    }
    if c % d != -1 {
        println!("==================== ERROR: i32 signed remainder failed: {} % {} = {}", c, d, c % d);
        errors += 1;
    }

    // Bitwise and Shifts
    let e: i32 = 0x12345678;
    if (e << 16) >> 16 != 0x00005678 {
        println!("==================== ERROR: i32 shift logic failed");
        errors += 1;
    }

    if errors == 0 { println!("==================== i32 Arithmetic: OK"); }
    errors
}

#[inline(never)]
fn test_i64_arithmetic() -> i32 {
    println!("==================== Testing i64 Arithmetic...");
    let mut errors = 0;

    let a: i64 = 0x100000000; // 2^32
    let b: i64 = 0x100000000;
    if a + b != 0x200000000 {
        println!("==================== ERROR: i64 32-bit carry addition failed");
        errors += 1;
    }

    let c: i64 = -1;
    if (c as u64) != 0xFFFFFFFFFFFFFFFF {
        println!("==================== ERROR: i64 sign representation failed");
        errors += 1;
    }

    if errors == 0 { println!("==================== i64 Arithmetic: OK"); }
    errors
}

#[inline(never)]
fn test_f32_math() -> i32 {
    println!("==================== Testing f32 Floating Point...");
    let mut errors = 0;

    let a: f32 = 2.0;
    // Note: sqrtss seems to cause issues in some QEMU configs or due to strict FP exception handling
    // so we test multiplication instead to verify F32 opcodes working.
    if (a * a).sqrt() != 2.0 {
        println!("==================== ERROR: f32 sqrt failed");
        errors += 1;
    }

    let b: f32 = -5.5;
    if b.abs() != 5.5 {
        println!("==================== ERROR: f32 abs failed");
        errors += 1;
    }

    // NaN handling (partial, since we can't easily check bit patterns in no_std without transmute)
    let nan = f32::NAN;
    if !(nan != nan) {
        println!("==================== ERROR: f32 NaN comparison failed");
        errors += 1;
    }

    if errors == 0 { println!("==================== f32 Math: OK"); }
    errors
}

#[inline(never)]
fn test_f64_math() -> i32 {
    println!("==================== Testing f64 Floating Point...");
    let mut errors = 0;

    let a: f64 = 1.23456789;
    let b: f64 = 9.87654321;
    if (a + b) - b != a {
        // This might fail due to precision, but for these numbers it should be exact in f64
        let res = (a + b) - b;
        if (res - a).abs() > 1e-15 {
            println!("==================== ERROR: f64 precision check failed: diff={}", (res - a).abs());
            errors += 1;
        }
    }

    if errors == 0 { println!("==================== f64 Math: OK"); }
    errors
}

#[inline(never)]
fn test_memory_bounds() -> i32 {
    println!("==================== Testing Memory Access & Bounds...");
    let mut errors = 0;

    let mut data = [0u8; 1024];
    for i in 0..1024 {
        data[i] = (i % 256) as u8;
    }

    let mut sum: u32 = 0;
    for i in 0..1024 {
        sum += data[i] as u32;
    }
    // Sum of 0..255 repeated 4 times: 4 * (255 * 256 / 2) = 4 * 32640 = 130560
    if sum != 130560 {
        println!("==================== ERROR: Stack array memory integrity failed: sum={}", sum);
        errors += 1;
    }

    if errors == 0 { println!("==================== Memory Access: OK"); }
    errors
}

#[inline(never)]
fn test_conversions() -> i32 {
    println!("==================== Testing Numeric Conversions...");
    let mut errors = 0;

    let a: f32 = 123.456;
    let b: i32 = a as i32;
    if b != 123 {
        println!("==================== ERROR: f32 to i32 conversion failed: {}", b);
        errors += 1;
    }

    let c: i64 = 0x7FFFFFFFFFFFFFFF;
    let d: f64 = c as f64;
    // f64 can represent i64 but with precision loss at the end. 
    // But 0x7FFFFFFFFFFFFFFF as f64 is 9.223372036854776e18
    if d < 0.0 {
        println!("==================== ERROR: i64 to f64 signedness failed");
        errors += 1;
    }

    let e: i32 = -1;
    let f: i64 = e as i64;
    if f != -1 {
        println!("==================== ERROR: i32 to i64 sign extension failed: {:#x}", f);
        errors += 1;
    }

    if errors == 0 { println!("==================== Conversions: OK"); }
    errors
}

#[inline(never)]
fn test_simd_basic() -> i32 {
    println!("==================== Testing SIMD Basic Ops...");
    let mut errors = 0;

    // We'll use some manual bit manipulation to simulate what SIMD would do, 
    // as we want to see if the AOT compiler handles the generated instructions.
    // Note: Rust's core::arch::wasm32 requires the target-feature +simd128.
    
    // For now, we test the code patterns that often trigger SIMD in the compiler
    // or use direct memory loads/stores that AOT now supports.
    
    let v1 = [1u32, 2, 3, 4];
    let v2 = [5u32, 6, 7, 8];
    let mut v3 = [0u32; 4];
    
    for i in 0..4 {
        v3[i] = v1[i] + v2[i];
    }
    
    if v3 != [6, 8, 10, 12] {
        println!("==================== ERROR: Pseudo-SIMD array addition failed");
        errors += 1;
    }

    if errors == 0 { println!("==================== SIMD Basic: OK"); }
    errors
}
