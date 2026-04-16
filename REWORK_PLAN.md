# KrakeOS Rework Plan

**Version:** 1.0
**Date:** 2026-04-12
**Status:** PLANNING
**Progress file:** `REWORK_PROGRESS.md`

---

## Overview

KrakeOS is a bare-metal x86_64 OS that exclusively runs WASM files (via AOT compilation) plus native kernel binaries. The current system boots, runs a desktop with wallpaper, taskbar, and two terminal instances, but has several critical issues listed below. This document is a comprehensive rework plan meant to be handed off between AI agents.


## Section 2 — Kernel Lockup Prevention (Interrupt Starvation)

**Clarification:** The goal here is NOT to remove kernel locking primitives. `RwLock`, `Mutex`, `SpinLock`, `YieldMutex` are all fine and should stay. The problem is **kernel lockups** — situations where a lock is held long enough (or in the wrong context) to starve the interrupt controller, freeze the scheduler, and halt the system entirely. Under SMP this is significantly worse because one CPU spinning with interrupts disabled can cause cross-CPU IPI stalls.

### Current Problem Patterns

**1. Spinlocks held across I/O waits**
Spinlocks (`Mutex<T>` = busy-wait) must never be held while waiting for VirtIO responses, disk I/O, or any operation with unbounded latency. Any such hold starves all other CPUs and ISRs polling that lock.

Known offender: `DISPLAY_SERVER locked` in the log appears *during* `ds.copy()` (a VirtIO GPU transfer). This is a spinlock held across a hardware I/O fence — instant lockup risk.

**2. Locks held with interrupts disabled**
If interrupt-disable (`cli`) wraps a critical section that calls into another subsystem that also takes a lock, the result is a deadlock that cannot be broken by the timer ISR.

**3. No lock ordering — nested lock acquisition**
Two subsystems acquiring each other's locks in different orders = classic deadlock. Currently there is no documented or enforced ordering.

**4. SMP: spinlocks not CPU-aware**
A spinlock acquired on CPU 0 that is never released (e.g., because the holder task was preempted and rescheduled away) will spin CPU 1 forever. Spinlocks need to save/restore interrupt state (`pushf`/`cli`/`popf`) and must not be held across a reschedule point.

### 2.1 Interrupt-Safe Spinlock Usage

The rule: **a spinlock must only be held for the duration of a memory operation — never across I/O, syscalls, or blocking.**

For each problematic site, fix the hold duration:

| Subsystem | Offending Hold | Fix |
|-----------|---------------|-----|
| Composer display flush | Spinlock held across `virtio_gpu.copy()` (VirtIO I/O) | Release lock before issuing VirtIO command; re-acquire to update state after |
| VirtIO GPU command send | Atomic bool spin while waiting for GPU ACK | Move wait-for-ACK outside the critical section |
| Ext2 write path | `YieldMutex` held across entire multi-MB block allocation + disk write | Hold only for inode/bitmap mutation; release during DMA transfer |
| AOT worker | Semaphore + VecDeque unprotected on SMP | Protect deque with a proper `Mutex` (brief hold only) |
| PMM frame allocator | Unknown — if spinlock, must not be held across page table walks | Audit `kernel/src/memory/pmm.rs`, ensure hold < 10 instructions |

**Files to audit:**
- `kernel/src/drivers/video/virtio/mod.rs` — `ds.copy()` lock hold
- `kernel/src/fs/ext2/fs.rs` — write path lock scope
- `kernel/src/task/aot_worker.rs` — queue lock
- `kernel/src/memory/pmm.rs` — allocator lock

### 2.2 Interrupt-State Discipline

Establish rules for every lock type:

```
SpinLock (busy-wait Mutex):
  - MUST save and disable interrupts on acquire (pushf + cli)
  - MUST restore interrupt state on release (popf)
  - MUST NOT be held across any blocking operation
  - Max hold time: ~100 cycles (simple struct read/write)

YieldMutex:
  - MUST be used when hold time may be > 1µs
  - MUST NOT be used in interrupt handlers (cannot yield from ISR)
  - Safe across short I/O waits IF interrupts remain enabled

RwLock / RwSpinlock:
  - Read locks: safe to hold across reads from cached memory
  - Write locks: same rules as SpinLock
  - Under SMP: readers on different CPUs are fine; writer must not starve readers
```

Add these rules as a doc comment at the top of `kernel/src/sync.rs`.

### 2.3 Global Lock Ordering (Deadlock Prevention)

Any code path that acquires multiple locks MUST acquire them in this order. Never acquire a lower-numbered lock while holding a higher-numbered one:

```
1. PMM frame allocator          (lowest — never nests)
2. VMM / page table lock
3. Task manager / scheduler
4. Filesystem (Ext2 superblock)
5. VirtIO block queue
6. VirtIO GPU queue
7. Compositor window list
8. Event queues                 (highest — brief, innermost)
```

Document this ordering in `kernel/src/sync.rs`. Add a `#[cfg(debug_assertions)]` lock-order checker if feasible.

### 2.4 SMP-Safe Lock Corrections

With multiple cores active, the following additional issues arise:

- **Per-CPU interrupt vectors:** Each AP must have its own LAPIC timer interrupt. Currently only BSP timer fires. APs with no timer never preempt, meaning a spinning task on AP1 runs forever if AP0 holds its lock.
- **IPI for cross-CPU wakeup:** When a `YieldMutex` is released on CPU0 and a waiter is on CPU1, CPU1 must be woken via IPI. Add `send_ipi_to(cpu_id, WAKE_VECTOR)` to `kernel/src/arch/x86_64/apic.rs`.
- **TLB shootdown:** When page tables are modified on one CPU, other CPUs must flush their TLBs. Add `tlb_shootdown_all()` using broadcast IPI to `kernel/src/memory/vmm.rs`.

### 2.5 Userspace Lock Discipline

WASM processes use `std/src/sync.rs` spinlocks. These are fine, but:

- After ~1000 spin iterations with no progress, call `SYS_YIELD` to give up the CPU
- This prevents a spinning WASM task from monopolizing a CPU core while the lock holder (on another task) can't run
- **Do not replace the spinlock type** — just add a yield backoff after N iterations

**Files:** `std/src/sync.rs` (add spin-then-yield loop), `kernel/src/syscalls/misc.rs` (ensure `SYS_YIELD` works correctly under SMP)

---

## Section 3 — File Read Optimization

### Current State
- Ext2 has a BTreeMap-based sector cache (max 8192 sectors = 4 MB at 512B/sector)
- Cache is checked on every read, but the BTreeMap lookup is O(log n)
- Large reads loop over sectors individually (visible: `Ext2Node::read of 3466762 bytes took 0 ticks over 16 loops`)
- Write path does read-modify-write per sector (slow for large writes)
- No readahead / prefetch

### 3.1 Cache Replacement

**Target:** Replace `BTreeMap<u64, [u8; 512]>` with a fixed-size hash-based cache.

- Use a direct-mapped or 4-way set-associative cache indexed by `lba % CACHE_SIZE`
- Cache line size: 4096 bytes (8 sectors) to match ext2 block size
- This reduces ext2 block reads from 8 sector reads to 1 cache line fetch
- Eviction: LRU within each set using a generation counter

**Files:** `kernel/src/fs/ext2/fs.rs`, `kernel/src/fs/cache.rs`

### 3.2 Large Read Optimization

- For contiguous block reads > 4KB, issue a single DMA transfer instead of per-sector reads
- Add `read_blocks_bulk(start_lba, count, buf)` to VirtIO block driver
- Ext2 `read_inode_data` should detect contiguous extent ranges and use bulk path

**Files:** `kernel/src/fs/virtio.rs`, `kernel/src/fs/ext2/fs.rs`

### 3.3 Write Optimization

- Batch small writes into a write buffer, flush asynchronously
- Journal (or at minimum: ordered writes) to prevent corruption
- `dirty_sectors` BTreeSet → dirty bitmap for O(1) membership check

### 3.4 WACC Cache Benefit

The current WACC (compiled AOT cache) saves ~7-8 MB per app. On subsequent boots:
- Cache hit path reads 8 MB in `256 ticks over 34 loops` — already fast
- Optimize: store WACC in a dedicated partition or fixed inode range for O(1) lookup

---

## Section 4 — SMP Implementation (No Kernel Locks)

### Current State
- `kernel/src/arch/x86_64/smp.rs` has AP bootstrap trampoline
- QEMU runs with 4 cores (`-smp 4` in Makefile)
- APs are started but scheduler does not distribute tasks across cores
- No per-CPU run queues — all tasks on a single global queue
- Timer interrupt on BSP only

### 4.1 Per-CPU Run Queues

**Target:** Each CPU has its own run queue (VecDeque or circular buffer).

```rust
// kernel/src/task/cpu.rs
pub struct CpuState {
    pub apic_id: u32,
    pub tss: Tss,
    pub run_queue: SpinLock<VecDeque<TaskId>>,  // local queue
    pub current_task: Option<TaskId>,
    pub idle_task: TaskId,
}
```

- New tasks assigned to least-loaded CPU (steal-on-idle allowed)
- Each AP runs its own timer interrupt → local LAPIC timer, not PIT
- Work-stealing: if a CPU's queue is empty, steal from the longest queue

**Files:** `kernel/src/task/cpu.rs`, `kernel/src/task/scheduler.rs`, `kernel/src/task/manager.rs`

### 4.2 AP Initialization Sequence

Current AP startup reaches long mode but does not fully initialize per-CPU state. Complete:

- [ ] Per-AP GDT with proper TSS (already partially done with `AP_GDT_BASE`)
- [ ] Per-AP IDT setup (currently shared, causes race on IDT load)
- [ ] Per-AP LAPIC timer calibration
- [ ] Per-AP syscall MSR setup (`STAR`, `LSTAR`, `SFMASK`)
- [ ] Per-AP stack (currently risk of stack collision)
- [ ] Per-AP `GS_BASE` pointing to its `CpuState`

**Files:** `kernel/src/arch/x86_64/smp.rs`, `kernel/src/arch/x86_64/apic.rs`

### 4.3 Lock-Free Critical Paths

- PMM: Use per-CPU free lists. Refill from global pool only when empty (reduces lock contention to ~1% of allocations)
- Task spawn: CAS-based task slot allocation instead of global mutex
- Event delivery: Lock-free SPSC ring buffer per process (already semi-present in event_manager)

### 4.4 AOT Worker on Dedicated Core

- Pin AOT compilation to CPU 3 (last core) so it does not interfere with interactive tasks
- Use `LAPIC IPI` to notify BSP when compilation finishes instead of polling

---

## Section 5 — WASM AOT Compiler Optimization

### Current State
- AOT output: ~7-8 MB per app (init, term, taskbar all ~7.5 MB)
- Compilation artifacts: `*.wacc` cached on disk
- Code segment: `code_len=7979405` (7.6 MB) for term.wasm
- No dead code elimination at AOT level
- No instruction fusion / peephole optimization

### 5.1 Binary Size Reduction

**Target:** Reduce AOT output by 40-60%.

- [ ] **Dead code elimination:** Track which WASM functions are reachable from exports+start. Skip unreachable functions.
- [ ] **Constant folding:** At AOT time, fold `i32.const X` followed by arithmetic into immediate instructions
- [ ] **Redundant load elimination:** If the same local/global is loaded twice without a store in between, reuse the register
- [ ] **Jump threading:** Replace indirect branches that always go to the same target with direct jumps
- [ ] **Tail call optimization:** Replace `call` + `return` sequences with `jmp`

**Files:** `std/src/wasm/aot/compiler.rs`, `std/src/wasm/aot/emitter.rs`

### 5.2 Instruction Quality

- Use `CMOV` (conditional move) instead of conditional jumps for simple select operations
- Use `LEA` for address arithmetic instead of `ADD`/`MUL` sequences
- Prefer `XOR reg, reg` over `MOV reg, 0` for zeroing
- Use 8-bit/16-bit immediate forms when values fit
- Use `TEST reg, reg` instead of `CMP reg, 0`

### 5.3 SIMD Exploitation

- Detect memory copy/fill operations and emit `VMOVDQU` / `REP STOSD` where appropriate
- For float arrays, use SSE2/AVX2 vectorized operations if the WASM module uses f32x4

### 5.4 Preserve Ring3-RT Structure

The ring3-rt blob is linked into each AOT module. **Do not change the ABI or trampoline layout.** Only improve the code the compiler emits for the WASM body instructions themselves. The jump table at index 0-1023 in `ring3-rt/src/lib.rs` must remain stable.

---

## Section 6 — Device Recognition and Driver Management

### Current State
- PCI scan: brute-force by vendor/device ID (`pci.rs::find_device`)
- No device registry or driver table
- No ACPI device namespace parsing
- Drivers are hardcoded, not dynamically loaded
- No USB controller (xHCI) implementation

### 6.1 Device Registry

Create a global device registry populated at boot:

```rust
// kernel/src/drivers/registry.rs
pub enum DeviceClass {
    DiskController,
    UsbController,
    NetworkCard,
    DisplayAdapter,
    InputDevice,
    AudioDevice,
    Unknown,
}

pub struct DeviceEntry {
    pub class: DeviceClass,
    pub vendor_id: u32,
    pub device_id: u32,
    pub bus: u8, pub device: u8, pub function: u8,
    pub driver: Option<DriverHandle>,
    pub description: &'static str,
}

pub static DEVICE_TABLE: Mutex<Vec<DeviceEntry>> = ...;
```

### 6.2 Boot Enumeration Sequence

At kernel init, after ACPI tables are read:

1. **Parse ACPI DSDT/SSDT** — find device nodes, extract `_HID`, `_CID`, `_ADR`
2. **Enumerate PCIe** — scan all buses/devices/functions, classify by PCI class code
3. **Enumerate legacy PIC devices** — PS/2, RTC, HPET
4. For each found device:
   - Look up class code in driver dispatch table
   - Instantiate driver if available
   - Register in `DEVICE_TABLE` with category

**PCI class codes to handle:**

| Class | Subclass | Device Type |
|-------|----------|-------------|
| 0x01 | 0x06 | SATA / AHCI controller |
| 0x01 | 0x08 | NVMe controller |
| 0x0C | 0x03 | USB xHCI controller |
| 0x0C | 0x00 | USB UHCI controller |
| 0x02 | 0x00 | Ethernet controller |
| 0x03 | 0x00 | Display adapter |
| 0x04 | 0x01 | Audio device |

### 6.3 Category-Based Syscall Dispatch

When a syscall needs a device (e.g., `SYS_READ` on a disk FD), instead of hardcoding VirtIO block:

```rust
fn get_disk_device() -> Option<&'static dyn BlockDevice> {
    DEVICE_TABLE.lock()
        .iter()
        .find(|d| d.class == DeviceClass::DiskController && d.driver.is_some())
        .and_then(|d| d.driver.as_ref())
        .map(|h| h.as_block_device())
}
```

This allows transparent fallback: if VirtIO block is unavailable, try AHCI, then NVMe.

**Files to create:** `kernel/src/drivers/registry.rs`
**Files to modify:** `kernel/src/syscalls/fs.rs`, `kernel/src/drivers/mod.rs`

---

## Section 7 — xHCI USB Driver + Device Enumeration

### 7.1 xHCI Controller Driver

**File to create:** `kernel/src/drivers/usb/xhci.rs`

**Implementation steps:**
1. Find xHCI PCI device (class 0x0C, subclass 0x03, prog-if 0x30)
2. Map MMIO BAR0 (capability registers, operational registers, runtime registers, doorbell registers)
3. Initialize host controller:
   - Reset controller (`USBCMD.HCRST`)
   - Set max device slots (`CONFIG.MaxSlotsEn`)
   - Set DCBAA (Device Context Base Address Array) pointer
   - Create command ring and event ring
   - Configure interrupter 0: event ring segment table, ERSTBA, ERDP
4. Enable MSI-X (see §8) for interrupt-driven operation
5. Start controller (`USBCMD.RS = 1`)
6. Enable port power, detect connections via Port Status Change events

### 7.2 USB Device Enumeration

For each connected port:
1. Issue `Enable Slot` command → get slot ID
2. Allocate Input Context, configure endpoint 0 (control)
3. Issue `Address Device` command
4. Read device descriptor (GET_DESCRIPTOR)
5. Read configuration descriptor
6. For each interface: check `bInterfaceClass`
   - Class 0x03 = HID → enumerate as input device
   - Class 0x08 = Mass Storage → enumerate as disk

Register each enumerated device in `DEVICE_TABLE`.

**Files to create:**
- `kernel/src/drivers/usb/mod.rs`
- `kernel/src/drivers/usb/xhci.rs`
- `kernel/src/drivers/usb/hid.rs`
- `kernel/src/drivers/usb/mass_storage.rs`

### 7.3 USB Disk Enumeration (under xHCI)

- Implement USB Mass Storage class (BBB protocol)
- Wrap as `BlockDevice` trait impl and register in `DEVICE_TABLE`
- Support SCSI READ(10) / WRITE(10) commands over bulk endpoints
- Enumerate as `/dev/usb0`, `/dev/usb1`, etc.

---

## Section 8 — USB Input (HID): Mouse, Keyboard, Tablet

### Priority Order
1. Try xHCI for USB HID devices
2. Fall back to PS/2 only if no USB HID found

### 8.1 USB HID Driver

**File to create:** `kernel/src/drivers/usb/hid.rs`

For each HID interface found during USB enumeration:
1. Read HID descriptor
2. Read Report descriptor → parse to determine report format
3. Set boot protocol (simpler, skip full HID report parsing for mouse/keyboard)
4. Configure interrupt IN endpoint
5. Enable MSI-X interrupt for this endpoint (see §8.3)

**Report types to handle:**

| Usage Page | Usage | Device |
|------------|-------|--------|
| 0x01 | 0x02 | Generic Mouse (relative) |
| 0x01 | 0x06 | Generic Keyboard |
| 0x0D | 0x01 | Digitizer Tablet (absolute coordinates) |

### 8.2 Input Device Abstraction

Create a unified input device trait:

```rust
// kernel/src/drivers/input/mod.rs
pub trait InputDevice: Send + Sync {
    fn read_event(&self) -> Option<InputEvent>;
    fn device_type(&self) -> InputDeviceType;
}

pub enum InputEvent {
    Key { keycode: u32, pressed: bool, modifiers: Modifiers },
    MouseRel { dx: i16, dy: i16, buttons: u8 },
    MouseAbs { x: u16, y: u16, buttons: u8 },  // tablet / VMware absolute
}
```

The existing PS/2 keyboard and mouse drivers implement this trait. The window manager input handler calls into this abstraction, not directly into PS/2 code.

### 8.3 Interrupt-Driven USB (MSI-X)

**No polling.** USB input must use interrupts.

Steps:
1. In xHCI init: find MSI-X capability in PCI config space
2. Allocate MSI-X vectors from the IDT free vector range (e.g., 0x40-0x6F)
3. For each active endpoint, program MSI-X table entry: address = LAPIC MSI address, data = vector number
4. In IDT handler for that vector: read xHCI event ring, dispatch to HID driver
5. Write ERDP to acknowledge

**Files to modify:** `kernel/src/drivers/pci.rs` (MSI-X helpers), `kernel/src/arch/x86_64/idt.rs` (dynamic vector allocation)

---

## Section 9 — Tiling Window Manager Optimization

### Current State
- BSP tree with 16 windows max, 5 workspaces
- Layout recomputed on every window add/remove
- Composer holds display lock during entire recompose
- Font rendering (glyph drawing) may be causing partial redraws

### 9.1 Fix Tiling Bugs (Immediate)

**Known issue:** When 3+ windows exist, tiling produces incorrect sizes (see `RESIZING 4 AT 902 X 533 at 112,33` then immediately `RESIZING 4 AT 497 X 533 at 517,33` — two resize events for the same window, second one correct).

- [ ] Tiling should compute layout ONCE and send ONE resize event per window
- [ ] Avoid double-resize by computing final layout before sending any events

**Files:** `kernel/src/window_manager/composer.rs` — `layout_tree()`, `add_window()`

### 9.2 Damage Tracking

Currently `recompose_except` redraws the ENTIRE screen. Implement damage rects:

```rust
pub struct DamageList {
    rects: [Rect; 16],
    count: usize,
}
```

Only blit dirty regions to the VirtIO GPU. This dramatically reduces GPU transfer size for small updates (e.g., cursor move, text typed in one terminal).

### 9.3 Decoupled Render Loop

The composer currently runs synchronously in syscall handlers (`handle_update_window` calls `ds.copy()` inline). This holds the display lock during VirtIO I/O.

**Target:** Decouple rendering:
1. `handle_update_window` marks the window framebuffer as dirty, returns immediately
2. A dedicated render task (on CPU 1) processes the dirty list and flushes to GPU
3. The render task wakes on a condition variable signaled by `handle_update_window`

This eliminates the `DISPLAY_SERVER locked` bottleneck visible in the log.

### 9.4 No New Locks

Every change to the window manager must be reviewed to ensure it introduces zero new spinlocks. Use:
- Per-window dirty flags (atomic bool)
- Lock-free damage ring buffer
- Single-writer-multiple-reader for the window list (SeqLock)

---

## Section 10 — VirtIO Driver Improvements

### 10.1 VirtIO GPU

- [ ] Implement `VIRTIO_GPU_CMD_GET_EDID` to query monitor capabilities
- [ ] Use scatter-gather for framebuffer transfers (avoid large contiguous allocation)
- [ ] Implement double-buffering properly: resource 1 = front, resource 2 = back, flip on vsync
- [ ] Remove atomic bool spin-lock on command submission, replace with proper descriptor ring management
- [ ] Support `VIRTIO_GPU_FLAG_FENCE` for synchronization instead of busy-wait

### 10.2 VirtIO Block

- [ ] Implement scatter-gather I/O (multiple descriptors per request) for large transfers
- [ ] Add request merging: consecutive sector reads from same task merged into one VirtIO request
- [ ] Implement `VIRTIO_BLK_T_FLUSH` for write barrier support
- [ ] Per-request completion callback instead of polling

### 10.3 VirtIO Network

- [ ] Currently minimal — add interrupt-driven RX (not polling)
- [ ] Implement TX/RX ring management properly with descriptor recycling
- [ ] Support `VIRTIO_NET_F_CSUM` for hardware checksum offload

### 10.4 Generic VirtIO Improvements

- [ ] Create a shared `VirtQueue` abstraction in `kernel/src/drivers/virtio/queue.rs` (currently each driver duplicates queue logic)
- [ ] Support MSI-X per-queue interrupts (not just pin-based INTx)

---

## Execution Order

Execute sections in this order to minimize regressions:

```
Phase 1 (Unblock development):
  §1 — Fix terminal + Super+Enter bug

Phase 2 (Stability):
  §2 — Lock audit and resolution
  §3 — File read optimization

Phase 3 (SMP):
  §4 — SMP per-CPU queues + AP init

Phase 4 (Compiler):
  §5 — AOT compiler optimization

Phase 5 (Device infrastructure):
  §6 — Device registry
  §7 — xHCI + USB enumeration
  §8 — USB HID input + MSI-X

Phase 6 (Polish):
  §9 — Tiling WM optimization
  §10 — VirtIO driver improvements
```

---

## Key Invariants (Never Break These)

1. **Ring3-rt trampoline ABI is frozen.** Indices 0-1023 in the jump table must not change.
2. **No SYS_WASM_HOST_CALL.** All host functions go through ring3-rt.
3. **`make run` is the only way to launch KrakeOS.** Never construct manual QEMU commands.
4. **WASM-only userland.** No native ELF processes in ring 3. Everything is AOT-compiled WASM.
5. **Lock ordering must be respected** (see §2.3) to prevent deadlocks.

---

## Files Index (Critical Paths)

| Purpose | File |
|---------|------|
| Kernel entry | `kernel/src/main.rs` |
| Sync primitives | `kernel/src/sync.rs` |
| SMP bootstrap | `kernel/src/arch/x86_64/smp.rs` |
| APIC | `kernel/src/arch/x86_64/apic.rs` |
| PCI scan | `kernel/src/drivers/pci.rs` |
| VirtIO GPU | `kernel/src/drivers/video/virtio/mod.rs` |
| VirtIO Block | `kernel/src/fs/virtio.rs` |
| Ext2 | `kernel/src/fs/ext2/fs.rs` |
| Tiling WM / Composer | `kernel/src/window_manager/composer.rs` |
| Input handler | `kernel/src/window_manager/input.rs` |
| Task manager | `kernel/src/task/manager.rs` |
| Scheduler | `kernel/src/task/scheduler.rs` |
| AOT worker | `kernel/src/task/aot_worker.rs` |
| Syscall dispatch | `kernel/src/syscalls/mod.rs` |
| Ring3-rt jump table | `ring3-rt/src/lib.rs` |
| AOT compiler | `std/src/wasm/aot/compiler.rs` |
| AOT emitter | `std/src/wasm/aot/emitter.rs` |
| Terminal app | `apps/term/src/main.rs` |
| Terminal buffer | `apps/term/src/buffer.rs` |
| Shell | `apps/shell/src/main.rs` |
