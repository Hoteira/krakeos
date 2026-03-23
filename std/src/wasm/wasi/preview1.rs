use crate::alloc::{string::String};
use crate::wasm::{
    common::{
        config::Config,
        interop::Linker,
    },
    interpreter::store::{Store},
};

pub static mut RANDOM_STATE: u64 = 1574;

pub fn create_wasi_imports<T: Config + Clone>(linker: &mut Linker, store: &mut Store<'_, T>) {
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

    let func_addr = store.func_alloc_unchecked(
        crate::wasm::common::reader::types::FuncType {
            params: crate::wasm::common::reader::types::ResultType {
                valtypes: crate::alloc::vec![
                    crate::wasm::common::reader::types::ValType::NumType(crate::wasm::common::reader::types::NumType::I32),
                    crate::wasm::common::reader::types::ValType::NumType(crate::wasm::common::reader::types::NumType::I32)
                ],
            },
            returns: crate::wasm::common::reader::types::ResultType {
                valtypes: crate::alloc::vec![],
            },
        },
        |_, _| Ok(crate::alloc::vec![]),
    );
    let _ = linker.define_unchecked("env".into(), "host_serial_print".into(), crate::wasm::interpreter::store::ExternVal::Func(func_addr));
}
