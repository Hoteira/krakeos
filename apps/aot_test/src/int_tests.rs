use std::println;
use core::num::Wrapping;

pub fn test_int_edge_cases() -> i32 {
    println!("==================== Testing Integer Edge Cases...");
    let mut errors = 0;

    errors += test_i32_bit_ops();
    errors += test_i64_bit_ops();
    errors += test_unsigned_arithmetic();
    errors += test_shifts_rotations();

    if errors == 0 { println!("==================== Integer Edge Cases: OK"); }
    errors
}

fn test_i32_bit_ops() -> i32 {
    let mut errors = 0;
    
    // CLZ
    if 0u32.leading_zeros() != 32 { errors += 1; println!("Error: i32.clz(0)"); }
    if 1u32.leading_zeros() != 31 { errors += 1; println!("Error: i32.clz(1)"); }
    if 0x80000000u32.leading_zeros() != 0 { errors += 1; println!("Error: i32.clz(MSB)"); }
    
    // CTZ
    if 0u32.trailing_zeros() != 32 { errors += 1; println!("Error: i32.ctz(0)"); }
    if 1u32.trailing_zeros() != 0 { errors += 1; println!("Error: i32.ctz(1)"); }
    if 0x80000000u32.trailing_zeros() != 31 { errors += 1; println!("Error: i32.ctz(MSB)"); }
    
    // Popcnt
    if 0u32.count_ones() != 0 { errors += 1; println!("Error: i32.popcnt(0)"); }
    if 0xFFFFFFFFu32.count_ones() != 32 { errors += 1; println!("Error: i32.popcnt(-1)"); }
    if 0x12345678u32.count_ones() != 13 { errors += 1; println!("Error: i32.popcnt(mixed)"); }

    errors
}

fn test_i64_bit_ops() -> i32 {
    let mut errors = 0;
    
    // CLZ
    if 0u64.leading_zeros() != 64 { errors += 1; println!("Error: i64.clz(0)"); }
    if 1u64.leading_zeros() != 63 { errors += 1; println!("Error: i64.clz(1)"); }
    if 0x8000000000000000u64.leading_zeros() != 0 { errors += 1; println!("Error: i64.clz(MSB)"); }
    
    // CTZ
    if 0u64.trailing_zeros() != 64 { errors += 1; println!("Error: i64.ctz(0)"); }
    if 1u64.trailing_zeros() != 0 { errors += 1; println!("Error: i64.ctz(1)"); }
    if 0x8000000000000000u64.trailing_zeros() != 63 { errors += 1; println!("Error: i64.ctz(MSB)"); }
    
    // Popcnt
    if 0u64.count_ones() != 0 { errors += 1; println!("Error: i64.popcnt(0)"); }
    if 0xFFFFFFFFFFFFFFFFu64.count_ones() != 64 { errors += 1; println!("Error: i64.popcnt(-1)"); }

    errors
}

fn test_unsigned_arithmetic() -> i32 {
    let mut errors = 0;

    // i32 DivU / RemU
    let a: u32 = 10;
    let b: u32 = 3;
    if a / b != 3 { errors += 1; println!("Error: i32.div_u"); }
    if a % b != 1 { errors += 1; println!("Error: i32.rem_u"); }

    let c: u32 = 0xFFFFFFFF;
    let d: u32 = 2;
    if c / d != 0x7FFFFFFF { errors += 1; println!("Error: i32.div_u large"); }
    if c % d != 1 { errors += 1; println!("Error: i32.rem_u large"); }

    // i64 DivU / RemU
    let e: u64 = 10;
    let f: u64 = 3;
    if e / f != 3 { errors += 1; println!("Error: i64.div_u"); }
    if e % f != 1 { errors += 1; println!("Error: i64.rem_u"); }

    let g: u64 = 0xFFFFFFFFFFFFFFFF;
    let h: u64 = 2;
    if g / h != 0x7FFFFFFFFFFFFFFF { errors += 1; println!("Error: i64.div_u large"); }
    if g % h != 1 { errors += 1; println!("Error: i64.rem_u large"); }

    errors
}

fn test_shifts_rotations() -> i32 {
    let mut errors = 0;

    // Rotations i32
    let a: u32 = 0x12345678;
    if a.rotate_left(8) != 0x34567812 { errors += 1; println!("Error: i32.rotl"); }
    if a.rotate_right(8) != 0x78123456 { errors += 1; println!("Error: i32.rotr"); }
    
    // Large rotation amounts (should be masked by 31)
    if a.rotate_left(40) != a.rotate_left(8) { errors += 1; println!("Error: i32.rotl masked"); }

    // Rotations i64
    let b: u64 = 0x123456789ABCDEF0;
    if b.rotate_left(16) != 0x56789ABCDEF01234 { errors += 1; println!("Error: i64.rotl"); }
    if b.rotate_right(16) != 0xDEF0123456789ABC { errors += 1; println!("Error: i64.rotr"); }
    
    // Large rotation amounts (should be masked by 63)
    if b.rotate_left(80) != b.rotate_left(16) { errors += 1; println!("Error: i64.rotl masked"); }

    // Shifts with large amounts
    let c: u32 = 0xFFFFFFFF;
    if shl_u32(c, 32) != c { errors += 1; println!("Error: i32.shl masked (32)"); }
    if shl_u32(c, 0) != c { errors += 1; println!("Error: i32.shl (0)"); }
    
    let d: i64 = -1; // 0xFFFFFFFFFFFFFFFF
    if shr_i64(d, 64) != d { errors += 1; println!("Error: i64.shr_s masked (64)"); }

    errors
}

#[inline(never)]
fn shl_u32(val: u32, shift: u32) -> u32 { val << shift }

#[inline(never)]
fn shr_i64(val: i64, shift: u32) -> i64 { val >> shift }
