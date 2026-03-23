# Ring 3 AOT Execution Plan

## Goal

Move WASM execution from Ring 0 (kernel thread) to Ring 3 (userspace). The kernel still performs AOT compilation, but the resulting native x86_64 code — including all host function implementations — is placed entirely in the process's code slot and executed as a Ring 3 task. No interpreter, no Store object, no trampoline function pointers into the kernel. Just native code + syscalls.

---

## Current Architecture (What Exists Today)

### How a WASM Process Runs Today

1. `kernel/src/syscalls/process.rs` → `spawn_process()` reads the `.wasm` file from the VFS
2. A **kernel thread** is spawned that calls `std::wasm::runner::run_with_buffer()`
3. The WASM binary is validated, a `Store<T>` object is created with `aot_enabled = true`
4. The linker registers ~231 host functions (WASI Preview 1, Preview 2, KrakeOS-specific)
5. `module_instantiate_unchecked()` in `std/src/wasm/interpreter/store/mod.rs` (line 314) calls `AotCompiler::new(validation_info).compile_module()` producing an `AotModule` (raw x86_64 machine code + function offset table)
6. The raw machine code is copied to the process's **code slot**: `CODE_REGION_BASE + (slot_id * 64 MiB)`
7. Each WASM function gets an `aot_ptr` pointing into the code slot
8. `resume_unchecked()` (line 512) invokes the entry point via inline assembly:
   ```asm
   mov r12, rsp        ; save host (kernel) RSP
   mov rsp, {sp}       ; switch to AOT stack (4 MiB allocation)
   call {aot_ptr}      ; call into code slot
   mov rsp, r12        ; restore kernel RSP
   ```
9. The AOT code runs **in Ring 0** as part of the kernel thread
10. When AOT code needs a host function (file I/O, memory.grow, etc.), it calls through a **trampoline table** — an array of 284 kernel function pointers stored at `AotContext.trampolines` (offset 72)

### Key Problem

Everything runs in Ring 0. The AOT-compiled WASM code has full kernel privileges. Host functions are direct function-pointer calls into kernel Rust code. There is no isolation boundary.

### Files Involved

| File | Role |
|---|---|
| `std/src/wasm/aot/compiler.rs` | AOT compiler — translates WASM bytecode to x86_64 |
| `std/src/wasm/aot/emitter.rs` | Low-level x86_64 code emission (REX prefixes, ModRM, relocations) |
| `std/src/wasm/aot/trampoline.rs` | 284 trampoline functions (host call dispatch, arithmetic helpers, SIMD ops) |
| `std/src/wasm/aot/runtime.rs` | `AotContext` struct (runtime context passed to every AOT function), `AotTrampoline` enum (284 variants), `AotModule` struct |
| `std/src/wasm/interpreter/store/mod.rs` | `module_instantiate_unchecked()` (AOT compilation + code slot setup), `resume_unchecked()` (AOT invocation via inline asm) |
| `std/src/wasm/runner.rs` | `run_with_buffer()` — top-level entry that validates, creates Store, instantiates, invokes |
| `std/src/wasm/wasi/preview1.rs` | WASI Preview 1 host function registration (delegates to submodules) |
| `std/src/wasm/wasi/preview2/mod.rs` | WASI Preview 2 host function registration |
| `kernel/src/syscalls/mod.rs` | Syscall dispatcher + all syscall number constants |
| `kernel/src/syscalls/process.rs` | `spawn_process()` — process creation, thread spawning |
| `kernel/src/memory/address_space.rs` | SAS layout constants, slot allocation |

---

## Current Data Structures

### AotContext (`std/src/wasm/aot/runtime.rs`)

This `#[repr(C)]` struct is passed (via RDI register) to every AOT-compiled function. It provides access to runtime state.

```rust
#[repr(C)]
pub struct AotContext {
    pub store: *mut usize,           // +0   Pointer to Store<T> (kernel heap)
    pub fuel: *mut u32,              // +8   Fuel counter pointer
    pub memory_base: *mut u8,        // +16  Linear memory base pointer
    pub memory_size: usize,          // +24  Linear memory current size
    pub stack_base: *mut u128,       // +32  AOT execution stack base
    pub locals_base: *mut u128,      // +40  Local variables area pointer
    pub module_addr: usize,          // +48  Module index in Store
    pub stack_limit: usize,          // +56  Stack overflow detection boundary
    pub trap_code: *mut i32,         // +64  Error code output (0 = ok)
    pub trampolines: *const usize,   // +72  Pointer to trampoline function table (284 entries)
}
```

**Critical**: The `store` pointer (offset 0) and `trampolines` pointer (offset 72) point into kernel memory. These cannot be dereferenced from Ring 3. This is the core thing that must change.

### AotTrampoline Enum (`std/src/wasm/aot/runtime.rs`)

284 variants (indices 0-283). Each maps to a function pointer in the trampoline table. Categories:

```
Indices 0-11:   Trap handlers (Trap, TrapOob, TrapFuel, TrapDivZero, TrapIntOverflow, TrapIndirect, TrapUnreachable, TrapStackOverflow, TrapHost, TrapUnimplementedFc, TrapUnimplementedSimd, TrapUnimplementedAtomic)
Indices 12-19:  Integer division/remainder (I32DivS, I32DivU, I32RemS, I32RemU, I64DivS, I64DivU, I64RemS, I64RemU)
Indices 20-31:  Float comparisons (F32Eq..F64Ge)
Indices 32-35:  Float min/max (F32Min, F32Max, F64Min, F64Max)
Indices 36-41:  Float conversions (F32ConvertI64U, F64ConvertI64U, I32TruncF32U, I32TruncF64U, I64TruncF32U, I64TruncF64U)
Indices 42-51:  Saturating truncations (I32TruncSatF32S..I64TruncSatF64U)
Indices 72-81:  Memory/table bulk ops (MemoryInit, DataDrop, MemoryCopy, MemoryFill, TableInit, ElemDrop, TableCopy, TableGrow, TableSize, TableFill)
Indices 82-83:  Memory size/grow (MemorySize, MemoryGrow)
Indices 84-85:  Global access (GlobalGet, GlobalSet)
Indices 86-87:  Table access (TableGet, TableSet)
Index 88:       CallIndirect
Index 89:       CallHost (the big one — dispatches all imported function calls)
Indices 90-93:  RefFunc, V128LoadLane, V128StoreLane
Indices 94-283: SIMD operations (190 variants)
```

### How Trampoline Calls Work Today

In `compiler.rs`, `emit_call_trampoline()` (line 322) emits:

```asm
mov rax, [rbp - 48]         ; Load AotContext pointer (saved on function entry)
mov rax, [rax + 72]         ; Load trampolines pointer (offset 72 in AotContext)
mov rax, [rax + idx * 8]    ; Index into table, load kernel function pointer
call rax                    ; Indirect call into Ring 0 kernel code
```

This is what must change. Instead of loading a kernel function pointer and calling it, the code must call directly to a function body that lives in the same code slot.

### Register Conventions (from `compiler.rs` function prologue, line 153)

```
RDI  = AotContext pointer (first argument, also saved to [RBP - 48])
R14  = memory_base (linear memory pointer, reloaded after host calls)
R15  = locals_base (pointer to local variable area)
RSP  = WASM operand stack pointer (grows downward, 16-byte aligned slots)
RBP  = Frame pointer. Callee-saved registers stored at:
         [RBP - 8]  = RBX
         [RBP - 16] = R12
         [RBP - 24] = R13
         [RBP - 32] = R14
         [RBP - 40] = R15
         [RBP - 48] = RDI (AotContext pointer)
```

WASM stack values are 128-bit (16 bytes each) to accommodate v128 SIMD values. Push = `sub rsp, 16; mov [rsp], value`. Pop = `mov value, [rsp]; add rsp, 16`.

### AotModule (`std/src/wasm/aot/runtime.rs`)

```rust
pub struct AotModule {
    pub code: Vec<u8>,              // Raw x86_64 machine code bytes
    pub func_offsets: Vec<usize>,   // Byte offset of each function within code
}
```

### Address Space Layout (`kernel/src/memory/address_space.rs`)

```
CODE_REGION_BASE       = 0x0000_0001_0000_0000  (4 GiB)
CODE_SLOT_SIZE         = 64 MiB per process
  Formula: code_base = CODE_REGION_BASE + slot_id * 64 MiB

STACK_REGION_BASE      = 0x0000_0041_0000_0000  (260 GiB)
STACK_SLOT_SIZE        = 2 MiB per process
  Formula: stack_top = STACK_REGION_BASE + slot_id * 2 MiB + 2 MiB

KERNEL_STACK_REGION_BASE = 0x0000_0043_0000_0000  (268 GiB)
KERNEL_STACK_SLOT_SIZE   = 128 KiB per process

LINEAR_MEMORY_BASE     = 0x0000_0043_2000_0000  (268.5 GiB)
LINEAR_MEMORY_SLOT_SIZE = 31 GiB per process
  Formula: linear_mem = LINEAR_MEMORY_BASE + slot_id * 31 GiB

MAX_SLOTS = 4096
```

Slots are allocated from a 4096-bit bitmap in `address_space.rs`. All regions share the same flat virtual address space (Single Address Space architecture — no per-process page tables).

### Existing Syscall Numbers (`kernel/src/syscalls/mod.rs`)

```
File I/O:    SYS_READ=0, SYS_WRITE=1, SYS_OPEN=2, SYS_CLOSE=3, SYS_STAT=4, SYS_FSTAT=5,
             SYS_POLL=7, SYS_LSEEK=8, SYS_IOCTL=16, SYS_PREAD64=17, SYS_PWRITE64=18,
             SYS_PIPE=22, SYS_FCNTL=72, SYS_GETDENTS=78, SYS_CHDIR=80, SYS_RENAME=82,
             SYS_MKDIR=83, SYS_RMDIR=84, SYS_CREATE=85, SYS_UNLINK=87, SYS_FTRUNCATE=77,
             SYS_LINKAT=265, SYS_SYMLINKAT=266, SYS_READLINKAT=267, SYS_UTIMENSAT=280
Memory:      SYS_MMAP=9, SYS_MUNMAP=11, SYS_BRK=12, SYS_SHM_GET=120, SYS_MMAP_FILE=121,
             SYS_SHM_MAP=122
Process:     SYS_NANOSLEEP=35, SYS_GETPID=39, SYS_EXECVE=59, SYS_EXIT=60, SYS_WAIT4=61,
             SYS_KILL=62, SYS_SPAWN_THREAD=112, SYS_THREAD_EXIT=113, SYS_SPAWN_EXT=114
Network:     SYS_SOCKET=41, SYS_CONNECT=42, SYS_ACCEPT=43, SYS_SENDTO=44, SYS_RECVFROM=45,
             SYS_BIND=49, SYS_SOCKET_CLOSE=50, SYS_LISTEN=51, SYS_TCP_SEND=52,
             SYS_TCP_RECV=53, SYS_TCP_CONNECT_FINISH=54
Window mgr:  SYS_ADD_WINDOW=100, SYS_REMOVE_WINDOW=101, SYS_UPDATE_WINDOW=102,
             SYS_UPDATE_WINDOW_AREA=103, SYS_GET_EVENTS=104, SYS_GET_MOUSE=105,
             SYS_GET_SCREEN_WIDTH=106, SYS_GET_SCREEN_HEIGHT=107
Misc:        SYS_GET_TIME=108, SYS_GET_TICKS=109, SYS_GET_DATE=115, SYS_YIELD=129,
             SYS_DEBUG_PRINT=999
Events:      SYS_WAIT_FOR_EVENT=130, SYS_REGISTER_EVENT=131, SYS_SIGNAL_EVENT=132,
             SYS_REGISTER_EVENT_QUEUE=138, SYS_DEREGISTER_EVENT_QUEUE=139
Info:        SYS_GET_PROCESS_LIST=110, SYS_GET_PROCESS_MEM=111, SYS_SET_NONBLOCK=133,
             SYS_GET_TOTAL_MEM=134, SYS_GET_USED_MEM=135, SYS_GET_VMA_DUMP=136,
             SYS_GET_SLOT_INFO=137, SYS_GET_DMESG=140
```

The syscall entry point is `syscall_entry()` in `kernel/src/syscalls/mod.rs` (line 164). It uses the `syscall`/`sysret` MSR mechanism: saves user RSP, switches to kernel stack, pushes an iret frame, calls `syscall_dispatcher()`, then `iretq` back to Ring 3. The syscall ABI is: `RAX` = syscall number, `RDI, RSI, RDX, R10, R8, R9` = arguments, return value in `RAX`.

---

## Target Architecture

### Code Slot Layout (64 MiB per process)

```
code_base + 0x0000_0000: ┌──────────────────────────────────────────┐
                         │  Ring3 Runtime Blob                      │
                         │  (precompiled .bin, ~64-256 KiB)         │
                         │                                          │
                         │  Contains:                               │
                         │  - Jump table (284 × 8 bytes at offset 0)│
                         │  - Trap handlers                         │
                         │  - Arithmetic helpers (divmod, float ops)│
                         │  - SIMD operation bodies                 │
                         │  - WASI host function stubs (syscall     │
                         │    wrappers for fd_read, fd_write, etc.) │
                         │  - KrakeOS host function stubs           │
                         │  - call_indirect dispatcher              │
                         │  - memory.grow / memory.size stubs       │
                         │  - global_get / global_set helpers       │
                         ├──────────────────────────────────────────┤
                         │  AOT-compiled WASM functions             │
                         │  (output of AotCompiler, same as today   │
                         │   but with modified trampoline calls)    │
                         │                                          │
                         │  func_0: push rbp; mov rbp, rsp; ...     │
                         │  func_1: ...                             │
                         │  ...                                     │
                         │  func_N: ...                             │
                         ├──────────────────────────────────────────┤
                         │  Data Region (page-aligned, RW|NX)       │
                         │                                          │
                         │  Ring3Context struct (see below)          │
                         │  Globals array (16 bytes per global)     │
                         │  Table 0 entries (8 bytes each)          │
                         │  Data segment scratch (init-time copy)   │
                         └──────────────────────────────────────────┘
```

Page permissions:
- Code + blob pages: `USER | PRESENT | READABLE | EXECUTABLE` (no write)
- Data region pages: `USER | PRESENT | READABLE | WRITABLE | NO_EXECUTE`

### Ring3Context (replaces AotContext)

The new context struct lives in the data region of the code slot, accessible from Ring 3. **No kernel pointers.**

```rust
#[repr(C)]
pub struct Ring3Context {
    // Same offsets as AotContext where possible to minimize compiler changes
    pub _reserved0: u64,             // +0   (was store pointer — unused in ring3)
    pub _reserved1: u64,             // +8   (was fuel pointer — unused in ring3)
    pub memory_base: *mut u8,        // +16  Linear memory base (same offset as before)
    pub memory_size: usize,          // +24  Linear memory current size (same offset)
    pub stack_base: *mut u128,       // +32  AOT execution stack base (same offset)
    pub locals_base: *mut u128,      // +40  Local variables area (same offset)
    pub _reserved2: usize,           // +48  (was module_addr — unused in ring3)
    pub stack_limit: usize,          // +56  Stack overflow boundary (same offset)
    pub trap_code: *mut i32,         // +64  Trap code output (same offset, points into data region)
    pub blob_base: u64,              // +72  Base address of blob in code slot (was trampolines)
    // --- New fields below (offsets > 72) ---
    pub globals_ptr: *mut u8,        // +80  Pointer to globals array in data region
    pub globals_count: u32,          // +88  Number of globals
    pub _pad0: u32,                  // +92
    pub table0_ptr: *mut u64,        // +96  Pointer to table 0 entries (code slot offsets)
    pub table0_size: u32,            // +104
    pub _pad1: u32,                  // +108
    pub func_table_ptr: *mut u64,    // +112 Code offsets for all functions (for call_indirect)
    pub func_count: u32,             // +120
    pub _pad2: u32,                  // +124
    pub pid: u64,                    // +128
    pub slot_id: u16,                // +136
    pub _pad3: [u8; 6],              // +138
    pub trap_code_storage: i32,      // +144 Actual trap code value (trap_code points here)
    pub _pad4: u32,                  // +148
    pub num_imported_funcs: u32,     // +152 Number of imported functions
    pub _pad5: u32,                  // +156
    pub import_stub_table: *const u64, // +160 Table mapping import index → blob stub offset
}
// Total: 168 bytes
```

**Design note**: Offsets 16, 24, 32, 40, 56, 64 are kept identical to `AotContext` so that most of the AOT compiler's existing code generation (memory access, stack limit checks, trap code checks) works unchanged. Only offset 72 changes from "trampoline table pointer" to "blob base address", and the trampoline dispatch code generation changes.

### How Trampoline Calls Change

**Today** (Ring 0, indirect call through kernel pointer table):
```asm
mov rax, [rbp - 48]         ; Load AotContext from frame
mov rax, [rax + 72]         ; Load trampolines pointer (kernel address)
mov rax, [rax + idx * 8]    ; Load kernel function pointer
call rax                    ; Indirect call to kernel code
```

**New** (Ring 3, direct call to blob function in same code slot):
```asm
mov rax, [rbp - 48]         ; Load Ring3Context from frame
mov rax, [rax + 72]         ; Load blob_base address
mov rax, [rax + idx * 8]    ; Load code offset from jump table at blob start
call rax                    ; Direct call to blob code in same code slot
```

The encoding is identical! The only difference is that offset 72 now points to the blob's jump table (in the code slot, Ring 3 accessible) instead of a kernel trampoline table. The jump table entries are absolute addresses pointing to function bodies within the blob.

**Even better optimization** — for frequently-used trampolines, emit `call rel32` directly:
```asm
call blob_func_label        ; PC-relative call, resolved at finalize()
```
This avoids the double indirection entirely. The AOT compiler knows the blob layout at compile time (the blob is embedded in the kernel as `include_bytes!`), so it can compute the relative offset from any WASM instruction to any blob function.

### How Host Function Calls Change

**Today**: `CallHost` trampoline → `aot_call_host()` in `trampoline.rs` → looks up function in Store → calls `hostcode()` Rust function → which internally calls kernel syscalls.

**New**: Each host function gets its own stub in the blob. The AOT compiler resolves import names at compile time and emits a direct call to the specific stub.

Example for WASI `fd_write`:

**Today's code path:**
```
AOT code → emit_call_trampoline(CallHost) → aot_call_host(ctx, func_idx=17, sp)
  → store.functions[func_addrs[17]] → HostFunc → hostcode(store, params)
    → krakeos::fd_write(fd, iovs_ptr, iovs_len) → SYS_WRITE syscall
```

**New code path:**
```
AOT code → call blob_wasi_fd_write(ctx, sp)
  → read params from WASM stack
  → execute SYS_WRITE syscall instruction directly
  → write results to WASM stack
  → return updated sp
```

The ABI between AOT code and blob stubs stays the same as today's `aot_call_host`:
- RDI = Ring3Context pointer
- RSI = function index (or unused for direct stubs)
- RDX = WASM stack pointer (sp, pointing to top of operand stack)
- Return value in RAX = new WASM stack pointer after results are pushed

---

## The Ring3 Runtime Blob

### What It Is

A `no_std`, `no_main` Rust crate compiled to a flat `.bin` file. Contains all the code that today lives in `trampoline.rs` plus WASI host function implementations, rewritten to use `syscall` instead of direct kernel calls.

### Why a .bin Blob Instead of Emitting Everything Through the Emitter

The existing trampoline code in `trampoline.rs` is ~1700 lines of Rust. WASI host functions add another few hundred. `aot_call_host()` alone (lines 834-928) involves parameter marshaling, type conversion, error handling, memory growth detection — that's hundreds of x86_64 instructions if hand-emitted. Writing and maintaining all of this as emitter calls would be impractical.

A compiled Rust .bin keeps the code maintainable while achieving the same result: native x86_64 code in the code slot, no kernel pointers, Ring 3 execution.

### How to Make It Position-Independent

On x86_64, code is **naturally position-independent** without any special flags:

1. All `call` and `jmp` instructions use `rel32` encoding (PC-relative)
2. All data references use `[rip + disp32]` addressing (PC-relative)
3. As long as code and rodata are contiguous (same blob), all internal references work at any load address

**Rules for the blob crate:**
- `#![no_std]`, `#![no_main]` — no OS dependencies
- **No `static mut` variables** — all mutable state goes through `Ring3Context` pointer (passed in RDI)
- `const` and `static` (immutable) data is fine — Rust/LLVM places it in `.rodata` and accesses via `[rip + disp]`
- All functions marked `#[no_mangle] pub extern "C"` for predictable symbol names
- Compiled with `-C relocation-model=static` (default for bare-metal targets)

**Linker script** (`ring3rt.ld`):
```ld
ENTRY(_blob_start)
SECTIONS {
    . = 0;
    .text : {
        KEEP(*(.jump_table))
        *(.text .text.*)
    }
    .rodata : { *(.rodata .rodata.*) }
    /DISCARD/ : { *(.eh_frame*) *(.note*) *(.comment) *(.gnu*) }
}
```

**Build command:**
```bash
cargo rustc --package=ring3-rt --target=bits64pie.json --release \
    -- -C link-arg=-Tring3rt.ld -C relocation-model=static
objcopy -O binary target/.../ring3_rt ring3_rt.bin
```

The output `.bin` is raw bytes — no ELF headers, no GOT, no PLT, no dynamic linking. Just machine code + read-only data.

### Blob Internal Layout

```
Offset 0x0000: Jump table (284 entries × 8 bytes = 2272 bytes)
  [0]  = absolute address of trap_generic handler
  [1]  = absolute address of trap_oob handler
  ...
  [12] = absolute address of i32_div_s helper
  ...
  [82] = absolute address of memory_size stub
  [83] = absolute address of memory_grow stub
  [84] = absolute address of global_get helper
  [85] = absolute address of global_set helper
  [86] = absolute address of table_get helper
  [87] = absolute address of table_set helper
  [88] = absolute address of call_indirect dispatcher
  [89] = absolute address of call_host dispatcher (or individual stubs)
  ...
  [283] = absolute address of v128_load32x2_u helper

Offset 0x08E0+: Function bodies
  trap_generic:  mov edi, 1; mov eax, SYS_EXIT; syscall; ud2
  trap_oob:      mov edi, 2; mov eax, SYS_EXIT; syscall; ud2
  ...
  i32_div_s:     <division with zero/overflow checks>
  ...
  wasi_fd_read:  <read params from stack, SYS_READ syscall, push results>
  wasi_fd_write: <read params from stack, SYS_WRITE syscall, push results>
  ...
```

**Jump table fixup**: The jump table contains absolute addresses. When the blob is compiled with base address 0, these are offsets from the blob start. At load time, the kernel adds `code_base` to each entry:
```rust
// In kernel, after copying blob to code slot:
let jump_table = code_base as *mut u64;
for i in 0..284 {
    unsafe { *jump_table.add(i) += code_base; }
}
```
This is the **only** relocation needed. 284 additions. Trivial.

### Build Integration

The blob is built by the Makefile alongside the kernel and embedded at compile time:

```rust
// In kernel or std crate:
const RING3_RT_BLOB: &[u8] = include_bytes!("../ring3_rt.bin");
```

A build script generates a Rust source file with the offset of each function in the blob:
```rust
// Generated by build.rs after compiling ring3-rt:
pub const BLOB_SIZE: usize = 65536;
pub const TRAP_GENERIC_OFF: usize = 0x08E0;
pub const TRAP_OOB_OFF: usize = 0x08F0;
// ... etc for all 284 entries
pub const WASI_FD_READ_OFF: usize = 0x1200;
pub const WASI_FD_WRITE_OFF: usize = 0x1400;
// ...
```

These offsets are obtained by parsing the ELF symbol table before stripping (the build script runs `nm` or `objdump` on the ELF, extracts symbol addresses, writes them as Rust constants).

---

## Implementation Phases

### Phase 1: Inline Globals and Tables

**Goal**: Eliminate `GlobalGet` (84), `GlobalSet` (85), `TableGet` (86), `TableSet` (87) trampoline calls by emitting direct memory access in the AOT compiler. This is a standalone optimization that works in Ring 0 too.

**Files to modify**: `std/src/wasm/aot/compiler.rs`

**Today** (global access via trampoline):
```asm
; GlobalGet(idx) — calls into kernel, which looks up Store.globals[idx]
sub rsp, 16
mov rdx, rsp
mov esi, idx
emit_call_trampoline(GlobalGet)  ; 5 instructions, indirect call
```

**New** (direct memory access):
```asm
; GlobalGet(idx) — direct load from userspace globals array
mov rax, [rbp - 48]              ; Load context
mov rax, [rax + 80]              ; Load globals_ptr (new field)
movups xmm0, [rax + idx * 16]   ; Load 128-bit global value
sub rsp, 16
movups [rsp], xmm0              ; Push to WASM stack
```

**For GlobalSet:**
```asm
movups xmm0, [rsp]              ; Pop from WASM stack
add rsp, 16
mov rax, [rbp - 48]
mov rax, [rax + 80]             ; globals_ptr
movups [rax + idx * 16], xmm0   ; Store
```

**What needs to happen:**
1. Add `globals_ptr` and `globals_count` fields to `AotContext` (or the new `Ring3Context`)
2. During `module_instantiate_unchecked()`, allocate a flat globals array in the code slot's data region (or a separate allocation), populate it with initial values from the WASM module's global section
3. Set `ctx.globals_ptr` to point at this array
4. In `compiler.rs`, when encountering `Instruction::GlobalGet(idx)` / `GlobalSet(idx)`, emit direct memory access instead of `emit_call_trampoline(GlobalGet)`

**Similarly for TableGet/TableSet**: the table becomes a flat array of function pointers (code slot offsets) in userspace memory. `table0_ptr` in the context struct points to it.

**Testing**: This can be tested in Ring 0 first. The globals array just needs to be accessible from the AOT code. In Ring 0, any pointer works.

---

### Phase 2: Inline Pure Arithmetic Helpers

**Goal**: Emit function bodies for the ~40 pure-computation trampolines directly into the code slot, either via the emitter or as part of the blob.

**Trampolines to inline** (indices 12-71, no kernel interaction needed):
- `I32DivS`, `I32DivU`, `I32RemS`, `I32RemU` (12-15)
- `I64DivS`, `I64DivU`, `I64RemS`, `I64RemU` (16-19)
- `F32Eq..F64Ge` (20-31) — float comparisons
- `F32Min`, `F32Max`, `F64Min`, `F64Max` (32-35)
- `F32ConvertI64U`, `F64ConvertI64U` (36-37)
- `I32TruncF32U..I64TruncF64U` (38-41)
- `I32TruncSatF32S..I64TruncSatF64U` (42-51)

These are pure functions — they take values from the WASM stack, compute a result, push it back. They don't touch Store, don't make syscalls, don't access globals or tables.

**Two approaches:**

**A. Emit via the existing emitter** (recommended for simple ones like divmod):
```rust
// In compiler.rs, instead of emit_call_trampoline(I32DivS):
fn emit_i32_div_s(&mut self) {
    // Pop divisor and dividend
    self.emitter.pop_wasm_stack(Reg::RBX);  // divisor
    self.emitter.pop_wasm_stack(Reg::RAX);  // dividend
    // Check for zero divisor
    self.emitter.test_r32_r32(Reg::EBX, Reg::EBX);
    self.emitter.jcc_label(0x84, self.trap_label);  // je trap
    // Check for overflow (INT_MIN / -1)
    self.emitter.cmp_r32_imm(Reg::EAX, 0x80000000u32 as i32);
    let no_overflow = self.emitter.new_label();
    self.emitter.jcc_label(0x85, no_overflow);  // jne
    self.emitter.cmp_r32_imm(Reg::EBX, -1);
    self.emitter.jcc_label(0x84, self.trap_label);  // je trap (overflow)
    self.emitter.bind_label(no_overflow);
    // Perform division
    self.emitter.emit(&[0x99]);  // cdq (sign-extend EAX → EDX:EAX)
    self.emitter.emit(&[0xF7, 0xFB]);  // idiv ebx
    self.emitter.push_wasm_stack(Reg::RAX);
}
```

**B. Include in the blob** (recommended for complex float/SIMD operations):
The blob contains the function bodies. The AOT compiler emits `call rel32` to the blob function's known offset.

---

### Phase 3: Create the ring3-rt Blob Crate

**Goal**: Create the `ring3-rt` crate skeleton, compile to `.bin`, embed in the kernel.

**New directory structure:**
```
ring3-rt/
├── Cargo.toml
├── ring3rt.ld          (linker script)
├── build.rs            (extracts symbol offsets after compilation)
└── src/
    ├── lib.rs          (blob entry, jump table definition)
    ├── context.rs      (Ring3Context struct definition)
    ├── traps.rs        (trap handlers — just call SYS_EXIT)
    ├── helpers.rs      (arithmetic: divmod, float ops, conversions)
    ├── simd.rs         (SIMD operation implementations)
    ├── memory.rs       (memory.grow, memory.size, memory.copy, memory.fill)
    ├── globals.rs      (global_get, global_set — direct memory access)
    ├── tables.rs       (table_get, table_set, call_indirect, table.grow)
    ├── wasi_fs.rs      (fd_read, fd_write, fd_close, fd_seek, path_open, etc.)
    ├── wasi_proc.rs    (proc_exit, sched_yield, clock_time_get, random_get)
    ├── wasi_env.rs     (args_get, args_sizes_get, environ_get, environ_sizes_get)
    ├── wasi_net.rs     (sock_send, sock_recv, sock_accept, etc.)
    ├── krakeos.rs      (KrakeOS-specific: window ops, spawn, debug_print, etc.)
    └── syscall.rs      (inline asm wrapper for syscall instruction)
```

**`src/syscall.rs`** — thin wrapper:
```rust
#[inline(always)]
pub unsafe fn syscall1(num: u64, arg1: u64) -> u64 {
    let ret: u64;
    core::arch::asm!(
        "syscall",
        in("rax") num,
        in("rdi") arg1,
        lateout("rax") ret,
        lateout("rcx") _,
        lateout("r11") _,
        options(nostack)
    );
    ret
}

#[inline(always)]
pub unsafe fn syscall3(num: u64, arg1: u64, arg2: u64, arg3: u64) -> u64 {
    let ret: u64;
    core::arch::asm!(
        "syscall",
        in("rax") num,
        in("rdi") arg1,
        in("rsi") arg2,
        in("rdx") arg3,
        lateout("rax") ret,
        lateout("rcx") _,
        lateout("r11") _,
        options(nostack)
    );
    ret
}

// syscall2, syscall4, syscall5, syscall6 similarly
```

**`src/traps.rs`** — trap handlers:
```rust
use crate::syscall::syscall1;

const SYS_EXIT: u64 = 60;

#[no_mangle]
pub extern "C" fn trap_generic(_ctx: &mut Ring3Context, _sp: *mut u128) -> *mut u128 {
    unsafe { syscall1(SYS_EXIT, 1); }
    unreachable!()
}

#[no_mangle]
pub extern "C" fn trap_oob(_ctx: &mut Ring3Context, _sp: *mut u128) -> *mut u128 {
    unsafe { syscall1(SYS_EXIT, 2); }
    unreachable!()
}

// ... etc for all 12 trap handlers (indices 0-11)
```

**`src/wasi_fs.rs`** — example host function stub:
```rust
use crate::context::Ring3Context;
use crate::syscall::syscall3;

const SYS_WRITE: u64 = 1;

/// WASI fd_write: write gathered buffers to a file descriptor.
/// WASM signature: (fd: i32, iovs: i32, iovs_len: i32, nwritten_ptr: i32) -> i32
#[no_mangle]
pub extern "C" fn wasi_fd_write(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        // Pop 4 params (WASM stack grows downward, params at sp[0..3])
        let nwritten_ptr = (*sp.add(0)) as u32;
        let iovs_len     = (*sp.add(1)) as u32;
        let iovs_offset  = (*sp.add(2)) as u32;
        let fd            = (*sp.add(3)) as i32;

        let mem = ctx.memory_base;
        let mut total_written: u32 = 0;

        for i in 0..iovs_len {
            let iov_addr = mem.add((iovs_offset + i * 8) as usize);
            let buf_offset = *(iov_addr as *const u32);
            let buf_len    = *(iov_addr.add(4) as *const u32);

            let buf_ptr = mem.add(buf_offset as usize);
            let ret = syscall3(SYS_WRITE, fd as u64, buf_ptr as u64, buf_len as u64);

            if ret > buf_len as u64 {
                // Error (ret is usize::MAX or similar)
                // Advance sp past 4 params, push errno
                let result_sp = sp.add(4).sub(1);
                *result_sp = 8u128; // EBADF or similar
                return result_sp;
            }
            total_written += ret as u32;
        }

        // Write nwritten to linear memory
        let nwritten_addr = mem.add(nwritten_ptr as usize) as *mut u32;
        *nwritten_addr = total_written;

        // Pop 4 params, push 1 result (errno = 0 = success)
        let result_sp = sp.add(4).sub(1);
        *result_sp = 0u128; // ESUCCESS
        result_sp
    }
}
```

**`src/lib.rs`** — jump table definition:
```rust
#![no_std]
#![no_main]

mod context;
mod syscall;
mod traps;
mod helpers;
mod simd;
mod memory;
mod globals;
mod tables;
mod wasi_fs;
mod wasi_proc;
mod wasi_env;
mod wasi_net;
mod krakeos;

// Jump table at offset 0 — 284 function pointers
// Placed in .jump_table section so the linker script puts it first
#[no_mangle]
#[link_section = ".jump_table"]
pub static JUMP_TABLE: [unsafe extern "C" fn(); 284] = [
    traps::trap_generic,           // [0]  Trap
    traps::trap_oob,               // [1]  TrapOob
    traps::trap_fuel,              // [2]  TrapFuel
    traps::trap_div_zero,          // [3]  TrapDivZero
    // ... all 284 entries matching AotTrampoline enum order ...
    helpers::i32_div_s,            // [12] I32DivS
    // ...
    memory::memory_size,           // [82] MemorySize
    memory::memory_grow,           // [83] MemoryGrow
    globals::global_get,           // [84] GlobalGet
    globals::global_set,           // [85] GlobalSet
    tables::table_get,             // [86] TableGet
    tables::table_set,             // [87] TableSet
    tables::call_indirect,         // [88] CallIndirect
    wasi_fs::call_host_dispatch,   // [89] CallHost (generic dispatcher)
    // ...
];

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    unsafe { syscall::syscall1(60, 99); } // SYS_EXIT(99)
    loop {}
}
```

**NOTE about the jump table**: This contains function pointers, which ARE absolute addresses. These need to be fixed up at load time (add `code_base` to each entry). See the "Jump table fixup" section above.

**ALTERNATIVE to the jump table**: Don't use a jump table at all. Instead, have the AOT compiler emit direct `call rel32` to specific blob functions using their known offsets (generated by build.rs). This is faster (no indirection) and avoids the fixup entirely. The jump table can be kept as a fallback for `call_indirect` into blob functions, but most trampolines would be direct calls.

---

### Phase 4: New Syscalls

Some WASI functions need kernel support that doesn't exist in the current syscall table. Add these:

| New Syscall | Number | Purpose |
|---|---|---|
| `SYS_MEMORY_GROW` | 200 | Grow linear memory by N pages. Kernel extends the mapping. Returns old page count or -1. |
| `SYS_MEMORY_SIZE` | 201 | Return current memory size in pages. |
| `SYS_ARGS_GET` | 202 | Copy argument strings to user buffer. |
| `SYS_ARGS_SIZES_GET` | 203 | Return (argc, total_argv_buf_size). |
| `SYS_ENVIRON_GET` | 204 | Copy environment strings to user buffer. |
| `SYS_ENVIRON_SIZES_GET` | 205 | Return (envc, total_env_buf_size). |
| `SYS_CLOCK_RES_GET` | 206 | Return clock resolution for clock_id. |
| `SYS_CLOCK_TIME_GET` | 207 | Return current time for clock_id. |
| `SYS_RANDOM_GET` | 208 | Fill buffer with random bytes (RDRAND). |
| `SYS_FD_PRESTAT_GET` | 209 | Get preopened fd type + name length. |
| `SYS_FD_PRESTAT_DIR_NAME` | 210 | Get preopened fd directory name. |
| `SYS_PROC_RAISE` | 211 | Raise a signal (or exit). |

These are implemented in `kernel/src/syscalls/` and added to `dispatch_syscall()`.

Some WASI functions can be handled entirely in the blob without new syscalls:
- `random_get` → use `RDRAND` instruction inline (no syscall needed)
- `sched_yield` → `SYS_YIELD` (129, already exists)
- `proc_exit` → `SYS_EXIT` (60, already exists)
- `clock_time_get` → `SYS_GET_TIME` (108) or `SYS_GET_TICKS` (109) — map WASI clock IDs to existing syscalls
- `fd_read/write/close/seek/stat` → `SYS_READ/WRITE/CLOSE/LSEEK/FSTAT` (0/1/3/8/5 — all exist)
- `path_open` → `SYS_OPEN` (2, exists)
- Most filesystem operations already have Linux-compatible syscall numbers

---

### Phase 5: AOT Compiler Modifications

**Files to modify:** `std/src/wasm/aot/compiler.rs`, `std/src/wasm/aot/runtime.rs`

**5a. Change `emit_call_trampoline()`**

The method currently emits an indirect call through the trampoline pointer table. Change it to emit a direct `call rel32` to the blob function's known offset.

```rust
fn emit_call_trampoline(&mut self, trampoline: AotTrampoline) {
    // OLD: indirect call through kernel pointer table
    // self.emitter.mov_reg_mem64(Reg::RAX, Reg::RBP, -48);
    // self.emitter.mov_reg_mem64(Reg::RAX, Reg::RAX, 72);
    // self.emitter.mov_reg_mem64(Reg::RAX, Reg::RAX, (trampoline as i32) * 8);
    // self.emitter.call_reg(Reg::RAX);

    // NEW: direct call to blob function (rel32, resolved at finalize)
    let blob_label = self.blob_func_labels[trampoline as usize];
    self.emitter.call_label(blob_label);
}
```

The `blob_func_labels` array is set up during compiler initialization. Each label maps to a fixed offset = `blob_func_offset[trampoline_idx]` (from build-generated constants). Since the blob is placed at the start of the code slot and WASM functions come after, the emitter can compute `rel32 = blob_offset - current_emit_position` during `finalize()`.

**5b. Change `CallHost` handling**

Today, ALL imported function calls go through one generic `CallHost` trampoline (index 89), which looks up the function in the Store by index.

New approach: the compiler knows which import index maps to which WASI/KrakeOS function (from the module's import section). At compile time, resolve each import to a specific blob stub:

```rust
// During compilation, when encountering Call(func_idx) where func_idx < num_imports:
fn emit_host_call(&mut self, import_idx: usize) {
    let import = &self.validation_info.imports[import_idx];
    let blob_offset = match (import.module, import.name) {
        ("wasi_snapshot_preview1", "fd_write") => WASI_FD_WRITE_OFF,
        ("wasi_snapshot_preview1", "fd_read")  => WASI_FD_READ_OFF,
        ("wasi_snapshot_preview1", "proc_exit") => WASI_PROC_EXIT_OFF,
        // ... etc for all ~43 WASI preview1 functions
        // ... and ~40 KrakeOS-specific functions
        _ => GENERIC_CALL_HOST_OFF, // fallback for unknown imports
    };

    // Emit: push params to expected locations, call blob stub
    self.emitter.sub_reg_imm32(Reg::RSP, 16); // align
    self.emitter.mov_reg_reg(Reg::RDX, Reg::RSP); // sp
    self.emitter.add_reg_imm32(Reg::RDX, 16 + reserve); // adjust for params
    self.emitter.mov_reg_imm32(Reg::RSI, import_idx as i32); // func_idx
    // Direct call to blob stub
    self.emitter.call_label(self.blob_func_labels[blob_offset_to_label]);
    // ... restore context, check trap_code (same as today)
}
```

**5c. Update AotContext → Ring3Context**

Change the `AotContext` struct definition in `runtime.rs` to `Ring3Context` with the new fields. Update the offset constants used in `compiler.rs` (offsets 16, 24, 32, 40, 56, 64 stay the same; offset 72 changes meaning from "trampoline table" to "blob base").

---

### Phase 6: Module Instantiation Changes

**File:** `std/src/wasm/interpreter/store/mod.rs`, function `module_instantiate_unchecked()`

**Today** (lines 314-356):
1. AOT compile → get `AotModule` (code bytes + func_offsets)
2. Copy code to code slot at current offset
3. Set `aot_ptr` for each function

**New:**
1. Copy blob to code slot at offset 0 (if not already there)
2. Fix up blob jump table (add code_base to each entry)
3. AOT compile → get `AotModule`
4. Copy WASM functions to code slot after the blob
5. Set up data region after WASM functions:
   - Allocate and populate Ring3Context
   - Copy global initial values into globals array
   - Build function table (code slot offsets for each function, for call_indirect)
   - Copy table element initializers
   - Copy data segment contents into linear memory
6. Record entry point offset

All of this happens in the kernel, still Ring 0. The code slot is populated before execution begins.

---

### Phase 7: Ring 3 Task Launch

**File:** `kernel/src/syscalls/process.rs` and/or `kernel/src/task/`

**Today**: `spawn_process()` creates a kernel thread that calls `run_with_buffer()` → validates WASM → compiles → invokes via `call aot_ptr` in Ring 0.

**New**: After compilation and code slot setup, create a **Ring 3 task** instead of a kernel thread.

```rust
fn spawn_ring3_wasm(slot_id: u16, code_base: u64, entry_offset: usize,
                    stack_top: u64, ctx_ptr: u64) {
    // Create a new thread with Ring 3 context
    let mut state = CPUState::default();

    // Ring 3 selectors (GDT entries for user code/data)
    state.cs = 0x23;       // User code segment (ring 3)
    state.ss = 0x1B;       // User data segment (ring 3)

    // Entry point: blob + WASM code, at the _start function
    state.rip = code_base + RING3_RT_BLOB.len() as u64 + entry_offset as u64;

    // User stack
    state.rsp = stack_top;

    // Pass Ring3Context pointer as first argument
    state.rdi = ctx_ptr;

    // Enable interrupts
    state.rflags = 0x202;  // IF flag set

    // Register the task with the scheduler
    // The scheduler's context switch will iretq into Ring 3
    let mut tm = TASK_MANAGER.int_lock();
    tm.add_ring3_task(pid, state);
}
```

The scheduler's existing context switch mechanism uses `iretq`, which naturally transitions to Ring 3 when CS has ring 3 bits set. The `syscall_entry` function in `kernel/src/syscalls/mod.rs` already handles the Ring 3 → Ring 0 transition for syscalls and the Ring 0 → Ring 3 return via `iretq`.

**Page permissions**: Before launching, the kernel must set the page table flags:
- Code slot code pages: `PRESENT | USER_ACCESSIBLE | WRITABLE` (writable only during setup, then changed to `PRESENT | USER_ACCESSIBLE` after code is written — or left writable if W^X isn't enforced yet)
- Code slot data pages: `PRESENT | USER_ACCESSIBLE | WRITABLE | NO_EXECUTE`
- Stack pages: `PRESENT | USER_ACCESSIBLE | WRITABLE | NO_EXECUTE`
- Linear memory pages: `PRESENT | USER_ACCESSIBLE | WRITABLE | NO_EXECUTE` (demand-paged via page fault handler, which already exists in `kernel/src/arch/x86_64/exceptions.rs`)

---

### Phase 8: Testing and Cutover

**8a. Test in Ring 0 first**: Before switching to Ring 3, test the blob + modified AOT compiler in Ring 0. Replace the trampoline table with the blob's jump table. All host function calls now go through blob stubs → syscalls → kernel. This validates the blob implementation without changing the execution ring.

**8b. Test trivial Ring 3**: Create a minimal WASM module that just calls `proc_exit(0)`. Compile it, set up Ring 3 task, verify it runs and exits cleanly.

**8c. Test I/O**: WASM module that writes "hello" to stdout. Validates fd_write blob stub → SYS_WRITE syscall path.

**8d. Test windowing**: WASM module that creates a window. Validates the full KrakeOS host function path (add_window, get_events, update_window).

**8e. Full cutover**: Run the existing apps (shell, term, taskbar, sysmon) via Ring 3 AOT. Compare behavior with Ring 0 path. Once stable, remove the old Ring 0 WASM execution path.

---

## Summary of All Changes by File

| File | What Changes |
|---|---|
| `ring3-rt/` (NEW) | Entire new crate — blob with all trampoline bodies + WASI stubs |
| `std/src/wasm/aot/runtime.rs` | `AotContext` → `Ring3Context`, add globals/table/blob fields |
| `std/src/wasm/aot/compiler.rs` | `emit_call_trampoline()` → direct rel32 calls. Host call resolution at compile time. Inline globals/tables. |
| `std/src/wasm/aot/emitter.rs` | Possibly add `call_label()` relocation variant if not already present |
| `std/src/wasm/interpreter/store/mod.rs` | `module_instantiate_unchecked()` — copy blob, set up data region, populate context/globals/tables. `resume_unchecked()` — replaced by Ring 3 task launch. |
| `kernel/src/syscalls/mod.rs` | Add new syscall constants (200-211) and dispatch entries |
| `kernel/src/syscalls/process.rs` | Add `spawn_ring3_wasm()` function |
| `kernel/src/syscalls/memory.rs` | Add `handle_memory_grow()`, `handle_memory_size()` |
| `kernel/src/syscalls/misc.rs` | Add clock, random, environ, args syscall handlers |
| `kernel/src/memory/address_space.rs` | Possibly adjust code slot layout for blob + data region |
| `kernel/src/task/` | Add Ring 3 task creation support (user CS/SS, iretq) |
| `Makefile` | Add ring3-rt build step |

---

## What Stays the Same

- WASM validation (`std/src/wasm/common/validation/`)
- AOT code generation for WASM instructions (add, sub, load, store, call, branch, etc.)
- x86_64 emitter infrastructure (`emitter.rs`)
- Syscall entry/exit mechanism (`syscall_entry`/`iretq`)
- SAS memory layout (code slots, stack slots, linear memory slots)
- Page fault handler for demand paging
- Window manager, compositor, scheduler, VFS — all kernel-side, unchanged
- The `.wasm` binary format — no new file formats needed

## What Gets Removed (Eventually)

- `trampoline.rs` — the 284 kernel function pointer implementations (replaced by blob)
- `aot_call_host()` — the generic host function dispatcher (replaced by direct blob stubs)
- The Ring 0 WASM execution path in `resume_unchecked()` — replaced by Ring 3 task launch
- The `Store<T>` runtime object — no longer needed at execution time (only during compilation/instantiation)
- All host function registration via the linker (`create_wasi_imports`, etc.) — replaced by compile-time import resolution
