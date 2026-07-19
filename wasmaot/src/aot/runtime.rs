//! AOT module runtime: validates + compiles a wasm module to RV64 machine
//! code, lays out linear memory / globals / tables, and drives execution.
//!
//! The generated code is written into a heap buffer. In KrakeOS the
//! wasm_runner's heap is mapped RWX (sbrk pages carry PTE_X), so the buffer
//! is directly executable after an `fence.i`.
//!
//! WASI preview 1 (and any other host import) is provided by the caller as a
//! single dispatch callback; see `HostDispatch`.

use alloc::alloc::{alloc, Layout};
use alloc::vec;
use alloc::vec::Vec;

use crate::aot::compiler::{trap, AotCompiler};
use crate::common::reader::types::data::DataMode;
use crate::common::reader::types::element::{ElemItems, ElemMode};
use crate::common::reader::types::export::ExportDesc;
use crate::common::reader::types::global::Global;
use crate::common::reader::WasmReader;
use crate::common::validation::ValidationInfo;

const PAGE_SIZE: u64 = 65536;
const WASM_STACK_SIZE: usize = 8 * 1024 * 1024; // native stack for wasm frames
const WASM_OPSTACK_SIZE: usize = 4 * 1024 * 1024; // operand stack (grows down)

/// Execution context handed to generated code. Field offsets MUST match
/// `compiler::ctx_off`.
#[repr(C)]
pub struct AotContext {
    pub mem_base: *mut u8,       // 0
    pub mem_size: u64,           // 8
    pub trap_code: u32,          // 16
    pub _pad0: u32,              // 20
    pub host_dispatch: usize,    // 24
    pub saved_sp: u64,           // 32
    pub stack_limit: u64,        // 40
    pub globals_ptr: *mut u64,   // 48
    pub table_ptr: *mut u64,     // 56
    pub table_len: u64,          // 64
    pub func_addrs: *mut u64,    // 72
    pub func_type_ids: *mut u32, // 80
    pub user_data: usize,        // 88 — opaque, for the host dispatcher
    pub mem_cap: u64,            // 96 — backing capacity in bytes
}

/// Host import dispatcher, called from generated import thunks:
///   extern "C" fn(ctx, import_idx, wsp) -> 0 on success, nonzero to trap.
/// Reads params from `wsp` (param i at wsp[n-1-i]) and writes results to the
/// same top slots (result j at wsp[n-1-j]); the thunk adjusts wsp by (n-m).
pub type HostDispatch =
    extern "C" fn(ctx: *mut AotContext, import_idx: u64, wsp: *mut u64) -> u64;

type EntryFn = extern "C" fn(
    ctx: *mut AotContext,
    wsp: *mut u64,
    func_addr: usize,
    native_stack_top: usize,
) -> *mut u64;

pub struct AotModule {
    // Kept alive for the process lifetime; raw pointers into these back the
    // context handed to generated code.
    _code: AlignedCode,
    _memory: Vec<u8>,
    _globals: Vec<u64>,
    _table: Vec<u64>,
    _func_addrs: Vec<u64>,
    _func_type_ids: Vec<u32>,
    _op_stack: Vec<u64>,
    _native_stack: Vec<u8>,

    ctx: AotContext,
    entry_ptr: usize,
    /// (name, func code address) for every exported function.
    exports: Vec<(alloc::string::String, usize)>,
    /// (module, name) of each imported function, in import-index order.
    func_imports: Vec<(alloc::string::String, alloc::string::String)>,
    op_stack_top: *mut u64,
    native_stack_top: usize,
}

/// 16-byte-aligned executable code buffer.
struct AlignedCode {
    ptr: *mut u8,
    layout: Layout,
}

impl AlignedCode {
    fn new(code: &[u8]) -> Self {
        let layout = Layout::from_size_align(code.len().max(16), 16).unwrap();
        let ptr = unsafe { alloc(layout) };
        assert!(!ptr.is_null(), "AOT code alloc failed");
        unsafe {
            core::ptr::copy_nonoverlapping(code.as_ptr(), ptr, code.len());
            // I-cache/pipeline sync: the buffer was just written as data.
            core::arch::asm!("fence.i");
        }
        AlignedCode { ptr, layout }
    }
    fn base(&self) -> usize {
        self.ptr as usize
    }
}

impl Drop for AlignedCode {
    fn drop(&mut self) {
        unsafe { alloc::alloc::dealloc(self.ptr, self.layout) }
    }
}

/// Why compilation/instantiation failed.
#[derive(Debug)]
pub enum AotError {
    Validation,
    /// Module imports a memory/table/global — unsupported (apps are
    /// self-contained; only function imports are allowed).
    UnsupportedImport,
}

impl AotModule {
    /// Validate + compile + instantiate. `host_dispatch` handles imports;
    /// `user_data` is stashed in the context for it.
    pub fn instantiate(
        wasm: &[u8],
        host_dispatch: HostDispatch,
        user_data: usize,
    ) -> Result<AotModule, AotError> {
        let validation_info =
            crate::common::validation::validate(wasm).map_err(|_| AotError::Validation)?;

        // Only function imports are supported.
        if validation_info.imports_length.imported_memories != 0
            || validation_info.imports_length.imported_tables != 0
            || validation_info.imports_length.imported_globals != 0
        {
            return Err(AotError::UnsupportedImport);
        }

        // ── compile ──
        let compiler = AotCompiler::new(&validation_info);
        let artifact = compiler.compile_module();
        let code = AlignedCode::new(&artifact.code);
        let code_base = code.base();

        // ── linear memory ──
        // The guest allocator (dlmalloc) grows memory on demand via
        // `memory.grow`; we don't support in-place growth, so instead we
        // allocate a generous fixed region up front and report its full size
        // as the current size. The guest then uses the whole region without
        // ever needing to grow. Sized from the module's declared max (when
        // present) else a default, clamped to a per-app ceiling.
        // Every app gets a flat backing capacity it can grow into. We ignore
        // the module's declared max (our apps declare 32 MiB, too small for
        // the shell's font + framebuffers); allowing growth past a
        // self-imposed max is invisible to a correct program. 80 MiB keeps
        // the shell + two apps inside the runner's 288 MiB heap.
        const FLAT_PAGES: u64 = 1280; // 80 MiB
        let mem = validation_info.memories.first();
        let min_pages = mem.map(|m| m.limits.min as u64).unwrap_or(1);
        let cap_pages = min_pages.max(FLAT_PAGES);
        let cap_bytes = (cap_pages * PAGE_SIZE) as usize;
        let mut memory = vec![0u8; cap_bytes];
        // Logical current size starts at the module's initial pages; the
        // guest allocator grows it toward the backing capacity via memory.grow.
        let initial_bytes = (min_pages * PAGE_SIZE) as u64;

        // ── globals ──
        let mut globals: Vec<u64> = Vec::with_capacity(validation_info.globals.len());
        for g in &validation_info.globals {
            let v = eval_const_global(&validation_info, g, &globals);
            globals.push(v);
        }

        // ── tables (single funcref table; entries = func_idx + 1) ──
        let table_len = validation_info
            .tables
            .first()
            .map(|t| t.lim.min as usize)
            .unwrap_or(0);
        let mut table: Vec<u64> = vec![0; table_len];

        // ── active data segments (loaded into the initial region) ──
        for seg in &validation_info.data {
            if let DataMode::Active(active) = &seg.mode {
                let offset = eval_const_i32(&validation_info, active.offset) as usize;
                let end = offset + seg.init.len();
                if end <= memory.len() {
                    memory[offset..end].copy_from_slice(&seg.init);
                }
            }
        }
        let _ = initial_bytes;

        // ── active element segments (fill table with func_idx + 1) ──
        for elem in &validation_info.elements {
            if let ElemMode::Active(active) = &elem.mode {
                let base = eval_const_i32(&validation_info, active.init_expr) as usize;
                match &elem.init {
                    ElemItems::RefFuncs(func_idxs) => {
                        for (i, &fidx) in func_idxs.iter().enumerate() {
                            if base + i < table.len() {
                                table[base + i] = (fidx as u64) + 1;
                            }
                        }
                    }
                    ElemItems::Exprs(_, spans) => {
                        for (i, &span) in spans.iter().enumerate() {
                            if base + i < table.len() {
                                table[base + i] = eval_const_ref(&validation_info, span);
                            }
                        }
                    }
                }
            }
        }

        // ── function address / type-id tables ──
        let mut func_addrs: Vec<u64> = Vec::with_capacity(artifact.func_offsets.len());
        for &off in &artifact.func_offsets {
            func_addrs.push((code_base + off) as u64);
        }
        let mut func_type_ids = artifact.func_canon_type_ids.clone();

        // ── native + operand stacks ──
        let mut native_stack = vec![0u8; WASM_STACK_SIZE];
        let native_base = native_stack.as_mut_ptr() as usize;
        let native_stack_top = native_base + WASM_STACK_SIZE;
        // guard margin: traps if wasm frames descend within 64 KiB of base
        let stack_limit = (native_base + 0x10000) as u64;

        let mut op_stack = vec![0u64; WASM_OPSTACK_SIZE / 8];
        let op_stack_top = unsafe { op_stack.as_mut_ptr().add(op_stack.len()) };

        // ── exports ──
        let mut exports = Vec::new();
        for exp in &validation_info.exports {
            if let ExportDesc::FuncIdx(fidx) = exp.desc {
                exports.push((
                    exp.name.clone(),
                    (code_base + artifact.func_offsets[fidx]) as usize,
                ));
            }
        }

        // ── imported functions, in import-index order ──
        use crate::common::reader::types::import::ImportDesc;
        let mut func_imports = Vec::new();
        for imp in &validation_info.imports {
            if let ImportDesc::Func(_) = imp.desc {
                func_imports.push((imp.module_name.clone(), imp.name.clone()));
            }
        }

        let mem_base = memory.as_mut_ptr();
        let backing = memory.len() as u64;
        let ctx = AotContext {
            mem_base,
            mem_size: initial_bytes,
            trap_code: 0,
            _pad0: 0,
            host_dispatch: host_dispatch as usize,
            saved_sp: 0,
            stack_limit,
            globals_ptr: globals.as_mut_ptr(),
            table_ptr: table.as_mut_ptr(),
            table_len: table.len() as u64,
            func_addrs: func_addrs.as_mut_ptr(),
            func_type_ids: func_type_ids.as_mut_ptr(),
            user_data,
            mem_cap: backing,
        };

        Ok(AotModule {
            _code: code,
            _memory: memory,
            _globals: globals,
            _table: table,
            _func_addrs: func_addrs,
            _func_type_ids: func_type_ids,
            _op_stack: op_stack,
            _native_stack: native_stack,
            ctx,
            entry_ptr: code_base + artifact.entry_offset,
            exports,
            func_imports,
            op_stack_top,
            native_stack_top,
        })
    }

    pub fn find_export(&self, name: &str) -> Option<usize> {
        self.exports.iter().find(|(n, _)| n == name).map(|(_, a)| *a)
    }

    /// Imported functions in import-index order (the index passed to the
    /// host dispatcher).
    pub fn func_imports(&self) -> &[(alloc::string::String, alloc::string::String)] {
        &self.func_imports
    }

    /// Set the opaque pointer the host dispatcher receives via `ctx.user_data`.
    pub fn set_user_data(&mut self, ud: usize) {
        self.ctx.user_data = ud;
    }

    /// Direct pointer into linear memory (for the host to read/write guest
    /// buffers by absolute offset).
    pub fn memory_ptr(&self) -> *mut u8 {
        self.ctx.mem_base
    }
    pub fn memory_len(&self) -> usize {
        self.ctx.mem_size as usize
    }

    pub fn ctx_ptr(&mut self) -> *mut AotContext {
        &mut self.ctx as *mut AotContext
    }

    /// Call an exported function that takes no params and no results
    /// (e.g. WASI `_start`). Returns `Ok(())` on clean return, `Err(code)`
    /// on trap.
    pub fn call_void(&mut self, func_addr: usize) -> Result<(), u32> {
        self.ctx.trap_code = 0;
        let entry: EntryFn = unsafe { core::mem::transmute(self.entry_ptr) };
        let ctx_ptr = &mut self.ctx as *mut AotContext;
        unsafe {
            entry(ctx_ptr, self.op_stack_top, func_addr, self.native_stack_top);
        }
        if self.ctx.trap_code == 0 {
            Ok(())
        } else {
            Err(self.ctx.trap_code)
        }
    }

    pub fn trap_name(code: u32) -> &'static str {
        match code {
            trap::NONE => "none",
            trap::GENERIC => "generic",
            trap::OOB => "out-of-bounds",
            trap::DIV_ZERO => "divide-by-zero",
            trap::INT_OVERFLOW => "integer-overflow",
            trap::INDIRECT => "indirect-call",
            trap::UNREACHABLE => "unreachable",
            trap::STACK_OVERFLOW => "stack-overflow",
            trap::UNIMPLEMENTED => "unimplemented-op",
            _ => "host",
        }
    }
}

// ── constant-expression evaluation ──────────────────────────────────
// p1 const exprs are one value-producing instruction followed by `end`.

fn eval_const_i32(vi: &ValidationInfo, span: crate::common::reader::span::Span) -> i32 {
    eval_const_raw(vi, span, &[]) as i32
}

fn eval_const_ref(vi: &ValidationInfo, span: crate::common::reader::span::Span) -> u64 {
    eval_const_raw(vi, span, &[])
}

fn eval_const_global(vi: &ValidationInfo, g: &Global, prior: &[u64]) -> u64 {
    eval_const_raw(vi, g.init_expr, prior)
}

/// Evaluate a constant expression from its span. `prior_globals` lets a
/// `global.get` reference an already-initialized (non-imported) global.
fn eval_const_raw(
    vi: &ValidationInfo,
    span: crate::common::reader::span::Span,
    prior_globals: &[u64],
) -> u64 {
    use crate::common::reader::types::opcode::*;
    let mut reader = WasmReader::new(vi.wasm);
    reader.pc = span.from;
    let opcode = match reader.read_u8() {
        Ok(b) => b,
        Err(_) => return 0,
    };
    match opcode {
        I32_CONST => reader.read_var_i32().unwrap_or(0) as u32 as u64,
        I64_CONST => reader.read_var_i64().unwrap_or(0) as u64,
        F32_CONST => reader.read_f32().unwrap_or(0) as u64,
        F64_CONST => reader.read_f64().unwrap_or(0),
        GLOBAL_GET => {
            let idx = reader.read_var_u32().unwrap_or(0) as usize;
            prior_globals.get(idx).copied().unwrap_or(0)
        }
        REF_NULL => 0,
        REF_FUNC => {
            let idx = reader.read_var_u32().unwrap_or(0);
            (idx as u64) + 1
        }
        _ => 0,
    }
}
