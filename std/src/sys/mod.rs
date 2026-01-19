#[cfg(target_arch = "x86_64")]
pub mod krakeos;
#[cfg(target_arch = "x86_64")]
pub use krakeos::*;

#[cfg(target_arch = "wasm32")]
pub mod wasi;
#[cfg(target_arch = "wasm32")]
pub use wasi::*;
