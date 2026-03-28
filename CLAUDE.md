# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

KrakeOS is a **WASM-native, Single Address Space (SAS)** 64-bit operating system written in Rust Nightly targeting x86_64. Isolation is enforced by the WASM runtime/AOT compiler rather than hardware paging. The kernel boots into an idle loop and runs all user programs as WASM components.

## Build Commands

```bash
# Full build: bootloader + kernel + userland WASM apps + disk image
make

# Build and launch in QEMU
make run

# Clean all build artifacts
make clean

# Fast verification of individual crates (no linking)
cargo check --package=kernel
cargo check --package=std
cargo check --package=inkui
```

To build a single WASM app manually:
```bash
cargo build --package=<app_name> --target=wasm32-wasip2 --release
```

To build the kernel manually:
```bash
cargo build -Z json-target-spec --package=kernel --target=swiftboot/bits64.json
```

## Workspace Structure

| Crate | Purpose | Target |
|---|---|---|
| `kernel/` | Kernel core: scheduler, memory, VFS, syscalls, window manager | `swiftboot/bits64.json` |
| `std/` | Custom runtime: WASM interpreter, AOT compiler, WASI, syscall wrappers | kernel + `wasm32-wasip2` |
| `inkui/` | GUI widget framework (no_std) | `wasm32-wasip2` |
| `swiftboot/` | Multi-stage bootloader | custom |
| `elfic/` | ELF loader | kernel |
| `userland/` | Bootstrap shell process | `wasm32-wasip2` |
| `apps/` | User applications (shell, term, sysmon, taskbar, etc.) | `wasm32-wasip2` |

Custom linker targets:
- **`swiftboot/bits64.json`** — bare metal x86_64 kernel
- **`bits64pie.json`** — position-independent native userland

## Architecture

### Memory Layout (Single Address Space)
- **User Code:** `0x0000_0001_0000_0000` — 1 GB per process
- **Shared Memory:** `0x0000_0040_0000_0000` — 256 GB
- **Heap:** `0x0000_0080_0000_0000` — 4 GB per process
- **Stack:** `0x0000_7FFF_FFFF_0000` — grows downward

### Kernel Boot Sequence (`kernel/src/main.rs`)
1. Memory init (PMM, paging, heap)
2. ISTs + IDT + PIC
3. VirtIO block + VFS + Ext2 mount at disk `0xE0`
4. Window manager + event queue + task manager
5. Spawn `/sys/bin/init.wasm` as PID 1
6. Enter idle `hlt` loop; scheduler runs via PIT interrupt

### Syscall Interface (`kernel/src/interrupts/syscalls/`)
- Entry via `syscall`/`sysret` MSRs; handler is `syscall_entry` (naked asm)
- Dispatcher in `syscalls/mod.rs` routes to submodules: `fs`, `memory`, `process`, `window`, `network`, `event`, `misc`
- Syscall numbers mirror Linux ABI where possible; custom syscalls start at 100 (window) and 112 (threads)

### WASM Runtime (`std/src/wasm/`)
- **Interpreter** (`interpreter/`): stack-based executor with sidetable for branch resolution
- **AOT Compiler** (`aot/`): emits x86_64 machine code; maintains strict 16-byte stack alignment in trampolines
- **Component Model** (`component/`): parses and executes WASM Component Model binaries
- **WASI** (`wasi/`): Preview 1 (`preview1.rs`) and KrakeOS-native (`krakeos.rs`) host function implementations
- The `std` crate is used both by the kernel (as the allocator/runtime host) and by userland WASM apps

### VFS & Filesystem (`kernel/src/fs/`)
- `vfs.rs` — global file descriptor table (256 slots), dispatch to mounted `FileSystem` trait objects
- `ext2/` — read/write Ext2 implementation
- `pipe.rs` — IPC pipes
- File paths use `/path` syntax (defaults to 0xE0 disk)

### Window Manager (`kernel/src/window_manager/`)
- In-kernel compositor with alpha blending (SIMD/SSE-accelerated in `composer.rs`)
- Windows registered via `SYS_ADD_WINDOW` syscall; userland draws into a shared framebuffer region
- Input routed through `events.rs` event queue; userland polls with `SYS_GET_EVENTS`

### GUI Framework (`inkui/`)
- `no_std` widget library; apps use `Window` + `Widget` + `Event` types
- Fonts via `titanf`, SVG/PNG assets via `asvgard`
- Renders via software rasterization; compositor handles final blending

## Engineering Standards

- All kernel shared state must use `sync::Mutex` or `sync::Spinlock` from `kernel/src/sync.rs`
- Rust Nightly required; crates are `no_std` + `alloc`
- WASI target is `wasm32-wasip2`; prioritize WASI Preview 2 (0.2.x) compliance
- AOT trampolines and emitters must maintain 16-byte stack alignment

## Global Hotkeys (Window Manager)
- `Super+T` — Terminal
- `Super+P` — VMA/Memory dump
- `Super+X` — Kill focused window
- `Super+Z` — Maximize toggle
- `Super+C` — Resize mode
