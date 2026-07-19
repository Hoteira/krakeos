//! Minimal no_std float helpers for common/value.rs (const-expr evaluation
//! and the wacc global encoding — not the hot path). Backed by libm.

pub trait FloatMath {
    fn abs(self) -> Self;
    fn floor(self) -> Self;
    fn ceil(self) -> Self;
    fn round(self) -> Self;
    fn trunc(self) -> Self;
    fn sqrt(self) -> Self;
}

impl FloatMath for f32 {
    fn abs(self) -> Self { libm::fabsf(self) }
    fn floor(self) -> Self { libm::floorf(self) }
    fn ceil(self) -> Self { libm::ceilf(self) }
    fn round(self) -> Self { libm::roundf(self) }
    fn trunc(self) -> Self { libm::truncf(self) }
    fn sqrt(self) -> Self { libm::sqrtf(self) }
}

impl FloatMath for f64 {
    fn abs(self) -> Self { libm::fabs(self) }
    fn floor(self) -> Self { libm::floor(self) }
    fn ceil(self) -> Self { libm::ceil(self) }
    fn round(self) -> Self { libm::round(self) }
    fn trunc(self) -> Self { libm::trunc(self) }
    fn sqrt(self) -> Self { libm::sqrt(self) }
}
