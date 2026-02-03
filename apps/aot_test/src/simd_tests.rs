#![allow(unused_imports)]
#[cfg(target_arch = "wasm32")]
use core::arch::wasm32::*;

#[cfg(target_arch = "wasm32")]
#[target_feature(enable = "simd128")]
pub unsafe fn test_simd() -> i32 {
    use std::println;
    println!("==================== Testing SIMD Intrinsics...");
    let mut errors = 0;

    // 1. Splat & Extraction
    let a = i32x4_splat(42);
    if i32x4_extract_lane::<0>(a) != 42 { println!("Error: i32x4_splat/extract lane 0"); errors += 1; }
    if i32x4_extract_lane::<3>(a) != 42 { println!("Error: i32x4_splat/extract lane 3"); errors += 1; }

    // 2. Arithmetic (Add)
    let b = i32x4_splat(10);
    let c = i32x4_add(a, b); // 52, 52, 52, 52
    if i32x4_extract_lane::<0>(c) != 52 { println!("Error: i32x4_add"); errors += 1; }

    // 3. Float Math (Sqrt)
    let f = f32x4_splat(16.0);
    let g = f32x4_sqrt(f);
    if f32x4_extract_lane::<0>(g) != 4.0 { println!("Error: f32x4_sqrt"); errors += 1; }

    // 4. Comparison (Eq)
    let eq = i32x4_eq(a, a); // All true (-1)
    if i32x4_extract_lane::<0>(eq) != -1 { println!("Error: i32x4_eq (true)"); errors += 1; }
    let ne = i32x4_eq(a, b); // All false (0)
    if i32x4_extract_lane::<0>(ne) != 0 { println!("Error: i32x4_eq (false)"); errors += 1; }

    // 5. Shuffle
    let v1 = i32x4(1, 2, 3, 4);
    let v2 = i32x4(5, 6, 7, 8);
    // Shuffle: 0, 4, 1, 5 -> 1, 5, 2, 6
    let s = i32x4_shuffle::<0, 4, 1, 5>(v1, v2); 
    if i32x4_extract_lane::<0>(s) != 1 { println!("Error: shuffle lane 0"); errors += 1; }
    if i32x4_extract_lane::<1>(s) != 5 { println!("Error: shuffle lane 1"); errors += 1; }
    if i32x4_extract_lane::<2>(s) != 2 { println!("Error: shuffle lane 2"); errors += 1; }
    if i32x4_extract_lane::<3>(s) != 6 { println!("Error: shuffle lane 3"); errors += 1; }

    // 6. Min/Max
    let m = i32x4_min(a, b); // min(42, 10) = 10
    if i32x4_extract_lane::<0>(m) != 10 { println!("Error: i32x4_min_s"); errors += 1; }

    if errors == 0 { println!("==================== SIMD Intrinsics: OK"); }
    errors
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn test_simd() -> i32 {
    0
}
