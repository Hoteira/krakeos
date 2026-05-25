<div align="center">
  <img src="img/icon.svg" alt="KrakeOS Logo" width="120" height="120" />

# KrakeOS

**A 64-bit Microkernel-ish Operating System written in Rust**

[![Rust](https://img.shields.io/badge/Language-Rust_Nightly-b7410e.svg?style=for-the-badge&logo=rust)](https://www.rust-lang.org/)
[![Platform](https://img.shields.io/badge/Platform-x86__64-blue.svg?style=for-the-badge&logo=intel)](https://en.wikipedia.org/wiki/X86-64)
[![WASM](https://img.shields.io/badge/Runtime-WASM_Component_Model-654ff0.svg?style=for-the-badge&logo=webassembly)](https://webassembly.org/)
[![License](https://img.shields.io/badge/License-MIT-green.svg?style=for-the-badge)](LICENSE)

</div>

<br>

> [!IMPORTANT]
> This has been a project of mine for quite some time where EVERYTHING was written by me. In the last months I've decided to create a fork which will start using external crates to lessen the burden (especially drivers). I'll keep this original version public for research purposes.

## Overview

**KrakeOS** is a modern operating system kernel built from scratch using Rust. Designed for OS development research, it features a custom microkernel architecture, a native WebAssembly (WASM) Component Model runtime, and a hardware-accelerated compositing window manager.

<div align="center">
  <img src="icon/screenshot1.png" alt="KrakeOS Desktop Screenshot" style="border: 1px solid #9070FF; border-radius: 8px;" width="640" height="360"/>
</div>

<br>

## Key Features

### Core Architecture
- **Microkernel Design:** Minimalist kernel focusing on IPC and task scheduling.
- **Safe Systems Programming:** Leveraging Rust's ownership model for memory safety.
- **x86_64 Support:** Optimized for modern 64-bit Intel/AMD architectures targeting Long Mode.

### Graphics & Windowing
- **Compositing Window Manager:** Alpha blending, z-ordering, and SIMD-optimized (SSE/AVX) blitting.
- **VirtIO GPU Driver:** Hardware-accelerated 2D graphics support.
- **InkUI:** A bespoke, lightweight GUI widget library written in Rust.

### WebAssembly Runtime
- **WASI Support:** Built-in WASI Preview 1 & 2 compliant runtime.
- **Component Model:** Native implementation allowing for modular and language-agnostic applications.
- **Isolation:** Strong security boundaries between system services.

### Filesystem & Drivers
- **Ext2 Support:** Complete Read/Write support with VFS abstraction.
- **Hardware Drivers:** VirtIO Block, PS/2 Keyboard/Mouse, and PCI enumeration.

## Userland

- **Shell:** Interactive CLI supporting pipes, logical operators, and environment variables.
- **Native Apps:** Terminal emulator (Term), System Monitor (Sysmon), and even a port of DOOM.

## Building & Running

Requires Rust Nightly, QEMU, and LLVM tools.

```bash
# Build the kernel and disk image
make build

# Launch in QEMU
make run
```

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE-AGPL) file for details.
