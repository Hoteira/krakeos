//! WASM → RV64GC AOT compiler, ported from the x86_64 compiler in
//! ref/std/src/wasm/aot/compiler.rs with a simplified ABI (no ring-3
//! machinery):
//!
//! Register roles (all stable for a whole module invocation):
//!   s1 = context pointer          s2 = linear memory base
//!   s3 = locals base (per frame)  s4 = wasm operand stack ptr (grows down)
//!   s5 = linear memory size       t0-t2 = slow-path scratch
//!   t6 = emitter reloc scratch    a0/a1 = host-call args
//!   a2-a7,t3-t5 = GP value cache  fa0-fa7,ft2-ft7 = FP value cache
//!
//! The wasm operand stack lives in its own memory region (8 bytes/slot),
//! separate from the native stack, and is continuous across wasm calls:
//! callees read their args in place and leave results in place. Traps
//! longjmp back to the entry stub via ctx.saved_sp.

use alloc::vec::Vec;

use crate::aot::emitter::{cond, rm, FReg, Reg, Rv64Emitter};
use crate::common::assert_validated::UnwrapValidatedExt;
use crate::common::reader::types::instruction::Instruction;
use crate::common::reader::types::memarg::MemArg;
use crate::common::reader::types::{BlockType, FuncType, ValType};
use crate::common::reader::{WasmReadable, WasmReader};
use crate::common::validation::ValidationInfo;

/// Context field offsets — must match `AotContext` in runtime.rs.
pub mod ctx_off {
    pub const MEM_BASE: i32 = 0;
    pub const MEM_SIZE: i32 = 8;
    pub const TRAP_CODE: i32 = 16;
    pub const HOST_DISPATCH: i32 = 24;
    pub const SAVED_SP: i32 = 32;
    pub const STACK_LIMIT: i32 = 40;
    pub const GLOBALS_PTR: i32 = 48;
    pub const TABLE_PTR: i32 = 56;
    pub const TABLE_LEN: i32 = 64;
    pub const FUNC_ADDRS: i32 = 72;
    pub const FUNC_TYPE_IDS: i32 = 80;
    pub const USER_DATA: i32 = 88;
    /// Backing capacity in bytes; memory.grow succeeds up to this.
    pub const MEM_CAP: i32 = 96;
}

/// Trap cause codes written to ctx.trap_code.
pub mod trap {
    pub const NONE: u32 = 0;
    pub const GENERIC: u32 = 1;
    pub const OOB: u32 = 2;
    pub const DIV_ZERO: u32 = 3;
    pub const INT_OVERFLOW: u32 = 4;
    pub const INDIRECT: u32 = 5;
    pub const UNREACHABLE: u32 = 6;
    pub const STACK_OVERFLOW: u32 = 7;
    pub const UNIMPLEMENTED: u32 = 8;
    // Codes >= 100 are set by the host dispatcher itself (e.g. proc_exit).
}

pub struct ControlBlock {
    pub kind: ControlBlockKind,
    /// Operand-stack depth at block entry, EXCLUDING block params.
    pub stack_depth_before: usize,
    pub param_count: usize,
    pub result_count: usize,
    pub end_label: usize,
    pub else_label: Option<usize>,
    pub start_label: Option<usize>,
}

#[derive(PartialEq)]
pub enum ControlBlockKind {
    Block,
    Loop,
    If,
    Func,
}

/// Where a cached operand-stack value currently lives (virtual stack).
#[derive(Clone, Copy)]
pub enum VLoc {
    Gp(Reg),
    Fp(FReg),
}

const GP_POOL: [Reg; 9] = [
    Reg::A2, Reg::A3, Reg::A4, Reg::A5, Reg::A6, Reg::A7,
    Reg::T3, Reg::T4, Reg::T5,
];

const FP_POOL: [FReg; 14] = [
    FReg::Fa0, FReg::Fa1, FReg::Fa2, FReg::Fa3, FReg::Fa4, FReg::Fa5,
    FReg::Fa6, FReg::Fa7, FReg::Ft2, FReg::Ft3, FReg::Ft4, FReg::Ft5,
    FReg::Ft6, FReg::Ft7,
];

/// Compiler output consumed by the runtime.
pub struct AotArtifact {
    pub code: Vec<u8>,
    /// Code offset of every function (imports = their host-call thunks).
    pub func_offsets: Vec<usize>,
    /// Code offset of the entry stub:
    /// extern "C" fn(ctx: *mut AotContext, wsp: *mut u64, func: usize) -> *mut u64
    pub entry_offset: usize,
    /// Canonical (structurally deduped) type id per function index.
    pub func_canon_type_ids: Vec<u32>,
}

pub struct AotCompiler<'a> {
    pub validation_info: &'a ValidationInfo<'a>,
    pub emitter: Rv64Emitter,
    control_stack: Vec<ControlBlock>,
    stack_depth: usize,
    func_labels: Vec<usize>,
    // trap stubs
    trap_oob: usize,
    trap_div_zero: usize,
    trap_int_overflow: usize,
    trap_indirect: usize,
    trap_unreachable: usize,
    trap_stack_overflow: usize,
    trap_unimplemented: usize,
    /// Trap path for host-signalled traps (trap_code already set by host).
    trap_from_host: usize,
    exit_restore: usize,
    // shared helpers
    helper_popcnt: usize,
    helper_memmove: usize,
    helper_memset: usize,
    result_count: usize,
    vstack: Vec<VLoc>,
    local_types: Vec<ValType>,
    canon_type_ids: Vec<u32>,
}

fn types_structurally_equal(a: &FuncType, b: &FuncType) -> bool {
    a.params.valtypes == b.params.valtypes && a.returns.valtypes == b.returns.valtypes
}

impl<'a> AotCompiler<'a> {
    pub fn new(validation_info: &'a ValidationInfo<'a>) -> Self {
        let mut emitter = Rv64Emitter::new();
        let total_funcs =
            validation_info.imports_length.imported_functions + validation_info.functions.len();
        let mut func_labels = Vec::with_capacity(total_funcs);
        for _ in 0..total_funcs {
            func_labels.push(emitter.new_label());
        }

        // Canonicalize type indices so call_indirect type checks succeed for
        // structurally equal but distinct type entries.
        let mut canon_type_ids: Vec<u32> = Vec::with_capacity(validation_info.types.len());
        for (i, ty) in validation_info.types.iter().enumerate() {
            let mut canon = i as u32;
            for j in 0..i {
                if types_structurally_equal(ty, &validation_info.types[j]) {
                    canon = canon_type_ids[j];
                    break;
                }
            }
            canon_type_ids.push(canon);
        }

        let trap_oob = emitter.new_label();
        let trap_div_zero = emitter.new_label();
        let trap_int_overflow = emitter.new_label();
        let trap_indirect = emitter.new_label();
        let trap_unreachable = emitter.new_label();
        let trap_stack_overflow = emitter.new_label();
        let trap_unimplemented = emitter.new_label();
        let trap_from_host = emitter.new_label();
        let exit_restore = emitter.new_label();
        let helper_popcnt = emitter.new_label();
        let helper_memmove = emitter.new_label();
        let helper_memset = emitter.new_label();

        Self {
            validation_info,
            emitter,
            control_stack: Vec::new(),
            stack_depth: 0,
            func_labels,
            trap_oob,
            trap_div_zero,
            trap_int_overflow,
            trap_indirect,
            trap_unreachable,
            trap_stack_overflow,
            trap_unimplemented,
            trap_from_host,
            exit_restore,
            helper_popcnt,
            helper_memmove,
            helper_memset,
            result_count: 0,
            vstack: Vec::new(),
            local_types: Vec::new(),
            canon_type_ids,
        }
    }

    pub fn compile_module(mut self) -> AotArtifact {
        let imported = self.validation_info.imports_length.imported_functions;
        let total = imported + self.validation_info.functions.len();

        // ── entry stub ──────────────────────────────────────────────
        // extern "C" fn(ctx: a0, wsp: a1, func: a2, native_stack_top: a3)
        //   -> new wsp (a0)
        // Switches to a dedicated native stack (a3) so wasm locals/frames
        // never touch the Rust caller's stack; the caller's sp is stashed in
        // the frame and restored on return or trap.
        let entry_offset = self.emitter.code.len();
        {
            let e = &mut self.emitter;
            e.mv(Reg::T0, Reg::Sp); // caller sp
            e.mv(Reg::Sp, Reg::A3); // switch to wasm native stack
            e.addi(Reg::Sp, Reg::Sp, -64);
            e.sd(Reg::Sp, 0, Reg::Ra);
            e.sd(Reg::Sp, 8, Reg::S1);
            e.sd(Reg::Sp, 16, Reg::S2);
            e.sd(Reg::Sp, 24, Reg::S3);
            e.sd(Reg::Sp, 32, Reg::S4);
            e.sd(Reg::Sp, 40, Reg::S5);
            e.sd(Reg::Sp, 48, Reg::T0); // caller sp
            e.mv(Reg::S1, Reg::A0);
            e.ld(Reg::S2, Reg::S1, ctx_off::MEM_BASE);
            e.ld(Reg::S5, Reg::S1, ctx_off::MEM_SIZE);
            e.mv(Reg::S4, Reg::A1);
            e.sd(Reg::S1, ctx_off::SAVED_SP, Reg::Sp);
            e.jalr(Reg::Ra, Reg::A2, 0);
        }
        // fall through to exit_restore
        {
            let er = self.exit_restore;
            self.emitter.bind_label(er);
            let e = &mut self.emitter;
            e.mv(Reg::A0, Reg::S4);
            e.ld(Reg::Ra, Reg::Sp, 0);
            e.ld(Reg::S1, Reg::Sp, 8);
            e.ld(Reg::S2, Reg::Sp, 16);
            e.ld(Reg::S3, Reg::Sp, 24);
            e.ld(Reg::S4, Reg::Sp, 32);
            e.ld(Reg::S5, Reg::Sp, 40);
            e.ld(Reg::T0, Reg::Sp, 48); // caller sp
            e.mv(Reg::Sp, Reg::T0);
            e.ret();
        }

        // ── trap stubs (set code, longjmp to entry frame) ───────────
        let traps = [
            (self.trap_oob, trap::OOB),
            (self.trap_div_zero, trap::DIV_ZERO),
            (self.trap_int_overflow, trap::INT_OVERFLOW),
            (self.trap_indirect, trap::INDIRECT),
            (self.trap_unreachable, trap::UNREACHABLE),
            (self.trap_stack_overflow, trap::STACK_OVERFLOW),
            (self.trap_unimplemented, trap::UNIMPLEMENTED),
        ];
        for (label, code) in traps {
            self.emitter.bind_label(label);
            self.emitter.li(Reg::T0, code as i64);
            self.emitter.sw(Reg::S1, ctx_off::TRAP_CODE, Reg::T0);
            let tfh = self.trap_from_host;
            self.emitter.jmp_label(tfh);
        }
        // host already set trap_code; just unwind
        {
            let tfh = self.trap_from_host;
            self.emitter.bind_label(tfh);
            self.emitter.ld(Reg::Sp, Reg::S1, ctx_off::SAVED_SP);
            let er = self.exit_restore;
            self.emitter.jmp_label(er);
        }

        // ── shared helpers ──────────────────────────────────────────
        self.emit_helper_popcnt();
        self.emit_helper_memmove();
        self.emit_helper_memset();

        // ── import thunks ───────────────────────────────────────────
        let mut func_offsets = Vec::with_capacity(total);
        for i in 0..imported {
            self.emitter.bind_label(self.func_labels[i]);
            func_offsets.push(self.emitter.code.len());
            let type_idx = self.validation_info.functions_types[i];
            let ft = &self.validation_info.types[type_idx];
            let n = ft.params.valtypes.len() as i64;
            let m = ft.returns.valtypes.len() as i64;

            {
                let e = &mut self.emitter;
                e.addi(Reg::Sp, Reg::Sp, -16);
                e.sd(Reg::Sp, 0, Reg::Ra);
                e.mv(Reg::A0, Reg::S1);
                e.li(Reg::A1, i as i64);
                e.mv(Reg::A2, Reg::S4);
                e.ld(Reg::T0, Reg::S1, ctx_off::HOST_DISPATCH);
                e.call_reg(Reg::T0);
            }
            let tfh = self.trap_from_host;
            self.emitter.bnez_label(Reg::A0, tfh);
            {
                let e = &mut self.emitter;
                e.addi_any(Reg::S4, Reg::S4, (n - m) * 8);
                e.ld(Reg::Ra, Reg::Sp, 0);
                e.addi(Reg::Sp, Reg::Sp, 16);
                e.ret();
            }
        }

        // ── local functions ─────────────────────────────────────────
        for i in 0..self.validation_info.functions.len() {
            let func_idx = imported + i;
            self.emitter.bind_label(self.func_labels[func_idx]);
            func_offsets.push(self.emitter.code.len());
            self.compile_function_body(i);
        }

        self.emitter.finalize();

        let mut func_canon_type_ids = Vec::with_capacity(total);
        for f in 0..total {
            let ti = self.validation_info.functions_types[f];
            func_canon_type_ids.push(self.canon_type_ids[ti]);
        }

        AotArtifact {
            code: self.emitter.code,
            func_offsets,
            entry_offset,
            func_canon_type_ids,
        }
    }

    // ── shared emitted subroutines ──────────────────────────────────

    /// popcnt64: t0 -> t0 (clobbers t1, t2).
    fn emit_helper_popcnt(&mut self) {
        let l = self.helper_popcnt;
        self.emitter.bind_label(l);
        let e = &mut self.emitter;
        e.li(Reg::T1, 0x5555_5555_5555_5555u64 as i64);
        e.srli(Reg::T2, Reg::T0, 1);
        e.and(Reg::T2, Reg::T2, Reg::T1);
        e.sub(Reg::T0, Reg::T0, Reg::T2);
        e.li(Reg::T1, 0x3333_3333_3333_3333u64 as i64);
        e.and(Reg::T2, Reg::T0, Reg::T1);
        e.srli(Reg::T0, Reg::T0, 2);
        e.and(Reg::T0, Reg::T0, Reg::T1);
        e.add(Reg::T0, Reg::T0, Reg::T2);
        e.srli(Reg::T2, Reg::T0, 4);
        e.add(Reg::T0, Reg::T0, Reg::T2);
        e.li(Reg::T1, 0x0F0F_0F0F_0F0F_0F0Fu64 as i64);
        e.and(Reg::T0, Reg::T0, Reg::T1);
        e.li(Reg::T1, 0x0101_0101_0101_0101u64 as i64);
        e.mul(Reg::T0, Reg::T0, Reg::T1);
        e.srli(Reg::T0, Reg::T0, 56);
        e.ret();
    }

    /// memmove: t0 = dst abs, t1 = src abs, t2 = len (clobbers a0).
    fn emit_helper_memmove(&mut self) {
        let l = self.helper_memmove;
        self.emitter.bind_label(l);
        let done = self.emitter.new_label();
        let fwd = self.emitter.new_label();
        let fwd_loop = self.emitter.new_label();
        let bwd_loop = self.emitter.new_label();
        let tail = self.emitter.new_label();
        let tail_loop = self.emitter.new_label();

        self.emitter.beqz_label(Reg::T2, done);
        self.emitter.bcc_label(cond::LTU, Reg::T0, Reg::T1, fwd);
        // backward byte copy from the end (dst > src overlap-safe)
        self.emitter.add(Reg::T0, Reg::T0, Reg::T2);
        self.emitter.add(Reg::T1, Reg::T1, Reg::T2);
        self.emitter.bind_label(bwd_loop);
        {
            let e = &mut self.emitter;
            e.addi(Reg::T0, Reg::T0, -1);
            e.addi(Reg::T1, Reg::T1, -1);
            e.lbu(Reg::A0, Reg::T1, 0);
            e.sb(Reg::T0, 0, Reg::A0);
            e.addi(Reg::T2, Reg::T2, -1);
        }
        self.emitter.bnez_label(Reg::T2, bwd_loop);
        self.emitter.jmp_label(done);
        // forward, word-at-a-time then byte tail
        self.emitter.bind_label(fwd);
        self.emitter.bind_label(fwd_loop);
        self.emitter.sltiu(Reg::A0, Reg::T2, 8);
        self.emitter.bnez_label(Reg::A0, tail);
        {
            let e = &mut self.emitter;
            e.ld(Reg::A0, Reg::T1, 0);
            e.sd(Reg::T0, 0, Reg::A0);
            e.addi(Reg::T0, Reg::T0, 8);
            e.addi(Reg::T1, Reg::T1, 8);
            e.addi(Reg::T2, Reg::T2, -8);
        }
        self.emitter.jmp_label(fwd_loop);
        self.emitter.bind_label(tail);
        self.emitter.beqz_label(Reg::T2, done);
        self.emitter.bind_label(tail_loop);
        {
            let e = &mut self.emitter;
            e.lbu(Reg::A0, Reg::T1, 0);
            e.sb(Reg::T0, 0, Reg::A0);
            e.addi(Reg::T0, Reg::T0, 1);
            e.addi(Reg::T1, Reg::T1, 1);
            e.addi(Reg::T2, Reg::T2, -1);
        }
        self.emitter.bnez_label(Reg::T2, tail_loop);
        self.emitter.bind_label(done);
        self.emitter.ret();
    }

    /// memset: t0 = dst abs, t1 = byte value, t2 = len.
    fn emit_helper_memset(&mut self) {
        let l = self.helper_memset;
        self.emitter.bind_label(l);
        let done = self.emitter.new_label();
        let loop_l = self.emitter.new_label();
        self.emitter.beqz_label(Reg::T2, done);
        self.emitter.bind_label(loop_l);
        {
            let e = &mut self.emitter;
            e.sb(Reg::T0, 0, Reg::T1);
            e.addi(Reg::T0, Reg::T0, 1);
            e.addi(Reg::T2, Reg::T2, -1);
        }
        self.emitter.bnez_label(Reg::T2, loop_l);
        self.emitter.bind_label(done);
        self.emitter.ret();
    }

    // ── function body ───────────────────────────────────────────────

    fn block_counts(&self, bt: &BlockType) -> (usize, usize) {
        match bt {
            BlockType::Empty => (0, 0),
            BlockType::Returns(_) => (0, 1),
            BlockType::Type(idx) => {
                let t = &self.validation_info.types[*idx as usize];
                (t.params.valtypes.len(), t.returns.valtypes.len())
            }
        }
    }

    fn compile_function_body(&mut self, local_func_idx: usize) {
        self.control_stack.clear();
        self.stack_depth = 0;
        self.vstack.clear();

        let imported = self.validation_info.imports_length.imported_functions;
        let func_idx = local_func_idx + imported;
        let type_idx = self.validation_info.functions_types[func_idx];
        let func_type = &self.validation_info.types[type_idx];
        self.result_count = func_type.returns.valtypes.len();
        let param_count = func_type.params.valtypes.len();

        let (span, _stp) = self.validation_info.func_blocks_stps[local_func_idx];
        let mut reader = WasmReader::new(self.validation_info.wasm);
        reader.pc = span.from;
        let locals =
            crate::common::validation::code::read_declared_locals(&mut reader).unwrap_validated();
        let total_locals = param_count + locals.len();

        self.local_types.clear();
        for vt in func_type.params.valtypes.iter() {
            self.local_types.push(*vt);
        }
        for vt in locals.iter() {
            self.local_types.push(*vt);
        }

        let locals_area = (total_locals * 8 + 15) & !15;

        // Prologue: save ra + caller's locals base, stack-limit check,
        // allocate locals, copy params from the wasm stack, zero the rest.
        {
            let e = &mut self.emitter;
            e.addi(Reg::Sp, Reg::Sp, -16);
            e.sd(Reg::Sp, 0, Reg::Ra);
            e.sd(Reg::Sp, 8, Reg::S3);
            e.addi_any(Reg::T0, Reg::Sp, -(locals_area as i64) - 1024);
            e.ld(Reg::T1, Reg::S1, ctx_off::STACK_LIMIT);
        }
        {
            let tso = self.trap_stack_overflow;
            self.emitter.bcc_label(cond::LTU, Reg::T0, Reg::T1, tso);
        }
        {
            let e = &mut self.emitter;
            e.addi_any(Reg::Sp, Reg::Sp, -(locals_area as i64));
            e.mv(Reg::S3, Reg::Sp);

            for i in 0..param_count {
                let src = ((param_count - 1 - i) * 8) as i64;
                e.ld_any(Reg::T0, Reg::S4, src);
                e.sd_any(Reg::S3, (i * 8) as i64, Reg::T0);
            }
            if param_count > 0 {
                e.addi_any(Reg::S4, Reg::S4, (param_count * 8) as i64);
            }
            for i in param_count..total_locals {
                e.sd_any(Reg::S3, (i * 8) as i64, Reg::Zero);
            }
        }

        let end_label = self.emitter.new_label();
        self.control_stack.push(ControlBlock {
            kind: ControlBlockKind::Func,
            stack_depth_before: 0,
            param_count: 0,
            result_count: self.result_count,
            end_label,
            else_label: None,
            start_label: None,
        });

        reader.pc = span.from;
        let _ = crate::common::validation::code::read_declared_locals(&mut reader)
            .unwrap_validated();

        while !self.control_stack.is_empty() {
            let instr = Instruction::read(&mut reader).unwrap_validated();
            self.compile_instruction(instr, &mut reader);
        }
        // NOTE: the Func block's End (compiled above) bound `end_label` and
        // left `stack_depth = result_count`; any pending cached values were
        // flushed by End (slow path).

        // Epilogue: results are on the wasm stack, exactly where the caller
        // expects them.
        {
            let e = &mut self.emitter;
            e.addi_any(Reg::Sp, Reg::Sp, locals_area as i64);
            e.ld(Reg::Ra, Reg::Sp, 0);
            e.ld(Reg::S3, Reg::Sp, 8);
            e.addi(Reg::Sp, Reg::Sp, 16);
            e.ret();
        }
    }

    // ── wasm operand stack helpers (slow path) ──────────────────────

    fn push_gp(&mut self, r: Reg) {
        self.emitter.addi(Reg::S4, Reg::S4, -8);
        self.emitter.sd(Reg::S4, 0, r);
    }

    fn pop_gp_to(&mut self, r: Reg) {
        self.emitter.ld(r, Reg::S4, 0);
        self.emitter.addi(Reg::S4, Reg::S4, 8);
    }

    fn push_fp(&mut self, f: FReg) {
        self.emitter.addi(Reg::S4, Reg::S4, -8);
        self.emitter.fsd(Reg::S4, 0, f);
    }

    fn pop_fp32_to(&mut self, f: FReg) {
        self.emitter.flw(f, Reg::S4, 0);
        self.emitter.addi(Reg::S4, Reg::S4, 8);
    }

    fn pop_fp64_to(&mut self, f: FReg) {
        self.emitter.fld(f, Reg::S4, 0);
        self.emitter.addi(Reg::S4, Reg::S4, 8);
    }

    // ── virtual stack (register cache) ──────────────────────────────

    fn local_is_gp(&self, idx: usize) -> bool {
        use crate::common::reader::types::NumType;
        matches!(
            self.local_types.get(idx),
            Some(ValType::NumType(NumType::I32)) | Some(ValType::NumType(NumType::I64))
        )
    }

    fn local_is_fp(&self, idx: usize) -> bool {
        use crate::common::reader::types::NumType;
        matches!(
            self.local_types.get(idx),
            Some(ValType::NumType(NumType::F32)) | Some(ValType::NumType(NumType::F64))
        )
    }

    fn gp_in_vstack(&self, r: Reg) -> bool {
        self.vstack.iter().any(|v| matches!(v, VLoc::Gp(x) if *x == r))
    }

    fn fp_in_vstack(&self, f: FReg) -> bool {
        self.vstack.iter().any(|v| matches!(v, VLoc::Fp(x) if *x == f))
    }

    /// Spill the deepest cached value to the memory stack. Correct because
    /// every cached value is shallower than everything in memory, and we
    /// always spill deepest-first.
    fn spill_deepest(&mut self) {
        let v = self.vstack.remove(0);
        match v {
            VLoc::Gp(r) => self.push_gp(r),
            VLoc::Fp(f) => self.push_fp(f),
        }
    }

    fn alloc_gp(&mut self, pinned: &[Reg]) -> Reg {
        loop {
            for &r in GP_POOL.iter() {
                if !pinned.contains(&r) && !self.gp_in_vstack(r) {
                    return r;
                }
            }
            self.spill_deepest();
        }
    }

    fn alloc_fp(&mut self, pinned: &[FReg]) -> FReg {
        loop {
            for &f in FP_POOL.iter() {
                if !pinned.contains(&f) && !self.fp_in_vstack(f) {
                    return f;
                }
            }
            self.spill_deepest();
        }
    }

    /// Spill the whole register cache to memory (deepest first).
    fn flush_cache(&mut self) {
        while !self.vstack.is_empty() {
            self.spill_deepest();
        }
    }

    fn vpush_gp(&mut self) -> Reg {
        let r = self.alloc_gp(&[]);
        self.vstack.push(VLoc::Gp(r));
        r
    }

    fn vpush_fp(&mut self) -> FReg {
        let f = self.alloc_fp(&[]);
        self.vstack.push(VLoc::Fp(f));
        f
    }

    fn vpop_gp(&mut self, pinned: &[Reg]) -> Reg {
        if let Some(VLoc::Gp(r)) = self.vstack.last().copied() {
            self.vstack.pop();
            return r;
        }
        debug_assert!(self.vstack.is_empty(), "type-confused vstack");
        let r = self.alloc_gp(pinned);
        self.pop_gp_to(r);
        r
    }

    fn vpop_fp32(&mut self, pinned: &[FReg]) -> FReg {
        if let Some(VLoc::Fp(f)) = self.vstack.last().copied() {
            self.vstack.pop();
            return f;
        }
        debug_assert!(self.vstack.is_empty(), "type-confused vstack");
        let f = self.alloc_fp(pinned);
        self.pop_fp32_to(f);
        f
    }

    fn vpop_fp64(&mut self, pinned: &[FReg]) -> FReg {
        if let Some(VLoc::Fp(f)) = self.vstack.last().copied() {
            self.vstack.pop();
            return f;
        }
        debug_assert!(self.vstack.is_empty(), "type-confused vstack");
        let f = self.alloc_fp(pinned);
        self.pop_fp64_to(f);
        f
    }

    fn v_binop_gp<F>(&mut self, op: F)
    where
        F: FnOnce(&mut Rv64Emitter, Reg, Reg),
    {
        let b = self.vpop_gp(&[]);
        let a = self.vpop_gp(&[b]);
        op(&mut self.emitter, a, b);
        self.vstack.push(VLoc::Gp(a));
        self.stack_depth = self.stack_depth.saturating_sub(1);
    }

    fn v_binop_fp32<F>(&mut self, op: F)
    where
        F: FnOnce(&mut Rv64Emitter, FReg, FReg),
    {
        let b = self.vpop_fp32(&[]);
        let a = self.vpop_fp32(&[b]);
        op(&mut self.emitter, a, b);
        self.vstack.push(VLoc::Fp(a));
        self.stack_depth = self.stack_depth.saturating_sub(1);
    }

    fn v_binop_fp64<F>(&mut self, op: F)
    where
        F: FnOnce(&mut Rv64Emitter, FReg, FReg),
    {
        let b = self.vpop_fp64(&[]);
        let a = self.vpop_fp64(&[b]);
        op(&mut self.emitter, a, b);
        self.vstack.push(VLoc::Fp(a));
        self.stack_depth = self.stack_depth.saturating_sub(1);
    }

    fn v_unop_gp<F>(&mut self, op: F)
    where
        F: FnOnce(&mut Rv64Emitter, Reg),
    {
        let a = self.vpop_gp(&[]);
        op(&mut self.emitter, a);
        self.vstack.push(VLoc::Gp(a));
    }

    fn v_unop_fp32<F>(&mut self, op: F)
    where
        F: FnOnce(&mut Rv64Emitter, FReg),
    {
        let a = self.vpop_fp32(&[]);
        op(&mut self.emitter, a);
        self.vstack.push(VLoc::Fp(a));
    }

    fn v_unop_fp64<F>(&mut self, op: F)
    where
        F: FnOnce(&mut Rv64Emitter, FReg),
    {
        let a = self.vpop_fp64(&[]);
        op(&mut self.emitter, a);
        self.vstack.push(VLoc::Fp(a));
    }

    fn v_relop_fp32<F>(&mut self, op: F)
    where
        F: FnOnce(&mut Rv64Emitter, Reg, FReg, FReg),
    {
        let b = self.vpop_fp32(&[]);
        let a = self.vpop_fp32(&[b]);
        let d = self.alloc_gp(&[]);
        op(&mut self.emitter, d, a, b);
        self.vstack.push(VLoc::Gp(d));
        self.stack_depth = self.stack_depth.saturating_sub(1);
    }

    fn v_relop_fp64<F>(&mut self, op: F)
    where
        F: FnOnce(&mut Rv64Emitter, Reg, FReg, FReg),
    {
        let b = self.vpop_fp64(&[]);
        let a = self.vpop_fp64(&[b]);
        let d = self.alloc_gp(&[]);
        op(&mut self.emitter, d, a, b);
        self.vstack.push(VLoc::Gp(d));
        self.stack_depth = self.stack_depth.saturating_sub(1);
    }

    // ── memory access (slow path only) ──────────────────────────────

    /// Pop the address, bounds-check `addr + offset + size <= mem_size`,
    /// leave the absolute address in T0.
    fn emit_addr_check(&mut self, memarg: &MemArg, size: u32) {
        self.pop_gp_to(Reg::T0);
        self.emitter.zext_w(Reg::T0, Reg::T0);
        self.emitter.addi_any(Reg::T0, Reg::T0, memarg.offset as i64);
        self.emitter.addi_any(Reg::T1, Reg::T0, size as i64);
        let oob = self.trap_oob;
        self.emitter.bcc_label(cond::LTU, Reg::S5, Reg::T1, oob);
        self.emitter.add(Reg::T0, Reg::S2, Reg::T0);
        self.stack_depth = self.stack_depth.saturating_sub(1);
    }

    fn emit_load<F>(&mut self, memarg: MemArg, size: u32, load: F)
    where
        F: FnOnce(&mut Rv64Emitter),
    {
        self.emit_addr_check(&memarg, size);
        load(&mut self.emitter); // loads from 0(T0) into T1
        self.push_gp(Reg::T1);
        self.stack_depth += 1;
    }

    fn emit_store<F>(&mut self, memarg: MemArg, size: u32, store: F)
    where
        F: FnOnce(&mut Rv64Emitter),
    {
        // value is on top, address below it
        self.pop_gp_to(Reg::T2);
        self.stack_depth = self.stack_depth.saturating_sub(1);
        self.emit_addr_check(&memarg, size);
        store(&mut self.emitter); // stores T2 to 0(T0)
    }

    // ── float→int conversions ───────────────────────────────────────

    /// fcvt with NV-flag check: traps on NaN or out-of-range input.
    fn emit_trunc_trap<F>(&mut self, is_f64: bool, cvt: F)
    where
        F: FnOnce(&mut Rv64Emitter, Reg, FReg),
    {
        let f = if is_f64 { self.vpop_fp64(&[]) } else { self.vpop_fp32(&[]) };
        self.flush_cache();
        self.emitter.csrrwi(Reg::Zero, 0x001, 0); // clear fflags
        cvt(&mut self.emitter, Reg::T0, f);
        self.emitter.csrrs(Reg::T1, 0x001, Reg::Zero);
        self.emitter.andi(Reg::T1, Reg::T1, 0x10); // NV
        let tio = self.trap_int_overflow;
        self.emitter.bnez_label(Reg::T1, tio);
        self.push_gp(Reg::T0);
    }

    /// Saturating fcvt: NaN → 0, out-of-range saturates (RISC-V native).
    fn emit_trunc_sat<G, F>(&mut self, is_f64: bool, feq: G, cvt: F)
    where
        G: FnOnce(&mut Rv64Emitter, Reg, FReg, FReg),
        F: FnOnce(&mut Rv64Emitter, Reg, FReg),
    {
        let f = if is_f64 { self.vpop_fp64(&[]) } else { self.vpop_fp32(&[]) };
        self.flush_cache();
        feq(&mut self.emitter, Reg::T1, f, f); // 0 when NaN
        cvt(&mut self.emitter, Reg::T0, f);
        self.emitter.neg(Reg::T1, Reg::T1); // 0 or -1 mask
        self.emitter.and(Reg::T0, Reg::T0, Reg::T1);
        self.push_gp(Reg::T0);
    }

    /// floor/ceil/trunc/nearest via int round-trip. |x| >= 2^precision is
    /// already integral (and NaN) — passes through unchanged; signed zero
    /// is preserved via fsgnj.
    fn emit_fround(&mut self, is_f64: bool, round_mode: u32) {
        let x = if is_f64 { self.vpop_fp64(&[]) } else { self.vpop_fp32(&[]) };
        self.flush_cache();
        {
            let e = &mut self.emitter;
            if is_f64 {
                e.fabs_d(FReg::Ft0, x);
                e.li(Reg::T0, (4503599627370496.0f64).to_bits() as i64); // 2^52
                e.fmv_d_x(FReg::Ft1, Reg::T0);
                e.flt_d(Reg::T0, FReg::Ft0, FReg::Ft1);
            } else {
                e.fabs_s(FReg::Ft0, x);
                e.li(Reg::T0, (8388608.0f32).to_bits() as u32 as i64); // 2^23
                e.fmv_w_x(FReg::Ft1, Reg::T0);
                e.flt_s(Reg::T0, FReg::Ft0, FReg::Ft1);
            }
        }
        let keep = self.emitter.new_label();
        self.emitter.beqz_label(Reg::T0, keep);
        {
            let e = &mut self.emitter;
            if is_f64 {
                e.fcvt_l_d(Reg::T1, x, round_mode);
                e.fcvt_d_l(FReg::Ft0, Reg::T1);
                e.fsgnj_d(x, FReg::Ft0, x);
            } else {
                e.fcvt_w_s(Reg::T1, x, round_mode);
                e.fcvt_s_w(FReg::Ft0, Reg::T1);
                e.fsgnj_s(x, FReg::Ft0, x);
            }
        }
        self.emitter.bind_label(keep);
        self.vstack.push(VLoc::Fp(x));
    }

    // ── rotates / bit counting / division ───────────────────────────

    fn emit_rot(&mut self, is64: bool, left: bool) {
        let b = self.vpop_gp(&[]);
        let a = self.vpop_gp(&[b]);
        let e = &mut self.emitter;
        let bits: i32 = if is64 { 64 } else { 32 };
        e.andi(Reg::T0, b, bits - 1);
        e.li(Reg::T1, bits as i64);
        e.sub(Reg::T1, Reg::T1, Reg::T0);
        e.andi(Reg::T1, Reg::T1, bits - 1);
        if is64 {
            let (shl_amt, shr_amt) = if left { (Reg::T0, Reg::T1) } else { (Reg::T1, Reg::T0) };
            e.sll(Reg::T2, a, shl_amt);
            e.srl(a, a, shr_amt);
            e.or(a, a, Reg::T2);
        } else {
            let (shl_amt, shr_amt) = if left { (Reg::T0, Reg::T1) } else { (Reg::T1, Reg::T0) };
            e.zext_w(a, a);
            e.sllw(Reg::T2, a, shl_amt);
            e.srlw(a, a, shr_amt);
            e.or(a, a, Reg::T2);
            e.sext_w(a, a);
        }
        self.vstack.push(VLoc::Gp(a));
        self.stack_depth = self.stack_depth.saturating_sub(1);
    }

    fn emit_popcnt(&mut self, is64: bool) {
        let a = self.vpop_gp(&[]);
        self.flush_cache();
        if is64 {
            self.emitter.mv(Reg::T0, a);
        } else {
            self.emitter.zext_w(Reg::T0, a);
        }
        let l = self.helper_popcnt;
        self.emitter.call_label(l);
        self.push_gp(Reg::T0);
    }

    fn emit_clz(&mut self, is64: bool) {
        let a = self.vpop_gp(&[]);
        self.flush_cache();
        {
            let e = &mut self.emitter;
            if is64 {
                e.mv(Reg::T0, a);
            } else {
                e.zext_w(Reg::T0, a);
            }
            for sh in [1u32, 2, 4, 8, 16, 32] {
                if !is64 && sh == 32 {
                    break;
                }
                e.srli(Reg::T1, Reg::T0, sh);
                e.or(Reg::T0, Reg::T0, Reg::T1);
            }
            e.not(Reg::T0, Reg::T0);
            if !is64 {
                e.zext_w(Reg::T0, Reg::T0);
            }
        }
        let l = self.helper_popcnt;
        self.emitter.call_label(l);
        self.push_gp(Reg::T0);
    }

    fn emit_ctz(&mut self, is64: bool) {
        let a = self.vpop_gp(&[]);
        self.flush_cache();
        {
            let e = &mut self.emitter;
            if is64 {
                e.mv(Reg::T2, a);
            } else {
                e.zext_w(Reg::T2, a);
                e.li(Reg::T1, 1i64 << 32); // ctz32(0) must be 32
                e.or(Reg::T2, Reg::T2, Reg::T1);
            }
            e.neg(Reg::T0, Reg::T2);
            e.and(Reg::T0, Reg::T0, Reg::T2);
            e.addi(Reg::T0, Reg::T0, -1);
        }
        let l = self.helper_popcnt;
        self.emitter.call_label(l);
        self.push_gp(Reg::T0);
    }

    fn emit_div(&mut self, is64: bool, signed: bool, is_rem: bool) {
        let b = self.vpop_gp(&[]);
        let a = self.vpop_gp(&[b]);
        let dz = self.trap_div_zero;
        self.emitter.beqz_label(b, dz);
        if signed && !is_rem {
            let ok = self.emitter.new_label();
            self.emitter.addi(Reg::T0, b, 1);
            self.emitter.bnez_label(Reg::T0, ok);
            if is64 {
                self.emitter.li(Reg::T1, i64::MIN);
            } else {
                self.emitter.li(Reg::T1, i32::MIN as i64);
            }
            let tio = self.trap_int_overflow;
            self.emitter.bcc_label(cond::EQ, a, Reg::T1, tio);
            self.emitter.bind_label(ok);
        }
        let e = &mut self.emitter;
        match (is64, signed, is_rem) {
            (true, true, false) => e.div(a, a, b),
            (true, false, false) => e.divu(a, a, b),
            (true, true, true) => e.rem(a, a, b),
            (true, false, true) => e.remu(a, a, b),
            (false, true, false) => e.divw(a, a, b),
            (false, true, true) => e.remw(a, a, b),
            (false, false, false) => {
                e.zext_w(Reg::T0, a);
                e.zext_w(Reg::T1, b);
                e.divuw(a, Reg::T0, Reg::T1);
            }
            (false, false, true) => {
                e.zext_w(Reg::T0, a);
                e.zext_w(Reg::T1, b);
                e.remuw(a, Reg::T0, Reg::T1);
            }
        }
        self.vstack.push(VLoc::Gp(a));
        self.stack_depth = self.stack_depth.saturating_sub(1);
    }

    // ── branches ────────────────────────────────────────────────────

    fn emit_unwind(&mut self, target_idx: usize) {
        let cb = &self.control_stack[target_idx];
        let keep = if cb.kind == ControlBlockKind::Loop {
            cb.param_count
        } else {
            cb.result_count
        };
        let drop_count = self
            .stack_depth
            .saturating_sub(cb.stack_depth_before + keep);

        if keep > 0 && drop_count > 0 {
            for i in (0..keep).rev() {
                self.emitter.ld_any(Reg::T0, Reg::S4, (i * 8) as i64);
                self.emitter
                    .sd_any(Reg::S4, ((i + drop_count) * 8) as i64, Reg::T0);
            }
        }
        if drop_count > 0 {
            self.emitter
                .addi_any(Reg::S4, Reg::S4, (drop_count * 8) as i64);
        }
    }

    fn branch_target(&self, label_idx: usize) -> (usize, usize) {
        let target_idx = self.control_stack.len() - 1 - label_idx;
        let cb = &self.control_stack[target_idx];
        (target_idx, cb.start_label.unwrap_or(cb.end_label))
    }

    /// Reload memory base/size after any call (memory.grow safety).
    fn emit_post_call_reload(&mut self) {
        self.emitter.ld(Reg::S2, Reg::S1, ctx_off::MEM_BASE);
        self.emitter.ld(Reg::S5, Reg::S1, ctx_off::MEM_SIZE);
    }

    // ── main instruction dispatch ───────────────────────────────────

    fn compile_instruction(&mut self, instr: Instruction, reader: &mut WasmReader) {
        use crate::common::reader::types::NumType;

        // Fast path: register-cached ops. Everything else flushes first.
        match &instr {
            Instruction::I32Const(v) => {
                let v = *v;
                let r = self.vpush_gp();
                self.emitter.li(r, v as i64); // sign-extended canonical form
                self.stack_depth += 1;
                return;
            }
            Instruction::I64Const(v) => {
                let v = *v;
                let r = self.vpush_gp();
                self.emitter.li(r, v);
                self.stack_depth += 1;
                return;
            }
            Instruction::F32Const(v) => {
                let bits = v.to_bits();
                let f = self.vpush_fp();
                self.emitter.li(Reg::T0, bits as i64);
                self.emitter.fmv_w_x(f, Reg::T0);
                self.stack_depth += 1;
                return;
            }
            Instruction::F64Const(v) => {
                let bits = v.to_bits();
                let f = self.vpush_fp();
                self.emitter.li(Reg::T0, bits as i64);
                self.emitter.fmv_d_x(f, Reg::T0);
                self.stack_depth += 1;
                return;
            }
            Instruction::LocalGet(idx) if self.local_is_gp(*idx) => {
                let off = (*idx * 8) as i64;
                let r = self.vpush_gp();
                self.emitter.ld_any(r, Reg::S3, off);
                self.stack_depth += 1;
                return;
            }
            Instruction::LocalSet(idx) if self.local_is_gp(*idx) => {
                let off = (*idx * 8) as i64;
                let r = self.vpop_gp(&[]);
                self.emitter.sd_any(Reg::S3, off, r);
                self.stack_depth = self.stack_depth.saturating_sub(1);
                return;
            }
            Instruction::LocalTee(idx) if self.local_is_gp(*idx) => {
                let off = (*idx * 8) as i64;
                let r = if let Some(VLoc::Gp(r)) = self.vstack.last().copied() {
                    r
                } else {
                    let r = self.alloc_gp(&[]);
                    self.pop_gp_to(r);
                    self.vstack.push(VLoc::Gp(r));
                    r
                };
                self.emitter.sd_any(Reg::S3, off, r);
                return;
            }
            Instruction::LocalGet(idx) if self.local_is_fp(*idx) => {
                let off = (*idx * 8) as i64;
                let is32 = matches!(self.local_types[*idx], ValType::NumType(NumType::F32));
                let f = self.vpush_fp();
                self.emitter.addi_any(Reg::T0, Reg::S3, off);
                if is32 {
                    self.emitter.flw(f, Reg::T0, 0);
                } else {
                    self.emitter.fld(f, Reg::T0, 0);
                }
                self.stack_depth += 1;
                return;
            }
            Instruction::LocalSet(idx) if self.local_is_fp(*idx) => {
                let off = (*idx * 8) as i64;
                let f = if let Some(VLoc::Fp(f)) = self.vstack.last().copied() {
                    self.vstack.pop();
                    f
                } else {
                    let f = self.alloc_fp(&[]);
                    self.pop_fp64_to(f); // full slot; NaN boxing preserved
                    f
                };
                self.emitter.addi_any(Reg::T0, Reg::S3, off);
                self.emitter.fsd(Reg::T0, 0, f);
                self.stack_depth = self.stack_depth.saturating_sub(1);
                return;
            }
            Instruction::LocalTee(idx) if self.local_is_fp(*idx) => {
                let off = (*idx * 8) as i64;
                let f = if let Some(VLoc::Fp(f)) = self.vstack.last().copied() {
                    f
                } else {
                    let f = self.alloc_fp(&[]);
                    self.pop_fp64_to(f);
                    self.vstack.push(VLoc::Fp(f));
                    f
                };
                self.emitter.addi_any(Reg::T0, Reg::S3, off);
                self.emitter.fsd(Reg::T0, 0, f);
                return;
            }
            // integer ALU (i32 canonical form = sign-extended)
            Instruction::I32Add => { self.v_binop_gp(|e, a, b| e.addw(a, a, b)); return; }
            Instruction::I32Sub => { self.v_binop_gp(|e, a, b| e.subw(a, a, b)); return; }
            Instruction::I32Mul => { self.v_binop_gp(|e, a, b| e.mulw(a, a, b)); return; }
            Instruction::I32And => { self.v_binop_gp(|e, a, b| e.and(a, a, b)); return; }
            Instruction::I32Or => { self.v_binop_gp(|e, a, b| e.or(a, a, b)); return; }
            Instruction::I32Xor => { self.v_binop_gp(|e, a, b| e.xor(a, a, b)); return; }
            Instruction::I64Add => { self.v_binop_gp(|e, a, b| e.add(a, a, b)); return; }
            Instruction::I64Sub => { self.v_binop_gp(|e, a, b| e.sub(a, a, b)); return; }
            Instruction::I64Mul => { self.v_binop_gp(|e, a, b| e.mul(a, a, b)); return; }
            Instruction::I64And => { self.v_binop_gp(|e, a, b| e.and(a, a, b)); return; }
            Instruction::I64Or => { self.v_binop_gp(|e, a, b| e.or(a, a, b)); return; }
            Instruction::I64Xor => { self.v_binop_gp(|e, a, b| e.xor(a, a, b)); return; }
            Instruction::I32Shl => { self.v_binop_gp(|e, a, b| e.sllw(a, a, b)); return; }
            Instruction::I32ShrS => { self.v_binop_gp(|e, a, b| e.sraw(a, a, b)); return; }
            Instruction::I32ShrU => {
                self.v_binop_gp(|e, a, b| {
                    e.zext_w(Reg::T0, a);
                    e.srlw(a, Reg::T0, b);
                });
                return;
            }
            Instruction::I64Shl => { self.v_binop_gp(|e, a, b| e.sll(a, a, b)); return; }
            Instruction::I64ShrS => { self.v_binop_gp(|e, a, b| e.sra(a, a, b)); return; }
            Instruction::I64ShrU => { self.v_binop_gp(|e, a, b| e.srl(a, a, b)); return; }
            Instruction::I32Rotl => { self.emit_rot(false, true); return; }
            Instruction::I32Rotr => { self.emit_rot(false, false); return; }
            Instruction::I64Rotl => { self.emit_rot(true, true); return; }
            Instruction::I64Rotr => { self.emit_rot(true, false); return; }
            // comparisons (canonical sign-extension makes 32/64 share code)
            Instruction::I32Eqz | Instruction::I64Eqz => {
                self.v_unop_gp(|e, a| e.seqz(a, a));
                return;
            }
            Instruction::I32Eq | Instruction::I64Eq => {
                self.v_binop_gp(|e, a, b| { e.xor(a, a, b); e.seqz(a, a); });
                return;
            }
            Instruction::I32Ne | Instruction::I64Ne => {
                self.v_binop_gp(|e, a, b| { e.xor(a, a, b); e.snez(a, a); });
                return;
            }
            Instruction::I32LtS | Instruction::I64LtS => {
                self.v_binop_gp(|e, a, b| e.slt(a, a, b));
                return;
            }
            Instruction::I32LtU | Instruction::I64LtU => {
                self.v_binop_gp(|e, a, b| e.sltu(a, a, b));
                return;
            }
            Instruction::I32GtS | Instruction::I64GtS => {
                self.v_binop_gp(|e, a, b| e.slt(a, b, a));
                return;
            }
            Instruction::I32GtU | Instruction::I64GtU => {
                self.v_binop_gp(|e, a, b| e.sltu(a, b, a));
                return;
            }
            Instruction::I32LeS | Instruction::I64LeS => {
                self.v_binop_gp(|e, a, b| { e.slt(a, b, a); e.seqz(a, a); });
                return;
            }
            Instruction::I32LeU | Instruction::I64LeU => {
                self.v_binop_gp(|e, a, b| { e.sltu(a, b, a); e.seqz(a, a); });
                return;
            }
            Instruction::I32GeS | Instruction::I64GeS => {
                self.v_binop_gp(|e, a, b| { e.slt(a, a, b); e.seqz(a, a); });
                return;
            }
            Instruction::I32GeU | Instruction::I64GeU => {
                self.v_binop_gp(|e, a, b| { e.sltu(a, a, b); e.seqz(a, a); });
                return;
            }
            // float arithmetic
            Instruction::F32Add => { self.v_binop_fp32(|e, a, b| e.fadd_s(a, a, b)); return; }
            Instruction::F32Sub => { self.v_binop_fp32(|e, a, b| e.fsub_s(a, a, b)); return; }
            Instruction::F32Mul => { self.v_binop_fp32(|e, a, b| e.fmul_s(a, a, b)); return; }
            Instruction::F32Div => { self.v_binop_fp32(|e, a, b| e.fdiv_s(a, a, b)); return; }
            Instruction::F64Add => { self.v_binop_fp64(|e, a, b| e.fadd_d(a, a, b)); return; }
            Instruction::F64Sub => { self.v_binop_fp64(|e, a, b| e.fsub_d(a, a, b)); return; }
            Instruction::F64Mul => { self.v_binop_fp64(|e, a, b| e.fmul_d(a, a, b)); return; }
            Instruction::F64Div => { self.v_binop_fp64(|e, a, b| e.fdiv_d(a, a, b)); return; }
            Instruction::F32Min => { self.v_binop_fp32(|e, a, b| e.fmin_s(a, a, b)); return; }
            Instruction::F32Max => { self.v_binop_fp32(|e, a, b| e.fmax_s(a, a, b)); return; }
            Instruction::F64Min => { self.v_binop_fp64(|e, a, b| e.fmin_d(a, a, b)); return; }
            Instruction::F64Max => { self.v_binop_fp64(|e, a, b| e.fmax_d(a, a, b)); return; }
            Instruction::F32Abs => { self.v_unop_fp32(|e, a| e.fabs_s(a, a)); return; }
            Instruction::F32Neg => { self.v_unop_fp32(|e, a| e.fneg_s(a, a)); return; }
            Instruction::F32Sqrt => { self.v_unop_fp32(|e, a| e.fsqrt_s(a, a)); return; }
            Instruction::F64Abs => { self.v_unop_fp64(|e, a| e.fabs_d(a, a)); return; }
            Instruction::F64Neg => { self.v_unop_fp64(|e, a| e.fneg_d(a, a)); return; }
            Instruction::F64Sqrt => { self.v_unop_fp64(|e, a| e.fsqrt_d(a, a)); return; }
            Instruction::F32Copysign => { self.v_binop_fp32(|e, a, b| e.fsgnj_s(a, a, b)); return; }
            Instruction::F64Copysign => { self.v_binop_fp64(|e, a, b| e.fsgnj_d(a, a, b)); return; }
            // float comparisons (quiet on NaN, matching wasm)
            Instruction::F32Eq => { self.v_relop_fp32(|e, d, a, b| e.feq_s(d, a, b)); return; }
            Instruction::F32Ne => { self.v_relop_fp32(|e, d, a, b| { e.feq_s(d, a, b); e.seqz(d, d); }); return; }
            Instruction::F32Lt => { self.v_relop_fp32(|e, d, a, b| e.flt_s(d, a, b)); return; }
            Instruction::F32Gt => { self.v_relop_fp32(|e, d, a, b| e.flt_s(d, b, a)); return; }
            Instruction::F32Le => { self.v_relop_fp32(|e, d, a, b| e.fle_s(d, a, b)); return; }
            Instruction::F32Ge => { self.v_relop_fp32(|e, d, a, b| e.fle_s(d, b, a)); return; }
            Instruction::F64Eq => { self.v_relop_fp64(|e, d, a, b| e.feq_d(d, a, b)); return; }
            Instruction::F64Ne => { self.v_relop_fp64(|e, d, a, b| { e.feq_d(d, a, b); e.seqz(d, d); }); return; }
            Instruction::F64Lt => { self.v_relop_fp64(|e, d, a, b| e.flt_d(d, a, b)); return; }
            Instruction::F64Gt => { self.v_relop_fp64(|e, d, a, b| e.flt_d(d, b, a)); return; }
            Instruction::F64Le => { self.v_relop_fp64(|e, d, a, b| e.fle_d(d, a, b)); return; }
            Instruction::F64Ge => { self.v_relop_fp64(|e, d, a, b| e.fle_d(d, b, a)); return; }
            // register-to-register conversions
            Instruction::I32WrapI64 => { self.v_unop_gp(|e, a| e.sext_w(a, a)); return; }
            Instruction::I64ExtendI32S => { self.v_unop_gp(|e, a| e.sext_w(a, a)); return; }
            Instruction::I64ExtendI32U => { self.v_unop_gp(|e, a| e.zext_w(a, a)); return; }
            Instruction::I32Extend8S | Instruction::I64Extend8S => {
                self.v_unop_gp(|e, a| { e.slli(a, a, 56); e.srai(a, a, 56); });
                return;
            }
            Instruction::I32Extend16S | Instruction::I64Extend16S => {
                self.v_unop_gp(|e, a| { e.slli(a, a, 48); e.srai(a, a, 48); });
                return;
            }
            Instruction::I64Extend32S => { self.v_unop_gp(|e, a| e.sext_w(a, a)); return; }
            Instruction::F32DemoteF64 => { self.v_unop_fp64(|e, a| e.fcvt_s_d(a, a)); return; }
            Instruction::F64PromoteF32 => { self.v_unop_fp32(|e, a| e.fcvt_d_s(a, a)); return; }
            Instruction::F32ConvertI32S => {
                let a = self.vpop_gp(&[]);
                let f = self.alloc_fp(&[]);
                self.emitter.fcvt_s_w(f, a);
                self.vstack.push(VLoc::Fp(f));
                return;
            }
            Instruction::F32ConvertI32U => {
                let a = self.vpop_gp(&[]);
                let f = self.alloc_fp(&[]);
                self.emitter.fcvt_s_wu(f, a);
                self.vstack.push(VLoc::Fp(f));
                return;
            }
            Instruction::F32ConvertI64S => {
                let a = self.vpop_gp(&[]);
                let f = self.alloc_fp(&[]);
                self.emitter.fcvt_s_l(f, a);
                self.vstack.push(VLoc::Fp(f));
                return;
            }
            Instruction::F32ConvertI64U => {
                let a = self.vpop_gp(&[]);
                let f = self.alloc_fp(&[]);
                self.emitter.fcvt_s_lu(f, a);
                self.vstack.push(VLoc::Fp(f));
                return;
            }
            Instruction::F64ConvertI32S => {
                let a = self.vpop_gp(&[]);
                let f = self.alloc_fp(&[]);
                self.emitter.fcvt_d_w(f, a);
                self.vstack.push(VLoc::Fp(f));
                return;
            }
            Instruction::F64ConvertI32U => {
                let a = self.vpop_gp(&[]);
                let f = self.alloc_fp(&[]);
                self.emitter.fcvt_d_wu(f, a);
                self.vstack.push(VLoc::Fp(f));
                return;
            }
            Instruction::F64ConvertI64S => {
                let a = self.vpop_gp(&[]);
                let f = self.alloc_fp(&[]);
                self.emitter.fcvt_d_l(f, a);
                self.vstack.push(VLoc::Fp(f));
                return;
            }
            Instruction::F64ConvertI64U => {
                let a = self.vpop_gp(&[]);
                let f = self.alloc_fp(&[]);
                self.emitter.fcvt_d_lu(f, a);
                self.vstack.push(VLoc::Fp(f));
                return;
            }
            Instruction::I32ReinterpretF32 => {
                let f = self.vpop_fp32(&[]);
                let r = self.alloc_gp(&[]);
                self.emitter.fmv_x_w(r, f);
                self.vstack.push(VLoc::Gp(r));
                return;
            }
            Instruction::I64ReinterpretF64 => {
                let f = self.vpop_fp64(&[]);
                let r = self.alloc_gp(&[]);
                self.emitter.fmv_x_d(r, f);
                self.vstack.push(VLoc::Gp(r));
                return;
            }
            Instruction::F32ReinterpretI32 => {
                let a = self.vpop_gp(&[]);
                let f = self.alloc_fp(&[]);
                self.emitter.fmv_w_x(f, a);
                self.vstack.push(VLoc::Fp(f));
                return;
            }
            Instruction::F64ReinterpretI64 => {
                let a = self.vpop_gp(&[]);
                let f = self.alloc_fp(&[]);
                self.emitter.fmv_d_x(f, a);
                self.vstack.push(VLoc::Fp(f));
                return;
            }
            Instruction::F32Ceil => { self.emit_fround(false, rm::RUP); return; }
            Instruction::F32Floor => { self.emit_fround(false, rm::RDN); return; }
            Instruction::F32Trunc => { self.emit_fround(false, rm::RTZ); return; }
            Instruction::F32Nearest => { self.emit_fround(false, rm::RNE); return; }
            Instruction::F64Ceil => { self.emit_fround(true, rm::RUP); return; }
            Instruction::F64Floor => { self.emit_fround(true, rm::RDN); return; }
            Instruction::F64Trunc => { self.emit_fround(true, rm::RTZ); return; }
            Instruction::F64Nearest => { self.emit_fround(true, rm::RNE); return; }
            Instruction::I32Clz => { self.emit_clz(false); return; }
            Instruction::I64Clz => { self.emit_clz(true); return; }
            Instruction::I32Ctz => { self.emit_ctz(false); return; }
            Instruction::I64Ctz => { self.emit_ctz(true); return; }
            Instruction::I32Popcnt => { self.emit_popcnt(false); return; }
            Instruction::I64Popcnt => { self.emit_popcnt(true); return; }
            Instruction::I32DivS => { self.emit_div(false, true, false); return; }
            Instruction::I32DivU => { self.emit_div(false, false, false); return; }
            Instruction::I32RemS => { self.emit_div(false, true, true); return; }
            Instruction::I32RemU => { self.emit_div(false, false, true); return; }
            Instruction::I64DivS => { self.emit_div(true, true, false); return; }
            Instruction::I64DivU => { self.emit_div(true, false, false); return; }
            Instruction::I64RemS => { self.emit_div(true, true, true); return; }
            Instruction::I64RemU => { self.emit_div(true, false, true); return; }
            Instruction::I32TruncF32S => { self.emit_trunc_trap(false, |e, d, f| e.fcvt_w_s(d, f, rm::RTZ)); return; }
            Instruction::I32TruncF32U => { self.emit_trunc_trap(false, |e, d, f| e.fcvt_wu_s(d, f, rm::RTZ)); return; }
            Instruction::I32TruncF64S => { self.emit_trunc_trap(true, |e, d, f| e.fcvt_w_d(d, f, rm::RTZ)); return; }
            Instruction::I32TruncF64U => { self.emit_trunc_trap(true, |e, d, f| e.fcvt_wu_d(d, f, rm::RTZ)); return; }
            Instruction::I64TruncF32S => { self.emit_trunc_trap(false, |e, d, f| e.fcvt_l_s(d, f, rm::RTZ)); return; }
            Instruction::I64TruncF32U => { self.emit_trunc_trap(false, |e, d, f| e.fcvt_lu_s(d, f, rm::RTZ)); return; }
            Instruction::I64TruncF64S => { self.emit_trunc_trap(true, |e, d, f| e.fcvt_l_d(d, f, rm::RTZ)); return; }
            Instruction::I64TruncF64U => { self.emit_trunc_trap(true, |e, d, f| e.fcvt_lu_d(d, f, rm::RTZ)); return; }
            _ => {
                self.flush_cache();
            }
        }

        // ── slow path: cache is flushed, canonical memory-stack state ──
        match instr {
            Instruction::Nop => {}
            Instruction::Unreachable => {
                let t = self.trap_unreachable;
                self.emitter.jmp_label(t);
            }
            Instruction::Drop => {
                self.emitter.addi(Reg::S4, Reg::S4, 8);
                self.stack_depth = self.stack_depth.saturating_sub(1);
            }
            Instruction::Select => {
                self.pop_gp_to(Reg::T2); // condition
                self.pop_gp_to(Reg::T1); // val2
                self.pop_gp_to(Reg::T0); // val1
                self.emitter.bcc(cond::NE, Reg::T2, Reg::Zero, 8);
                self.emitter.mv(Reg::T0, Reg::T1);
                self.push_gp(Reg::T0);
                self.stack_depth = self.stack_depth.saturating_sub(2);
            }
            Instruction::LocalGet(idx) => {
                self.emitter.ld_any(Reg::T0, Reg::S3, (idx * 8) as i64);
                self.push_gp(Reg::T0);
                self.stack_depth += 1;
            }
            Instruction::LocalSet(idx) => {
                self.pop_gp_to(Reg::T0);
                self.emitter.sd_any(Reg::S3, (idx * 8) as i64, Reg::T0);
                self.stack_depth = self.stack_depth.saturating_sub(1);
            }
            Instruction::LocalTee(idx) => {
                self.emitter.ld(Reg::T0, Reg::S4, 0);
                self.emitter.sd_any(Reg::S3, (idx * 8) as i64, Reg::T0);
            }
            Instruction::GlobalGet(idx) => {
                self.emitter.ld(Reg::T1, Reg::S1, ctx_off::GLOBALS_PTR);
                self.emitter.ld_any(Reg::T0, Reg::T1, (idx as i64) * 8);
                self.push_gp(Reg::T0);
                self.stack_depth += 1;
            }
            Instruction::GlobalSet(idx) => {
                self.pop_gp_to(Reg::T0);
                self.emitter.ld(Reg::T1, Reg::S1, ctx_off::GLOBALS_PTR);
                self.emitter.sd_any(Reg::T1, (idx as i64) * 8, Reg::T0);
                self.stack_depth = self.stack_depth.saturating_sub(1);
            }
            Instruction::RefNull(_) => {
                self.push_gp(Reg::Zero);
                self.stack_depth += 1;
            }
            Instruction::RefIsNull => {
                self.pop_gp_to(Reg::T0);
                self.emitter.seqz(Reg::T0, Reg::T0);
                self.push_gp(Reg::T0);
            }
            Instruction::RefFunc(idx) => {
                // table representation: func_idx + 1 (0 = null)
                self.emitter.li(Reg::T0, idx as i64 + 1);
                self.push_gp(Reg::T0);
                self.stack_depth += 1;
            }
            Instruction::TableGet(_t) => {
                self.pop_gp_to(Reg::T0);
                self.emitter.ld(Reg::T1, Reg::S1, ctx_off::TABLE_PTR);
                self.emitter.slli(Reg::T0, Reg::T0, 3);
                self.emitter.add(Reg::T0, Reg::T0, Reg::T1);
                self.emitter.ld(Reg::T0, Reg::T0, 0);
                self.push_gp(Reg::T0);
            }
            Instruction::TableSet(_t) => {
                self.pop_gp_to(Reg::T2);
                self.pop_gp_to(Reg::T0);
                self.emitter.ld(Reg::T1, Reg::S1, ctx_off::TABLE_PTR);
                self.emitter.slli(Reg::T0, Reg::T0, 3);
                self.emitter.add(Reg::T0, Reg::T0, Reg::T1);
                self.emitter.sd(Reg::T0, 0, Reg::T2);
                self.stack_depth = self.stack_depth.saturating_sub(2);
            }

            // ── memory ──────────────────────────────────────────────
            Instruction::I32Load(m) => self.emit_load(m, 4, |e| e.lw(Reg::T1, Reg::T0, 0)),
            Instruction::I64Load(m) => self.emit_load(m, 8, |e| e.ld(Reg::T1, Reg::T0, 0)),
            Instruction::I32Load8S(m) | Instruction::I64Load8S(m) => {
                self.emit_load(m, 1, |e| e.lb(Reg::T1, Reg::T0, 0))
            }
            Instruction::I32Load8U(m) | Instruction::I64Load8U(m) => {
                self.emit_load(m, 1, |e| e.lbu(Reg::T1, Reg::T0, 0))
            }
            Instruction::I32Load16S(m) | Instruction::I64Load16S(m) => {
                self.emit_load(m, 2, |e| e.lh(Reg::T1, Reg::T0, 0))
            }
            Instruction::I32Load16U(m) | Instruction::I64Load16U(m) => {
                self.emit_load(m, 2, |e| e.lhu(Reg::T1, Reg::T0, 0))
            }
            Instruction::I64Load32S(m) => self.emit_load(m, 4, |e| e.lw(Reg::T1, Reg::T0, 0)),
            Instruction::I64Load32U(m) => self.emit_load(m, 4, |e| e.lwu(Reg::T1, Reg::T0, 0)),
            Instruction::F32Load(m) => {
                self.emit_addr_check(&m, 4);
                self.emitter.flw(FReg::Ft0, Reg::T0, 0);
                self.push_fp(FReg::Ft0);
                self.stack_depth += 1;
            }
            Instruction::F64Load(m) => {
                self.emit_addr_check(&m, 8);
                self.emitter.fld(FReg::Ft0, Reg::T0, 0);
                self.push_fp(FReg::Ft0);
                self.stack_depth += 1;
            }
            Instruction::I32Store(m) => self.emit_store(m, 4, |e| e.sw(Reg::T0, 0, Reg::T2)),
            Instruction::I64Store(m) => self.emit_store(m, 8, |e| e.sd(Reg::T0, 0, Reg::T2)),
            Instruction::I32Store8(m) | Instruction::I64Store8(m) => {
                self.emit_store(m, 1, |e| e.sb(Reg::T0, 0, Reg::T2))
            }
            Instruction::I32Store16(m) | Instruction::I64Store16(m) => {
                self.emit_store(m, 2, |e| e.sh(Reg::T0, 0, Reg::T2))
            }
            Instruction::I64Store32(m) => self.emit_store(m, 4, |e| e.sw(Reg::T0, 0, Reg::T2)),
            Instruction::F32Store(m) => {
                self.emitter.flw(FReg::Ft0, Reg::S4, 0);
                self.emitter.addi(Reg::S4, Reg::S4, 8);
                self.stack_depth = self.stack_depth.saturating_sub(1);
                self.emit_addr_check(&m, 4);
                self.emitter.fsw(Reg::T0, 0, FReg::Ft0);
            }
            Instruction::F64Store(m) => {
                self.emitter.fld(FReg::Ft0, Reg::S4, 0);
                self.emitter.addi(Reg::S4, Reg::S4, 8);
                self.stack_depth = self.stack_depth.saturating_sub(1);
                self.emit_addr_check(&m, 8);
                self.emitter.fsd(Reg::T0, 0, FReg::Ft0);
            }
            Instruction::MemorySize => {
                self.emitter.srli(Reg::T0, Reg::S5, 16);
                self.push_gp(Reg::T0);
                self.stack_depth += 1;
            }
            Instruction::MemoryGrow => {
                // Grow the logical linear-memory size up to the pre-allocated
                // backing capacity (ctx.mem_cap). Backing never moves, so
                // s2 (mem_base) stays valid; only s5 (mem_size) changes.
                self.pop_gp_to(Reg::T0); // n (pages)
                self.emitter.zext_w(Reg::T0, Reg::T0);
                self.emitter.srli(Reg::T1, Reg::S5, 16); // current pages
                self.emitter.add(Reg::T2, Reg::T1, Reg::T0); // new pages
                // new_bytes = new_pages << 16; compare against mem_cap
                self.emitter.slli(Reg::T0, Reg::T2, 16);
                self.emitter.ld(Reg::T2, Reg::S1, ctx_off::MEM_CAP);
                let fail = self.emitter.new_label();
                let done = self.emitter.new_label();
                // fail when new_bytes > mem_cap, i.e. mem_cap < new_bytes
                self.emitter.bcc_label(cond::LTU, Reg::T2, Reg::T0, fail);
                // commit: mem_size = new_bytes; s5 = new_bytes; push old pages
                self.emitter.sd(Reg::S1, ctx_off::MEM_SIZE, Reg::T0);
                self.emitter.mv(Reg::S5, Reg::T0);
                self.push_gp(Reg::T1);
                self.emitter.jmp_label(done);
                self.emitter.bind_label(fail);
                self.emitter.li(Reg::T0, -1);
                self.push_gp(Reg::T0);
                self.emitter.bind_label(done);
            }

            // ── calls ───────────────────────────────────────────────
            Instruction::Call(idx) => {
                let type_idx = self.validation_info.functions_types[idx];
                let ft = &self.validation_info.types[type_idx];
                let param_count = ft.params.valtypes.len();
                let result_count = ft.returns.valtypes.len();

                let label = self.func_labels[idx];
                self.emitter.call_label(label);
                self.emit_post_call_reload();

                self.stack_depth = (self.stack_depth as isize + result_count as isize
                    - param_count as isize).max(0) as usize;
            }
            Instruction::CallIndirect(type_idx, _table_idx) => {
                let ft = &self.validation_info.types[type_idx as usize];
                let param_count = ft.params.valtypes.len();
                let result_count = ft.returns.valtypes.len();
                let expected_canon = self.canon_type_ids[type_idx as usize];

                self.pop_gp_to(Reg::T0); // table index
                self.emitter.zext_w(Reg::T0, Reg::T0);
                self.emitter.ld(Reg::T1, Reg::S1, ctx_off::TABLE_LEN);
                let ti = self.trap_indirect;
                self.emitter.bcc_label(cond::GEU, Reg::T0, Reg::T1, ti);
                self.emitter.ld(Reg::T1, Reg::S1, ctx_off::TABLE_PTR);
                self.emitter.slli(Reg::T0, Reg::T0, 3);
                self.emitter.add(Reg::T0, Reg::T0, Reg::T1);
                self.emitter.ld(Reg::T0, Reg::T0, 0);
                self.emitter.beqz_label(Reg::T0, ti);
                self.emitter.addi(Reg::T0, Reg::T0, -1); // func_idx
                // type check
                self.emitter.ld(Reg::T1, Reg::S1, ctx_off::FUNC_TYPE_IDS);
                self.emitter.slli(Reg::T2, Reg::T0, 2);
                self.emitter.add(Reg::T1, Reg::T1, Reg::T2);
                self.emitter.lwu(Reg::T1, Reg::T1, 0);
                self.emitter.li(Reg::T2, expected_canon as i64);
                self.emitter.bcc_label(cond::NE, Reg::T1, Reg::T2, ti);
                // load code address and call
                self.emitter.ld(Reg::T1, Reg::S1, ctx_off::FUNC_ADDRS);
                self.emitter.slli(Reg::T0, Reg::T0, 3);
                self.emitter.add(Reg::T1, Reg::T1, Reg::T0);
                self.emitter.ld(Reg::T1, Reg::T1, 0);
                self.emitter.call_reg(Reg::T1);
                self.emit_post_call_reload();

                self.stack_depth = (self.stack_depth as isize - 1 + result_count as isize
                    - param_count as isize).max(0) as usize;
            }

            // ── control flow ────────────────────────────────────────
            Instruction::Block(bt) => {
                let (pc, rc) = self.block_counts(&bt);
                let end_label = self.emitter.new_label();
                self.control_stack.push(ControlBlock {
                    kind: ControlBlockKind::Block,
                    stack_depth_before: self.stack_depth.saturating_sub(pc),
                    param_count: pc,
                    result_count: rc,
                    end_label,
                    else_label: None,
                    start_label: None,
                });
            }
            Instruction::Loop(bt) => {
                let (pc, rc) = self.block_counts(&bt);
                let start_label = self.emitter.new_label();
                let end_label = self.emitter.new_label();
                self.emitter.bind_label(start_label);
                self.control_stack.push(ControlBlock {
                    kind: ControlBlockKind::Loop,
                    stack_depth_before: self.stack_depth.saturating_sub(pc),
                    param_count: pc,
                    result_count: rc,
                    end_label,
                    else_label: None,
                    start_label: Some(start_label),
                });
            }
            Instruction::If(bt) => {
                let (pc, rc) = self.block_counts(&bt);
                self.pop_gp_to(Reg::T0);
                self.stack_depth = self.stack_depth.saturating_sub(1);
                let else_label = self.emitter.new_label();
                let end_label = self.emitter.new_label();
                self.emitter.beqz_label(Reg::T0, else_label);
                self.control_stack.push(ControlBlock {
                    kind: ControlBlockKind::If,
                    stack_depth_before: self.stack_depth.saturating_sub(pc),
                    param_count: pc,
                    result_count: rc,
                    end_label,
                    else_label: Some(else_label),
                    start_label: None,
                });
            }
            Instruction::Else => {
                let cb = self.control_stack.last_mut().expect("control underflow");
                let end_label = cb.end_label;
                let sd = cb.stack_depth_before + cb.param_count;
                if let Some(else_label) = cb.else_label.take() {
                    self.emitter.jmp_label(end_label);
                    self.emitter.bind_label(else_label);
                    self.stack_depth = sd;
                }
            }
            Instruction::End => {
                let cb = self.control_stack.pop().expect("control underflow");
                if let Some(else_label) = cb.else_label {
                    self.emitter.bind_label(else_label);
                }
                self.emitter.bind_label(cb.end_label);
                self.stack_depth = cb.stack_depth_before + cb.result_count;
            }
            Instruction::Br(label_idx) => {
                let (target_idx, target_label) = self.branch_target(label_idx);
                self.emit_unwind(target_idx);
                self.emitter.jmp_label(target_label);
            }
            Instruction::BrIf(label_idx) => {
                self.pop_gp_to(Reg::T0);
                self.stack_depth = self.stack_depth.saturating_sub(1);
                let skip = self.emitter.new_label();
                self.emitter.beqz_label(Reg::T0, skip);
                let (target_idx, target_label) = self.branch_target(label_idx);
                self.emit_unwind(target_idx);
                self.emitter.jmp_label(target_label);
                self.emitter.bind_label(skip);
            }
            Instruction::BrTable(targets, default) => {
                self.pop_gp_to(Reg::T2);
                self.stack_depth = self.stack_depth.saturating_sub(1);
                self.emitter.zext_w(Reg::T2, Reg::T2);
                for (i, target) in targets.iter().enumerate() {
                    let skip = self.emitter.new_label();
                    self.emitter.li(Reg::T1, i as i64);
                    self.emitter.bcc_label(cond::NE, Reg::T2, Reg::T1, skip);
                    let (target_idx, target_label) = self.branch_target(*target);
                    self.emit_unwind(target_idx);
                    self.emitter.jmp_label(target_label);
                    self.emitter.bind_label(skip);
                }
                let (default_idx, default_label) = self.branch_target(default);
                self.emit_unwind(default_idx);
                self.emitter.jmp_label(default_label);
            }
            Instruction::Return => {
                self.emit_unwind(0);
                let end = self.control_stack[0].end_label;
                self.emitter.jmp_label(end);
            }

            Instruction::FcExtension(sub) => self.compile_fc(sub, reader),
            Instruction::FdExtension(_sub) => {
                let t = self.trap_unimplemented;
                self.emitter.jmp_label(t);
            }
            Instruction::Atomic(_sub) => {
                let t = self.trap_unimplemented;
                self.emitter.jmp_label(t);
            }
            other => {
                crate::debugln!("AOT: unimplemented instruction {:?}", other);
                let t = self.trap_unimplemented;
                self.emitter.jmp_label(t);
            }
        }
    }

    fn compile_fc(&mut self, sub: u32, reader: &mut WasmReader) {
        match sub {
            // saturating truncations (RISC-V fcvt saturates; NaN forced to 0)
            0x00 => self.emit_trunc_sat(false, |e, d, a, b| e.feq_s(d, a, b), |e, d, f| e.fcvt_w_s(d, f, rm::RTZ)),
            0x01 => self.emit_trunc_sat(false, |e, d, a, b| e.feq_s(d, a, b), |e, d, f| e.fcvt_wu_s(d, f, rm::RTZ)),
            0x02 => self.emit_trunc_sat(true, |e, d, a, b| e.feq_d(d, a, b), |e, d, f| e.fcvt_w_d(d, f, rm::RTZ)),
            0x03 => self.emit_trunc_sat(true, |e, d, a, b| e.feq_d(d, a, b), |e, d, f| e.fcvt_wu_d(d, f, rm::RTZ)),
            0x04 => self.emit_trunc_sat(false, |e, d, a, b| e.feq_s(d, a, b), |e, d, f| e.fcvt_l_s(d, f, rm::RTZ)),
            0x05 => self.emit_trunc_sat(false, |e, d, a, b| e.feq_s(d, a, b), |e, d, f| e.fcvt_lu_s(d, f, rm::RTZ)),
            0x06 => self.emit_trunc_sat(true, |e, d, a, b| e.feq_d(d, a, b), |e, d, f| e.fcvt_l_d(d, f, rm::RTZ)),
            0x07 => self.emit_trunc_sat(true, |e, d, a, b| e.feq_d(d, a, b), |e, d, f| e.fcvt_lu_d(d, f, rm::RTZ)),
            0x0A => {
                // memory.copy
                let _ = reader.read_u8();
                let _ = reader.read_u8();
                self.pop_gp_to(Reg::T2); // len
                self.pop_gp_to(Reg::T1); // src
                self.pop_gp_to(Reg::T0); // dst
                self.stack_depth = self.stack_depth.saturating_sub(3);
                self.emitter.zext_w(Reg::T0, Reg::T0);
                self.emitter.zext_w(Reg::T1, Reg::T1);
                self.emitter.zext_w(Reg::T2, Reg::T2);
                self.emitter.add(Reg::A0, Reg::T0, Reg::T2);
                let oob = self.trap_oob;
                self.emitter.bcc_label(cond::LTU, Reg::S5, Reg::A0, oob);
                self.emitter.add(Reg::A0, Reg::T1, Reg::T2);
                self.emitter.bcc_label(cond::LTU, Reg::S5, Reg::A0, oob);
                self.emitter.add(Reg::T0, Reg::S2, Reg::T0);
                self.emitter.add(Reg::T1, Reg::S2, Reg::T1);
                let mm = self.helper_memmove;
                self.emitter.call_label(mm);
            }
            0x0B => {
                // memory.fill
                let _ = reader.read_u8();
                self.pop_gp_to(Reg::T2); // len
                self.pop_gp_to(Reg::T1); // value
                self.pop_gp_to(Reg::T0); // dst
                self.stack_depth = self.stack_depth.saturating_sub(3);
                self.emitter.zext_w(Reg::T0, Reg::T0);
                self.emitter.zext_w(Reg::T2, Reg::T2);
                self.emitter.add(Reg::A0, Reg::T0, Reg::T2);
                let oob = self.trap_oob;
                self.emitter.bcc_label(cond::LTU, Reg::S5, Reg::A0, oob);
                self.emitter.add(Reg::T0, Reg::S2, Reg::T0);
                let ms = self.helper_memset;
                self.emitter.call_label(ms);
            }
            0x09 => {
                // data.drop: no-op (segments stay resident in the module)
                let _ = reader.read_var_u32();
            }
            0x08 => {
                // memory.init (passive segments unsupported for now)
                let _ = reader.read_var_u32();
                let _ = reader.read_u8();
                let t = self.trap_unimplemented;
                self.emitter.jmp_label(t);
            }
            0x0C => {
                let _ = reader.read_var_u32();
                let _ = reader.read_var_u32();
                let t = self.trap_unimplemented;
                self.emitter.jmp_label(t);
            }
            0x0D => {
                let _ = reader.read_var_u32();
                let t = self.trap_unimplemented;
                self.emitter.jmp_label(t);
            }
            0x0E..=0x11 => {
                let _ = reader.read_var_u32();
                if sub == 0x0E {
                    let _ = reader.read_var_u32();
                }
                let t = self.trap_unimplemented;
                self.emitter.jmp_label(t);
            }
            _ => {
                let t = self.trap_unimplemented;
                self.emitter.jmp_label(t);
            }
        }
    }
}
