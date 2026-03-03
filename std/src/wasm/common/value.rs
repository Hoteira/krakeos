use crate::math::FloatMath;
use crate::wasm::common::reader::types::{NumType, ValType};
use crate::wasm::common::reader::types::RefType;
use core::fmt::{Debug, Display};
use core::ops::{Add, Div, Mul, Sub};
use core::{f32, f64};

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
        self.0.to_bits() == other.0.to_bits()
    }
}

impl Eq for F32 {}

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
        Self::from_bits(self.to_bits() & 0x7FFFFFFF)
    }
    pub fn neg(&self) -> Self {
        Self::from_bits(self.to_bits() ^ 0x80000000)
    }
    pub fn ceil(&self) -> Self {
        if self.is_nan() { return *self; }
        Self(self.0.ceil())
    }
    pub fn floor(&self) -> Self {
        if self.is_nan() { return *self; }
        Self(self.0.floor())
    }
    pub fn trunc(&self) -> Self {
        if self.is_nan() { return *self; }
        Self(self.0.trunc())
    }
    pub fn nearest(&self) -> Self {
        if self.is_nan() { return *self; }
        let f = self.0;
        let round = f.round();
        if (f - round).abs() == 0.5 {
            if round % 2.0 != 0.0 {
                return Self(round - f.signum());
            }
        }
        Self(round)
    }
    pub fn round(&self) -> Self {
        Self(self.0.round())
    }
    pub fn sqrt(&self) -> Self {
        Self(self.0.sqrt())
    }
    pub fn min(&self, rhs: Self) -> Self {
        if self.is_nan() { return *self; }
        if rhs.is_nan() { return rhs; }
        let (a, b) = (self.0, rhs.0);
        if a < b { return *self; }
        if b < a { return rhs; }
        Self::from_bits(self.to_bits() | rhs.to_bits())
    }
    pub fn max(&self, rhs: Self) -> Self {
        if self.is_nan() { return *self; }
        if rhs.is_nan() { return rhs; }
        let (a, b) = (self.0, rhs.0);
        if a > b { return *self; }
        if b > a { return rhs; }
        Self::from_bits(self.to_bits() & rhs.to_bits())
    }
    pub fn copysign(&self, rhs: Self) -> Self {
        Self::from_bits((self.to_bits() & 0x7FFFFFFF) | (rhs.to_bits() & 0x80000000))
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
        self.0.to_bits() == other.0.to_bits()
    }
}

impl Eq for F64 {}

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
        Self::from_bits(self.to_bits() & 0x7FFFFFFFFFFFFFFF)
    }
    pub fn neg(&self) -> Self {
        Self::from_bits(self.to_bits() ^ 0x8000000000000000)
    }
    pub fn ceil(&self) -> Self {
        if self.is_nan() { return *self; }
        Self(self.0.ceil())
    }
    pub fn floor(&self) -> Self {
        if self.is_nan() { return *self; }
        Self(self.0.floor())
    }
    pub fn trunc(&self) -> Self {
        if self.is_nan() { return *self; }
        Self(self.0.trunc())
    }
    pub fn nearest(&self) -> Self {
        if self.is_nan() { return *self; }
        let f = self.0;
        let round = f.round();
        if (f - round).abs() == 0.5 {
            if round % 2.0 != 0.0 {
                return Self(round - f.signum());
            }
        }
        Self(round)
    }
    pub fn round(&self) -> Self {
        Self(self.0.round())
    }
    pub fn sqrt(&self) -> Self {
        Self(self.0.sqrt())
    }
    pub fn min(&self, rhs: Self) -> Self {
        if self.is_nan() { return *self; }
        if rhs.is_nan() { return rhs; }
        let (a, b) = (self.0, rhs.0);
        if a < b { return *self; }
        if b < a { return rhs; }
        Self::from_bits(self.to_bits() | rhs.to_bits())
    }
    pub fn max(&self, rhs: Self) -> Self {
        if self.is_nan() { return *self; }
        if rhs.is_nan() { return rhs; }
        let (a, b) = (self.0, rhs.0);
        if a > b { return *self; }
        if b > a { return rhs; }
        Self::from_bits(self.to_bits() & rhs.to_bits())
    }
    pub fn copysign(&self, rhs: Self) -> Self {
        Self::from_bits((self.to_bits() & 0x7FFFFFFFFFFFFFFF) | (rhs.to_bits() & 0x8000000000000000))
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    Func(usize),
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
    pub fn to_u128(&self) -> u128 {
        match self {
            Value::I32(x) => *x as u128,
            Value::I64(x) => *x as u128,
            Value::F32(x) => x.to_bits() as u128,
            Value::F64(x) => x.to_bits() as u128,
            Value::V128(x) => u128::from_le_bytes(*x),
            Value::Ref(Ref::Func(addr)) => *addr as u128,
            Value::Ref(_) => 0,
        }
    }

    pub fn from_u128(val: u128, ty: ValType) -> Self {
        match ty {
            ValType::NumType(NumType::I32) => Value::I32(val as u32),
            ValType::NumType(NumType::I64) => Value::I64(val as u64),
            ValType::NumType(NumType::F32) => Value::F32(F32::from_bits(val as u32)),
            ValType::NumType(NumType::F64) => Value::F64(F64::from_bits(val as u64)),
            ValType::VecType => Value::V128(val.to_le_bytes()),
            ValType::RefType(crate::wasm::common::reader::types::RefType::FuncRef) => Value::Ref(Ref::Func(val as usize)),
            _ => Value::I32(0),
        }
    }

    pub fn to_ty(&self) -> ValType {
        match self {
            Value::I32(_) => ValType::NumType(NumType::I32),
            Value::I64(_) => ValType::NumType(NumType::I64),
            Value::F32(_) => ValType::NumType(NumType::F32),
            Value::F64(_) => ValType::NumType(NumType::F64),
            Value::Ref(Ref::Null(ref_type)) => ValType::RefType(*ref_type),
            Value::Ref(Ref::Func(_)) => ValType::RefType(crate::wasm::common::reader::types::RefType::FuncRef),
            Value::Ref(Ref::Extern(_)) => ValType::RefType(crate::wasm::common::reader::types::RefType::ExternRef),
            Value::V128(_) => ValType::VecType,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct ValueTypeMismatchError {
    pub expected: &'static str,
    pub actual: Value,
}

impl Display for ValueTypeMismatchError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "failed to convert Value to a {} because the actual value was {:?}", self.expected, self.actual)
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
            Value::I64(x) => Ok(x as u32),
            Value::F32(x) => Ok(x.to_bits()),
            Value::F64(x) => Ok(x.to_bits() as u32),
            _ => Err(ValueTypeMismatchError { expected: "u32", actual: value }),
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
            Value::I64(x) => Ok(x as i32),
            Value::F32(x) => Ok(x.to_bits() as i32),
            Value::F64(x) => Ok(x.to_bits() as i32),
            _ => Err(ValueTypeMismatchError { expected: "i32", actual: value }),
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
            Value::I32(x) => Ok(x as u64),
            Value::I64(x) => Ok(x),
            Value::F32(x) => Ok(x.to_bits() as u64),
            Value::F64(x) => Ok(x.to_bits()),
            _ => Err(ValueTypeMismatchError { expected: "u64", actual: value }),
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
            Value::I32(x) => Ok(x as i64),
            Value::I64(x) => Ok(x as i64),
            Value::F32(x) => Ok(x.to_bits() as i64),
            Value::F64(x) => Ok(x.to_bits() as i64),
            _ => Err(ValueTypeMismatchError { expected: "i64", actual: value }),
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
            Value::I32(x) => Ok(F32::from_bits(x)),
            Value::I64(x) => Ok(F32::from_bits(x as u32)),
            Value::F32(x) => Ok(x),
            Value::F64(x) => Ok(F32::from_bits(x.to_bits() as u32)),
            _ => Err(ValueTypeMismatchError { expected: "F32", actual: value }),
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
            Value::I32(x) => Ok(F64::from_bits(x as u64)),
            Value::I64(x) => Ok(F64::from_bits(x)),
            Value::F32(x) => Ok(F64::from_bits(x.to_bits() as u64)),
            Value::F64(x) => Ok(x),
            _ => Err(ValueTypeMismatchError { expected: "F64", actual: value }),
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
            _ => Err(ValueTypeMismatchError { expected: "[u8; 16]", actual: value }),
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
            _ => Err(ValueTypeMismatchError { expected: "Ref", actual: value }),
        }
    }
}
