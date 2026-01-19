<div align="center">
<br>
<br>

<img src="icon/squid.svg" alt="KrakeOS Logo" width="180" height="180" />

# 🦑 KrakeOS

**Custom 64-bit operating system written in Rust**

[![Rust](https://img.shields.io/badge/Language-Rust_Nightly-b7410e.svg?style=for-the-badge&logo=rust)](https://www.rust-lang.org/)
[![Platform](https://img.shields.io/badge/Platform-x86__64-blue.svg?style=for-the-badge&logo=intel)](https://en.wikipedia.org/wiki/X86-64)
[![QEMU](https://img.shields.io/badge/Emulation-QEMU_VirtIO-ff7e00.svg?style=for-the-badge&logo=qemu)](https://www.qemu.org/)
[![License](https://img.shields.io/badge/License-MIT-green.svg?style=for-the-badge)](https://opensource.org/licenses/MIT)

<br>

<img src="icon/screenshot1.png" alt="KrakeOS Screenshot" style="border: 1px solid #9070FF;" width="640" height="360"/>

<br>

*A hobby OS with GUI, custom bootloader, and yes, it runs DOOM*

</div>

---

## Overview

KrakeOS is a hobby operating system written in Rust. It includes a custom bootloader, basic multitasking, memory
management, hardware drivers, a filesystem, and a graphical window manager. This is a learning project that explores OS
development from the ground up.

## Features

### Core Kernel & Architecture

- x86_64 (Long Mode)
- Custom multi-stage bootloader (Swiftboot)
- Preemptive multitasking with round-robin scheduling
- User mode (Ring 3) support
- Around 25 system calls using `syscall`/`sysret`
- Context switching with FPU/SSE state save/restore

### Memory Management

- 4-level paging (PML4)
- Virtual memory manager
- Physical frame allocator
- Separate heap allocators for kernel and userspace
- Page-level memory protection and NX bit support

### Drivers

**Graphics:**

- VirtIO GPU with hardware-accelerated 2D and hardware cursor
- VBE framebuffer fallback

**Storage:**

- VirtIO Block device
- ATA/IDE (PIO mode)
- DMA support (PIIX4 Bus Mastering)

**Input:**

- PS/2 Keyboard with modifier keys
- PS/2 Mouse with scroll wheel support

**Other:**

- PCI enumeration and configuration
- PIT for scheduling
- RTC for timekeeping

### Filesystem

- Ext2 read/write support
- Virtual filesystem (VFS) layer
- Anonymous pipes for IPC
- ELF loader for 64-bit PIE executables

### Window Manager

- In-kernel compositing window manager
- Z-ordering, alpha blending, and transparency
- SSE-optimized blitting
- Window dragging, resizing, and focus management
- Dirty rectangle tracking and double buffering

### Userland

- Custom Rust standard library and C library (krake_libc)
- Interactive shell with pipes
- Terminal emulator with ANSI escape codes
- InkUI widget library (buttons, windows, labels, layouts)
- Apps: taskbar, cat, texture mapper
- DOOM (via doomgeneric)

## Building & Running

```bash
# You'll need: Rust nightly, QEMU, and build tools

# Build
make build

# Run
make run
```

## Why?

This is a learning project. The goal is to understand how operating systems work by building one from scratch. It's not
meant to be production-ready or particularly practical—just a fun way to learn about kernels, drivers, and low-level
programming.

## Contributing

Feel free to open issues or PRs if you spot something interesting.

## License

MIT License

---

<div align="center">

</div>