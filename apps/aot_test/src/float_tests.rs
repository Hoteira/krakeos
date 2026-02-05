use std::println;
use std::math::FloatMath;

pub fn test_float_precision_rounding() -> i32 {
    println!("==================== Testing Floating Point Precision & Rounding...");
    let mut errors = 0;

    errors += test_rounding_modes();
    errors += test_min_max_edge_cases();
    errors += test_trunc_sat();
    errors += test_nan_inf_logic();

    if errors == 0 { println!("==================== Floating Point: OK"); }
    errors
}

fn test_rounding_modes() -> i32 {
    let mut errors = 0;

    // Test ties-to-even (nearest)
    // 0.5 -> 0, 1.5 -> 2, 2.5 -> 2, 3.5 -> 4
    if 0.5f32.round_ties_even() != 0.0 { errors += 1; println!("Error: f32.nearest(0.5) != 0.0"); }
    if 1.5f32.round_ties_even() != 2.0 { errors += 1; println!("Error: f32.nearest(1.5) != 2.0"); }
    if 2.5f32.round_ties_even() != 2.0 { errors += 1; println!("Error: f32.nearest(2.5) != 2.0"); }
    
    if 0.5f64.round_ties_even() != 0.0 { errors += 1; println!("Error: f64.nearest(0.5) != 0.0"); }
    if 1.5f64.round_ties_even() != 2.0 { errors += 1; println!("Error: f64.nearest(1.5) != 2.0"); }

    // Ceil/Floor/Trunc
    if 1.1f32.ceil() != 2.0 { errors += 1; println!("Error: f32.ceil(1.1)"); }
    if 1.9f32.floor() != 1.0 { errors += 1; println!("Error: f32.floor(1.9)"); }
    if (-1.9f32).trunc() != -1.0 { errors += 1; println!("Error: f32.trunc(-1.9)"); }

    errors
}

fn test_min_max_edge_cases() -> i32 {
    let mut errors = 0;

    // WASM min/max behavior with -0.0 and 0.0
    // min(-0.0, 0.0) -> -0.0
    // max(-0.0, 0.0) -> 0.0
    let pz = 0.0f32;
    let nz = -0.0f32;
    
    if wasm_f32_min(pz, nz).to_bits() != nz.to_bits() { errors += 1; println!("Error: f32.min(0.0, -0.0)"); }
    if wasm_f32_max(pz, nz).to_bits() != pz.to_bits() { errors += 1; println!("Error: f32.max(0.0, -0.0)"); }

    // NaN handling in min/max (if one is NaN, result is NaN)
    if !wasm_f32_min(pz, f32::NAN).is_nan() { errors += 1; println!("Error: f32.min(0.0, NaN)"); }
    if !wasm_f32_min(f32::NAN, pz).is_nan() { errors += 1; println!("Error: f32.min(NaN, 0.0)"); }

    errors
}

#[inline(never)]
fn wasm_f32_min(a: f32, b: f32) -> f32 {
    let bits_a = a.to_bits();
    let bits_b = b.to_bits();
    let is_nan_a = (bits_a & 0x7F800000) == 0x7F800000 && (bits_a & 0x007FFFFF) != 0;
    let is_nan_b = (bits_b & 0x7F800000) == 0x7F800000 && (bits_b & 0x007FFFFF) != 0;
    if is_nan_a { return a; }
    if is_nan_b { return b; }
    if a < b { return a; }
    if b < a { return b; }
    f32::from_bits(bits_a | bits_b)
}

#[inline(never)]
fn wasm_f32_max(a: f32, b: f32) -> f32 {
    let bits_a = a.to_bits();
    let bits_b = b.to_bits();
    let is_nan_a = (bits_a & 0x7F800000) == 0x7F800000 && (bits_a & 0x007FFFFF) != 0;
    let is_nan_b = (bits_b & 0x7F800000) == 0x7F800000 && (bits_b & 0x007FFFFF) != 0;
    if is_nan_a { return a; }
    if is_nan_b { return b; }
    if a > b { return a; }
    if b > a { return b; }
    f32::from_bits(bits_a & bits_b)
}

fn test_trunc_sat() -> i32 {
    let mut errors = 0;

    // These test the non-trapping truncation (saturating)
    // f32 -> i32.s
    let large = 1e15f32;
    if large.trunc_sat_i32_s() != i32::MAX { errors += 1; println!("Error: f32.trunc_sat_i32_s(large)"); }
    
    let neg_large = -1e15f32;
    if neg_large.trunc_sat_i32_s() != i32::MIN { errors += 1; println!("Error: f32.trunc_sat_i32_s(neg_large)"); }

    // f64 -> u64
    let large_f64 = 1e25f64;
    if large_f64.trunc_sat_u64() != u64::MAX { errors += 1; println!("Error: f64.trunc_sat_u64(large)"); }

    errors
}

fn test_nan_inf_logic() -> i32 {
    let mut errors = 0;

    let inf = f32::INFINITY;
    let neg_inf = f32::NEG_INFINITY;
    let nan = f32::NAN;

    if (inf + neg_inf).is_nan() == false { errors += 1; println!("Error: inf + -inf should be NaN"); }
    if (inf / inf).is_nan() == false { errors += 1; println!("Error: inf / inf should be NaN"); }
    
    // Comparisons
    if (nan == nan) != false { errors += 1; println!("Error: NaN == NaN should be false"); }
    if (nan != nan) != true { errors += 1; println!("Error: NaN != NaN should be true"); }
    
    // Inf Comparisons
    if (inf > 1e38) == false { errors += 1; println!("Error: inf > 1e38"); }
    if (neg_inf < -1e38) == false { errors += 1; println!("Error: -inf < -1e38"); }

    errors
}

trait WASMFit {
    fn round_ties_even(self) -> Self;
    fn trunc_sat_i32_s(self) -> i32;
    fn trunc_sat_u64(self) -> u64;
}

impl WASMFit for f32 {
    fn round_ties_even(self) -> f32 {
        let f = self;
        let r = f.round();
        if (f - r).abs() == 0.5 {
            if r % 2.0 != 0.0 {
                if r > 0.0 { r - 1.0 } else { r + 1.0 }
            } else {
                r
            }
        } else {
            r
        }
    }
    
    fn trunc_sat_i32_s(self) -> i32 {
        if self.is_nan() { return 0; }
        if self <= i32::MIN as f32 { return i32::MIN; }
        if self >= i32::MAX as f32 { return i32::MAX; }
        self as i32
    }
    
    fn trunc_sat_u64(self) -> u64 { 0 } // Not used
}

impl WASMFit for f64 {
    fn round_ties_even(self) -> f64 {
        let f = self;
        let r = f.round();
        if (f - r).abs() == 0.5 {
            if r % 2.0 != 0.0 {
                if r > 0.0 { r - 1.0 } else { r + 1.0 }
            } else {
                r
            }
        } else {
            r
        }
    }
    
    fn trunc_sat_i32_s(self) -> i32 { 0 } // Not used
    
    fn trunc_sat_u64(self) -> u64 {
        if self.is_nan() { return 0; }
        if self <= 0.0 { return 0; }
        if self >= u64::MAX as f64 { return u64::MAX; }
        self as u64
    }
}
