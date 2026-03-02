use crate::alloc::vec::Vec;
use crate::wasm::common::indices::MemIdx;
use crate::wasm::common::rw_spinlock::RwSpinLock;
use crate::wasm::common::little_endian::LittleEndianBytes;
use crate::wasm::common::runtime_error::{RuntimeError, TrapError};
use crate::wasm::common::reader::types::Limits;
use core::{
    iter,
    sync::atomic::{AtomicU8, Ordering},
};

pub struct LinearMemory<const PAGE_SIZE: usize = { Limits::MEM_PAGE_SIZE as usize }> {
    inner_data: RwSpinLock<Vec<AtomicU8>>,
}

pub type PageCountTy = u32;

impl<const PAGE_SIZE: usize> LinearMemory<PAGE_SIZE> {
    const PAGE_SIZE: usize = PAGE_SIZE;
    pub fn new() -> Self {
        Self {
            inner_data: RwSpinLock::new(Vec::new()),
        }
    }
    pub fn new_with_initial_pages(pages: PageCountTy) -> Self {
        let size_bytes = Self::PAGE_SIZE * pages as usize;
        let mut data = Vec::with_capacity(size_bytes);
        data.resize_with(size_bytes, || AtomicU8::new(0));
        Self {
            inner_data: RwSpinLock::new(data),
        }
    }
    pub fn grow(&self, pages_to_add: PageCountTy) -> Result<(), ()> {
        let mut lock_guard = self.inner_data.write();
        let prior_length_bytes = lock_guard.len();
        let new_length_bytes = prior_length_bytes + Self::PAGE_SIZE * pages_to_add as usize;
        if lock_guard.try_reserve(new_length_bytes - prior_length_bytes).is_err() {
            return Err(());
        }
        lock_guard.resize_with(new_length_bytes, || AtomicU8::new(0));
        Ok(())
    }
    pub fn pages(&self) -> PageCountTy {
        PageCountTy::try_from(self.inner_data.read().len() / PAGE_SIZE).unwrap()
    }
    pub fn len(&self) -> usize {
        self.inner_data.read().len()
    }

    pub fn get_base_ptr(&self) -> *mut u8 {
        self.inner_data.read().as_ptr() as *mut u8
    }

    pub fn store<const N: usize, T: LittleEndianBytes<N>>(
        &self,
        index: MemIdx,
        value: T,
    ) -> Result<(), RuntimeError> {
        self.store_bytes::<N>(index, value.to_le_bytes())
    }
    pub fn store_bytes<const N: usize>(
        &self,
        index: MemIdx,
        bytes: [u8; N],
    ) -> Result<(), RuntimeError> {
        let lock_guard = self.inner_data.read();
        if N > lock_guard.len() {
            return Err(TrapError::MemoryOrDataAccessOutOfBounds.into());
        }
        if index > lock_guard.len() - N {
            return Err(TrapError::MemoryOrDataAccessOutOfBounds.into());
        }
        for (i, byte) in bytes.into_iter().enumerate() {
            let dst = unsafe { lock_guard.get_unchecked(i + index) };
            dst.store(byte, Ordering::Relaxed);
        }
        Ok(())
    }
    pub fn load<const N: usize, T: LittleEndianBytes<N>>(
        &self,
        index: MemIdx,
    ) -> Result<T, RuntimeError> {
        self.load_bytes::<N>(index).map(T::from_le_bytes)
    }
    pub fn load_bytes<const N: usize>(&self, index: MemIdx) -> Result<[u8; N], RuntimeError> {
        let lock_guard = self.inner_data.read();
        if N > lock_guard.len() {
            return Err(TrapError::MemoryOrDataAccessOutOfBounds.into());
        }
        if index > lock_guard.len() - N {
            return Err(TrapError::MemoryOrDataAccessOutOfBounds.into());
        }
        let mut bytes = [0; N];
        for (i, byte) in bytes.iter_mut().enumerate() {
            let src = unsafe { lock_guard.get_unchecked(i + index) };
            *byte = src.load(Ordering::Relaxed);
        }
        Ok(bytes)
    }
    pub fn read_slice(&self, index: MemIdx, buf: &mut [u8]) -> Result<(), RuntimeError> {
        let lock_guard = self.inner_data.read();
        let len = buf.len();
        if len > lock_guard.len() {
            return Err(TrapError::MemoryOrDataAccessOutOfBounds.into());
        }
        if index > lock_guard.len() - len {
            return Err(TrapError::MemoryOrDataAccessOutOfBounds.into());
        }
        for (i, byte) in buf.iter_mut().enumerate() {
            let src = unsafe { lock_guard.get_unchecked(i + index) };
            *byte = src.load(Ordering::Relaxed);
        }
        Ok(())
    }
    pub fn fill(&self, index: MemIdx, data_byte: u8, count: MemIdx) -> Result<(), RuntimeError> {
        let lock_guard = self.inner_data.read();
        if count > lock_guard.len() {
            return Err(TrapError::MemoryOrDataAccessOutOfBounds.into());
        }
        if index > lock_guard.len() - count {
            return Err(TrapError::MemoryOrDataAccessOutOfBounds.into());
        }
        if count == 0 {
            return Ok(());
        }
        for i in index..(index + count) {
            let lin_mem_byte = unsafe { lock_guard.get_unchecked(i) };
            lin_mem_byte.store(data_byte, Ordering::Relaxed);
        }
        Ok(())
    }
    pub fn copy(
        &self,
        destination_index: MemIdx,
        source_mem: &Self,
        source_index: MemIdx,
        count: MemIdx,
    ) -> Result<(), RuntimeError> {
        let lock_guard_self = self.inner_data.read();
        let lock_guard_other = source_mem.inner_data.read();
        if count > lock_guard_other.len() {
            return Err(TrapError::MemoryOrDataAccessOutOfBounds.into());
        }
        if source_index > lock_guard_other.len() - count {
            return Err(TrapError::MemoryOrDataAccessOutOfBounds.into());
        }
        if count > lock_guard_self.len() {
            return Err(TrapError::MemoryOrDataAccessOutOfBounds.into());
        }
        if destination_index > lock_guard_self.len() - count {
            return Err(TrapError::MemoryOrDataAccessOutOfBounds.into());
        }
        if count == 0 {
            return Ok(());
        }
        let copy_one_byte = move |i| {
            let src_byte: &AtomicU8 = unsafe { lock_guard_other.get_unchecked(i + source_index) };
            let dst_byte: &AtomicU8 =
                unsafe { lock_guard_self.get_unchecked(i + destination_index) };
            let byte = src_byte.load(Ordering::Relaxed);
            dst_byte.store(byte, Ordering::Relaxed);
        };
        if destination_index <= source_index {
            (0..count).for_each(copy_one_byte)
        } else {
            (0..count).rev().for_each(copy_one_byte)
        }
        Ok(())
    }
    pub fn init(
        &self,
        destination_index: MemIdx,
        source_data: &[u8],
        source_index: MemIdx,
        count: MemIdx,
    ) -> Result<(), RuntimeError> {
        let lock_guard_self = self.inner_data.read();
        let data_len = source_data.len();
        if count > data_len {
            return Err(TrapError::MemoryOrDataAccessOutOfBounds.into());
        }
        if source_index > data_len - count {
            return Err(TrapError::MemoryOrDataAccessOutOfBounds.into());
        }
        if count > lock_guard_self.len() {
            return Err(TrapError::MemoryOrDataAccessOutOfBounds.into());
        }
        if destination_index > lock_guard_self.len() - count {
            return Err(TrapError::MemoryOrDataAccessOutOfBounds.into());
        }
        if count == 0 {
            return Ok(());
        }
        for i in 0..count {
            let src_byte = unsafe { source_data.get_unchecked(i + source_index) };
            let dst_byte = unsafe { lock_guard_self.get_unchecked(i + destination_index) };
            dst_byte.store(*src_byte, Ordering::Relaxed);
        }
        Ok(())
    }
}

impl<const PAGE_SIZE: usize> core::fmt::Debug for LinearMemory<PAGE_SIZE> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("LinearMemory")
            .field("size", &self.len())
            .finish()
    }
}

impl<const PAGE_SIZE: usize> Default for LinearMemory<PAGE_SIZE> {
    fn default() -> Self {
        Self::new()
    }
}