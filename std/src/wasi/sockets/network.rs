#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "wasi:sockets/network@0.2.0")]
unsafe extern "C" {}

#[cfg(target_arch = "x86_64")]
pub unsafe fn placeholder() {}
