#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "wasi:sockets/network@0.2.0")]
unsafe extern "C" {}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn placeholder() {}
