use crate::alloc::vec::Vec;
use crate::wasm::common::config::Config;
use crate::wasm::interpreter::store::Store;

#[repr(C)]
pub struct AotContext {
    pub store: *mut usize, // Pointer to Store<T>
    pub fuel: *mut u32,
    pub memory_base: *mut u8,
    pub memory_size: usize,
    pub stack_base: *mut u128,
    pub locals_base: *mut u128,
    pub module_addr: usize,
    pub stack_limit: usize,
    pub trap_code: *mut i32,
}

pub struct AotModule {
    pub code: Vec<u8>,
    pub func_offsets: Vec<usize>,
}

unsafe impl Send for AotModule {}
unsafe impl Sync for AotModule {}

impl AotModule {
    pub fn new(code: &[u8], func_offsets: Vec<usize>) -> Self {
        Self {
            code: code.to_vec(),
            func_offsets,
        }
    }

    pub fn get_func_ptr(&self, func_idx: usize) -> *const u8 {
        let offset = self.func_offsets[func_idx];
        unsafe { self.code.as_ptr().add(offset) }
    }
}
