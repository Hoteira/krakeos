pub trait FloatMath {
    fn abs(self) -> Self;
    fn floor(self) -> Self;
    fn ceil(self) -> Self;
    fn round(self) -> Self;
    fn trunc(self) -> Self;
    fn sqrt(self) -> Self;
    fn powf(self, exp: Self) -> Self;
    fn powi(self, n: i32) -> Self;
    fn sin(self) -> Self;
    fn cos(self) -> Self;
    fn tan(self) -> Self;
    fn atan(self) -> Self;
    fn atan2(self, other: Self) -> Self;
    fn to_radians(self) -> Self;
    fn to_degrees(self) -> Self;
    fn min(self, other: Self) -> Self;
    fn max(self, other: Self) -> Self;
}

const PI: f64 = 3.14159265358979323846;

impl FloatMath for f64 {
    fn abs(self) -> Self {
        unsafe { core::intrinsics::fabsf64(self) }
    }

    fn floor(self) -> Self {
        unsafe { core::intrinsics::floorf64(self) }
    }

    fn ceil(self) -> Self {
        unsafe { core::intrinsics::ceilf64(self) }
    }

    fn round(self) -> Self {
        unsafe { core::intrinsics::roundf64(self) }
    }

    fn trunc(self) -> Self {
        unsafe { core::intrinsics::truncf64(self) }
    }

    fn sqrt(self) -> Self {
        unsafe { core::intrinsics::sqrtf64(self) }
    }

    fn powf(self, exp: Self) -> Self {
        if self < 0.0 { return f64::NAN; }
        if self == 0.0 { return 0.0; }
        exp_approx(ln_approx(self) * exp)
    }

    fn powi(self, mut n: i32) -> Self {
        if n == 0 { return 1.0; }
        let mut x = self;
        if n < 0 {
            x = 1.0 / x;
            n = n.saturating_neg();
        }
        let mut res = 1.0;
        let mut n_unsigned = n as u32;
        while n_unsigned > 0 {
            if n_unsigned & 1 != 0 { res *= x; }
            x *= x;
            n_unsigned >>= 1;
        }
        res
    }

    fn sin(self) -> Self {
        sin_approx(self)
    }

    fn cos(self) -> Self {
        cos_approx(self)
    }

    fn tan(self) -> Self {
        self.sin() / self.cos()
    }

    fn atan(self) -> Self {
        atan_approx(self)
    }

    fn atan2(self, x: Self) -> Self {
        let y = self;
        if x > 0.0 {
            atan_approx(y / x)
        } else if x < 0.0 {
            if y >= 0.0 {
                atan_approx(y / x) + PI
            } else {
                atan_approx(y / x) - PI
            }
        } else {
            if y > 0.0 {
                PI / 2.0
            } else if y < 0.0 {
                -PI / 2.0
            } else {
                0.0
            }
        }
    }

    fn to_radians(self) -> Self {
        self * (PI / 180.0)
    }

    fn to_degrees(self) -> Self {
        self * (180.0 / PI)
    }

    fn min(self, other: Self) -> Self {
        if self.is_nan() { return self; }
        if other.is_nan() { return other; }
        if self < other { return self; }
        if other < self { return other; }
        Self::from_bits(self.to_bits() | other.to_bits())
    }

    fn max(self, other: Self) -> Self {
        if self.is_nan() { return self; }
        if other.is_nan() { return other; }
        if self > other { return self; }
        if other > self { return other; }
        Self::from_bits(self.to_bits() & other.to_bits())
    }
}

impl FloatMath for f32 {
    fn abs(self) -> Self {
        unsafe { core::intrinsics::fabsf32(self) }
    }

    fn floor(self) -> Self {
        unsafe { core::intrinsics::floorf32(self) }
    }

    fn ceil(self) -> Self {
        unsafe { core::intrinsics::ceilf32(self) }
    }

    fn round(self) -> Self {
        unsafe { core::intrinsics::roundf32(self) }
    }

    fn trunc(self) -> Self {
        unsafe { core::intrinsics::truncf32(self) }
    }

    fn sqrt(self) -> Self {
        unsafe { core::intrinsics::sqrtf32(self) }
    }

    fn powf(self, exp: Self) -> Self {
        (self as f64).powf(exp as f64) as f32
    }

    fn powi(self, n: i32) -> Self {
        (self as f64).powi(n) as f32
    }

    fn sin(self) -> Self {
        (self as f64).sin() as f32
    }

    fn cos(self) -> Self {
        (self as f64).cos() as f32
    }

    fn tan(self) -> Self {
        (self as f64).tan() as f32
    }

    fn atan(self) -> Self {
        (self as f64).atan() as f32
    }

    fn atan2(self, other: Self) -> Self {
        (self as f64).atan2(other as f64) as f32
    }

    fn to_radians(self) -> Self {
        self * (PI as f32 / 180.0)
    }

    fn to_degrees(self) -> Self {
        self * (180.0 / PI as f32)
    }

    fn min(self, other: Self) -> Self {
        if self.is_nan() { return self; }
        if other.is_nan() { return other; }
        if self < other { return self; }
        if other < self { return other; }
        Self::from_bits(self.to_bits() | other.to_bits())
    }

    fn max(self, other: Self) -> Self {
        if self.is_nan() { return self; }
        if other.is_nan() { return other; }
        if self > other { return self; }
        if other > self { return other; }
        Self::from_bits(self.to_bits() & other.to_bits())
    }
}

fn sin_approx(mut x: f64) -> f64 {
    while x > PI { x -= 2.0 * PI; }
    while x < -PI { x += 2.0 * PI; }
    let x2 = x * x;
    let x3 = x * x2;
    let x5 = x3 * x2;
    let x7 = x5 * x2;
    let x9 = x7 * x2;
    x - (x3 / 6.0) + (x5 / 120.0) - (x7 / 5040.0) + (x9 / 362880.0)
}

fn cos_approx(mut x: f64) -> f64 {
    while x > PI { x -= 2.0 * PI; }
    while x < -PI { x += 2.0 * PI; }
    let x2 = x * x;
    let x4 = x2 * x2;
    let x6 = x4 * x2;
    let x8 = x6 * x2;
    1.0 - (x2 / 2.0) + (x4 / 24.0) - (x6 / 720.0) + (x8 / 40320.0)
}

fn ln_approx(x: f64) -> f64 {
    if x <= 0.0 { return f64::NAN; }
    let mut val = x;
    let mut k = 0;
    while val > 1.5 {
        val /= 2.718281828459;
        k += 1;
    }
    while val < 0.5 {
        val *= 2.718281828459;
        k -= 1;
    }
    let y = (val - 1.0) / (val + 1.0);
    let y2 = y * y;
    let mut sum = y;
    let mut term = y;
    for i in 1..6 {
        term *= y2;
        sum += term / (2.0 * i as f64 + 1.0);
    }
    2.0 * sum + (k as f64)
}

fn exp_approx(x: f64) -> f64 {
    let mut sum = 1.0;
    let mut term = 1.0;
    for i in 1..12 {
        term *= x / i as f64;
        sum += term;
    }
    sum
}

fn atan_approx(x: f64) -> f64 {
    if x < 0.0 { return -atan_approx(-x); }
    if x > 1.0 { return PI / 2.0 - atan_approx(1.0 / x); }
    let x2 = x * x;
    let x3 = x * x2;
    let x5 = x3 * x2;
    let x7 = x5 * x2;
    let x9 = x7 * x2;
    let x11 = x9 * x2;
    x - (x3 / 3.0) + (x5 / 5.0) - (x7 / 7.0) + (x9 / 9.0) - (x11 / 11.0)
}