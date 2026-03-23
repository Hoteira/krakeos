pub mod emitter;
pub mod compiler;
pub mod runtime;
pub mod trampoline;

pub use compiler::AotCompiler;
pub use runtime::AotModule;

pub const RING3_RT_BLOB: &[u8] = include_bytes!("../../../../build/ring3_rt.bin");
