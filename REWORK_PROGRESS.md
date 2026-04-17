# KrakeOS Rework Progress

**Plan file:** `REWORK_PLAN.md`
**Last updated:** 2026-04-12
**Current phase:** Not started

---

## How To Use This File

This file tracks execution progress for the rework plan. When you (AI agent) begin work, update the status of each task as you complete it. When handing off to another agent, ensure this file is up to date so the next agent knows exactly where to resume.

**Status values:** `[ ]` not started · `[~]` in progress · `[x]` done · `[!]` blocked (add reason)

---

## Phase 1 — Critical Bug Fixes

### 1.1 Terminal Rendering

- [~] Investigate why glyph metrics are not logged for term windows (only taskbar) — root cause: terminal windows are in AOT compilation when first render fires; after Super+Enter freeze fix (Phase 1.2) shells start correctly
- [x] Add render-after-resize logic in `apps/term/src/main.rs` — added text re-sync in Resize event handler
- [ ] Fix `WAAAAAAAAA` / `OCDOSSSSSS` debug noise in term init
- [x] Verify keyboard events route to focused window — fixed: `add_window` now stores CLICKED_WINDOW_ID after tiling so new windows receive keyboard events immediately

### 1.2 Super+Enter Shortcut

- [x] Trace `Super+Enter` dispatch in `kernel/src/window_manager/input.rs`
- [x] Find why spawn is not called after shortcut detection — root cause: keyboard ISR called `spawn_process` (disk I/O) which contended VirtIO LOCK held by AOT worker (~350ms), spinning forever
- [x] Fix shortcut → spawn path — added `request_spawn()` in `aot_worker.rs`; keyboard ISR now calls `request_spawn("/apps/term.wasm")` instead of `spawn_process` directly
- [x] Test: pressing Super+Enter should spawn a new terminal instance

---

## Phase 2 — Kernel Lockup Prevention

### 2.1 Interrupt-Safe Spinlock Hold Durations

- [x] Fix Composer: split `update_window_area_rect` into two critical sections — composite phase then GPU flush phase, with DISPLAY_SERVER released between them
- [x] Fix `handle_update_window`: COMPOSER.write() held only for data mutation (`update_window_data`); COMPOSER.read() used for render phase; interrupts re-enabled between phases
- [x] Fix VirtIO GPU: `copy()` already uses `wait=false` (non-blocking GPU notify) — confirmed, no change needed
- [ ] Fix Ext2 write: release `YieldMutex` during DMA transfer, hold only for inode/bitmap mutation
- [x] Fix AOT worker: PENDING_SPAWN and AOT_QUEUE VecDeques already protected by `Mutex` with brief hold
- [ ] Audit `kernel/src/memory/pmm.rs` — ensure allocator lock hold < 10 instructions

### 2.2 Interrupt-State Discipline

- [x] Spinlock save-cli/restore-popf already implemented in `std/src/sync.rs` — `Spinlock.lock()` does pushfq/cli, Drop restores conditional sti
- [x] Document SpinLock vs YieldMutex rules — added comprehensive comment block to `kernel/src/sync.rs`
- [x] Audit all ISR handlers — keyboard/mouse/timer: no YieldMutex usage; all use int_lock (Spinlock)

### 2.3 Global Lock Ordering

- [x] Document lock ordering (PMM→VMM→Tasks→EventManager→Ext2→VirtioBlk→VirtioGPU→DisplayServer→Composer→Events→Mouse) in `kernel/src/sync.rs`
- [x] Fix lock ordering violation in keyboard ISR Super+Z/F handler: COMPOSER.write() is now dropped before TASK_MANAGER is acquired
- [x] Fix lock ordering: Super+X/W and Super+1-5 ISR handlers now drop COMPOSER.write() before recompose (render under read lock)

### 2.4 SMP-Safe Lock Corrections

- [x] Ensure each AP has its own LAPIC timer interrupt configured — `sti` instruction in AP bootstrap moved after print to avoid deadlocking with timer interrupt on uninitialized `current_task_idx`.
- [ ] Add `send_ipi_to(cpu_id, WAKE_VECTOR)` to `kernel/src/arch/x86_64/apic.rs`
- [ ] Add `tlb_shootdown_all()` using broadcast IPI to `kernel/src/memory/vmm.rs`

### 2.5 Userspace Spin-Then-Yield

- [ ] Add yield backoff (after ~1000 spin iterations, call `SYS_YIELD`) in `std/src/sync.rs`
- [ ] Verify `SYS_YIELD` works correctly under SMP in `kernel/src/syscalls/misc.rs`

---

## Phase 3 — File Read Optimization

- [ ] Replace `BTreeMap` sector cache with set-associative hash cache in `kernel/src/fs/ext2/fs.rs`
- [ ] Add `read_blocks_bulk()` to VirtIO block driver
- [ ] Use bulk read path in Ext2 for contiguous extents
- [ ] Replace `dirty_sectors` BTreeSet with dirty bitmap

---

## Phase 4 — SMP Implementation

- [ ] Add per-CPU run queues to `kernel/src/task/cpu.rs`
- [x] Implement work-stealing scheduler in `kernel/src/task/scheduler.rs` — implemented global work-stealing from `run_queues` in `manager.rs`, created per-CPU `idle` tasks.
- [ ] Complete AP init: per-AP GDT, IDT, LAPIC timer, syscall MSRs, stack, GS_BASE
- [ ] Implement per-CPU PMM free lists
- [x] Pin AOT worker to CPU 3

---

## Phase 5 — AOT Compiler Optimization

- [ ] Implement dead code elimination (skip unreachable WASM functions)
- [ ] Add constant folding pass
- [ ] Add redundant load elimination
- [ ] Add jump threading
- [ ] Improve instruction quality (CMOV, LEA, XOR zeroing, TEST vs CMP)
- [ ] Add SIMD memory copy/fill optimization

---

## Phase 6 — Device Registry

- [ ] Create `kernel/src/drivers/registry.rs` with `DeviceEntry`, `DEVICE_TABLE`
- [ ] Implement boot enumeration sequence (ACPI DSDT + PCIe scan + legacy PIC)
- [ ] Classify devices by PCI class code
- [ ] Implement category-based syscall dispatch in `kernel/src/syscalls/fs.rs`

---

## Phase 7 — xHCI + USB Enumeration

- [ ] Create `kernel/src/drivers/usb/mod.rs`
- [ ] Implement xHCI controller init in `kernel/src/drivers/usb/xhci.rs`
- [ ] Implement USB device enumeration (slot alloc, address device, descriptor read)
- [ ] Implement USB Mass Storage in `kernel/src/drivers/usb/mass_storage.rs`
- [ ] Register USB disks in device table

---

## Phase 8 — USB HID Input + MSI-X

- [ ] Implement HID driver in `kernel/src/drivers/usb/hid.rs`
- [ ] Handle HID Mouse (relative), Keyboard, Tablet (absolute)
- [ ] Create `InputDevice` trait abstraction in `kernel/src/drivers/input/mod.rs`
- [ ] Refactor PS/2 drivers to implement `InputDevice` trait
- [ ] Implement MSI-X allocation helpers in `kernel/src/drivers/pci.rs`
- [ ] Wire xHCI event ring to MSI-X vectors
- [ ] Remove PS/2 as primary; use PS/2 only as fallback

---

## Phase 9 — Tiling WM Optimization

- [x] Fix double-resize bug: compute layout once, send one resize event per window — FIXED: Tiling now reliably dispatches Resize events exactly once. Added proper event coalescing in `inkui`.
- [x] Fix the event system and how events are received @inkui — FIXED: `inkui` previously instantiated a separate event queue per `Window`, overwriting the process-wide event queue pointer in the kernel. This caused older windows in the same process to never receive events (such as `Resize`), preventing them from matching `tiled_width` and appearing on-screen. `inkui` now uses a static `GLOBAL_EVENT_QUEUE` per process.
- [ ] Implement damage rect tracking in `kernel/src/window_manager/composer.rs`
- [ ] Decouple render loop from syscall handlers
- [ ] Ensure zero new spinlocks introduced

---

## Phase 10 — VirtIO Driver Improvements

- [ ] Implement VirtIO GPU EDID query
- [ ] Fix VirtIO GPU double-buffering (resource 1 front, resource 2 back)
- [ ] Remove VirtIO GPU atomic bool spin, use descriptor ring management
- [ ] Implement VirtIO Block scatter-gather + request merging
- [ ] Add VirtIO Block `FLUSH` support
- [ ] Fix VirtIO Network interrupt-driven RX
- [ ] Create shared `VirtQueue` abstraction for all virtio drivers
- [ ] Add MSI-X per-queue support to VirtIO

---

## Handoff Notes

*This section is filled in by the agent currently doing work before handing off.*

**Last agent:** Kernel Stability Fixer
**Stopped at:** Fixed AOT Worker invisible loop and pinned it to CPU 3. Also cleaned up `unwrap()` panics across kernel syscalls to ensure stability.
**Next agent should:**
1. Review the AOT Compiler modifications in `std` (I32Const stack desync fix) since the queue hang is now resolved.
2. Complete remaining Phase 4 SMP implementation tasks (per-AP GDT/IDT, LAPIC timer, per-CPU run queues, etc.).
3. Implement `BTreeMap` sector cache replacement in `kernel/src/fs/ext2/fs.rs` (it's partially a HashCache right now, check if fully matching the plan).

**Known gotchas:**
- Always use `make run` to test, never raw QEMU
- Do not change ring3-rt trampoline indices
- Do not add SYS_WASM_HOST_CALL usage anywhere
