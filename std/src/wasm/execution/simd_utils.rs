use crate::wasm::execution::little_endian::LittleEndianBytes;
use core::array;

#[inline(always)]
pub fn to_lanes<const M: usize, const N: usize, T: LittleEndianBytes<M>>(data: [u8; 16]) -> [T; N] {
    // static_assertions::const_assert_eq!(M * N, 16);
    let mut lanes = data
        .chunks(M)
        .map(|chunk| T::from_le_bytes(chunk.try_into().unwrap()));
    array::from_fn(|_| lanes.next().unwrap())
}

#[inline(always)]
pub fn to_lanes_8<const M: usize, const N: usize, T: LittleEndianBytes<M>>(data: [u8; 8]) -> [T; N] {
    let mut lanes = data
        .chunks(M)
        .map(|chunk| T::from_le_bytes(chunk.try_into().unwrap()));
    array::from_fn(|_| lanes.next().unwrap())
}

#[inline(always)]
pub fn from_lanes<const M: usize, const N: usize, T: LittleEndianBytes<M>>(lanes: [T; N]) -> [u8; 16] {
    // static_assertions::const_assert_eq!(M * N, 16);
    let mut bytes = lanes.into_iter().flat_map(T::to_le_bytes);
    array::from_fn(|_| bytes.next().unwrap())
}

macro_rules! binop {
    ($name:ident, $type:ty, $lane_count:expr, $lane_size:expr, $op:expr) => {
        pub fn $name(a: [u8; 16], b: [u8; 16]) -> [u8; 16] {
            let a_lanes: [$type; $lane_count] = to_lanes::<$lane_size, $lane_count, $type>(a);
            let b_lanes: [$type; $lane_count] = to_lanes::<$lane_size, $lane_count, $type>(b);
            let res_lanes = array::from_fn(|i| $op(a_lanes[i], b_lanes[i]));
            from_lanes::<$lane_size, $lane_count, $type>(res_lanes)
        }
    };
}

macro_rules! unop {
    ($name:ident, $type:ty, $lane_count:expr, $lane_size:expr, $op:expr) => {
        pub fn $name(a: [u8; 16]) -> [u8; 16] {
            let a_lanes: [$type; $lane_count] = to_lanes::<$lane_size, $lane_count, $type>(a);
            let res_lanes = array::from_fn(|i| $op(a_lanes[i]));
            from_lanes::<$lane_size, $lane_count, $type>(res_lanes)
        }
    };
}

macro_rules! relop {
    ($name:ident, $type:ty, $lane_count:expr, $lane_size:expr, $op:expr) => {
        pub fn $name(a: [u8; 16], b: [u8; 16]) -> [u8; 16] {
            let a_lanes: [$type; $lane_count] = to_lanes::<$lane_size, $lane_count, $type>(a);
            let b_lanes: [$type; $lane_count] = to_lanes::<$lane_size, $lane_count, $type>(b);
            // For relops, true is all 1s (-1), false is 0
            // let true_val: $type = unsafe { core::mem::transmute(!0 as $type) }; 
            // Warning: !0 might not work for floats directly or unsigned? 
            // Actually for integer vectors, -1 is 0xFF...FF.
            // For boolean vectors, we want all bits 1.
            // Let's manually construct the result masks.
            let mut res_bytes = [0u8; 16];
            for i in 0..$lane_count {
                let val = if $op(a_lanes[i], b_lanes[i]) { 0xFF } else { 0 };
                // Write 'val' to the corresponding slice in res_bytes
                let chunk = &mut res_bytes[i * $lane_size..(i + 1) * $lane_size];
                chunk.fill(val);
            }
            res_bytes
        }
    };
}

// Integer Arithmetic
binop!(i8x16_add, i8, 16, 1, |a: i8, b: i8| a.wrapping_add(b));
binop!(i8x16_sub, i8, 16, 1, |a: i8, b: i8| a.wrapping_sub(b));
binop!(i16x8_add, i16, 8, 2, |a: i16, b: i16| a.wrapping_add(b));
binop!(i16x8_sub, i16, 8, 2, |a: i16, b: i16| a.wrapping_sub(b));
binop!(i16x8_mul, i16, 8, 2, |a: i16, b: i16| a.wrapping_mul(b));
binop!(i32x4_add, i32, 4, 4, |a: i32, b: i32| a.wrapping_add(b));
binop!(i32x4_sub, i32, 4, 4, |a: i32, b: i32| a.wrapping_sub(b));
binop!(i32x4_mul, i32, 4, 4, |a: i32, b: i32| a.wrapping_mul(b));
binop!(i64x2_add, i64, 2, 8, |a: i64, b: i64| a.wrapping_add(b));
binop!(i64x2_sub, i64, 2, 8, |a: i64, b: i64| a.wrapping_sub(b));
binop!(i64x2_mul, i64, 2, 8, |a: i64, b: i64| a.wrapping_mul(b));

// Floating Point Arithmetic (using F32/F64 wrappers or f32/f64 primitives)
// We need to handle them carefully. interpreter_loop uses crate::wasm::execution::value::{F32, F64}.
// But LittleEndianBytes is implemented for them.

use crate::wasm::execution::value::{F32, F64};

binop!(f32x4_add, F32, 4, 4, |a: F32, b: F32| a + b);
binop!(f32x4_sub, F32, 4, 4, |a: F32, b: F32| a - b);
binop!(f32x4_mul, F32, 4, 4, |a: F32, b: F32| a * b);
binop!(f32x4_div, F32, 4, 4, |a: F32, b: F32| a / b);
binop!(f32x4_min, F32, 4, 4, |a: F32, b: F32| a.min(b));
binop!(f32x4_max, F32, 4, 4, |a: F32, b: F32| a.max(b));
binop!(f32x4_pmin, F32, 4, 4, |a: F32, b: F32| if b.0 < a.0 { b } else { a }); // Pseudo-min (wasm spec)
binop!(f32x4_pmax, F32, 4, 4, |a: F32, b: F32| if b.0 > a.0 { b } else { a });

binop!(f64x2_add, F64, 2, 8, |a: F64, b: F64| a + b);
binop!(f64x2_sub, F64, 2, 8, |a: F64, b: F64| a - b);
binop!(f64x2_mul, F64, 2, 8, |a: F64, b: F64| a * b);
binop!(f64x2_div, F64, 2, 8, |a: F64, b: F64| a / b);
binop!(f64x2_min, F64, 2, 8, |a: F64, b: F64| a.min(b));
binop!(f64x2_max, F64, 2, 8, |a: F64, b: F64| a.max(b));
binop!(f64x2_pmin, F64, 2, 8, |a: F64, b: F64| if b.0 < a.0 { b } else { a });
binop!(f64x2_pmax, F64, 2, 8, |a: F64, b: F64| if b.0 > a.0 { b } else { a });

// Neg / Abs / Sqrt
unop!(f32x4_neg, F32, 4, 4, |a: F32| a.neg());
unop!(f32x4_abs, F32, 4, 4, |a: F32| a.abs());
unop!(f32x4_sqrt, F32, 4, 4, |a: F32| a.sqrt());
unop!(f32x4_ceil, F32, 4, 4, |a: F32| a.ceil());
unop!(f32x4_floor, F32, 4, 4, |a: F32| a.floor());
unop!(f32x4_trunc, F32, 4, 4, |a: F32| a.trunc());
unop!(f32x4_nearest, F32, 4, 4, |a: F32| a.nearest());

unop!(f64x2_neg, F64, 2, 8, |a: F64| a.neg());
unop!(f64x2_abs, F64, 2, 8, |a: F64| a.abs());
unop!(f64x2_sqrt, F64, 2, 8, |a: F64| a.sqrt());
unop!(f64x2_ceil, F64, 2, 8, |a: F64| a.ceil());
unop!(f64x2_floor, F64, 2, 8, |a: F64| a.floor());
unop!(f64x2_trunc, F64, 2, 8, |a: F64| a.trunc());
unop!(f64x2_nearest, F64, 2, 8, |a: F64| a.nearest());

// Relational
relop!(i8x16_eq, i8, 16, 1, |a, b| a == b);
relop!(i8x16_ne, i8, 16, 1, |a, b| a != b);
relop!(i8x16_lt_s, i8, 16, 1, |a, b| a < b);
relop!(i8x16_lt_u, u8, 16, 1, |a, b| a < b);
relop!(i8x16_gt_s, i8, 16, 1, |a, b| a > b);
relop!(i8x16_gt_u, u8, 16, 1, |a, b| a > b);
relop!(i8x16_le_s, i8, 16, 1, |a, b| a <= b);
relop!(i8x16_le_u, u8, 16, 1, |a, b| a <= b);
relop!(i8x16_ge_s, i8, 16, 1, |a, b| a >= b);
relop!(i8x16_ge_u, u8, 16, 1, |a, b| a >= b);

relop!(i16x8_eq, i16, 8, 2, |a, b| a == b);
relop!(i16x8_ne, i16, 8, 2, |a, b| a != b);
relop!(i16x8_lt_s, i16, 8, 2, |a, b| a < b);
relop!(i16x8_lt_u, u16, 8, 2, |a, b| a < b);
relop!(i16x8_gt_s, i16, 8, 2, |a, b| a > b);
relop!(i16x8_gt_u, u16, 8, 2, |a, b| a > b);
relop!(i16x8_le_s, i16, 8, 2, |a, b| a <= b);
relop!(i16x8_le_u, u16, 8, 2, |a, b| a <= b);
relop!(i16x8_ge_s, i16, 8, 2, |a, b| a >= b);
relop!(i16x8_ge_u, u16, 8, 2, |a, b| a >= b);

relop!(i32x4_eq, i32, 4, 4, |a, b| a == b);
relop!(i32x4_ne, i32, 4, 4, |a, b| a != b);
relop!(i32x4_lt_s, i32, 4, 4, |a, b| a < b);
relop!(i32x4_lt_u, u32, 4, 4, |a, b| a < b);
relop!(i32x4_gt_s, i32, 4, 4, |a, b| a > b);
relop!(i32x4_gt_u, u32, 4, 4, |a, b| a > b);
relop!(i32x4_le_s, i32, 4, 4, |a, b| a <= b);
relop!(i32x4_le_u, u32, 4, 4, |a, b| a <= b);
relop!(i32x4_ge_s, i32, 4, 4, |a, b| a >= b);
relop!(i32x4_ge_u, u32, 4, 4, |a, b| a >= b);

relop!(i64x2_eq, i64, 2, 8, |a, b| a == b);
relop!(i64x2_ne, i64, 2, 8, |a, b| a != b);
relop!(i64x2_lt_s, i64, 2, 8, |a, b| a < b);
relop!(i64x2_gt_s, i64, 2, 8, |a, b| a > b);
relop!(i64x2_le_s, i64, 2, 8, |a, b| a <= b);
relop!(i64x2_ge_s, i64, 2, 8, |a, b| a >= b);

relop!(f32x4_eq, F32, 4, 4, |a: F32, b: F32| a == b);
relop!(f32x4_ne, F32, 4, 4, |a: F32, b: F32| a != b);
relop!(f32x4_lt, F32, 4, 4, |a: F32, b: F32| a < b);
relop!(f32x4_gt, F32, 4, 4, |a: F32, b: F32| a > b);
relop!(f32x4_le, F32, 4, 4, |a: F32, b: F32| a <= b);
relop!(f32x4_ge, F32, 4, 4, |a: F32, b: F32| a >= b);

relop!(f64x2_eq, F64, 2, 8, |a: F64, b: F64| a == b);
relop!(f64x2_ne, F64, 2, 8, |a: F64, b: F64| a != b);
relop!(f64x2_lt, F64, 2, 8, |a: F64, b: F64| a < b);
relop!(f64x2_gt, F64, 2, 8, |a: F64, b: F64| a > b);
relop!(f64x2_le, F64, 2, 8, |a: F64, b: F64| a <= b);
relop!(f64x2_ge, F64, 2, 8, |a: F64, b: F64| a >= b);

// Bitwise
pub fn v128_not(a: [u8; 16]) -> [u8; 16] {
    let mut res = [0u8; 16];
    for i in 0..16 { res[i] = !a[i]; }
    res
}
pub fn v128_and(a: [u8; 16], b: [u8; 16]) -> [u8; 16] {
    let mut res = [0u8; 16];
    for i in 0..16 { res[i] = a[i] & b[i]; }
    res
}
pub fn v128_andnot(a: [u8; 16], b: [u8; 16]) -> [u8; 16] {
    let mut res = [0u8; 16];
    for i in 0..16 { res[i] = a[i] & !b[i]; }
    res
}
pub fn v128_or(a: [u8; 16], b: [u8; 16]) -> [u8; 16] {
    let mut res = [0u8; 16];
    for i in 0..16 { res[i] = a[i] | b[i]; }
    res
}
pub fn v128_xor(a: [u8; 16], b: [u8; 16]) -> [u8; 16] {
    let mut res = [0u8; 16];
    for i in 0..16 { res[i] = a[i] ^ b[i]; }
    res
}
pub fn v128_bitselect(v1: [u8; 16], v2: [u8; 16], c: [u8; 16]) -> [u8; 16] {
    let mut res = [0u8; 16];
    for i in 0..16 {
        res[i] = (v1[i] & c[i]) | (v2[i] & !c[i]);
    }
    res
}

// Any True / All True
pub fn v128_any_true(a: [u8; 16]) -> bool {
    a.iter().any(|&x| x != 0)
}

pub fn i8x16_all_true(a: [u8; 16]) -> bool {
    a.iter().all(|&x| x != 0)
}
pub fn i8x16_bitmask(a: [u8; 16]) -> i32 {
    let mut mask = 0;
    for (i, &byte) in a.iter().enumerate() {
        if (byte as i8) < 0 { mask |= 1 << i; }
    }
    mask
}

pub fn i16x8_all_true(a: [u8; 16]) -> bool {
    let lanes: [i16; 8] = to_lanes::<2, 8, i16>(a);
    lanes.iter().all(|&x| x != 0)
}
pub fn i16x8_bitmask(a: [u8; 16]) -> i32 {
    let lanes: [i16; 8] = to_lanes::<2, 8, i16>(a);
    let mut mask = 0;
    for (i, &val) in lanes.iter().enumerate() {
        if val < 0 { mask |= 1 << i; }
    }
    mask
}

pub fn i32x4_all_true(a: [u8; 16]) -> bool {
    let lanes: [i32; 4] = to_lanes::<4, 4, i32>(a);
    lanes.iter().all(|&x| x != 0)
}
pub fn i32x4_bitmask(a: [u8; 16]) -> i32 {
    let lanes: [i32; 4] = to_lanes::<4, 4, i32>(a);
    let mut mask = 0;
    for (i, &val) in lanes.iter().enumerate() {
        if val < 0 { mask |= 1 << i; }
    }
    mask
}

pub fn i64x2_all_true(a: [u8; 16]) -> bool {
    let lanes: [i64; 2] = to_lanes::<8, 2, i64>(a);
    lanes.iter().all(|&x| x != 0)
}
pub fn i64x2_bitmask(a: [u8; 16]) -> i32 {
    let lanes: [i64; 2] = to_lanes::<8, 2, i64>(a);
    let mut mask = 0;
    for (i, &val) in lanes.iter().enumerate() {
        if val < 0 { mask |= 1 << i; }
    }
    mask
}

// Shifts
pub fn i8x16_shl(a: [u8; 16], b: u32) -> [u8; 16] {
    let shift = (b % 8) as u8;
    let mut res = [0u8; 16];
    for i in 0..16 { res[i] = a[i] << shift; }
    res
}
pub fn i8x16_shr_s(a: [u8; 16], b: u32) -> [u8; 16] {
    let shift = (b % 8) as i8;
    let mut res = [0u8; 16];
    for i in 0..16 { res[i] = ((a[i] as i8) >> shift) as u8; }
    res
}
pub fn i8x16_shr_u(a: [u8; 16], b: u32) -> [u8; 16] {
    let shift = (b % 8) as u8;
    let mut res = [0u8; 16];
    for i in 0..16 { res[i] = a[i] >> shift; }
    res
}

pub fn i16x8_shl(a: [u8; 16], b: u32) -> [u8; 16] {
    let shift = (b % 16) as u16;
    let lanes: [u16; 8] = to_lanes::<2, 8, u16>(a);
    let res_lanes = array::from_fn(|i| lanes[i] << shift);
    from_lanes::<2, 8, u16>(res_lanes)
}
pub fn i16x8_shr_s(a: [u8; 16], b: u32) -> [u8; 16] {
    let shift = (b % 16) as i16;
    let lanes: [i16; 8] = to_lanes::<2, 8, i16>(a);
    let res_lanes = array::from_fn(|i| lanes[i] >> shift);
    from_lanes::<2, 8, i16>(res_lanes)
}
pub fn i16x8_shr_u(a: [u8; 16], b: u32) -> [u8; 16] {
    let shift = (b % 16) as u16;
    let lanes: [u16; 8] = to_lanes::<2, 8, u16>(a);
    let res_lanes = array::from_fn(|i| lanes[i] >> shift);
    from_lanes::<2, 8, u16>(res_lanes)
}

pub fn i32x4_shl(a: [u8; 16], b: u32) -> [u8; 16] {
    let shift = (b % 32) as u32;
    let lanes: [u32; 4] = to_lanes::<4, 4, u32>(a);
    let res_lanes = array::from_fn(|i| lanes[i] << shift);
    from_lanes::<4, 4, u32>(res_lanes)
}
pub fn i32x4_shr_s(a: [u8; 16], b: u32) -> [u8; 16] {
    let shift = (b % 32) as i32;
    let lanes: [i32; 4] = to_lanes::<4, 4, i32>(a);
    let res_lanes = array::from_fn(|i| lanes[i] >> shift);
    from_lanes::<4, 4, i32>(res_lanes)
}
pub fn i32x4_shr_u(a: [u8; 16], b: u32) -> [u8; 16] {
    let shift = (b % 32) as u32;
    let lanes: [u32; 4] = to_lanes::<4, 4, u32>(a);
    let res_lanes = array::from_fn(|i| lanes[i] >> shift);
    from_lanes::<4, 4, u32>(res_lanes)
}

pub fn i64x2_shl(a: [u8; 16], b: u32) -> [u8; 16] {
    let shift = (b % 64) as u64;
    let lanes: [u64; 2] = to_lanes::<8, 2, u64>(a);
    let res_lanes = array::from_fn(|i| lanes[i] << shift);
    from_lanes::<8, 2, u64>(res_lanes)
}
pub fn i64x2_shr_s(a: [u8; 16], b: u32) -> [u8; 16] {
    let shift = (b % 64) as i64;
    let lanes: [i64; 2] = to_lanes::<8, 2, i64>(a);
    let res_lanes = array::from_fn(|i| lanes[i] >> shift);
    from_lanes::<8, 2, i64>(res_lanes)
}
pub fn i64x2_shr_u(a: [u8; 16], b: u32) -> [u8; 16] {
    let shift = (b % 64) as u64;
    let lanes: [u64; 2] = to_lanes::<8, 2, u64>(a);
    let res_lanes = array::from_fn(|i| lanes[i] >> shift);
    from_lanes::<8, 2, u64>(res_lanes)
}

// Saturating arithmetic
binop!(i8x16_add_sat_s, i8, 16, 1, |a: i8, b: i8| a.saturating_add(b));
binop!(i8x16_add_sat_u, u8, 16, 1, |a: u8, b: u8| a.saturating_add(b));
binop!(i8x16_sub_sat_s, i8, 16, 1, |a: i8, b: i8| a.saturating_sub(b));
binop!(i8x16_sub_sat_u, u8, 16, 1, |a: u8, b: u8| a.saturating_sub(b));
binop!(i16x8_add_sat_s, i16, 8, 2, |a: i16, b: i16| a.saturating_add(b));
binop!(i16x8_add_sat_u, u16, 8, 2, |a: u16, b: u16| a.saturating_add(b));
binop!(i16x8_sub_sat_s, i16, 8, 2, |a: i16, b: i16| a.saturating_sub(b));
binop!(i16x8_sub_sat_u, u16, 8, 2, |a: u16, b: u16| a.saturating_sub(b));

// Min/Max/Avgr
binop!(i8x16_min_s, i8, 16, 1, |a: i8, b: i8| a.min(b));
binop!(i8x16_min_u, u8, 16, 1, |a: u8, b: u8| a.min(b));
binop!(i8x16_max_s, i8, 16, 1, |a: i8, b: i8| a.max(b));
binop!(i8x16_max_u, u8, 16, 1, |a: u8, b: u8| a.max(b));
binop!(i16x8_min_s, i16, 8, 2, |a: i16, b: i16| a.min(b));
binop!(i16x8_min_u, u16, 8, 2, |a: u16, b: u16| a.min(b));
binop!(i16x8_max_s, i16, 8, 2, |a: i16, b: i16| a.max(b));
binop!(i16x8_max_u, u16, 8, 2, |a: u16, b: u16| a.max(b));
binop!(i32x4_min_s, i32, 4, 4, |a: i32, b: i32| a.min(b));
binop!(i32x4_min_u, u32, 4, 4, |a: u32, b: u32| a.min(b));
binop!(i32x4_max_s, i32, 4, 4, |a: i32, b: i32| a.max(b));
binop!(i32x4_max_u, u32, 4, 4, |a: u32, b: u32| a.max(b));

binop!(i8x16_avgr_u, u8, 16, 1, |a: u8, b: u8| ((a as u16 + b as u16 + 1) / 2) as u8);
binop!(i16x8_avgr_u, u16, 8, 2, |a: u16, b: u16| ((a as u32 + b as u32 + 1) / 2) as u16);

// Narrowing
pub fn i8x16_narrow_i16x8_s(a: [u8; 16], b: [u8; 16]) -> [u8; 16] {
    let lanes_a = to_lanes::<2, 8, i16>(a);
    let lanes_b = to_lanes::<2, 8, i16>(b);
    let mut res = [0i8; 16];
    for i in 0..8 {
        res[i] = lanes_a[i].clamp(i8::MIN as i16, i8::MAX as i16) as i8;
        res[i + 8] = lanes_b[i].clamp(i8::MIN as i16, i8::MAX as i16) as i8;
    }
    from_lanes::<1, 16, i8>(res)
}
pub fn i8x16_narrow_i16x8_u(a: [u8; 16], b: [u8; 16]) -> [u8; 16] {
    let lanes_a = to_lanes::<2, 8, i16>(a);
    let lanes_b = to_lanes::<2, 8, i16>(b);
    let mut res = [0u8; 16];
    for i in 0..8 {
        res[i] = lanes_a[i].clamp(0, u8::MAX as i16) as u8;
        res[i + 8] = lanes_b[i].clamp(0, u8::MAX as i16) as u8;
    }
    res
}
pub fn i16x8_narrow_i32x4_s(a: [u8; 16], b: [u8; 16]) -> [u8; 16] {
    let lanes_a = to_lanes::<4, 4, i32>(a);
    let lanes_b = to_lanes::<4, 4, i32>(b);
    let mut res = [0i16; 8];
    for i in 0..4 {
        res[i] = lanes_a[i].clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        res[i + 4] = lanes_b[i].clamp(i16::MIN as i32, i16::MAX as i32) as i16;
    }
    from_lanes::<2, 8, i16>(res)
}
pub fn i16x8_narrow_i32x4_u(a: [u8; 16], b: [u8; 16]) -> [u8; 16] {
    let lanes_a = to_lanes::<4, 4, i32>(a);
    let lanes_b = to_lanes::<4, 4, i32>(b);
    let mut res = [0u16; 8];
    for i in 0..4 {
        res[i] = lanes_a[i].clamp(0, u16::MAX as i32) as u16;
        res[i + 4] = lanes_b[i].clamp(0, u16::MAX as i32) as u16;
    }
    from_lanes::<2, 8, u16>(res)
}

// Widening
// Note: These instructions take ONE v128 operand and return ONE v128 (but conceptually it's half lanes)
// Wait, spec says: i16x8.extend_low_i8x16_s : [v128] -> [v128]
pub fn i16x8_extend_low_i8x16_s(a: [u8; 16]) -> [u8; 16] {
    let lanes_a = to_lanes::<1, 16, i8>(a);
    let mut res = [0i16; 8];
    for i in 0..8 { res[i] = lanes_a[i] as i16; }
    from_lanes::<2, 8, i16>(res)
}
pub fn i16x8_extend_high_i8x16_s(a: [u8; 16]) -> [u8; 16] {
    let lanes_a = to_lanes::<1, 16, i8>(a);
    let mut res = [0i16; 8];
    for i in 0..8 { res[i] = lanes_a[i + 8] as i16; }
    from_lanes::<2, 8, i16>(res)
}
pub fn i16x8_extend_low_i8x16_u(a: [u8; 16]) -> [u8; 16] {
    let lanes_a = to_lanes::<1, 16, u8>(a);
    let mut res = [0u16; 8];
    for i in 0..8 { res[i] = lanes_a[i] as u16; }
    from_lanes::<2, 8, u16>(res)
}
pub fn i16x8_extend_high_i8x16_u(a: [u8; 16]) -> [u8; 16] {
    let lanes_a = to_lanes::<1, 16, u8>(a);
    let mut res = [0u16; 8];
    for i in 0..8 { res[i] = lanes_a[i + 8] as u16; }
    from_lanes::<2, 8, u16>(res)
}

pub fn i32x4_extend_low_i16x8_s(a: [u8; 16]) -> [u8; 16] {
    let lanes_a = to_lanes::<2, 8, i16>(a);
    let mut res = [0i32; 4];
    for i in 0..4 { res[i] = lanes_a[i] as i32; }
    from_lanes::<4, 4, i32>(res)
}
pub fn i32x4_extend_high_i16x8_s(a: [u8; 16]) -> [u8; 16] {
    let lanes_a = to_lanes::<2, 8, i16>(a);
    let mut res = [0i32; 4];
    for i in 0..4 { res[i] = lanes_a[i + 4] as i32; }
    from_lanes::<4, 4, i32>(res)
}
pub fn i32x4_extend_low_i16x8_u(a: [u8; 16]) -> [u8; 16] {
    let lanes_a = to_lanes::<2, 8, u16>(a);
    let mut res = [0u32; 4];
    for i in 0..4 { res[i] = lanes_a[i] as u32; }
    from_lanes::<4, 4, u32>(res)
}
pub fn i32x4_extend_high_i16x8_u(a: [u8; 16]) -> [u8; 16] {
    let lanes_a = to_lanes::<2, 8, u16>(a);
    let mut res = [0u32; 4];
    for i in 0..4 { res[i] = lanes_a[i + 4] as u32; }
    from_lanes::<4, 4, u32>(res)
}

pub fn i64x2_extend_low_i32x4_s(a: [u8; 16]) -> [u8; 16] {
    let lanes_a = to_lanes::<4, 4, i32>(a);
    let mut res = [0i64; 2];
    for i in 0..2 { res[i] = lanes_a[i] as i64; }
    from_lanes::<8, 2, i64>(res)
}
pub fn i64x2_extend_high_i32x4_s(a: [u8; 16]) -> [u8; 16] {
    let lanes_a = to_lanes::<4, 4, i32>(a);
    let mut res = [0i64; 2];
    for i in 0..2 { res[i] = lanes_a[i + 2] as i64; }
    from_lanes::<8, 2, i64>(res)
}
pub fn i64x2_extend_low_i32x4_u(a: [u8; 16]) -> [u8; 16] {
    let lanes_a = to_lanes::<4, 4, u32>(a);
    let mut res = [0u64; 2];
    for i in 0..2 { res[i] = lanes_a[i] as u64; }
    from_lanes::<8, 2, u64>(res)
}
pub fn i64x2_extend_high_i32x4_u(a: [u8; 16]) -> [u8; 16] {
    let lanes_a = to_lanes::<4, 4, u32>(a);
    let mut res = [0u64; 2];
    for i in 0..2 { res[i] = lanes_a[i + 2] as u64; }
    from_lanes::<8, 2, u64>(res)
}

// Misc
pub fn i8x16_abs(a: [u8; 16]) -> [u8; 16] {
    let lanes = to_lanes::<1, 16, i8>(a);
    let res = array::from_fn(|i| lanes[i].abs());
    from_lanes::<1, 16, i8>(res)
}
pub fn i8x16_neg(a: [u8; 16]) -> [u8; 16] {
    let lanes = to_lanes::<1, 16, i8>(a);
    let res = array::from_fn(|i| lanes[i].wrapping_neg());
    from_lanes::<1, 16, i8>(res)
}
pub fn i8x16_popcnt(a: [u8; 16]) -> [u8; 16] {
    let lanes = to_lanes::<1, 16, u8>(a);
    let res = array::from_fn(|i| lanes[i].count_ones() as u8);
    from_lanes::<1, 16, u8>(res)
}

pub fn i16x8_abs(a: [u8; 16]) -> [u8; 16] {
    let lanes = to_lanes::<2, 8, i16>(a);
    let res = array::from_fn(|i| lanes[i].wrapping_abs());
    from_lanes::<2, 8, i16>(res)
}
pub fn i16x8_neg(a: [u8; 16]) -> [u8; 16] {
    let lanes = to_lanes::<2, 8, i16>(a);
    let res = array::from_fn(|i| lanes[i].wrapping_neg());
    from_lanes::<2, 8, i16>(res)
}
// i16x8.q15mulr_sat_s
pub fn i16x8_q15mulrsat_s(a: [u8; 16], b: [u8; 16]) -> [u8; 16] {
    let lanes_a = to_lanes::<2, 8, i16>(a);
    let lanes_b = to_lanes::<2, 8, i16>(b);
    let res = array::from_fn(|i| {
        let val = ((lanes_a[i] as i32 * lanes_b[i] as i32) + 0x4000) >> 15;
        val.clamp(i16::MIN as i32, i16::MAX as i32) as i16
    });
    from_lanes::<2, 8, i16>(res)
}

pub fn i32x4_abs(a: [u8; 16]) -> [u8; 16] {
    let lanes = to_lanes::<4, 4, i32>(a);
    let res = array::from_fn(|i| lanes[i].wrapping_abs());
    from_lanes::<4, 4, i32>(res)
}
pub fn i32x4_neg(a: [u8; 16]) -> [u8; 16] {
    let lanes = to_lanes::<4, 4, i32>(a);
    let res = array::from_fn(|i| lanes[i].wrapping_neg());
    from_lanes::<4, 4, i32>(res)
}
// i32x4.dot_i16x8_s
pub fn i32x4_dot_i16x8_s(a: [u8; 16], b: [u8; 16]) -> [u8; 16] {
    let lanes_a = to_lanes::<2, 8, i16>(a);
    let lanes_b = to_lanes::<2, 8, i16>(b);
    let mut res = [0i32; 4];
    for i in 0..4 {
        let lo = lanes_a[2 * i] as i32 * lanes_b[2 * i] as i32;
        let hi = lanes_a[2 * i + 1] as i32 * lanes_b[2 * i + 1] as i32;
        res[i] = lo.wrapping_add(hi);
    }
    from_lanes::<4, 4, i32>(res)
}

pub fn i64x2_abs(a: [u8; 16]) -> [u8; 16] {
    let lanes = to_lanes::<8, 2, i64>(a);
    let res = array::from_fn(|i| lanes[i].wrapping_abs());
    from_lanes::<8, 2, i64>(res)
}
pub fn i64x2_neg(a: [u8; 16]) -> [u8; 16] {
    let lanes = to_lanes::<8, 2, i64>(a);
    let res = array::from_fn(|i| lanes[i].wrapping_neg());
    from_lanes::<8, 2, i64>(res)
}

// Extmul
pub fn i16x8_extmul_low_i8x16_s(a: [u8; 16], b: [u8; 16]) -> [u8; 16] {
    let la = to_lanes::<1, 16, i8>(a);
    let lb = to_lanes::<1, 16, i8>(b);
    let mut res = [0i16; 8];
    for i in 0..8 { res[i] = (la[i] as i16) * (lb[i] as i16); }
    from_lanes::<2, 8, i16>(res)
}
pub fn i16x8_extmul_high_i8x16_s(a: [u8; 16], b: [u8; 16]) -> [u8; 16] {
    let la = to_lanes::<1, 16, i8>(a);
    let lb = to_lanes::<1, 16, i8>(b);
    let mut res = [0i16; 8];
    for i in 0..8 { res[i] = (la[i + 8] as i16) * (lb[i + 8] as i16); }
    from_lanes::<2, 8, i16>(res)
}
pub fn i16x8_extmul_low_i8x16_u(a: [u8; 16], b: [u8; 16]) -> [u8; 16] {
    let la = to_lanes::<1, 16, u8>(a);
    let lb = to_lanes::<1, 16, u8>(b);
    let mut res = [0u16; 8];
    for i in 0..8 { res[i] = (la[i] as u16) * (lb[i] as u16); }
    from_lanes::<2, 8, u16>(res)
}
pub fn i16x8_extmul_high_i8x16_u(a: [u8; 16], b: [u8; 16]) -> [u8; 16] {
    let la = to_lanes::<1, 16, u8>(a);
    let lb = to_lanes::<1, 16, u8>(b);
    let mut res = [0u16; 8];
    for i in 0..8 { res[i] = (la[i + 8] as u16) * (lb[i + 8] as u16); }
    from_lanes::<2, 8, u16>(res)
}

pub fn i32x4_extmul_low_i16x8_s(a: [u8; 16], b: [u8; 16]) -> [u8; 16] {
    let la = to_lanes::<2, 8, i16>(a);
    let lb = to_lanes::<2, 8, i16>(b);
    let mut res = [0i32; 4];
    for i in 0..4 { res[i] = (la[i] as i32) * (lb[i] as i32); }
    from_lanes::<4, 4, i32>(res)
}
pub fn i32x4_extmul_high_i16x8_s(a: [u8; 16], b: [u8; 16]) -> [u8; 16] {
    let la = to_lanes::<2, 8, i16>(a);
    let lb = to_lanes::<2, 8, i16>(b);
    let mut res = [0i32; 4];
    for i in 0..4 { res[i] = (la[i + 4] as i32) * (lb[i + 4] as i32); }
    from_lanes::<4, 4, i32>(res)
}
pub fn i32x4_extmul_low_i16x8_u(a: [u8; 16], b: [u8; 16]) -> [u8; 16] {
    let la = to_lanes::<2, 8, u16>(a);
    let lb = to_lanes::<2, 8, u16>(b);
    let mut res = [0u32; 4];
    for i in 0..4 { res[i] = (la[i] as u32) * (lb[i] as u32); }
    from_lanes::<4, 4, u32>(res)
}
pub fn i32x4_extmul_high_i16x8_u(a: [u8; 16], b: [u8; 16]) -> [u8; 16] {
    let la = to_lanes::<2, 8, u16>(a);
    let lb = to_lanes::<2, 8, u16>(b);
    let mut res = [0u32; 4];
    for i in 0..4 { res[i] = (la[i + 4] as u32) * (lb[i + 4] as u32); }
    from_lanes::<4, 4, u32>(res)
}

pub fn i64x2_extmul_low_i32x4_s(a: [u8; 16], b: [u8; 16]) -> [u8; 16] {
    let la = to_lanes::<4, 4, i32>(a);
    let lb = to_lanes::<4, 4, i32>(b);
    let mut res = [0i64; 2];
    for i in 0..2 { res[i] = (la[i] as i64) * (lb[i] as i64); }
    from_lanes::<8, 2, i64>(res)
}
pub fn i64x2_extmul_high_i32x4_s(a: [u8; 16], b: [u8; 16]) -> [u8; 16] {
    let la = to_lanes::<4, 4, i32>(a);
    let lb = to_lanes::<4, 4, i32>(b);
    let mut res = [0i64; 2];
    for i in 0..2 { res[i] = (la[i + 2] as i64) * (lb[i + 2] as i64); }
    from_lanes::<8, 2, i64>(res)
}
pub fn i64x2_extmul_low_i32x4_u(a: [u8; 16], b: [u8; 16]) -> [u8; 16] {
    let la = to_lanes::<4, 4, u32>(a);
    let lb = to_lanes::<4, 4, u32>(b);
    let mut res = [0u64; 2];
    for i in 0..2 { res[i] = (la[i] as u64) * (lb[i] as u64); }
    from_lanes::<8, 2, u64>(res)
}
pub fn i64x2_extmul_high_i32x4_u(a: [u8; 16], b: [u8; 16]) -> [u8; 16] {
    let la = to_lanes::<4, 4, u32>(a);
    let lb = to_lanes::<4, 4, u32>(b);
    let mut res = [0u64; 2];
    for i in 0..2 { res[i] = (la[i + 2] as u64) * (lb[i + 2] as u64); }
    from_lanes::<8, 2, u64>(res)
}

// Extadd pairwise
pub fn i16x8_extadd_pairwise_i8x16_s(a: [u8; 16]) -> [u8; 16] {
    let la = to_lanes::<1, 16, i8>(a);
    let mut res = [0i16; 8];
    for i in 0..8 { res[i] = (la[2 * i] as i16) + (la[2 * i + 1] as i16); }
    from_lanes::<2, 8, i16>(res)
}
pub fn i16x8_extadd_pairwise_i8x16_u(a: [u8; 16]) -> [u8; 16] {
    let la = to_lanes::<1, 16, u8>(a);
    let mut res = [0u16; 8];
    for i in 0..8 { res[i] = (la[2 * i] as u16) + (la[2 * i + 1] as u16); }
    from_lanes::<2, 8, u16>(res)
}
pub fn i32x4_extadd_pairwise_i16x8_s(a: [u8; 16]) -> [u8; 16] {
    let la = to_lanes::<2, 8, i16>(a);
    let mut res = [0i32; 4];
    for i in 0..4 { res[i] = (la[2 * i] as i32) + (la[2 * i + 1] as i32); }
    from_lanes::<4, 4, i32>(res)
}
pub fn i32x4_extadd_pairwise_i16x8_u(a: [u8; 16]) -> [u8; 16] {
    let la = to_lanes::<2, 8, u16>(a);
    let mut res = [0u32; 4];
    for i in 0..4 { res[i] = (la[2 * i] as u32) + (la[2 * i + 1] as u32); }
    from_lanes::<4, 4, u32>(res)
}

pub fn i8x16_swizzle(a: [u8; 16], s: [u8; 16]) -> [u8; 16] {
    let mut res = [0u8; 16];
    for i in 0..16 {
        let idx = s[i];
        res[i] = if idx < 16 { a[idx as usize] } else { 0 };
    }
    res
}

pub fn i8x16_shuffle(a: [u8; 16], b: [u8; 16], lanes: [u8; 16]) -> [u8; 16] {
    let mut res = [0u8; 16];
    for i in 0..16 {
        let idx = lanes[i];
        res[i] = if idx < 16 { a[idx as usize] } else { b[(idx - 16) as usize] };
    }
    res
}

pub fn splat<const LANE_SIZE: usize>(val: [u8; LANE_SIZE]) -> [u8; 16] {
    let mut res = [0u8; 16];
    for i in 0..(16 / LANE_SIZE) {
        res[i * LANE_SIZE..(i + 1) * LANE_SIZE].copy_from_slice(&val);
    }
    res
}

// Conversions
pub fn i32x4_trunc_sat_f32x4_s(a: [u8; 16]) -> [u8; 16] {
    let lanes = to_lanes::<4, 4, F32>(a);
    let res = array::from_fn(|i| {
        let f = lanes[i].0;
        if f.is_nan() { 0 } else if f >= 2147483648.0 { i32::MAX } else if f < -2147483648.0 { i32::MIN } else { f as i32 }
    });
    from_lanes::<4, 4, i32>(res)
}
pub fn i32x4_trunc_sat_f32x4_u(a: [u8; 16]) -> [u8; 16] {
    let lanes = to_lanes::<4, 4, F32>(a);
    let res = array::from_fn(|i| {
        let f = lanes[i].0;
        if f.is_nan() { 0 } else if f >= 4294967296.0 { u32::MAX } else if f <= -1.0 { 0 } else { f as u32 }
    });
    from_lanes::<4, 4, u32>(res)
}
pub fn f32x4_convert_i32x4_s(a: [u8; 16]) -> [u8; 16] {
    let lanes = to_lanes::<4, 4, i32>(a);
    let res = array::from_fn(|i| F32(lanes[i] as f32));
    from_lanes::<4, 4, F32>(res)
}
pub fn f32x4_convert_i32x4_u(a: [u8; 16]) -> [u8; 16] {
    let lanes = to_lanes::<4, 4, u32>(a);
    let res = array::from_fn(|i| F32(lanes[i] as f32));
    from_lanes::<4, 4, F32>(res)
}
// Zero-extending truncates
pub fn i32x4_trunc_sat_f64x2_s_zero(a: [u8; 16]) -> [u8; 16] {
    let lanes = to_lanes::<8, 2, F64>(a);
    let mut res = [0i32; 4];
    for i in 0..2 {
        let f = lanes[i].0;
        res[i] = if f.is_nan() { 0 } else if f >= 2147483648.0 { i32::MAX } else if f < -2147483648.0 { i32::MIN } else { f as i32 };
    }
    // Upper lanes zeroed (already 0)
    from_lanes::<4, 4, i32>(res)
}
pub fn i32x4_trunc_sat_f64x2_u_zero(a: [u8; 16]) -> [u8; 16] {
    let lanes = to_lanes::<8, 2, F64>(a);
    let mut res = [0u32; 4];
    for i in 0..2 {
        let f = lanes[i].0;
        res[i] = if f.is_nan() { 0 } else if f >= 4294967296.0 { u32::MAX } else if f <= -1.0 { 0 } else { f as u32 };
    }
    from_lanes::<4, 4, u32>(res)
}
pub fn f64x2_convert_low_i32x4_s(a: [u8; 16]) -> [u8; 16] {
    let lanes = to_lanes::<4, 4, i32>(a);
    let mut res = [F64(0.0); 2];
    for i in 0..2 { res[i] = F64(lanes[i] as f64); }
    from_lanes::<8, 2, F64>(res)
}
pub fn f64x2_convert_low_i32x4_u(a: [u8; 16]) -> [u8; 16] {
    let lanes = to_lanes::<4, 4, u32>(a);
    let mut res = [F64(0.0); 2];
    for i in 0..2 { res[i] = F64(lanes[i] as f64); }
    from_lanes::<8, 2, F64>(res)
}
