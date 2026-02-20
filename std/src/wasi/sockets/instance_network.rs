#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "wasi:sockets/instance-network@0.2.0")]
unsafe extern "C" {
    #[link_name = "instance-network"]
    pub fn instance_network() -> i32;
}

#[cfg(target_arch = "x86_64")]
pub unsafe fn instance_network() -> i32 {
    0 // Return a dummy network handle
}
