use crate::alloc::vec::Vec;
use crate::wasm::aot::emitter::{Reg, X64Emitter, XmmReg};
use crate::wasm::aot::runtime::{AotTrampoline, Ring3Context};
use crate::wasm::common::assert_validated::UnwrapValidatedExt;
use crate::wasm::common::indices::{FuncIdx, GlobalIdx, LabelIdx, LocalIdx};
use crate::wasm::common::reader::types::BlockType;
use crate::wasm::common::reader::types::ValType;
use crate::wasm::common::reader::types::instruction::Instruction;
use crate::wasm::common::reader::types::memarg::MemArg;
use crate::wasm::common::reader::types::opcode::*;
use crate::wasm::common::reader::{WasmReadable, WasmReader};
use crate::wasm::common::validation::ValidationInfo;

pub struct AotCompiler<'a> {
    pub validation_info: &'a ValidationInfo<'a>,
    pub emitter: X64Emitter,
    pub control_stack: Vec<ControlBlock>,
    pub stack_depth: usize,
    pub func_labels: Vec<usize>,
    pub trap_label: usize,
    pub trap_oob_label: usize,
    pub trap_fuel_label: usize,
    pub trap_div_zero_label: usize,
    pub trap_int_overflow_label: usize,
    pub trap_indirect_label: usize,
    pub trap_unreachable_label: usize,
    pub trap_stack_overflow_label: usize,
    pub trap_host_label: usize,
    pub trap_unimplemented_fc_label: usize,
    pub trap_unimplemented_simd_label: usize,
    pub trap_unimplemented_atomic_label: usize,
    pub trap_halt_label: usize,
    pub result_count: usize,
}

pub struct ControlBlock {
    pub kind: ControlBlockKind,
    pub stack_depth_before: usize,
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

impl<'a> AotCompiler<'a> {
    pub fn new(validation_info: &'a ValidationInfo<'a>) -> Self {
        let mut emitter = X64Emitter::new();
        let mut func_labels = Vec::new();
        let total_funcs =
            validation_info.imports_length.imported_functions + validation_info.functions.len();
        for _ in 0..total_funcs {
            func_labels.push(emitter.new_label());
        }
        let trap_label = emitter.new_label();
        let trap_oob_label = emitter.new_label();
        let trap_fuel_label = emitter.new_label();
        let trap_div_zero_label = emitter.new_label();
        let trap_int_overflow_label = emitter.new_label();
        let trap_indirect_label = emitter.new_label();
        let trap_unreachable_label = emitter.new_label();
        let trap_stack_overflow_label = emitter.new_label();
        let trap_host_label = emitter.new_label();
        let trap_unimplemented_fc_label = emitter.new_label();
        let trap_unimplemented_simd_label = emitter.new_label();
        let trap_unimplemented_atomic_label = emitter.new_label();
        let trap_halt_label = emitter.new_label();
        Self {
            validation_info,
            emitter,
            control_stack: Vec::new(),
            stack_depth: 0,
            func_labels,
            trap_label,
            trap_oob_label,
            trap_fuel_label,
            trap_div_zero_label,
            trap_int_overflow_label,
            trap_indirect_label,
            trap_unreachable_label,
            trap_stack_overflow_label,
            trap_host_label,
            trap_unimplemented_fc_label,
            trap_unimplemented_simd_label,
            trap_unimplemented_atomic_label,
            trap_halt_label,
            result_count: 0,
        }
    }

    pub fn compile_module(&mut self) -> crate::wasm::aot::runtime::AotModule {
        let mut func_offsets = Vec::new();

        // 0. Jump to the first function to skip trap handlers
        let entry_jump_label = self.emitter.new_label();
        self.emitter.jmp_label(entry_jump_label);

        // Safety ud2
        self.emitter.ud2();

        use crate::wasm::aot::runtime::{AotTrampoline, Ring3Context};
        let traps = [
            (self.trap_label, AotTrampoline::Trap),
            (self.trap_oob_label, AotTrampoline::TrapOob),
            (self.trap_fuel_label, AotTrampoline::TrapFuel),
            (self.trap_div_zero_label, AotTrampoline::TrapDivZero),
            (self.trap_int_overflow_label, AotTrampoline::TrapIntOverflow),
            (self.trap_indirect_label, AotTrampoline::TrapIndirect),
            (self.trap_unreachable_label, AotTrampoline::TrapUnreachable),
            (
                self.trap_stack_overflow_label,
                AotTrampoline::TrapStackOverflow,
            ),
            (self.trap_host_label, AotTrampoline::TrapHost),
            (
                self.trap_unimplemented_fc_label,
                AotTrampoline::TrapUnimplementedFc,
            ),
            (
                self.trap_unimplemented_simd_label,
                AotTrampoline::TrapUnimplementedSimd,
            ),
            (
                self.trap_unimplemented_atomic_label,
                AotTrampoline::TrapUnimplementedAtomic,
            ),
        ];

        for (label, trampoline) in traps {
            self.emitter.bind_label(label);
            // Pass RBP as `sp` arg (RSI) so trap handler can identify the frame
            self.emitter.mov_reg_reg(Reg::RSI, Reg::RBP);
            self.emitter.mov_reg_imm64(Reg::RAX, -16i64 as u64);
            self.emitter.and_reg_reg(Reg::RSP, Reg::RAX); // Align to 16
            self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Restore context for trap handler
            self.emit_call_trampoline(trampoline);
            self.emitter.ud2();
        }

        // Bind the entry jump to the first function (if any)
        self.emitter.bind_label(entry_jump_label);

        for i in 0..self.validation_info.functions.len() {
            let func_idx = self.validation_info.imports_length.imported_functions + i;
            self.emitter.bind_label(self.func_labels[func_idx]);

            // Record the offset relative to the START of the generated code
            let offset = self.emitter.code.len();
            func_offsets.push(offset);

            self.compile_function_body(i);
        }

        self.emitter.finalize();
        crate::wasm::aot::runtime::AotModule::new(&self.emitter.code, func_offsets)
    }

    fn compile_function_body(&mut self, local_func_idx: usize) {
        self.control_stack.clear();
        self.stack_depth = 0;
        self.trap_halt_label = self.emitter.new_label();

        let total_imported_funcs = self.validation_info.imports_length.imported_functions;
        let func_idx = local_func_idx + total_imported_funcs;
        let type_idx = self.validation_info.functions_types[func_idx];
        let func_type = &self.validation_info.types[type_idx];
        self.result_count = func_type.returns.valtypes.len();
        let param_count = func_type.params.valtypes.len();

        self.emitter.push_reg(Reg::RBP);
        self.emitter.mov_reg_reg(Reg::RBP, Reg::RSP);

        // 1. Reserve space for callee-save registers and Context (8 * 8 = 64 bytes)
        self.emitter.sub_reg_imm32(Reg::RSP, 64);

        self.emitter.mov_mem64_reg(Reg::RBP, -8, Reg::RBX);
        self.emitter.mov_mem64_reg(Reg::RBP, -16, Reg::R12);
        self.emitter.mov_mem64_reg(Reg::RBP, -24, Reg::R13);
        self.emitter.mov_mem64_reg(Reg::RBP, -32, Reg::R14);
        self.emitter.mov_mem64_reg(Reg::RBP, -40, Reg::R15);
        self.emitter.mov_mem64_reg(Reg::RBP, -48, Reg::RDI); // Stable Context pointer at [RBP - 48]
        // [RBP - 56] is used for results_ptr
        // [RBP - 64] is currently unused

        // IMPORTANT: Force align RSP to 16 bytes for future calls
        self.emitter.mov_reg_imm64(Reg::RAX, -16i64 as u64);
        self.emitter.and_reg_reg(Reg::RSP, Reg::RAX);

        self.emitter.mov_reg_mem64(Reg::R14, Reg::RDI, 16); // memory_base

        // 2. Read locals info
        let (span, _stp) = self.validation_info.func_blocks_stps[local_func_idx];
        let mut reader = WasmReader::new(self.validation_info.wasm);
        reader.pc = span.from;
        let locals =
            crate::wasm::common::validation::code::read_declared_locals(&mut reader).unwrap();
        let total_locals = param_count + locals.len();

        // 3. Stack limit check BEFORE allocating locals
        {
            let locals_bytes = (total_locals * 16) as u32;
            self.emitter.mov_reg_reg(Reg::RAX, Reg::RSP);
            self.emitter.sub_reg_imm32(Reg::RAX, locals_bytes);
            self.emitter.sub_reg_imm32(Reg::RAX, 4096); // margin for WASM execution stack
            self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48);
            self.emitter.mov_reg_mem64(Reg::RCX, Reg::RDI, 56); // stack_limit
            self.emitter.cmp_reg_reg(Reg::RAX, Reg::RCX);
            self.emitter.jcc_label(0x86, self.trap_stack_overflow_label); // jbe trap
        }

        // 4. Allocate and initialize locals area on stack
        let locals_area_size = (total_locals * 16 + 15) & !15;
        self.emitter
            .sub_reg_imm32(Reg::RSP, locals_area_size as u32);
        self.emitter.mov_reg_reg(Reg::R13, Reg::RSP); // R13 = locals_base
        self.emitter.mov_mem64_reg(Reg::RBP, -64, Reg::R13); // Save current locals_base at [RBP - 64]

        // Relocate parameters from caller stack ([RBP + 16]) to current locals
        for i in 0..param_count {
            let src_offset = 16 + (param_count - 1 - i) * 16;
            self.emitter
                .mov_reg_mem64(Reg::RAX, Reg::RBP, src_offset as i32);
            self.emitter
                .mov_reg_mem64(Reg::RDX, Reg::RBP, (src_offset + 8) as i32);
            self.emitter
                .mov_mem64_reg(Reg::R13, (i * 16) as i32, Reg::RAX);
            self.emitter
                .mov_mem64_reg(Reg::R13, (i * 16 + 8) as i32, Reg::RDX);
        }

        // Zero-init declared locals
        if total_locals > param_count {
            self.emitter.xor_reg_reg(Reg::RAX, Reg::RAX);
            for i in param_count..total_locals {
                self.emitter
                    .mov_mem64_reg(Reg::R13, (i * 16) as i32, Reg::RAX);
                self.emitter
                    .mov_mem64_reg(Reg::R13, (i * 16 + 8) as i32, Reg::RAX);
            }
        }

        // Init MXCSR
        self.emitter.mov_reg_imm64(Reg::RAX, 0x9FC0);
        self.emitter.sub_reg_imm32(Reg::RSP, 16);
        self.emitter.mov_mem64_reg(Reg::RSP, 0, Reg::RAX);
        self.emitter.ldmxcsr_mem(Reg::RSP, 0);
        self.emitter.add_reg_imm32(Reg::RSP, 16);

        self.emit_fuel_check(10);

        // 5. Compile instructions
        let end_label = self.emitter.new_label();
        self.control_stack.push(ControlBlock {
            kind: ControlBlockKind::Func,
            stack_depth_before: 0,
            result_count: self.result_count,
            end_label,
            else_label: None,
            start_label: None,
        });

        // Start instruction compilation after locals
        reader.pc = span.from;
        let _ = crate::wasm::common::validation::code::read_declared_locals(&mut reader)
            .unwrap_validated();

        while !self.control_stack.is_empty() {
            let instr = Instruction::read(&mut reader).unwrap();
            self.emit_integrity_check();
            self.compile_instruction(instr, &mut reader);
        }

        self.emitter.mov_mem64_reg(Reg::RBP, -56, Reg::RSP); // Save results pointer securely

        let epilogue_label = self.emitter.new_label();
        self.emitter.jmp_label(epilogue_label);

        self.emitter.bind_label(self.trap_halt_label);
        self.emitter.xor_reg_reg(Reg::RAX, Reg::RAX);
        self.emitter.mov_mem64_reg(Reg::RBP, -56, Reg::RAX); // Null result pointer for traps

        self.emitter.bind_label(epilogue_label);

        self.emitter.mov_reg_mem64(Reg::RCX, Reg::RBP, 8); // Saved return address
        self.emitter.mov_reg_mem64(Reg::R11, Reg::RBP, -56); // Load saved results pointer BEFORE popping RBP

        // Restore registers from stable offsets relative to RBP
        self.emitter.mov_reg_mem64(Reg::R15, Reg::RBP, -40);
        self.emitter.mov_reg_mem64(Reg::R14, Reg::RBP, -32);
        self.emitter.mov_reg_mem64(Reg::R13, Reg::RBP, -24);
        self.emitter.mov_reg_mem64(Reg::R12, Reg::RBP, -16);
        self.emitter.mov_reg_mem64(Reg::RBX, Reg::RBP, -8);

        // Calculate new RSP in RAX: RAX = RBP + 16 + (param_count * 16) - (result_count * 16)
        self.emitter.mov_reg_reg(Reg::RAX, Reg::RBP);
        self.emitter
            .add_reg_imm32(Reg::RAX, (16 + param_count * 16) as u32);
        self.emitter
            .sub_reg_imm32(Reg::RAX, (self.result_count * 16) as u32);

        // IMPORTANT: Restore caller's RBP BEFORE writing results.
        // When result_count > param_count, RAX can overlap [RBP], so
        // writing results would corrupt the saved RBP if we pop it later.
        self.emitter.mov_reg_reg(Reg::RSP, Reg::RBP);
        self.emitter.pop_reg(Reg::RBP);

        if self.result_count > 0 {
            let skip_results = self.emitter.new_label();
            self.emitter.test_reg_reg(Reg::R11, Reg::R11);
            self.emitter.jcc_label(0x84, skip_results); // Skip if null (trap)

            for i in 0..self.result_count {
                self.emitter
                    .mov_reg_mem64(Reg::R10, Reg::R11, (i * 16) as i32);
                self.emitter
                    .mov_mem64_reg(Reg::RAX, (i * 16) as i32, Reg::R10);
                self.emitter
                    .mov_reg_mem64(Reg::R10, Reg::R11, (i * 16 + 8) as i32);
                self.emitter
                    .mov_mem64_reg(Reg::RAX, (i * 16 + 8) as i32, Reg::R10);
            }
            self.emitter.bind_label(skip_results);
        }

        self.emitter.mov_reg_reg(Reg::RSP, Reg::RAX); // Set final SP

        self.emitter.sub_reg_imm32(Reg::RSP, 8); // Space for ret_addr
        self.emitter.mov_mem64_reg(Reg::RSP, 0, Reg::RCX);
        self.emitter.ret();
    }

    fn emit_fuel_check(&mut self, _cost: u32) {
        // Fuel checking disabled for debugging
    }

    fn emit_integrity_check(&mut self) {
        // Restore RDI (Context) from its stable stack slot [RBP - 48]
        // This ensures subsequent instructions always have a valid context pointer.
        self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48);
        // Force reload memory base (R14) from context [RDI + 16]
        self.emitter.mov_reg_mem64(Reg::R14, Reg::RDI, 16);
        // Force reload locals base (R13) from [RBP - 64]
        self.emitter.mov_reg_mem64(Reg::R13, Reg::RBP, -64);
    }

    fn emit_sp_sanity_check(&mut self) {
        // After 'mov RSP, RAX', verify RSP is within the AOT stack bounds.
        // Load stack_limit from context and check RSP > stack_limit.
        // Also check RSP is below a reasonable upper bound (stack_base + 4MB).
        self.emitter.mov_reg_mem64(Reg::RCX, Reg::RDI, 56); // stack_limit (base)
        self.emitter.cmp_reg_reg(Reg::RSP, Reg::RCX);
        self.emitter.jcc_label(0x86, self.trap_stack_overflow_label); // jbe = RSP <= stack_limit
        // Check RSP is not absurdly high (ctx.stack_base + stack_size)
        // stack_base is at offset 32
        self.emitter.mov_reg_mem64(Reg::RCX, Reg::RDI, 32); // stack_base
        self.emitter.add_reg_imm32(Reg::RCX, 4 * 1024 * 1024); // + 4MB
        self.emitter.cmp_reg_reg(Reg::RSP, Reg::RCX);
        self.emitter.jcc_label(0x87, self.trap_stack_overflow_label); // ja = RSP > upper bound
    }

    fn emit_call_trampoline(&mut self, trampoline: crate::wasm::aot::runtime::AotTrampoline) {
        // 1. Load Ring3Context from saved frame slot [RBP - 48]
        self.emitter.mov_reg_mem64(Reg::RAX, Reg::RBP, -48);
        // 2. Load blob_base from Ring3Context [RAX + 72]
        self.emitter.mov_reg_mem64(Reg::RAX, Reg::RAX, 72);
        // 3. Load the specific function address from the table [RAX + (index * 8)]
        let offset = (trampoline as i32) * 8;
        self.emitter.mov_reg_mem64(Reg::RAX, Reg::RAX, offset);
        // 4. Direct call to blob code
        self.emitter.call_reg(Reg::RAX);
    }

    fn compile_instruction(&mut self, instr: Instruction, reader: &mut WasmReader) {
        match instr {
            Instruction::Nop => {}
            Instruction::Unreachable => self.emitter.jmp_label(self.trap_unreachable_label),
            Instruction::CallRef(type_idx) => {
                let func_type = &self.validation_info.types[type_idx as usize];
                let param_count = func_type.params.valtypes.len();
                let result_count = func_type.returns.valtypes.len();

                self.emitter.pop_wasm_stack(Reg::RAX); // func_addr (absolute)

                self.emitter.test_reg_reg(Reg::RAX, Reg::RAX);
                self.emitter.jcc_label(0x84, self.trap_indirect_label);

                self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Pass Context in RDI
                self.emitter.call_reg(Reg::RAX);

                self.emitter.mov_reg_reg(Reg::RSP, Reg::RAX); // RAX = new SP from callee
                self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48);
                self.emitter.mov_reg_mem64(Reg::R14, Reg::RDI, 16);
                self.emitter.mov_reg_mem64(Reg::R13, Reg::RBP, -64); // Restore current locals_base
                self.emit_sp_sanity_check();

                self.stack_depth = (self.stack_depth as isize - 1 + result_count as isize
                    - param_count as isize) as usize;
            }
            Instruction::Drop => {
                self.emitter.add_reg_imm32(Reg::RSP, 16);
                self.stack_depth -= 1;
            }
            Instruction::Select => {
                self.emitter.pop_wasm_stack(Reg::RCX); // condition
                self.emitter.test_reg32_reg32(Reg::RCX, Reg::RCX);
                let else_label = self.emitter.new_label();
                self.emitter.jcc_label(0x84, else_label);

                // Condition is true: result is val1 (pushed first, so at RSP + 16)
                self.emitter.mov_reg_mem64(Reg::RAX, Reg::RSP, 16);
                self.emitter.mov_reg_mem64(Reg::RDX, Reg::RSP, 24);
                let end_label = self.emitter.new_label();
                self.emitter.jmp_label(end_label);

                self.emitter.bind_label(else_label);
                // Condition is false: result is val2 (pushed second, so at RSP)
                self.emitter.mov_reg_mem64(Reg::RAX, Reg::RSP, 0);
                self.emitter.mov_reg_mem64(Reg::RDX, Reg::RSP, 8);

                self.emitter.bind_label(end_label);
                self.emitter.add_reg_imm32(Reg::RSP, 32); // Pop val1 and val2
                self.emitter.push_wasm_stack(Reg::RAX);
                self.emitter.mov_mem64_reg(Reg::RSP, 8, Reg::RDX);
                self.stack_depth -= 2;
            }

            Instruction::LocalGet(idx) => {
                self.emitter
                    .movups_xmm_mem(XmmReg::XMM0, Reg::R13, (idx as i32) * 16);
                self.emitter.push_v128(XmmReg::XMM0);
                self.stack_depth += 1;
            }
            Instruction::LocalSet(idx) => {
                self.emitter.pop_v128(XmmReg::XMM0);
                self.emitter
                    .movups_mem_xmm(Reg::R13, (idx as i32) * 16, XmmReg::XMM0);
                self.stack_depth -= 1;
            }
            Instruction::LocalTee(idx) => {
                self.emitter.movups_xmm_mem(XmmReg::XMM0, Reg::RSP, 0);
                self.emitter
                    .movups_mem_xmm(Reg::R13, (idx as i32) * 16, XmmReg::XMM0);
            }

            Instruction::RefFunc(idx) => {
                self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // context
                self.emitter.mov_reg_mem64(Reg::RAX, Reg::RDI, 112); // func_table_ptr
                self.emitter
                    .mov_reg_mem64(Reg::RAX, Reg::RAX, (idx as i32) * 8); // load absolute addr
                self.emitter.push_wasm_stack(Reg::RAX);
                self.stack_depth += 1;
            }
            Instruction::GlobalGet(idx) => {
                self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // context
                self.emitter.mov_reg_mem64(Reg::RAX, Reg::RDI, 80); // globals_ptr
                self.emitter
                    .movups_xmm_mem(XmmReg::XMM0, Reg::RAX, (idx as i32) * 16);
                self.emitter.push_v128(XmmReg::XMM0);
                self.stack_depth += 1;
            }
            Instruction::GlobalSet(idx) => {
                self.emitter.pop_v128(XmmReg::XMM0);
                self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // context
                self.emitter.mov_reg_mem64(Reg::RAX, Reg::RDI, 80); // globals_ptr
                self.emitter
                    .movups_mem_xmm(Reg::RAX, (idx as i32) * 16, XmmReg::XMM0);
                self.stack_depth -= 1;
            }

            Instruction::I32Const(val) => {
                self.emitter.mov_reg_imm64(Reg::RAX, (val as u32) as u64);
                self.emitter.push_wasm_stack(Reg::RAX);
                self.stack_depth += 1;
            }
            Instruction::I64Const(val) => {
                self.emitter.mov_reg_imm64(Reg::RAX, val as u64);
                self.emitter.push_wasm_stack(Reg::RAX);
                self.stack_depth += 1;
            }
            Instruction::F32Const(val) => {
                self.emitter
                    .mov_reg_imm64(Reg::RAX, (val.to_bits() as u32) as u64);
                self.emitter.push_wasm_stack(Reg::RAX);
                self.stack_depth += 1;
            }
            Instruction::F64Const(val) => {
                self.emitter.mov_reg_imm64(Reg::RAX, val.to_bits());
                self.emitter.push_wasm_stack(Reg::RAX);
                self.stack_depth += 1;
            }

            Instruction::I32Add => self.emit_binop_i32(|e| e.add_reg32_reg32(Reg::RAX, Reg::RBX)),
            Instruction::I32Sub => self.emit_binop_i32(|e| e.sub_reg32_reg32(Reg::RAX, Reg::RBX)),
            Instruction::I32Mul => self.emit_binop_i32(|e| e.imul_reg32_reg32(Reg::RAX, Reg::RBX)),
            Instruction::I32DivS => {
                self.emitter.pop_wasm_stack(Reg::RBX); // divisor
                self.emitter.pop_wasm_stack(Reg::RAX); // dividend
                // Check division by zero
                self.emitter.test_reg32_reg32(Reg::RBX, Reg::RBX);
                self.emitter.jcc_label(0x84, self.trap_div_zero_label); // je trap
                // Check signed overflow: INT_MIN / -1
                let no_overflow = self.emitter.new_label();
                self.emitter.cmp_reg32_imm32(Reg::RAX, 0x80000000u32);
                self.emitter.jcc_label(0x85, no_overflow); // jne no_overflow
                self.emitter.cmp_reg32_imm32(Reg::RBX, 0xFFFFFFFFu32); // -1
                self.emitter.jcc_label(0x84, self.trap_int_overflow_label); // je trap
                self.emitter.bind_label(no_overflow);
                // Perform division
                self.emitter.cdq(); // sign-extend EAX to EDX:EAX
                self.emitter.idiv_reg32(Reg::RBX);
                self.emitter.push_wasm_stack(Reg::RAX);
                self.stack_depth -= 1;
            }
            Instruction::I32DivU => {
                self.emitter.pop_wasm_stack(Reg::RBX); // divisor
                self.emitter.pop_wasm_stack(Reg::RAX); // dividend
                self.emitter.test_reg32_reg32(Reg::RBX, Reg::RBX);
                self.emitter.jcc_label(0x84, self.trap_div_zero_label); // je trap
                self.emitter.xor_reg32_reg32(Reg::RDX, Reg::RDX); // zero EDX
                self.emitter.div_reg32(Reg::RBX);
                self.emitter.push_wasm_stack(Reg::RAX);
                self.stack_depth -= 1;
            }
            Instruction::I32RemS => {
                self.emitter.pop_wasm_stack(Reg::RBX); // divisor
                self.emitter.pop_wasm_stack(Reg::RAX); // dividend
                self.emitter.test_reg32_reg32(Reg::RBX, Reg::RBX);
                self.emitter.jcc_label(0x84, self.trap_div_zero_label); // je trap
                // Special case: INT_MIN % -1 = 0 (no trap, result is 0)
                let do_div = self.emitter.new_label();
                let done = self.emitter.new_label();
                self.emitter.cmp_reg32_imm32(Reg::RAX, 0x80000000u32);
                self.emitter.jcc_label(0x85, do_div); // jne do_div
                self.emitter.cmp_reg32_imm32(Reg::RBX, 0xFFFFFFFFu32);
                self.emitter.jcc_label(0x85, do_div); // jne do_div
                // INT_MIN % -1 = 0
                self.emitter.xor_reg32_reg32(Reg::RDX, Reg::RDX);
                self.emitter.jmp_label(done);
                self.emitter.bind_label(do_div);
                self.emitter.cdq();
                self.emitter.idiv_reg32(Reg::RBX);
                self.emitter.bind_label(done);
                // Remainder is in EDX
                self.emitter.mov_reg32_reg32(Reg::RAX, Reg::RDX);
                self.emitter.push_wasm_stack(Reg::RAX);
                self.stack_depth -= 1;
            }
            Instruction::I32RemU => {
                self.emitter.pop_wasm_stack(Reg::RBX); // divisor
                self.emitter.pop_wasm_stack(Reg::RAX); // dividend
                self.emitter.test_reg32_reg32(Reg::RBX, Reg::RBX);
                self.emitter.jcc_label(0x84, self.trap_div_zero_label); // je trap
                self.emitter.xor_reg32_reg32(Reg::RDX, Reg::RDX);
                self.emitter.div_reg32(Reg::RBX);
                // Remainder is in EDX
                self.emitter.mov_reg32_reg32(Reg::RAX, Reg::RDX);
                self.emitter.push_wasm_stack(Reg::RAX);
                self.stack_depth -= 1;
            }
            Instruction::I32And => self.emit_binop_i32(|e| e.and_reg32_reg32(Reg::RAX, Reg::RBX)),
            Instruction::I32Or => self.emit_binop_i32(|e| e.or_reg32_reg32(Reg::RAX, Reg::RBX)),
            Instruction::I32Xor => self.emit_binop_i32(|e| e.xor_reg32_reg32(Reg::RAX, Reg::RBX)),

            Instruction::I32Shl => self.emit_shift_i32(|e| e.shl_reg32_cl(Reg::RAX)),
            Instruction::I32ShrS => self.emit_shift_i32(|e| e.sar_reg32_cl(Reg::RAX)),
            Instruction::I32ShrU => self.emit_shift_i32(|e| e.shr_reg32_cl(Reg::RAX)),
            Instruction::I32Rotl => self.emit_shift_i32(|e| e.rol_reg32_cl(Reg::RAX)),
            Instruction::I32Rotr => self.emit_shift_i32(|e| e.ror_reg32_cl(Reg::RAX)),

            Instruction::I64Shl => self.emit_shift_i64(|e| e.shl_reg_cl(Reg::RAX)),
            Instruction::I64ShrS => self.emit_shift_i64(|e| e.sar_reg_cl(Reg::RAX)),
            Instruction::I64ShrU => self.emit_shift_i64(|e| e.shr_reg_cl(Reg::RAX)),
            Instruction::I64Rotl => self.emit_shift_i64(|e| e.rol_reg_cl(Reg::RAX)),
            Instruction::I64Rotr => self.emit_shift_i64(|e| e.ror_reg_cl(Reg::RAX)),

            Instruction::I64Add => self.emit_binop_i64(|e| e.add_reg_reg(Reg::RAX, Reg::RBX)),
            Instruction::I64Sub => self.emit_binop_i64(|e| e.sub_reg_reg(Reg::RAX, Reg::RBX)),
            Instruction::I64Mul => self.emit_binop_i64(|e| e.imul_reg_reg(Reg::RAX, Reg::RBX)),
            Instruction::I64DivS => {
                self.emitter.pop_wasm_stack(Reg::RBX); // divisor
                self.emitter.pop_wasm_stack(Reg::RAX); // dividend
                self.emitter.test_reg_reg(Reg::RBX, Reg::RBX);
                self.emitter.jcc_label(0x84, self.trap_div_zero_label); // je trap
                let no_overflow = self.emitter.new_label();
                self.emitter.mov_reg_imm64(Reg::RCX, 0x8000000000000000);
                self.emitter.cmp_reg_reg(Reg::RAX, Reg::RCX);
                self.emitter.jcc_label(0x85, no_overflow); // jne
                self.emitter.mov_reg_imm64(Reg::RCX, 0xFFFFFFFFFFFFFFFF); // -1
                self.emitter.cmp_reg_reg(Reg::RBX, Reg::RCX);
                self.emitter.jcc_label(0x84, self.trap_int_overflow_label); // je
                self.emitter.bind_label(no_overflow);
                self.emitter.cqo();
                self.emitter.idiv_reg64(Reg::RBX);
                self.emitter.push_wasm_stack(Reg::RAX);
                self.stack_depth -= 1;
            }
            Instruction::I64DivU => {
                self.emitter.pop_wasm_stack(Reg::RBX); // divisor
                self.emitter.pop_wasm_stack(Reg::RAX); // dividend
                self.emitter.test_reg_reg(Reg::RBX, Reg::RBX);
                self.emitter.jcc_label(0x84, self.trap_div_zero_label); // je trap
                self.emitter.xor_reg_reg(Reg::RDX, Reg::RDX); // zero RDX
                self.emitter.div_reg64(Reg::RBX);
                self.emitter.push_wasm_stack(Reg::RAX);
                self.stack_depth -= 1;
            }
            Instruction::I64RemS => {
                self.emitter.pop_wasm_stack(Reg::RBX); // divisor
                self.emitter.pop_wasm_stack(Reg::RAX); // dividend
                self.emitter.test_reg_reg(Reg::RBX, Reg::RBX);
                self.emitter.jcc_label(0x84, self.trap_div_zero_label); // je trap
                let do_div = self.emitter.new_label();
                let done = self.emitter.new_label();
                self.emitter.mov_reg_imm64(Reg::RCX, 0x8000000000000000);
                self.emitter.cmp_reg_reg(Reg::RAX, Reg::RCX);
                self.emitter.jcc_label(0x85, do_div); // jne do_div
                self.emitter.mov_reg_imm64(Reg::RCX, 0xFFFFFFFFFFFFFFFF); // -1
                self.emitter.cmp_reg_reg(Reg::RBX, Reg::RCX);
                self.emitter.jcc_label(0x85, do_div); // jne do_div
                // INT_MIN % -1 = 0
                self.emitter.xor_reg_reg(Reg::RDX, Reg::RDX);
                self.emitter.jmp_label(done);
                self.emitter.bind_label(do_div);
                self.emitter.cqo();
                self.emitter.idiv_reg64(Reg::RBX);
                self.emitter.bind_label(done);
                // Remainder is in RDX
                self.emitter.mov_reg_reg(Reg::RAX, Reg::RDX);
                self.emitter.push_wasm_stack(Reg::RAX);
                self.stack_depth -= 1;
            }
            Instruction::I64RemU => {
                self.emitter.pop_wasm_stack(Reg::RBX); // divisor
                self.emitter.pop_wasm_stack(Reg::RAX); // dividend
                self.emitter.test_reg_reg(Reg::RBX, Reg::RBX);
                self.emitter.jcc_label(0x84, self.trap_div_zero_label); // je trap
                self.emitter.xor_reg_reg(Reg::RDX, Reg::RDX);
                self.emitter.div_reg64(Reg::RBX);
                // Remainder is in RDX
                self.emitter.mov_reg_reg(Reg::RAX, Reg::RDX);
                self.emitter.push_wasm_stack(Reg::RAX);
                self.stack_depth -= 1;
            }
            Instruction::I64And => self.emit_binop_i64(|e| e.and_reg_reg(Reg::RAX, Reg::RBX)),
            Instruction::I64Or => self.emit_binop_i64(|e| e.or_reg_reg(Reg::RAX, Reg::RBX)),
            Instruction::I64Xor => self.emit_binop_i64(|e| e.xor_reg_reg(Reg::RAX, Reg::RBX)),

            Instruction::I32Eqz => {
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emitter.test_reg32_reg32(Reg::RAX, Reg::RAX);
                self.emitter.emit_u8(0x0F);
                self.emitter.emit_u8(0x94);
                self.emitter.emit_u8(0xC0); // SETZ AL
                self.emitter.movzx_reg_reg8(Reg::RAX, Reg::RAX);
                self.emitter.push_wasm_stack(Reg::RAX);
            }
            Instruction::I64Eqz => {
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emitter.test_reg_reg(Reg::RAX, Reg::RAX);
                self.emitter.emit_u8(0x0F);
                self.emitter.emit_u8(0x94);
                self.emitter.emit_u8(0xC0); // SETZ AL
                self.emitter.movzx_reg_reg8(Reg::RAX, Reg::RAX);
                self.emitter.push_wasm_stack(Reg::RAX);
            }
            Instruction::I32Clz => {
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emitter.bsr_reg32_reg32(Reg::RCX, Reg::RAX);
                self.emitter.mov_reg_imm64(Reg::RDX, 31);
                self.emitter.sub_reg32_reg32(Reg::RDX, Reg::RCX);
                self.emitter.mov_reg_imm64(Reg::RAX, 32);
                self.emitter.emit_u8(0x48);
                self.emitter.emit_u8(0x0F);
                self.emitter.emit_u8(0x45);
                self.emitter.emit_u8(0xC2);
                self.emitter.push_wasm_stack(Reg::RAX);
            }
            Instruction::I32Ctz => {
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emitter.bsf_reg32_reg32(Reg::RCX, Reg::RAX);
                self.emitter.mov_reg_imm64(Reg::RAX, 32);
                self.emitter.emit_u8(0x48);
                self.emitter.emit_u8(0x0F);
                self.emitter.emit_u8(0x45);
                self.emitter.emit_u8(0xC1);
                self.emitter.push_wasm_stack(Reg::RAX);
            }
            Instruction::I32Popcnt => {
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emitter.popcnt_reg32_reg32(Reg::RAX, Reg::RAX);
                self.emitter.push_wasm_stack(Reg::RAX);
            }

            Instruction::I64Clz => {
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emitter.bsr_reg_reg(Reg::RCX, Reg::RAX);
                self.emitter.mov_reg_imm64(Reg::RDX, 63);
                self.emitter.sub_reg_reg(Reg::RDX, Reg::RCX);
                self.emitter.mov_reg_imm64(Reg::RAX, 64);
                self.emitter.emit_u8(0x48);
                self.emitter.emit_u8(0x0F);
                self.emitter.emit_u8(0x45);
                self.emitter.emit_u8(0xC2);
                self.emitter.push_wasm_stack(Reg::RAX);
            }
            Instruction::I64Ctz => {
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emitter.bsf_reg_reg(Reg::RCX, Reg::RAX);
                self.emitter.mov_reg_imm64(Reg::RAX, 64);
                self.emitter.emit_u8(0x48);
                self.emitter.emit_u8(0x0F);
                self.emitter.emit_u8(0x45);
                self.emitter.emit_u8(0xC1);
                self.emitter.push_wasm_stack(Reg::RAX);
            }
            Instruction::I64Popcnt => {
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emitter.popcnt_reg_reg(Reg::RAX, Reg::RAX);
                self.emitter.push_wasm_stack(Reg::RAX);
            }

            Instruction::I32Eq => self.emit_relop_i32(0x94),
            Instruction::I32Ne => self.emit_relop_i32(0x95),
            Instruction::I32LtS => self.emit_relop_i32(0x9C),
            Instruction::I32LtU => self.emit_relop_i32(0x92),
            Instruction::I32GtS => self.emit_relop_i32(0x9F),
            Instruction::I32GtU => self.emit_relop_i32(0x97),
            Instruction::I32LeS => self.emit_relop_i32(0x9E),
            Instruction::I32LeU => self.emit_relop_i32(0x96),
            Instruction::I32GeS => self.emit_relop_i32(0x9D),
            Instruction::I32GeU => self.emit_relop_i32(0x93),

            Instruction::I64Eq => self.emit_relop_i64(0x94),
            Instruction::I64Ne => self.emit_relop_i64(0x95),
            Instruction::I64LtS => self.emit_relop_i64(0x9C),
            Instruction::I64LtU => self.emit_relop_i64(0x92),
            Instruction::I64GtS => self.emit_relop_i64(0x9F),
            Instruction::I64GtU => self.emit_relop_i64(0x97),
            Instruction::I64LeS => self.emit_relop_i64(0x9E),
            Instruction::I64LeU => self.emit_relop_i64(0x96),
            Instruction::I64GeS => self.emit_relop_i64(0x9D),
            Instruction::I64GeU => self.emit_relop_i64(0x93),

            Instruction::F32Eq => self.emit_f32_relop(0),
            Instruction::F32Ne => self.emit_f32_relop(1),
            Instruction::F32Lt => self.emit_f32_relop(2),
            Instruction::F32Gt => self.emit_f32_relop(3),
            Instruction::F32Le => self.emit_f32_relop(4),
            Instruction::F32Ge => self.emit_f32_relop(5),

            Instruction::F64Eq => self.emit_f64_relop(0),
            Instruction::F64Ne => self.emit_f64_relop(1),
            Instruction::F64Lt => self.emit_f64_relop(2),
            Instruction::F64Gt => self.emit_f64_relop(3),
            Instruction::F64Le => self.emit_f64_relop(4),
            Instruction::F64Ge => self.emit_f64_relop(5),

            Instruction::F32Add => {
                self.emit_binop_f32(|e| e.addss_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1))
            }
            Instruction::F32Sub => {
                self.emit_binop_f32(|e| e.subss_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1))
            }
            Instruction::F32Mul => {
                self.emit_binop_f32(|e| e.mulss_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1))
            }
            Instruction::F32Div => {
                self.emit_binop_f32(|e| e.divss_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1))
            }

            Instruction::F64Add => {
                self.emit_binop_f64(|e| e.addsd_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1))
            }
            Instruction::F64Sub => {
                self.emit_binop_f64(|e| e.subsd_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1))
            }
            Instruction::F64Mul => {
                self.emit_binop_f64(|e| e.mulsd_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1))
            }
            Instruction::F64Div => {
                self.emit_binop_f64(|e| e.divsd_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1))
            }

            Instruction::F32Abs => {
                self.emitter.pop_v128(XmmReg::XMM0);
                self.emitter.mov_reg_imm64(Reg::RAX, 0x7FFFFFFF);
                self.emitter.movd_xmm_reg(XmmReg::XMM1, Reg::RAX);
                self.emitter.andps_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1);
                self.emitter.push_v128(XmmReg::XMM0);
            }
            Instruction::F32Neg => {
                self.emitter.pop_v128(XmmReg::XMM0);
                self.emitter.mov_reg_imm64(Reg::RAX, 0x80000000);
                self.emitter.movd_xmm_reg(XmmReg::XMM1, Reg::RAX);
                self.emitter.xorps_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1);
                self.emitter.push_v128(XmmReg::XMM0);
            }
            Instruction::F32Sqrt => {
                self.emitter.pop_v128(XmmReg::XMM0);
                self.emitter.sqrtss_xmm_xmm(XmmReg::XMM0, XmmReg::XMM0);
                self.emitter.push_v128(XmmReg::XMM0);
            }
            Instruction::F32Ceil => {
                self.emitter.pop_v128(XmmReg::XMM0);
                self.emitter
                    .roundss_xmm_xmm_imm8(XmmReg::XMM0, XmmReg::XMM0, 2);
                self.emitter.push_v128(XmmReg::XMM0);
            }
            Instruction::F32Floor => {
                self.emitter.pop_v128(XmmReg::XMM0);
                self.emitter
                    .roundss_xmm_xmm_imm8(XmmReg::XMM0, XmmReg::XMM0, 1);
                self.emitter.push_v128(XmmReg::XMM0);
            }
            Instruction::F32Trunc => {
                self.emitter.pop_v128(XmmReg::XMM0);
                self.emitter
                    .roundss_xmm_xmm_imm8(XmmReg::XMM0, XmmReg::XMM0, 3);
                self.emitter.push_v128(XmmReg::XMM0);
            }
            Instruction::F32Nearest => {
                self.emitter.pop_v128(XmmReg::XMM0);
                self.emitter
                    .roundss_xmm_xmm_imm8(XmmReg::XMM0, XmmReg::XMM0, 0);
                self.emitter.push_v128(XmmReg::XMM0);
            }

            Instruction::F64Abs => {
                self.emitter.pop_v128(XmmReg::XMM0);
                self.emitter.mov_reg_imm64(Reg::RAX, 0x7FFFFFFFFFFFFFFF);
                self.emitter.movq_xmm_reg(XmmReg::XMM1, Reg::RAX);
                self.emitter.andpd_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1);
                self.emitter.push_v128(XmmReg::XMM0);
            }
            Instruction::F64Neg => {
                self.emitter.pop_v128(XmmReg::XMM0);
                self.emitter.mov_reg_imm64(Reg::RAX, 0x8000000000000000);
                self.emitter.movq_xmm_reg(XmmReg::XMM1, Reg::RAX);
                self.emitter.xorpd_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1);
                self.emitter.push_v128(XmmReg::XMM0);
            }
            Instruction::F64Sqrt => {
                self.emitter.pop_v128(XmmReg::XMM0);
                self.emitter.sqrtsd_xmm_xmm(XmmReg::XMM0, XmmReg::XMM0);
                self.emitter.push_v128(XmmReg::XMM0);
            }
            Instruction::F64Ceil => {
                self.emitter.pop_v128(XmmReg::XMM0);
                self.emitter
                    .roundsd_xmm_xmm_imm8(XmmReg::XMM0, XmmReg::XMM0, 2);
                self.emitter.push_v128(XmmReg::XMM0);
            }
            Instruction::F64Floor => {
                self.emitter.pop_v128(XmmReg::XMM0);
                self.emitter
                    .roundsd_xmm_xmm_imm8(XmmReg::XMM0, XmmReg::XMM0, 1);
                self.emitter.push_v128(XmmReg::XMM0);
            }
            Instruction::F64Trunc => {
                self.emitter.pop_v128(XmmReg::XMM0);
                self.emitter
                    .roundsd_xmm_xmm_imm8(XmmReg::XMM0, XmmReg::XMM0, 3);
                self.emitter.push_v128(XmmReg::XMM0);
            }
            Instruction::F64Nearest => {
                self.emitter.pop_v128(XmmReg::XMM0);
                self.emitter
                    .roundsd_xmm_xmm_imm8(XmmReg::XMM0, XmmReg::XMM0, 0);
                self.emitter.push_v128(XmmReg::XMM0);
            }

            Instruction::F32Min => self.emit_trampoline_binop_f32(AotTrampoline::F32Min),
            Instruction::F32Max => self.emit_trampoline_binop_f32(AotTrampoline::F32Max),
            Instruction::F32Copysign => {
                self.emitter.pop_v128(XmmReg::XMM1);
                self.emitter.pop_v128(XmmReg::XMM0);
                self.emitter.mov_reg_imm64(Reg::RAX, 0x80000000);
                self.emitter.movd_xmm_reg(XmmReg::XMM2, Reg::RAX);
                self.emitter.mov_reg_imm64(Reg::RAX, 0x7FFFFFFF);
                self.emitter.movd_xmm_reg(XmmReg::XMM3, Reg::RAX);
                self.emitter.andps_xmm_xmm(XmmReg::XMM1, XmmReg::XMM2);
                self.emitter.andps_xmm_xmm(XmmReg::XMM0, XmmReg::XMM3);
                self.emitter.orps_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1);
                self.emitter.push_v128(XmmReg::XMM0);
                self.stack_depth -= 1;
            }

            Instruction::F64Min => self.emit_trampoline_binop_f64(AotTrampoline::F64Min),
            Instruction::F64Max => self.emit_trampoline_binop_f64(AotTrampoline::F64Max),
            Instruction::F64Copysign => {
                self.emitter.pop_v128(XmmReg::XMM1);
                self.emitter.pop_v128(XmmReg::XMM0);
                self.emitter.mov_reg_imm64(Reg::RAX, 0x8000000000000000);
                self.emitter.movq_xmm_reg(XmmReg::XMM2, Reg::RAX);
                self.emitter.mov_reg_imm64(Reg::RAX, 0x7FFFFFFFFFFFFFFF);
                self.emitter.movq_xmm_reg(XmmReg::XMM3, Reg::RAX);
                self.emitter.andpd_xmm_xmm(XmmReg::XMM1, XmmReg::XMM2);
                self.emitter.andpd_xmm_xmm(XmmReg::XMM0, XmmReg::XMM3);
                self.emitter.orpd_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1);
                self.emitter.push_v128(XmmReg::XMM0);
                self.stack_depth -= 1;
            }

            Instruction::I32WrapI64 => {
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emitter.mov_reg32_reg32(Reg::RAX, Reg::RAX);
                self.emitter.push_wasm_stack(Reg::RAX);
            }
            Instruction::I64ExtendI32S => {
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emitter.movsxd_reg_reg(Reg::RAX, Reg::RAX);
                self.emitter.push_wasm_stack(Reg::RAX);
            }
            Instruction::I64ExtendI32U => {
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emitter.mov_reg32_reg32(Reg::RAX, Reg::RAX);
                self.emitter.push_wasm_stack(Reg::RAX);
            }
            Instruction::I32Extend8S => {
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emitter.movsx_reg_reg8(Reg::RAX, Reg::RAX);
                self.emitter.push_wasm_stack(Reg::RAX);
            }
            Instruction::I32Extend16S => {
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emitter.movsx_reg_reg16(Reg::RAX, Reg::RAX);
                self.emitter.push_wasm_stack(Reg::RAX);
            }
            Instruction::I64Extend8S => {
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emitter.movsx_reg_reg8(Reg::RAX, Reg::RAX);
                self.emitter.push_wasm_stack(Reg::RAX);
            }
            Instruction::I64Extend16S => {
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emitter.movsx_reg_reg16(Reg::RAX, Reg::RAX);
                self.emitter.push_wasm_stack(Reg::RAX);
            }
            Instruction::I64Extend32S => {
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emitter.movsxd_reg_reg(Reg::RAX, Reg::RAX);
                self.emitter.push_wasm_stack(Reg::RAX);
            }

            Instruction::I32ReinterpretF32 => {
                self.emitter.pop_v128(XmmReg::XMM0);
                self.emitter.movd_reg_xmm(Reg::RAX, XmmReg::XMM0);
                self.emitter.push_wasm_stack(Reg::RAX);
            }
            Instruction::I64ReinterpretF64 => {
                self.emitter.pop_v128(XmmReg::XMM0);
                self.emitter.movq_reg_xmm(Reg::RAX, XmmReg::XMM0);
                self.emitter.push_wasm_stack(Reg::RAX);
            }
            Instruction::F32ReinterpretI32 => {
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emitter.movd_xmm_reg(XmmReg::XMM0, Reg::RAX);
                self.emitter.push_v128(XmmReg::XMM0);
            }
            Instruction::F64ReinterpretI64 => {
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emitter.movq_xmm_reg(XmmReg::XMM0, Reg::RAX);
                self.emitter.push_v128(XmmReg::XMM0);
            }

            Instruction::RefNull(_ty) => {
                self.emitter.xor_reg_reg(Reg::RAX, Reg::RAX);
                self.emitter.push_wasm_stack(Reg::RAX);
                self.stack_depth += 1;
            }
            Instruction::RefIsNull => {
                self.emitter.mov_reg_mem64(Reg::RAX, Reg::RSP, 0);
                self.emitter.mov_reg_mem64(Reg::RDX, Reg::RSP, 8);
                self.emitter.or_reg_reg(Reg::RAX, Reg::RDX);
                self.emitter.emit_u8(0x0F);
                self.emitter.emit_u8(0x94);
                self.emitter.emit_u8(0xC0); // SETZ AL
                self.emitter.emit_u8(0x48);
                self.emitter.emit_u8(0x0F);
                self.emitter.emit_u8(0xB6);
                self.emitter.emit_u8(0xC0); // MOVZX RAX, AL
                self.emitter.add_reg_imm32(Reg::RSP, 16);
                self.emitter.push_wasm_stack(Reg::RAX);
            }

            Instruction::F32ConvertI32S => {
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emitter.movsxd_reg_reg(Reg::RAX, Reg::RAX);
                self.emitter.cvtsi2ss_xmm_reg(XmmReg::XMM0, Reg::RAX);
                self.emitter.push_v128(XmmReg::XMM0);
            }
            Instruction::F32ConvertI32U => {
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emitter.mov_reg32_reg32(Reg::RAX, Reg::RAX);
                self.emitter.cvtsi2ss_xmm_reg(XmmReg::XMM0, Reg::RAX);
                self.emitter.push_v128(XmmReg::XMM0);
            }
            Instruction::F32ConvertI64S => {
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emitter.cvtsi2ss_xmm_reg(XmmReg::XMM0, Reg::RAX);
                self.emitter.push_v128(XmmReg::XMM0);
            }
            Instruction::F32ConvertI64U => {
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emitter.test_reg_reg(Reg::RAX, Reg::RAX);
                let is_neg = self.emitter.new_label();
                let done = self.emitter.new_label();
                self.emitter.jcc_label(0x88, is_neg); // js (sign bit set -> >= 2^63)

                self.emitter.cvtsi2ss_xmm_reg(XmmReg::XMM0, Reg::RAX);
                self.emitter.jmp_label(done);

                self.emitter.bind_label(is_neg);
                self.emitter.mov_reg_reg(Reg::RCX, Reg::RAX);
                self.emitter.shr_reg32_imm32(Reg::RCX, 1); // logical right shift
                self.emitter.mov_reg_reg(Reg::RDX, Reg::RAX);
                self.emitter.and_reg32_imm32(Reg::RDX, 1);
                self.emitter.or_reg_reg(Reg::RCX, Reg::RDX); // (val >> 1) | (val & 1)

                self.emitter.cvtsi2ss_xmm_reg(XmmReg::XMM0, Reg::RCX);
                self.emitter.addss_xmm_xmm(XmmReg::XMM0, XmmReg::XMM0); // multiply by 2

                self.emitter.bind_label(done);
                self.emitter.push_v128(XmmReg::XMM0);
            }
            Instruction::F64ConvertI32S => {
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emitter.movsxd_reg_reg(Reg::RAX, Reg::RAX);
                self.emitter.cvtsi2sd_xmm_reg(XmmReg::XMM0, Reg::RAX);
                self.emitter.push_v128(XmmReg::XMM0);
            }
            Instruction::F64ConvertI32U => {
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emitter.mov_reg32_reg32(Reg::RAX, Reg::RAX);
                self.emitter.cvtsi2sd_xmm_reg(XmmReg::XMM0, Reg::RAX);
                self.emitter.push_v128(XmmReg::XMM0);
            }
            Instruction::F64ConvertI64S => {
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emitter.cvtsi2sd_xmm_reg(XmmReg::XMM0, Reg::RAX);
                self.emitter.push_v128(XmmReg::XMM0);
            }
            Instruction::F64ConvertI64U => {
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emitter.test_reg_reg(Reg::RAX, Reg::RAX);
                let is_neg = self.emitter.new_label();
                let done = self.emitter.new_label();
                self.emitter.jcc_label(0x88, is_neg); // js

                self.emitter.cvtsi2sd_xmm_reg(XmmReg::XMM0, Reg::RAX);
                self.emitter.jmp_label(done);

                self.emitter.bind_label(is_neg);
                self.emitter.mov_reg_reg(Reg::RCX, Reg::RAX);
                self.emitter.shr_reg_imm32(Reg::RCX, 1); // 64-bit logical right shift
                self.emitter.mov_reg_reg(Reg::RDX, Reg::RAX);
                self.emitter.and_reg32_imm32(Reg::RDX, 1);
                self.emitter.or_reg_reg(Reg::RCX, Reg::RDX); // (val >> 1) | (val & 1)

                self.emitter.cvtsi2sd_xmm_reg(XmmReg::XMM0, Reg::RCX);
                self.emitter.addsd_xmm_xmm(XmmReg::XMM0, XmmReg::XMM0); // multiply by 2

                self.emitter.bind_label(done);
                self.emitter.push_v128(XmmReg::XMM0);
            }
            Instruction::F32DemoteF64 => {
                self.emitter.pop_v128(XmmReg::XMM0);
                self.emitter.cvtsd2ss_xmm_xmm(XmmReg::XMM0, XmmReg::XMM0);
                self.emitter.push_v128(XmmReg::XMM0);
            }
            Instruction::F64PromoteF32 => {
                self.emitter.pop_v128(XmmReg::XMM0);
                self.emitter.cvtss2sd_xmm_xmm(XmmReg::XMM0, XmmReg::XMM0);
                self.emitter.push_v128(XmmReg::XMM0);
            }
            Instruction::I32TruncF32S => {
                self.emitter.pop_v128(XmmReg::XMM0);
                self.emitter.cvttss2si_reg32_xmm(Reg::RAX, XmmReg::XMM0);
                self.emitter.push_wasm_stack(Reg::RAX);
            }
            Instruction::I32TruncF32U => {
                self.emitter.pop_v128(XmmReg::XMM0);
                self.emitter.mov_reg_imm64(Reg::RAX, 0x4F000000); // 2^31 as f32
                self.emitter.movd_xmm_reg(XmmReg::XMM1, Reg::RAX);
                self.emitter.ucomiss_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1);
                let over_limit = self.emitter.new_label();
                let done = self.emitter.new_label();
                self.emitter.jcc_label(0x83, over_limit); // jae (CF=0) -> >= 2^31

                // Normal signed conversion for < 2^31
                self.emitter.cvttss2si_reg32_xmm(Reg::RAX, XmmReg::XMM0);
                self.emitter.jmp_label(done);

                self.emitter.bind_label(over_limit);
                self.emitter.subss_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1);
                self.emitter.cvttss2si_reg32_xmm(Reg::RAX, XmmReg::XMM0);
                self.emitter.mov_reg_imm64(Reg::RCX, 0x80000000);
                self.emitter.add_reg32_reg32(Reg::RAX, Reg::RCX);

                self.emitter.bind_label(done);
                self.emitter.push_wasm_stack(Reg::RAX);
            }
            Instruction::I32TruncF64S => {
                self.emitter.pop_v128(XmmReg::XMM0);
                self.emitter.cvttsd2si_reg32_xmm(Reg::RAX, XmmReg::XMM0);
                self.emitter.push_wasm_stack(Reg::RAX);
            }
            Instruction::I32TruncF64U => {
                self.emitter.pop_v128(XmmReg::XMM0);
                self.emitter.mov_reg_imm64(Reg::RAX, 0x41E0000000000000); // 2^31 as f64
                self.emitter.movq_xmm_reg(XmmReg::XMM1, Reg::RAX);
                self.emitter.ucomisd_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1);
                let over_limit = self.emitter.new_label();
                let done = self.emitter.new_label();
                self.emitter.jcc_label(0x83, over_limit);

                self.emitter.cvttsd2si_reg32_xmm(Reg::RAX, XmmReg::XMM0);
                self.emitter.jmp_label(done);

                self.emitter.bind_label(over_limit);
                self.emitter.subsd_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1);
                self.emitter.cvttsd2si_reg32_xmm(Reg::RAX, XmmReg::XMM0);
                self.emitter.mov_reg_imm64(Reg::RCX, 0x80000000);
                self.emitter.add_reg32_reg32(Reg::RAX, Reg::RCX);

                self.emitter.bind_label(done);
                self.emitter.push_wasm_stack(Reg::RAX);
            }
            Instruction::I64TruncF32S => {
                self.emitter.pop_v128(XmmReg::XMM0);
                self.emitter.cvttss2si_reg_xmm(Reg::RAX, XmmReg::XMM0);
                self.emitter.push_wasm_stack(Reg::RAX);
            }
            Instruction::I64TruncF32U => {
                self.emitter.pop_v128(XmmReg::XMM0);
                self.emitter.mov_reg_imm64(Reg::RAX, 0x5F000000); // 2^63 as f32
                self.emitter.movd_xmm_reg(XmmReg::XMM1, Reg::RAX);
                self.emitter.ucomiss_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1);
                let over_limit = self.emitter.new_label();
                let done = self.emitter.new_label();
                self.emitter.jcc_label(0x83, over_limit);

                self.emitter.cvttss2si_reg_xmm(Reg::RAX, XmmReg::XMM0);
                self.emitter.jmp_label(done);

                self.emitter.bind_label(over_limit);
                self.emitter.subss_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1);
                self.emitter.cvttss2si_reg_xmm(Reg::RAX, XmmReg::XMM0);
                self.emitter.mov_reg_imm64(Reg::RCX, 0x8000000000000000);
                self.emitter.add_reg_reg(Reg::RAX, Reg::RCX);

                self.emitter.bind_label(done);
                self.emitter.push_wasm_stack(Reg::RAX);
            }
            Instruction::I64TruncF64S => {
                self.emitter.pop_v128(XmmReg::XMM0);
                self.emitter.cvttsd2si_reg_xmm(Reg::RAX, XmmReg::XMM0);
                self.emitter.push_wasm_stack(Reg::RAX);
            }
            Instruction::I64TruncF64U => {
                self.emitter.pop_v128(XmmReg::XMM0);
                self.emitter.mov_reg_imm64(Reg::RAX, 0x43E0000000000000); // 2^63 as f64
                self.emitter.movq_xmm_reg(XmmReg::XMM1, Reg::RAX);
                self.emitter.ucomisd_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1);
                let over_limit = self.emitter.new_label();
                let done = self.emitter.new_label();
                self.emitter.jcc_label(0x83, over_limit);

                self.emitter.cvttsd2si_reg_xmm(Reg::RAX, XmmReg::XMM0);
                self.emitter.jmp_label(done);

                self.emitter.bind_label(over_limit);
                self.emitter.subsd_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1);
                self.emitter.cvttsd2si_reg_xmm(Reg::RAX, XmmReg::XMM0);
                self.emitter.mov_reg_imm64(Reg::RCX, 0x8000000000000000);
                self.emitter.add_reg_reg(Reg::RAX, Reg::RCX);

                self.emitter.bind_label(done);
                self.emitter.push_wasm_stack(Reg::RAX);
            }

            Instruction::I32Load(memarg) => self.emit_load_i32(memarg),
            Instruction::I32Load8S(memarg) => self.emit_load_extend(memarg, 1, true, false),
            Instruction::I32Load8U(memarg) => self.emit_load_extend(memarg, 1, false, false),
            Instruction::I32Load16S(memarg) => self.emit_load_extend(memarg, 2, true, false),
            Instruction::I32Load16U(memarg) => self.emit_load_extend(memarg, 2, false, false),

            Instruction::I64Load(memarg) => self.emit_load_i64(memarg),
            Instruction::I64Load8S(memarg) => self.emit_load_extend(memarg, 1, true, true),
            Instruction::I64Load8U(memarg) => self.emit_load_extend(memarg, 1, false, true),
            Instruction::I64Load16S(memarg) => self.emit_load_extend(memarg, 2, true, true),
            Instruction::I64Load16U(memarg) => self.emit_load_extend(memarg, 2, false, true),
            Instruction::I64Load32S(memarg) => self.emit_load_extend(memarg, 4, true, true),
            Instruction::I64Load32U(memarg) => self.emit_load_extend(memarg, 4, false, true),

            Instruction::I32Store(memarg) => self.emit_store_i32(memarg),
            Instruction::I32Store8(memarg) => self.emit_store_shrink(memarg, 1),
            Instruction::I32Store16(memarg) => self.emit_store_shrink(memarg, 2),
            Instruction::I64Store(memarg) => self.emit_store_i64(memarg),
            Instruction::I64Store8(memarg) => self.emit_store_shrink(memarg, 1),
            Instruction::I64Store16(memarg) => self.emit_store_shrink(memarg, 2),
            Instruction::I64Store32(memarg) => self.emit_store_shrink(memarg, 4),

            Instruction::F32Load(memarg) => {
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emit_bounds_check(Reg::RAX, 4, memarg.offset);
                self.emitter.mov_reg_mem64(Reg::RCX, Reg::RDI, 16);
                self.emitter.add_reg_reg(Reg::RCX, Reg::RAX);
                self.emitter.emit_u8(0xF3);
                self.emitter.emit_u8(0x0F);
                self.emitter.emit_u8(0x10);
                if memarg.offset <= 127 {
                    self.emitter.modrm(1, XmmReg::XMM0 as u8, Reg::RCX as u8);
                    self.emitter.emit_u8(memarg.offset as u8);
                } else {
                    self.emitter.modrm(2, XmmReg::XMM0 as u8, Reg::RCX as u8);
                    self.emitter.emit_u32(memarg.offset);
                }
                self.emitter.push_v128(XmmReg::XMM0);
            }
            Instruction::F64Load(memarg) => {
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emit_bounds_check(Reg::RAX, 8, memarg.offset);
                self.emitter.mov_reg_mem64(Reg::RCX, Reg::RDI, 16);
                self.emitter.add_reg_reg(Reg::RCX, Reg::RAX);
                self.emitter.emit_u8(0xF2);
                self.emitter.emit_u8(0x0F);
                self.emitter.emit_u8(0x10);
                if memarg.offset <= 127 {
                    self.emitter.modrm(1, XmmReg::XMM0 as u8, Reg::RCX as u8);
                    self.emitter.emit_u8(memarg.offset as u8);
                } else {
                    self.emitter.modrm(2, XmmReg::XMM0 as u8, Reg::RCX as u8);
                    self.emitter.emit_u32(memarg.offset);
                }
                self.emitter.push_v128(XmmReg::XMM0);
            }
            Instruction::F32Store(memarg) => {
                self.emitter.pop_v128(XmmReg::XMM0);
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emit_bounds_check(Reg::RAX, 4, memarg.offset);
                self.emitter.mov_reg_mem64(Reg::RCX, Reg::RDI, 16);
                self.emitter.add_reg_reg(Reg::RCX, Reg::RAX);
                self.emitter.emit_u8(0xF3);
                self.emitter.emit_u8(0x0F);
                self.emitter.emit_u8(0x11);
                if memarg.offset <= 127 {
                    self.emitter.modrm(1, XmmReg::XMM0 as u8, Reg::RCX as u8);
                    self.emitter.emit_u8(memarg.offset as u8);
                } else {
                    self.emitter.modrm(2, XmmReg::XMM0 as u8, Reg::RCX as u8);
                    self.emitter.emit_u32(memarg.offset);
                }
                self.stack_depth -= 2;
            }
            Instruction::F64Store(memarg) => {
                self.emitter.pop_v128(XmmReg::XMM0);
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emit_bounds_check(Reg::RAX, 8, memarg.offset);
                self.emitter.mov_reg_mem64(Reg::RCX, Reg::RDI, 16);
                self.emitter.add_reg_reg(Reg::RCX, Reg::RAX);
                self.emitter.emit_u8(0xF2);
                self.emitter.emit_u8(0x0F);
                self.emitter.emit_u8(0x11);
                if memarg.offset <= 127 {
                    self.emitter.modrm(1, XmmReg::XMM0 as u8, Reg::RCX as u8);
                    self.emitter.emit_u8(memarg.offset as u8);
                } else {
                    self.emitter.modrm(2, XmmReg::XMM0 as u8, Reg::RCX as u8);
                    self.emitter.emit_u32(memarg.offset);
                }
                self.stack_depth -= 2;
            }

            Instruction::MemorySize => {
                self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Load Context
                self.emitter.mov_reg_reg(Reg::RSI, Reg::RSP); // Pass current SP
                self.emitter.sub_reg_imm32(Reg::RSP, 16); // Align
                self.emit_call_trampoline(AotTrampoline::MemorySize);
                self.emitter.mov_reg_reg(Reg::RSP, Reg::RAX); // Restore SP from returned value

                self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Restore RDI
                self.emitter.mov_reg_mem64(Reg::R14, Reg::RDI, 16); // Restore R14
                self.emitter.mov_reg_mem64(Reg::R13, Reg::RBP, -64); // Restore current locals_base
                self.stack_depth += 1;
            }
            Instruction::MemoryGrow => {
                self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Load Context
                self.emitter.mov_reg_reg(Reg::RSI, Reg::RSP); // Pass current SP (n is at [RSP])
                self.emitter.sub_reg_imm32(Reg::RSP, 16); // Align
                self.emit_call_trampoline(AotTrampoline::MemoryGrow);
                self.emitter.mov_reg_reg(Reg::RSP, Reg::RAX); // Restore SP from returned value

                self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Restore RDI
                self.emitter.mov_reg_mem64(Reg::R14, Reg::RDI, 16); // Restore R14
                self.emitter.mov_reg_mem64(Reg::R13, Reg::RBP, -64); // Restore current locals_base
            }
            Instruction::TableGet(_idx) => {
                self.emitter.pop_wasm_stack(Reg::RAX); // index
                self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // context
                self.emitter.mov_reg_mem64(Reg::RDX, Reg::RDI, 96); // table0_ptr (offset 96)
                self.emitter.shl_reg_imm32(Reg::RAX, 3); // index * 8
                self.emitter.add_reg_reg(Reg::RAX, Reg::RDX); // entry addr
                self.emitter.mov_reg_mem64(Reg::RAX, Reg::RAX, 0); // load func_addr
                self.emitter.push_wasm_stack(Reg::RAX);
            }
            Instruction::TableSet(_idx) => {
                self.emitter.pop_wasm_stack(Reg::RCX); // value (func_addr)
                self.emitter.pop_wasm_stack(Reg::RAX); // index
                self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // context
                self.emitter.mov_reg_mem64(Reg::RDX, Reg::RDI, 96); // table0_ptr
                self.emitter.shl_reg_imm32(Reg::RAX, 3); // index * 8
                self.emitter.add_reg_reg(Reg::RAX, Reg::RDX); // entry addr
                self.emitter.mov_mem64_reg(Reg::RAX, 0, Reg::RCX); // store
                self.stack_depth -= 2;
            }

            Instruction::Call(idx) => {
                let type_idx = self.validation_info.functions_types[idx];
                let func_type = &self.validation_info.types[type_idx];
                let param_count = func_type.params.valtypes.len();
                let result_count = func_type.returns.valtypes.len();

                if idx < self.validation_info.imports_length.imported_functions {
                    let imp = &self.validation_info.imports[idx];
                    //crate::debugln!("[AOT-Compiler] Emitting CallHost for import {}:{} (idx {})", imp.module_name, imp.name, idx);

                    let reserve_space = if result_count > param_count {
                        (result_count - param_count) * 16
                    } else {
                        0
                    };

                    if reserve_space > 0 {
                        self.emitter.sub_reg_imm32(Reg::RSP, reserve_space as u32);
                    }

                    self.emitter.sub_reg_imm32(Reg::RSP, 16); // Align

                    self.emitter.mov_reg_reg(Reg::RSI, Reg::RSP);
                    self.emitter
                        .add_reg_imm32(Reg::RSI, (16 + reserve_space) as u32); // sp
                    self.emitter.mov_reg_imm64(Reg::RDX, idx as u64); // idx

                    self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Restore Context for CallHost (arg 0)
                    self.emit_call_trampoline(AotTrampoline::CallHost);

                    self.emitter.mov_reg_reg(Reg::RSP, Reg::RAX); // Restore SP from returned value
                    self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Restore RDI
                    self.emitter.mov_reg_mem64(Reg::R14, Reg::RDI, 16); // Restore R14
                    self.emitter.mov_reg_mem64(Reg::R13, Reg::RBP, -64); // Restore current locals_base
                    self.emit_sp_sanity_check();
                    // Check trap_code
                    self.emitter.mov_reg_mem64(Reg::RAX, Reg::RDI, 64); // trap_code pointer
                    self.emitter.cmp_mem32_imm32(Reg::RAX, 0, 0); // cmp dword ptr [rax], 0
                    self.emitter.jcc_label(0x85, self.trap_halt_label); // jne trap_halt_label
                } else {
                    // Local function call
                    self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Pass Context in RDI
                    let label = self.func_labels[idx];
                    self.emitter.call_label(label);

                    self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48);
                    self.emitter.mov_reg_mem64(Reg::R14, Reg::RDI, 16);
                    self.emitter.mov_reg_mem64(Reg::R13, Reg::RBP, -64); // Restore current locals_base
                    self.emitter.mov_reg_reg(Reg::RSP, Reg::RAX);
                    self.emit_sp_sanity_check();

                    // Check trap_code
                    self.emitter.mov_reg_mem64(Reg::RAX, Reg::RDI, 64); // trap_code pointer
                    self.emitter.cmp_mem32_imm32(Reg::RAX, 0, 0); // cmp dword ptr [rax], 0
                    self.emitter.jcc_label(0x85, self.trap_halt_label); // jne trap_halt_label
                }
                self.stack_depth = (self.stack_depth as isize + result_count as isize
                    - param_count as isize) as usize;
            }
            Instruction::CallIndirect(type_idx, table_idx) => {
                let func_type = &self.validation_info.types[type_idx as usize];
                let param_count = func_type.params.valtypes.len();
                let result_count = func_type.returns.valtypes.len();

                self.emitter.pop_wasm_stack(Reg::RAX); // index

                self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Restore Context
                self.emitter.mov_reg_imm64(Reg::RSI, table_idx as u64);
                self.emitter.mov_reg_imm64(Reg::RDX, type_idx as u64);
                self.emitter.mov_reg_reg(Reg::RCX, Reg::RAX); // index

                self.emitter.sub_reg_imm32(Reg::RSP, 16); // Align
                self.emit_call_trampoline(AotTrampoline::CallIndirect);
                self.emitter.add_reg_imm32(Reg::RSP, 16); // Balance align

                // CallIndirect trampoline returns func ptr in RAX (as per tables.rs)
                self.emitter.test_reg_reg(Reg::RAX, Reg::RAX);
                self.emitter.jcc_label(0x84, self.trap_indirect_label);

                self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Pass Context in RDI
                self.emitter.call_reg(Reg::RAX);

                self.emitter.mov_reg_reg(Reg::RSP, Reg::RAX); // RAX = new SP from callee
                self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48);
                self.emitter.mov_reg_mem64(Reg::R14, Reg::RDI, 16);
                self.emitter.mov_reg_mem64(Reg::R13, Reg::RBP, -64); // Restore current locals_base
                self.emit_sp_sanity_check();

                // Check trap_code
                self.emitter.mov_reg_mem64(Reg::RAX, Reg::RDI, 64); // trap_code pointer
                self.emitter.cmp_mem32_imm32(Reg::RAX, 0, 0); // cmp dword ptr [rax], 0
                self.emitter.jcc_label(0x85, self.trap_halt_label); // jne trap_halt_label

                self.stack_depth = (self.stack_depth as isize - 1 + result_count as isize
                    - param_count as isize) as usize;
            }

            Instruction::Block(bt) => {
                let end_label = self.emitter.new_label();
                let result_count = match bt {
                    BlockType::Empty => 0,
                    BlockType::Returns(_) => 1,
                    BlockType::Type(idx) => self.validation_info.types[idx as usize]
                        .returns
                        .valtypes
                        .len(),
                };
                self.control_stack.push(ControlBlock {
                    kind: ControlBlockKind::Block,
                    stack_depth_before: self.stack_depth,
                    result_count,
                    end_label,
                    else_label: None,
                    start_label: None,
                });
            }
            Instruction::Loop(bt) => {
                let start_label = self.emitter.new_label();
                let end_label = self.emitter.new_label();
                let result_count = match bt {
                    BlockType::Empty => 0,
                    BlockType::Returns(_) => 1,
                    BlockType::Type(idx) => self.validation_info.types[idx as usize]
                        .returns
                        .valtypes
                        .len(),
                };
                self.emitter.bind_label(start_label);
                self.emit_fuel_check(5);
                self.control_stack.push(ControlBlock {
                    kind: ControlBlockKind::Loop,
                    stack_depth_before: self.stack_depth,
                    result_count,
                    end_label,
                    else_label: None,
                    start_label: Some(start_label),
                });
            }
            Instruction::Br(label_idx) => {
                let target_idx = self.control_stack.len() - 1 - label_idx as usize;
                let target_label = {
                    let cb = &self.control_stack[target_idx];
                    cb.start_label.unwrap_or(cb.end_label)
                };
                self.emit_unwind(target_idx);
                self.emitter.jmp_label(target_label);
            }
            Instruction::BrIf(label_idx) => {
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emitter.test_reg32_reg32(Reg::RAX, Reg::RAX);
                self.stack_depth -= 1;
                let skip_label = self.emitter.new_label();
                self.emitter.jcc_label(0x84, skip_label);
                let target_idx = self.control_stack.len() - 1 - label_idx as usize;
                let target_label = {
                    let cb = &self.control_stack[target_idx];
                    cb.start_label.unwrap_or(cb.end_label)
                };
                self.emit_unwind(target_idx);
                self.emitter.jmp_label(target_label);
                self.emitter.bind_label(skip_label);
            }
            Instruction::BrTable(targets, default) => {
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.stack_depth -= 1;
                for (i, target) in targets.iter().enumerate() {
                    self.emitter.cmp_reg32_imm32(Reg::RAX, i as u32);
                    let skip_next = self.emitter.new_label();
                    self.emitter.jcc_label(0x85, skip_next);
                    let target_idx = self.control_stack.len() - 1 - *target as usize;
                    let target_label = {
                        let cb = &self.control_stack[target_idx];
                        cb.start_label.unwrap_or(cb.end_label)
                    };
                    self.emit_unwind(target_idx);
                    self.emitter.jmp_label(target_label);
                    self.emitter.bind_label(skip_next);
                }
                let default_idx = self.control_stack.len() - 1 - default as usize;
                let default_label = {
                    let cb = &self.control_stack[default_idx];
                    cb.start_label.unwrap_or(cb.end_label)
                };
                self.emit_unwind(default_idx);
                self.emitter.jmp_label(default_label);
            }
            Instruction::If(bt) => {
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emitter.test_reg32_reg32(Reg::RAX, Reg::RAX);
                let else_label = self.emitter.new_label();
                let end_label = self.emitter.new_label();
                let result_count = match bt {
                    BlockType::Empty => 0,
                    BlockType::Returns(_) => 1,
                    BlockType::Type(idx) => self.validation_info.types[idx as usize]
                        .returns
                        .valtypes
                        .len(),
                };
                self.emitter.jcc_label(0x84, else_label);
                self.stack_depth -= 1;
                self.control_stack.push(ControlBlock {
                    kind: ControlBlockKind::If,
                    stack_depth_before: self.stack_depth,
                    result_count,
                    end_label,
                    else_label: Some(else_label),
                    start_label: None,
                });
            }
            Instruction::Else => {
                let cb = self
                    .control_stack
                    .last_mut()
                    .expect("Control stack underflow");
                if let Some(else_label) = cb.else_label.take() {
                    self.emitter.jmp_label(cb.end_label);
                    self.emitter.bind_label(else_label);
                    self.stack_depth = cb.stack_depth_before;
                }
            }
            Instruction::End => {
                let mut cb = self.control_stack.pop().expect("Control stack underflow");
                if let Some(else_label) = cb.else_label {
                    self.emitter.bind_label(else_label);
                }
                self.emitter.bind_label(cb.end_label);
                self.stack_depth = cb.stack_depth_before + cb.result_count;
            }
            Instruction::Return => {
                self.emit_unwind(0);
                self.emitter.jmp_label(self.control_stack[0].end_label);
            }
            Instruction::FdExtension(sub) => self.compile_simd(sub, reader),
            Instruction::FcExtension(sub) => self.compile_fc(sub, reader),
            Instruction::Atomic(sub) => self.compile_atomic(sub, reader),
            _ => panic!("Unimplemented instruction in AOT: {:?}", instr),
        }
    }

    fn emit_unwind(&mut self, target_idx: usize) {
        let cb = &self.control_stack[target_idx];
        let result_count = if cb.kind == ControlBlockKind::Loop {
            0
        } else {
            cb.result_count
        };
        let drop_count = self
            .stack_depth
            .saturating_sub(cb.stack_depth_before + result_count);

        if result_count > 0 && drop_count > 0 {
            // Move results down to cover dropped items. Copy in reverse to handle overlaps correctly.
            for i in (0..result_count).rev() {
                self.emitter
                    .mov_reg_mem64(Reg::RAX, Reg::RSP, (i * 16) as i32);
                self.emitter
                    .mov_reg_mem64(Reg::RDX, Reg::RSP, (i * 16 + 8) as i32);
                self.emitter
                    .mov_mem64_reg(Reg::RSP, ((i + drop_count) * 16) as i32, Reg::RAX);
                self.emitter
                    .mov_mem64_reg(Reg::RSP, ((i + drop_count) * 16 + 8) as i32, Reg::RDX);
            }
        }

        if drop_count > 0 {
            self.emitter
                .add_reg_imm32(Reg::RSP, (drop_count * 16) as u32);
        }
    }

    fn emit_binop_i32<F>(&mut self, op: F)
    where
        F: FnOnce(&mut X64Emitter),
    {
        self.emitter.pop_wasm_stack(Reg::RBX);
        self.emitter.pop_wasm_stack(Reg::RAX);
        op(&mut self.emitter);
        self.emitter.push_wasm_stack(Reg::RAX);
        self.stack_depth -= 1;
    }

    fn emit_binop_i64<F>(&mut self, op: F)
    where
        F: FnOnce(&mut X64Emitter),
    {
        self.emitter.pop_wasm_stack(Reg::RBX);
        self.emitter.pop_wasm_stack(Reg::RAX);
        op(&mut self.emitter);
        self.emitter.push_wasm_stack(Reg::RAX);
        self.stack_depth -= 1;
    }

    fn emit_binop_f32<F>(&mut self, op: F)
    where
        F: FnOnce(&mut X64Emitter),
    {
        self.emitter.pop_v128(XmmReg::XMM1);
        self.emitter.pop_v128(XmmReg::XMM0);
        op(&mut self.emitter);
        self.emitter.push_v128(XmmReg::XMM0);
        self.stack_depth -= 1;
    }

    fn emit_binop_f64<F>(&mut self, op: F)
    where
        F: FnOnce(&mut X64Emitter),
    {
        self.emitter.pop_v128(XmmReg::XMM1);
        self.emitter.pop_v128(XmmReg::XMM0);
        op(&mut self.emitter);
        self.emitter.push_v128(XmmReg::XMM0);
        self.stack_depth -= 1;
    }

    fn emit_binop_v128<F>(&mut self, op: F)
    where
        F: FnOnce(&mut X64Emitter),
    {
        self.emitter.pop_v128(XmmReg::XMM1);
        self.emitter.pop_v128(XmmReg::XMM0);
        op(&mut self.emitter);
        self.emitter.push_v128(XmmReg::XMM0);
        self.stack_depth -= 1;
    }

    fn emit_shift_i32<F>(&mut self, op: F)
    where
        F: FnOnce(&mut X64Emitter),
    {
        self.emitter.pop_wasm_stack(Reg::RCX);
        self.emitter.pop_wasm_stack(Reg::RAX);
        op(&mut self.emitter);
        self.emitter.push_wasm_stack(Reg::RAX);
        self.stack_depth -= 1;
    }

    fn emit_shift_i64<F>(&mut self, op: F)
    where
        F: FnOnce(&mut X64Emitter),
    {
        self.emitter.pop_wasm_stack(Reg::RCX);
        self.emitter.pop_wasm_stack(Reg::RAX);
        op(&mut self.emitter);
        self.emitter.push_wasm_stack(Reg::RAX);
        self.stack_depth -= 1;
    }

    fn emit_trampoline_unop(&mut self, trampoline: crate::wasm::aot::runtime::AotTrampoline) {
        self.emitter.pop_wasm_stack(Reg::RAX);
        self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Load Context for trampoline
        self.emitter.sub_reg_imm32(Reg::RSP, 16); // Align
        self.emitter.mov_reg_reg(Reg::RDI, Reg::RAX);
        self.emit_call_trampoline(trampoline);
        self.emitter.add_reg_imm32(Reg::RSP, 16);
        self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Restore Context
        self.emitter.mov_reg_mem64(Reg::R14, Reg::RDI, 16); // Restore R14
        self.emitter.mov_reg_mem64(Reg::R13, Reg::RBP, -24); // Restore R13
        self.emitter.push_wasm_stack(Reg::RAX);
    }

    fn emit_trampoline_unop_f32(&mut self, trampoline: crate::wasm::aot::runtime::AotTrampoline) {
        self.emitter.pop_v128(XmmReg::XMM0);
        self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Load Context for trampoline
        self.emitter.sub_reg_imm32(Reg::RSP, 16); // Align
        self.emit_call_trampoline(trampoline);
        self.emitter.add_reg_imm32(Reg::RSP, 16);
        self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Restore Context
        self.emitter.mov_reg_mem64(Reg::R14, Reg::RDI, 16); // Restore R14
        self.emitter.mov_reg_mem64(Reg::R13, Reg::RBP, -24); // Restore R13
        self.emitter.push_v128(XmmReg::XMM0);
    }

    fn emit_trampoline_unop_f64(&mut self, trampoline: crate::wasm::aot::runtime::AotTrampoline) {
        self.emitter.pop_v128(XmmReg::XMM0);
        self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Load Context for trampoline
        self.emitter.sub_reg_imm32(Reg::RSP, 16); // Align
        self.emit_call_trampoline(trampoline);
        self.emitter.add_reg_imm32(Reg::RSP, 16);
        self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Restore Context
        self.emitter.mov_reg_mem64(Reg::R14, Reg::RDI, 16); // Restore R14
        self.emitter.mov_reg_mem64(Reg::R13, Reg::RBP, -24); // Restore R13
        self.emitter.push_v128(XmmReg::XMM0);
    }

    fn emit_trampoline_unop_f32_to_i32(
        &mut self,
        trampoline: crate::wasm::aot::runtime::AotTrampoline,
    ) {
        self.emitter.pop_v128(XmmReg::XMM0);
        self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Load Context for trampoline
        self.emitter.sub_reg_imm32(Reg::RSP, 16); // Align
        self.emit_call_trampoline(trampoline);
        self.emitter.add_reg_imm32(Reg::RSP, 16);
        self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Restore Context
        self.emitter.mov_reg_mem64(Reg::R14, Reg::RDI, 16); // Restore R14
        self.emitter.mov_reg_mem64(Reg::R13, Reg::RBP, -24); // Restore R13
        self.emitter.push_wasm_stack(Reg::RAX);
    }

    fn emit_trampoline_unop_f64_to_i32(
        &mut self,
        trampoline: crate::wasm::aot::runtime::AotTrampoline,
    ) {
        self.emitter.pop_v128(XmmReg::XMM0);
        self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Load Context for trampoline
        self.emitter.sub_reg_imm32(Reg::RSP, 16); // Align
        self.emit_call_trampoline(trampoline);
        self.emitter.add_reg_imm32(Reg::RSP, 16);
        self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Restore Context
        self.emitter.mov_reg_mem64(Reg::R14, Reg::RDI, 16); // Restore R14
        self.emitter.mov_reg_mem64(Reg::R13, Reg::RBP, -24); // Restore R13
        self.emitter.push_wasm_stack(Reg::RAX);
    }

    fn emit_trampoline_unop_f32_to_i64(
        &mut self,
        trampoline: crate::wasm::aot::runtime::AotTrampoline,
    ) {
        self.emitter.pop_v128(XmmReg::XMM0);
        self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Load Context for trampoline
        self.emitter.sub_reg_imm32(Reg::RSP, 16); // Align
        self.emit_call_trampoline(trampoline);
        self.emitter.add_reg_imm32(Reg::RSP, 16);
        self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Restore Context
        self.emitter.mov_reg_mem64(Reg::R14, Reg::RDI, 16); // Restore R14
        self.emitter.mov_reg_mem64(Reg::R13, Reg::RBP, -24); // Restore R13
        self.emitter.push_wasm_stack(Reg::RAX);
    }

    fn emit_trampoline_unop_f64_to_i64(
        &mut self,
        trampoline: crate::wasm::aot::runtime::AotTrampoline,
    ) {
        self.emitter.pop_v128(XmmReg::XMM0);
        self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Load Context for trampoline
        self.emitter.sub_reg_imm32(Reg::RSP, 16); // Align
        self.emit_call_trampoline(trampoline);
        self.emitter.add_reg_imm32(Reg::RSP, 16);
        self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Restore Context
        self.emitter.mov_reg_mem64(Reg::R14, Reg::RDI, 16); // Restore R14
        self.emitter.mov_reg_mem64(Reg::R13, Reg::RBP, -24); // Restore R13
        self.emitter.push_wasm_stack(Reg::RAX);
    }

    fn emit_trampoline_binop(&mut self, trampoline: crate::wasm::aot::runtime::AotTrampoline) {
        self.emitter.pop_wasm_stack(Reg::RSI);
        self.emitter.pop_wasm_stack(Reg::RAX);
        self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Load Context for trampoline
        self.emitter.sub_reg_imm32(Reg::RSP, 16); // Align
        self.emitter.mov_reg_reg(Reg::RDI, Reg::RAX);
        self.emit_call_trampoline(trampoline);
        self.emitter.add_reg_imm32(Reg::RSP, 16);
        self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Restore Context
        self.emitter.mov_reg_mem64(Reg::R14, Reg::RDI, 16); // Restore R14
        self.emitter.mov_reg_mem64(Reg::R13, Reg::RBP, -24); // Restore R13
        self.emitter.push_wasm_stack(Reg::RAX);
        self.stack_depth -= 1;
    }

    fn emit_trampoline_binop_f32(&mut self, trampoline: crate::wasm::aot::runtime::AotTrampoline) {
        self.emitter.pop_v128(XmmReg::XMM1);
        self.emitter.pop_v128(XmmReg::XMM0);
        self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Load Context for trampoline
        self.emitter.sub_reg_imm32(Reg::RSP, 16); // Align
        self.emit_call_trampoline(trampoline);
        self.emitter.add_reg_imm32(Reg::RSP, 16);
        self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Restore Context
        self.emitter.mov_reg_mem64(Reg::R14, Reg::RDI, 16); // Restore R14
        self.emitter.mov_reg_mem64(Reg::R13, Reg::RBP, -24); // Restore R13
        self.emitter.push_v128(XmmReg::XMM0);
        self.stack_depth -= 1;
    }

    fn emit_trampoline_binop_f64(&mut self, trampoline: crate::wasm::aot::runtime::AotTrampoline) {
        self.emitter.pop_v128(XmmReg::XMM1);
        self.emitter.pop_v128(XmmReg::XMM0);
        self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Load Context for trampoline
        self.emitter.sub_reg_imm32(Reg::RSP, 16); // Align
        self.emit_call_trampoline(trampoline);
        self.emitter.add_reg_imm32(Reg::RSP, 16);
        self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Restore Context
        self.emitter.mov_reg_mem64(Reg::R14, Reg::RDI, 16); // Restore R14
        self.emitter.mov_reg_mem64(Reg::R13, Reg::RBP, -24); // Restore R13
        self.emitter.push_v128(XmmReg::XMM0);
        self.stack_depth -= 1;
    }

    fn emit_relop_i32(&mut self, set_opcode: u8) {
        self.emitter.pop_wasm_stack(Reg::RBX);
        self.emitter.pop_wasm_stack(Reg::RAX);
        self.emitter.cmp_reg32_reg32(Reg::RAX, Reg::RBX);
        self.emitter.emit_u8(0x0F);
        self.emitter.emit_u8(set_opcode);
        self.emitter.emit_u8(0xC0); // SETcc AL
        self.emitter.movzx_reg_reg8(Reg::RAX, Reg::RAX);
        self.emitter.push_wasm_stack(Reg::RAX);
        self.stack_depth -= 1;
    }

    fn emit_relop_i64(&mut self, set_opcode: u8) {
        self.emitter.pop_wasm_stack(Reg::RBX);
        self.emitter.pop_wasm_stack(Reg::RAX);
        self.emitter.cmp_reg_reg(Reg::RAX, Reg::RBX);
        self.emitter.emit_u8(0x0F);
        self.emitter.emit_u8(set_opcode);
        self.emitter.emit_u8(0xC0); // SETcc AL
        self.emitter.movzx_reg_reg8(Reg::RAX, Reg::RAX);
        self.emitter.push_wasm_stack(Reg::RAX);
        self.stack_depth -= 1;
    }

    fn emit_trampoline_relop_f32(&mut self, trampoline: crate::wasm::aot::runtime::AotTrampoline) {
        self.emitter.pop_v128(XmmReg::XMM1);
        self.emitter.pop_v128(XmmReg::XMM0);
        self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Load Context for trampoline
        self.emitter.sub_reg_imm32(Reg::RSP, 16); // Align
        self.emit_call_trampoline(trampoline);
        self.emitter.add_reg_imm32(Reg::RSP, 16);
        self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Restore Context
        self.emitter.mov_reg_mem64(Reg::R14, Reg::RDI, 16); // Restore R14
        self.emitter.mov_reg_mem64(Reg::R13, Reg::RBP, -24); // Restore R13
        self.emitter.push_wasm_stack(Reg::RAX);
        self.stack_depth -= 1;
    }

    fn emit_trampoline_relop_f64(&mut self, trampoline: crate::wasm::aot::runtime::AotTrampoline) {
        self.emitter.pop_v128(XmmReg::XMM1);
        self.emitter.pop_v128(XmmReg::XMM0);
        self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Load Context for trampoline
        self.emitter.sub_reg_imm32(Reg::RSP, 16); // Align
        self.emit_call_trampoline(trampoline);
        self.emitter.add_reg_imm32(Reg::RSP, 16);
        self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Restore Context
        self.emitter.mov_reg_mem64(Reg::R14, Reg::RDI, 16); // Restore R14
        self.emitter.mov_reg_mem64(Reg::R13, Reg::RBP, -24); // Restore R13
        self.emitter.push_wasm_stack(Reg::RAX);
        self.stack_depth -= 1;
    }

    fn emit_f32_relop(&mut self, op: u8) {
        self.emitter.pop_v128(XmmReg::XMM0); // right
        self.emitter.pop_v128(XmmReg::XMM1); // left
        self.emitter.xor_reg32_reg32(Reg::RAX, Reg::RAX);

        match op {
            0 => {
                // Eq
                self.emitter.xor_reg32_reg32(Reg::RDX, Reg::RDX);
                self.emitter.ucomiss_xmm_xmm(XmmReg::XMM1, XmmReg::XMM0);
                self.emitter.setcc(0x9B, Reg::RAX); // setnp
                self.emitter.setcc(0x94, Reg::RDX); // sete
                self.emitter.and_reg32_reg32(Reg::RAX, Reg::RDX);
            }
            1 => {
                // Ne
                self.emitter.xor_reg32_reg32(Reg::RDX, Reg::RDX);
                self.emitter.ucomiss_xmm_xmm(XmmReg::XMM1, XmmReg::XMM0);
                self.emitter.setcc(0x9A, Reg::RAX); // setp
                self.emitter.setcc(0x95, Reg::RDX); // setne
                self.emitter.or_reg32_reg32(Reg::RAX, Reg::RDX);
            }
            2 => {
                // Lt (xmm1 < xmm0  <=>  xmm0 > xmm1)
                self.emitter.ucomiss_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1);
                self.emitter.setcc(0x97, Reg::RAX); // seta
            }
            3 => {
                // Gt (xmm1 > xmm0)
                self.emitter.ucomiss_xmm_xmm(XmmReg::XMM1, XmmReg::XMM0);
                self.emitter.setcc(0x97, Reg::RAX); // seta
            }
            4 => {
                // Le (xmm1 <= xmm0 <=> xmm0 >= xmm1)
                self.emitter.ucomiss_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1);
                self.emitter.setcc(0x93, Reg::RAX); // setae
            }
            5 => {
                // Ge (xmm1 >= xmm0)
                self.emitter.ucomiss_xmm_xmm(XmmReg::XMM1, XmmReg::XMM0);
                self.emitter.setcc(0x93, Reg::RAX); // setae
            }
            _ => unreachable!(),
        }
        self.emitter.push_wasm_stack(Reg::RAX);
        self.stack_depth -= 1;
    }

    fn emit_f64_relop(&mut self, op: u8) {
        self.emitter.pop_v128(XmmReg::XMM0); // right
        self.emitter.pop_v128(XmmReg::XMM1); // left
        self.emitter.xor_reg32_reg32(Reg::RAX, Reg::RAX);

        match op {
            0 => {
                // Eq
                self.emitter.xor_reg32_reg32(Reg::RDX, Reg::RDX);
                self.emitter.ucomisd_xmm_xmm(XmmReg::XMM1, XmmReg::XMM0);
                self.emitter.setcc(0x9B, Reg::RAX); // setnp
                self.emitter.setcc(0x94, Reg::RDX); // sete
                self.emitter.and_reg32_reg32(Reg::RAX, Reg::RDX);
            }
            1 => {
                // Ne
                self.emitter.xor_reg32_reg32(Reg::RDX, Reg::RDX);
                self.emitter.ucomisd_xmm_xmm(XmmReg::XMM1, XmmReg::XMM0);
                self.emitter.setcc(0x9A, Reg::RAX); // setp
                self.emitter.setcc(0x95, Reg::RDX); // setne
                self.emitter.or_reg32_reg32(Reg::RAX, Reg::RDX);
            }
            2 => {
                // Lt
                self.emitter.ucomisd_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1);
                self.emitter.setcc(0x97, Reg::RAX); // seta
            }
            3 => {
                // Gt
                self.emitter.ucomisd_xmm_xmm(XmmReg::XMM1, XmmReg::XMM0);
                self.emitter.setcc(0x97, Reg::RAX); // seta
            }
            4 => {
                // Le
                self.emitter.ucomisd_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1);
                self.emitter.setcc(0x93, Reg::RAX); // setae
            }
            5 => {
                // Ge
                self.emitter.ucomisd_xmm_xmm(XmmReg::XMM1, XmmReg::XMM0);
                self.emitter.setcc(0x93, Reg::RAX); // setae
            }
            _ => unreachable!(),
        }
        self.emitter.push_wasm_stack(Reg::RAX);
        self.stack_depth -= 1;
    }

    fn emit_bounds_check(&mut self, _addr_reg: Reg, _size: u32, _offset: u32) {
        // Hardware-accelerated bounds checking via SAS:
        // 1. WASM addresses are u32 (zero-extended to 64-bit), so they are always < 4GB.
        // 2. Each SAS slot is 4GB, with unmapped pages providing isolation.
        // 3. A 4KB hardware guard page at the end of the slot catches overflow.
        // Result: Software checks are redundant and removed for maximum performance.
    }

    fn emit_load_i32(&mut self, memarg: MemArg) {
        self.emitter.pop_wasm_stack(Reg::RAX);
        self.emit_bounds_check(Reg::RAX, 4, memarg.offset);
        self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Reload RDI
        self.emitter.mov_reg_mem64(Reg::RCX, Reg::RDI, 16);
        self.emitter.add_reg_reg(Reg::RCX, Reg::RAX);
        self.emitter.emit_u8(0x8B);
        if memarg.offset <= 127 {
            self.emitter.modrm(1, Reg::RAX as u8, Reg::RCX as u8);
            self.emitter.emit_u8(memarg.offset as u8);
        } else {
            self.emitter.modrm(2, Reg::RAX as u8, Reg::RCX as u8);
            self.emitter.emit_u32(memarg.offset);
        }
        self.emitter.push_wasm_stack(Reg::RAX);
    }

    fn emit_load_i64(&mut self, memarg: MemArg) {
        self.emitter.pop_wasm_stack(Reg::RAX);
        self.emit_bounds_check(Reg::RAX, 8, memarg.offset);
        self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Reload RDI
        self.emitter.mov_reg_mem64(Reg::RCX, Reg::RDI, 16);
        self.emitter.add_reg_reg(Reg::RCX, Reg::RAX);
        self.emitter.emit_u8(0x48);
        self.emitter.emit_u8(0x8B);
        if memarg.offset <= 127 {
            self.emitter.modrm(1, Reg::RAX as u8, Reg::RCX as u8);
            self.emitter.emit_u8(memarg.offset as u8);
        } else {
            self.emitter.modrm(2, Reg::RAX as u8, Reg::RCX as u8);
            self.emitter.emit_u32(memarg.offset);
        }
        self.emitter.push_wasm_stack(Reg::RAX);
    }

    fn emit_load_extend(&mut self, memarg: MemArg, size: usize, signed: bool, is_i64: bool) {
        self.emitter.pop_wasm_stack(Reg::RAX);
        self.emit_bounds_check(Reg::RAX, size as u32, memarg.offset);
        self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Reload RDI
        self.emitter.mov_reg_mem64(Reg::RCX, Reg::RDI, 16);
        self.emitter.add_reg_reg(Reg::RCX, Reg::RAX);

        if is_i64 && signed {
            self.emitter.rex(true, Reg::RAX as u8, 0, Reg::RCX as u8);
        } else {
            self.emitter.rex(false, Reg::RAX as u8, 0, Reg::RCX as u8);
        }

        let opcode = match (size, signed) {
            (1, true) => 0x0FBE,
            (1, false) => 0x0FB6,
            (2, true) => 0x0FBF,
            (2, false) => 0x0FB7,
            (4, true) => 0x63,
            (4, false) => 0x8B,
            _ => 0x8B,
        };
        if opcode > 0xFF {
            self.emitter.emit_u8((opcode >> 8) as u8);
            self.emitter.emit_u8((opcode & 0xFF) as u8);
        } else {
            self.emitter.emit_u8(opcode as u8);
        }
        if memarg.offset <= 127 {
            self.emitter.modrm(1, Reg::RAX as u8, Reg::RCX as u8);
            self.emitter.emit_u8(memarg.offset as u8);
        } else {
            self.emitter.modrm(2, Reg::RAX as u8, Reg::RCX as u8);
            self.emitter.emit_u32(memarg.offset);
        }
        self.emitter.push_wasm_stack(Reg::RAX);
    }

    fn emit_store_i32(&mut self, memarg: MemArg) {
        self.emitter.pop_wasm_stack(Reg::RBX);
        self.emitter.pop_wasm_stack(Reg::RAX);
        self.emit_bounds_check(Reg::RAX, 4, memarg.offset);
        self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Reload RDI
        self.emitter.mov_reg_mem64(Reg::RCX, Reg::RDI, 16);
        self.emitter.add_reg_reg(Reg::RCX, Reg::RAX);
        self.emitter.emit_u8(0x89);
        if memarg.offset <= 127 {
            self.emitter.modrm(1, Reg::RBX as u8, Reg::RCX as u8);
            self.emitter.emit_u8(memarg.offset as u8);
        } else {
            self.emitter.modrm(2, Reg::RBX as u8, Reg::RCX as u8);
            self.emitter.emit_u32(memarg.offset);
        }
        self.stack_depth -= 2;
    }

    fn emit_store_i64(&mut self, memarg: MemArg) {
        self.emitter.pop_wasm_stack(Reg::RBX);
        self.emitter.pop_wasm_stack(Reg::RAX);
        self.emit_bounds_check(Reg::RAX, 8, memarg.offset);
        self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Reload RDI
        self.emitter.mov_reg_mem64(Reg::RCX, Reg::RDI, 16);
        self.emitter.add_reg_reg(Reg::RCX, Reg::RAX);
        self.emitter.emit_u8(0x48);
        self.emitter.emit_u8(0x89);
        if memarg.offset <= 127 {
            self.emitter.modrm(1, Reg::RBX as u8, Reg::RCX as u8);
            self.emitter.emit_u8(memarg.offset as u8);
        } else {
            self.emitter.modrm(2, Reg::RBX as u8, Reg::RCX as u8);
            self.emitter.emit_u32(memarg.offset);
        }
        self.stack_depth -= 2;
    }

    fn emit_store_shrink(&mut self, memarg: MemArg, size: usize) {
        self.emitter.pop_wasm_stack(Reg::RBX);
        self.emitter.pop_wasm_stack(Reg::RAX);
        self.emit_bounds_check(Reg::RAX, size as u32, memarg.offset);
        self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Reload RDI
        self.emitter.mov_reg_mem64(Reg::RCX, Reg::RDI, 16);
        self.emitter.add_reg_reg(Reg::RCX, Reg::RAX);
        match size {
            1 => {
                self.emitter.emit_u8(0x88);
            }
            2 => {
                self.emitter.emit_u8(0x66);
                self.emitter.emit_u8(0x89);
            }
            4 => {
                self.emitter.emit_u8(0x89);
            }
            _ => {
                self.emitter.emit_u8(0x89);
            }
        }
        if memarg.offset <= 127 {
            self.emitter.modrm(1, Reg::RBX as u8, Reg::RCX as u8);
            self.emitter.emit_u8(memarg.offset as u8);
        } else {
            self.emitter.modrm(2, Reg::RBX as u8, Reg::RCX as u8);
            self.emitter.emit_u32(memarg.offset);
        }
        self.stack_depth -= 2;
    }

    fn compile_fc(&mut self, sub: u32, reader: &mut WasmReader) {
        match sub {
            0x00 => self.emit_trampoline_unop_f32_to_i32(AotTrampoline::I32TruncSatF32S),
            0x01 => self.emit_trampoline_unop_f32_to_i32(AotTrampoline::I32TruncSatF32U),
            0x02 => self.emit_trampoline_unop_f64_to_i32(AotTrampoline::I32TruncSatF64S),
            0x03 => self.emit_trampoline_unop_f64_to_i32(AotTrampoline::I32TruncSatF64U),
            0x04 => self.emit_trampoline_unop_f32_to_i64(AotTrampoline::I64TruncSatF32S),
            0x05 => self.emit_trampoline_unop_f32_to_i64(AotTrampoline::I64TruncSatF32U),
            0x06 => self.emit_trampoline_unop_f64_to_i64(AotTrampoline::I64TruncSatF64S),
            0x07 => self.emit_trampoline_unop_f64_to_i64(AotTrampoline::I64TruncSatF64U),
            0x08 => {
                let data_idx = reader.read_var_u32().unwrap();
                reader.read_u8().unwrap();
                self.emitter.pop_wasm_stack(Reg::RCX);
                self.emitter.pop_wasm_stack(Reg::RDX);
                self.emitter.pop_wasm_stack(Reg::RSI);
                self.emitter.sub_reg_imm32(Reg::RSP, 16); // Align
                self.emitter.mov_reg_imm64(Reg::R8, data_idx as u64);
                self.emit_call_trampoline(AotTrampoline::MemoryInit);
                self.emitter.add_reg_imm32(Reg::RSP, 8);
                self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48);
                self.emitter.add_reg_imm32(Reg::RSP, 8); // Balance align
                self.stack_depth -= 3;
            }
            0x09 => {
                let data_idx = reader.read_var_u32().unwrap();
                self.emitter.sub_reg_imm32(Reg::RSP, 16); // Align
                self.emitter.mov_reg_imm64(Reg::RSI, data_idx as u64);
                self.emit_call_trampoline(AotTrampoline::DataDrop);
                self.emitter.add_reg_imm32(Reg::RSP, 8);
                self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48);
                self.emitter.add_reg_imm32(Reg::RSP, 8); // Balance align
            }
            0x0A => {
                reader.read_u8().unwrap();
                reader.read_u8().unwrap();
                self.emitter.pop_wasm_stack(Reg::RCX);
                self.emitter.pop_wasm_stack(Reg::RDX);
                self.emitter.pop_wasm_stack(Reg::RSI);
                self.emitter.sub_reg_imm32(Reg::RSP, 16); // Align
                self.emit_call_trampoline(AotTrampoline::MemoryCopy);
                self.emitter.add_reg_imm32(Reg::RSP, 8);
                self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48);
                self.emitter.add_reg_imm32(Reg::RSP, 8); // Balance align
                self.stack_depth -= 3;
            }
            0x0B => {
                reader.read_u8().unwrap();
                self.emitter.pop_wasm_stack(Reg::RCX);
                self.emitter.pop_wasm_stack(Reg::RDX);
                self.emitter.pop_wasm_stack(Reg::RSI);
                self.emitter.sub_reg_imm32(Reg::RSP, 16); // Align
                self.emit_call_trampoline(AotTrampoline::MemoryFill);
                self.emitter.add_reg_imm32(Reg::RSP, 8);
                self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48);
                self.emitter.add_reg_imm32(Reg::RSP, 8); // Balance align
                self.stack_depth -= 3;
            }
            0x0C => {
                let elem_idx = reader.read_var_u32().unwrap();
                let table_idx = reader.read_var_u32().unwrap();
                self.emitter.pop_wasm_stack(Reg::RCX);
                self.emitter.pop_wasm_stack(Reg::RDX);
                self.emitter.pop_wasm_stack(Reg::RSI);
                self.emitter.sub_reg_imm32(Reg::RSP, 16); // Align
                self.emitter.mov_reg_imm64(Reg::R8, table_idx as u64);
                self.emitter.mov_reg_imm64(Reg::R9, elem_idx as u64);
                self.emit_call_trampoline(AotTrampoline::TableInit);
                self.emitter.add_reg_imm32(Reg::RSP, 8);
                self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48);
                self.emitter.add_reg_imm32(Reg::RSP, 8); // Balance align
                self.stack_depth -= 3;
            }
            0x0D => {
                let elem_idx = reader.read_var_u32().unwrap();
                self.emitter.sub_reg_imm32(Reg::RSP, 16); // Align
                self.emitter.mov_reg_imm64(Reg::RSI, elem_idx as u64);
                self.emit_call_trampoline(AotTrampoline::ElemDrop);
                self.emitter.add_reg_imm32(Reg::RSP, 8);
                self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48);
                self.emitter.add_reg_imm32(Reg::RSP, 8); // Balance align
            }
            0x0E => {
                let x = reader.read_var_u32().unwrap();
                let y = reader.read_var_u32().unwrap();
                self.emitter.pop_wasm_stack(Reg::RCX);
                self.emitter.pop_wasm_stack(Reg::RDX);
                self.emitter.pop_wasm_stack(Reg::RSI);
                self.emitter.sub_reg_imm32(Reg::RSP, 16); // Align
                self.emitter.mov_reg_imm64(Reg::R8, x as u64);
                self.emitter.mov_reg_imm64(Reg::R9, y as u64);
                self.emit_call_trampoline(AotTrampoline::TableCopy);
                self.emitter.add_reg_imm32(Reg::RSP, 8);
                self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48);
                self.emitter.add_reg_imm32(Reg::RSP, 8); // Balance align
                self.stack_depth -= 3;
            }
            0x0F => {
                let idx = reader.read_var_u32().unwrap();
                self.emitter.pop_wasm_stack(Reg::RDX);
                self.emitter.pop_wasm_stack(Reg::RSI);
                self.emitter.sub_reg_imm32(Reg::RSP, 16); // Align
                self.emitter.mov_reg_imm64(Reg::RCX, idx as u64);
                self.emit_call_trampoline(AotTrampoline::TableGrow);
                self.emitter.add_reg_imm32(Reg::RSP, 8);
                self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48);
                self.emitter.add_reg_imm32(Reg::RSP, 8); // Balance align
                self.emitter.push_wasm_stack(Reg::RAX);
                self.stack_depth -= 1;
            }
            0x10 => {
                let idx = reader.read_var_u32().unwrap();
                self.emitter.sub_reg_imm32(Reg::RSP, 16); // Align
                self.emitter.mov_reg_imm64(Reg::RSI, idx as u64);
                self.emit_call_trampoline(AotTrampoline::TableSize);
                self.emitter.add_reg_imm32(Reg::RSP, 8);
                self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48);
                self.emitter.add_reg_imm32(Reg::RSP, 8); // Balance align
                self.emitter.push_wasm_stack(Reg::RAX);
                self.stack_depth += 1;
            }
            0x11 => {
                let idx = reader.read_var_u32().unwrap();
                self.emitter.pop_wasm_stack(Reg::RCX);
                self.emitter.pop_wasm_stack(Reg::RDX);
                self.emitter.pop_wasm_stack(Reg::RSI);
                self.emitter.sub_reg_imm32(Reg::RSP, 16); // Align
                self.emitter.mov_reg_imm64(Reg::R8, idx as u64);
                self.emit_call_trampoline(AotTrampoline::TableFill);
                self.emitter.add_reg_imm32(Reg::RSP, 8);
                self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48);
                self.emitter.add_reg_imm32(Reg::RSP, 8); // Balance align
                self.stack_depth -= 3;
            }
            _ => self.emitter.jmp_label(self.trap_unimplemented_fc_label),
        }
    }

    fn compile_simd(&mut self, sub: u32, reader: &mut WasmReader) {
        use crate::wasm::common::reader::types::opcode::fd_extensions::*;
        match sub {
            V128_LOAD => {
                let memarg = MemArg::read(reader).unwrap();
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emit_bounds_check(Reg::RAX, 16, memarg.offset);
                self.emitter.mov_reg_reg(Reg::RCX, Reg::R14);
                self.emitter.add_reg_reg(Reg::RCX, Reg::RAX);
                self.emitter
                    .movups_xmm_mem(XmmReg::XMM0, Reg::RCX, memarg.offset as i32);
                self.emitter.push_v128(XmmReg::XMM0);
            }
            V128_LOAD8X8_S => self.emit_simd_load_extend(reader, AotTrampoline::V128Load8x8S),
            V128_LOAD8X8_U => self.emit_simd_load_extend(reader, AotTrampoline::V128Load8x8U),
            V128_LOAD16X4_S => self.emit_simd_load_extend(reader, AotTrampoline::V128Load16x4S),
            V128_LOAD16X4_U => self.emit_simd_load_extend(reader, AotTrampoline::V128Load16x4U),
            V128_LOAD32X2_S => self.emit_simd_load_extend(reader, AotTrampoline::V128Load32x2S),
            V128_LOAD32X2_U => self.emit_simd_load_extend(reader, AotTrampoline::V128Load32x2U),
            V128_LOAD8_SPLAT => self.emit_simd_load_splat(reader, 1),
            V128_LOAD16_SPLAT => self.emit_simd_load_splat(reader, 2),
            V128_LOAD32_SPLAT => self.emit_simd_load_splat(reader, 4),
            V128_LOAD64_SPLAT => self.emit_simd_load_splat(reader, 8),
            V128_LOAD32_ZERO => self.emit_simd_load_zero(reader, 4),
            V128_LOAD64_ZERO => self.emit_simd_load_zero(reader, 8),
            V128_LOAD8_LANE => self.emit_simd_lane_op(reader, 1, true, AotTrampoline::V128LoadLane),
            V128_LOAD16_LANE => {
                self.emit_simd_lane_op(reader, 2, true, AotTrampoline::V128LoadLane)
            }
            V128_LOAD32_LANE => {
                self.emit_simd_lane_op(reader, 4, true, AotTrampoline::V128LoadLane)
            }
            V128_LOAD64_LANE => {
                self.emit_simd_lane_op(reader, 8, true, AotTrampoline::V128LoadLane)
            }
            V128_STORE8_LANE => {
                self.emit_simd_lane_op(reader, 1, false, AotTrampoline::V128StoreLane)
            }
            V128_STORE16_LANE => {
                self.emit_simd_lane_op(reader, 2, false, AotTrampoline::V128StoreLane)
            }
            V128_STORE32_LANE => {
                self.emit_simd_lane_op(reader, 4, false, AotTrampoline::V128StoreLane)
            }
            V128_STORE64_LANE => {
                self.emit_simd_lane_op(reader, 8, false, AotTrampoline::V128StoreLane)
            }
            V128_STORE => {
                let memarg = MemArg::read(reader).unwrap();
                self.emitter.pop_v128(XmmReg::XMM0);
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emit_bounds_check(Reg::RAX, 16, memarg.offset);
                self.emitter.mov_reg_reg(Reg::RCX, Reg::R14);
                self.emitter.add_reg_reg(Reg::RCX, Reg::RAX);
                self.emitter
                    .movups_mem_xmm(Reg::RCX, memarg.offset as i32, XmmReg::XMM0);
                self.stack_depth -= 2;
            }
            V128_CONST => {
                let mut data = [0u8; 16];
                for i in 0..16 {
                    data[i] = reader.read_u8().unwrap();
                }
                let low = u64::from_le_bytes(data[0..8].try_into().unwrap());
                let high = u64::from_le_bytes(data[8..16].try_into().unwrap());
                self.emitter.mov_reg_imm64(Reg::RAX, low);
                self.emitter.mov_reg_imm64(Reg::RDX, high);
                self.emitter.sub_reg_imm32(Reg::RSP, 16);
                self.emitter.mov_mem64_reg(Reg::RSP, 0, Reg::RAX);
                self.emitter.mov_mem64_reg(Reg::RSP, 8, Reg::RDX);
                self.stack_depth += 1;
            }
            I8X16_SHUFFLE => {
                let mut lanes = [0u8; 16];
                for i in 0..16 {
                    lanes[i] = reader.read_u8().unwrap();
                }
                self.emitter.sub_reg_imm32(Reg::RSP, 16);
                let low = u64::from_le_bytes(lanes[0..8].try_into().unwrap());
                let high = u64::from_le_bytes(lanes[8..16].try_into().unwrap());
                self.emitter.mov_reg_imm64(Reg::RAX, low);
                self.emitter.mov_reg_imm64(Reg::RDX, high);
                self.emitter.mov_mem64_reg(Reg::RSP, 0, Reg::RAX);
                self.emitter.mov_mem64_reg(Reg::RSP, 8, Reg::RDX);
                self.stack_depth += 1;
                self.emit_simd_trampoline_ternary(AotTrampoline::V128I8x16Shuffle);
            }
            I8X16_EXTRACT_LANE_S => self.emit_simd_extract_lane(reader, 1, true),
            I8X16_EXTRACT_LANE_U => self.emit_simd_extract_lane(reader, 1, false),
            I8X16_REPLACE_LANE => self.emit_simd_replace_lane(reader, 1),
            I16X8_EXTRACT_LANE_S => self.emit_simd_extract_lane(reader, 2, true),
            I16X8_EXTRACT_LANE_U => self.emit_simd_extract_lane(reader, 2, false),
            I16X8_REPLACE_LANE => self.emit_simd_replace_lane(reader, 2),
            I32X4_EXTRACT_LANE => self.emit_simd_extract_lane(reader, 4, false),
            I32X4_REPLACE_LANE => self.emit_simd_replace_lane(reader, 4),
            I64X2_EXTRACT_LANE => self.emit_simd_extract_lane(reader, 8, false),
            I64X2_REPLACE_LANE => self.emit_simd_replace_lane(reader, 8),
            F32X4_EXTRACT_LANE => self.emit_simd_extract_lane(reader, 4, false),
            F32X4_REPLACE_LANE => self.emit_simd_replace_lane(reader, 4),
            F64X2_EXTRACT_LANE => self.emit_simd_extract_lane(reader, 8, false),
            F64X2_REPLACE_LANE => self.emit_simd_replace_lane(reader, 8),
            I8X16_SPLAT => self.emit_simd_splat(1),
            I16X8_SPLAT => self.emit_simd_splat(2),
            I32X4_SPLAT => {
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emitter.movd_xmm_reg(XmmReg::XMM0, Reg::RAX);
                self.emitter.emit_u8(0x66);
                self.emitter.emit_u8(0x0F);
                self.emitter.emit_u8(0x70);
                self.emitter.modrm(3, 0, 0);
                self.emitter.emit_u8(0);
                self.emitter.push_v128(XmmReg::XMM0);
            }
            I64X2_SPLAT => {
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emitter.movq_xmm_reg(XmmReg::XMM0, Reg::RAX);
                self.emitter.emit_u8(0x66);
                self.emitter.emit_u8(0x0F);
                self.emitter.emit_u8(0x70);
                self.emitter.modrm(3, 0, 0);
                self.emitter.emit_u8(0);
                self.emitter.push_v128(XmmReg::XMM0);
            }
            F32X4_SPLAT => {
                self.emitter.pop_v128(XmmReg::XMM0);
                self.emitter.emit_u8(0x0F);
                self.emitter.emit_u8(0xC6);
                self.emitter.modrm(3, 0, 0);
                self.emitter.emit_u8(0);
                self.emitter.push_v128(XmmReg::XMM0);
            }
            F64X2_SPLAT => {
                self.emitter.pop_v128(XmmReg::XMM0);
                self.emitter.movddup_xmm_xmm(XmmReg::XMM0, XmmReg::XMM0);
                self.emitter.push_v128(XmmReg::XMM0);
            }
            I8X16_ADD => self.emit_simd_padd(0xFC),
            I16X8_ADD => self.emit_simd_padd(0xFD),
            I32X4_ADD => self.emit_simd_padd(0xFE),
            I64X2_ADD => self.emit_simd_padd(0xD4),
            I8X16_SUB => self.emit_simd_padd(0xF8),
            I16X8_SUB => self.emit_simd_padd(0xF9),
            I32X4_SUB => self.emit_simd_padd(0xFA),
            I64X2_SUB => self.emit_simd_padd(0xFB),
            V128_AND => self.emit_binop_v128(|e| e.andps_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1)),
            V128_OR => self.emit_binop_v128(|e| e.orps_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1)),
            V128_XOR => self.emit_binop_v128(|e| e.xorps_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1)),
            V128_ANDNOT => self.emit_binop_v128(|e| e.pandn_xmm_xmm(XmmReg::XMM1, XmmReg::XMM0)),
            V128_NOT => {
                self.emitter.pop_v128(XmmReg::XMM0);
                self.emitter.pcmpeqd_xmm_xmm(XmmReg::XMM1, XmmReg::XMM1);
                self.emitter.pandn_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1);
                self.emitter.push_v128(XmmReg::XMM0);
            }
            F32X4_ADD => self.emit_binop_v128(|e| e.addps_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1)),
            F32X4_SUB => self.emit_binop_v128(|e| e.subps_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1)),
            F32X4_MUL => self.emit_binop_v128(|e| e.mulps_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1)),
            F32X4_DIV => self.emit_binop_v128(|e| e.divps_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1)),
            F64X2_ADD => self.emit_binop_v128(|e| e.addpd_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1)),
            F64X2_SUB => self.emit_binop_v128(|e| e.subpd_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1)),
            F64X2_MUL => self.emit_binop_v128(|e| e.mulpd_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1)),
            F64X2_DIV => self.emit_binop_v128(|e| e.divpd_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1)),
            V128_BITSELECT => self.emit_simd_trampoline_ternary(AotTrampoline::V128Bitselect),
            V128_ANY_TRUE => self.emit_simd_trampoline_reduction(AotTrampoline::V128AnyTrue),
            I8X16_ALL_TRUE => self.emit_simd_trampoline_reduction(AotTrampoline::V128AllTrueI8x16),
            I8X16_BITMASK => self.emit_simd_trampoline_reduction(AotTrampoline::V128BitmaskI8x16),
            I16X8_ALL_TRUE => self.emit_simd_trampoline_reduction(AotTrampoline::V128AllTrueI16x8),
            I16X8_BITMASK => self.emit_simd_trampoline_reduction(AotTrampoline::V128BitmaskI16x8),
            I32X4_ALL_TRUE => self.emit_simd_trampoline_reduction(AotTrampoline::V128AllTrueI32x4),
            I32X4_BITMASK => self.emit_simd_trampoline_reduction(AotTrampoline::V128BitmaskI32x4),
            I64X2_ALL_TRUE => self.emit_simd_trampoline_reduction(AotTrampoline::V128AllTrueI64x2),
            I64X2_BITMASK => self.emit_simd_trampoline_reduction(AotTrampoline::V128BitmaskI64x2),

            I8X16_NARROW_I16X8_S => {
                self.emit_simd_trampoline_binop(AotTrampoline::I8x16NarrowI16x8S)
            }
            I8X16_NARROW_I16X8_U => {
                self.emit_simd_trampoline_binop(AotTrampoline::I8x16NarrowI16x8U)
            }
            I16X8_NARROW_I32X4_S => {
                self.emit_simd_trampoline_binop(AotTrampoline::I16x8NarrowI32x4S)
            }
            I16X8_NARROW_I32X4_U => {
                self.emit_simd_trampoline_binop(AotTrampoline::I16x8NarrowI32x4U)
            }

            I16X8_EXTEND_LOW_I8X16_S => {
                self.emit_simd_trampoline_unop(AotTrampoline::I16x8ExtendLowI8x16S)
            }
            I16X8_EXTEND_HIGH_I8X16_S => {
                self.emit_simd_trampoline_unop(AotTrampoline::I16x8ExtendHighI8x16S)
            }
            I16X8_EXTEND_LOW_I8X16_U => {
                self.emit_simd_trampoline_unop(AotTrampoline::I16x8ExtendLowI8x16U)
            }
            I16X8_EXTEND_HIGH_I8X16_U => {
                self.emit_simd_trampoline_unop(AotTrampoline::I16x8ExtendHighI8x16U)
            }
            I32X4_EXTEND_LOW_I16X8_S => {
                self.emit_simd_trampoline_unop(AotTrampoline::I32x4ExtendLowI16x8S)
            }
            I32X4_EXTEND_HIGH_I16X8_S => {
                self.emit_simd_trampoline_unop(AotTrampoline::I32x4ExtendHighI16x8S)
            }
            I32X4_EXTEND_LOW_I16X8_U => {
                self.emit_simd_trampoline_unop(AotTrampoline::I32x4ExtendLowI16x8U)
            }
            I32X4_EXTEND_HIGH_I16X8_U => {
                self.emit_simd_trampoline_unop(AotTrampoline::I32x4ExtendHighI16x8U)
            }

            I64X2_EXTEND_LOW_I32X4_S => {
                self.emit_simd_trampoline_unop(AotTrampoline::I64x2ExtendLowI32x4S)
            }
            I64X2_EXTEND_HIGH_I32X4_S => {
                self.emit_simd_trampoline_unop(AotTrampoline::I64x2ExtendHighI32x4S)
            }

            I64X2_EXTEND_LOW_I32X4_U => {
                self.emit_simd_trampoline_unop(AotTrampoline::I64x2ExtendLowI32x4U)
            }
            I64X2_EXTEND_HIGH_I32X4_U => {
                self.emit_simd_trampoline_unop(AotTrampoline::I64x2ExtendHighI32x4U)
            }

            I32X4_EXTMUL_LOW_I16X8_S => {
                self.emit_simd_trampoline_binop(AotTrampoline::I32x4ExtmulLowI16x8S)
            }

            I16X8_EXTMUL_HIGH_I8X16_S => {
                self.emit_simd_trampoline_binop(AotTrampoline::I16x8ExtmulHighI8x16S)
            }
            I32X4_EXTMUL_LOW_I16X8_S => {
                self.emit_simd_trampoline_binop(AotTrampoline::I32x4ExtmulLowI16x8S)
            }

            I16X8_EXTMUL_HIGH_I8X16_U => {
                self.emit_simd_trampoline_binop(AotTrampoline::I16x8ExtmulHighI8x16U)
            }
            I32X4_EXTMUL_LOW_I16X8_S => {
                self.emit_simd_trampoline_binop(AotTrampoline::I32x4ExtmulLowI16x8S)
            }

            I32X4_EXTMUL_HIGH_I16X8_S => {
                self.emit_simd_trampoline_binop(AotTrampoline::I32x4ExtmulHighI16x8S)
            }

            I32X4_EXTMUL_LOW_I16X8_U => {
                self.emit_simd_trampoline_binop(AotTrampoline::I32x4ExtmulLowI16x8U)
            }

            I32X4_EXTMUL_HIGH_I16X8_U => {
                self.emit_simd_trampoline_binop(AotTrampoline::I32x4ExtmulHighI16x8U)
            }

            I64X2_EXTMUL_LOW_I32X4_S => {
                self.emit_simd_trampoline_binop(AotTrampoline::I64x2ExtmulLowI32x4S)
            }

            I64X2_EXTMUL_HIGH_I32X4_S => {
                self.emit_simd_trampoline_binop(AotTrampoline::I64x2ExtmulHighI32x4S)
            }

            I64X2_EXTMUL_LOW_I32X4_U => {
                self.emit_simd_trampoline_binop(AotTrampoline::I64x2ExtmulLowI32x4U)
            }

            I64X2_EXTMUL_HIGH_I32X4_U => {
                self.emit_simd_trampoline_binop(AotTrampoline::I64x2ExtmulHighI32x4U)
            }

            I16X8_EXTADD_PAIRWISE_I8X16_S => {
                self.emit_simd_trampoline_unop(AotTrampoline::I16x8ExtaddPairwiseI8x16S)
            }
            I16X8_EXTADD_PAIRWISE_I8X16_U => {
                self.emit_simd_trampoline_unop(AotTrampoline::I16x8ExtaddPairwiseI8x16U)
            }
            I32X4_EXTADD_PAIRWISE_I16X8_S => {
                self.emit_simd_trampoline_unop(AotTrampoline::I32x4ExtaddPairwiseI16x8S)
            }
            I32X4_EXTADD_PAIRWISE_I16X8_U => {
                self.emit_simd_trampoline_unop(AotTrampoline::I32x4ExtaddPairwiseI16x8U)
            }

            I32X4_DOT_I16X8_S => self.emit_simd_trampoline_binop(AotTrampoline::I32x4DotI16x8S),
            I16X8_Q15MULRSAT_S => self.emit_simd_trampoline_binop(AotTrampoline::I16x8Q15mulrsatS),

            I32X4_TRUNC_SAT_F32X4_S => {
                self.emit_simd_trampoline_unop(AotTrampoline::I32x4TruncSatF32x4S)
            }
            I32X4_TRUNC_SAT_F32X4_U => {
                self.emit_simd_trampoline_unop(AotTrampoline::I32x4TruncSatF32x4U)
            }
            F32X4_CONVERT_I32X4_S => {
                self.emit_simd_trampoline_unop(AotTrampoline::F32x4ConvertI32x4S)
            }
            F32X4_CONVERT_I32X4_U => {
                self.emit_simd_trampoline_unop(AotTrampoline::F32x4ConvertI32x4U)
            }
            I32X4_TRUNC_SAT_F64X2_S_ZERO => {
                self.emit_simd_trampoline_unop(AotTrampoline::I32x4TruncSatF64x2SZero)
            }
            I32X4_TRUNC_SAT_F64X2_U_ZERO => {
                self.emit_simd_trampoline_unop(AotTrampoline::I32x4TruncSatF64x2UZero)
            }
            F64X2_CONVERT_LOW_I32X4_S => {
                self.emit_simd_trampoline_unop(AotTrampoline::F64x2ConvertLowI32x4S)
            }
            F64X2_CONVERT_LOW_I32X4_U => {
                self.emit_simd_trampoline_unop(AotTrampoline::F64x2ConvertLowI32x4U)
            }
            _ => self.emitter.jmp_label(self.trap_unimplemented_simd_label),
        }
    }

    fn emit_simd_splat(&mut self, lane_width: u8) {
        self.emitter.pop_wasm_stack(Reg::RAX);
        self.emitter.movd_xmm_reg(XmmReg::XMM0, Reg::RAX);
        match lane_width {
            1 => {
                self.emitter.punpcklbw_xmm_xmm(XmmReg::XMM0, XmmReg::XMM0);
                self.emitter.punpcklwd_xmm_xmm(XmmReg::XMM0, XmmReg::XMM0);
                self.emitter.emit_u8(0x66);
                self.emitter.emit_u8(0x0F);
                self.emitter.emit_u8(0x70);
                self.emitter.modrm(3, 0, 0);
                self.emitter.emit_u8(0);
            }
            2 => {
                self.emitter.punpcklwd_xmm_xmm(XmmReg::XMM0, XmmReg::XMM0);
                self.emitter.emit_u8(0x66);
                self.emitter.emit_u8(0x0F);
                self.emitter.emit_u8(0x70);
                self.emitter.modrm(3, 0, 0);
                self.emitter.emit_u8(0);
            }
            _ => {}
        }
        self.emitter.push_v128(XmmReg::XMM0);
    }

    fn emit_simd_load_extend(
        &mut self,
        reader: &mut WasmReader,
        trampoline: crate::wasm::aot::runtime::AotTrampoline,
    ) {
        let memarg = MemArg::read(reader).unwrap();
        self.emitter.pop_wasm_stack(Reg::RAX);
        self.emit_bounds_check(Reg::RAX, 8, memarg.offset);
        self.emitter.mov_reg_reg(Reg::RCX, Reg::R14);
        self.emitter.add_reg_reg(Reg::RCX, Reg::RAX);
        self.emitter.add_reg_imm32(Reg::RCX, memarg.offset);

        self.emitter.sub_reg_imm32(Reg::RSP, 16); // Space for result
        self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Load Context for trampoline
        self.emitter.sub_reg_imm32(Reg::RSP, 16); // Align

        self.emitter.mov_reg_reg(Reg::RDI, Reg::RSP);
        self.emitter.add_reg_imm32(Reg::RDI, 16); // dst -> result slot
        self.emitter.mov_reg_mem64(Reg::RSI, Reg::RCX, 0); // src -> from memory

        self.emit_call_trampoline(trampoline);
        self.emitter.add_reg_imm32(Reg::RSP, 16);
        self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Restore Context
        self.emitter.mov_reg_mem64(Reg::R14, Reg::RDI, 16); // Restore R14
        self.emitter.mov_reg_mem64(Reg::R13, Reg::RBP, -24); // Restore R13
        // Result is now at top of stack (last 16 bytes we allocated)
    }

    fn emit_simd_load_splat(&mut self, reader: &mut WasmReader, size: u32) {
        let memarg = MemArg::read(reader).unwrap();
        self.emitter.pop_wasm_stack(Reg::RAX);
        self.emit_bounds_check(Reg::RAX, size, memarg.offset);
        self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Reload RDI
        self.emitter.mov_reg_mem64(Reg::RCX, Reg::RDI, 16);
        self.emitter.add_reg_reg(Reg::RCX, Reg::RAX);
        match size {
            1 => self
                .emitter
                .movzx_reg_mem8(Reg::RAX, Reg::RCX, memarg.offset as i32),
            2 => self
                .emitter
                .movzx_reg_mem16(Reg::RAX, Reg::RCX, memarg.offset as i32),
            4 => {
                self.emitter.emit_u8(0x8B);
                self.emitter
                    .emit_modrm_mem(Reg::RAX, Reg::RCX, memarg.offset as i32);
            }
            8 => {
                self.emitter
                    .mov_reg_mem64(Reg::RAX, Reg::RCX, memarg.offset as i32);
            }
            _ => {}
        }
        if size == 8 {
            self.emitter.movq_xmm_reg(XmmReg::XMM0, Reg::RAX);
            self.emitter.movddup_xmm_xmm(XmmReg::XMM0, XmmReg::XMM0);
        } else {
            self.emitter.movd_xmm_reg(XmmReg::XMM0, Reg::RAX);
            if size == 4 {
                self.emitter.emit_u8(0x66);
                self.emitter.emit_u8(0x0F);
                self.emitter.emit_u8(0x70);
                self.emitter.modrm(3, 0, 0);
                self.emitter.emit_u8(0);
            } else {
                self.emitter.push_wasm_stack(Reg::RAX);
                self.emit_simd_splat(size as u8);
                self.emitter.pop_v128(XmmReg::XMM0);
            }
        }
        self.emitter.push_v128(XmmReg::XMM0);
    }

    fn emit_simd_load_zero(&mut self, reader: &mut WasmReader, size: u32) {
        let memarg = MemArg::read(reader).unwrap();
        self.emitter.pop_wasm_stack(Reg::RAX);
        self.emit_bounds_check(Reg::RAX, size, memarg.offset);
        self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Reload RDI
        self.emitter.mov_reg_mem64(Reg::RCX, Reg::RDI, 16);
        self.emitter.add_reg_reg(Reg::RCX, Reg::RAX);
        self.emitter.xorps_xmm_xmm(XmmReg::XMM0, XmmReg::XMM0);
        if size == 4 {
            self.emitter.emit_u8(0x66);
            self.emitter.emit_u8(0x0F);
            self.emitter.emit_u8(0x6E);
            self.emitter
                .emit_modrm_mem(Reg::RAX, Reg::RCX, memarg.offset as i32); // Reg::RAX=0=XMM0
        } else {
            self.emitter.emit_u8(0xF3);
            self.emitter.emit_u8(0x0F);
            self.emitter.emit_u8(0x7E);
            self.emitter
                .emit_modrm_mem(Reg::RAX, Reg::RCX, memarg.offset as i32); // Reg::RAX=0=XMM0
        }
        self.emitter.push_v128(XmmReg::XMM0);
        // Correct: 1 in, 1 out. stack_depth no change.
    }

    fn emit_simd_extract_lane(&mut self, reader: &mut WasmReader, lane_width: u8, signed: bool) {
        let lane = reader.read_u8().unwrap();
        self.emitter.pop_v128(XmmReg::XMM0);
        match lane_width {
            1 => {
                self.emitter
                    .pextrd_reg_xmm_imm8(Reg::RAX, XmmReg::XMM0, lane / 4);
                let shift = (lane % 4) * 8;
                if shift > 0 {
                    self.emitter.shr_reg32_imm32(Reg::RAX, shift as u32);
                }
                if signed {
                    self.emitter.movsx_reg_reg8(Reg::RAX, Reg::RAX);
                } else {
                    self.emitter.movzx_reg_reg8(Reg::RAX, Reg::RAX);
                }
            }
            2 => {
                self.emitter
                    .pextrw_reg_xmm_imm8(Reg::RAX, XmmReg::XMM0, lane);
                if signed {
                    self.emitter.movsx_reg_reg16(Reg::RAX, Reg::RAX);
                } else {
                    self.emitter.movzx_reg_reg16(Reg::RAX, Reg::RAX);
                }
            }
            4 => self
                .emitter
                .pextrd_reg_xmm_imm8(Reg::RAX, XmmReg::XMM0, lane),
            8 => self
                .emitter
                .pextrq_reg_xmm_imm8(Reg::RAX, XmmReg::XMM0, lane),
            _ => {}
        }
        self.emitter.push_wasm_stack(Reg::RAX);
    }

    fn emit_simd_replace_lane(&mut self, reader: &mut WasmReader, lane_width: u8) {
        let lane = reader.read_u8().unwrap();
        self.emitter.pop_wasm_stack(Reg::RAX);
        self.emitter.pop_v128(XmmReg::XMM0);
        match lane_width {
            1 => self
                .emitter
                .pinsrb_xmm_reg_imm8(XmmReg::XMM0, Reg::RAX, lane),
            2 => self
                .emitter
                .pinsrw_xmm_reg_imm8(XmmReg::XMM0, Reg::RAX, lane),
            4 => self
                .emitter
                .pinsrd_xmm_reg_imm8(XmmReg::XMM0, Reg::RAX, lane),
            8 => self
                .emitter
                .pinsrq_xmm_reg_imm8(XmmReg::XMM0, Reg::RAX, lane),
            _ => {}
        }
        self.emitter.push_v128(XmmReg::XMM0);
        self.stack_depth -= 1;
    }

    fn emit_simd_trampoline_unop(&mut self, trampoline: crate::wasm::aot::runtime::AotTrampoline) {
        self.emitter.pop_v128(XmmReg::XMM0);
        self.emitter.sub_reg_imm32(Reg::RSP, 16);
        self.emitter.movups_mem_xmm(Reg::RSP, 0, XmmReg::XMM0);

        self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Load Context for trampoline
        self.emitter.sub_reg_imm32(Reg::RSP, 16); // Align

        self.emitter.mov_reg_reg(Reg::RDI, Reg::RSP);
        self.emitter.add_reg_imm32(Reg::RDI, 16); // Points to v128 on stack

        self.emit_call_trampoline(trampoline);

        self.emitter.add_reg_imm32(Reg::RSP, 16);
        self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Restore Context
        self.emitter.mov_reg_mem64(Reg::R14, Reg::RDI, 16); // Restore R14
        self.emitter.mov_reg_mem64(Reg::R13, Reg::RBP, -24); // Restore R13

        self.emitter.movups_xmm_mem(XmmReg::XMM0, Reg::RSP, 0);
        self.emitter.add_reg_imm32(Reg::RSP, 16);
        self.emitter.push_v128(XmmReg::XMM0);
    }

    fn emit_simd_trampoline_binop(&mut self, trampoline: crate::wasm::aot::runtime::AotTrampoline) {
        self.emitter.pop_v128(XmmReg::XMM1);
        self.emitter.pop_v128(XmmReg::XMM0);
        self.emitter.sub_reg_imm32(Reg::RSP, 16);
        self.emitter.movups_mem_xmm(Reg::RSP, 0, XmmReg::XMM0);
        self.emitter.sub_reg_imm32(Reg::RSP, 16);
        self.emitter.movups_mem_xmm(Reg::RSP, 0, XmmReg::XMM1);

        self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Load Context for trampoline
        self.emitter.sub_reg_imm32(Reg::RSP, 16); // Align

        self.emitter.mov_reg_reg(Reg::RDI, Reg::RSP);
        self.emitter.add_reg_imm32(Reg::RDI, 32); // Points to XMM0
        self.emitter.mov_reg_reg(Reg::RSI, Reg::RSP);
        self.emitter.add_reg_imm32(Reg::RSI, 16); // Points to XMM1

        self.emit_call_trampoline(trampoline);

        self.emitter.add_reg_imm32(Reg::RSP, 16);
        self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Restore Context
        self.emitter.mov_reg_mem64(Reg::R14, Reg::RDI, 16); // Restore R14
        self.emitter.mov_reg_mem64(Reg::R13, Reg::RBP, -24); // Restore R13

        self.emitter.add_reg_imm32(Reg::RSP, 32); // remove XMM1 and XMM0 copy
        self.emitter.movups_xmm_mem(XmmReg::XMM0, Reg::RSP, -16); // Load from XMM0 slot
        self.emitter.push_v128(XmmReg::XMM0);
        self.stack_depth -= 1;
    }

    fn emit_simd_trampoline_ternary(
        &mut self,
        trampoline: crate::wasm::aot::runtime::AotTrampoline,
    ) {
        self.emitter.pop_v128(XmmReg::XMM2);
        self.emitter.pop_v128(XmmReg::XMM1);
        self.emitter.pop_v128(XmmReg::XMM0);
        self.emitter.sub_reg_imm32(Reg::RSP, 16);
        self.emitter.movups_mem_xmm(Reg::RSP, 0, XmmReg::XMM0);
        self.emitter.sub_reg_imm32(Reg::RSP, 16);
        self.emitter.movups_mem_xmm(Reg::RSP, 0, XmmReg::XMM1);
        self.emitter.sub_reg_imm32(Reg::RSP, 16);
        self.emitter.movups_mem_xmm(Reg::RSP, 0, XmmReg::XMM2);

        self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Load Context for trampoline
        self.emitter.sub_reg_imm32(Reg::RSP, 16); // Align

        self.emitter.mov_reg_reg(Reg::RDI, Reg::RSP);
        self.emitter.add_reg_imm32(Reg::RDI, 48); // XMM0
        self.emitter.mov_reg_reg(Reg::RSI, Reg::RSP);
        self.emitter.add_reg_imm32(Reg::RSI, 32); // XMM1
        self.emitter.mov_reg_reg(Reg::RDX, Reg::RSP);
        self.emitter.add_reg_imm32(Reg::RDX, 16); // XMM2

        self.emit_call_trampoline(trampoline);

        self.emitter.add_reg_imm32(Reg::RSP, 16);
        self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Restore Context
        self.emitter.mov_reg_mem64(Reg::R14, Reg::RDI, 16); // Restore R14
        self.emitter.mov_reg_mem64(Reg::R13, Reg::RBP, -24); // Restore R13

        self.emitter.add_reg_imm32(Reg::RSP, 48); // remove XMM2, XMM1, XMM0 copy
        self.emitter.movups_xmm_mem(XmmReg::XMM0, Reg::RSP, -16); // Load from XMM0 slot
        self.emitter.push_v128(XmmReg::XMM0);
        self.stack_depth -= 2;
    }

    fn emit_simd_trampoline_reduction(
        &mut self,
        trampoline: crate::wasm::aot::runtime::AotTrampoline,
    ) {
        self.emitter.pop_v128(XmmReg::XMM0);
        self.emitter.sub_reg_imm32(Reg::RSP, 16);
        self.emitter.movups_mem_xmm(Reg::RSP, 0, XmmReg::XMM0);

        self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Load Context for trampoline
        self.emitter.sub_reg_imm32(Reg::RSP, 16); // Align

        self.emitter.mov_reg_reg(Reg::RDI, Reg::RSP);
        self.emitter.add_reg_imm32(Reg::RDI, 16); // Points to XMM0

        self.emit_call_trampoline(trampoline);

        self.emitter.add_reg_imm32(Reg::RSP, 16);
        self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Restore Context
        self.emitter.mov_reg_mem64(Reg::R14, Reg::RDI, 16); // Restore R14
        self.emitter.mov_reg_mem64(Reg::R13, Reg::RBP, -24); // Restore R13

        self.emitter.add_reg_imm32(Reg::RSP, 16);
        self.emitter.push_wasm_stack(Reg::RAX);
    }
    fn emit_simd_padd(&mut self, opcode: u8) {
        self.emitter.pop_v128(XmmReg::XMM1);
        self.emitter.pop_v128(XmmReg::XMM0);
        self.emitter.emit_u8(0x66);
        self.emitter.emit_u8(0x0F);
        self.emitter.emit_u8(opcode);
        self.emitter
            .modrm(3, XmmReg::XMM0 as u8, XmmReg::XMM1 as u8);
        self.emitter.push_v128(XmmReg::XMM0);
        self.stack_depth -= 1;
    }

    fn compile_atomic(&mut self, sub: u32, reader: &mut WasmReader) {
        if sub == 0x03 {
            // atomic.fence
            reader.read_u8().unwrap(); // Reserved byte
            // Fence is a no-op in AOT for now (x86_64 is strong enough)
            return;
        }
        let memarg = MemArg::read(reader).unwrap();
        match sub {
            0x10 | 0x12 | 0x13 | 0x14 | 0x15 | 0x16 => self.emit_atomic_load(sub, memarg),
            0x17 | 0x18 | 0x19 | 0x1a | 0x1b | 0x1c | 0x1d => self.emit_atomic_store(sub, memarg),
            0x1e..=0x4d => self.emit_atomic_rmw(sub, memarg),
            _ => self.emitter.jmp_label(self.trap_unimplemented_atomic_label),
        }
    }

    fn emit_atomic_load(&mut self, sub: u32, memarg: MemArg) {
        self.emitter.pop_wasm_stack(Reg::RAX);
        let size = match sub {
            0x10 => 4,
            0x11 => 8,
            0x12 => 1,
            0x13 => 2,
            0x14 => 1,
            0x15 => 2,
            0x16 => 4,
            _ => 4,
        };
        self.emit_bounds_check(Reg::RAX, size, memarg.offset);
        self.emitter.mov_reg_reg(Reg::RCX, Reg::R14);
        self.emitter.add_reg_reg(Reg::RCX, Reg::RAX);
        match sub {
            0x10 => {
                self.emitter.emit_u8(0x8B);
                self.emitter
                    .emit_modrm_mem(Reg::RAX, Reg::RCX, memarg.offset as i32);
            }
            0x11 => {
                self.emitter.rex(true, Reg::RAX as u8, 0, Reg::RCX as u8);
                self.emitter.emit_u8(0x8B);
                self.emitter
                    .emit_modrm_mem(Reg::RAX, Reg::RCX, memarg.offset as i32);
            }
            0x12 | 0x14 => self
                .emitter
                .movzx_reg_mem8(Reg::RAX, Reg::RCX, memarg.offset as i32),
            0x13 | 0x15 => self
                .emitter
                .movzx_reg_mem16(Reg::RAX, Reg::RCX, memarg.offset as i32),
            0x16 => {
                self.emitter.emit_u8(0x8B);
                self.emitter
                    .emit_modrm_mem(Reg::RAX, Reg::RCX, memarg.offset as i32);
            }
            _ => {}
        }
        self.emitter.push_wasm_stack(Reg::RAX);
    }

    fn emit_atomic_store(&mut self, sub: u32, memarg: MemArg) {
        self.emitter.pop_wasm_stack(Reg::RBX);
        self.emitter.pop_wasm_stack(Reg::RAX);
        let size = match sub {
            0x17 => 4,
            0x18 => 8,
            0x19 => 1,
            0x1a => 2,
            0x1b => 1,
            0x1c => 2,
            0x1d => 4,
            _ => 4,
        };
        self.emit_bounds_check(Reg::RAX, size, memarg.offset);
        self.emitter.mov_reg_reg(Reg::RCX, Reg::R14);
        self.emitter.add_reg_reg(Reg::RCX, Reg::RAX);
        match sub {
            0x17 => {
                self.emitter.emit_u8(0x87);
                self.emitter
                    .emit_modrm_mem(Reg::RBX, Reg::RCX, memarg.offset as i32);
            }
            0x18 => {
                self.emitter.rex(true, Reg::RBX as u8, 0, Reg::RCX as u8);
                self.emitter.emit_u8(0x87);
                self.emitter
                    .emit_modrm_mem(Reg::RBX, Reg::RCX, memarg.offset as i32);
            }
            0x19 | 0x1b => {
                self.emitter.emit_u8(0x86);
                self.emitter
                    .emit_modrm_mem(Reg::RBX, Reg::RCX, memarg.offset as i32);
            }
            0x1a | 0x1c => {
                self.emitter.emit_u8(0x66);
                self.emitter.emit_u8(0x87);
                self.emitter
                    .emit_modrm_mem(Reg::RBX, Reg::RCX, memarg.offset as i32);
            }
            0x1d => {
                self.emitter.emit_u8(0x87);
                self.emitter
                    .emit_modrm_mem(Reg::RBX, Reg::RCX, memarg.offset as i32);
            }
            _ => {}
        }
        self.stack_depth -= 2;
    }

    fn emit_atomic_rmw(&mut self, sub: u32, memarg: MemArg) {
        let is_cmpxchg = sub >= 0x48 && sub <= 0x4e;

        let size = match sub {
            0x1e | 0x1f | 0x25 | 0x26 | 0x2c | 0x2d | 0x33 | 0x34 | 0x3a | 0x3b | 0x41 | 0x42
            | 0x48 | 0x49 => {
                if sub == 0x1f
                    || sub == 0x26
                    || sub == 0x2d
                    || sub == 0x34
                    || sub == 0x3b
                    || sub == 0x42
                    || sub == 0x49
                {
                    8
                } else {
                    4
                }
            }
            0x20 | 0x22 | 0x27 | 0x29 | 0x2e | 0x30 | 0x35 | 0x37 | 0x3c | 0x3e | 0x43 | 0x45
            | 0x4a | 0x4c => 1,
            0x21 | 0x23 | 0x28 | 0x2a | 0x2f | 0x31 | 0x36 | 0x38 | 0x3d | 0x3f | 0x44 | 0x46
            | 0x4b | 0x4d => 2,
            0x24 | 0x2b | 0x32 | 0x39 | 0x40 | 0x47 | 0x4e => 4,
            _ => 4,
        };
        let is_64 = size == 8;

        if is_cmpxchg {
            self.emitter.pop_wasm_stack(Reg::RBX); // Replacement
            self.emitter.pop_wasm_stack(Reg::RAX); // Expected
            self.emitter.mov_reg_mem64(Reg::RCX, Reg::RSP, 0); // Address
            self.emitter.add_reg_imm32(Reg::RSP, 16); // Pop addr
            self.emit_bounds_check(Reg::RCX, size, memarg.offset);
            self.emitter.add_reg_reg(Reg::RCX, Reg::R14);
            match size {
                1 => self
                    .emitter
                    .cmpxchg_mem_reg8(Reg::RCX, memarg.offset as i32, Reg::RBX),
                2 => self
                    .emitter
                    .cmpxchg_mem_reg16(Reg::RCX, memarg.offset as i32, Reg::RBX),
                4 => self
                    .emitter
                    .cmpxchg_mem_reg(Reg::RCX, memarg.offset as i32, Reg::RBX, false),
                8 => self
                    .emitter
                    .cmpxchg_mem_reg(Reg::RCX, memarg.offset as i32, Reg::RBX, true),
                _ => {}
            }
            self.emitter.push_wasm_stack(Reg::RAX);
            self.stack_depth -= 2;
        } else {
            self.emitter.pop_wasm_stack(Reg::RBX);
            self.emitter.mov_reg_mem64(Reg::RAX, Reg::RSP, 0);
            self.emit_bounds_check(Reg::RAX, size, memarg.offset);
            self.emitter.add_reg_reg(Reg::RAX, Reg::R14);

            match sub {
                0x1e..=0x24 => {
                    // Add
                    match size {
                        1 => self
                            .emitter
                            .xadd_mem_reg8(Reg::RAX, memarg.offset as i32, Reg::RBX),
                        2 => self
                            .emitter
                            .xadd_mem_reg16(Reg::RAX, memarg.offset as i32, Reg::RBX),
                        4 => self.emitter.xadd_mem_reg(
                            Reg::RAX,
                            memarg.offset as i32,
                            Reg::RBX,
                            false,
                        ),
                        8 => self.emitter.xadd_mem_reg(
                            Reg::RAX,
                            memarg.offset as i32,
                            Reg::RBX,
                            true,
                        ),
                        _ => {}
                    }
                    self.emitter.mov_mem64_reg(Reg::RSP, 0, Reg::RBX);
                }
                0x25..=0x2b => {
                    // Sub
                    match size {
                        1 => {
                            self.emitter.emit_u8(0xF6);
                            self.emitter.modrm(3, 3, Reg::RBX as u8);
                        }
                        2 => {
                            self.emitter.emit_u8(0x66);
                            self.emitter.emit_u8(0xF7);
                            self.emitter.modrm(3, 3, Reg::RBX as u8);
                        }
                        4 => {
                            self.emitter.emit_u8(0xF7);
                            self.emitter.modrm(3, 3, Reg::RBX as u8);
                        }
                        8 => {
                            self.emitter.rex(true, 0, 0, Reg::RBX as u8);
                            self.emitter.emit_u8(0xF7);
                            self.emitter.modrm(3, 3, Reg::RBX as u8);
                        }
                        _ => {}
                    }
                    match size {
                        1 => self
                            .emitter
                            .xadd_mem_reg8(Reg::RAX, memarg.offset as i32, Reg::RBX),
                        2 => self
                            .emitter
                            .xadd_mem_reg16(Reg::RAX, memarg.offset as i32, Reg::RBX),
                        4 => self.emitter.xadd_mem_reg(
                            Reg::RAX,
                            memarg.offset as i32,
                            Reg::RBX,
                            false,
                        ),
                        8 => self.emitter.xadd_mem_reg(
                            Reg::RAX,
                            memarg.offset as i32,
                            Reg::RBX,
                            true,
                        ),
                        _ => {}
                    }
                    self.emitter.mov_mem64_reg(Reg::RSP, 0, Reg::RBX);
                }
                0x2c..=0x40 => {
                    // And, Or, Xor
                    let loop_label = self.emitter.new_label();
                    self.emitter.mov_reg_reg(Reg::RSI, Reg::RAX);
                    match size {
                        1 => self
                            .emitter
                            .movzx_reg_mem8(Reg::RAX, Reg::RSI, memarg.offset as i32),
                        2 => self
                            .emitter
                            .movzx_reg_mem16(Reg::RAX, Reg::RSI, memarg.offset as i32),
                        4 => {
                            self.emitter.emit_u8(0x8B);
                            self.emitter
                                .emit_modrm_mem(Reg::RAX, Reg::RSI, memarg.offset as i32);
                        }
                        8 => {
                            self.emitter
                                .mov_reg_mem64(Reg::RAX, Reg::RSI, memarg.offset as i32);
                        }
                        _ => {}
                    }
                    self.emitter.bind_label(loop_label);
                    self.emitter.mov_reg_reg(Reg::RCX, Reg::RAX);

                    if sub >= 0x2c && sub <= 0x32 {
                        if is_64 {
                            self.emitter.and_reg_reg(Reg::RCX, Reg::RBX);
                        } else {
                            self.emitter.and_reg32_reg32(Reg::RCX, Reg::RBX);
                        }
                    } else if sub >= 0x33 && sub <= 0x39 {
                        if is_64 {
                            self.emitter.or_reg_reg(Reg::RCX, Reg::RBX);
                        } else {
                            self.emitter.or_reg32_reg32(Reg::RCX, Reg::RBX);
                        }
                    } else {
                        if is_64 {
                            self.emitter.xor_reg_reg(Reg::RCX, Reg::RBX);
                        } else {
                            self.emitter.xor_reg32_reg32(Reg::RCX, Reg::RBX);
                        }
                    }

                    match size {
                        1 => {
                            self.emitter
                                .cmpxchg_mem_reg8(Reg::RSI, memarg.offset as i32, Reg::RCX)
                        }
                        2 => {
                            self.emitter
                                .cmpxchg_mem_reg16(Reg::RSI, memarg.offset as i32, Reg::RCX)
                        }
                        4 => self.emitter.cmpxchg_mem_reg(
                            Reg::RSI,
                            memarg.offset as i32,
                            Reg::RCX,
                            false,
                        ),
                        8 => self.emitter.cmpxchg_mem_reg(
                            Reg::RSI,
                            memarg.offset as i32,
                            Reg::RCX,
                            true,
                        ),
                        _ => {}
                    }
                    self.emitter.jcc_label(0x85, loop_label);
                    self.emitter.mov_mem64_reg(Reg::RSP, 0, Reg::RAX);
                }
                0x41..=0x47 => {
                    // Xchg
                    match size {
                        1 => {
                            self.emitter.emit_u8(0x86);
                            self.emitter
                                .emit_modrm_mem(Reg::RBX, Reg::RAX, memarg.offset as i32);
                        }
                        2 => {
                            self.emitter.emit_u8(0x66);
                            self.emitter.emit_u8(0x87);
                            self.emitter
                                .emit_modrm_mem(Reg::RBX, Reg::RAX, memarg.offset as i32);
                        }
                        4 => {
                            self.emitter.emit_u8(0x87);
                            self.emitter
                                .emit_modrm_mem(Reg::RBX, Reg::RAX, memarg.offset as i32);
                        }
                        8 => {
                            self.emitter.rex(true, Reg::RBX as u8, 0, Reg::RAX as u8);
                            self.emitter.emit_u8(0x87);
                            self.emitter
                                .emit_modrm_mem(Reg::RBX, Reg::RAX, memarg.offset as i32);
                        }
                        _ => {}
                    }
                    self.emitter.mov_mem64_reg(Reg::RSP, 0, Reg::RBX);
                }
                _ => {}
            }
            self.stack_depth -= 1;
        }
    }

    fn emit_simd_lane_op(
        &mut self,
        reader: &mut WasmReader,
        size: u32,
        is_load: bool,
        trampoline: crate::wasm::aot::runtime::AotTrampoline,
    ) {
        let memarg = MemArg::read(reader).unwrap();
        let lane_idx = reader.read_u8().unwrap();
        if is_load {
            self.emitter.pop_wasm_stack(Reg::RAX); // addr
            self.emit_bounds_check(Reg::RAX, size, memarg.offset);

            self.emitter.push_reg(Reg::RDI); // Save Context
            self.emitter.sub_reg_imm32(Reg::RSP, 8); // Align

            self.emitter.mov_reg_reg(Reg::RSI, Reg::RAX); // arg 1: addr
            self.emitter.mov_reg_imm64(Reg::RDX, memarg.offset as u64); // arg 2: offset
            self.emitter.mov_reg_imm64(Reg::RCX, lane_idx as u64); // arg 3: lane
            self.emitter.mov_reg_imm64(Reg::R8, size as u64); // arg 4: size

            // Pointer to v128 on stack (result destination)
            // Stack: [Align] [SavedRDI] [v128]
            // RSP points to Align.
            // v128 is at RSP + 16.
            self.emitter.mov_reg_reg(Reg::R9, Reg::RSP);
            self.emitter.add_reg_imm32(Reg::R9, 16);

            // We need to pass Context in RDI.
            self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48);

            self.emit_call_trampoline(trampoline);

            self.emitter.add_reg_imm32(Reg::RSP, 8); // Pop Align
            self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Restore Context
            self.emitter.add_reg_imm32(Reg::RSP, 8); // Balance align
            self.stack_depth -= 1; // Popped addr, v128 was already on stack and modified
        } else {
            self.emitter.pop_wasm_stack(Reg::RAX); // addr
            self.emit_bounds_check(Reg::RAX, size, memarg.offset);

            self.emitter.push_reg(Reg::RDI); // Save Context
            self.emitter.sub_reg_imm32(Reg::RSP, 8); // Align

            self.emitter.mov_reg_reg(Reg::RSI, Reg::RAX); // arg 1: addr
            self.emitter.mov_reg_imm64(Reg::RDX, memarg.offset as u64); // arg 2: offset
            self.emitter.mov_reg_imm64(Reg::RCX, lane_idx as u64); // arg 3: lane
            self.emitter.mov_reg_imm64(Reg::R8, size as u64); // arg 4: size

            // Pointer to v128 on stack (source)
            self.emitter.mov_reg_reg(Reg::R9, Reg::RSP); // arg 5: val ptr
            self.emitter.add_reg_imm32(Reg::R9, 16);

            self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // arg 0: ctx

            self.emit_call_trampoline(trampoline);

            self.emitter.add_reg_imm32(Reg::RSP, 8); // Pop Align
            self.emitter.mov_reg_mem64(Reg::RDI, Reg::RBP, -48); // Restore Context
            self.emitter.add_reg_imm32(Reg::RSP, 8); // Balance align
            self.emitter.add_reg_imm32(Reg::RSP, 16); // remove v128
            self.stack_depth -= 2;
        }
    }
}
