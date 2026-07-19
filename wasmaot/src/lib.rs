//! WASM → RISC-V AOT compiler for KrakeOS, repurposed from the x86_64
//! compiler in ref/std/src/wasm/aot. Scope: core wasm modules + WASI
//! preview 1 only — no component model, no preview 2.
//!
//! - `common/` is the arch-neutral reader/validator layer, ported nearly
//!   verbatim from ref.
//! - `aot/` is the RISC-V backend: Rv64Emitter (new), compiler (ported
//!   per-instruction from the x86 version with a simplified ABI), runtime.

#![no_std]

extern crate alloc;

/// Debug logging stub (the ref tree routed this to its kernel log). Enable
/// by swapping the body for a UART write if needed.
#[macro_export]
macro_rules! debugln {
    ($($arg:tt)*) => {{ let _ = format_args!($($arg)*); }};
}

/// Trace logging stub (ref routed this to its kernel log).
#[macro_export]
macro_rules! trace {
    ($($arg:tt)*) => {{ let _ = format_args!($($arg)*); }};
}

pub mod aot;
pub mod common;
pub mod math;
