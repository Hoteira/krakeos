use crate::rust_alloc::{string::String};
use crate::wasm::{
    common::{
        config::Config,
        interop::Linker,
    },
    interpreter::store::{Store},
};

pub static mut RANDOM_STATE: u64 = 1574;

pub fn create_wasi_imports<T: Config>(linker: &mut Linker, store: &mut Store<'_, T>) {
    if store.wasi_ctx.is_none() {
        store.wasi_ctx = Some(crate::wasm::wasi::ctx::WasiCtx::default());
    }

    crate::time::wasi::register_wasi(linker, store);
    crate::fs::wasi::register_wasi(linker, store);
    crate::io::wasi::register_wasi(linker, store);
    crate::env::wasi::register_wasi(linker, store);
    crate::process::wasi::register_wasi(linker, store);
    crate::random::wasi::register_wasi(linker, store);
    crate::net::wasi::register_wasi(linker, store);
}
