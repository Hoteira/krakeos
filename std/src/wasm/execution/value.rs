use core::fmt::{Debug, Display};
use core::ops::{Add, Div, Mul, Sub};
use core::{f32, f64};

use crate::math::FloatMath;
use crate::wasm::core::reader::types::{NumType, ValType};
use crate::wasm::execution::store::addrs::FuncAddr;
use crate::wasm::RefType;

#[derive(Clone, Debug, Copy, PartialOrd)]
#[repr(transparent)]
pub struct F32(pub f32);

impl Display for F32 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl PartialEq for F32 {
    fn eq(&self, other: &Self) -> bool {
        self.0.eq(&other.0)
    }
}

impl Add for F32 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl Sub for F32 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}

impl Mul for F32 {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self::Output {
        Self(self.0 * rhs.0)
    }
}

impl Div for F32 {
    type Output = Self;
    fn div(self, rhs: Self) -> Self::Output {
        Self(self.0 / rhs.0)
    }
}

impl F32 {
    pub fn abs(&self) -> Self {
        Self(self.0.abs())
    }
    pub fn neg(&self) -> Self {
        Self(-self.0)
    }
    pub fn ceil(&self) -> Self {
        if self.0.is_nan() {
            return Self(f32::NAN);
        }
        Self(self.0.ceil())
    }
    pub fn floor(&self) -> Self {
        if self.0.is_nan() {
            return Self(f32::NAN);
        }
        Self(self.0.floor())
    }
    pub fn trunc(&self) -> Self {
        if self.0.is_nan() {
            return Self(f32::NAN);
        }
        Self(self.0.trunc())
    }
    pub fn nearest(&self) -> Self {
        if self.0.is_nan() {
            return Self(f32::NAN);
        }
        let val = self.0;
        // If large enough, it's already an integer.
        if val.abs() >= (1u32 << 23) as f32 {
            return Self(val);
        }

        let floor = val.floor();
        let ceil = val.ceil();
        let diff_floor = (val - floor).abs();
        let diff_ceil = (ceil - val).abs();

        if diff_floor < diff_ceil {
            Self(floor)
        } else if diff_ceil < diff_floor {
            Self(ceil)
        } else {
            // Tie. Round to even.
            if floor % 2.0 == 0.0 {
                Self(floor)
            } else {
                Self(ceil)
            }
        }
    }
    pub fn round(&self) -> Self {
        Self(self.0.round())
    }
    pub fn sqrt(&self) -> Self {
        Self(self.0.sqrt())
    }

    pub fn min(&self, rhs: Self) -> Self {
        Self(if self.0.is_nan() || rhs.0.is_nan() {
            f32::NAN
        } else if self.0 == 0.0 && rhs.0 == 0.0 {
            if self.to_bits() >> 31 == 1 {
                self.0
            } else {
                rhs.0
            }
        } else {
            self.0.min(rhs.0)
        })
    }
    pub fn max(&self, rhs: Self) -> Self {
        Self(if self.0.is_nan() || rhs.0.is_nan() {
            f32::NAN
        } else if self.0 == 0.0 && rhs.0 == 0.0 {
            if self.to_bits() >> 31 == 1 {
                rhs.0
            } else {
                self.0
            }
        } else {
            self.0.max(rhs.0)
        })
    }
    pub fn copysign(&self, rhs: Self) -> Self {
        Self(self.0.copysign(rhs.0))
    }
    pub fn from_bits(other: u32) -> Self {
        Self(f32::from_bits(other))
    }
    pub fn is_nan(&self) -> bool {
        self.0.is_nan()
    }
    pub fn is_infinity(&self) -> bool {
        self.0.is_infinite()
    }
    pub fn is_negative_infinity(&self) -> bool {
        self.0.is_infinite() && self.0 < 0.0
    }

    pub fn as_i32(&self) -> i32 {
        self.0 as i32
    }
    pub fn as_u32(&self) -> u32 {
        self.0 as u32
    }
    pub fn as_i64(&self) -> i64 {
        self.0 as i64
    }
    pub fn as_u64(&self) -> u64 {
        self.0 as u64
    }
    pub fn as_f64(&self) -> F64 {
        F64(self.0 as f64)
    }
    pub fn reinterpret_as_i32(&self) -> i32 {
        self.0.to_bits() as i32
    }
    pub fn to_bits(&self) -> u32 {
        self.0.to_bits()
    }
}

#[derive(Clone, Debug, Copy, PartialOrd)]
#[repr(transparent)]
pub struct F64(pub f64);

impl Display for F64 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl PartialEq for F64 {
    fn eq(&self, other: &Self) -> bool {
        self.0.eq(&other.0)
    }
}

impl Add for F64 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl Sub for F64 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}

impl Mul for F64 {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self::Output {
        Self(self.0 * rhs.0)
    }
}

impl Div for F64 {
    type Output = Self;
    fn div(self, rhs: Self) -> Self::Output {
        Self(self.0 / rhs.0)
    }
}

impl F64 {
    pub fn abs(&self) -> Self {
        Self(self.0.abs())
    }
    pub fn neg(&self) -> Self {
        Self(-self.0)
    }
    pub fn ceil(&self) -> Self {
        if self.0.is_nan() {
            return Self(f64::NAN);
        }
        Self(self.0.ceil())
    }
    pub fn floor(&self) -> Self {
        if self.0.is_nan() {
            return Self(f64::NAN);
        }
        Self(self.0.floor())
    }
    pub fn trunc(&self) -> Self {
        if self.0.is_nan() {
            return Self(f64::NAN);
        }
        Self(self.0.trunc())
    }
    pub fn nearest(&self) -> Self {
        if self.0.is_nan() {
            return Self(f64::NAN);
        }
        let val = self.0;
        // If large enough, it's already an integer.
        if val.abs() >= (1u64 << 52) as f64 {
            return Self(val);
        }

        let floor = val.floor();
        let ceil = val.ceil();
        let diff_floor = (val - floor).abs();
        let diff_ceil = (ceil - val).abs();

        if diff_floor < diff_ceil {
            Self(floor)
        } else if diff_ceil < diff_floor {
            Self(ceil)
        } else {
            // Tie. Round to even.
            if floor % 2.0 == 0.0 {
                Self(floor)
            } else {
                Self(ceil)
            }
        }
    }
    pub fn round(&self) -> Self {
        Self(self.0.round())
    }
    pub fn sqrt(&self) -> Self {
        Self(self.0.sqrt())
    }

    pub fn min(&self, rhs: Self) -> Self {
        Self(if self.0.is_nan() || rhs.0.is_nan() {
            f64::NAN
        } else if self.0 == 0.0 && rhs.0 == 0.0 {
            if self.to_bits() >> 63 == 1 {
                self.0
            } else {
                rhs.0
            }
        } else {
            self.0.min(rhs.0)
        })
    }
    pub fn max(&self, rhs: Self) -> Self {
        Self(if self.0.is_nan() || rhs.0.is_nan() {
            f64::NAN
        } else if self.0 == 0.0 && rhs.0 == 0.0 {
            if self.to_bits() >> 63 == 1 {
                rhs.0
            } else {
                self.0
            }
        } else {
            self.0.max(rhs.0)
        })
    }
    pub fn copysign(&self, rhs: Self) -> Self {
        Self(self.0.copysign(rhs.0))
    }

    pub fn from_bits(other: u64) -> Self {
        Self(f64::from_bits(other))
    }
    pub fn is_nan(&self) -> bool {
        self.0.is_nan()
    }
    pub fn is_infinity(&self) -> bool {
        self.0.is_infinite()
    }
    pub fn is_negative_infinity(&self) -> bool {
        self.0.is_infinite() && self.0 < 0.0
    }

    pub fn as_i32(&self) -> i32 {
        self.0 as i32
    }
    pub fn as_u32(&self) -> u32 {
        self.0 as u32
    }
    pub fn as_i64(&self) -> i64 {
        self.0 as i64
    }
    pub fn as_u64(&self) -> u64 {
        self.0 as u64
    }
    pub fn as_f32(&self) -> F32 {
        F32(self.0 as f32)
    }
    pub fn reinterpret_as_i64(&self) -> i64 {
        self.0.to_bits() as i64
    }
    pub fn to_bits(&self) -> u64 {
        self.0.to_bits()
    }
}

/// A value at runtime. This is essentially a duplicate of [ValType] just with additional values.
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#values>
// TODO implement missing variants
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    I32(u32),
    I64(u64),
    F32(F32),
    F64(F64),
    V128([u8; 16]),
    Ref(Ref),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ref {
    Null(RefType),
    Func(FuncAddr),
    Extern(ExternAddr),
}

impl Display for Ref {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Ref::Func(func_addr) => write!(f, "FuncRef({func_addr:?})"),
            Ref::Extern(extern_addr) => write!(f, "ExternRef({extern_addr:?})"),
            Ref::Null(ty) => write!(f, "Null({ty:?})"),
        }
    }
}

impl Ref {
    pub fn ty(self) -> RefType {
        match self {
            Ref::Null(ref_type) => ref_type,
            Ref::Func(_) => RefType::FuncRef,
            Ref::Extern(_) => RefType::ExternRef,
        }
    }
}

/// The WebAssembly specification defines an externaddr as an address to an
/// "external" type, i.e. is a type which is managed by the embedder. For this
/// interpreter the task of managing external objects and relating them to
/// addresses is handed off to the user, which means that an [`ExternAddr`] can
/// simply be seen as an integer that is opaque to Wasm code without any meaning
/// assigned to it.
///
/// See: WebAssembly Specification 2.0 - 2.3.3, 4.2.1
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExternAddr(pub usize);

impl Value {
    pub fn default_from_ty(ty: ValType) -> Self {
        match ty {
            ValType::NumType(NumType::I32) => Self::I32(0),
            ValType::NumType(NumType::I64) => Self::I64(0),
            ValType::NumType(NumType::F32) => Self::F32(F32(0.0)),
            ValType::NumType(NumType::F64) => Self::F64(F64(0.0_f64)),
            ValType::RefType(ref_type) => Self::Ref(Ref::Null(ref_type)),
            ValType::VecType => Self::V128([0; 16]),
        }
    }

    pub fn to_ty(&self) -> ValType {
        match self {
            Value::I32(_) => ValType::NumType(NumType::I32),
            Value::I64(_) => ValType::NumType(NumType::I64),
            Value::F32(_) => ValType::NumType(NumType::F32),
            Value::F64(_) => ValType::NumType(NumType::F64),
            Value::Ref(Ref::Null(ref_type)) => ValType::RefType(*ref_type),
            Value::Ref(Ref::Func(_)) => ValType::RefType(RefType::FuncRef),
            Value::Ref(Ref::Extern(_)) => ValType::RefType(RefType::ExternRef),
            Value::V128(_) => ValType::VecType,
        }
    }
}

/// An error used in all [`TryFrom<Value>`] implementations for Rust types ([`i32`], [`F32`], [`Ref`], ...)
#[derive(Debug, PartialEq, Eq)]
pub struct ValueTypeMismatchError;

impl Display for ValueTypeMismatchError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("failed to convert Value to a Rust value because the types did not match")
    }
}

impl From<u32> for Value {
    fn from(x: u32) -> Self {
        Value::I32(x)
    }
}
impl TryFrom<Value> for u32 {
    type Error = ValueTypeMismatchError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::I32(x) => Ok(x),
            _ => Err(ValueTypeMismatchError),
        }
    }
}

impl From<i32> for Value {
    fn from(x: i32) -> Self {
        Value::I32(x as u32)
    }
}
impl TryFrom<Value> for i32 {
    type Error = ValueTypeMismatchError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::I32(x) => Ok(x as i32),
            _ => Err(ValueTypeMismatchError),
        }
    }
}

impl From<u64> for Value {
    fn from(x: u64) -> Self {
        Value::I64(x)
    }
}
impl TryFrom<Value> for u64 {
    type Error = ValueTypeMismatchError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::I64(x) => Ok(x),
            _ => Err(ValueTypeMismatchError),
        }
    }
}
impl From<i64> for Value {
    fn from(x: i64) -> Self {
        Value::I64(x as u64)
    }
}
impl TryFrom<Value> for i64 {
    type Error = ValueTypeMismatchError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::I64(x) => Ok(x as i64),
            _ => Err(ValueTypeMismatchError),
        }
    }
}

impl From<F32> for Value {
    fn from(x: F32) -> Self {
        Value::F32(x)
    }
}
impl TryFrom<Value> for F32 {
    type Error = ValueTypeMismatchError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::F32(x) => Ok(x),
            _ => Err(ValueTypeMismatchError),
        }
    }
}

impl From<F64> for Value {
    fn from(x: F64) -> Self {
        Value::F64(x)
    }
}
impl TryFrom<Value> for F64 {
    type Error = ValueTypeMismatchError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::F64(x) => Ok(x),
            _ => Err(ValueTypeMismatchError),
        }
    }
}

impl From<[u8; 16]> for Value {
    fn from(value: [u8; 16]) -> Self {
        Value::V128(value)
    }
}
impl TryFrom<Value> for [u8; 16] {
    type Error = ValueTypeMismatchError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::V128(x) => Ok(x),
            _ => Err(ValueTypeMismatchError),
        }
    }
}

impl From<Ref> for Value {
    fn from(value: Ref) -> Self {
        Self::Ref(value)
    }
}

impl TryFrom<Value> for Ref {
    type Error = ValueTypeMismatchError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::Ref(rref) => Ok(rref),
            _ => Err(ValueTypeMismatchError),
        }
    }
}

#[cfg(test)]
mod test {
    use crate::wasm::execution::value::{F32, F64};

    #[test]
    fn rounding_f32() {
        let round_towards_0_f32 = F32(0.5 - f32::EPSILON).round();
        let round_towards_1_f32 = F32(0.5 + f32::EPSILON).round();

        assert_eq!(round_towards_0_f32, F32(0.0));
        assert_eq!(round_towards_1_f32, F32(1.0));
    }

    #[test]
    fn rounding_f64() {
        let round_towards_0_f64 = F64(0.5 - f64::EPSILON).round();
        let round_towards_1_f64 = F64(0.5 + f64::EPSILON).round();

        assert_eq!(round_towards_0_f64, F64(0.0));
        assert_eq!(round_towards_1_f64, F64(1.0));
    }

    // ...
}
