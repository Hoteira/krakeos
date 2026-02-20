#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "wasi:random/random@0.2.0")]
unsafe extern "C" {
    #[link_name = "get-random-bytes"]
    pub fn get_random_bytes(len: u64, result_ptr: *mut u8);
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn get_random_bytes(len: u64, result_ptr: *mut u8) {
    // Pseudo-random for native
    static mut STATE: u64 = 1574;
    if STATE == 1574 {
        STATE = crate::sys::syscall(109, 0, 0, 0).wrapping_add(0xACE1BADE);
    }
    for i in 0..len as usize {
        STATE ^= STATE << 13;
        STATE ^= STATE >> 17;
        STATE ^= STATE << 5;
        *result_ptr.add(i) = (STATE & 0xFF) as u8;
    }
}
