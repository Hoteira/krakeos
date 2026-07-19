use crate::common::value::{F32, F64};

pub trait LittleEndianBytes<const N: usize>: Sized {
    fn from_le_bytes(bytes: [u8; N]) -> Self;
    fn to_le_bytes(self) -> [u8; N];
}

impl LittleEndianBytes<4> for u32 {
    fn from_le_bytes(bytes: [u8; 4]) -> Self { u32::from_le_bytes(bytes) }
    fn to_le_bytes(self) -> [u8; 4] { self.to_le_bytes() }
}

impl LittleEndianBytes<4> for i32 {
    fn from_le_bytes(bytes: [u8; 4]) -> Self { i32::from_le_bytes(bytes) }
    fn to_le_bytes(self) -> [u8; 4] { self.to_le_bytes() }
}

impl LittleEndianBytes<8> for u64 {
    fn from_le_bytes(bytes: [u8; 8]) -> Self { u64::from_le_bytes(bytes) }
    fn to_le_bytes(self) -> [u8; 8] { self.to_le_bytes() }
}

impl LittleEndianBytes<8> for i64 {
    fn from_le_bytes(bytes: [u8; 8]) -> Self { i64::from_le_bytes(bytes) }
    fn to_le_bytes(self) -> [u8; 8] { self.to_le_bytes() }
}

impl LittleEndianBytes<2> for u16 {
    fn from_le_bytes(bytes: [u8; 2]) -> Self { u16::from_le_bytes(bytes) }
    fn to_le_bytes(self) -> [u8; 2] { self.to_le_bytes() }
}

impl LittleEndianBytes<2> for i16 {
    fn from_le_bytes(bytes: [u8; 2]) -> Self { i16::from_le_bytes(bytes) }
    fn to_le_bytes(self) -> [u8; 2] { self.to_le_bytes() }
}

impl LittleEndianBytes<1> for u8 {
    fn from_le_bytes(bytes: [u8; 1]) -> Self { bytes[0] }
    fn to_le_bytes(self) -> [u8; 1] { [self] }
}

impl LittleEndianBytes<1> for i8 {
    fn from_le_bytes(bytes: [u8; 1]) -> Self { bytes[0] as i8 }
    fn to_le_bytes(self) -> [u8; 1] { [self as u8] }
}

impl LittleEndianBytes<4> for F32 {
    fn from_le_bytes(bytes: [u8; 4]) -> Self { F32::from_bits(u32::from_le_bytes(bytes)) }
    fn to_le_bytes(self) -> [u8; 4] { self.to_bits().to_le_bytes() }
}

impl LittleEndianBytes<8> for F64 {
    fn from_le_bytes(bytes: [u8; 8]) -> Self { F64::from_bits(u64::from_le_bytes(bytes)) }
    fn to_le_bytes(self) -> [u8; 8] { self.to_bits().to_le_bytes() }
}
