# WASM AOT Compiler (Naive)

## Objective
Implement a fully featured Ahead-Of-Time (AOT) compiler for WebAssembly within KrakeOS, matching the capabilities of the existing interpreter.

## Implementation Details
- **Architecture**: Targeting x86_64.
- **Strategy**: Naive translation of WASM instructions to machine code.
- **Stack Mapping**: Mapping WASM value stack to the x86_64 hardware stack or a dedicated register-backed region.
- **Control Flow**: Direct jump translation instead of sidetable-driven interpretation.
- **Interoperability**: Full support for host functions, globals, tables, and linear memory.

## Challenges
- **Fuel Tracking**: Implementing fuel exhaustion checks in compiled code.
- **SIMD/Atomics**: Direct machine code generation for complex multi-byte instructions.
- **Dynamic Re-linking**: Efficiently calling between AOT functions and host functions.

## Progress
- [x] Initial `GEMINI.md` setup.
- [x] AOT module structure.
- [x] Machine code emitter (x86_64 + SSE).
- [x] WASM instruction set implementation (Core Numeric, Variable, Parametric).
- [x] Control flow implementation (Block, Loop, If, Br, BrIf, BrTable).
- [x] SIMD & Atomics implementation (Inline + Trampolines).
- [x] Host function interoperability via `aot_call_host`.
- [x] Executable memory management.
- [x] Multi-value return support.
- [x] Fuel tracking in compiled code.
- [x] Memory bounds checking in compiled code.

## Challenges Solved
- **Mixed Stack Sizes**: Solved by using uniform 16-byte slots on the hardware stack, ensuring SSE alignment and `v128` compatibility.
- **Indirect Calls**: Implemented via a signature-checking trampoline that returns AOT code pointers.
- **SIMD Complexity**: Handled by a combination of inline x86_64 SSE instructions for common paths and a robust trampoline system for complex opcodes.
