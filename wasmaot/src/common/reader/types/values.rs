use alloc::vec::Vec;
use crate::common::error::ValidationError;
use crate::common::reader::WasmReader;

const CONTINUATION_BIT: u8 = 0b10000000;
const INTEGER_BIT_FLAG: u8 = !CONTINUATION_BIT;

impl WasmReader<'_> {
    pub fn read_u8(&mut self) -> Result<u8, ValidationError> {
        let byte = self.peek_u8()?;
        self.pc += 1;
        Ok(byte)
    }

    pub fn read_var_u32(&mut self) -> Result<u32, ValidationError> {
        const PADDING_IN_LAST_BYTE_BIT_MASK: u8 = 0b01110000;
        let mut result: u32 = 0;
        let byte = self.read_u8()?;
        result |= u32::from(byte & INTEGER_BIT_FLAG);
        if byte & CONTINUATION_BIT == 0 {
            return Ok(result);
        }
        let byte = self.read_u8()?;
        result |= u32::from(byte & INTEGER_BIT_FLAG) << 7;
        if byte & CONTINUATION_BIT == 0 {
            return Ok(result);
        }
        let byte = self.read_u8()?;
        result |= u32::from(byte & INTEGER_BIT_FLAG) << 14;
        if byte & CONTINUATION_BIT == 0 {
            return Ok(result);
        }
        let byte = self.read_u8()?;
        result |= u32::from(byte & INTEGER_BIT_FLAG) << 21;
        if byte & CONTINUATION_BIT == 0 {
            return Ok(result);
        }
        let byte = self.read_u8()?;
        result |= u32::from(byte & INTEGER_BIT_FLAG) << 28;
        let has_next_byte = byte & CONTINUATION_BIT > 0;
        let padding_bits_are_not_zero = byte & PADDING_IN_LAST_BYTE_BIT_MASK > 0;
        if has_next_byte || padding_bits_are_not_zero {
            return Err(ValidationError::MalformedVariableLengthInteger);
        }
        Ok(result)
    }

    pub fn read_f64(&mut self) -> Result<u64, ValidationError> {
        let bytes = self.strip_bytes::<8>()?;
        Ok(u64::from_le_bytes(bytes))
    }

    pub fn read_var_i32(&mut self) -> Result<i32, ValidationError> {
        const PADDING_IN_LAST_BYTE_BITMASK: u8 = 0b01110000;
        const SIGN_IN_LAST_BYTE_BITFLAG: u8 = 0b00001000;
        const NUM_BITS: u32 = 32;
        let mut result: i32 = 0;
        let byte = self.read_u8()?;
        result |= i32::from(byte & INTEGER_BIT_FLAG);
        if byte & CONTINUATION_BIT == 0 {
            const NUM_UNSPECIFIED_BITS: u32 = NUM_BITS - 7;
            let sign_extended_result = (result << NUM_UNSPECIFIED_BITS) >> NUM_UNSPECIFIED_BITS;
            return Ok(sign_extended_result);
        }
        let byte = self.read_u8()?;
        result |= i32::from(byte & INTEGER_BIT_FLAG) << 7;
        if byte & CONTINUATION_BIT == 0 {
            const NUM_UNSPECIFIED_BITS: u32 = NUM_BITS - 14;
            let sign_extended_result = (result << NUM_UNSPECIFIED_BITS) >> NUM_UNSPECIFIED_BITS;
            return Ok(sign_extended_result);
        }
        let byte = self.read_u8()?;
        result |= i32::from(byte & INTEGER_BIT_FLAG) << 14;
        if byte & CONTINUATION_BIT == 0 {
            const NUM_UNSPECIFIED_BITS: u32 = NUM_BITS - 21;
            let sign_extended_result = (result << NUM_UNSPECIFIED_BITS) >> NUM_UNSPECIFIED_BITS;
            return Ok(sign_extended_result);
        }
        let byte = self.read_u8()?;
        result |= i32::from(byte & INTEGER_BIT_FLAG) << 21;
        if byte & CONTINUATION_BIT == 0 {
            const NUM_UNSPECIFIED_BITS: u32 = NUM_BITS - 28;
            let sign_extended_result = (result << NUM_UNSPECIFIED_BITS) >> NUM_UNSPECIFIED_BITS;
            return Ok(sign_extended_result);
        }
        let byte = self.read_u8()?;
        result |= i32::from(byte & INTEGER_BIT_FLAG) << 28;
        let has_next_byte = byte & CONTINUATION_BIT > 0;
        if has_next_byte {
            return Err(ValidationError::MalformedVariableLengthInteger);
        }
        const PADDING_AND_SIGN_BITMASK: u8 = PADDING_IN_LAST_BYTE_BITMASK | SIGN_IN_LAST_BYTE_BITFLAG;
        let number_of_ones_in_padding_and_sign_bits = (byte & PADDING_AND_SIGN_BITMASK).count_ones();
        let padding_bits_match_sign_bit = number_of_ones_in_padding_and_sign_bits == PADDING_AND_SIGN_BITMASK.count_ones() || number_of_ones_in_padding_and_sign_bits == 0;
        if !padding_bits_match_sign_bit {
            return Err(ValidationError::MalformedVariableLengthInteger);
        }
        Ok(result)
    }

    pub fn read_var_i33_as_u32(&mut self) -> Result<u32, ValidationError> {
        const PADDING_IN_LAST_BYTE_BITMASK: u8 = 0b01100000;
        const SIGN_IN_LAST_BYTE_BITFLAG: u8 = 0b00010000;
        const NUM_BITS: u32 = 33;
        let mut result: i64 = 0;
        let byte = self.read_u8()?;
        result |= i64::from(byte & INTEGER_BIT_FLAG);
        if byte & CONTINUATION_BIT == 0 {
            const NUM_UNSPECIFIED_BITS: u32 = NUM_BITS - 7;
            let sign_extended_result = (result << NUM_UNSPECIFIED_BITS) >> NUM_UNSPECIFIED_BITS;
            return u32::try_from(sign_extended_result).map_err(|_| ValidationError::I33IsNegative);
        }
        let byte = self.read_u8()?;
        result |= i64::from(byte & INTEGER_BIT_FLAG) << 7;
        if byte & CONTINUATION_BIT == 0 {
            const NUM_UNSPECIFIED_BITS: u32 = NUM_BITS - 14;
            let sign_extended_result = (result << NUM_UNSPECIFIED_BITS) >> NUM_UNSPECIFIED_BITS;
            return u32::try_from(sign_extended_result).map_err(|_| ValidationError::I33IsNegative);
        }
        let byte = self.read_u8()?;
        result |= i64::from(byte & INTEGER_BIT_FLAG) << 14;
        if byte & CONTINUATION_BIT == 0 {
            const NUM_UNSPECIFIED_BITS: u32 = NUM_BITS - 21;
            let sign_extended_result = (result << NUM_UNSPECIFIED_BITS) >> NUM_UNSPECIFIED_BITS;
            return u32::try_from(sign_extended_result).map_err(|_| ValidationError::I33IsNegative);
        }
        let byte = self.read_u8()?;
        result |= i64::from(byte & INTEGER_BIT_FLAG) << 21;
        if byte & CONTINUATION_BIT == 0 {
            const NUM_UNSPECIFIED_BITS: u32 = NUM_BITS - 28;
            let sign_extended_result = (result << NUM_UNSPECIFIED_BITS) >> NUM_UNSPECIFIED_BITS;
            return u32::try_from(sign_extended_result).map_err(|_| ValidationError::I33IsNegative);
        }
        let byte = self.read_u8()?;
        result |= i64::from(byte & INTEGER_BIT_FLAG) << 28;
        let has_next_byte = byte & CONTINUATION_BIT > 0;
        if has_next_byte {
            return Err(ValidationError::MalformedVariableLengthInteger);
        }
        const PADDING_AND_SIGN_BITMASK: u8 = PADDING_IN_LAST_BYTE_BITMASK | SIGN_IN_LAST_BYTE_BITFLAG;
        let number_of_ones_in_padding_and_sign_bits = (byte & PADDING_AND_SIGN_BITMASK).count_ones();
        let padding_bits_match_sign_bit = number_of_ones_in_padding_and_sign_bits == PADDING_AND_SIGN_BITMASK.count_ones() || number_of_ones_in_padding_and_sign_bits == 0;
        if !padding_bits_match_sign_bit {
            return Err(ValidationError::MalformedVariableLengthInteger);
        }
        u32::try_from(result).map_err(|_| ValidationError::I33IsNegative)
    }

    pub fn read_f32(&mut self) -> Result<u32, ValidationError> {
        let bytes = self.strip_bytes::<4>()?;
        Ok(u32::from_le_bytes(bytes))
    }

    pub fn read_var_i64(&mut self) -> Result<i64, ValidationError> {
        const PADDING_IN_LAST_BYTE_BITMASK: u8 = 0b01111110;
        const SIGN_IN_LAST_BYTE_BITFLAG: u8 = 0b00000001;
        const NUM_BITS: u32 = 64;
        let mut result: i64 = 0;
        let byte = self.read_u8()?;
        result |= i64::from(byte & INTEGER_BIT_FLAG);
        if byte & CONTINUATION_BIT == 0 {
            const NUM_UNSPECIFIED_BITS: u32 = NUM_BITS - 7;
            let sign_extended_result = (result << NUM_UNSPECIFIED_BITS) >> NUM_UNSPECIFIED_BITS;
            return Ok(sign_extended_result);
        }
        let byte = self.read_u8()?;
        result |= i64::from(byte & INTEGER_BIT_FLAG) << 7;
        if byte & CONTINUATION_BIT == 0 {
            const NUM_UNSPECIFIED_BITS: u32 = NUM_BITS - 14;
            let sign_extended_result = (result << NUM_UNSPECIFIED_BITS) >> NUM_UNSPECIFIED_BITS;
            return Ok(sign_extended_result);
        }
        let byte = self.read_u8()?;
        result |= i64::from(byte & INTEGER_BIT_FLAG) << 14;
        if byte & CONTINUATION_BIT == 0 {
            const NUM_UNSPECIFIED_BITS: u32 = NUM_BITS - 21;
            let sign_extended_result = (result << NUM_UNSPECIFIED_BITS) >> NUM_UNSPECIFIED_BITS;
            return Ok(sign_extended_result);
        }
        let byte = self.read_u8()?;
        result |= i64::from(byte & INTEGER_BIT_FLAG) << 21;
        if byte & CONTINUATION_BIT == 0 {
            const NUM_UNSPECIFIED_BITS: u32 = NUM_BITS - 28;
            let sign_extended_result = (result << NUM_UNSPECIFIED_BITS) >> NUM_UNSPECIFIED_BITS;
            return Ok(sign_extended_result);
        }
        let byte = self.read_u8()?;
        result |= i64::from(byte & INTEGER_BIT_FLAG) << 28;
        if byte & CONTINUATION_BIT == 0 {
            const NUM_UNSPECIFIED_BITS: u32 = NUM_BITS - 35;
            let sign_extended_result = (result << NUM_UNSPECIFIED_BITS) >> NUM_UNSPECIFIED_BITS;
            return Ok(sign_extended_result);
        }
        let byte = self.read_u8()?;
        result |= i64::from(byte & INTEGER_BIT_FLAG) << 35;
        if byte & CONTINUATION_BIT == 0 {
            const NUM_UNSPECIFIED_BITS: u32 = NUM_BITS - 42;
            let sign_extended_result = (result << NUM_UNSPECIFIED_BITS) >> NUM_UNSPECIFIED_BITS;
            return Ok(sign_extended_result);
        }
        let byte = self.read_u8()?;
        result |= i64::from(byte & INTEGER_BIT_FLAG) << 42;
        if byte & CONTINUATION_BIT == 0 {
            const NUM_UNSPECIFIED_BITS: u32 = NUM_BITS - 49;
            let sign_extended_result = (result << NUM_UNSPECIFIED_BITS) >> NUM_UNSPECIFIED_BITS;
            return Ok(sign_extended_result);
        }
        let byte = self.read_u8()?;
        result |= i64::from(byte & INTEGER_BIT_FLAG) << 49;
        if byte & CONTINUATION_BIT == 0 {
            const NUM_UNSPECIFIED_BITS: u32 = NUM_BITS - 56;
            let sign_extended_result = (result << NUM_UNSPECIFIED_BITS) >> NUM_UNSPECIFIED_BITS;
            return Ok(sign_extended_result);
        }
        let byte = self.read_u8()?;
        result |= i64::from(byte & INTEGER_BIT_FLAG) << 56;
        if byte & CONTINUATION_BIT == 0 {
            const NUM_UNSPECIFIED_BITS: u32 = NUM_BITS - 63;
            let sign_extended_result = (result << NUM_UNSPECIFIED_BITS) >> NUM_UNSPECIFIED_BITS;
            return Ok(sign_extended_result);
        }
        let byte = self.read_u8()?;
        result |= i64::from(byte & INTEGER_BIT_FLAG) << 63;
        let has_next_byte = byte & CONTINUATION_BIT > 0;
        if has_next_byte {
            return Err(ValidationError::MalformedVariableLengthInteger);
        }
        const PADDING_AND_SIGN_BITMASK: u8 = PADDING_IN_LAST_BYTE_BITMASK | SIGN_IN_LAST_BYTE_BITFLAG;
        let number_of_ones_in_padding_and_sign_bits = (byte & PADDING_AND_SIGN_BITMASK).count_ones();
        let padding_bits_match_sign_bit = number_of_ones_in_padding_and_sign_bits == PADDING_AND_SIGN_BITMASK.count_ones() || number_of_ones_in_padding_and_sign_bits == 0;
        if !padding_bits_match_sign_bit {
            return Err(ValidationError::MalformedVariableLengthInteger);
        }
        Ok(result)
    }

    pub fn read_name(&mut self) -> Result<&str, ValidationError> {
        let len = self.read_var_u32()? as usize;
        let utf8_str = &self
            .full_wasm_binary
            .get(self.pc..(self.pc + len))
            .ok_or(ValidationError::Eof)?;
        self.pc += len;
        core::str::from_utf8(utf8_str).map_err(ValidationError::MalformedUtf8)
    }

    pub fn read_component_name(&mut self) -> Result<alloc::string::String, ValidationError> {
        use alloc::string::ToString;
        use alloc::format;
        let tag = self.read_u8()?;
        match tag {
            0x00 | 0x01 => Ok(self.read_name()?.to_string()),
            0x02 => {
                let s1 = self.read_name()?.to_string();
                let s2 = self.read_name()?.to_string();
                Ok(format!("{}:{}", s1, s2))
            }
            _ => {
                crate::debugln!("Invalid component name tag: {:#x} at pc {:#x}", tag, self.pc - 1);
                Err(ValidationError::Component(tag))
            }
        }
    }

    pub fn read_vec_enumerated<T, F>(
        &mut self,
        mut read_element: F,
    ) -> Result<Vec<T>, ValidationError>
    where
        F: FnMut(&mut Self, usize) -> Result<T, ValidationError>,
    {
        let mut idx = 0;
        self.read_vec(|wasm| {
            let ret = read_element(wasm, idx);
            idx += 1;
            ret
        })
    }

    pub fn read_vec<T, F>(&mut self, mut read_element: F) -> Result<Vec<T>, ValidationError>
    where
        F: FnMut(&mut Self) -> Result<T, ValidationError>,
    {
        let len = self.read_var_u32()?;
        core::iter::repeat_with(|| read_element(self))
            .take(len as usize)
            .collect()
    }
}
