use crate::rust_alloc::vec::Vec;
use crate::wasm::{
    core::indices::MemIdx,
    core::rw_spinlock::{ReadLockGuard, RwSpinLock},
    execution::little_endian::LittleEndianBytes,
    RuntimeError, TrapError,
};
use core::{
    iter,
    sync::atomic::{AtomicU8, Ordering},
};
pub struct LinearMemory<const PAGE_SIZE: usize = { crate::wasm::Limits::MEM_PAGE_SIZE as usize }> {
    inner_data: RwSpinLock<Vec<AtomicU8>>,
}
pub type PageCountTy = u16;
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
            error!("value does not fit into linear memory");
            return Err(TrapError::MemoryOrDataAccessOutOfBounds.into());
        }
        if index > lock_guard.len() - N {
            error!("value write would extend beyond the end of the linear memory");
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
            error!("value does not fit into linear memory");
            return Err(TrapError::MemoryOrDataAccessOutOfBounds.into());
        }
        if index > lock_guard.len() - N {
            error!("value read would extend beyond the end of the linear_memory");
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
            error!("fill count is bigger than the linear memory");
            return Err(TrapError::MemoryOrDataAccessOutOfBounds.into());
        }
        if index > lock_guard.len() - count {
            error!("fill extends beyond the linear memory's end");
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
            error!("copy count is bigger than the source linear memory");
            return Err(TrapError::MemoryOrDataAccessOutOfBounds.into());
        }
        if source_index > lock_guard_other.len() - count {
            error!("copy source extends beyond the linear memory's end");
            return Err(TrapError::MemoryOrDataAccessOutOfBounds.into());
        }
        if count > lock_guard_self.len() {
            error!("copy count is bigger than the destination linear memory");
            return Err(TrapError::MemoryOrDataAccessOutOfBounds.into());
        }
        if destination_index > lock_guard_self.len() - count {
            error!("copy destination extends beyond the linear memory's end");
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
            error!("init count is bigger than the data instance");
            return Err(TrapError::MemoryOrDataAccessOutOfBounds.into());
        }
        if source_index > data_len - count {
            error!("init source extends beyond the data instance's end");
            return Err(TrapError::MemoryOrDataAccessOutOfBounds.into());
        }
        if count > lock_guard_self.len() {
            error!("init count is bigger than the linear memory");
            return Err(TrapError::MemoryOrDataAccessOutOfBounds.into());
        }
        if destination_index > lock_guard_self.len() - count {
            error!("init extends beyond the linear memory's end");
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
        /// A helper struct for formatting a [`Vec<UnsafeCell<u8>>`] which is guarded by a [`ReadLockGuard`].
        /// This formatter is able to detect and format byte repetitions in a compact way.
        struct RepetitionDetectingMemoryWriter<'a>(ReadLockGuard<'a, Vec<AtomicU8>>);
        impl core::fmt::Debug for RepetitionDetectingMemoryWriter<'_> {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                /// The number of repetitions required for successive elements to be grouped
                // together.
                const MIN_REPETITIONS_FOR_GROUP: usize = 8;
                // First we create an iterator over all bytes
                let mut bytes = self.0.iter().map(|x| x.load(Ordering::Relaxed));
                // Then we iterate over all bytes and deduplicate repetitions. This produces an
                // iterator of pairs, consisting of the number of repetitions and the repeated byte
                // itself. `current_group` is captured by the iterator and used as state to track
                // the current group.
                let mut current_group: Option<(usize, u8)> = None;
                let deduplicated_with_count = iter::from_fn(|| {
                    for byte in bytes.by_ref() {
                        // If the next byte is different than the one being tracked currently...
                        if current_group.is_some() && current_group.unwrap().1 != byte {
                            // ...then end and emit the current group but also start a new group for
                            // the next byte with an initial count of 1.
                            return current_group.replace((1, byte));
                        }
                        // Otherwise increment the current group's counter or start a new group if
                        current_group.get_or_insert((0, byte)).0 += 1;
                    }
                    current_group.take()
                });
                let mut list = f.debug_list();
                deduplicated_with_count.for_each(|(count, value)| {
                    if count < MIN_REPETITIONS_FOR_GROUP {
                        list.entries(iter::repeat(value).take(count));
                    } else {
                        list.entry(&format_args!("#{count} × {value}"));
                    }
                });
                list.finish()
            }
        }
        f.debug_struct("LinearMemory")
            .field(
                "inner_data",
                &RepetitionDetectingMemoryWriter(self.inner_data.read()),
            )
            .finish()
    }
}
impl<const PAGE_SIZE: usize> Default for LinearMemory<PAGE_SIZE> {
    fn default() -> Self {
        Self::new()
    }
}
#[cfg(test)]
mod test {
    use super::*;
    const PAGE_SIZE: usize = 1 << 8;
    const PAGES: PageCountTy = 2;
    #[test]
    fn new_constructor() {
        let lin_mem = LinearMemory::<PAGE_SIZE>::new();
        assert_eq!(lin_mem.pages(), 0);
    }
    #[test]
    fn new_grow() {
        let lin_mem = LinearMemory::<PAGE_SIZE>::new();
        lin_mem.grow(1);
        assert_eq!(lin_mem.pages(), 1);
    }
}
