// Float/conversion helpers called by AOT code via the blob's jump table.
// These use native C ABI (args in XMM0/XMM1 or RDI, result in XMM0 or RAX).

#[no_mangle]
pub extern "C" fn f32_eq(a: f32, b: f32) -> i32 { (a == b) as i32 }
#[no_mangle]
pub extern "C" fn f32_ne(a: f32, b: f32) -> i32 { (a != b) as i32 }
#[no_mangle]
pub extern "C" fn f32_lt(a: f32, b: f32) -> i32 { (a < b) as i32 }
#[no_mangle]
pub extern "C" fn f32_gt(a: f32, b: f32) -> i32 { (a > b) as i32 }
#[no_mangle]
pub extern "C" fn f32_le(a: f32, b: f32) -> i32 { (a <= b) as i32 }
#[no_mangle]
pub extern "C" fn f32_ge(a: f32, b: f32) -> i32 { (a >= b) as i32 }

#[no_mangle]
pub extern "C" fn f64_eq(a: f64, b: f64) -> i32 { (a == b) as i32 }
#[no_mangle]
pub extern "C" fn f64_ne(a: f64, b: f64) -> i32 { (a != b) as i32 }
#[no_mangle]
pub extern "C" fn f64_lt(a: f64, b: f64) -> i32 { (a < b) as i32 }
#[no_mangle]
pub extern "C" fn f64_gt(a: f64, b: f64) -> i32 { (a > b) as i32 }
#[no_mangle]
pub extern "C" fn f64_le(a: f64, b: f64) -> i32 { (a <= b) as i32 }
#[no_mangle]
pub extern "C" fn f64_ge(a: f64, b: f64) -> i32 { (a >= b) as i32 }

#[no_mangle]
pub extern "C" fn f32_min(a: f32, b: f32) -> f32 {
    let bits_a = a.to_bits();
    let bits_b = b.to_bits();
    if (bits_a & 0x7FFFFFFF) > 0x7F800000 { return a; }
    if (bits_b & 0x7FFFFFFF) > 0x7F800000 { return b; }
    if a < b { return a; }
    if b < a { return b; }
    f32::from_bits(bits_a | bits_b)
}

#[no_mangle]
pub extern "C" fn f32_max(a: f32, b: f32) -> f32 {
    let bits_a = a.to_bits();
    let bits_b = b.to_bits();
    if (bits_a & 0x7FFFFFFF) > 0x7F800000 { return a; }
    if (bits_b & 0x7FFFFFFF) > 0x7F800000 { return b; }
    if a > b { return a; }
    if b > a { return b; }
    f32::from_bits(bits_a & bits_b)
}

#[no_mangle]
pub extern "C" fn f64_min(a: f64, b: f64) -> f64 {
    let bits_a = a.to_bits();
    let bits_b = b.to_bits();
    if (bits_a & 0x7FFFFFFFFFFFFFFF) > 0x7FF0000000000000 { return a; }
    if (bits_b & 0x7FFFFFFFFFFFFFFF) > 0x7FF0000000000000 { return b; }
    if a < b { return a; }
    if b < a { return b; }
    f64::from_bits(bits_a | bits_b)
}

#[no_mangle]
pub extern "C" fn f64_max(a: f64, b: f64) -> f64 {
    let bits_a = a.to_bits();
    let bits_b = b.to_bits();
    if (bits_a & 0x7FFFFFFFFFFFFFFF) > 0x7FF0000000000000 { return a; }
    if (bits_b & 0x7FFFFFFFFFFFFFFF) > 0x7FF0000000000000 { return b; }
    if a > b { return a; }
    if b > a { return b; }
    f64::from_bits(bits_a & bits_b)
}

#[no_mangle]
pub extern "C" fn f32_convert_i64_u(a: u64) -> f32 { a as f32 }
#[no_mangle]
pub extern "C" fn f64_convert_i64_u(a: u64) -> f64 { a as f64 }
#[no_mangle]
pub extern "C" fn i32_trunc_f32_u(a: f32) -> u32 { a as u32 }
#[no_mangle]
pub extern "C" fn i32_trunc_f64_u(a: f64) -> u32 { a as u32 }
#[no_mangle]
pub extern "C" fn i64_trunc_f32_u(a: f32) -> u64 { a as u64 }
#[no_mangle]
pub extern "C" fn i64_trunc_f64_u(a: f64) -> u64 { a as u64 }

#[no_mangle]
pub extern "C" fn i32_trunc_sat_f32_s(a: f32) -> i32 {
    if a.is_nan() { 0 }
    else if a >= 2147483648.0 { i32::MAX }
    else if a < -2147483648.0 { i32::MIN }
    else { a as i32 }
}

#[no_mangle]
pub extern "C" fn i32_trunc_sat_f32_u(a: f32) -> u32 {
    if a.is_nan() { 0 }
    else if a >= 4294967296.0 { u32::MAX }
    else if a <= -1.0 { 0 }
    else { a as u32 }
}

#[no_mangle]
pub extern "C" fn i32_trunc_sat_f64_s(a: f64) -> i32 {
    if a.is_nan() { 0 }
    else if a >= 2147483648.0 { i32::MAX }
    else if a < -2147483648.0 { i32::MIN }
    else { a as i32 }
}

#[no_mangle]
pub extern "C" fn i32_trunc_sat_f64_u(a: f64) -> u32 {
    if a.is_nan() { 0 }
    else if a >= 4294967296.0 { u32::MAX }
    else if a <= -1.0 { 0 }
    else { a as u32 }
}

#[no_mangle]
pub extern "C" fn i64_trunc_sat_f32_s(a: f32) -> i64 {
    if a.is_nan() { 0 }
    else if a >= 9223372036854775808.0 { i64::MAX }
    else if a < -9223372036854775808.0 { i64::MIN }
    else { a as i64 }
}

#[no_mangle]
pub extern "C" fn i64_trunc_sat_f32_u(a: f32) -> u64 {
    if a.is_nan() { 0 }
    else if a >= 18446744073709551616.0 { u64::MAX }
    else if a <= -1.0 { 0 }
    else { a as u64 }
}

#[no_mangle]
pub extern "C" fn i64_trunc_sat_f64_s(a: f64) -> i64 {
    if a.is_nan() { 0 }
    else if a >= 9223372036854775808.0 { i64::MAX }
    else if a < -9223372036854775808.0 { i64::MIN }
    else { a as i64 }
}

#[no_mangle]
pub extern "C" fn i64_trunc_sat_f64_u(a: f64) -> u64 {
    if a.is_nan() { 0 }
    else if a >= 18446744073709551616.0 { u64::MAX }
    else if a <= -1.0 { 0 }
    else { a as u64 }
}

#[no_mangle]
pub extern "C" fn f32_ceil(a: f32) -> f32 {
    let mut r = a as i32 as f32;
    if r < a { r += 1.0; }
    r
}

#[no_mangle]
pub extern "C" fn f32_floor(a: f32) -> f32 {
    let mut r = a as i32 as f32;
    if r > a { r -= 1.0; }
    r
}

#[no_mangle]
pub extern "C" fn f32_trunc(a: f32) -> f32 {
    a as i32 as f32
}

#[no_mangle]
pub extern "C" fn f32_nearest(a: f32) -> f32 {
    let f = a;
    let r = if f > 0.0 { f32_floor(f + 0.5) } else { f32_ceil(f - 0.5) };
    let diff = (f - r).abs();
    if diff == 0.5 {
        if (r as i32) % 2 != 0 {
            if r > 0.0 { r - 1.0 } else { r + 1.0 }
        } else {
            r
        }
    } else {
        r
    }
}

#[no_mangle]
pub extern "C" fn f64_ceil(a: f64) -> f64 {
    let mut r = a as i64 as f64;
    if r < a { r += 1.0; }
    r
}

#[no_mangle]
pub extern "C" fn f64_floor(a: f64) -> f64 {
    let mut r = a as i64 as f64;
    if r > a { r -= 1.0; }
    r
}

#[no_mangle]
pub extern "C" fn f64_trunc(a: f64) -> f64 {
    a as i64 as f64
}

#[no_mangle]
pub extern "C" fn f64_nearest(a: f64) -> f64 {
    let f = a;
    let r = if f > 0.0 { f64_floor(f + 0.5) } else { f64_ceil(f - 0.5) };
    let diff = (f - r).abs();
    if diff == 0.5 {
        if (r as i64) % 2 != 0 {
            if r > 0.0 { r - 1.0 } else { r + 1.0 }
        } else {
            r
        }
    } else {
        r
    }
}
