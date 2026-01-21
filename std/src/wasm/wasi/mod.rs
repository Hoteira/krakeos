pub mod ctx;
pub mod preview1;
pub mod preview2;
pub mod env;
pub mod krakeos;

pub use ctx::{WasiCtx, WasiResource};
pub use preview1::create_wasi_imports;
pub use preview2::create_wasi_p2_imports;

pub static mut ICRNL: bool = false;