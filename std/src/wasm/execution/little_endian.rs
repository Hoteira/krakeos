use super::value::{F32, F64};
macro_rules! impl_LittleEndianBytes {
        [$($type:ty),+] => {
            $(impl LittleEndianBytes<{ ::core::mem::size_of::<$type>() }> for $type {
                fn from_le_bytes(bytes: [u8; ::core::mem::size_of::<$type>()]) -> Self {
                    Self::from_le_bytes(bytes)
                }
                fn to_le_bytes(self) -> [u8; ::core::mem::size_of::<$type>()] {
                    self.to_le_bytes()
                }
            })+
        }
    }
pub trait LittleEndianBytes<const N: usize> {
    fn from_le_bytes(bytes: [u8; N]) -> Self;
    fn to_le_bytes(self) -> [u8; N];
}
impl_LittleEndianBytes![i8, i16, i32, i64, i128, u8, u16, u32, u64, u128];
impl LittleEndianBytes<4> for F32 {
    fn from_le_bytes(bytes: [u8; 4]) -> Self {
        F32(f32::from_le_bytes(bytes))
    }
    fn to_le_bytes(self) -> [u8; 4] {
        self.0.to_le_bytes()
    }
}
impl LittleEndianBytes<8> for F64 {
    fn from_le_bytes(bytes: [u8; 8]) -> Self {
        F64(f64::from_le_bytes(bytes))
    }
    fn to_le_bytes(self) -> [u8; 8] {
        self.0.to_le_bytes()
    }
}
