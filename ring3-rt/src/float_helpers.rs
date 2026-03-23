// Float/conversion helpers called by AOT code via the blob's jump table.
// These use native C ABI (args in XMM0/XMM1 or RDI, result in XMM0 or RAX).

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
