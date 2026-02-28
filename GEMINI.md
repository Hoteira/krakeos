# Gemini CLI Mandates: KrakeOS

This document serves as the foundational authority for all AI agents operating within the KrakeOS codebase. The instructions herein take absolute precedence over general defaults.

## 1. Project Identity & Intent
KrakeOS is a **WASM-native, Single Address Space (SAS) operating system**. Its primary objective is to treat WebAssembly as a first-class execution format, implementing a recursive, sandbox-secure environment inspired by Lisp Machines.

## 2. System Architecture & Memory Layout
All development must adhere to the Single Address Space model where isolation is enforced by the WASM runtime/AOT compiler.

### Virtual Address Map
- **User Code:** `0x0000_0001_0000_0000` (4GB). 1GB per process.
- **Shared Memory (SHM):** `0x0000_0040_0000_0000` (256GB).
- **Heap:** `0x0000_0080_0000_0000` (512GB). 4GB per process.
- **Stack:** `0x0000_7FFF_FFFF_0000` (~112TB) [Grows Downward].

### WASM Containers
- **Nested Execution:** Support in-process nesting where sub-programs are "planted" in parent linear memory.
- **Memory Propagation:** Ensure child `memory.grow` calls correctly propagate to parent buffers.
- **Return Values:** Use sub-program return values (typically `i32`) as the primary inter-container communication channel.

## 3. Engineering Standards

### Toolchain & Environment
- **Language:** Rust Nightly (`no_std` + `alloc`).
- **Native Target:** x86_64 (`bits64.json`, `bits64pie.json`).
- **WASM Target:** `wasm32-wasip2`.

### Coding Practices
- **Memory Safety:** Kernel-side shared state **must** be protected by `sync::Mutex` or `sync::Spinlock`.
- **AOT Integrity:** Maintain strict 16-byte stack alignment in the AOT compiler trampolines and emitters.
- **WASI Compliance:** Prioritize implementations aligning with WASI Preview 2 (0.2.x) and Preview 3 (0.3.0) interfaces.

## 4. Operational Guidelines

### Build & Verification
- **Build Entrypoint:** Use `make.bat` for full system builds and image generation.
- **Fast Feedback:** Use `cargo check` on individual crates (`kernel`, `std`, `inkui`) for rapid verification.
- **Path Resolution:** Default `PATH` is `@0xE0/sys/bin;@0xE0/apps`.

### GUI Development (`inkui`)
- **Rendering:** Software rasterization via `inkui`.
- **Compositor:** Kernel window manager handles alpha blending (SSE-accelerated).
- **Assets:** Use `titanf` for fonts and `asvgard` for SVG/PNG assets.

## 5. Global System Keys (Mandatory Retention)
- **Super + T:** Terminal
- **Super + P:** VMA/Memory Dump
- **Super + X:** Kill Window
- **Super + Z:** Maximize Toggle
- **Super + C:** Resize Mode

## 6. Project Structure
- `kernel/`: Monolithic Kernel Core.
- `std/`: Runtime Library (WASM AOT/Interpreter, WASI).
- `inkui/`: GUI Framework.
- `userland/`: Bootstrap Shell.
- `swiftboot/`: Bootloader.
- `tree/`: Filesystem Template.
