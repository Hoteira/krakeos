use crate::wasm::core::error::ValidationError;
use crate::wasm::core::reader::span::Span;
pub mod section_header;
pub mod types;
#[derive(Clone)]
pub struct WasmReader<'a> {
    /// Entire WASM binary as slice
    pub full_wasm_binary: &'a [u8],
    pub pc: usize,
}
impl<'a> WasmReader<'a> {
    pub const fn new(wasm: &'a [u8]) -> Self {
        Self {
            full_wasm_binary: wasm,
            pc: 0,
        }
    }
    /// Advance the cursor to the first byte of the provided [Span] and validates that entire [Span] fits the WASM binary
    ///
    /// # Note
    ///
    /// This allows setting the [`pc`](WasmReader::pc) to one byte *past* the end of
    /// [full_wasm_binary](WasmReader::full_wasm_binary), **if** the [Span]'s length is 0. For
    pub fn move_start_to(&mut self, span: Span) -> Result<(), ValidationError> {
        if span.from + span.len > self.full_wasm_binary.len() {
            return Err(ValidationError::Eof);
        }
        self.pc = span.from;
        Ok(())
    }
    pub fn remaining_bytes(&self) -> &[u8] {
        &self.full_wasm_binary[self.pc..]
    }
    pub fn make_span(&self, len: usize) -> Result<Span, ValidationError> {
        if self.pc + len > self.full_wasm_binary.len() {
            return Err(ValidationError::Eof);
        }
        Ok(Span::new(self.pc, len))
    }
    pub fn strip_bytes<const N: usize>(&mut self) -> Result<[u8; N], ValidationError> {
        if N > self.full_wasm_binary.len() - self.pc {
            return Err(ValidationError::Eof);
        }
        let bytes = &self.full_wasm_binary[self.pc..(self.pc + N)];
        self.pc += N;
        Ok(bytes.try_into().expect("the slice length to be exactly N"))
    }
    pub fn strip_bytes_dynamic(&mut self, len: usize) -> Result<&'a [u8], ValidationError> {
        if len > self.full_wasm_binary.len() - self.pc {
            return Err(ValidationError::Eof);
        }
        let bytes = &self.full_wasm_binary[self.pc..(self.pc + len)];
        self.pc += len;
        Ok(bytes)
    }
    /// Read the current byte without advancing the [`pc`](Self::pc)
    ///
    /// May yield an error if the [`pc`](Self::pc) advanced past the end of the WASM binary slice
    pub fn peek_u8(&self) -> Result<u8, ValidationError> {
        self.full_wasm_binary
            .get(self.pc)
            .copied()
            .ok_or(ValidationError::Eof)
    }
    /// Call a closure that may mutate the [WasmReader]
    ///
    /// Returns a tuple of the closure's return value and the number of bytes that the [`WasmReader`]
    pub fn measure_num_read_bytes<T>(
        &mut self,
        f: impl FnOnce(&mut WasmReader) -> Result<T, ValidationError>,
    ) -> Result<(T, usize), ValidationError> {
        let before = self.pc;
        let ret = f(self)?;
        debug_assert!(
            self.pc >= before,
            "pc was advanced backwards towards the start"
        );
        let num_read_bytes = self.pc - before;
        Ok((ret, num_read_bytes))
    }
    #[allow(dead_code)]
    pub fn skip(&mut self, num_bytes: usize) -> Result<(), ValidationError> {
        if num_bytes > self.full_wasm_binary.len() - self.pc {
            return Err(ValidationError::Eof);
        }
        self.pc += num_bytes;
        Ok(())
    }
    pub fn into_inner(self) -> &'a [u8] {
        self.full_wasm_binary
    }
    /// A wrapper function for reads with transaction-like behavior.
    ///
    /// The provided closure will be called with `&mut self` and its result will be returned.
    /// However if the closure returns `Err(_)`, `self` will be reset as if the closure was never called.
    #[allow(dead_code)]
    pub fn handle_transaction<T, E>(
        &mut self,
        f: impl FnOnce(&mut WasmReader<'a>) -> Result<T, E>,
    ) -> Result<T, E> {
        let original = self.clone();
        f(self).inspect_err(|_| {
            *self = original;
        })
    }
}
pub trait WasmReadable: Sized {
    fn read(wasm: &mut WasmReader) -> Result<Self, ValidationError>;
}
pub mod span {
    use crate::wasm::core::reader::WasmReader;
    use core::ops::Index;
    #[derive(Copy, Clone, Debug, Hash)]
    pub struct Span {
        pub from: usize,
        pub len: usize,
    }
    impl Span {
        pub const fn new(from: usize, len: usize) -> Self {
            Self { from, len }
        }
        pub const fn len(&self) -> usize {
            self.len
        }
        pub const fn from(&self) -> usize {
            self.from
        }
    }
    impl<'a> Index<Span> for WasmReader<'a> {
        type Output = [u8];
        fn index(&self, index: Span) -> &'a Self::Output {
            &self.full_wasm_binary[index.from..(index.from + index.len)]
        }
    }
}
#[cfg(test)]
mod test {
    use super::*;
    use crate::rust_alloc::vec;
    use crate::wasm::ValType;
    #[test]
    fn move_start_to() {
        let my_bytes = vec![0x11, 0x12, 0x13, 0x14, 0x15];
        let mut wasm_reader = WasmReader::new(&my_bytes);
        let span = Span::new(0, 0);
        wasm_reader.move_start_to(span).unwrap();
        wasm_reader.peek_u8().unwrap();
        let span = Span::new(0, my_bytes.len());
        wasm_reader.move_start_to(span).unwrap();
        wasm_reader.peek_u8().unwrap();
        assert_eq!(wasm_reader[span], my_bytes);
        let span = Span::new(my_bytes.len(), 0);
        wasm_reader.move_start_to(span).unwrap();
        let span = Span::new(my_bytes.len() - 1, 1);
        wasm_reader.move_start_to(span).unwrap();
        assert_eq!(wasm_reader.peek_u8().unwrap(), *my_bytes.last().unwrap());
    }
    #[test]
    fn move_start_to_out_of_bounds_1() {
        let my_bytes = vec![0x11, 0x12, 0x13, 0x14, 0x15];
        let mut wasm_reader = WasmReader::new(&my_bytes);
        let span = Span::new(my_bytes.len(), 1);
        assert_eq!(wasm_reader.move_start_to(span), Err(ValidationError::Eof));
    }
    #[test]
    fn move_start_to_out_of_bounds_2() {
        let my_bytes = vec![0x11, 0x12, 0x13, 0x14, 0x15];
        let mut wasm_reader = WasmReader::new(&my_bytes);
        let span = Span::new(0, my_bytes.len() + 1);
        assert_eq!(wasm_reader.move_start_to(span), Err(ValidationError::Eof));
    }
    #[test]
    fn remaining_bytes_1() {
        let my_bytes = vec![0x11, 0x12, 0x13, 0x14, 0x15];
        let mut wasm_reader = WasmReader::new(&my_bytes);
        assert_eq!(wasm_reader.remaining_bytes(), my_bytes);
        wasm_reader.skip(4).unwrap();
        assert_eq!(wasm_reader.peek_u8().unwrap(), 0x15);
        assert_eq!(wasm_reader.remaining_bytes(), &my_bytes[4..]);
    }
    #[test]
    fn remaining_bytes_2() {
        let my_bytes = vec![0x11, 0x12, 0x13, 0x14, 0x15];
        let mut wasm_reader = WasmReader::new(&my_bytes);
        assert_eq!(wasm_reader.remaining_bytes(), my_bytes);
        wasm_reader.skip(5).unwrap();
        assert_eq!(wasm_reader.remaining_bytes(), &my_bytes[5..]);
        assert_eq!(wasm_reader.remaining_bytes(), &[]);
    }
    #[test]
    fn strip_bytes_1() {
        let my_bytes = vec![0x11, 0x12, 0x13, 0x14, 0x15];
        let mut wasm_reader = WasmReader::new(&my_bytes);
        assert_eq!(wasm_reader.remaining_bytes(), my_bytes);
        let stripped_bytes = wasm_reader.strip_bytes::<4>().unwrap();
        assert_eq!(&stripped_bytes, &my_bytes[..4]);
        assert_eq!(wasm_reader.remaining_bytes(), &[0x15]);
    }
    #[test]
    fn strip_bytes_2() {
        let my_bytes = vec![0x11, 0x12, 0x13, 0x14, 0x15];
        let mut wasm_reader = WasmReader::new(&my_bytes);
        assert_eq!(wasm_reader.remaining_bytes(), my_bytes);
        wasm_reader.skip(1).unwrap();
        let stripped_bytes = wasm_reader.strip_bytes::<4>().unwrap();
        assert_eq!(&stripped_bytes, &my_bytes[1..5]);
        assert_eq!(wasm_reader.remaining_bytes(), &[]);
    }
    #[test]
    fn strip_bytes_3() {
        let my_bytes = vec![0x11, 0x12, 0x13, 0x14, 0x15];
        let mut wasm_reader = WasmReader::new(&my_bytes);
        assert_eq!(wasm_reader.remaining_bytes(), my_bytes);
        wasm_reader.skip(2).unwrap();
        let stripped_bytes = wasm_reader.strip_bytes::<4>();
        assert_eq!(stripped_bytes, Err(ValidationError::Eof));
    }
    #[test]
    fn strip_bytes_4() {
        let my_bytes = vec![0x11, 0x12, 0x13, 0x14, 0x15];
        let mut wasm_reader = WasmReader::new(&my_bytes);
        assert_eq!(wasm_reader.remaining_bytes(), my_bytes);
        wasm_reader.skip(5).unwrap();
        let stripped_bytes = wasm_reader.strip_bytes::<0>().unwrap();
        assert_eq!(stripped_bytes, [0u8; 0]);
    }
    #[test]
    fn skip_1() {
        let my_bytes = vec![0x11, 0x12, 0x13, 0x14, 0x15];
        let mut wasm_reader = WasmReader::new(&my_bytes);
        assert_eq!(wasm_reader.remaining_bytes(), my_bytes);
        assert_eq!(wasm_reader.skip(6), Err(ValidationError::Eof));
    }
    #[test]
    fn reader_transaction() {
        let bytes = [0x1, 0x2, 0x3, 0x4, 0x5, 0x6];
        let mut reader = WasmReader::new(&bytes);
        assert_eq!(
            reader.handle_transaction(|reader| { reader.strip_bytes::<2>() }),
            Ok([0x1, 0x2]),
        );
        let transaction_result: Result<(), ValidationError> = reader.handle_transaction(|reader| {
            assert_eq!(reader.strip_bytes::<2>(), Ok([0x3, 0x4]));
            Err(ValidationError::InvalidMagic)
        });
        assert_eq!(transaction_result, Err(ValidationError::InvalidMagic));
        assert_eq!(reader.strip_bytes::<3>(), Ok([0x3, 0x4, 0x5]));
    }
    #[test]
    fn reader_transaction_ergonomics() {
        let bytes = [0x1, 0x2, 0x3, 0x4, 0x5, 0x6];
        let mut reader = WasmReader::new(&bytes);
        assert_eq!(reader.handle_transaction(WasmReader::read_u8), Ok(0x1));
        assert_eq!(
            reader.handle_transaction(ValType::read),
            Err(ValidationError::MalformedValType)
        );
    }
}
