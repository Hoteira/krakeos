use crate::rust_alloc::vec::Vec;
use crate::wasm::common::config::Config;
use crate::wasm::common::validation::ValidationInfo;
use crate::wasm::aot::emitter::{X64Emitter, Reg, XmmReg};
use crate::wasm::common::reader::{WasmReader, WasmReadable};
use crate::wasm::common::reader::types::instruction::Instruction;
use crate::wasm::common::reader::types::ValType;
use crate::wasm::common::reader::types::BlockType;
use crate::wasm::common::reader::types::memarg::MemArg;
use crate::wasm::common::reader::types::opcode::*;
use crate::wasm::common::indices::{LocalIdx, GlobalIdx, FuncIdx, LabelIdx};

pub struct AotCompiler<'a, T: Config> {
    pub validation_info: &'a ValidationInfo<'a>,
    pub emitter: X64Emitter,
    pub control_stack: Vec<ControlBlock>,
    pub stack_depth: usize,
    pub func_labels: Vec<usize>,
    pub trap_label: usize,
    _phantom: core::marker::PhantomData<T>,
}

pub struct ControlBlock {
    pub kind: ControlBlockKind,
    pub stack_depth_before: usize,
    pub end_label: usize,
    pub else_label: Option<usize>,
    pub start_label: Option<usize>,
}

pub enum ControlBlockKind {
    Block,
    Loop,
    If,
    Func,
}

impl<'a, T: Config> AotCompiler<'a, T> {
    pub fn new(validation_info: &'a ValidationInfo<'a>) -> Self {
        let mut emitter = X64Emitter::new();
        let mut func_labels = Vec::new();
        let total_funcs = validation_info.imports_length.imported_functions + validation_info.functions.len();
        for _ in 0..total_funcs {
            func_labels.push(emitter.new_label());
        }
        let trap_label = emitter.new_label();
        Self {
            validation_info,
            emitter,
            control_stack: Vec::new(),
            stack_depth: 0,
            func_labels,
            trap_label,
            _phantom: core::marker::PhantomData,
        }
    }

    pub fn compile_module(&mut self) -> crate::wasm::aot::runtime::AotModule {
        let mut func_offsets = Vec::new();
        
        self.emitter.bind_label(self.trap_label);
        self.emitter.emit_u8(0x0F); self.emitter.emit_u8(0x0B);

        for i in 0..self.validation_info.functions.len() {
            let func_idx = self.validation_info.imports_length.imported_functions + i;
            self.emitter.bind_label(self.func_labels[func_idx]);
            func_offsets.push(self.emitter.code.len());
            self.compile_function_body(i);
        }

        self.emitter.finalize();
        crate::wasm::aot::runtime::AotModule::new(&self.emitter.code, func_offsets)
    }

    fn compile_function_body(&mut self, local_func_idx: usize) {
        self.control_stack.clear();
        self.stack_depth = 0;

        let type_idx = self.validation_info.functions[local_func_idx];
        let func_type = &self.validation_info.types[type_idx];

        self.emitter.push_reg(Reg::RBP);
        self.emitter.mov_reg_reg(Reg::RBP, Reg::RSP);
        self.emitter.push_reg(Reg::RBX);
        self.emitter.push_reg(Reg::R12);
        self.emitter.push_reg(Reg::R13);
        self.emitter.push_reg(Reg::R14);
        self.emitter.push_reg(Reg::R15);

        self.emitter.mov_reg_mem64(Reg::R14, Reg::RDI, 16); // memory_base
        self.emitter.mov_reg_mem64(Reg::R15, Reg::RDI, 40); // locals_base

        self.emit_fuel_check(10);

        let end_label = self.emitter.new_label();
        self.control_stack.push(ControlBlock {
            kind: ControlBlockKind::Func,
            stack_depth_before: 0,
            end_label,
            else_label: None,
            start_label: None,
        });

        let (span, _stp) = self.validation_info.func_blocks_stps[local_func_idx];
        let mut reader = WasmReader::new(self.validation_info.wasm);
        reader.pc = span.from;
        let _ = crate::wasm::common::validation::code::read_declared_locals(&mut reader).unwrap();

        while reader.pc < span.from + span.len {
            let instr = Instruction::read(&mut reader).unwrap();
            self.compile_instruction(instr, &mut reader);
        }

        self.emitter.bind_label(end_label);
        self.emitter.mov_reg_reg(Reg::RAX, Reg::RSP);
        self.emitter.pop_reg(Reg::R15);
        self.emitter.pop_reg(Reg::R14);
        self.emitter.pop_reg(Reg::R13);
        self.emitter.pop_reg(Reg::R12);
        self.emitter.pop_reg(Reg::RBX);
        self.emitter.mov_reg_reg(Reg::RSP, Reg::RBP);
        self.emitter.pop_reg(Reg::RBP);
        self.emitter.ret();
    }

    fn emit_fuel_check(&mut self, cost: u32) {
        self.emitter.mov_reg_mem64(Reg::RAX, Reg::RDI, 8);
        self.emitter.emit_u8(0x8B); self.emitter.modrm(0, Reg::RCX as u8, Reg::RAX as u8);
        self.emitter.sub_reg_imm32(Reg::RCX, cost);
        self.emitter.emit_u8(0x89); self.emitter.modrm(0, Reg::RCX as u8, Reg::RAX as u8);
        self.emitter.jcc_label(0x88, self.trap_label);
    }

    fn compile_instruction(&mut self, instr: Instruction, reader: &mut WasmReader) {
        match instr {
            Instruction::Nop => {}
            Instruction::Unreachable => self.emitter.jmp_label(self.trap_label),
            Instruction::Drop => { self.emitter.pop_wasm_stack(Reg::RAX); self.stack_depth -= 1; }
            Instruction::Select => {
                self.emitter.pop_wasm_stack(Reg::RCX);
                self.emitter.pop_wasm_stack(Reg::RBX);
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emitter.test_reg_reg(Reg::RCX, Reg::RCX);
                self.emitter.emit_u8(0x48); self.emitter.emit_u8(0x0F); self.emitter.emit_u8(0x44); self.emitter.emit_u8(0xC3);
                self.emitter.push_wasm_stack(Reg::RAX);
                self.stack_depth -= 2;
            }

            Instruction::LocalGet(idx) => {
                self.emitter.mov_reg_mem64(Reg::RAX, Reg::R15, (idx * 16) as i32);
                self.emitter.mov_reg_mem64(Reg::RDX, Reg::R15, (idx * 16 + 8) as i32);
                self.emitter.push_wasm_stack(Reg::RAX);
                self.emitter.mov_mem64_reg(Reg::RSP, 8, Reg::RDX);
                self.stack_depth += 1;
            }
            Instruction::LocalSet(idx) => {
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emitter.mov_reg_mem64(Reg::RDX, Reg::RSP, -8);
                self.emitter.mov_mem64_reg(Reg::R15, (idx * 16) as i32, Reg::RAX);
                self.emitter.mov_mem64_reg(Reg::R15, (idx * 16 + 8) as i32, Reg::RDX);
                self.stack_depth -= 1;
            }
            Instruction::LocalTee(idx) => {
                self.emitter.mov_reg_mem64(Reg::RAX, Reg::RSP, 0);
                self.emitter.mov_reg_mem64(Reg::RDX, Reg::RSP, 8);
                self.emitter.mov_mem64_reg(Reg::R15, (idx * 16) as i32, Reg::RAX);
                self.emitter.mov_mem64_reg(Reg::R15, (idx * 16 + 8) as i32, Reg::RDX);
            }

            Instruction::GlobalGet(idx) => {
                self.emitter.sub_reg_imm32(Reg::RSP, 16);
                self.emitter.mov_reg_reg(Reg::RDX, Reg::RSP);
                self.emitter.mov_reg_imm64(Reg::RSI, idx as u64);
                self.emitter.mov_reg_reg(Reg::RDI, Reg::RDI);
                self.emitter.mov_reg_imm64(Reg::RAX, crate::wasm::aot::trampoline::aot_global_get as usize as u64);
                self.emitter.call_reg(Reg::RAX);
                self.stack_depth += 1;
            }
            Instruction::GlobalSet(idx) => {
                self.emitter.mov_reg_reg(Reg::RDX, Reg::RSP);
                self.emitter.mov_reg_imm64(Reg::RSI, idx as u64);
                self.emitter.mov_reg_reg(Reg::RDI, Reg::RDI);
                self.emitter.mov_reg_imm64(Reg::RAX, crate::wasm::aot::trampoline::aot_global_set as usize as u64);
                self.emitter.call_reg(Reg::RAX);
                self.emitter.add_reg_imm32(Reg::RSP, 16);
                self.stack_depth -= 1;
            }

            Instruction::I32Const(val) => {
                self.emitter.mov_reg_imm64(Reg::RAX, val as u64);
                self.emitter.push_wasm_stack(Reg::RAX);
                self.stack_depth += 1;
            }
            Instruction::I64Const(val) => {
                self.emitter.mov_reg_imm64(Reg::RAX, val as u64);
                self.emitter.push_wasm_stack(Reg::RAX);
                self.stack_depth += 1;
            }

            Instruction::I32Add => self.emit_binop_i32(|e| e.add_reg_reg(Reg::RAX, Reg::RBX)),
            Instruction::I32Sub => self.emit_binop_i32(|e| e.sub_reg_reg(Reg::RAX, Reg::RBX)),
            Instruction::I32Mul => self.emit_binop_i32(|e| e.imul_reg_reg(Reg::RAX, Reg::RBX)),
            Instruction::I32DivS => self.emit_trampoline_binop(crate::wasm::aot::trampoline::aot_i32_div_s as usize),
            Instruction::I32DivU => self.emit_trampoline_binop(crate::wasm::aot::trampoline::aot_i32_div_u as usize),
            Instruction::I32RemS => self.emit_trampoline_binop(crate::wasm::aot::trampoline::aot_i32_rem_s as usize),
            Instruction::I32RemU => self.emit_trampoline_binop(crate::wasm::aot::trampoline::aot_i32_rem_u as usize),
            
            Instruction::I32Shl => self.emit_shift_i32(|e| e.shl_reg_cl(Reg::RAX)),
            Instruction::I32ShrS => self.emit_shift_i32(|e| e.sar_reg_cl(Reg::RAX)),
            Instruction::I32ShrU => self.emit_shift_i32(|e| e.shr_reg_cl(Reg::RAX)),
            Instruction::I32Rotl => self.emit_shift_i32(|e| e.rol_reg_cl(Reg::RAX)),
            Instruction::I32Rotr => self.emit_shift_i32(|e| e.ror_reg_cl(Reg::RAX)),

            Instruction::I64Shl => self.emit_shift_i64(|e| e.shl_reg_cl(Reg::RAX)),
            Instruction::I64ShrS => self.emit_shift_i64(|e| e.sar_reg_cl(Reg::RAX)),
            Instruction::I64ShrU => self.emit_shift_i64(|e| e.shr_reg_cl(Reg::RAX)),
            Instruction::I64Rotl => self.emit_shift_i64(|e| e.rol_reg_cl(Reg::RAX)),
            Instruction::I64Rotr => self.emit_shift_i64(|e| e.ror_reg_cl(Reg::RAX)),

            Instruction::I64Add => self.emit_binop_i64(|e| e.add_reg_reg(Reg::RAX, Reg::RBX)),
            Instruction::I64Sub => self.emit_binop_i64(|e| e.sub_reg_reg(Reg::RAX, Reg::RBX)),
            Instruction::I64Mul => self.emit_binop_i64(|e| e.imul_reg_reg(Reg::RAX, Reg::RBX)),
            Instruction::I64DivS => self.emit_trampoline_binop(crate::wasm::aot::trampoline::aot_i64_div_s as usize),
            Instruction::I64DivU => self.emit_trampoline_binop(crate::wasm::aot::trampoline::aot_i64_div_u as usize),
            Instruction::I64RemS => self.emit_trampoline_binop(crate::wasm::aot::trampoline::aot_i64_rem_s as usize),
            Instruction::I64RemU => self.emit_trampoline_binop(crate::wasm::aot::trampoline::aot_i64_rem_u as usize),

            Instruction::I32Eqz => {
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emitter.test_reg_reg(Reg::RAX, Reg::RAX);
                self.emitter.emit_u8(0x0F); self.emitter.emit_u8(0x94); self.emitter.emit_u8(0xC0);
                self.emitter.emit_u8(0x48); self.emitter.emit_u8(0x0F); self.emitter.emit_u8(0xB6); self.emitter.emit_u8(0xC0);
                self.emitter.push_wasm_stack(Reg::RAX);
            }
            Instruction::I64Eqz => {
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emitter.test_reg_reg(Reg::RAX, Reg::RAX);
                self.emitter.emit_u8(0x0F); self.emitter.emit_u8(0x94); self.emitter.emit_u8(0xC0);
                self.emitter.emit_u8(0x48); self.emitter.emit_u8(0x0F); self.emitter.emit_u8(0xB6); self.emitter.emit_u8(0xC0);
                self.emitter.push_wasm_stack(Reg::RAX);
            }
            Instruction::I32Clz => self.emit_trampoline_unop(crate::wasm::aot::trampoline::aot_i32_clz as usize),
            Instruction::I32Ctz => self.emit_trampoline_unop(crate::wasm::aot::trampoline::aot_i32_ctz as usize),
            Instruction::I32Popcnt => self.emit_trampoline_unop(crate::wasm::aot::trampoline::aot_i32_popcnt as usize),
            
            Instruction::I64Clz => self.emit_trampoline_unop(crate::wasm::aot::trampoline::aot_i64_clz as usize),
            Instruction::I64Ctz => self.emit_trampoline_unop(crate::wasm::aot::trampoline::aot_i64_ctz as usize),
            Instruction::I64Popcnt => self.emit_trampoline_unop(crate::wasm::aot::trampoline::aot_i64_popcnt as usize),

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

            Instruction::F32Eq => self.emit_relop_f32(0x94, true),
            Instruction::F32Ne => self.emit_relop_f32(0x95, false),
            Instruction::F32Lt => self.emit_relop_f32(0x92, true),
            Instruction::F32Gt => self.emit_relop_f32(0x97, true),
            Instruction::F32Le => self.emit_relop_f32(0x96, true),
            Instruction::F32Ge => self.emit_relop_f32(0x93, true),

            Instruction::F64Eq => self.emit_relop_f64(0x94, true),
            Instruction::F64Ne => self.emit_relop_f64(0x95, false),
            Instruction::F64Lt => self.emit_relop_f64(0x92, true),
            Instruction::F64Gt => self.emit_relop_f64(0x97, true),
            Instruction::F64Le => self.emit_relop_f64(0x96, true),
            Instruction::F64Ge => self.emit_relop_f64(0x93, true),

            Instruction::F32Add => self.emit_binop_f32(|e| e.addss_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1)),
            Instruction::F32Sub => self.emit_binop_f32(|e| e.subss_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1)),
            Instruction::F32Mul => self.emit_binop_f32(|e| e.mulss_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1)),
            Instruction::F32Div => self.emit_binop_f32(|e| e.divss_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1)),

            Instruction::F64Add => self.emit_binop_f64(|e| e.addsd_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1)),
            Instruction::F64Sub => self.emit_binop_f64(|e| e.subsd_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1)),
            Instruction::F64Mul => self.emit_binop_f64(|e| e.mulsd_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1)),
            Instruction::F64Div => self.emit_binop_f64(|e| e.divsd_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1)),

            Instruction::F32Abs => self.emit_trampoline_unop_f32(crate::wasm::aot::trampoline::aot_f32_abs as usize),
            Instruction::F32Neg => self.emit_trampoline_unop_f32(crate::wasm::aot::trampoline::aot_f32_neg as usize),
            Instruction::F32Sqrt => {
                self.emitter.pop_v128(XmmReg::XMM0);
                self.emitter.sqrtss_xmm_xmm(XmmReg::XMM0, XmmReg::XMM0);
                self.emitter.push_v128(XmmReg::XMM0);
            }
            Instruction::F32Ceil => self.emit_trampoline_unop_f32(crate::wasm::aot::trampoline::aot_f32_ceil as usize),
            Instruction::F32Floor => self.emit_trampoline_unop_f32(crate::wasm::aot::trampoline::aot_f32_floor as usize),
            Instruction::F32Trunc => self.emit_trampoline_unop_f32(crate::wasm::aot::trampoline::aot_f32_trunc as usize),
            Instruction::F32Nearest => self.emit_trampoline_unop_f32(crate::wasm::aot::trampoline::aot_f32_nearest as usize),

            Instruction::F64Abs => self.emit_trampoline_unop_f64(crate::wasm::aot::trampoline::aot_f64_abs as usize),
            Instruction::F64Neg => self.emit_trampoline_unop_f64(crate::wasm::aot::trampoline::aot_f64_neg as usize),
            Instruction::F64Sqrt => {
                self.emitter.pop_v128(XmmReg::XMM0);
                self.emitter.sqrtsd_xmm_xmm(XmmReg::XMM0, XmmReg::XMM0);
                self.emitter.push_v128(XmmReg::XMM0);
            }
            Instruction::F64Ceil => self.emit_trampoline_unop_f64(crate::wasm::aot::trampoline::aot_f64_ceil as usize),
            Instruction::F64Floor => self.emit_trampoline_unop_f64(crate::wasm::aot::trampoline::aot_f64_floor as usize),
            Instruction::F64Trunc => self.emit_trampoline_unop_f64(crate::wasm::aot::trampoline::aot_f64_trunc as usize),
            Instruction::F64Nearest => self.emit_trampoline_unop_f64(crate::wasm::aot::trampoline::aot_f64_nearest as usize),

            Instruction::F32Min => self.emit_trampoline_binop_f32(crate::wasm::aot::trampoline::aot_f32_min as usize),
            Instruction::F32Max => self.emit_trampoline_binop_f32(crate::wasm::aot::trampoline::aot_f32_max as usize),
            Instruction::F32Copysign => self.emit_trampoline_binop_f32(crate::wasm::aot::trampoline::aot_f32_copysign as usize),

            Instruction::F64Min => self.emit_trampoline_binop_f64(crate::wasm::aot::trampoline::aot_f64_min as usize),
            Instruction::F64Max => self.emit_trampoline_binop_f64(crate::wasm::aot::trampoline::aot_f64_max as usize),
            Instruction::F64Copysign => self.emit_trampoline_binop_f64(crate::wasm::aot::trampoline::aot_f64_copysign as usize),

            Instruction::I32WrapI64 => {
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emitter.mov_reg_reg(Reg::RAX, Reg::RAX);
                self.emitter.push_wasm_stack(Reg::RAX);
            }
            Instruction::I64ExtendI32S => {
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emitter.movsxd_reg_reg(Reg::RAX, Reg::RAX);
                self.emitter.push_wasm_stack(Reg::RAX);
            }
            Instruction::I64ExtendI32U => {
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emitter.mov_reg_reg(Reg::RAX, Reg::RAX);
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
                self.emitter.movd_xmm_reg(XmmReg::XMM0, Reg::RAX);
                self.emitter.push_v128(XmmReg::XMM0);
            }

            Instruction::F32ConvertI32S => {
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emitter.cvtsi2ss_xmm_reg(XmmReg::XMM0, Reg::RAX);
                self.emitter.push_v128(XmmReg::XMM0);
            }
            Instruction::F64ConvertI32S => {
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emitter.cvtsi2sd_xmm_reg(XmmReg::XMM0, Reg::RAX);
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
                self.emitter.cvttss2si_reg_xmm(Reg::RAX, XmmReg::XMM0);
                self.emitter.push_wasm_stack(Reg::RAX);
            }
            Instruction::I64TruncF64S => {
                self.emitter.pop_v128(XmmReg::XMM0);
                self.emitter.cvttsd2si_reg_xmm(Reg::RAX, XmmReg::XMM0);
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
                self.emitter.emit_u8(0xF3); self.emitter.emit_u8(0x0F); self.emitter.emit_u8(0x10);
                self.emitter.modrm(2, XmmReg::XMM0 as u8, 4);
                self.emitter.emit_u8(0x01);
                self.emitter.emit_u32(memarg.offset);
                self.emitter.push_v128(XmmReg::XMM0);
            }
            Instruction::F64Load(memarg) => {
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emit_bounds_check(Reg::RAX, 8, memarg.offset);
                self.emitter.mov_reg_mem64(Reg::RCX, Reg::RDI, 16);
                self.emitter.emit_u8(0xF2); self.emitter.emit_u8(0x0F); self.emitter.emit_u8(0x10);
                self.emitter.modrm(2, XmmReg::XMM0 as u8, 4);
                self.emitter.emit_u8(0x01);
                self.emitter.emit_u32(memarg.offset);
                self.emitter.push_v128(XmmReg::XMM0);
            }
            Instruction::F32Store(memarg) => {
                self.emitter.pop_v128(XmmReg::XMM0);
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emit_bounds_check(Reg::RAX, 4, memarg.offset);
                self.emitter.mov_reg_mem64(Reg::RCX, Reg::RDI, 16);
                self.emitter.emit_u8(0xF3); self.emitter.emit_u8(0x0F); self.emitter.emit_u8(0x11);
                self.emitter.modrm(2, XmmReg::XMM0 as u8, 4);
                self.emitter.emit_u8(0x01);
                self.emitter.emit_u32(memarg.offset);
                self.stack_depth -= 2;
            }
            Instruction::F64Store(memarg) => {
                self.emitter.pop_v128(XmmReg::XMM0);
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emit_bounds_check(Reg::RAX, 8, memarg.offset);
                self.emitter.mov_reg_mem64(Reg::RCX, Reg::RDI, 16);
                self.emitter.emit_u8(0xF2); self.emitter.emit_u8(0x0F); self.emitter.emit_u8(0x11);
                self.emitter.modrm(2, XmmReg::XMM0 as u8, 4);
                self.emitter.emit_u8(0x01);
                self.emitter.emit_u32(memarg.offset);
                self.stack_depth -= 2;
            }

            Instruction::MemorySize => {
                self.emitter.push_reg(Reg::RDI);
                self.emitter.mov_reg_imm64(Reg::RAX, crate::wasm::aot::trampoline::aot_memory_size as usize as u64);
                self.emitter.call_reg(Reg::RAX);
                self.emitter.pop_reg(Reg::RDI);
                self.emitter.push_wasm_stack(Reg::RAX);
                self.stack_depth += 1;
            }
            Instruction::MemoryGrow => {
                self.emitter.pop_wasm_stack(Reg::RSI);
                self.emitter.push_reg(Reg::RDI);
                self.emitter.mov_reg_imm64(Reg::RAX, crate::wasm::aot::trampoline::aot_memory_grow as usize as u64);
                self.emitter.call_reg(Reg::RAX);
                self.emitter.pop_reg(Reg::RDI);
                self.emitter.push_wasm_stack(Reg::RAX);
            }

            Instruction::TableGet(idx) => {
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emitter.push_reg(Reg::RDI);
                self.emitter.mov_reg_reg(Reg::RSI, Reg::RAX);
                self.emitter.mov_reg_imm64(Reg::RDX, idx as u64);
                self.emitter.mov_reg_imm64(Reg::RAX, crate::wasm::aot::trampoline::aot_table_get as usize as u64);
                self.emitter.call_reg(Reg::RAX);
                self.emitter.pop_reg(Reg::RDI);
                self.emitter.push_wasm_stack(Reg::RAX);
            }
            Instruction::TableSet(idx) => {
                self.emitter.pop_wasm_stack(Reg::RDX);
                self.emitter.pop_wasm_stack(Reg::RSI);
                self.emitter.push_reg(Reg::RDI);
                self.emitter.mov_reg_imm64(Reg::RCX, idx as u64);
                self.emitter.mov_reg_imm64(Reg::RAX, crate::wasm::aot::trampoline::aot_table_set as usize as u64);
                self.emitter.call_reg(Reg::RAX);
                self.emitter.pop_reg(Reg::RDI);
                self.stack_depth -= 1;
            }

            Instruction::Call(idx) => {
                if idx < self.validation_info.imports_length.imported_functions {
                    self.emitter.mov_reg_reg(Reg::RDX, Reg::RSP);
                    self.emitter.mov_reg_imm64(Reg::RSI, idx as u64);
                    self.emitter.mov_reg_reg(Reg::RDI, Reg::RDI);
                    self.emitter.mov_reg_imm64(Reg::RAX, crate::wasm::aot::trampoline::aot_call_host as usize as u64);
                    self.emitter.call_reg(Reg::RAX);
                    self.emitter.mov_reg_reg(Reg::RSP, Reg::RAX);
                } else {
                    let label = self.func_labels[idx];
                    self.emitter.emit_u8(0xE8);
                    let pos = self.emitter.code.len();
                    self.emitter.emit_u32(0);
                    self.emitter.relocs.push(crate::wasm::aot::emitter::Reloc {
                        pos, label_id: label, kind: crate::wasm::aot::emitter::RelocKind::Call32
                    });
                }
            }
            Instruction::CallIndirect(type_idx, table_idx) => {
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emitter.push_reg(Reg::RDI);
                self.emitter.mov_reg_imm64(Reg::RSI, table_idx as u64);
                self.emitter.mov_reg_imm64(Reg::RDX, type_idx as u64);
                self.emitter.mov_reg_reg(Reg::RCX, Reg::RAX);
                
                self.emitter.mov_reg_imm64(Reg::RAX, crate::wasm::aot::trampoline::aot_call_indirect as usize as u64);
                self.emitter.call_reg(Reg::RAX);
                self.emitter.pop_reg(Reg::RDI);
                
                self.emitter.test_reg_reg(Reg::RAX, Reg::RAX);
                self.emitter.jcc_label(0x84, self.trap_label);
                
                self.emitter.call_reg(Reg::RAX);
                self.stack_depth -= 1;
            }

            Instruction::Block(_) => {
                let end_label = self.emitter.new_label();
                self.control_stack.push(ControlBlock {
                    kind: ControlBlockKind::Block,
                    stack_depth_before: self.stack_depth,
                    end_label,
                    else_label: None,
                    start_label: None,
                });
            }
            Instruction::Loop(_) => {
                let start_label = self.emitter.new_label();
                let end_label = self.emitter.new_label();
                self.emitter.bind_label(start_label);
                self.emit_fuel_check(5);
                self.control_stack.push(ControlBlock {
                    kind: ControlBlockKind::Loop,
                    stack_depth_before: self.stack_depth,
                    end_label,
                    else_label: None,
                    start_label: Some(start_label),
                });
            }
            Instruction::Br(label_idx) => {
                let cb = &self.control_stack[self.control_stack.len() - 1 - label_idx as usize];
                self.emitter.jmp_label(cb.start_label.unwrap_or(cb.end_label));
            }
            Instruction::BrIf(label_idx) => {
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emitter.test_reg_reg(Reg::RAX, Reg::RAX);
                let cb = &self.control_stack[self.control_stack.len() - 1 - label_idx as usize];
                self.emitter.jcc_label(0x85, cb.start_label.unwrap_or(cb.end_label));
                self.stack_depth -= 1;
            }
            Instruction::BrTable(targets, default) => {
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emitter.cmp_reg_imm32(Reg::RAX, targets.len() as u32);
                let default_cb = &self.control_stack[self.control_stack.len() - 1 - default as usize];
                let default_target = default_cb.start_label.unwrap_or(default_cb.end_label);
                self.emitter.jcc_label(0x83, default_target);
                
                for (i, target) in targets.iter().enumerate() {
                    self.emitter.cmp_reg_imm32(Reg::RAX, i as u32);
                    let cb = &self.control_stack[self.control_stack.len() - 1 - *target as usize];
                    let t = cb.start_label.unwrap_or(cb.end_label);
                    self.emitter.jcc_label(0x84, t);
                }
                self.emitter.jmp_label(default_target);
                self.stack_depth -= 1;
            }
            Instruction::End => {
                let mut cb = self.control_stack.pop().expect("Control stack underflow");
                if let Some(else_label) = cb.else_label {
                    self.emitter.bind_label(else_label);
                }
                self.emitter.bind_label(cb.end_label);
            }
            
            Instruction::FdExtension(sub) => self.compile_simd(sub, reader),
            Instruction::FcExtension(sub) => self.compile_fc(sub),
            Instruction::Atomic(sub) => self.compile_atomic(sub),

            _ => {
                self.emitter.emit_u8(0x0F); self.emitter.emit_u8(0x0B);
            }
        }
    }

    fn emit_binop_i32<F>(&mut self, op: F) where F: FnOnce(&mut X64Emitter) {
        self.emitter.pop_wasm_stack(Reg::RBX);
        self.emitter.pop_wasm_stack(Reg::RAX);
        op(&mut self.emitter);
        self.emitter.push_wasm_stack(Reg::RAX);
        self.stack_depth -= 1;
    }

    fn emit_binop_i64<F>(&mut self, op: F) where F: FnOnce(&mut X64Emitter) {
        self.emitter.pop_wasm_stack(Reg::RBX);
        self.emitter.pop_wasm_stack(Reg::RAX);
        op(&mut self.emitter);
        self.emitter.push_wasm_stack(Reg::RAX);
        self.stack_depth -= 1;
    }

    fn emit_binop_f32<F>(&mut self, op: F) where F: FnOnce(&mut X64Emitter) {
        self.emitter.pop_v128(XmmReg::XMM1);
        self.emitter.pop_v128(XmmReg::XMM0);
        op(&mut self.emitter);
        self.emitter.push_v128(XmmReg::XMM0);
        self.stack_depth -= 1;
    }

    fn emit_binop_f64<F>(&mut self, op: F) where F: FnOnce(&mut X64Emitter) {
        self.emitter.pop_v128(XmmReg::XMM1);
        self.emitter.pop_v128(XmmReg::XMM0);
        op(&mut self.emitter);
        self.emitter.push_v128(XmmReg::XMM0);
        self.stack_depth -= 1;
    }

    fn emit_shift_i32<F>(&mut self, op: F) where F: FnOnce(&mut X64Emitter) {
        self.emitter.pop_wasm_stack(Reg::RCX);
        self.emitter.pop_wasm_stack(Reg::RAX);
        op(&mut self.emitter);
        self.emitter.push_wasm_stack(Reg::RAX);
        self.stack_depth -= 1;
    }

    fn emit_shift_i64<F>(&mut self, op: F) where F: FnOnce(&mut X64Emitter) {
        self.emitter.pop_wasm_stack(Reg::RCX);
        self.emitter.pop_wasm_stack(Reg::RAX);
        op(&mut self.emitter);
        self.emitter.push_wasm_stack(Reg::RAX);
        self.stack_depth -= 1;
    }

    fn emit_trampoline_unop(&mut self, func_ptr: usize) {
        self.emitter.pop_wasm_stack(Reg::RAX);
        self.emitter.push_reg(Reg::RDI);
        self.emitter.mov_reg_reg(Reg::RDI, Reg::RAX);
        self.emitter.mov_reg_imm64(Reg::RAX, func_ptr as u64);
        self.emitter.call_reg(Reg::RAX);
        self.emitter.pop_reg(Reg::RDI);
        self.emitter.push_wasm_stack(Reg::RAX);
    }

    fn emit_trampoline_unop_f32(&mut self, func_ptr: usize) {
        self.emitter.pop_v128(XmmReg::XMM0);
        self.emitter.push_reg(Reg::RDI);
        self.emitter.mov_reg_imm64(Reg::RAX, func_ptr as u64);
        self.emitter.call_reg(Reg::RAX);
        self.emitter.pop_reg(Reg::RDI);
        self.emitter.push_v128(XmmReg::XMM0);
    }

    fn emit_trampoline_unop_f64(&mut self, func_ptr: usize) {
        self.emitter.pop_v128(XmmReg::XMM0);
        self.emitter.push_reg(Reg::RDI);
        self.emitter.mov_reg_imm64(Reg::RAX, func_ptr as u64);
        self.emitter.call_reg(Reg::RAX);
        self.emitter.pop_reg(Reg::RDI);
        self.emitter.push_v128(XmmReg::XMM0);
    }

    fn emit_trampoline_binop(&mut self, func_ptr: usize) {
        self.emitter.pop_wasm_stack(Reg::RSI);
        self.emitter.pop_wasm_stack(Reg::RAX);
        self.emitter.push_reg(Reg::RDI);
        self.emitter.mov_reg_reg(Reg::RDI, Reg::RAX);
        self.emitter.mov_reg_imm64(Reg::RAX, func_ptr as u64);
        self.emitter.call_reg(Reg::RAX);
        self.emitter.pop_reg(Reg::RDI);
        self.emitter.push_wasm_stack(Reg::RAX);
        self.stack_depth -= 1;
    }

    fn emit_trampoline_binop_f32(&mut self, func_ptr: usize) {
        self.emitter.pop_v128(XmmReg::XMM1);
        self.emitter.pop_v128(XmmReg::XMM0);
        self.emitter.push_reg(Reg::RDI);
        self.emitter.mov_reg_imm64(Reg::RAX, func_ptr as u64);
        self.emitter.call_reg(Reg::RAX);
        self.emitter.pop_reg(Reg::RDI);
        self.emitter.push_v128(XmmReg::XMM0);
        self.stack_depth -= 1;
    }

    fn emit_trampoline_binop_f64(&mut self, func_ptr: usize) {
        self.emitter.pop_v128(XmmReg::XMM1);
        self.emitter.pop_v128(XmmReg::XMM0);
        self.emitter.push_reg(Reg::RDI);
        self.emitter.mov_reg_imm64(Reg::RAX, func_ptr as u64);
        self.emitter.call_reg(Reg::RAX);
        self.emitter.pop_reg(Reg::RDI);
        self.emitter.push_v128(XmmReg::XMM0);
        self.stack_depth -= 1;
    }

    fn emit_relop_i32(&mut self, set_opcode: u8) {
        self.emitter.pop_wasm_stack(Reg::RBX);
        self.emitter.pop_wasm_stack(Reg::RAX);
        self.emitter.cmp_reg_reg(Reg::RAX, Reg::RBX);
        self.emitter.emit_u8(0x0F); self.emitter.emit_u8(set_opcode); self.emitter.emit_u8(0xC0);
        self.emitter.emit_u8(0x48); self.emitter.emit_u8(0x0F); self.emitter.emit_u8(0xB6); self.emitter.emit_u8(0xC0);
        self.emitter.push_wasm_stack(Reg::RAX);
        self.stack_depth -= 1;
    }

    fn emit_relop_i64(&mut self, set_opcode: u8) {
        self.emitter.pop_wasm_stack(Reg::RBX);
        self.emitter.pop_wasm_stack(Reg::RAX);
        self.emitter.cmp_reg_reg(Reg::RAX, Reg::RBX);
        self.emitter.emit_u8(0x0F); self.emitter.emit_u8(set_opcode); self.emitter.emit_u8(0xC0);
        self.emitter.emit_u8(0x48); self.emitter.emit_u8(0x0F); self.emitter.emit_u8(0xB6); self.emitter.emit_u8(0xC0);
        self.emitter.push_wasm_stack(Reg::RAX);
        self.stack_depth -= 1;
    }

    fn emit_relop_f32(&mut self, set_opcode: u8, check_parity: bool) {
        self.emitter.pop_v128(XmmReg::XMM1);
        self.emitter.pop_v128(XmmReg::XMM0);
        self.emitter.ucomiss_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1);
        self.emit_fp_setcc(set_opcode, check_parity);
    }

    fn emit_relop_f64(&mut self, set_opcode: u8, check_parity: bool) {
        self.emitter.pop_v128(XmmReg::XMM1);
        self.emitter.pop_v128(XmmReg::XMM0);
        self.emitter.ucomisd_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1);
        self.emit_fp_setcc(set_opcode, check_parity);
    }

    fn emit_fp_setcc(&mut self, set_opcode: u8, check_parity: bool) {
        if check_parity {
            self.emitter.emit_u8(0x0F); self.emitter.emit_u8(set_opcode); self.emitter.emit_u8(0xC0);
            self.emitter.emit_u8(0x0F); self.emitter.emit_u8(0x9B); self.emitter.emit_u8(0xC2);
            self.emitter.and_reg_reg(Reg::RAX, Reg::RDX);
        } else {
            self.emitter.emit_u8(0x0F); self.emitter.emit_u8(set_opcode); self.emitter.emit_u8(0xC0);
            self.emitter.emit_u8(0x0F); self.emitter.emit_u8(0x9A); self.emitter.emit_u8(0xC2);
            self.emitter.or_reg_reg(Reg::RAX, Reg::RDX);
        }
        self.emitter.emit_u8(0x48); self.emitter.emit_u8(0x0F); self.emitter.emit_u8(0xB6); self.emitter.emit_u8(0xC0);
        self.emitter.push_wasm_stack(Reg::RAX);
        self.stack_depth -= 1;
    }

    fn emit_bounds_check(&mut self, addr_reg: Reg, size: u32, offset: u32) {
        // Effective address = addr_reg + offset
        // We need to check if Effective address + size <= memory_size
        // memory_size is at [RDI + 24]
        self.emitter.mov_reg_reg(Reg::R11, addr_reg);
        if offset != 0 {
            self.emitter.add_reg_imm32(Reg::R11, offset);
        }
        self.emitter.add_reg_imm32(Reg::R11, size);
        
        self.emitter.mov_reg_mem64(Reg::R10, Reg::RDI, 24); // memory_size
        self.emitter.cmp_reg_reg(Reg::R11, Reg::R10);
        self.emitter.jcc_label(0x87, self.trap_label); // ja (above) -> trap if R11 > memory_size
    }

    fn emit_load_i32(&mut self, memarg: MemArg) {
        self.emitter.pop_wasm_stack(Reg::RAX);
        self.emit_bounds_check(Reg::RAX, 4, memarg.offset);
        self.emitter.mov_reg_mem64(Reg::RCX, Reg::RDI, 16);
        self.emitter.emit_u8(0x8B); 
        self.emitter.modrm(2, Reg::RAX as u8, 4);
        self.emitter.emit_u8(0x01);
        self.emitter.emit_u32(memarg.offset);
        self.emitter.push_wasm_stack(Reg::RAX);
    }

    fn emit_load_i64(&mut self, memarg: MemArg) {
        self.emitter.pop_wasm_stack(Reg::RAX);
        self.emit_bounds_check(Reg::RAX, 8, memarg.offset);
        self.emitter.mov_reg_mem64(Reg::RCX, Reg::RDI, 16);
        self.emitter.emit_u8(0x48); self.emitter.emit_u8(0x8B); 
        self.emitter.modrm(2, Reg::RAX as u8, 4);
        self.emitter.emit_u8(0x01);
        self.emitter.emit_u32(memarg.offset);
        self.emitter.push_wasm_stack(Reg::RAX);
    }

    fn emit_load_extend(&mut self, memarg: MemArg, size: usize, signed: bool, is_i64: bool) {
        self.emitter.pop_wasm_stack(Reg::RAX);
        self.emit_bounds_check(Reg::RAX, size as u32, memarg.offset);
        self.emitter.mov_reg_mem64(Reg::RCX, Reg::RDI, 16);
        self.emitter.emit_u8(if is_i64 { 0x48 } else { 0x40 });
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
        self.emitter.modrm(2, Reg::RAX as u8, 4);
        self.emitter.emit_u8(0x01);
        self.emitter.emit_u32(memarg.offset);
        self.emitter.push_wasm_stack(Reg::RAX);
    }

    fn emit_store_i32(&mut self, memarg: MemArg) {
        self.emitter.pop_wasm_stack(Reg::RBX);
        self.emitter.pop_wasm_stack(Reg::RAX);
        self.emit_bounds_check(Reg::RAX, 4, memarg.offset);
        self.emitter.mov_reg_mem64(Reg::RCX, Reg::RDI, 16);
        self.emitter.emit_u8(0x89);
        self.emitter.modrm(2, Reg::RBX as u8, 4);
        self.emitter.emit_u8(0x01);
        self.emitter.emit_u32(memarg.offset);
        self.stack_depth -= 2;
    }

    fn emit_store_i64(&mut self, memarg: MemArg) {
        self.emitter.pop_wasm_stack(Reg::RBX);
        self.emitter.pop_wasm_stack(Reg::RAX);
        self.emit_bounds_check(Reg::RAX, 8, memarg.offset);
        self.emitter.mov_reg_mem64(Reg::RCX, Reg::RDI, 16);
        self.emitter.emit_u8(0x48); self.emitter.emit_u8(0x89);
        self.emitter.modrm(2, Reg::RBX as u8, 4);
        self.emitter.emit_u8(0x01);
        self.emitter.emit_u32(memarg.offset);
        self.stack_depth -= 2;
    }

    fn emit_store_shrink(&mut self, memarg: MemArg, size: usize) {
        self.emitter.pop_wasm_stack(Reg::RBX);
        self.emitter.pop_wasm_stack(Reg::RAX);
        self.emit_bounds_check(Reg::RAX, size as u32, memarg.offset);
        self.emitter.mov_reg_mem64(Reg::RCX, Reg::RDI, 16);
        match size {
            1 => { self.emitter.emit_u8(0x88); }
            2 => { self.emitter.emit_u8(0x66); self.emitter.emit_u8(0x89); }
            4 => { self.emitter.emit_u8(0x89); }
            _ => { self.emitter.emit_u8(0x89); }
        }
        self.emitter.modrm(2, Reg::RBX as u8, 4);
        self.emitter.emit_u8(0x01);
        self.emitter.emit_u32(memarg.offset);
        self.stack_depth -= 2;
    }

    fn compile_simd(&mut self, sub: u32, reader: &mut WasmReader) {
        use crate::wasm::common::reader::types::opcode::fd_extensions::*;
        match sub {
            V128_LOAD => {
                let memarg = MemArg::read(reader).unwrap();
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emit_bounds_check(Reg::RAX, 16, memarg.offset);
                self.emitter.mov_reg_mem64(Reg::RCX, Reg::RDI, 16); // memory_base
                self.emitter.movups_xmm_mem(XmmReg::XMM0, Reg::RAX, memarg.offset as i32 + 0); // Wait, base is RAX, offset is memarg.offset.
                // Wait, movups_xmm_mem takes (dst, base, offset). 
                // But base should be memory_base + RAX + memarg.offset.
                // My emitter's movups_xmm_mem does [base + offset].
                // So I need RCX = memory_base + RAX.
                self.emitter.add_reg_reg(Reg::RCX, Reg::RAX);
                self.emitter.movups_xmm_mem(XmmReg::XMM0, Reg::RCX, memarg.offset as i32);
                self.emitter.push_v128(XmmReg::XMM0);
            }
            V128_STORE => {
                let memarg = MemArg::read(reader).unwrap();
                self.emitter.pop_v128(XmmReg::XMM0);
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emit_bounds_check(Reg::RAX, 16, memarg.offset);
                self.emitter.mov_reg_mem64(Reg::RCX, Reg::RDI, 16);
                self.emitter.add_reg_reg(Reg::RCX, Reg::RAX);
                self.emitter.movups_mem_xmm(Reg::RCX, memarg.offset as i32, XmmReg::XMM0);
                self.stack_depth -= 2;
            }
            V128_CONST => {
                let mut data = [0u8; 16];
                for i in 0..16 { data[i] = reader.read_u8().unwrap(); }
                let low = u64::from_le_bytes(data[0..8].try_into().unwrap());
                let high = u64::from_le_bytes(data[8..16].try_into().unwrap());
                self.emitter.mov_reg_imm64(Reg::RAX, low);
                self.emitter.mov_reg_imm64(Reg::RDX, high);
                self.emitter.sub_reg_imm32(Reg::RSP, 16);
                self.emitter.mov_mem64_reg(Reg::RSP, 0, Reg::RAX);
                self.emitter.mov_mem64_reg(Reg::RSP, 8, Reg::RDX);
                self.stack_depth += 1;
            }
            I8X16_ADD => self.emit_simd_padd(0xFC),
            I16X8_ADD => self.emit_simd_padd(0xFD),
            I32X4_ADD => self.emit_simd_padd(0xFE),
            I64X2_ADD => self.emit_simd_padd(0xD4),
            I8X16_SUB => self.emit_simd_padd(0xF8),
            I16X8_SUB => self.emit_simd_padd(0xF9),
            I32X4_SUB => self.emit_simd_padd(0xFA),
            I64X2_SUB => self.emit_simd_padd(0xFB),
            
            V128_AND => self.emit_simd_trampoline_binop(crate::wasm::aot::trampoline::aot_v128_and as usize),
            V128_OR  => self.emit_simd_trampoline_binop(crate::wasm::aot::trampoline::aot_v128_or as usize),
            V128_XOR => self.emit_simd_trampoline_binop(crate::wasm::aot::trampoline::aot_v128_xor as usize),
            
            I32X4_SPLAT => {
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emitter.movd_xmm_reg(XmmReg::XMM0, Reg::RAX);
                self.emitter.emit_u8(0x66); self.emitter.emit_u8(0x0F); self.emitter.emit_u8(0x70); self.emitter.modrm(3, 0, 0); self.emitter.emit_u8(0);
                self.emitter.push_v128(XmmReg::XMM0);
            }
            F32X4_SPLAT => {
                self.emitter.pop_v128(XmmReg::XMM0);
                self.emitter.emit_u8(0x0F); self.emitter.emit_u8(0xC6); self.emitter.modrm(3, 0, 0); self.emitter.emit_u8(0);
                self.emitter.push_v128(XmmReg::XMM0);
            }

            V128_BITSELECT => self.emit_simd_trampoline_ternary(crate::wasm::aot::trampoline::aot_v128_bitselect as usize),
            
            I8X16_EQ => self.emit_simd_trampoline_binop(crate::wasm::aot::trampoline::aot_v128_eq_i8x16 as usize),
            I16X8_EQ => self.emit_simd_trampoline_binop(crate::wasm::aot::trampoline::aot_v128_eq_i16x8 as usize),
            I32X4_EQ => self.emit_simd_trampoline_binop(crate::wasm::aot::trampoline::aot_v128_eq_i32x4 as usize),
            I64X2_EQ => self.emit_simd_trampoline_binop(crate::wasm::aot::trampoline::aot_v128_eq_i64x2 as usize),
            F32X4_EQ => self.emit_simd_trampoline_binop(crate::wasm::aot::trampoline::aot_v128_eq_f32x4 as usize),
            F64X2_EQ => self.emit_simd_trampoline_binop(crate::wasm::aot::trampoline::aot_v128_eq_f64x2 as usize),

            V128_ANY_TRUE => self.emit_simd_trampoline_reduction(crate::wasm::aot::trampoline::aot_v128_any_true as usize),
            I8X16_BITMASK => self.emit_simd_trampoline_reduction(crate::wasm::aot::trampoline::aot_v128_bitmask_i8x16 as usize),

            V128_NOT => {
                self.emitter.pop_v128(XmmReg::XMM0);
                self.emitter.pcmpeqd_xmm_xmm(XmmReg::XMM1, XmmReg::XMM1);
                self.emitter.pandn_xmm_xmm(XmmReg::XMM0, XmmReg::XMM1);
                self.emitter.push_v128(XmmReg::XMM0);
            }

            I32X4_EXTRACT_LANE => {
                let lane = reader.read_u8().unwrap();
                self.emitter.pop_v128(XmmReg::XMM0);
                self.emitter.pextrd_reg_xmm_imm8(Reg::RAX, XmmReg::XMM0, lane);
                self.emitter.push_wasm_stack(Reg::RAX);
            }

            I32X4_REPLACE_LANE => {
                let lane = reader.read_u8().unwrap();
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emitter.pop_v128(XmmReg::XMM0);
                self.emitter.pinsrd_xmm_reg_imm8(XmmReg::XMM0, Reg::RAX, lane);
                self.emitter.push_v128(XmmReg::XMM0);
                self.stack_depth -= 1;
            }

            _ => {
                self.emitter.jmp_label(self.trap_label);
            }
        }
    }

    fn emit_simd_padd(&mut self, opcode: u8) {
        self.emitter.pop_v128(XmmReg::XMM1);
        self.emitter.pop_v128(XmmReg::XMM0);
        self.emitter.emit_u8(0x66); self.emitter.emit_u8(0x0F); self.emitter.emit_u8(opcode);
        self.emitter.modrm(3, 0, 1);
        self.emitter.push_v128(XmmReg::XMM0);
        self.stack_depth -= 1;
    }

    fn emit_simd_trampoline_binop(&mut self, func_ptr: usize) {
        self.emitter.mov_reg_reg(Reg::RSI, Reg::RSP);
        self.emitter.mov_reg_reg(Reg::RDI, Reg::RSP);
        self.emitter.add_reg_imm32(Reg::RDI, 16);
        self.emitter.push_reg(Reg::RDI);
        self.emitter.mov_reg_imm64(Reg::RAX, func_ptr as u64);
        self.emitter.call_reg(Reg::RAX);
        self.emitter.pop_reg(Reg::RDI);
        self.emitter.add_reg_imm32(Reg::RSP, 16);
        self.stack_depth -= 1;
    }

    fn emit_simd_trampoline_ternary(&mut self, func_ptr: usize) {
        self.emitter.mov_reg_reg(Reg::RDX, Reg::RSP);
        self.emitter.mov_reg_reg(Reg::RSI, Reg::RSP);
        self.emitter.add_reg_imm32(Reg::RSI, 16);
        self.emitter.mov_reg_reg(Reg::RDI, Reg::RSP);
        self.emitter.add_reg_imm32(Reg::RDI, 32);
        self.emitter.push_reg(Reg::RDI);
        self.emitter.mov_reg_imm64(Reg::RAX, func_ptr as u64);
        self.emitter.call_reg(Reg::RAX);
        self.emitter.pop_reg(Reg::RDI);
        self.emitter.add_reg_imm32(Reg::RSP, 32);
        self.stack_depth -= 2;
    }

    fn emit_simd_trampoline_reduction(&mut self, func_ptr: usize) {
        self.emitter.mov_reg_reg(Reg::RDI, Reg::RSP);
        self.emitter.push_reg(Reg::RDI);
        self.emitter.mov_reg_imm64(Reg::RAX, func_ptr as u64);
        self.emitter.call_reg(Reg::RAX);
        self.emitter.pop_reg(Reg::RDI);
        self.emitter.add_reg_imm32(Reg::RSP, 16);
        self.emitter.push_wasm_stack(Reg::RAX);
    }

    fn compile_fc(&mut self, sub: u32) {
        match sub {
            0x00 => self.emit_trampoline_unop_f32(crate::wasm::aot::trampoline::aot_i32_trunc_sat_f32_s as usize),
            0x01 => self.emit_trampoline_unop_f32(crate::wasm::aot::trampoline::aot_i32_trunc_sat_f32_u as usize),
            _ => {
                self.emitter.jmp_label(self.trap_label);
            }
        }
    }

    fn compile_atomic(&mut self, sub: u32) {
        match sub {
            0x10 => {
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emitter.mov_reg_mem64(Reg::RCX, Reg::RDI, 16);
                self.emitter.emit_u8(0x8B); self.emitter.modrm(0, 0, 4); self.emitter.emit_u8(0x01);
                self.emitter.push_wasm_stack(Reg::RAX);
            }
            0x17 => {
                self.emitter.pop_wasm_stack(Reg::RBX);
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emitter.mov_reg_mem64(Reg::RCX, Reg::RDI, 16);
                self.emitter.emit_u8(0x87); self.emitter.modrm(0, 3, 4); self.emitter.emit_u8(0x01);
                self.stack_depth -= 2;
            }
            0x1e => {
                self.emitter.pop_wasm_stack(Reg::RBX);
                self.emitter.pop_wasm_stack(Reg::RAX);
                self.emitter.mov_reg_mem64(Reg::RCX, Reg::RDI, 16);
                self.emitter.emit_u8(0xF0); self.emitter.emit_u8(0x0F); self.emitter.emit_u8(0xC1); self.emitter.modrm(0, 3, 4); self.emitter.emit_u8(0x01);
                self.emitter.push_wasm_stack(Reg::RBX);
            }
            _ => {
                self.emitter.jmp_label(self.trap_label);
            }
        }
    }
}
