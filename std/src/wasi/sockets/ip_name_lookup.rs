#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "wasi:sockets/ip-name-lookup@0.2.0")]
unsafe extern "C" {
    #[link_name = "resolve-addresses"]
    pub fn resolve_addresses(network: i32, name_ptr: *const u8, name_len: u32, result_ptr: *mut u8);
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn resolve_addresses(_network: i32, _name_ptr: *const u8, _name_len: u32, result_ptr: *mut u8) {
    *result_ptr = 1; // Not supported natively yet
}
