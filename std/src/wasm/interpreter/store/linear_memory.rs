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

pub enum LinearMemoryStorage {
    Managed(Vec<AtomicU8>),
    Sas {
        base: *mut AtomicU8,
        current_pages: u32,
        max_pages: u32,
    },
    Nested {
        container_id: u64,
        base: *mut AtomicU8,
        current_pages: u32,
        max_pages: u32,
    }
}

pub struct LinearMemory<const PAGE_SIZE: usize = { Limits::MEM_PAGE_SIZE as usize }> {
    storage: RwSpinLock<LinearMemoryStorage>,
}

pub type PageCountTy = u32;

impl<const PAGE_SIZE: usize> LinearMemory<PAGE_SIZE> {
    const PAGE_SIZE: usize = PAGE_SIZE;

    pub fn new() -> Self {
        Self {
            storage: RwSpinLock::new(LinearMemoryStorage::Managed(Vec::new())),
        }
    }

    pub fn new_with_initial_pages(pages: PageCountTy) -> Self {
        let size_bytes = Self::PAGE_SIZE * pages as usize;
        let mut data = Vec::with_capacity(size_bytes);
        data.resize_with(size_bytes, || AtomicU8::new(0));
        Self {
            storage: RwSpinLock::new(LinearMemoryStorage::Managed(data)),
        }
    }

    pub fn new_sas(base_addr: u64, pages: PageCountTy) -> Self {
        let size_bytes = Self::PAGE_SIZE * pages as usize;
        // Map the SAS pages immediately
        unsafe {
            // syscall 9 = MMAP(addr, len, prot, flags, fd, offset)
            // prot=7 (READ|WRITE|EXEC), flags=0, fd=0, offset=0
            crate::sys::syscall6(9, base_addr, size_bytes as u64, 7, 0, 0, 0);
        }
        Self {
            storage: RwSpinLock::new(LinearMemoryStorage::Sas {
                base: base_addr as *mut AtomicU8,
                current_pages: pages,
                max_pages: 65536, // 4GB max for WASM32
            }),
        }
    }

    pub fn new_view(container_id: u64, base_ptr: *mut u8, pages: PageCountTy, max_pages: PageCountTy) -> Self {
        Self {
            storage: RwSpinLock::new(LinearMemoryStorage::Nested {
                container_id,
                base: base_ptr as *mut AtomicU8,
                current_pages: pages,
                max_pages,
            }),
        }
    }

    pub fn grow(&self, pages_to_add: PageCountTy) -> Result<(), ()> {
        let mut lock_guard = self.storage.write();
        match &mut *lock_guard {
            LinearMemoryStorage::Managed(data) => {
                let prior_length_bytes = data.len();
                let new_length_bytes = prior_length_bytes + Self::PAGE_SIZE * pages_to_add as usize;
                if data.try_reserve(new_length_bytes - prior_length_bytes).is_err() {
                    return Err(());
                }
                data.resize_with(new_length_bytes, || AtomicU8::new(0));
                Ok(())
            }
            LinearMemoryStorage::Sas { base, current_pages, max_pages } => {
                if *current_pages + pages_to_add > *max_pages {
                    return Err(());
                }
                let old_size = *current_pages as usize * Self::PAGE_SIZE;
                let add_size = pages_to_add as usize * Self::PAGE_SIZE;
                
                unsafe {
                    let target_addr = (*base as usize + old_size) as u64;
                    crate::sys::syscall6(9, target_addr, add_size as u64, 7, 0, 0, 0);
                }
                
                *current_pages += pages_to_add;
                Ok(())
            }
            LinearMemoryStorage::Nested { container_id, current_pages, max_pages, .. } => {
                if *current_pages + pages_to_add > *max_pages {
                    // In a more advanced implementation, we would request the parent to grow here.
                    // For now, if the requested growth is within the container's max_pages, we allow it.
                    return Err(());
                }
                
                // Update container registry metadata
                {
                    let registry = crate::wasm::container::CONTAINER_REGISTRY.lock();
                    if let Some(container) = registry.get(container_id) {
                        let mut c = container.lock();
                        c.memory_size += pages_to_add as u64 * Self::PAGE_SIZE as u64;
                    }
                }

                *current_pages += pages_to_add;
                Ok(())
            }
        }
    }

    pub fn pages(&self) -> PageCountTy {
        let lock_guard = self.storage.read();
        match &*lock_guard {
            LinearMemoryStorage::Managed(data) => PageCountTy::try_from(data.len() / PAGE_SIZE).unwrap(),
            LinearMemoryStorage::Sas { current_pages, .. } => *current_pages,
            LinearMemoryStorage::Nested { current_pages, .. } => *current_pages,
        }
    }

    pub fn len(&self) -> usize {
        let lock_guard = self.storage.read();
        match &*lock_guard {
            LinearMemoryStorage::Managed(data) => data.len(),
            LinearMemoryStorage::Sas { current_pages, .. } => *current_pages as usize * Self::PAGE_SIZE,
            LinearMemoryStorage::Nested { current_pages, .. } => *current_pages as usize * Self::PAGE_SIZE,
        }
    }

    pub fn get_base_ptr(&self) -> *mut u8 {
        let lock_guard = self.storage.read();
        match &*lock_guard {
            LinearMemoryStorage::Managed(data) => data.as_ptr() as *mut u8,
            LinearMemoryStorage::Sas { base, .. } => *base as *mut u8,
            LinearMemoryStorage::Nested { base, .. } => *base as *mut u8,
        }
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
        let lock_guard = self.storage.read();
        let len = match &*lock_guard {
            LinearMemoryStorage::Managed(data) => data.len(),
            LinearMemoryStorage::Sas { current_pages, .. } => *current_pages as usize * Self::PAGE_SIZE,
            LinearMemoryStorage::Nested { current_pages, .. } => *current_pages as usize * Self::PAGE_SIZE,
        };

        if N > len || index > len - N {
            return Err(TrapError::MemoryOrDataAccessOutOfBounds.into());
        }

        match &*lock_guard {
            LinearMemoryStorage::Managed(data) => {
                for (i, byte) in bytes.into_iter().enumerate() {
                    let dst = unsafe { data.get_unchecked(i + index) };
                    dst.store(byte, Ordering::Relaxed);
                }
            }
            LinearMemoryStorage::Sas { base, .. } => {
                for (i, byte) in bytes.into_iter().enumerate() {
                    let dst = unsafe { &*base.add(i + index) };
                    dst.store(byte, Ordering::Relaxed);
                }
            }
            LinearMemoryStorage::Nested { base, .. } => {
                for (i, byte) in bytes.into_iter().enumerate() {
                    let dst = unsafe { &*base.add(i + index) };
                    dst.store(byte, Ordering::Relaxed);
                }
            }
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
        let lock_guard = self.storage.read();
        let len = match &*lock_guard {
            LinearMemoryStorage::Managed(data) => data.len(),
            LinearMemoryStorage::Sas { current_pages, .. } => *current_pages as usize * Self::PAGE_SIZE,
            LinearMemoryStorage::Nested { current_pages, .. } => *current_pages as usize * Self::PAGE_SIZE,
        };

        if N > len || index > len - N {
            return Err(TrapError::MemoryOrDataAccessOutOfBounds.into());
        }

        let mut bytes = [0; N];
        match &*lock_guard {
            LinearMemoryStorage::Managed(data) => {
                for (i, byte) in bytes.iter_mut().enumerate() {
                    let src = unsafe { data.get_unchecked(i + index) };
                    *byte = src.load(Ordering::Relaxed);
                }
            }
            LinearMemoryStorage::Sas { base, .. } => {
                for (i, byte) in bytes.iter_mut().enumerate() {
                    let src = unsafe { &*base.add(i + index) };
                    *byte = src.load(Ordering::Relaxed);
                }
            }
            LinearMemoryStorage::Nested { base, .. } => {
                for (i, byte) in bytes.iter_mut().enumerate() {
                    let src = unsafe { &*base.add(i + index) };
                    *byte = src.load(Ordering::Relaxed);
                }
            }
        }
        Ok(bytes)
    }

    pub fn read_slice(&self, index: MemIdx, buf: &mut [u8]) -> Result<(), RuntimeError> {
        let lock_guard = self.storage.read();
        let len = match &*lock_guard {
            LinearMemoryStorage::Managed(data) => data.len(),
            LinearMemoryStorage::Sas { current_pages, .. } => *current_pages as usize * Self::PAGE_SIZE,
            LinearMemoryStorage::Nested { current_pages, .. } => *current_pages as usize * Self::PAGE_SIZE,
        };

        let buf_len = buf.len();
        if buf_len > len || index > len - buf_len {
            return Err(TrapError::MemoryOrDataAccessOutOfBounds.into());
        }

        match &*lock_guard {
            LinearMemoryStorage::Managed(data) => {
                for (i, byte) in buf.iter_mut().enumerate() {
                    let src = unsafe { data.get_unchecked(i + index) };
                    *byte = src.load(Ordering::Relaxed);
                }
            }
            LinearMemoryStorage::Sas { base, .. } => {
                for (i, byte) in buf.iter_mut().enumerate() {
                    let src = unsafe { &*base.add(i + index) };
                    *byte = src.load(Ordering::Relaxed);
                }
            }
            LinearMemoryStorage::Nested { base, .. } => {
                for (i, byte) in buf.iter_mut().enumerate() {
                    let src = unsafe { &*base.add(i + index) };
                    *byte = src.load(Ordering::Relaxed);
                }
            }
        }
        Ok(())
    }

    pub fn fill(&self, index: MemIdx, data_byte: u8, count: MemIdx) -> Result<(), RuntimeError> {
        let lock_guard = self.storage.read();
        let len = match &*lock_guard {
            LinearMemoryStorage::Managed(data) => data.len(),
            LinearMemoryStorage::Sas { current_pages, .. } => *current_pages as usize * Self::PAGE_SIZE,
            LinearMemoryStorage::Nested { current_pages, .. } => *current_pages as usize * Self::PAGE_SIZE,
        };

        if count > len || index > len - count {
            return Err(TrapError::MemoryOrDataAccessOutOfBounds.into());
        }
        if count == 0 {
            return Ok(());
        }

        match &*lock_guard {
            LinearMemoryStorage::Managed(data) => {
                for i in index..(index + count) {
                    let lin_mem_byte = unsafe { data.get_unchecked(i) };
                    lin_mem_byte.store(data_byte, Ordering::Relaxed);
                }
            }
            LinearMemoryStorage::Sas { base, .. } => {
                for i in index..(index + count) {
                    let lin_mem_byte = unsafe { &*base.add(i) };
                    lin_mem_byte.store(data_byte, Ordering::Relaxed);
                }
            }
            LinearMemoryStorage::Nested { base, .. } => {
                for i in index..(index + count) {
                    let lin_mem_byte = unsafe { &*base.add(i) };
                    lin_mem_byte.store(data_byte, Ordering::Relaxed);
                }
            }
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
        let lock_guard_self = self.storage.read();
        let lock_guard_other = source_mem.storage.read();

        let len_self = match &*lock_guard_self {
            LinearMemoryStorage::Managed(data) => data.len(),
            LinearMemoryStorage::Sas { current_pages, .. } => *current_pages as usize * Self::PAGE_SIZE,
            LinearMemoryStorage::Nested { current_pages, .. } => *current_pages as usize * Self::PAGE_SIZE,
        };
        let len_other = match &*lock_guard_other {
            LinearMemoryStorage::Managed(data) => data.len(),
            LinearMemoryStorage::Sas { current_pages, .. } => *current_pages as usize * Self::PAGE_SIZE,
            LinearMemoryStorage::Nested { current_pages, .. } => *current_pages as usize * Self::PAGE_SIZE,
        };

        if count > len_other || source_index > len_other - count {
            return Err(TrapError::MemoryOrDataAccessOutOfBounds.into());
        }
        if count > len_self || destination_index > len_self - count {
            return Err(TrapError::MemoryOrDataAccessOutOfBounds.into());
        }
        if count == 0 {
            return Ok(());
        }

        let get_src = |i| match &*lock_guard_other {
            LinearMemoryStorage::Managed(data) => unsafe { data.get_unchecked(i + source_index) },
            LinearMemoryStorage::Sas { base, .. } => unsafe { &*base.add(i + source_index) },
            LinearMemoryStorage::Nested { base, .. } => unsafe { &*base.add(i + source_index) },
        };

        let get_dst = |i| match &*lock_guard_self {
            LinearMemoryStorage::Managed(data) => unsafe { data.get_unchecked(i + destination_index) },
            LinearMemoryStorage::Sas { base, .. } => unsafe { &*base.add(i + destination_index) },
            LinearMemoryStorage::Nested { base, .. } => unsafe { &*base.add(i + destination_index) },
        };

        if destination_index <= source_index {
            for i in 0..count {
                let byte = get_src(i).load(Ordering::Relaxed);
                get_dst(i).store(byte, Ordering::Relaxed);
            }
        } else {
            for i in (0..count).rev() {
                let byte = get_src(i).load(Ordering::Relaxed);
                get_dst(i).store(byte, Ordering::Relaxed);
            }
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
        let lock_guard_self = self.storage.read();
        let len_self = match &*lock_guard_self {
            LinearMemoryStorage::Managed(data) => data.len(),
            LinearMemoryStorage::Sas { current_pages, .. } => *current_pages as usize * Self::PAGE_SIZE,
            LinearMemoryStorage::Nested { current_pages, .. } => *current_pages as usize * Self::PAGE_SIZE,
        };

        let data_len = source_data.len();
        if count > data_len || source_index > data_len - count {
            return Err(TrapError::MemoryOrDataAccessOutOfBounds.into());
        }
        if count > len_self || destination_index > len_self - count {
            return Err(TrapError::MemoryOrDataAccessOutOfBounds.into());
        }
        if count == 0 {
            return Ok(());
        }

        match &*lock_guard_self {
            LinearMemoryStorage::Managed(data) => {
                for i in 0..count {
                    let src_byte = unsafe { source_data.get_unchecked(i + source_index) };
                    let dst_byte = unsafe { data.get_unchecked(i + destination_index) };
                    dst_byte.store(*src_byte, Ordering::Relaxed);
                }
            }
            LinearMemoryStorage::Sas { base, .. } => {
                for i in 0..count {
                    let src_byte = unsafe { source_data.get_unchecked(i + source_index) };
                    let dst_byte = unsafe { &*base.add(i + destination_index) };
                    dst_byte.store(*src_byte, Ordering::Relaxed);
                }
            }
            LinearMemoryStorage::Nested { base, .. } => {
                for i in 0..count {
                    let src_byte = unsafe { source_data.get_unchecked(i + source_index) };
                    let dst_byte = unsafe { &*base.add(i + destination_index) };
                    dst_byte.store(*src_byte, Ordering::Relaxed);
                }
            }
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
