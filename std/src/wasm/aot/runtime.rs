use crate::rust_alloc::vec::Vec;
use crate::wasm::interpreter::store::Store;
use crate::wasm::common::config::Config;

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
}

pub struct AotModule {
    pub code_ptr: *mut u8,
    pub code_size: usize,
    pub func_offsets: Vec<usize>,
}

impl AotModule {
    pub fn new(code: &[u8], func_offsets: Vec<usize>) -> Self {
        let size = (code.len() + 0xFFF) & !0xFFF;
        let ptr = unsafe { crate::sys::alloc_pages(size) };
        if ptr.is_null() { panic!("Failed to allocate executable memory for AOT"); }
        unsafe {
            core::ptr::copy_nonoverlapping(code.as_ptr(), ptr, code.len());
        }
        Self {
            code_ptr: ptr,
            code_size: size,
            func_offsets,
        }
    }

    pub fn get_func_ptr(&self, func_idx: usize) -> *const u8 {
        let offset = self.func_offsets[func_idx];
        unsafe { self.code_ptr.add(offset) }
    }
}
