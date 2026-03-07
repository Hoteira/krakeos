# KrakeOS Transformation Plan — Incremental Build-and-Run

Every step compiles and boots. Each step builds on the previous.
Format: **what changes -> build -> verify it still runs**.

---

## Current State (Baseline)

- Single Address Space already in practice (all processes share `KERNEL_PML4`, no CR3 switching)
- Hybrid: ELF init (`user.elf`) spawns WASM apps via `std::wasm::run()`
- WASM runs both interpreted and AOT-compiled inside kernel threads
- `pml4_phys` on `Process` is dead code -- set to `KERNEL_PML4`, never switched
- WASI Preview 1 + partial Preview 2 (~30 functions missing, see Appendix B)
- Component model: parsing only, no canonical ABI
- Address layout needs rework: old model had separate heap/stack/SHM regions, new model uses linear memory + code + stack per slot, SHM mapped into linear memory
- `method_export!` macro forces all functions to be `pub unsafe fn` -- callers littered with unnecessary `unsafe` blocks

---

## [DONE] Phase 1: Clean Up Dead Code, Safety & Formalize SAS (Steps 1-6)

### Step 1 -- Make `method_export!` generate safe functions
**Files:** `std/src/lib.rs`, `std/src/os/krakeos/mod.rs`, all call sites
**Changes:**
- Rewrite the `method_export!` macro so it always produces `pub fn` (see Appendix D for exact macro)
- The `unsafe extern "C"` (WASM import) and `unsafe { syscall(...) }` (native body) are hidden inside the generated function
- Change all 22 function declarations in `std/src/os/krakeos/mod.rs` from `pub unsafe fn` to `pub fn`
- Remove every `unsafe { ... }` wrapper at ~30+ call sites across `mod.rs` helpers, apps, inkui
- Delete the dead stubs `net_send` (returns -1) and `net_recv` (returns 0)

**Verify:** `cargo check --package=std`. Full build + boot. All apps still run.

**Decisions:**
- The WASM extern path uses `unsafe` internally to call the raw import -- is there a way to mark WASM imports as safe in nightly Rust? (If yes, use `safe fn` in the extern block)

---

### [DONE] Step 2 -- Remove dead `pml4_phys` plumbing
**Files:** `kernel/src/interrupts/task.rs`, `kernel/src/memory/vmm.rs`
**Changes:**
- Remove `pml4_phys` field from `Process` struct
- Remove `pml4_phys` from `Process::new()` initialization
- Remove `pml4_phys` from `schedule()` return tuple (return only `(new_state, k_stack)`)
- Remove the unused `pml4_phys` variable in context switch callers
- Delete `map_user_memory_into_kernel()` (dead code in vmm.rs, never called)
- Fix `initial-cwd` in `preview2/mod.rs` line 184 -- currently aliased to `get_environment`, should return the CWD string

**Verify:** Build with `make.bat`. Boot in QEMU. Wallpaper, taskbar.wasm, aot_test.wasm, net_test.wasm all start.

**Decisions:**
- Delete `pml4_phys` outright or gate behind `#[cfg(feature = "per_process_paging")]`?

---

### [DONE] Step 3 -- Add SAS region constants and 4096-slot layout
**Files:** `kernel/src/memory/address_space.rs`
**Changes:**
- Replace the old separate heap/stack/code regions with 3-region slot model (see Appendix A):
  ```rust
  pub const MAX_SLOTS: u16 = 4096;

  // AOT code: 16 MiB per slot × 4096 = 64 GiB. Most WASM < 4 MiB native.
  pub const CODE_REGION_BASE: u64  = 0x0000_0001_0000_0000; // 4 GiB
  pub const CODE_SLOT_SIZE: u64    = 64 * 1024 * 1024; // 64 MiB
  // Code ends at: 4 GiB + 64 GiB = 68 GiB (0x11_0000_0000)

  // Stack: 2 MiB per slot × 4096 = 8 GiB. Guard pages on both sides.
  pub const STACK_REGION_BASE: u64 = 0x0000_0041_0000_0000; // 260 GiB (right after code)
  pub const STACK_SLOT_SIZE: u64   = 2 * 1024 * 1024; // 2 MiB
  // Stack ends at: 68 GiB + 8 GiB = 76 GiB (0x13_0000_0000)

  // Linear memory: 31 GiB per slot × 4096 = ~124 TiB. Everything WASM touches.
  pub const LINEAR_MEMORY_BASE: u64      = 0x0000_0043_0000_0000; // 268 GiB (after stack)
  pub const LINEAR_MEMORY_SLOT_SIZE: u64 = 31 * 1024 * 1024 * 1024; // 31 GiB
  // Linear ends at: 76 GiB + 124 TiB ≈ 124.07 TiB (fits in 128 TiB canonical)
  ```
- Remove `HEAP_REGION_BASE`, `HEAP_SLOT_SIZE`, `SHM_REGION_BASE`
- Replace `allocate_heap()` with `allocate_linear_memory(slot_id) -> u64`
- Keep `allocate_stack()` (native stack is separate from WASM linear memory)
- Keep `allocate_code()`
- Remove `allocate_shm()` — SHM is now mapped into processes' linear memory (see Appendix A)
- Replace the monotonic `AtomicU32` counter with a bitmap allocator:
  - `SLOT_BITMAP: Mutex<[u64; 64]>` — 4096 bits = 512 bytes, 1 = free, 0 = used
  - `allocate_slot() -> Option<u16>` scans for first set bit, clears it
  - `free_slot(id: u16)` sets the bit back
  - Returns `None` if all 4096 slots in use

**Why SHM is not a separate region:** Shared memory buffers are physical pages mapped into
multiple processes' linear memory at different offsets. WASM accesses them via normal
memory operations. No separate VA region needed.

**Verify:** `cargo check --package=kernel`. Full build + boot.

**Decisions:**
- 31 GiB per slot linear memory + 64 MiB code + 2 MiB stack. VA is free.
- 4096 slots: 256 GiB code + 8 GiB stacks + ~124 TiB linear memory. Fits in 128 TiB with ~3.7 TiB headroom.

---

### [DONE] Step 4 -- Add guard pages between process slots
**Files:** `kernel/src/memory/address_space.rs`, `kernel/src/memory/vmm.rs`, `kernel/src/interrupts/exceptions.rs`
**Changes:**
- After each slot allocation, leave unmapped guard pages between adjacent slots:
- Guard at end of each linear memory slot (catches WASM OOB access)
- Guard at both ends of each stack slot (catches stack overflow/underflow)
- Guard at end of each code slot
- In page fault handler, add a debug message identifying which slot's guard was hit:
  `"Guard page fault: slot {id}, region {linear_memory|stack|code}, CR2={addr}"`

**Verify:** Build + boot. Existing apps don't touch guard pages, no behavior change.

**Decisions:**
- Guard page size: 4 KiB (one page) or 64 KiB (one WASM page)?

---

### [DONE] Step 5 -- Add `slot_id` and container metadata to Process
**Files:** `kernel/src/interrupts/task.rs`
**Changes:**
- Add `slot_id: u16` field to `Process` (from the bitmap in Step 3)
- Add `parent_pid: Option<u64>` field (for future nested containers, `None` for now)
- Add `children: Vec<u64>` field (empty for now)
- Derive `linear_memory_base` from `slot_id`:
  `linear_memory_base = LINEAR_MEMORY_BASE + (slot_id as u64) * LINEAR_MEMORY_SLOT_SIZE`
- All existing behavior preserved -- just adds metadata

**Verify:** Build + boot. All apps run identically.

**Decisions:**
- Maximum slot count? (4096 from Step 3, vs current MAX_THREADS = 128)
- Slot IDs are reusable — freed on process exit, returned to freelist.

---

### [DONE] Step 6 -- Implement `Super+P` VMA/Memory dump
**Files:** `kernel/src/drivers/input/keyboard.rs`, `kernel/src/memory/vma.rs`
**Changes:**
- In keyboard interrupt handler, detect `Super + P` key combo
- When pressed, iterate `GLOBAL_VMA` and print all regions to serial:
  ```
  [SAS DUMP]
  Slot 0 (PID 1): Code 0x100000000..0x103FFFFFF  LinMem 0x1000000000..0x13FFFFFFFF (2 MiB mapped)  Stack 0x7FFFFFFDF000..0x7FFFFFFFFFFF
  Slot 1 (PID 3): Code 0x104000000..0x107FFFFFF  LinMem 0x1400000000..0x17FFFFFFFF (8 MiB mapped)  Stack 0x7FFFFFFBF000..0x7FFFFFFDFFFF
    SHM "events_1" @ offset 0x200000 (8 KiB, shared with slot 0)
  Physical: 48 MiB used / 4096 MiB total
  ```
- Also print total physical memory used (from PMM frame allocator)

**Verify:** Build + boot. Press `Super+P` -> see memory dump on serial console.

**Decisions:**
- Output to serial only, or also render an on-screen overlay?
- Include per-slot memory usage breakdown?

---

## Phase 2: WASM Container Infrastructure (Steps 7-13)

### [DONE] Step 7 -- Add `WasmContainer` tracking struct (data only)
**Files:** New: `std/src/wasm/container.rs`, update `std/src/wasm/mod.rs`
**Changes:**
- Define:
  ```rust
  pub struct WasmContainer {
      pub id: u64,
      pub slot_id: u16,
      pub parent_id: Option<u64>,
      pub linear_memory_base: u64,  // = LINEAR_MEMORY_BASE + slot_id * LINEAR_MEMORY_SLOT_SIZE
      pub linear_memory_size: u64,  // current mapped size (grows via memory.grow)
      pub linear_memory_max: u64,   // = LINEAR_MEMORY_SLOT_SIZE (31 GiB)
      pub code_base: u64,           // = CODE_REGION_BASE + slot_id * CODE_SLOT_SIZE
      pub stack_base: u64,          // = STACK_REGION_BASE + slot_id * STACK_SLOT_SIZE
      pub shm_mappings: Vec<(u64, u64, String)>, // (offset_in_linear_mem, size, name)
      pub return_value: Option<i32>,
  }
  ```
- Add global `CONTAINER_REGISTRY: Mutex<BTreeMap<u64, WasmContainer>>`
- Register a container entry when `std::wasm::run()` is called
- Deregister when `run()` returns
- No execution changes -- pure bookkeeping

**Verify:** Build + boot. All WASM apps run. Container registry tracks them silently.

**Decisions:**
- Registry in `std` (accessible to WASM runner) or `kernel` (accessible to syscalls)?
- Use slot_id as container_id, or separate ID space?

---

### [DONE] Step 8 -- Wire WASM linear memory to SAS slot region
**Files:** `std/src/wasm/interpreter/store/linear_memory.rs`, `std/src/wasm/runner.rs`
**Changes:**
- Currently `LinearMemory` uses `Vec<AtomicU8>` on kernel heap
- Add alternative constructor: `LinearMemory::new_sas(base_addr: u64, initial_pages: u32)`
  - Backed by SAS-mapped pages at the process's linear memory slot address
  - `base_addr = LINEAR_MEMORY_BASE + slot_id * LINEAR_MEMORY_SLOT_SIZE`
  - `memory.grow()` maps additional physical pages into the slot region
  - Max 31 GiB per slot (4 GiB wasm32 limit + SHM mappings + future memory64)
  - The WASM guest sees offset 0 = base_addr, offset N = base_addr + N
- SHM allocations are mapped into the linear memory region at offsets beyond current memory.size
  - WASM accesses SHM via normal loads/stores at those offsets
  - Kernel tracks which offset ranges are SHM vs private
- In `runner.rs`, use `new_sas()` if the process has a valid SAS slot, else fall back to `Vec`
- The existing `Vec<AtomicU8>` path stays as default -- `new_sas()` is opt-in for now

**Verify:** Build + boot. Default path unchanged. Enable SAS memory for one app, verify it works.

**Decisions:**
- Should `new_sas()` be the default for all WASM apps, or opt-in?
- Map pages on demand (page fault handler) or eagerly on `memory.grow`?
- Does AOT's `AotContext.memory_base` need updating for SAS addresses?

---

### [DONE] Step 9 -- AOT compiler: support SAS memory base pointers
**Files:** `std/src/wasm/aot/compiler.rs`, `std/src/wasm/aot/runtime.rs`
**Changes:**
- `AotContext.memory_base` currently points to Vec's data pointer
- When using SAS linear memory, set it to the SAS virtual address
- AOT-generated code already uses R14 as memory base -- no codegen changes needed
- Verify bounds checks work with SAS addresses
- Verify trap handlers work (same address space, should be fine)

**Verify:** Build + boot. `aot_test.wasm` (AOT=true) works with SAS-backed memory.

**Decisions:**
- Use hardware guard pages for bounds checking instead of software
- Verify 16-byte stack alignment

---

### [DONE] Step 10 -- Implement nested container planting (memory reservation)
**Files:** `std/src/wasm/container.rs`, `std/src/wasm/runner.rs`
**Changes:**
- Add `plant(parent_store, wasm_bytes, offset_in_parent, size) -> child_container_id`:
  1. Validate `offset_in_parent + size` within parent's linear memory
  2. Create child `WasmContainer` with `memory_base = parent.memory_base + offset`
  3. Load + validate child WASM module
  4. Create child Store whose linear memory is a sub-slice of parent's
  5. Register child in `CONTAINER_REGISTRY` with parent link
- Add `harvest(child_id) -> Option<i32>` to retrieve return value after child exits
- Expose as Rust API only (no WASI export yet)

**Verify:** Build + boot. Existing apps unaffected. Test with a child WASM that returns 42.

**Decisions:**
- Child runs synchronously (blocking parent) or asynchronously (separate thread)?
- Child trap: kill child only, or propagate to parent?
- Recursive nesting (child plants grandchild)?

---

### [DONE] Step 11 -- Memory propagation for nested containers
**Files:** `std/src/wasm/container.rs`, `std/src/wasm/interpreter/store/linear_memory.rs`
**Changes:**
- When child calls `memory.grow()` and exceeds its allocation:
  1. Check if parent has room to expand child's region
  2. If parent needs to grow, call parent's `memory.grow()` recursively
  3. Adjust child's `memory_size` and `memory_max`
  4. Return new page count on success, -1 on failure
- Track parent-child memory offsets in container tree
- SAS addresses are stable (no relocation on parent grow -- SAS advantage)

**Verify:** Build + boot. Test with child that grows memory multiple times.

**Decisions:**
- Should parent approve child's grow requests? (callback vs automatic)
- Maximum total memory across nested tree?
- Conflict detection if child region overlaps parent's own data?

---

### [DONE] Step 12 -- Expose container operations as krakeos WASI extensions
**Files:** `std/src/wasm/wasi/krakeos.rs`, `std/src/wasm/wasi/preview2/mod.rs`
**Changes:**
- Add WASI host functions for `krakeos:system/container@0.1.0`:
  - `plant(wasm-bytes: list<u8>, offset: u32, size: u32) -> result<u64, string>`
  - `plant-from-path(path: string, offset: u32, size: u32) -> result<u64, string>`
  - `harvest(child-id: u64) -> result<i32, string>`
  - `list-children() -> list<u64>`
  - `kill-child(child-id: u64) -> result<_, string>`
- Register in `create_wasi_p2_imports()` alongside existing krakeos extensions
- Write test WASM app (`apps/container_test`) that exercises plant/harvest

**Verify:** Build + boot. Existing apps unaffected. `container_test.wasm` demonstrates nesting.

**Decisions:**
- `plant()` with raw bytes vs `plant-from-path()` with filesystem path? Support both.
- Async harvest (poll-based) or sync only? fir now just sync
- Do containers inherit parent's WASI context (fds, env vars)? No, but the parent can chose the child's context.

---

### [DONE] Step 13 -- Add `container_test.wasm` to boot sequence
**Files:** `userland/src/main.rs`, new: `apps/container_test/`
**Changes:**
- Create `apps/container_test/src/main.rs` that:
  1. Reads a tiny WASM module from filesystem (or embeds hardcoded bytes)
  2. Plants it at offset 0x10000 in its own memory
  3. Harvests the return value
  4. Prints `"Container test: child returned {value}"` to serial
- Add to `Cargo.toml` workspace members
- Add spawn in userland: `std::wasm::run("@0xE0/apps/container_test.wasm", ...)`
- Update `make.bat` to build it

**Verify:** Build + boot. See "Container test: child returned 42" on serial. All other apps work.

**Decisions:**
- Run with AOT or interpreter? AOT
- Permanent boot app or just a test? just keep it there

---

## [DONE] Phase 3: WASI Preview 2 Compliance (Steps 14-20)

### Step 14 -- Fix bugs and add missing `wasi:io/streams@0.2.0` functions
**Files:** `std/src/wasm/wasi/preview2/mod.rs`, `std/src/io/wasi.rs`
**Changes:**
- Add missing `[method]output-stream.check-write() -> result<u64, stream-error>` (backpressure)
- Fix: `initial-cwd` currently aliased to `get_environment` -- implement properly to return CWD
- Fix: `get-random-u64` in `wasi:random/random@0.2.0` reuses insecure PRNG -- use RDRAND or separate seed
- Remove duplicate window create/update code in `preview2/mod.rs` (lines 525-648) -- use `krakeos/wasi.rs` versions only via `register_wasi()`

**Verify:** Build + boot. Existing apps unaffected. New functions callable.

**Decisions:**
- `check-write` return value: always return buffer capacity, or track actual available space?

---

### [DONE]  Step 15 -- Complete `wasi:io/poll@0.2.0`
**Files:** `std/src/wasm/wasi/preview2/mod.rs`
**Changes:**
- Add missing `[method]pollable.ready() -> bool` (non-blocking readiness check)
- Improve `poll()` implementation:
  - Timer pollables: check against monotonic clock
  - Stream read pollables: check if data available
  - Stream write pollables: check if buffer has space
  - Socket pollables: check `rx_queue` non-empty
- For now, implement as busy-poll with yield -- upgrade to blocking later

**Verify:** Build + boot. Timer-based poll works.

**Decisions:**
- Busy-poll or kernel-assisted blocking (new syscall)?
- Maximum number of pollables per call?

---

### [DONE] Step 16 -- Add missing `wasi:clocks/monotonic-clock@0.2.0` function
**Files:** `std/src/wasm/wasi/preview2/mod.rs`, `std/src/time/wasi.rs`
**Changes:**
- Add `subscribe-instant(when: instant) -> pollable` -- creates a timer pollable that fires at an absolute timestamp
- Verify `now()` returns nanosecond precision
- Verify `subscribe-duration` creates proper timer pollable (already registered)

**Verify:** Build + boot. Apps using monotonic clock unchanged.

**Decisions:**
- Timer resolution: PIT ticks converted to nanoseconds, or actual RTC?

---

### [DONE] Step 17 -- Complete `wasi:sockets/tcp@0.2.0` (missing 20 functions)
**Files:** `std/src/wasm/wasi/preview2/mod.rs`, `std/src/net/wasi.rs`
**Changes:**
- Add address introspection:
  - `local-address() -> result<ip-socket-address, error-code>`
  - `remote-address() -> result<ip-socket-address, error-code>`
  - `is-listening() -> bool`
  - `address-family() -> ip-address-family`
- Add connection control:
  - `shutdown(type) -> result<_, error-code>`
  - `set-listen-backlog-size(value) -> result<_, error-code>`
  - `subscribe() -> pollable`
- Add TCP options (stub with defaults, implement kernel-side later):
  - `keep-alive-enabled` / `set-keep-alive-enabled`
  - `keep-alive-idle-time` / `set-keep-alive-idle-time`
  - `keep-alive-interval` / `set-keep-alive-interval`
  - `keep-alive-count` / `set-keep-alive-count`
  - `hop-limit` / `set-hop-limit`
  - `receive-buffer-size` / `set-receive-buffer-size`
  - `send-buffer-size` / `set-send-buffer-size`
- Register all in `create_wasi_p2_imports()` under `wasi:sockets/tcp@0.2.0`

**Verify:** Build + boot. `net_test.wasm` TCP test still works. New functions callable.

**Decisions:**
- Which options need real kernel TCP stack changes vs just storing a value? 
- Buffer size defaults?

---

### [DONE] Step 18 -- Complete `wasi:sockets/udp@0.2.0` (missing 12 functions)
**Files:** `std/src/wasm/wasi/preview2/mod.rs`, `std/src/net/wasi.rs`
**Changes:**
- Add missing:
  - `finish-bind() -> result<_, error-code>`
  - `stream(remote-addr) -> result<(in-dgram-stream, out-dgram-stream), error-code>`
  - `local-address()`, `remote-address()`, `address-family()`
  - `unicast-hop-limit` / `set-unicast-hop-limit`
  - `receive-buffer-size` / `set-receive-buffer-size`
  - `send-buffer-size` / `set-send-buffer-size`
  - `subscribe() -> pollable`
  - `incoming-datagram-stream.subscribe() -> pollable`
  - `outgoing-datagram-stream.check-send() -> result<u64, error-code>`
  - `outgoing-datagram-stream.subscribe() -> pollable`

**Verify:** Build + boot. UDP tests work.

**Decisions:**
- IPv6 support: stub or implement? stub for now.

---

### [DONE] Step 19 -- Complete `wasi:sockets/ip-name-lookup@0.2.0`
**Files:** `std/src/wasm/wasi/preview2/mod.rs`, `std/src/net/wasi.rs`
**Changes:**
- Add missing:
  - `[method]resolve-address-stream.resolve-next-address() -> result<option<ip-address>, error-code>`
  - `[method]resolve-address-stream.subscribe() -> pollable`
- For now: only resolve "localhost" -> 127.0.0.1, everything else -> error

**Verify:** Build + boot. DNS stub works for localhost.

**Decisions:**
- Real DNS resolver (UDP stub forwarder) or just hardcoded entries?

---

### [DONE] Step 20 -- Implement Canonical ABI value lifting/lowering
**Files:** New: `std/src/wasm/component/canonical.rs`, update `std/src/wasm/component/mod.rs`
**Changes:**
- Implement core canonical ABI:
  - `lift_flat(values, type) -> ComponentValue`
  - `lower_flat(value, type) -> Vec<WasmValue>`
  - Types: bool, u8-u64, s8-s64, f32, f64, char, string, list, record, variant, enum, flags, option, result
  - String: UTF-8 encode/decode with pointer+length in linear memory
  - List: element pointer + count
- Wire into component executor: `canon.lift` and `canon.lower` instructions
- Implement resource handle management: `resource.new`, `resource.drop`, `resource.rep`

**Verify:** Build + boot. Existing apps unaffected. Unit test lifts/lowers basic types.

**Decisions:**
- Support `realloc` canonical option? y
- Handle table per-component or per-store? 
- String encoding: UTF-8 only or also UTF-16? utf8

---

## Phase 4: KrakeOS WASI Extensions & App Porting (Steps 21-30)

### [DONE] Step 21 -- Add missing krakeos WASI host wrappers (Tier 1: taskbar)
**Files:** `std/src/wasm/wasi/krakeos.rs`, `std/src/os/krakeos/wasi.rs`
**Changes:**
- Expose to WASM host (these exist natively but have NO WASI wrapper):
  - `get-pid() -> u64` -- inkui needs this for event queue naming (`events_{pid}`)
  - `get-current-user() -> string` -- taskbar displays username
- `get-time` and `get-date` can be derived from existing `wasi:clocks/wall-clock@0.2.0::now()` inside the WASM app, so no new host function needed -- just ensure wall-clock works correctly

**Verify:** Build + boot. Taskbar.wasm can call get-pid and get-current-user.

**Decisions:**
- Should `get-pid` return the OS PID or the container ID? container's PID, if nested then the PID ofthe memory slot they all reside in.
- Should `get-current-user` be a real user system or always return "root"? real user sys
 
---

### [DONE] Step 22 -- Add missing krakeos WASI host wrappers (Tier 2: terminal)
**Files:** `std/src/wasm/wasi/krakeos.rs`, `std/src/os/krakeos/wasi.rs`
**Changes:**
- Expose to WASM host:
  - `ioctl(fd: u32, request: u64, arg: u64) -> i32` -- terminal needs TIOCSWINSZ
  - `set-nonblock(fd: u32, nonblock: bool) -> i32` -- non-blocking pipe reads
  - `poll(fds: list<poll-fd>, timeout: i32) -> i32` -- multiplex I/O
- Better: create `krakeos:system/terminal@0.1.0` interface:
  - `set-window-size(fd: u32, rows: u16, cols: u16) -> result<_, string>`
  - `get-window-size(fd: u32) -> result<(u16, u16), string>`
  This wraps the ioctl(TIOCSWINSZ) pattern into a clean typed API

**Verify:** Build + boot. Terminal-related host functions callable from WASM.

**Decisions:**
- Expose raw `ioctl` or only typed wrappers like `set-window-size`? `set-window-size` 
- Should `poll` integrate with `wasi:io/poll` or be separate? integrate

---

### [DONE] Step 23 -- Add missing krakeos WASI host wrappers (Tier 3: sysmon)
**Files:** `std/src/wasm/wasi/krakeos.rs`, `std/src/os/krakeos/wasi.rs`
**Changes:**
- Expose to WASM host under `krakeos:system/debug@0.1.0`:
  - `get-process-list() -> list<process-info>` -- wraps syscall 110
  - `kill(pid: u64, signal: u32) -> result<_, string>` -- wraps syscall 62
  - `dump-vma() -> string` -- returns the Super+P dump as a string
  - `get-memory-usage() -> (used: u64, total: u64)`

**Verify:** Build + boot. Sysmon can query process list from WASM.

**Decisions:**
- Should kill be restricted? (only kill own children, or any process?) SHould be able to kill own children / itself.
- Should process-info include slot_id? (for debugging)

---

### [DONE] Step 24 -- Ensure inkui compiles for wasm32-wasip2
**Files:** `inkui/src/lib.rs`, `inkui/src/window.rs`, `inkui/Cargo.toml`
**Changes:**
- Audit all inkui code for non-WASM-compatible operations:
  - `std::memory::malloc/free` -> use standard allocator (works in WASM via custom allocator)
  - `std::memory::shm_get` -> use `krakeos:system/memory` WASI interface (already exposed)
  - Window syscalls -> use `krakeos:system/window` (already exposed)
  - `std::os::process_get_pid()` -> use `krakeos:system/process::get-pid` (exposed in Step 21)
  - Framebuffer pointers -> must be offsets within WASM linear memory
- Add `#[cfg(target_arch = "wasm32")]` paths where needed
- Keep `#[cfg(not(target_arch = "wasm32"))]` paths for native builds (backward compat)

**Verify:** `cargo check --package=inkui --target=wasm32-wasip2` compiles.

**Decisions:**
- Does titanf (font renderer) work in WASM? Yes, don't touch
- Does asvgard (SVG/PNG) work in WASM? Yes, don't touch
- Framebuffer sharing: WASM linear memory offsets vs SAS virtual addresses? in program's linear memory 

---

### [DONE] Step 25 -- Port `taskbar` to use WASI Preview 2 fully (Do nothing basically)
**Files:** `apps/taskbar/src/main.rs`
**Changes:**
- Taskbar already builds as WASM -- audit its WASI usage
- Replace any remaining Preview 1 calls with Preview 2
- Use krakeos extensions from Steps 21-23
- Verify works with both interpreter and AOT

**Verify:** Build + boot. Taskbar appears, shows time and username.

**Decisions:**
- AOT or interpreter for taskbar? 

---

### [DONE] Step 26 -- Port `shell` to WASM
**Files:** `apps/shell/src/main.rs`, `apps/shell/Cargo.toml`
**Changes:**
- Change build target to `wasm32-wasip2`
- Replace direct syscalls with WASI equivalents:
  - File I/O -> `wasi:filesystem`
  - Process spawning -> `krakeos:system/process::spawn` or `krakeos:system/container::plant-from-path`
  - stdin/stdout -> `wasi:cli`
  - waitpid -> `krakeos:system/process::waitpid`
- Update `make.bat`: build shell.wasm, place in `tree/apps/`

**Verify:** Build + boot. Launch shell.wasm. Type commands, see output.

**Decisions:**
- Shell uses containers (plant/harvest) or process spawning (separate PID)? container, everything should be contained unless spawed by userland directly. 
- Builtins: cd, ls, cat, echo, exit minimum? yes
- Tab completion? No, for now

---

### [DONE] Step 27 -- Port `term` (terminal emulator) to WASM
**Files:** `apps/term/src/main.rs`, `apps/term/Cargo.toml`
**Changes:**
- Change build target to `wasm32-wasip2`
- Terminal uses `inkui` (ported in Step 24) for rendering
- Replace direct memory/SHM ops with WASI equivalents
- Terminal spawns shell as child container via `krakeos:system/container::plant-from-path`
- Pipe stdin/stdout between terminal <-> shell via WASI streams and `krakeos:system/process::pipe`
- Use `krakeos:system/terminal::set-window-size` instead of raw ioctl

**Verify:** Build + boot. `Super+T` spawns terminal with shell inside.

**Decisions:**
- inkui: statically linked into each WASM app, or shared component? Static link / import i ncargo.toml
- Font loading from WASM? (read via `wasi:filesystem`) Yes, use titanf
- ANSI escape code support? Yes

---

### Step 28 -- Port remaining native apps to WASM
**Files:** `apps/sysmon/`, `apps/cat/`, `apps/fps_test/`, `apps/tmap/`
**Changes:**
- For each app, change target to `wasm32-wasip2`
- Replace direct syscalls with WASI equivalents
- Update `make.bat` to build all as WASM
- Move outputs to `tree/apps/`
- Remove `tree/sys/bin/` entries for ported apps

**Verify:** Build + boot. Each app functions when spawned.

**Decisions:**
- Priority order? (cat simplest, sysmon most complex)
- Keep fps_test native for benchmarking comparison?

---

### Step 29 -- Port `userland` init process to WASM
**Files:** `userland/src/main.rs`, `kernel/src/main.rs`
**Changes:**
- Rewrite userland as `init.wasm`:
  1. Load wallpaper via `wasi:filesystem` + `krakeos:system/window`
  2. Spawn taskbar, shell as containers via `krakeos:system/container`
  3. Enter event loop
- Update kernel boot: load and run init.wasm directly via `std::wasm::run()` from `_start()`
- This removes the last mandatory ELF from the boot path

**Verify:** Build + boot. Everything works, but init is now WASM.

**Decisions:**
- AOT for init.wasm? (likely yes, for perf)
- init.wasm crash -> kernel panic or restart loop?
- Embed in kernel binary or load from disk?

---

### Step 30 -- Remove ELF loader (gate behind feature flag)
**Files:** `kernel/src/fs/elf.rs`, `kernel/src/interrupts/syscalls/process.rs`, `elfic/`
**Changes:**
- Gate all ELF loading behind `#[cfg(feature = "elf_support")]`
- Default: feature disabled (pure WASM boot)
- Remove `bits64pie.json` from default `make.bat`
- Remove native app build steps from `make.bat`
- Clean up `tree/sys/bin/` -- delete all `.elf` files
- Keep `elfic` crate in workspace but don't build by default

**Verify:** Build + boot with ELF support disabled. Everything runs as WASM.

**Decisions:**
- Keep elfic for debug builds?
- Remove `adapter_close_badfd` and `cli_run` stubs from P1 compat layer?

---

## Phase 5: System Integration & Polish (Steps 31-38)

### Step 31 -- Implement remaining global keybindings
**Files:** `kernel/src/drivers/input/keyboard.rs`, `kernel/src/window_manager/`
**Changes:**
- `Super + T`: Spawn terminal -- send "spawn terminal" event to init.wasm via SHM event queue
- `Super + X`: Kill focused window's container -- find window -> owning PID -> terminate
- `Super + Z`: Toggle maximize -- store previous geometry, set fullscreen, or restore
- `Super + C`: Enter resize mode -- arrow keys resize focused window, Enter/Escape exits
- All kernel-level handlers (before dispatching to apps)

**Verify:** Build + boot. Press each combo, verify behavior.

**Decisions:**
- How does kernel send "spawn terminal" to init.wasm? (SHM event? syscall?)
- `Super+X` confirmation or immediate kill?
- Resize mode: arrow key step size?

---

### Step 32 -- PATH resolution for WASM modules
**Files:** `kernel/src/interrupts/syscalls/process.rs` or new `kernel/src/fs/path.rs`
**Changes:**
- Default PATH: `@0xE0/sys/bin;@0xE0/apps`
- When `spawn("shell")` called without full path:
  1. Try `@0xE0/sys/bin/shell.wasm`
  2. Try `@0xE0/apps/shell.wasm`
  3. Try without extension
- Store PATH in process environment (inheritable)

**Verify:** Build + boot. Shell launches apps by name.

**Decisions:**
- PATH: per-process env var or kernel global?
- `.wasm` auto-appended?

---

### Step 33 -- Fuel metering enforcement
**Files:** `std/src/wasm/interpreter/loop_executor.rs`, `std/src/wasm/aot/runtime.rs`
**Changes:**
- Enable fuel metering by default:
  - Default budget: 10M instructions per time-slice
  - Interpreter: enforce existing fuel tracking (trap on exhaustion)
  - AOT: fuel check at function entry + loop back-edges
- Fuel exhaustion -> trap -> container terminated
- Init and system containers get unlimited fuel
- Add `krakeos:system/container::set-fuel(child-id, amount)`

**Verify:** Build + boot. Normal apps run fine. Infinite loop test hits limit.

**Decisions:**
- Default budget?
- Replenish periodically (time-slice based)?
- Fuel cost model?

---

### Step 34 -- Inter-container communication channels
**Files:** `kernel/src/memory/shm.rs`, `std/src/wasm/wasi/krakeos.rs`
**Changes:**
- `krakeos:system/ipc@0.1.0`:
  - `channel-create(name, capacity) -> result<u64, string>`
  - `channel-send(id, data) -> result<_, string>`
  - `channel-recv(id) -> result<list<u8>, string>`
  - `channel-try-recv(id) -> result<option<list<u8>>, string>`
  - `channel-subscribe(id) -> pollable`
  - `channel-close(id)`
- Implementation: ring buffer in SHM (read_pos, write_pos, capacity as atomics)
- Integrate with `wasi:io/poll`

**Verify:** Build + boot. Two WASM apps communicate via channel.

**Decisions:**
- Maximum message size?
- Access control?

---

### Step 35 -- Process cleanup & slot reuse
**Files:** `kernel/src/interrupts/task.rs`, `kernel/src/memory/address_space.rs`
**Changes:**
- On container exit:
  1. Recursively terminate children (via container tree)
  2. Unmap SAS pages for linear memory, AOT code, and stack regions
  3. Free physical frames (including SHM pages if last reference)
  4. Close FDs, sockets, destroy windows
  5. Unmap SHM mappings from linear memory (decrement refcount on shared pages)
  6. Mark slot as free for reuse
- Add slot freelist to `address_space.rs`

**Verify:** Build + boot. Kill app (Super+X), slot reused by next spawn.

**Decisions:**
- Force-kill children timeout?
- Zero freed memory?

---

### Step 36 -- Scheduler improvements
**Files:** `kernel/src/interrupts/task.rs`, `kernel/src/drivers/periferics/timer.rs`
**Changes:**
- Priority levels: System (init), Normal (user apps), Background
- `get_next_thread()` checks highest priority first
- Per-container CPU time tracking
- Reduce timer: 100ms -> 10ms

**Verify:** Build + boot. System more responsive.

**Decisions:**
- Number of priority levels?
- Priority inheritance?

---

### Step 37 -- Network stack improvements
**Files:** `kernel/src/net/tcp.rs`, `kernel/src/net/socket.rs`
**Changes:**
- `TCP_NODELAY`: send immediately without Nagle
- `SO_REUSEADDR`: bind to recently-closed ports
- TCP connection timeout: 5 second SYN-ACK deadline
- Per-container socket limits (max 16)
- Socket cleanup on container exit

**Verify:** Build + boot. TCP tests pass.

**Decisions:**
- Timeout fixed or configurable?
- SO_KEEPALIVE with probes?

---

### Step 38 -- WIT files, component linking, testing & documentation
**Files:** New: `wit/`, update `GEMINI.md`
**Changes:**
- Create WIT files:
  ```
  wit/
    krakeos-system.wit     (process, container, memory, ipc, terminal, debug)
    krakeos-graphics.wit   (window, screen)
  ```
- Improve component executor (from Step 20):
  - Replace fuzzy import linker with proper interface-based resolution
  - Support `canon.lift`/`canon.lower` wrapped function calls
  - Test with two-component composition
- Create WASM test suite: `apps/test_suite/` -- tests each WASI interface
- Update `GEMINI.md` with final architecture

**Verify:** Build + boot. Test suite passes. WIT files parse correctly.

**Decisions:**
- WIT validation: `wit-parser` crate or external tool?
- Test suite: every boot or on demand?
- Auto-generate host function signatures from WIT?

---

## Summary

| Phase | Steps | Theme | Key Milestone |
|-------|-------|-------|---------------|
| 1 | 1-6 | Cleanup, safety, formalize SAS | Safe API, dead code gone, 4096 slots (31 GiB linear mem + 64 MiB code + 2 MiB stack), Super+P |
| 2 | 7-13 | WASM container infrastructure | Nested containers with plant/harvest, SAS-backed linear memory |
| 3 | 14-20 | WASI Preview 2 compliance | ~30 missing functions added, canonical ABI |
| 4 | 21-30 | KrakeOS extensions & app porting | All apps WASM, init.wasm, ELF deprecated |
| 5 | 31-38 | System integration & polish | Keybindings, fuel, IPC, scheduler, WIT |

**Total: 38 steps**, each compilable and bootable.

**Critical path:** 1 (safe macro) -> 2-3 (cleanup + slots) -> 7-9 (SAS memory) -> 10-12 (containers) -> 21-24 (WASI extensions + inkui) -> 29-30 (init WASM + ELF removal)

**Parallel work possible:**
- Phase 3 (WASI compliance) can run alongside Phase 2 (containers)
- Phase 4 app porting can start after Steps 21-24
- Phase 5 can be interleaved throughout

---
---

# APPENDIX A: SAS Slot Layout (4096 Slots)

The virtual address space is divided into 4096 fixed slots. Each slot has
four regions: **AOT code**, **user stack**, **kernel stack**, and **linear memory**.

**Everything the WASM guest touches lives in linear memory.** This includes:
heap allocations (dlmalloc/wee_alloc compiled into the wasm), shadow stack,
data/bss segments, string literals, AND shared memory buffers. There is no
separate heap region — the WASM heap is dlmalloc inside linear memory.
SHM is not a separate VA region — it's just physical pages mapped into
multiple processes' linear memory at different offsets.

The user (WASM shadow) stack lives inside linear memory. The **kernel stack**
is a separate region used by the kernel thread while executing WASM on behalf
of the process. The **user stack** region holds the x86-64 native stack for
the AOT runtime / interpreter call frames.

## Constants

```rust
pub const MAX_SLOTS: u16 = 4096;

// --- AOT code --- (bottom of VA, right after kernel low 4 GiB)
// 64 MiB per slot × 4096 = 256 GiB. Most WASM compiles to < 4 MiB native.
pub const CODE_REGION_BASE: u64  = 0x0000_0001_0000_0000; // 4 GiB
pub const CODE_SLOT_SIZE: u64    = 64 * 1024 * 1024; // 64 MiB
// Ends at: 260 GiB (0x41_0000_0000)

// --- User stack --- (interpreter/AOT native x86-64 stack)
// 2 MiB per slot × 4096 = 8 GiB. Guard pages on both ends.
pub const STACK_REGION_BASE: u64 = 0x0000_0041_0000_0000; // 260 GiB
pub const STACK_SLOT_SIZE: u64   = 2 * 1024 * 1024; // 2 MiB
// Ends at: 268 GiB (0x43_0000_0000)

// --- Kernel stack --- (kernel thread stack per WASM process)
// 128 KiB per slot × 4096 = 512 MiB.
pub const KERNEL_STACK_REGION_BASE: u64 = 0x0000_0043_0000_0000; // 268 GiB
pub const KERNEL_STACK_SLOT_SIZE: u64   = 128 * 1024; // 128 KiB
// Ends at: 268.5 GiB (0x43_2000_0000)

// --- Linear memory --- (bulk of the address space)
// 31 GiB per slot × 4096 = ~124 TiB. Everything WASM touches.
// VA reservation is free; only mapped physical pages cost real RAM.
// The heap lives inside linear memory (no separate heap region).
pub const LINEAR_MEMORY_BASE: u64      = 0x0000_0043_2000_0000; // 268.5 GiB
pub const LINEAR_MEMORY_SLOT_SIZE: u64 = 31 * 1024 * 1024 * 1024; // 31 GiB
// Ends at: ~124 TiB (fits in 128 TiB lower canonical half with ~3.7 TiB headroom)
```

**Layout math:** 4-level paging = 128 TiB lower canonical. Kernel takes 4 GiB,
code takes 256 GiB, user stack takes 8 GiB, kernel stack takes 0.5 GiB =
268.5 GiB overhead. Remaining ~127.7 TiB / 4096 slots ≈ 31.93 GiB →
31 GiB per slot with ~3.7 TiB headroom.

## Address calculation per slot

```
Slot N:
  code_base         = CODE_REGION_BASE + N * CODE_SLOT_SIZE
                    = 0x1_0000_0000 + N * 64 MiB

  stack_base        = STACK_REGION_BASE + N * STACK_SLOT_SIZE
                    = 0x41_0000_0000 + N * 2 MiB
  stack_top         = stack_base + STACK_SLOT_SIZE         // grows down from top

  kernel_stack_base = KERNEL_STACK_REGION_BASE + N * KERNEL_STACK_SLOT_SIZE
                    = 0x43_0000_0000 + N * 128 KiB

  linear_memory_base = LINEAR_MEMORY_BASE + N * LINEAR_MEMORY_SLOT_SIZE
                     = 0x43_2000_0000 + N * 31 GiB
  (this is what the WASM runtime sets as memory_base / R14 for AOT)
  WASM offset 0     → linear_memory_base
  WASM offset K     → linear_memory_base + K
  WASM heap         → linear_memory_base + <dlmalloc managed region>
  SHM mapped at     → linear_memory_base + <some offset within slot>
```

## SHM = linear memory mappings

Shared memory is **not** a separate VA region. Instead:
1. Process A calls `shm_create("buffer", 4096)` → kernel allocates physical pages
2. Kernel maps those pages into A's linear memory at offset X → A sees it at WASM offset X
3. Process B calls `shm_open("buffer")` → kernel maps SAME physical pages into B's linear
   memory at offset Y → B sees it at WASM offset Y
4. Both processes read/write the same physical memory via their own WASM offsets

This means WASM programs access SHM via normal `i32.load`/`i32.store` — no special
instructions or API needed beyond the initial map/unmap syscalls.

## Guard pages

- 4 KiB unmapped guard page at **each end** of each stack slot (catches stack overflow/underflow)
- 4 KiB unmapped guard page at the **end** of each linear memory slot (catches OOB)
- 4 KiB unmapped guard page at end of each code slot

## Range validation (no overlaps)

```
Kernel low:          0 .. 4 GiB                  = 0x00_0000_0000 .. 0x01_0000_0000   reserved
4096 code slots:     4 GiB .. 260 GiB            = 0x01_0000_0000 .. 0x41_0000_0000   256 GiB
4096 stack slots:    260 GiB .. 268 GiB          = 0x41_0000_0000 .. 0x43_0000_0000   8 GiB
4096 linear memory:  268 GiB .. ~124 TiB         = 0x43_0000_0000 .. 0x7C83_0000_0000 ~124 TiB
                                                   (canonical limit: 0x7FFF_FFFF_FFFF = 128 TiB)
                                                   headroom: ~3.7 TiB
```

## Design notes

- **Linear memory 31 GiB:** ~8× more than wasm32's 4 GiB limit. VA is free — only
  mapped pages consume physical RAM. Room for SHM mappings + future memory64.
- **SHM inside linear memory:** Eliminates the old separate SHM VA region. WASM programs
  access shared buffers via normal memory operations. The kernel just maps the same physical
  pages into multiple slots.
- **Code 64 MiB:** AOT-compiled native x86-64. Most WASM modules compile to < 4 MiB native.
- **Stack 2 MiB:** Native call stack for interpreter loop / AOT trampolines. Separate from
  linear memory so WASM can't overwrite return addresses. Guard pages on both sides.
- **4096 slots:** 4× more than old 1024-slot layout. Current MAX_THREADS = 128.

---
---

# APPENDIX B: WASI Preview 2 -- Complete Function Audit

Status: check = implemented, warn = stub/partial, X = missing

## `wasi:cli/run@0.2.0`
| Function | Status | Notes |
|----------|--------|-------|
| `run` | check | Returns 0 |

## `wasi:cli/exit@0.2.0`
| Function | Status | Notes |
|----------|--------|-------|
| `exit` | check | Via HaltExecutionError |

## `wasi:cli/environment@0.2.0`
| Function | Status | Notes |
|----------|--------|-------|
| `get-environment` | check | Via KrakeosWasiEnv::environ_get() |
| `get-arguments` | check | Via KrakeosWasiEnv::args_get() |
| `initial-cwd` | BUG | Wrongly aliased to get-environment. **Needs fix.** |

## `wasi:cli/stdin@0.2.0`, `stdout@0.2.0`, `stderr@0.2.0`
| Function | Status |
|----------|--------|
| `get-stdin` | check |
| `get-stdout` | check |
| `get-stderr` | check |

## `wasi:cli/terminal-stdin@0.2.0`, `terminal-stdout@0.2.0`, `terminal-stderr@0.2.0`
| Function | Status |
|----------|--------|
| `get-terminal-stdin` | check |
| `get-terminal-stdout` | check |
| `get-terminal-stderr` | check |
| `[resource-drop]terminal-input` | check |
| `[resource-drop]terminal-output` | check |

## `wasi:io/streams@0.2.0`
| Function | Status | Notes |
|----------|--------|-------|
| `[method]input-stream.read` | check | |
| `[method]input-stream.blocking-read` | check | Same as read |
| `[method]input-stream.skip` | check | |
| `[method]input-stream.blocking-skip` | check | |
| `[method]input-stream.subscribe` | check | |
| `[method]output-stream.check-write` | X | **Missing -- needed for backpressure** |
| `[method]output-stream.write` | check | |
| `[method]output-stream.blocking-write-and-flush` | check | |
| `[method]output-stream.flush` | check | |
| `[method]output-stream.blocking-flush` | check | |
| `[method]output-stream.subscribe` | check | |
| `[method]output-stream.write-zeroes` | check | |
| `[method]output-stream.blocking-write-zeroes` | check | |
| `[method]output-stream.splice` | check | |
| `[method]output-stream.blocking-splice` | check | |
| `[resource-drop]input-stream` | check | |
| `[resource-drop]output-stream` | check | |

## `wasi:io/poll@0.2.0`
| Function | Status | Notes |
|----------|--------|-------|
| `poll` | check | |
| `[method]pollable.ready` | X | **Missing -- non-blocking readiness check** |
| `[method]pollable.block` | check | |
| `[resource-drop]pollable` | check | |

## `wasi:io/error@0.2.0`
| Function | Status |
|----------|--------|
| `[method]error.to-debug-string` | check |
| `[resource-drop]error` | check |

## `wasi:clocks/monotonic-clock@0.2.0`
| Function | Status | Notes |
|----------|--------|-------|
| `now` | check | |
| `resolution` | check | |
| `subscribe-instant` | X | **Missing** |
| `subscribe-duration` | check | |

## `wasi:clocks/wall-clock@0.2.0`
| Function | Status |
|----------|--------|
| `now` | check |
| `resolution` | check |

## `wasi:clocks/timezone@0.2.0` (deprecated, optional)
| Function | Status |
|----------|--------|
| `display` | check |
| `utc-offset` | check |

## `wasi:random/random@0.2.0`
| Function | Status | Notes |
|----------|--------|-------|
| `get-random-bytes` | warn | Uses xorshift PRNG. **Should use RDRAND.** |
| `get-random-u64` | BUG | Reuses insecure PRNG. **Must be crypto-grade.** |

## `wasi:random/insecure@0.2.0`
| Function | Status |
|----------|--------|
| `get-insecure-random-bytes` | check |
| `get-insecure-random-u64` | check |

## `wasi:random/insecure-seed@0.2.0`
| Function | Status |
|----------|--------|
| `insecure-seed` | check |

## `wasi:filesystem/types@0.2.0`
| Function | Status |
|----------|--------|
| `[method]descriptor.read-via-stream` | check |
| `[method]descriptor.write-via-stream` | check |
| `[method]descriptor.append-via-stream` | check |
| `[method]descriptor.advise` | check |
| `[method]descriptor.sync-data` | check |
| `[method]descriptor.get-flags` | check |
| `[method]descriptor.get-type` | check |
| `[method]descriptor.set-size` | check |
| `[method]descriptor.set-times` | check |
| `[method]descriptor.read` | check |
| `[method]descriptor.write` | check |
| `[method]descriptor.read-directory` | check |
| `[method]descriptor.sync` | check |
| `[method]descriptor.create-directory-at` | check |
| `[method]descriptor.stat` | check |
| `[method]descriptor.stat-at` | check |
| `[method]descriptor.set-times-at` | check |
| `[method]descriptor.link-at` | warn (ENOTSUP) |
| `[method]descriptor.open-at` | check |
| `[method]descriptor.readlink-at` | warn (ENOTSUP) |
| `[method]descriptor.remove-directory-at` | check |
| `[method]descriptor.rename-at` | check |
| `[method]descriptor.symlink-at` | warn (ENOTSUP) |
| `[method]descriptor.unlink-file-at` | check |
| `[method]descriptor.is-same-object` | check |
| `[method]descriptor.metadata-hash` | check |
| `[method]descriptor.metadata-hash-at` | check |
| `[method]descriptor.seek` | check |
| `filesystem-error-code` | check |
| `[method]directory-entry-stream.read-directory-entry` | check |
| `[resource-drop]descriptor` | check |
| `[resource-drop]directory-entry-stream` | check |

## `wasi:filesystem/preopens@0.2.0`
| Function | Status |
|----------|--------|
| `get-directories` | check |

## `wasi:sockets/tcp@0.2.0`
| Function | Status | Notes |
|----------|--------|-------|
| `[method]tcp-socket.start-bind` | check | |
| `[method]tcp-socket.finish-bind` | check | |
| `[method]tcp-socket.start-connect` | check | |
| `[method]tcp-socket.finish-connect` | check | |
| `[method]tcp-socket.start-listen` | check | |
| `[method]tcp-socket.finish-listen` | check | |
| `[method]tcp-socket.accept` | check | |
| `[method]tcp-socket.local-address` | X | **Missing** |
| `[method]tcp-socket.remote-address` | X | **Missing** |
| `[method]tcp-socket.is-listening` | X | **Missing** |
| `[method]tcp-socket.address-family` | X | **Missing** |
| `[method]tcp-socket.set-listen-backlog-size` | X | **Missing** |
| `[method]tcp-socket.keep-alive-enabled` | X | **Missing** |
| `[method]tcp-socket.set-keep-alive-enabled` | X | **Missing** |
| `[method]tcp-socket.keep-alive-idle-time` | X | **Missing** |
| `[method]tcp-socket.set-keep-alive-idle-time` | X | **Missing** |
| `[method]tcp-socket.keep-alive-interval` | X | **Missing** |
| `[method]tcp-socket.set-keep-alive-interval` | X | **Missing** |
| `[method]tcp-socket.keep-alive-count` | X | **Missing** |
| `[method]tcp-socket.set-keep-alive-count` | X | **Missing** |
| `[method]tcp-socket.hop-limit` | X | **Missing** |
| `[method]tcp-socket.set-hop-limit` | X | **Missing** |
| `[method]tcp-socket.receive-buffer-size` | X | **Missing** |
| `[method]tcp-socket.set-receive-buffer-size` | X | **Missing** |
| `[method]tcp-socket.send-buffer-size` | X | **Missing** |
| `[method]tcp-socket.set-send-buffer-size` | X | **Missing** |
| `[method]tcp-socket.subscribe` | X | **Missing** |
| `[method]tcp-socket.shutdown` | X | **Missing** |
| `[resource-drop]tcp-socket` | check | |

## `wasi:sockets/tcp-create-socket@0.2.0`
| Function | Status |
|----------|--------|
| `create-tcp-socket` | check |

## `wasi:sockets/udp@0.2.0`
| Function | Status | Notes |
|----------|--------|-------|
| `[method]udp-socket.start-bind` | check | |
| `[method]udp-socket.finish-bind` | X | **Missing** |
| `[method]udp-socket.stream` | X | **Missing** |
| `[method]udp-socket.local-address` | X | **Missing** |
| `[method]udp-socket.remote-address` | X | **Missing** |
| `[method]udp-socket.address-family` | X | **Missing** |
| `[method]udp-socket.unicast-hop-limit` | X | **Missing** |
| `[method]udp-socket.set-unicast-hop-limit` | X | **Missing** |
| `[method]udp-socket.receive-buffer-size` | X | **Missing** |
| `[method]udp-socket.set-receive-buffer-size` | X | **Missing** |
| `[method]udp-socket.send-buffer-size` | X | **Missing** |
| `[method]udp-socket.set-send-buffer-size` | X | **Missing** |
| `[method]udp-socket.subscribe` | X | **Missing** |
| `[method]incoming-datagram-stream.receive` | check | |
| `[method]incoming-datagram-stream.subscribe` | X | **Missing** |
| `[method]outgoing-datagram-stream.check-send` | X | **Missing** |
| `[method]outgoing-datagram-stream.send` | check | |
| `[method]outgoing-datagram-stream.subscribe` | X | **Missing** |
| `[resource-drop]udp-socket` | check | |
| `[resource-drop]incoming-datagram-stream` | check | |
| `[resource-drop]outgoing-datagram-stream` | check | |

## `wasi:sockets/udp-create-socket@0.2.0`
| Function | Status |
|----------|--------|
| `create-udp-socket` | check |

## `wasi:sockets/network@0.2.0`
| Function | Status |
|----------|--------|
| `[resource-drop]network` | check |

## `wasi:sockets/instance-network@0.2.0`
| Function | Status |
|----------|--------|
| `instance-network` | check |

## `wasi:sockets/ip-name-lookup@0.2.0`
| Function | Status | Notes |
|----------|--------|-------|
| `resolve-addresses` | warn | Limited stub |
| `[method]resolve-address-stream.resolve-next-address` | X | **Missing** |
| `[method]resolve-address-stream.subscribe` | X | **Missing** |
| `[resource-drop]resolve-address-stream` | check | |

## Totals

- **Implemented:** ~75 functions
- **Missing:** ~30 functions (mostly TCP/UDP options, subscribe/pollable, name-lookup)
- **Bugs:** 3 (initial-cwd aliased wrong, get-random-u64 not crypto, random uses weak PRNG)

---
---

# APPENDIX C: KrakeOS Extension Functions -- Complete Inventory

## Currently Implemented

### `krakeos:graphics/screen@0.2.0`
| Function | WASI Host | Native | Status |
|----------|-----------|--------|--------|
| `get-width` | check | check (syscall 106) | Done |
| `get-height` | check | check (syscall 107) | Done |

### `krakeos:system/window@0.2.0`
| Function | WASI Host | Native | Status |
|----------|-----------|--------|--------|
| `create` | check | check (syscall 100) | Done |
| `update` | check | check (syscall 102) | Done (duplicated in preview2/mod.rs -- remove dupe) |
| `get-events` | check | check (syscall 104) | Done (duplicated -- remove dupe) |

### `krakeos:system/process@0.2.0`
| Function | WASI Host | Native | Status |
|----------|-----------|--------|--------|
| `spawn` | check | check (syscall 59) | Done |
| `waitpid` | check | check (syscall 61) | Done |
| `pipe` | check | check (syscall 22) | Done |
| `yield` | check | check (int 0x81) | Done |
| `get-pid` | X | check (syscall 39) | **Need WASI host wrapper** |
| `ioctl` | X | check (syscall 16) | **Need WASI host wrapper** |
| `set-nonblock` | X | check (syscall 133) | **Need WASI host wrapper** |
| `poll` | X | check (syscall 7) | **Need WASI host wrapper** |
| `get-list` | X | check (syscall 110) | **Need WASI host wrapper** |

### `krakeos:system/memory@0.2.0`
| Function | WASI Host | Native | Status |
|----------|-----------|--------|--------|
| `shm-get` | check | check (syscall 120) | Rework: SHM now maps into linear memory, not separate VA region |
| `brk` | X | check (syscall 12) | **Need WASI host wrapper** |

### `krakeos:system/network@0.2.0`
| Function | WASI Host | Native | Notes |
|----------|-----------|--------|-------|
| `socket-create` | X | check (syscall 41) | Used internally by wasi:sockets host |
| `socket-connect` | X | check (syscall 42) | " |
| `socket-finish-connect` | X | check (syscall 54) | " |
| `socket-bind` | X | check (syscall 49) | " |
| `socket-listen` | X | check (syscall 51) | " |
| `socket-accept` | X | check (syscall 43) | " |
| `socket-send` | X | check (syscall 52) | " |
| `socket-recv` | X | check (syscall 53) | " |
| `socket-udp-send` | X | check (syscall 44) | " |
| `socket-udp-recv` | X | check (syscall 45) | " |
| `raw-send` | X | DEAD (returns -1) | **Delete** |
| `raw-recv` | X | DEAD (returns 0) | **Delete** |

## Missing WASI Host Wrappers (blocks WASM apps)

### Tier 1 -- Blocks taskbar/inkui
| Function | Needed By | Step |
|----------|-----------|------|
| `get-pid` | inkui (event queue: `events_{pid}`) | Step 21 |
| `get-current-user` (new) | taskbar (username display) | Step 21 |

### Tier 2 -- Blocks terminal
| Function | Needed By | Step |
|----------|-----------|------|
| `ioctl` or `set-window-size` | term (TIOCSWINSZ) | Step 22 |
| `set-nonblock` | term (async pipe reads) | Step 22 |
| `poll` | term (multiplex I/O) | Step 22 |

### Tier 3 -- Blocks sysmon
| Function | Needed By | Step |
|----------|-----------|------|
| `get-list` | sysmon (process list) | Step 23 |
| `kill` (new, syscall 62) | sysmon (kill process) | Step 23 |

## New KrakeOS Interfaces to Create

### `krakeos:system/container@0.1.0` (Steps 10-12)
```
plant(wasm-bytes: list<u8>, offset: u32, size: u32) -> result<u64, string>
plant-from-path(path: string, offset: u32, size: u32) -> result<u64, string>
harvest(child-id: u64) -> result<i32, string>
list-children() -> list<u64>
kill-child(child-id: u64) -> result<_, string>
set-fuel(child-id: u64, amount: u64) -> result<_, string>
```

### `krakeos:system/ipc@0.1.0` (Step 34)
```
channel-create(name: string, capacity: u32) -> result<u64, string>
channel-send(id: u64, data: list<u8>) -> result<_, string>
channel-recv(id: u64) -> result<list<u8>, string>
channel-try-recv(id: u64) -> result<option<list<u8>>, string>
channel-subscribe(id: u64) -> pollable
channel-close(id: u64)
```

### `krakeos:system/terminal@0.1.0` (Step 22)
```
set-window-size(fd: u32, rows: u16, cols: u16) -> result<_, string>
get-window-size(fd: u32) -> result<(u16, u16), string>
```

### `krakeos:system/debug@0.1.0` (Step 23)
```
get-process-list() -> list<process-info>
kill(pid: u64, signal: u32) -> result<_, string>
dump-vma() -> string
get-memory-usage() -> (used: u64, total: u64)
get-slot-info(slot-id: u16) -> option<slot-info>
```

---
---

# APPENDIX D: `method_export!` Macro Redesign

## Problem

The current macro in `std/src/lib.rs` forces all generated functions to be `pub unsafe fn`:

```rust
// Current (broken):
macro_rules! method_export {
    ($module:literal, $method:literal,
     pub unsafe fn $name:ident($($arg:ident: $ty:ty),*) $(-> $ret:ty)? $body:block
    ) => {
        #[cfg(target_arch = "wasm32")]
        #[link(wasm_import_module = $module)]
        unsafe extern "C" {
            #[link_name = $method]
            pub fn $name($($arg: $ty),*) $(-> $ret)?;  // caller must use unsafe
        }

        #[cfg(not(target_arch = "wasm32"))]
        pub unsafe fn $name($($arg: $ty),*) $(-> $ret)? $body  // caller must use unsafe
    };
}
```

Every call site is forced to write `unsafe { process_yield(); }` even though
yielding the CPU is perfectly safe from the caller's perspective.

## Fix

The macro always generates `pub fn`. The `unsafe` is hidden inside the function body:

```rust
macro_rules! method_export {
    ($module:literal, $method:literal,
     pub fn $name:ident($($arg:ident: $ty:ty),* $(,)?) $(-> $ret:ty)? $body:block
    ) => {
        #[cfg(target_arch = "wasm32")]
        pub fn $name($($arg: $ty),*) $(-> $ret)? {
            #[link(wasm_import_module = $module)]
            unsafe extern "C" {
                #[link_name = $method]
                fn __raw($($arg: $ty),*) $(-> $ret)?;
            }
            unsafe { __raw($($arg),*) }
        }

        #[cfg(not(target_arch = "wasm32"))]
        pub fn $name($($arg: $ty),*) $(-> $ret)? {
            unsafe { (|| $body)() }
        }
    };
}
```

## Usage change

```rust
// BEFORE:
method_export!("krakeos:system/process@0.2.0", "yield",
    pub unsafe fn process_yield() {
        core::arch::asm!("int 0x81");
    }
);
// Caller: unsafe { process_yield(); }

// AFTER:
method_export!("krakeos:system/process@0.2.0", "yield",
    pub fn process_yield() {
        core::arch::asm!("int 0x81");
    }
);
// Caller: process_yield();
```

## All 22 functions to change

Every function in `std/src/os/krakeos/mod.rs` changes from `pub unsafe fn` to `pub fn`.
Two dead stubs are deleted entirely:

| Function | Change |
|----------|--------|
| `process_yield` | `pub unsafe fn` -> `pub fn` |
| `process_get_pid` | `pub unsafe fn` -> `pub fn` |
| `process_waitpid` | `pub unsafe fn` -> `pub fn` |
| `process_spawn` | `pub unsafe fn` -> `pub fn` |
| `process_pipe` | `pub unsafe fn` -> `pub fn` |
| `process_ioctl` | `pub unsafe fn` -> `pub fn` |
| `process_set_nonblock` | `pub unsafe fn` -> `pub fn` |
| `process_get_list` | `pub unsafe fn` -> `pub fn` |
| `process_poll` | `pub unsafe fn` -> `pub fn` |
| `shm_get_raw` | `pub unsafe fn` -> `pub fn` |
| `get_random_bytes` | `pub unsafe fn` -> `pub fn` |
| `memory_brk` | `pub unsafe fn` -> `pub fn` |
| `socket_create` | `pub unsafe fn` -> `pub fn` |
| `socket_connect` | `pub unsafe fn` -> `pub fn` |
| `socket_finish_connect` | `pub unsafe fn` -> `pub fn` |
| `socket_bind` | `pub unsafe fn` -> `pub fn` |
| `socket_listen` | `pub unsafe fn` -> `pub fn` |
| `socket_accept` | `pub unsafe fn` -> `pub fn` |
| `socket_send` | `pub unsafe fn` -> `pub fn` |
| `socket_recv` | `pub unsafe fn` -> `pub fn` |
| `socket_udp_send` | `pub unsafe fn` -> `pub fn` |
| `socket_udp_recv` | `pub unsafe fn` -> `pub fn` |
| `net_send` | **DELETE** (dead stub, returns -1) |
| `net_recv` | **DELETE** (dead stub, returns 0) |

Then remove every `unsafe { ... }` wrapper at ~30+ call sites across:
- `std/src/os/krakeos/mod.rs` (helper functions like `yield_task`, `brk`, `pipe`, etc.)
- `std/src/os/krakeos/wasi.rs`
- `std/src/net/wasi.rs`
- `std/src/net/host.rs`
- `apps/*/src/main.rs`
- `inkui/src/window.rs`
- `userland/src/main.rs`

---
---

# APPENDIX E: Dead Code to Remove

| Code | File | Reason | Step |
|------|------|--------|------|
| `pml4_phys` on `Process` | `kernel/src/interrupts/task.rs` | Never used for CR3 switching, always set to KERNEL_PML4 | 2 |
| `pml4_phys` in `schedule()` return | `kernel/src/interrupts/task.rs` | Returned but never read | 2 |
| `map_user_memory_into_kernel()` | `kernel/src/memory/vmm.rs` | Never called anywhere in codebase | 2 |
| `net_send()` stub | `std/src/os/krakeos/mod.rs` | Returns -1, never called | 1 |
| `net_recv()` stub | `std/src/os/krakeos/mod.rs` | Returns 0, never called | 1 |
| Duplicate `window_create_host` | `std/src/wasm/wasi/preview2/mod.rs:525-588` | Duplicated by `krakeos/wasi.rs::window_create` | 14 |
| Duplicate `window_update_host` | `std/src/wasm/wasi/preview2/mod.rs:590-648` | Duplicated by `krakeos/wasi.rs::window_update` | 14 |
| Duplicate `window_get_events_host` | `std/src/wasm/wasi/preview2/mod.rs:650-668` | Duplicated by `krakeos/wasi.rs::window_get_events` | 14 |
| Duplicate `shm_get_host` | `std/src/wasm/wasi/preview2/mod.rs:670-688` | Duplicated by `krakeos/wasi.rs::shm_get_host` | 14 |
| Duplicate `process_spawn_host` | `std/src/wasm/wasi/preview2/mod.rs:445-483` | Can be consolidated | 14 |
| Duplicate `process_waitpid_host` | `std/src/wasm/wasi/preview2/mod.rs:485-493` | Can be consolidated | 14 |
| Duplicate `process_pipe_host` | `std/src/wasm/wasi/preview2/mod.rs:495-523` | Can be consolidated | 14 |
| `adapter_close_badfd` | `std/src/wasm/wasi/preview2/mod.rs` | P1 compat only | 30 |
| `cli_run` stub | `std/src/wasm/wasi/preview2/mod.rs` | Returns 0, never meaningful | 30 |
| `initial-cwd` wrong impl | `std/src/wasm/wasi/preview2/mod.rs:184` | Calls `get_environment` instead of returning CWD | 2 (fix) |
