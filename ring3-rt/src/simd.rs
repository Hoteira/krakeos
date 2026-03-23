// SIMD helper functions for AOT-compiled WASM in ring3.
// These are called via the JUMP_TABLE from AOT code.
// Signatures match what the AOT compiler emits (NOT the JUMP_TABLE type).

use crate::context::Ring3Context;

// ===== Lane conversion helpers =====

#[inline(always)]
fn lanes_i8(a: &[u8; 16]) -> [i8; 16] {
    unsafe { core::mem::transmute(*a) }
}
#[inline(always)]
fn lanes_i16(a: &[u8; 16]) -> [i16; 8] {
    unsafe { core::mem::transmute(*a) }
}
#[inline(always)]
fn lanes_u16(a: &[u8; 16]) -> [u16; 8] {
    unsafe { core::mem::transmute(*a) }
}
#[inline(always)]
fn lanes_i32(a: &[u8; 16]) -> [i32; 4] {
    unsafe { core::mem::transmute(*a) }
}
#[inline(always)]
fn lanes_u32(a: &[u8; 16]) -> [u32; 4] {
    unsafe { core::mem::transmute(*a) }
}
#[inline(always)]
fn lanes_i64(a: &[u8; 16]) -> [i64; 2] {
    unsafe { core::mem::transmute(*a) }
}
#[inline(always)]
fn lanes_u64(a: &[u8; 16]) -> [u64; 2] {
    unsafe { core::mem::transmute(*a) }
}
#[inline(always)]
fn lanes_f32(a: &[u8; 16]) -> [f32; 4] {
    unsafe { core::mem::transmute(*a) }
}
#[inline(always)]
fn lanes_f64(a: &[u8; 16]) -> [f64; 2] {
    unsafe { core::mem::transmute(*a) }
}
#[inline(always)]
fn from_i8(v: [i8; 16]) -> [u8; 16] {
    unsafe { core::mem::transmute(v) }
}
#[inline(always)]
fn from_i16(v: [i16; 8]) -> [u8; 16] {
    unsafe { core::mem::transmute(v) }
}
#[inline(always)]
fn from_u16(v: [u16; 8]) -> [u8; 16] {
    unsafe { core::mem::transmute(v) }
}
#[inline(always)]
fn from_i32(v: [i32; 4]) -> [u8; 16] {
    unsafe { core::mem::transmute(v) }
}
#[inline(always)]
fn from_u32(v: [u32; 4]) -> [u8; 16] {
    unsafe { core::mem::transmute(v) }
}
#[inline(always)]
fn from_i64(v: [i64; 2]) -> [u8; 16] {
    unsafe { core::mem::transmute(v) }
}
#[inline(always)]
fn from_u64(v: [u64; 2]) -> [u8; 16] {
    unsafe { core::mem::transmute(v) }
}
#[inline(always)]
fn from_f32(v: [f32; 4]) -> [u8; 16] {
    unsafe { core::mem::transmute(v) }
}
#[inline(always)]
fn from_f64(v: [f64; 2]) -> [u8; 16] {
    unsafe { core::mem::transmute(v) }
}

// Float intrinsics via x86_64 SSE instructions (no_std compatible)
#[inline(always)]
fn sqrt_f32(x: f32) -> f32 {
    let r: f32;
    unsafe { core::arch::asm!("sqrtss {0}, {0}", inout(xmm_reg) x => r, options(pure, nomem, nostack)); }
    r
}
#[inline(always)]
fn sqrt_f64(x: f64) -> f64 {
    let r: f64;
    unsafe { core::arch::asm!("sqrtsd {0}, {0}", inout(xmm_reg) x => r, options(pure, nomem, nostack)); }
    r
}
#[inline(always)]
fn ceil_f32(x: f32) -> f32 {
    let r: f32;
    unsafe { core::arch::asm!("roundss {0}, {0}, 0x02", inout(xmm_reg) x => r, options(pure, nomem, nostack)); }
    r
}
#[inline(always)]
fn ceil_f64(x: f64) -> f64 {
    let r: f64;
    unsafe { core::arch::asm!("roundsd {0}, {0}, 0x02", inout(xmm_reg) x => r, options(pure, nomem, nostack)); }
    r
}
#[inline(always)]
fn floor_f32(x: f32) -> f32 {
    let r: f32;
    unsafe { core::arch::asm!("roundss {0}, {0}, 0x01", inout(xmm_reg) x => r, options(pure, nomem, nostack)); }
    r
}
#[inline(always)]
fn floor_f64(x: f64) -> f64 {
    let r: f64;
    unsafe { core::arch::asm!("roundsd {0}, {0}, 0x01", inout(xmm_reg) x => r, options(pure, nomem, nostack)); }
    r
}
#[inline(always)]
fn trunc_f32(x: f32) -> f32 {
    let r: f32;
    unsafe { core::arch::asm!("roundss {0}, {0}, 0x03", inout(xmm_reg) x => r, options(pure, nomem, nostack)); }
    r
}
#[inline(always)]
fn trunc_f64(x: f64) -> f64 {
    let r: f64;
    unsafe { core::arch::asm!("roundsd {0}, {0}, 0x03", inout(xmm_reg) x => r, options(pure, nomem, nostack)); }
    r
}
// Wasm "nearest" = round half to even (roundss/roundsd imm=0)
#[inline(always)]
fn nearest_f32(x: f32) -> f32 {
    let r: f32;
    unsafe { core::arch::asm!("roundss {0}, {0}, 0x00", inout(xmm_reg) x => r, options(pure, nomem, nostack)); }
    r
}
#[inline(always)]
fn nearest_f64(x: f64) -> f64 {
    let r: f64;
    unsafe { core::arch::asm!("roundsd {0}, {0}, 0x00", inout(xmm_reg) x => r, options(pure, nomem, nostack)); }
    r
}
#[inline(always)]
fn abs_f32(x: f32) -> f32 {
    f32::from_bits(x.to_bits() & 0x7FFFFFFF)
}
#[inline(always)]
fn abs_f64(x: f64) -> f64 {
    f64::from_bits(x.to_bits() & 0x7FFFFFFFFFFFFFFF)
}

// Wasm float min/max (propagate NaN, handle -0/+0)
#[inline(always)]
fn wasm_f32_min(a: f32, b: f32) -> f32 {
    if a.is_nan() || b.is_nan() {
        return f32::NAN;
    }
    if a == 0.0 && b == 0.0 {
        return f32::from_bits(a.to_bits() | b.to_bits()); // -0 < +0
    }
    if a < b { a } else { b }
}
#[inline(always)]
fn wasm_f32_max(a: f32, b: f32) -> f32 {
    if a.is_nan() || b.is_nan() {
        return f32::NAN;
    }
    if a == 0.0 && b == 0.0 {
        return f32::from_bits(a.to_bits() & b.to_bits());
    }
    if a > b { a } else { b }
}
#[inline(always)]
fn wasm_f64_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        return f64::NAN;
    }
    if a == 0.0 && b == 0.0 {
        return f64::from_bits(a.to_bits() | b.to_bits());
    }
    if a < b { a } else { b }
}
#[inline(always)]
fn wasm_f64_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        return f64::NAN;
    }
    if a == 0.0 && b == 0.0 {
        return f64::from_bits(a.to_bits() & b.to_bits());
    }
    if a > b { a } else { b }
}

// ===== Macros for generating extern "C" functions =====

macro_rules! simd_binop_i8 {
    ($name:ident, $op:expr) => {
        pub extern "C" fn $name(a: *mut [u8; 16], b: *const [u8; 16]) {
            unsafe {
                let la = lanes_i8(&*a);
                let lb = lanes_i8(&*b);
                let mut r = [0i8; 16];
                let mut i = 0;
                while i < 16 { r[i] = $op(la[i], lb[i]); i += 1; }
                *a = from_i8(r);
            }
        }
    };
}

macro_rules! simd_binop_u8 {
    ($name:ident, $op:expr) => {
        pub extern "C" fn $name(a: *mut [u8; 16], b: *const [u8; 16]) {
            unsafe {
                let la = *a;
                let lb = *b;
                let mut r = [0u8; 16];
                let mut i = 0;
                while i < 16 { r[i] = $op(la[i], lb[i]); i += 1; }
                *a = r;
            }
        }
    };
}

macro_rules! simd_binop_i16 {
    ($name:ident, $op:expr) => {
        pub extern "C" fn $name(a: *mut [u8; 16], b: *const [u8; 16]) {
            unsafe {
                let la = lanes_i16(&*a);
                let lb = lanes_i16(&*b);
                let mut r = [0i16; 8];
                let mut i = 0;
                while i < 8 { r[i] = $op(la[i], lb[i]); i += 1; }
                *a = from_i16(r);
            }
        }
    };
}

macro_rules! simd_binop_u16 {
    ($name:ident, $op:expr) => {
        pub extern "C" fn $name(a: *mut [u8; 16], b: *const [u8; 16]) {
            unsafe {
                let la = lanes_u16(&*a);
                let lb = lanes_u16(&*b);
                let mut r = [0u16; 8];
                let mut i = 0;
                while i < 8 { r[i] = $op(la[i], lb[i]); i += 1; }
                *a = from_u16(r);
            }
        }
    };
}

macro_rules! simd_binop_i32 {
    ($name:ident, $op:expr) => {
        pub extern "C" fn $name(a: *mut [u8; 16], b: *const [u8; 16]) {
            unsafe {
                let la = lanes_i32(&*a);
                let lb = lanes_i32(&*b);
                let mut r = [0i32; 4];
                let mut i = 0;
                while i < 4 { r[i] = $op(la[i], lb[i]); i += 1; }
                *a = from_i32(r);
            }
        }
    };
}

macro_rules! simd_binop_u32 {
    ($name:ident, $op:expr) => {
        pub extern "C" fn $name(a: *mut [u8; 16], b: *const [u8; 16]) {
            unsafe {
                let la = lanes_u32(&*a);
                let lb = lanes_u32(&*b);
                let mut r = [0u32; 4];
                let mut i = 0;
                while i < 4 { r[i] = $op(la[i], lb[i]); i += 1; }
                *a = from_u32(r);
            }
        }
    };
}

macro_rules! simd_binop_i64 {
    ($name:ident, $op:expr) => {
        pub extern "C" fn $name(a: *mut [u8; 16], b: *const [u8; 16]) {
            unsafe {
                let la = lanes_i64(&*a);
                let lb = lanes_i64(&*b);
                let mut r = [0i64; 2];
                let mut i = 0;
                while i < 2 { r[i] = $op(la[i], lb[i]); i += 1; }
                *a = from_i64(r);
            }
        }
    };
}

macro_rules! simd_binop_f32 {
    ($name:ident, $op:expr) => {
        pub extern "C" fn $name(a: *mut [u8; 16], b: *const [u8; 16]) {
            unsafe {
                let la = lanes_f32(&*a);
                let lb = lanes_f32(&*b);
                let mut r = [0.0f32; 4];
                let mut i = 0;
                while i < 4 { r[i] = $op(la[i], lb[i]); i += 1; }
                *a = from_f32(r);
            }
        }
    };
}

macro_rules! simd_binop_f64 {
    ($name:ident, $op:expr) => {
        pub extern "C" fn $name(a: *mut [u8; 16], b: *const [u8; 16]) {
            unsafe {
                let la = lanes_f64(&*a);
                let lb = lanes_f64(&*b);
                let mut r = [0.0f64; 2];
                let mut i = 0;
                while i < 2 { r[i] = $op(la[i], lb[i]); i += 1; }
                *a = from_f64(r);
            }
        }
    };
}

macro_rules! simd_relop_i8 {
    ($name:ident, $op:expr) => {
        pub extern "C" fn $name(a: *mut [u8; 16], b: *const [u8; 16]) {
            unsafe {
                let la = lanes_i8(&*a);
                let lb = lanes_i8(&*b);
                let mut r = [0u8; 16];
                let mut i = 0;
                while i < 16 {
                    r[i] = if $op(la[i], lb[i]) { 0xFF } else { 0 };
                    i += 1;
                }
                *a = r;
            }
        }
    };
}

macro_rules! simd_relop_u8 {
    ($name:ident, $op:expr) => {
        pub extern "C" fn $name(a: *mut [u8; 16], b: *const [u8; 16]) {
            unsafe {
                let la = *a;
                let lb = *b;
                let mut r = [0u8; 16];
                let mut i = 0;
                while i < 16 {
                    r[i] = if $op(la[i], lb[i]) { 0xFF } else { 0 };
                    i += 1;
                }
                *a = r;
            }
        }
    };
}

macro_rules! simd_relop_i16 {
    ($name:ident, $op:expr) => {
        pub extern "C" fn $name(a: *mut [u8; 16], b: *const [u8; 16]) {
            unsafe {
                let la = lanes_i16(&*a);
                let lb = lanes_i16(&*b);
                let mut r = [0i16; 8];
                let mut i = 0;
                while i < 8 {
                    r[i] = if $op(la[i], lb[i]) { -1 } else { 0 };
                    i += 1;
                }
                *a = from_i16(r);
            }
        }
    };
}

macro_rules! simd_relop_u16 {
    ($name:ident, $op:expr) => {
        pub extern "C" fn $name(a: *mut [u8; 16], b: *const [u8; 16]) {
            unsafe {
                let la = lanes_u16(&*a);
                let lb = lanes_u16(&*b);
                let mut r = [0u16; 8];
                let mut i = 0;
                while i < 8 {
                    r[i] = if $op(la[i], lb[i]) { 0xFFFF } else { 0 };
                    i += 1;
                }
                *a = from_u16(r);
            }
        }
    };
}

macro_rules! simd_relop_i32 {
    ($name:ident, $op:expr) => {
        pub extern "C" fn $name(a: *mut [u8; 16], b: *const [u8; 16]) {
            unsafe {
                let la = lanes_i32(&*a);
                let lb = lanes_i32(&*b);
                let mut r = [0i32; 4];
                let mut i = 0;
                while i < 4 {
                    r[i] = if $op(la[i], lb[i]) { -1 } else { 0 };
                    i += 1;
                }
                *a = from_i32(r);
            }
        }
    };
}

macro_rules! simd_relop_u32 {
    ($name:ident, $op:expr) => {
        pub extern "C" fn $name(a: *mut [u8; 16], b: *const [u8; 16]) {
            unsafe {
                let la = lanes_u32(&*a);
                let lb = lanes_u32(&*b);
                let mut r = [0u32; 4];
                let mut i = 0;
                while i < 4 {
                    r[i] = if $op(la[i], lb[i]) { 0xFFFFFFFF } else { 0 };
                    i += 1;
                }
                *a = from_u32(r);
            }
        }
    };
}

macro_rules! simd_relop_i64 {
    ($name:ident, $op:expr) => {
        pub extern "C" fn $name(a: *mut [u8; 16], b: *const [u8; 16]) {
            unsafe {
                let la = lanes_i64(&*a);
                let lb = lanes_i64(&*b);
                let mut r = [0i64; 2];
                let mut i = 0;
                while i < 2 {
                    r[i] = if $op(la[i], lb[i]) { -1 } else { 0 };
                    i += 1;
                }
                *a = from_i64(r);
            }
        }
    };
}

macro_rules! simd_relop_f32 {
    ($name:ident, $op:expr) => {
        pub extern "C" fn $name(a: *mut [u8; 16], b: *const [u8; 16]) {
            unsafe {
                let la = lanes_f32(&*a);
                let lb = lanes_f32(&*b);
                let mut r = [0u32; 4];
                let mut i = 0;
                while i < 4 {
                    r[i] = if $op(la[i], lb[i]) { 0xFFFFFFFF } else { 0 };
                    i += 1;
                }
                *a = from_u32(r);
            }
        }
    };
}

macro_rules! simd_relop_f64 {
    ($name:ident, $op:expr) => {
        pub extern "C" fn $name(a: *mut [u8; 16], b: *const [u8; 16]) {
            unsafe {
                let la = lanes_f64(&*a);
                let lb = lanes_f64(&*b);
                let mut r = [0u64; 2];
                let mut i = 0;
                while i < 2 {
                    r[i] = if $op(la[i], lb[i]) { 0xFFFFFFFFFFFFFFFF } else { 0 };
                    i += 1;
                }
                *a = from_u64(r);
            }
        }
    };
}

// ===== Bitwise ops (71-73, 181, 254) =====

pub extern "C" fn v128_and(a: *mut [u8; 16], b: *const [u8; 16]) {
    unsafe { let mut i = 0; while i < 16 { (*a)[i] &= (*b)[i]; i += 1; } }
}
pub extern "C" fn v128_or(a: *mut [u8; 16], b: *const [u8; 16]) {
    unsafe { let mut i = 0; while i < 16 { (*a)[i] |= (*b)[i]; i += 1; } }
}
pub extern "C" fn v128_xor(a: *mut [u8; 16], b: *const [u8; 16]) {
    unsafe { let mut i = 0; while i < 16 { (*a)[i] ^= (*b)[i]; i += 1; } }
}
pub extern "C" fn v128_andnot(a: *mut [u8; 16], b: *const [u8; 16]) {
    unsafe { let mut i = 0; while i < 16 { (*a)[i] &= !(*b)[i]; i += 1; } }
}
pub extern "C" fn v128_not(a: *mut [u8; 16]) {
    unsafe { let mut i = 0; while i < 16 { (*a)[i] = !(*a)[i]; i += 1; } }
}

// ===== Bitselect (74) - ternary =====

pub extern "C" fn v128_bitselect(a: *mut [u8; 16], b: *const [u8; 16], c: *const [u8; 16]) {
    unsafe {
        let mut i = 0;
        while i < 16 {
            (*a)[i] = ((*a)[i] & (*c)[i]) | ((*b)[i] & !(*c)[i]);
            i += 1;
        }
    }
}

// ===== Equality ops (75-80) - these are the V128Eq* variants =====

simd_relop_i8!(v128_eq_i8x16, |a: i8, b: i8| a == b);
simd_relop_i16!(v128_eq_i16x8, |a: i16, b: i16| a == b);
simd_relop_i32!(v128_eq_i32x4, |a: i32, b: i32| a == b);
simd_relop_i64!(v128_eq_i64x2, |a: i64, b: i64| a == b);
simd_relop_f32!(v128_eq_f32x4, |a: f32, b: f32| a == b);
simd_relop_f64!(v128_eq_f64x2, |a: f64, b: f64| a == b);

// ===== Reductions (81, 82, 205-211) =====

pub extern "C" fn v128_any_true(a: *const [u8; 16]) -> i32 {
    unsafe {
        let mut i = 0;
        while i < 16 { if (*a)[i] != 0 { return 1; } i += 1; }
        0
    }
}

pub extern "C" fn v128_bitmask_i8x16(a: *const [u8; 16]) -> i32 {
    unsafe {
        let la = lanes_i8(&*a);
        let mut mask = 0i32;
        let mut i = 0;
        while i < 16 { if la[i] < 0 { mask |= 1 << i; } i += 1; }
        mask
    }
}

pub extern "C" fn v128_bitmask_i16x8(a: *const [u8; 16]) -> i32 {
    unsafe {
        let la = lanes_i16(&*a);
        let mut mask = 0i32;
        let mut i = 0;
        while i < 8 { if la[i] < 0 { mask |= 1 << i; } i += 1; }
        mask
    }
}

pub extern "C" fn v128_bitmask_i32x4(a: *const [u8; 16]) -> i32 {
    unsafe {
        let la = lanes_i32(&*a);
        let mut mask = 0i32;
        let mut i = 0;
        while i < 4 { if la[i] < 0 { mask |= 1 << i; } i += 1; }
        mask
    }
}

pub extern "C" fn v128_bitmask_i64x2(a: *const [u8; 16]) -> i32 {
    unsafe {
        let la = lanes_i64(&*a);
        let mut mask = 0i32;
        let mut i = 0;
        while i < 2 { if la[i] < 0 { mask |= 1 << i; } i += 1; }
        mask
    }
}

pub extern "C" fn v128_all_true_i8x16(a: *const [u8; 16]) -> i32 {
    unsafe {
        let mut i = 0;
        while i < 16 { if (*a)[i] == 0 { return 0; } i += 1; }
        1
    }
}

pub extern "C" fn v128_all_true_i16x8(a: *const [u8; 16]) -> i32 {
    unsafe {
        let la = lanes_i16(&*a);
        let mut i = 0;
        while i < 8 { if la[i] == 0 { return 0; } i += 1; }
        1
    }
}

pub extern "C" fn v128_all_true_i32x4(a: *const [u8; 16]) -> i32 {
    unsafe {
        let la = lanes_i32(&*a);
        let mut i = 0;
        while i < 4 { if la[i] == 0 { return 0; } i += 1; }
        1
    }
}

pub extern "C" fn v128_all_true_i64x2(a: *const [u8; 16]) -> i32 {
    unsafe {
        let la = lanes_i64(&*a);
        let mut i = 0;
        while i < 2 { if la[i] == 0 { return 0; } i += 1; }
        1
    }
}

// ===== Shuffle (83) - ternary =====

pub extern "C" fn i8x16_shuffle(a: *mut [u8; 16], b: *const [u8; 16], c: *const [u8; 16]) {
    unsafe {
        let va = *a;
        let vb = *b;
        let lanes = *c;
        let mut r = [0u8; 16];
        let mut i = 0;
        while i < 16 {
            let idx = lanes[i];
            r[i] = if idx < 16 { va[idx as usize] } else { vb[(idx - 16) as usize] };
            i += 1;
        }
        *a = r;
    }
}

// ===== Integer arithmetic (84-94) =====

simd_binop_i8!(i8x16_add, |a: i8, b: i8| a.wrapping_add(b));
simd_binop_i8!(i8x16_sub, |a: i8, b: i8| a.wrapping_sub(b));
simd_binop_i16!(i16x8_add, |a: i16, b: i16| a.wrapping_add(b));
simd_binop_i16!(i16x8_sub, |a: i16, b: i16| a.wrapping_sub(b));
simd_binop_i16!(i16x8_mul, |a: i16, b: i16| a.wrapping_mul(b));
simd_binop_i32!(i32x4_add, |a: i32, b: i32| a.wrapping_add(b));
simd_binop_i32!(i32x4_sub, |a: i32, b: i32| a.wrapping_sub(b));
simd_binop_i32!(i32x4_mul, |a: i32, b: i32| a.wrapping_mul(b));
simd_binop_i64!(i64x2_add, |a: i64, b: i64| a.wrapping_add(b));
simd_binop_i64!(i64x2_sub, |a: i64, b: i64| a.wrapping_sub(b));
simd_binop_i64!(i64x2_mul, |a: i64, b: i64| a.wrapping_mul(b));

// ===== Float arithmetic (95-110) =====

simd_binop_f32!(f32x4_add, |a: f32, b: f32| a + b);
simd_binop_f32!(f32x4_sub, |a: f32, b: f32| a - b);
simd_binop_f32!(f32x4_mul, |a: f32, b: f32| a * b);
simd_binop_f32!(f32x4_div, |a: f32, b: f32| a / b);
simd_binop_f32!(f32x4_min, |a: f32, b: f32| wasm_f32_min(a, b));
simd_binop_f32!(f32x4_max, |a: f32, b: f32| wasm_f32_max(a, b));
simd_binop_f32!(f32x4_pmin, |a: f32, b: f32| if b < a { b } else { a });
simd_binop_f32!(f32x4_pmax, |a: f32, b: f32| if b > a { b } else { a });

simd_binop_f64!(f64x2_add, |a: f64, b: f64| a + b);
simd_binop_f64!(f64x2_sub, |a: f64, b: f64| a - b);
simd_binop_f64!(f64x2_mul, |a: f64, b: f64| a * b);
simd_binop_f64!(f64x2_div, |a: f64, b: f64| a / b);
simd_binop_f64!(f64x2_min, |a: f64, b: f64| wasm_f64_min(a, b));
simd_binop_f64!(f64x2_max, |a: f64, b: f64| wasm_f64_max(a, b));
simd_binop_f64!(f64x2_pmin, |a: f64, b: f64| if b < a { b } else { a });
simd_binop_f64!(f64x2_pmax, |a: f64, b: f64| if b > a { b } else { a });

// ===== Integer relational (111-146) =====

simd_relop_i8!(i8x16_eq, |a: i8, b: i8| a == b);
simd_relop_i8!(i8x16_ne, |a: i8, b: i8| a != b);
simd_relop_i8!(i8x16_lt_s, |a: i8, b: i8| a < b);
simd_relop_u8!(i8x16_lt_u, |a: u8, b: u8| a < b);
simd_relop_i8!(i8x16_gt_s, |a: i8, b: i8| a > b);
simd_relop_u8!(i8x16_gt_u, |a: u8, b: u8| a > b);
simd_relop_i8!(i8x16_le_s, |a: i8, b: i8| a <= b);
simd_relop_u8!(i8x16_le_u, |a: u8, b: u8| a <= b);
simd_relop_i8!(i8x16_ge_s, |a: i8, b: i8| a >= b);
simd_relop_u8!(i8x16_ge_u, |a: u8, b: u8| a >= b);

simd_relop_i16!(i16x8_eq, |a: i16, b: i16| a == b);
simd_relop_i16!(i16x8_ne, |a: i16, b: i16| a != b);
simd_relop_i16!(i16x8_lt_s, |a: i16, b: i16| a < b);
simd_relop_u16!(i16x8_lt_u, |a: u16, b: u16| a < b);
simd_relop_i16!(i16x8_gt_s, |a: i16, b: i16| a > b);
simd_relop_u16!(i16x8_gt_u, |a: u16, b: u16| a > b);
simd_relop_i16!(i16x8_le_s, |a: i16, b: i16| a <= b);
simd_relop_u16!(i16x8_le_u, |a: u16, b: u16| a <= b);
simd_relop_i16!(i16x8_ge_s, |a: i16, b: i16| a >= b);
simd_relop_u16!(i16x8_ge_u, |a: u16, b: u16| a >= b);

simd_relop_i32!(i32x4_eq, |a: i32, b: i32| a == b);
simd_relop_i32!(i32x4_ne, |a: i32, b: i32| a != b);
simd_relop_i32!(i32x4_lt_s, |a: i32, b: i32| a < b);
simd_relop_u32!(i32x4_lt_u, |a: u32, b: u32| a < b);
simd_relop_i32!(i32x4_gt_s, |a: i32, b: i32| a > b);
simd_relop_u32!(i32x4_gt_u, |a: u32, b: u32| a > b);
simd_relop_i32!(i32x4_le_s, |a: i32, b: i32| a <= b);
simd_relop_u32!(i32x4_le_u, |a: u32, b: u32| a <= b);
simd_relop_i32!(i32x4_ge_s, |a: i32, b: i32| a >= b);
simd_relop_u32!(i32x4_ge_u, |a: u32, b: u32| a >= b);

simd_relop_i64!(i64x2_eq, |a: i64, b: i64| a == b);
simd_relop_i64!(i64x2_ne, |a: i64, b: i64| a != b);
simd_relop_i64!(i64x2_lt_s, |a: i64, b: i64| a < b);
simd_relop_i64!(i64x2_gt_s, |a: i64, b: i64| a > b);
simd_relop_i64!(i64x2_le_s, |a: i64, b: i64| a <= b);
simd_relop_i64!(i64x2_ge_s, |a: i64, b: i64| a >= b);

// ===== Float relational (147-158) =====

simd_relop_f32!(f32x4_eq, |a: f32, b: f32| a == b);
simd_relop_f32!(f32x4_ne, |a: f32, b: f32| a != b);
simd_relop_f32!(f32x4_lt, |a: f32, b: f32| a < b);
simd_relop_f32!(f32x4_gt, |a: f32, b: f32| a > b);
simd_relop_f32!(f32x4_le, |a: f32, b: f32| a <= b);
simd_relop_f32!(f32x4_ge, |a: f32, b: f32| a >= b);

simd_relop_f64!(f64x2_eq, |a: f64, b: f64| a == b);
simd_relop_f64!(f64x2_ne, |a: f64, b: f64| a != b);
simd_relop_f64!(f64x2_lt, |a: f64, b: f64| a < b);
simd_relop_f64!(f64x2_gt, |a: f64, b: f64| a > b);
simd_relop_f64!(f64x2_le, |a: f64, b: f64| a <= b);
simd_relop_f64!(f64x2_ge, |a: f64, b: f64| a >= b);

// ===== Integer unary (159-166) =====

pub extern "C" fn i8x16_neg(a: *mut [u8; 16]) {
    unsafe {
        let la = lanes_i8(&*a);
        let mut r = [0i8; 16];
        let mut i = 0;
        while i < 16 { r[i] = la[i].wrapping_neg(); i += 1; }
        *a = from_i8(r);
    }
}
pub extern "C" fn i8x16_abs(a: *mut [u8; 16]) {
    unsafe {
        let la = lanes_i8(&*a);
        let mut r = [0i8; 16];
        let mut i = 0;
        while i < 16 { r[i] = la[i].wrapping_abs(); i += 1; }
        *a = from_i8(r);
    }
}
pub extern "C" fn i16x8_neg(a: *mut [u8; 16]) {
    unsafe {
        let la = lanes_i16(&*a);
        let mut r = [0i16; 8];
        let mut i = 0;
        while i < 8 { r[i] = la[i].wrapping_neg(); i += 1; }
        *a = from_i16(r);
    }
}
pub extern "C" fn i16x8_abs(a: *mut [u8; 16]) {
    unsafe {
        let la = lanes_i16(&*a);
        let mut r = [0i16; 8];
        let mut i = 0;
        while i < 8 { r[i] = la[i].wrapping_abs(); i += 1; }
        *a = from_i16(r);
    }
}
pub extern "C" fn i32x4_neg(a: *mut [u8; 16]) {
    unsafe {
        let la = lanes_i32(&*a);
        let mut r = [0i32; 4];
        let mut i = 0;
        while i < 4 { r[i] = la[i].wrapping_neg(); i += 1; }
        *a = from_i32(r);
    }
}
pub extern "C" fn i32x4_abs(a: *mut [u8; 16]) {
    unsafe {
        let la = lanes_i32(&*a);
        let mut r = [0i32; 4];
        let mut i = 0;
        while i < 4 { r[i] = la[i].wrapping_abs(); i += 1; }
        *a = from_i32(r);
    }
}
pub extern "C" fn i64x2_neg(a: *mut [u8; 16]) {
    unsafe {
        let la = lanes_i64(&*a);
        let mut r = [0i64; 2];
        let mut i = 0;
        while i < 2 { r[i] = la[i].wrapping_neg(); i += 1; }
        *a = from_i64(r);
    }
}
pub extern "C" fn i64x2_abs(a: *mut [u8; 16]) {
    unsafe {
        let la = lanes_i64(&*a);
        let mut r = [0i64; 2];
        let mut i = 0;
        while i < 2 { r[i] = la[i].wrapping_abs(); i += 1; }
        *a = from_i64(r);
    }
}

// ===== Float unary (167-180) =====

pub extern "C" fn f32x4_neg(a: *mut [u8; 16]) {
    unsafe {
        let la = lanes_f32(&*a);
        let r = [(-la[0]), (-la[1]), (-la[2]), (-la[3])];
        *a = from_f32(r);
    }
}
pub extern "C" fn f32x4_abs(a: *mut [u8; 16]) {
    unsafe {
        let la = lanes_f32(&*a);
        let r = [abs_f32(la[0]), abs_f32(la[1]), abs_f32(la[2]), abs_f32(la[3])];
        *a = from_f32(r);
    }
}
pub extern "C" fn f32x4_sqrt(a: *mut [u8; 16]) {
    unsafe {
        let la = lanes_f32(&*a);
        let r = [sqrt_f32(la[0]), sqrt_f32(la[1]), sqrt_f32(la[2]), sqrt_f32(la[3])];
        *a = from_f32(r);
    }
}
pub extern "C" fn f32x4_ceil(a: *mut [u8; 16]) {
    unsafe {
        let la = lanes_f32(&*a);
        let r = [ceil_f32(la[0]), ceil_f32(la[1]), ceil_f32(la[2]), ceil_f32(la[3])];
        *a = from_f32(r);
    }
}
pub extern "C" fn f32x4_floor(a: *mut [u8; 16]) {
    unsafe {
        let la = lanes_f32(&*a);
        let r = [floor_f32(la[0]), floor_f32(la[1]), floor_f32(la[2]), floor_f32(la[3])];
        *a = from_f32(r);
    }
}
pub extern "C" fn f32x4_trunc(a: *mut [u8; 16]) {
    unsafe {
        let la = lanes_f32(&*a);
        let r = [trunc_f32(la[0]), trunc_f32(la[1]), trunc_f32(la[2]), trunc_f32(la[3])];
        *a = from_f32(r);
    }
}
pub extern "C" fn f32x4_nearest(a: *mut [u8; 16]) {
    unsafe {
        let la = lanes_f32(&*a);
        let r = [nearest_f32(la[0]), nearest_f32(la[1]), nearest_f32(la[2]), nearest_f32(la[3])];
        *a = from_f32(r);
    }
}

pub extern "C" fn f64x2_neg(a: *mut [u8; 16]) {
    unsafe {
        let la = lanes_f64(&*a);
        *a = from_f64([-la[0], -la[1]]);
    }
}
pub extern "C" fn f64x2_abs(a: *mut [u8; 16]) {
    unsafe {
        let la = lanes_f64(&*a);
        *a = from_f64([abs_f64(la[0]), abs_f64(la[1])]);
    }
}
pub extern "C" fn f64x2_sqrt(a: *mut [u8; 16]) {
    unsafe {
        let la = lanes_f64(&*a);
        *a = from_f64([sqrt_f64(la[0]), sqrt_f64(la[1])]);
    }
}
pub extern "C" fn f64x2_ceil(a: *mut [u8; 16]) {
    unsafe {
        let la = lanes_f64(&*a);
        *a = from_f64([ceil_f64(la[0]), ceil_f64(la[1])]);
    }
}
pub extern "C" fn f64x2_floor(a: *mut [u8; 16]) {
    unsafe {
        let la = lanes_f64(&*a);
        *a = from_f64([floor_f64(la[0]), floor_f64(la[1])]);
    }
}
pub extern "C" fn f64x2_trunc(a: *mut [u8; 16]) {
    unsafe {
        let la = lanes_f64(&*a);
        *a = from_f64([trunc_f64(la[0]), trunc_f64(la[1])]);
    }
}
pub extern "C" fn f64x2_nearest(a: *mut [u8; 16]) {
    unsafe {
        let la = lanes_f64(&*a);
        *a = from_f64([nearest_f64(la[0]), nearest_f64(la[1])]);
    }
}

// ===== Min/Max (182-193) =====

simd_binop_i8!(i8x16_min_s, |a: i8, b: i8| if a < b { a } else { b });
simd_binop_u8!(i8x16_min_u, |a: u8, b: u8| if a < b { a } else { b });
simd_binop_i8!(i8x16_max_s, |a: i8, b: i8| if a > b { a } else { b });
simd_binop_u8!(i8x16_max_u, |a: u8, b: u8| if a > b { a } else { b });
simd_binop_i16!(i16x8_min_s, |a: i16, b: i16| if a < b { a } else { b });
simd_binop_u16!(i16x8_min_u, |a: u16, b: u16| if a < b { a } else { b });
simd_binop_i16!(i16x8_max_s, |a: i16, b: i16| if a > b { a } else { b });
simd_binop_u16!(i16x8_max_u, |a: u16, b: u16| if a > b { a } else { b });
simd_binop_i32!(i32x4_min_s, |a: i32, b: i32| if a < b { a } else { b });
simd_binop_u32!(i32x4_min_u, |a: u32, b: u32| if a < b { a } else { b });
simd_binop_i32!(i32x4_max_s, |a: i32, b: i32| if a > b { a } else { b });
simd_binop_u32!(i32x4_max_u, |a: u32, b: u32| if a > b { a } else { b });

// ===== Average (194-195) =====

simd_binop_u8!(i8x16_avgr_u, |a: u8, b: u8| ((a as u16 + b as u16 + 1) / 2) as u8);
simd_binop_u16!(i16x8_avgr_u, |a: u16, b: u16| ((a as u32 + b as u32 + 1) / 2) as u16);

// ===== Saturating arithmetic (196-203) =====

simd_binop_i8!(i8x16_add_sat_s, |a: i8, b: i8| a.saturating_add(b));
simd_binop_u8!(i8x16_add_sat_u, |a: u8, b: u8| a.saturating_add(b));
simd_binop_i8!(i8x16_sub_sat_s, |a: i8, b: i8| a.saturating_sub(b));
simd_binop_u8!(i8x16_sub_sat_u, |a: u8, b: u8| a.saturating_sub(b));
simd_binop_i16!(i16x8_add_sat_s, |a: i16, b: i16| a.saturating_add(b));
simd_binop_u16!(i16x8_add_sat_u, |a: u16, b: u16| a.saturating_add(b));
simd_binop_i16!(i16x8_sub_sat_s, |a: i16, b: i16| a.saturating_sub(b));
simd_binop_u16!(i16x8_sub_sat_u, |a: u16, b: u16| a.saturating_sub(b));

// ===== Popcnt (204) =====

pub extern "C" fn i8x16_popcnt(a: *mut [u8; 16]) {
    unsafe {
        let mut i = 0;
        while i < 16 {
            (*a)[i] = (*a)[i].count_ones() as u8;
            i += 1;
        }
    }
}

// ===== Narrowing (212-215) =====

pub extern "C" fn i8x16_narrow_i16x8_s(a: *mut [u8; 16], b: *const [u8; 16]) {
    unsafe {
        let la = lanes_i16(&*a);
        let lb = lanes_i16(&*b);
        let mut r = [0i8; 16];
        let mut i = 0;
        while i < 8 {
            r[i] = la[i].clamp(i8::MIN as i16, i8::MAX as i16) as i8;
            r[i + 8] = lb[i].clamp(i8::MIN as i16, i8::MAX as i16) as i8;
            i += 1;
        }
        *a = from_i8(r);
    }
}

pub extern "C" fn i8x16_narrow_i16x8_u(a: *mut [u8; 16], b: *const [u8; 16]) {
    unsafe {
        let la = lanes_i16(&*a);
        let lb = lanes_i16(&*b);
        let mut r = [0u8; 16];
        let mut i = 0;
        while i < 8 {
            r[i] = la[i].clamp(0, u8::MAX as i16) as u8;
            r[i + 8] = lb[i].clamp(0, u8::MAX as i16) as u8;
            i += 1;
        }
        *a = r;
    }
}

pub extern "C" fn i16x8_narrow_i32x4_s(a: *mut [u8; 16], b: *const [u8; 16]) {
    unsafe {
        let la = lanes_i32(&*a);
        let lb = lanes_i32(&*b);
        let mut r = [0i16; 8];
        let mut i = 0;
        while i < 4 {
            r[i] = la[i].clamp(i16::MIN as i32, i16::MAX as i32) as i16;
            r[i + 4] = lb[i].clamp(i16::MIN as i32, i16::MAX as i32) as i16;
            i += 1;
        }
        *a = from_i16(r);
    }
}

pub extern "C" fn i16x8_narrow_i32x4_u(a: *mut [u8; 16], b: *const [u8; 16]) {
    unsafe {
        let la = lanes_i32(&*a);
        let lb = lanes_i32(&*b);
        let mut r = [0u16; 8];
        let mut i = 0;
        while i < 4 {
            r[i] = la[i].clamp(0, u16::MAX as i32) as u16;
            r[i + 4] = lb[i].clamp(0, u16::MAX as i32) as u16;
            i += 1;
        }
        *a = from_u16(r);
    }
}

// ===== Widening/extend (216-227) =====

pub extern "C" fn i16x8_extend_low_i8x16_s(a: *mut [u8; 16]) {
    unsafe {
        let la = lanes_i8(&*a);
        let mut r = [0i16; 8];
        let mut i = 0;
        while i < 8 { r[i] = la[i] as i16; i += 1; }
        *a = from_i16(r);
    }
}
pub extern "C" fn i16x8_extend_high_i8x16_s(a: *mut [u8; 16]) {
    unsafe {
        let la = lanes_i8(&*a);
        let mut r = [0i16; 8];
        let mut i = 0;
        while i < 8 { r[i] = la[i + 8] as i16; i += 1; }
        *a = from_i16(r);
    }
}
pub extern "C" fn i16x8_extend_low_i8x16_u(a: *mut [u8; 16]) {
    unsafe {
        let la = *a;
        let mut r = [0u16; 8];
        let mut i = 0;
        while i < 8 { r[i] = la[i] as u16; i += 1; }
        *a = from_u16(r);
    }
}
pub extern "C" fn i16x8_extend_high_i8x16_u(a: *mut [u8; 16]) {
    unsafe {
        let la = *a;
        let mut r = [0u16; 8];
        let mut i = 0;
        while i < 8 { r[i] = la[i + 8] as u16; i += 1; }
        *a = from_u16(r);
    }
}
pub extern "C" fn i32x4_extend_low_i16x8_s(a: *mut [u8; 16]) {
    unsafe {
        let la = lanes_i16(&*a);
        let mut r = [0i32; 4];
        let mut i = 0;
        while i < 4 { r[i] = la[i] as i32; i += 1; }
        *a = from_i32(r);
    }
}
pub extern "C" fn i32x4_extend_high_i16x8_s(a: *mut [u8; 16]) {
    unsafe {
        let la = lanes_i16(&*a);
        let mut r = [0i32; 4];
        let mut i = 0;
        while i < 4 { r[i] = la[i + 4] as i32; i += 1; }
        *a = from_i32(r);
    }
}
pub extern "C" fn i32x4_extend_low_i16x8_u(a: *mut [u8; 16]) {
    unsafe {
        let la = lanes_u16(&*a);
        let mut r = [0u32; 4];
        let mut i = 0;
        while i < 4 { r[i] = la[i] as u32; i += 1; }
        *a = from_u32(r);
    }
}
pub extern "C" fn i32x4_extend_high_i16x8_u(a: *mut [u8; 16]) {
    unsafe {
        let la = lanes_u16(&*a);
        let mut r = [0u32; 4];
        let mut i = 0;
        while i < 4 { r[i] = la[i + 4] as u32; i += 1; }
        *a = from_u32(r);
    }
}
pub extern "C" fn i64x2_extend_low_i32x4_s(a: *mut [u8; 16]) {
    unsafe {
        let la = lanes_i32(&*a);
        *a = from_i64([la[0] as i64, la[1] as i64]);
    }
}
pub extern "C" fn i64x2_extend_high_i32x4_s(a: *mut [u8; 16]) {
    unsafe {
        let la = lanes_i32(&*a);
        *a = from_i64([la[2] as i64, la[3] as i64]);
    }
}
pub extern "C" fn i64x2_extend_low_i32x4_u(a: *mut [u8; 16]) {
    unsafe {
        let la = lanes_u32(&*a);
        *a = from_u64([la[0] as u64, la[1] as u64]);
    }
}
pub extern "C" fn i64x2_extend_high_i32x4_u(a: *mut [u8; 16]) {
    unsafe {
        let la = lanes_u32(&*a);
        *a = from_u64([la[2] as u64, la[3] as u64]);
    }
}

// ===== Extmul (228-239) =====

pub extern "C" fn i16x8_extmul_low_i8x16_s(a: *mut [u8; 16], b: *const [u8; 16]) {
    unsafe {
        let la = lanes_i8(&*a);
        let lb = lanes_i8(&*b);
        let mut r = [0i16; 8];
        let mut i = 0;
        while i < 8 { r[i] = (la[i] as i16) * (lb[i] as i16); i += 1; }
        *a = from_i16(r);
    }
}
pub extern "C" fn i16x8_extmul_high_i8x16_s(a: *mut [u8; 16], b: *const [u8; 16]) {
    unsafe {
        let la = lanes_i8(&*a);
        let lb = lanes_i8(&*b);
        let mut r = [0i16; 8];
        let mut i = 0;
        while i < 8 { r[i] = (la[i + 8] as i16) * (lb[i + 8] as i16); i += 1; }
        *a = from_i16(r);
    }
}
pub extern "C" fn i16x8_extmul_low_i8x16_u(a: *mut [u8; 16], b: *const [u8; 16]) {
    unsafe {
        let la = *a;
        let lb = *b;
        let mut r = [0u16; 8];
        let mut i = 0;
        while i < 8 { r[i] = (la[i] as u16) * (lb[i] as u16); i += 1; }
        *a = from_u16(r);
    }
}
pub extern "C" fn i16x8_extmul_high_i8x16_u(a: *mut [u8; 16], b: *const [u8; 16]) {
    unsafe {
        let la = *a;
        let lb = *b;
        let mut r = [0u16; 8];
        let mut i = 0;
        while i < 8 { r[i] = (la[i + 8] as u16) * (lb[i + 8] as u16); i += 1; }
        *a = from_u16(r);
    }
}
pub extern "C" fn i32x4_extmul_low_i16x8_s(a: *mut [u8; 16], b: *const [u8; 16]) {
    unsafe {
        let la = lanes_i16(&*a);
        let lb = lanes_i16(&*b);
        let mut r = [0i32; 4];
        let mut i = 0;
        while i < 4 { r[i] = (la[i] as i32) * (lb[i] as i32); i += 1; }
        *a = from_i32(r);
    }
}
pub extern "C" fn i32x4_extmul_high_i16x8_s(a: *mut [u8; 16], b: *const [u8; 16]) {
    unsafe {
        let la = lanes_i16(&*a);
        let lb = lanes_i16(&*b);
        let mut r = [0i32; 4];
        let mut i = 0;
        while i < 4 { r[i] = (la[i + 4] as i32) * (lb[i + 4] as i32); i += 1; }
        *a = from_i32(r);
    }
}
pub extern "C" fn i32x4_extmul_low_i16x8_u(a: *mut [u8; 16], b: *const [u8; 16]) {
    unsafe {
        let la = lanes_u16(&*a);
        let lb = lanes_u16(&*b);
        let mut r = [0u32; 4];
        let mut i = 0;
        while i < 4 { r[i] = (la[i] as u32) * (lb[i] as u32); i += 1; }
        *a = from_u32(r);
    }
}
pub extern "C" fn i32x4_extmul_high_i16x8_u(a: *mut [u8; 16], b: *const [u8; 16]) {
    unsafe {
        let la = lanes_u16(&*a);
        let lb = lanes_u16(&*b);
        let mut r = [0u32; 4];
        let mut i = 0;
        while i < 4 { r[i] = (la[i + 4] as u32) * (lb[i + 4] as u32); i += 1; }
        *a = from_u32(r);
    }
}
pub extern "C" fn i64x2_extmul_low_i32x4_s(a: *mut [u8; 16], b: *const [u8; 16]) {
    unsafe {
        let la = lanes_i32(&*a);
        let lb = lanes_i32(&*b);
        *a = from_i64([(la[0] as i64) * (lb[0] as i64), (la[1] as i64) * (lb[1] as i64)]);
    }
}
pub extern "C" fn i64x2_extmul_high_i32x4_s(a: *mut [u8; 16], b: *const [u8; 16]) {
    unsafe {
        let la = lanes_i32(&*a);
        let lb = lanes_i32(&*b);
        *a = from_i64([(la[2] as i64) * (lb[2] as i64), (la[3] as i64) * (lb[3] as i64)]);
    }
}
pub extern "C" fn i64x2_extmul_low_i32x4_u(a: *mut [u8; 16], b: *const [u8; 16]) {
    unsafe {
        let la = lanes_u32(&*a);
        let lb = lanes_u32(&*b);
        *a = from_u64([(la[0] as u64) * (lb[0] as u64), (la[1] as u64) * (lb[1] as u64)]);
    }
}
pub extern "C" fn i64x2_extmul_high_i32x4_u(a: *mut [u8; 16], b: *const [u8; 16]) {
    unsafe {
        let la = lanes_u32(&*a);
        let lb = lanes_u32(&*b);
        *a = from_u64([(la[2] as u64) * (lb[2] as u64), (la[3] as u64) * (lb[3] as u64)]);
    }
}

// ===== Extadd pairwise (240-243) =====

pub extern "C" fn i16x8_extadd_pairwise_i8x16_s(a: *mut [u8; 16]) {
    unsafe {
        let la = lanes_i8(&*a);
        let mut r = [0i16; 8];
        let mut i = 0;
        while i < 8 { r[i] = (la[2 * i] as i16) + (la[2 * i + 1] as i16); i += 1; }
        *a = from_i16(r);
    }
}
pub extern "C" fn i16x8_extadd_pairwise_i8x16_u(a: *mut [u8; 16]) {
    unsafe {
        let la = *a;
        let mut r = [0u16; 8];
        let mut i = 0;
        while i < 8 { r[i] = (la[2 * i] as u16) + (la[2 * i + 1] as u16); i += 1; }
        *a = from_u16(r);
    }
}
pub extern "C" fn i32x4_extadd_pairwise_i16x8_s(a: *mut [u8; 16]) {
    unsafe {
        let la = lanes_i16(&*a);
        let mut r = [0i32; 4];
        let mut i = 0;
        while i < 4 { r[i] = (la[2 * i] as i32) + (la[2 * i + 1] as i32); i += 1; }
        *a = from_i32(r);
    }
}
pub extern "C" fn i32x4_extadd_pairwise_i16x8_u(a: *mut [u8; 16]) {
    unsafe {
        let la = lanes_u16(&*a);
        let mut r = [0u32; 4];
        let mut i = 0;
        while i < 4 { r[i] = (la[2 * i] as u32) + (la[2 * i + 1] as u32); i += 1; }
        *a = from_u32(r);
    }
}

// ===== Dot product (244) =====

pub extern "C" fn i32x4_dot_i16x8_s(a: *mut [u8; 16], b: *const [u8; 16]) {
    unsafe {
        let la = lanes_i16(&*a);
        let lb = lanes_i16(&*b);
        let mut r = [0i32; 4];
        let mut i = 0;
        while i < 4 {
            let lo = (la[2 * i] as i32) * (lb[2 * i] as i32);
            let hi = (la[2 * i + 1] as i32) * (lb[2 * i + 1] as i32);
            r[i] = lo.wrapping_add(hi);
            i += 1;
        }
        *a = from_i32(r);
    }
}

// ===== Q15mulrsat (245) =====

pub extern "C" fn i16x8_q15mulrsat_s(a: *mut [u8; 16], b: *const [u8; 16]) {
    unsafe {
        let la = lanes_i16(&*a);
        let lb = lanes_i16(&*b);
        let mut r = [0i16; 8];
        let mut i = 0;
        while i < 8 {
            let val = ((la[i] as i32 * lb[i] as i32) + 0x4000) >> 15;
            r[i] = val.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
            i += 1;
        }
        *a = from_i16(r);
    }
}

// ===== Conversions (246-253) =====

pub extern "C" fn i32x4_trunc_sat_f32x4_s(a: *mut [u8; 16]) {
    unsafe {
        let la = lanes_f32(&*a);
        let mut r = [0i32; 4];
        let mut i = 0;
        while i < 4 {
            let f = la[i];
            r[i] = if f.is_nan() { 0 }
                   else if f >= 2147483648.0 { i32::MAX }
                   else if f < -2147483648.0 { i32::MIN }
                   else { f as i32 };
            i += 1;
        }
        *a = from_i32(r);
    }
}
pub extern "C" fn i32x4_trunc_sat_f32x4_u(a: *mut [u8; 16]) {
    unsafe {
        let la = lanes_f32(&*a);
        let mut r = [0u32; 4];
        let mut i = 0;
        while i < 4 {
            let f = la[i];
            r[i] = if f.is_nan() { 0 }
                   else if f >= 4294967296.0 { u32::MAX }
                   else if f <= -1.0 { 0 }
                   else { f as u32 };
            i += 1;
        }
        *a = from_u32(r);
    }
}
pub extern "C" fn f32x4_convert_i32x4_s(a: *mut [u8; 16]) {
    unsafe {
        let la = lanes_i32(&*a);
        *a = from_f32([la[0] as f32, la[1] as f32, la[2] as f32, la[3] as f32]);
    }
}
pub extern "C" fn f32x4_convert_i32x4_u(a: *mut [u8; 16]) {
    unsafe {
        let la = lanes_u32(&*a);
        *a = from_f32([la[0] as f32, la[1] as f32, la[2] as f32, la[3] as f32]);
    }
}
pub extern "C" fn i32x4_trunc_sat_f64x2_s_zero(a: *mut [u8; 16]) {
    unsafe {
        let la = lanes_f64(&*a);
        let mut r = [0i32; 4];
        let mut i = 0;
        while i < 2 {
            let f = la[i];
            r[i] = if f.is_nan() { 0 }
                   else if f >= 2147483648.0 { i32::MAX }
                   else if f < -2147483648.0 { i32::MIN }
                   else { f as i32 };
            i += 1;
        }
        *a = from_i32(r);
    }
}
pub extern "C" fn i32x4_trunc_sat_f64x2_u_zero(a: *mut [u8; 16]) {
    unsafe {
        let la = lanes_f64(&*a);
        let mut r = [0u32; 4];
        let mut i = 0;
        while i < 2 {
            let f = la[i];
            r[i] = if f.is_nan() { 0 }
                   else if f >= 4294967296.0 { u32::MAX }
                   else if f <= -1.0 { 0 }
                   else { f as u32 };
            i += 1;
        }
        *a = from_u32(r);
    }
}
pub extern "C" fn f64x2_convert_low_i32x4_s(a: *mut [u8; 16]) {
    unsafe {
        let la = lanes_i32(&*a);
        *a = from_f64([la[0] as f64, la[1] as f64]);
    }
}
pub extern "C" fn f64x2_convert_low_i32x4_u(a: *mut [u8; 16]) {
    unsafe {
        let la = lanes_u32(&*a);
        *a = from_f64([la[0] as f64, la[1] as f64]);
    }
}

// ===== Load/Store lane (69, 70) =====
// In ring3, memory is directly accessible at ctx.memory_base

pub extern "C" fn v128_load_lane(
    ctx: &Ring3Context, addr: u32, offset: u32, lane_idx: u8, size: u32, val: *mut [u8; 16],
) {
    unsafe {
        let mem = ctx.memory_base;
        let idx = addr as usize + offset as usize;
        let src = mem.add(idx);
        let v = &mut *val;
        match size {
            1 => v[lane_idx as usize] = *src,
            2 => {
                let start = lane_idx as usize * 2;
                v[start] = *src;
                v[start + 1] = *src.add(1);
            }
            4 => {
                let start = lane_idx as usize * 4;
                let mut j = 0;
                while j < 4 { v[start + j] = *src.add(j); j += 1; }
            }
            8 => {
                let start = lane_idx as usize * 8;
                let mut j = 0;
                while j < 8 { v[start + j] = *src.add(j); j += 1; }
            }
            _ => {}
        }
    }
}

pub extern "C" fn v128_store_lane(
    ctx: &Ring3Context, addr: u32, offset: u32, lane_idx: u8, size: u32, val: *const [u8; 16],
) {
    unsafe {
        let mem = ctx.memory_base;
        let idx = addr as usize + offset as usize;
        let dst = mem.add(idx);
        let v = &*val;
        match size {
            1 => *dst = v[lane_idx as usize],
            2 => {
                let start = lane_idx as usize * 2;
                *dst = v[start];
                *dst.add(1) = v[start + 1];
            }
            4 => {
                let start = lane_idx as usize * 4;
                let mut j = 0;
                while j < 4 { *dst.add(j) = v[start + j]; j += 1; }
            }
            8 => {
                let start = lane_idx as usize * 8;
                let mut j = 0;
                while j < 8 { *dst.add(j) = v[start + j]; j += 1; }
            }
            _ => {}
        }
    }
}

// ===== Load extend (255-260) =====
// Signature: extern "C" fn(dst: *mut [u8; 16], src: u64)
// src contains the raw 8 bytes loaded from memory

pub extern "C" fn v128_load8x8_s(dst: *mut [u8; 16], src: u64) {
    let bytes = src.to_le_bytes();
    let mut r = [0i16; 8];
    let mut i = 0;
    while i < 8 { r[i] = (bytes[i] as i8) as i16; i += 1; }
    unsafe { *dst = from_i16(r); }
}

pub extern "C" fn v128_load8x8_u(dst: *mut [u8; 16], src: u64) {
    let bytes = src.to_le_bytes();
    let mut r = [0u16; 8];
    let mut i = 0;
    while i < 8 { r[i] = bytes[i] as u16; i += 1; }
    unsafe { *dst = from_u16(r); }
}

pub extern "C" fn v128_load16x4_s(dst: *mut [u8; 16], src: u64) {
    let bytes = src.to_le_bytes();
    let mut r = [0i32; 4];
    let mut i = 0;
    while i < 4 {
        let v = i16::from_le_bytes([bytes[i * 2], bytes[i * 2 + 1]]);
        r[i] = v as i32;
        i += 1;
    }
    unsafe { *dst = from_i32(r); }
}

pub extern "C" fn v128_load16x4_u(dst: *mut [u8; 16], src: u64) {
    let bytes = src.to_le_bytes();
    let mut r = [0u32; 4];
    let mut i = 0;
    while i < 4 {
        let v = u16::from_le_bytes([bytes[i * 2], bytes[i * 2 + 1]]);
        r[i] = v as u32;
        i += 1;
    }
    unsafe { *dst = from_u32(r); }
}

pub extern "C" fn v128_load32x2_s(dst: *mut [u8; 16], src: u64) {
    let bytes = src.to_le_bytes();
    let v0 = i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let v1 = i32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    unsafe { *dst = from_i64([v0 as i64, v1 as i64]); }
}

pub extern "C" fn v128_load32x2_u(dst: *mut [u8; 16], src: u64) {
    let bytes = src.to_le_bytes();
    let v0 = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let v1 = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    unsafe { *dst = from_u64([v0 as u64, v1 as u64]); }
}
