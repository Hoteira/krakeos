// WASM to RISC-V AOT compiler for KrakeOS: core wasm modules plus WASI preview 1 only.
// common/ is the arch-neutral reader and validator, aot/ is the RISC-V backend.

#![no_std]

extern crate alloc;

// Debug logging stub; swap the body for a UART write to enable.
#[macro_export]
macro_rules! debugln {
    ($($arg:tt)*) => {{ let _ = format_args!($($arg)*); }};
}

// Trace logging stub.
#[macro_export]
macro_rules! trace {
    ($($arg:tt)*) => {{ let _ = format_args!($($arg)*); }};
}

pub mod aot;
pub mod common;
pub mod math;
