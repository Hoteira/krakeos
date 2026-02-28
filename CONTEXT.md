# KrakeOS Context

KrakeOS is a WASM-native operating system designed to run WASM-WASI applications as first-class citizens, mirroring the philosophy of a Lisp machine but for the WebAssembly ecosystem. It primarily executes WASM modules (via interpreter or AOT) while maintaining support for occasional native `.elf` binaries.

## 🚀 Core Intent
- **WASM-Native:** A system where WASM is the primary execution format.
- **WASI-First:** Full support for WASI Preview 1 and Preview 2.
- **SAS Architecture:** Uses a **Single Address Space** for all processes, simplifying memory management and inter-process communication while maintaining isolation through the WASM runtime and VMA tracking.
- **Modern GUI:** Graphical interface managed by `inkui` (located in `inkui/`), a `no_std` widget-based library.

## 🏗️ System Architecture
### Memory Layout (SAS)
The system uses a 64-bit Single Address Space layout:
- **User Code:** Starts at `0x0000_0001_0000_0000` (4GB). Each process is allocated a 1GB region.
- **Shared Memory (SHM):** Starts at `0x0000_0040_0000_0000` (256GB).
- **Heap:** Starts at `0x0000_0080_0000_0000` (512GB). Each process is allocated a 4GB region.
- **Stack:** Starts at `0x0000_7FFF_FFFF_0000` (~112TB) and grows downwards.

### Execution Runtimes
- **WASM Interpreter:** Standard execution for WASM modules.
- **WASM AOT Compiler:** x86_64 Ahead-Of-Time compiler supporting:
  - **SIMD128** (FD_EXTENSIONS)
  - **Atomics** (ATOMIC_PREFIX)
  - **Multi-value returns**
  - **Bulk Memory Operations**
- **Native ELF:** Supports PIE (Position Independent Executable) ELFs.

### Kernel & Drivers
- **Monolithic Rust Kernel:** Cooperative and preemptive multitasking.
- **File System:** Ext2 support with VFS abstraction.
- **GUI Compositor:** Kernel-space window manager with alpha blending (SSE/SIMD accelerated).
- **GPU:** VirtIO-GPU driver with hardware cursor support.
- **Networking:** VirtIO-Net driver with a custom stack (ARP, IPv4, ICMP, UDP) and syscall-based socket API.

## 🎨 GUI & `inkui`
The userland GUI is built using the `inkui` library:
- **Widget System:** Supports `Frame`, `Button`, `Label`, `TextInput`, `Canvas`, and `Image`.
- **Layouts:** `Flex` and `Grid` display models.
- **Rendering:** Software-based rasterization with alpha blending support.
- **Dependencies:** Uses `titanf` for TrueType font rendering and `asvgard` for image/SVG loading.

## 🛠️ Build & Development
### Toolchain Requirements
- Rust (Nightly)
- `wasm32-wasip2` target for WASM modules.
- `genext2fs`, `dd`, `objcopy` (available via `make.bat`).

### Build Process
Use `make.bat` to build the entire system:
- Compiles the bootloader (`swiftboot`), kernel, and userland apps.
- Generates an Ext2 disk image.
- Packages everything into a bootable `krakeos.img`.

### Testing
- Test individual components using `cargo check` to avoid full rebuilds.
- Run in QEMU: `make.bat run`.

## ⌨️ Global Keybindings (Kernel Window Manager)
- **Super + T**: Open Terminal.
- **Super + P**: Dump memory/VMA state.
- **Super + X**: Kill active window.
- **Super + Z**: Maximize/Restore window.
- **Super + C**: Toggle Resize mode.
