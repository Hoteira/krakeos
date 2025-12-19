# KrakeOS

KrakeOS is a custom, from-scratch 64-bit operating system written in Rust. It is designed with a philosophy of educational transparency and extreme ownership of the stack, avoiding external dependencies for core logic whenever possible.

## Core Philosophy

KrakeOS is built on the principle that the best way to understand a system is to build it. Every layer—from the bootloader to the DEFLATE decompression engine—is implemented with a focus on simplicity, idiomatic Rust, and performance.

## Key Features & Architecture

### 1. Memory Management
KrakeOS implements a flat memory model with identity mapping.
- **Physical Memory Manager (PMM)**: Uses a bitmap/list-based allocation strategy to track used and free physical frames.
- **Dynamic Heap Allocator**: A sophisticated userland allocator that supports:
    - **Automatic Extension**: When the heap is exhausted, it automatically requests more memory from the kernel.
    - **Relocation Support**: If the kernel cannot extend the heap in-place, the allocator can relocate itself to a new memory region, automatically fixing up all internal pointers (free lists and bins) to remain valid in the new address space.
    - **Binning**: Optimized allocation for common small sizes to reduce fragmentation.

### 2. Graphics Subsystem
- **VirtIO GPU Driver**: A modern GPU driver that supports high-resolution displays and hardware-accelerated features (where available via QEMU/virgl).
- **Resolution Detection**: The driver dynamically queries the GPU for the preferred display resolution instead of relying on hardcoded values.
- **Double Buffering & Page Flipping**: Implements smooth rendering by drawing to a back buffer and performing an atomic swap (flip) to the front buffer during the vertical blanking interval.

### 3. InkUI (User Interface Framework)
A custom UI library designed for KrakeOS.
- **Widget Hierarchy**: Supports frames, buttons, labels, and images.
- **Flexible Layout**: Implements a layout engine with support for relative/absolute positioning and automatic size calculation.
- **Image Rasterization**: Integration with a custom image loader that supports PNG decoding.
- **Alpha Enforcement**: Robust rendering logic that ensures correct alpha-blending or enforcement of opacity for critical UI elements like wallpapers.

### 4. Custom Implementations (No-Crate Policy)
To maximize learning and control, several complex algorithms are implemented from scratch:
- **DEFLATE/zlib Decompression**: Hand-written BitReader, Huffman tree construction, and LZ77 decompression.
- **PNG Parser**: Custom chunk traversal (IHDR, IDAT, PLTE, etc.) and unfiltering logic.
- **Ext2 File System**: A robust read-only (and expanding) implementation of the Ext2 filesystem.

## Building and Running

KrakeOS uses a custom toolchain and build script.

### Prerequisites
- Rust (Nightly channel)
- QEMU (with VirtIO support)
- WSL (for certain image manipulation tools like `objcopy` and `genext2fs`)

### To Run
Execute the provided `make.bat` in the root directory:
```batch
make.bat
```
This script will compile the bootloader, kernel, and userland, package them into a disk image, and launch QEMU with the appropriate VirtIO GPU flags.

## Project Structure
- `kernel/`: The core operating system logic (interrupts, drivers, memory management).
- `std/`: The standard library for KrakeOS applications (syscall wrappers, heap allocator).
- `inkui/`: The high-level UI framework.
- `userland/`: Example applications (like the Wallpaper app).
- `swiftboot/`: The multi-stage bootloader.
- `elfic/`: A custom ELF parsing library.
