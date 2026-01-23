use super::emitter::{Assembler, Reg};
use super::memory::ExecutableBuffer;
use crate::rust_alloc::vec::Vec;
use crate::rust_alloc::collections::BTreeMap;
use crate::wasm::core::reader::{WasmReader, WasmReadable};
use crate::wasm::core::reader::types::{opcode, BlockType, memarg::MemArg, FuncType};
use crate::wasm::execution::store::instances::{WasmFuncInst, ModuleInst};
use crate::wasm::execution::store::addrs::FuncAddr;
use crate::wasm::execution::config::Config;
use crate::wasm::core::error::ValidationError;

struct ControlBlock {
    kind: u8, // 0=Block, 1=Loop, 2=If
    // Label for BR to target (start of loop, or end of block/if)
    label_offset: Option<usize>, 
    // Patches for BRs targeting this block (to be patched at end)
    br_patches: Vec<usize>,
    // Patches for ELSE/END of IF
    else_patch: Option<usize>,
    stack_height: usize, // To check stack balance (simplified)
}

pub fn compile_function<T: Config>(
    func: &WasmFuncInst,
    module: &ModuleInst,
    module_addr: usize,
    func_types: &BTreeMap<FuncAddr, FuncType>,
) -> Result<ExecutableBuffer, ValidationError> {
    let mut asm = Assembler::new();
    let mut wasm = WasmReader::new(module.wasm_bytecode);
    
    // Move to function body
    wasm.move_start_to(func.code_expr).map_err(|_| ValidationError::Eof)?;

    // Prologue
    asm.push_reg(Reg::RBP);
    asm.mov_reg_reg(Reg::RBP, Reg::RSP);

    // Save R15 (Mem Base), R14 (Store/VM Context)
    asm.push_reg(Reg::R15);
    asm.push_reg(Reg::R14);
    
    // Move RCX (arg 4: Mem Base) to R15
    asm.mov_reg_reg(Reg::R15, Reg::RCX);
    // Move RDI (arg 1: Store/VM Context) to R14
    asm.mov_reg_reg(Reg::R14, Reg::RDI);

    let param_count = func.function_type.params.valtypes.len();
    
    // Load params from RSI (arg 2) array
    for i in 0..param_count {
        asm.mov_reg_mem(Reg::RAX, Reg::RSI, (i * 8) as i32);
        asm.push_reg(Reg::RAX);
    }

    // Initialize locals (zero)
    for _ in 0..func.locals.len() {
        asm.mov_reg_imm64(Reg::RAX, 0);
        asm.push_reg(Reg::RAX);
    }

    let mut ctrl_stack: Vec<ControlBlock> = Vec::new();
    // Implicit function block
    ctrl_stack.push(ControlBlock { 
        kind: 0, 
        label_offset: None, 
        br_patches: Vec::new(), 
        else_patch: None,
        stack_height: 0 
    });

    loop {
        let opcode = wasm.read_u8().map_err(|_| ValidationError::Eof)?;
        match opcode {
            opcode::END => {
                let block = ctrl_stack.pop().unwrap();
                let current_off = asm.current_offset();
                
                for patch in block.br_patches {
                    let rel = (current_off as isize) - (patch as isize) - 4;
                    asm.patch_i32(patch, rel as i32);
                }

                if block.kind == 2 {
                    if let Some(else_p) = block.else_patch {
                        let rel = (current_off as isize) - (else_p as isize) - 4;
                        asm.patch_i32(else_p, rel as i32);
                    }
                }

                if ctrl_stack.is_empty() {
                    break;
                }
            }
            opcode::RETURN => {
                let target_idx = 0;
                let target = &mut ctrl_stack[target_idx];
                let patch = asm.current_offset() + 1;
                asm.jmp_rel32(0);
                target.br_patches.push(patch);
            }
            opcode::SELECT => {
                asm.pop_reg(Reg::RCX);
                asm.pop_reg(Reg::RBX);
                asm.pop_reg(Reg::RAX);
                asm.mov_reg_imm64(Reg::RDX, 0);
                asm.cmp_reg_reg(Reg::RCX, Reg::RDX);
                asm.cmov_reg_reg(0x4, Reg::RAX, Reg::RBX);
                asm.push_reg(Reg::RAX);
            }
            opcode::GLOBAL_GET => {
                let global_idx = wasm.read_var_u32()?;
                asm.mov_reg_reg(Reg::RDI, Reg::R14);
                asm.mov_reg_imm64(Reg::RSI, global_idx as u64);
                asm.mov_reg_imm64(Reg::RDX, module_addr as u64);
                let trampoline = crate::wasm::aot::trampoline::aot_global_get::<T> as usize;
                asm.mov_reg_imm64(Reg::RAX, trampoline as u64);
                asm.call_reg(Reg::RAX);
                asm.push_reg(Reg::RAX);
            }
            opcode::GLOBAL_SET => {
                let global_idx = wasm.read_var_u32()?;
                asm.pop_reg(Reg::RDX);
                asm.mov_reg_reg(Reg::RDI, Reg::R14);
                asm.mov_reg_imm64(Reg::RSI, global_idx as u64);
                asm.mov_reg_imm64(Reg::RCX, module_addr as u64);
                let trampoline = crate::wasm::aot::trampoline::aot_global_set::<T> as usize;
                asm.mov_reg_imm64(Reg::RAX, trampoline as u64);
                asm.call_reg(Reg::RAX);
            }
            opcode::MEMORY_SIZE => {
                let mem_idx = wasm.read_u8()?;
                asm.mov_reg_reg(Reg::RDI, Reg::R14);
                asm.mov_reg_imm64(Reg::RSI, mem_idx as u64);
                asm.mov_reg_imm64(Reg::RDX, module_addr as u64);
                let trampoline = crate::wasm::aot::trampoline::aot_memory_size::<T> as usize;
                asm.mov_reg_imm64(Reg::RAX, trampoline as u64);
                asm.call_reg(Reg::RAX);
                asm.push_reg(Reg::RAX);
            }
            opcode::MEMORY_GROW => {
                let mem_idx = wasm.read_u8()?;
                asm.pop_reg(Reg::RDX);
                asm.mov_reg_reg(Reg::RDI, Reg::R14);
                asm.mov_reg_imm64(Reg::RSI, mem_idx as u64);
                asm.mov_reg_imm64(Reg::RCX, module_addr as u64);
                let trampoline = crate::wasm::aot::trampoline::aot_memory_grow::<T> as usize;
                asm.mov_reg_imm64(Reg::RAX, trampoline as u64);
                asm.call_reg(Reg::RAX);
                asm.push_reg(Reg::RAX);
            }
            
            opcode::F32_CONST => {
                let val = wasm.read_f32()?;
                asm.mov_reg_imm64(Reg::RAX, val as u64);
                asm.push_reg(Reg::RAX);
            }
            opcode::F64_CONST => {
                let val = wasm.read_f64()?;
                asm.mov_reg_imm64(Reg::RAX, val);
                asm.push_reg(Reg::RAX);
            }
            opcode::F32_ADD | opcode::F64_ADD => {
                asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX);
                asm.movq_xmm_reg(1, Reg::RCX);
                asm.movq_xmm_reg(0, Reg::RAX);
                if opcode == opcode::F32_ADD { asm.addss_xmm_xmm(0, 1); } else { asm.addsd_xmm_xmm(0, 1); }
                asm.movq_reg_xmm(Reg::RAX, 0);
                asm.push_reg(Reg::RAX);
            }
            opcode::F32_SUB | opcode::F64_SUB => {
                asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX);
                asm.movq_xmm_reg(1, Reg::RCX);
                asm.movq_xmm_reg(0, Reg::RAX);
                if opcode == opcode::F32_SUB { asm.subss_xmm_xmm(0, 1); } else { asm.subsd_xmm_xmm(0, 1); }
                asm.movq_reg_xmm(Reg::RAX, 0);
                asm.push_reg(Reg::RAX);
            }
            opcode::F32_MUL | opcode::F64_MUL => {
                asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX);
                asm.movq_xmm_reg(1, Reg::RCX);
                asm.movq_xmm_reg(0, Reg::RAX);
                if opcode == opcode::F32_MUL { asm.mulss_xmm_xmm(0, 1); } else { asm.mulsd_xmm_xmm(0, 1); }
                asm.movq_reg_xmm(Reg::RAX, 0);
                asm.push_reg(Reg::RAX);
            }
            opcode::F32_DIV | opcode::F64_DIV => {
                asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX);
                asm.movq_xmm_reg(1, Reg::RCX);
                asm.movq_xmm_reg(0, Reg::RAX);
                if opcode == opcode::F32_DIV { asm.divss_xmm_xmm(0, 1); } else { asm.divsd_xmm_xmm(0, 1); }
                asm.movq_reg_xmm(Reg::RAX, 0);
                asm.push_reg(Reg::RAX);
            }
            
            opcode::CALL_INDIRECT => {
                let type_idx = wasm.read_var_u32()?;
                let table_idx = wasm.read_var_u32()?;
                let func_ty = module.types[type_idx as usize].clone();
                let n_args = func_ty.params.valtypes.len();
                let n_results = func_ty.returns.valtypes.len();
                asm.pop_reg(Reg::RCX); 
                for i in 0..n_args {
                    asm.pop_reg(Reg::RAX);
                    asm.mov_mem_reg(Reg::RSP, -8 * ((i as i32) + 1), Reg::RAX);
                }
                if n_args > 0 {
                    asm.mov_reg_imm64(Reg::RAX, (n_args * 8) as u64);
                    asm.sub_reg_reg(Reg::RSP, Reg::RAX);
                }
                if n_results > 0 {
                    asm.mov_reg_imm64(Reg::RAX, (n_results * 8) as u64);
                    asm.sub_reg_reg(Reg::RSP, Reg::RAX);
                }
                asm.mov_reg_reg(Reg::RDI, Reg::R14);
                asm.mov_reg_imm64(Reg::RSI, type_idx as u64);
                asm.mov_reg_imm64(Reg::RDX, table_idx as u64);
                asm.mov_reg_reg(Reg::R8, Reg::RSP);
                if n_results > 0 {
                    asm.mov_reg_imm64(Reg::RAX, (n_results * 8) as u64);
                    asm.add_reg_reg(Reg::R8, Reg::RAX);
                }
                asm.mov_reg_reg(Reg::R9, Reg::RSP);
                asm.mov_reg_imm64(Reg::RAX, module_addr as u64);
                asm.push_reg(Reg::RAX);
                let trampoline = crate::wasm::aot::trampoline::aot_call_indirect::<T> as usize;
                asm.mov_reg_imm64(Reg::RAX, trampoline as u64);
                asm.call_reg(Reg::RAX);
                asm.pop_reg(Reg::RAX); 
                
                // RELOAD MEMORY BASE (R15)
                asm.mov_reg_reg(Reg::RDI, Reg::R14);
                asm.mov_reg_imm64(Reg::RSI, module_addr as u64);
                let get_mem_base = crate::wasm::aot::trampoline::aot_get_mem_base::<T> as usize;
                asm.mov_reg_imm64(Reg::RAX, get_mem_base as u64);
                asm.call_reg(Reg::RAX);
                asm.mov_reg_reg(Reg::R15, Reg::RAX);

                for i in 0..n_results {
                    asm.mov_reg_mem(Reg::RAX, Reg::RSP, (i * 8) as i32);
                    asm.mov_mem_reg(Reg::RSP, ((i * 8) + (n_args * 8)) as i32, Reg::RAX);
                }
                if n_args > 0 {
                    asm.mov_reg_imm64(Reg::RAX, (n_args * 8) as u64);
                    asm.add_reg_reg(Reg::RSP, Reg::RAX);
                }
            }
            opcode::CALL => {
                let func_idx = wasm.read_var_u32()?;
                let func_addr = module.func_addrs[func_idx as usize];
                let func_ty = func_types.get(&func_addr).expect("FuncType not found");
                let n_args = func_ty.params.valtypes.len();
                let n_results = func_ty.returns.valtypes.len();
                for i in 0..n_args {
                    asm.pop_reg(Reg::RAX);
                    asm.mov_mem_reg(Reg::RSP, -8 * ((i as i32) + 1), Reg::RAX);
                }
                if n_args > 0 {
                    asm.mov_reg_imm64(Reg::RAX, (n_args * 8) as u64);
                    asm.sub_reg_reg(Reg::RSP, Reg::RAX);
                }
                if n_results > 0 {
                    asm.mov_reg_imm64(Reg::RAX, (n_results * 8) as u64);
                    asm.sub_reg_reg(Reg::RSP, Reg::RAX);
                }
                asm.mov_reg_reg(Reg::RDI, Reg::R14);
                asm.mov_reg_imm64(Reg::RSI, func_addr as u64);
                asm.mov_reg_reg(Reg::RCX, Reg::RSP);
                asm.mov_reg_reg(Reg::RDX, Reg::RSP);
                if n_results > 0 {
                    asm.mov_reg_imm64(Reg::RAX, (n_results * 8) as u64);
                    asm.add_reg_reg(Reg::RDX, Reg::RAX);
                }
                let trampoline = crate::wasm::aot::trampoline::aot_invoke_trampoline::<T> as usize;
                asm.mov_reg_imm64(Reg::RAX, trampoline as u64);
                asm.call_reg(Reg::RAX);

                // RELOAD MEMORY BASE (R15)
                asm.mov_reg_reg(Reg::RDI, Reg::R14);
                asm.mov_reg_imm64(Reg::RSI, module_addr as u64);
                let get_mem_base = crate::wasm::aot::trampoline::aot_get_mem_base::<T> as usize;
                asm.mov_reg_imm64(Reg::RAX, get_mem_base as u64);
                asm.call_reg(Reg::RAX);
                asm.mov_reg_reg(Reg::R15, Reg::RAX);

                for i in 0..n_results {
                    asm.mov_reg_mem(Reg::RAX, Reg::RSP, (i * 8) as i32);
                    asm.mov_mem_reg(Reg::RSP, ((i * 8) + (n_args * 8)) as i32, Reg::RAX);
                }
                if n_args > 0 {
                    asm.mov_reg_imm64(Reg::RAX, (n_args * 8) as u64);
                    asm.add_reg_reg(Reg::RSP, Reg::RAX);
                }
            }
            opcode::BLOCK => {
                let _ty = BlockType::read(&mut wasm)?;
                ctrl_stack.push(ControlBlock { kind: 0, label_offset: None, br_patches: Vec::new(), else_patch: None, stack_height: 0, });
            }
            opcode::LOOP => {
                let _ty = BlockType::read(&mut wasm)?;
                let start_off = asm.current_offset();
                ctrl_stack.push(ControlBlock { kind: 1, label_offset: Some(start_off), br_patches: Vec::new(), else_patch: None, stack_height: 0, });
            }
            opcode::IF => {
                let _ty = BlockType::read(&mut wasm)?;
                asm.pop_reg(Reg::RAX);
                asm.mov_reg_imm64(Reg::RCX, 0);
                asm.cmp_reg_reg(Reg::RAX, Reg::RCX);
                let patch_off = asm.current_offset() + 2;
                asm.jcc_rel32(0x4, 0); 
                ctrl_stack.push(ControlBlock { kind: 2, label_offset: None, br_patches: Vec::new(), else_patch: Some(patch_off), stack_height: 0, });
            }
            opcode::ELSE => {
                let block = ctrl_stack.last_mut().unwrap();
                let jmp_patch = asm.current_offset() + 1;
                asm.jmp_rel32(0);
                let current_off = asm.current_offset();
                if let Some(else_p) = block.else_patch {
                    let rel = (current_off as isize) - (else_p as isize) - 4;
                    asm.patch_i32(else_p, rel as i32);
                }
                block.else_patch = Some(jmp_patch);
            }
            opcode::BR => {
                let depth = wasm.read_var_u32()?;
                let target_idx = ctrl_stack.len() - 1 - (depth as usize);
                let target = &mut ctrl_stack[target_idx];
                if target.kind == 1 {
                    let off = target.label_offset.unwrap();
                    let rel = (off as isize) - (asm.current_offset() as isize) - 5;
                    asm.jmp_rel32(rel as i32);
                } else {
                    let patch = asm.current_offset() + 1;
                    asm.jmp_rel32(0);
                    target.br_patches.push(patch);
                }
            }
            opcode::BR_IF => {
                let depth = wasm.read_var_u32()?;
                asm.pop_reg(Reg::RAX);
                asm.mov_reg_imm64(Reg::RCX, 0);
                asm.cmp_reg_reg(Reg::RAX, Reg::RCX);
                let target_idx = ctrl_stack.len() - 1 - (depth as usize);
                let target = &mut ctrl_stack[target_idx];
                if target.kind == 1 {
                    let off = target.label_offset.unwrap();
                    let rel = (off as isize) - (asm.current_offset() as isize) - 6;
                    asm.jcc_rel32(0x5, rel as i32);
                } else {
                    let patch = asm.current_offset() + 2;
                    asm.jcc_rel32(0x5, 0);
                    target.br_patches.push(patch);
                }
            }
            opcode::I32_CONST => {
                let val = wasm.read_var_i32()?;
                asm.mov_reg_imm64(Reg::RAX, val as u64);
                asm.push_reg(Reg::RAX);
            }
            opcode::I64_CONST => {
                let val = wasm.read_var_i64()?;
                asm.mov_reg_imm64(Reg::RAX, val as u64);
                asm.push_reg(Reg::RAX);
            }
            opcode::I32_ADD | opcode::I64_ADD => {
                asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX);
                if opcode == opcode::I32_ADD { asm.add_r32_r32(Reg::RAX, Reg::RCX); } else { asm.add_reg_reg(Reg::RAX, Reg::RCX); }
                asm.push_reg(Reg::RAX);
            }
            opcode::I32_SUB | opcode::I64_SUB => {
                asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX);
                if opcode == opcode::I32_SUB { asm.sub_r32_r32(Reg::RAX, Reg::RCX); } else { asm.sub_reg_reg(Reg::RAX, Reg::RCX); }
                asm.push_reg(Reg::RAX);
            }
            opcode::I32_MUL | opcode::I64_MUL => {
                asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX);
                if opcode == opcode::I32_MUL { asm.imul_r32_r32(Reg::RAX, Reg::RCX); } else { asm.imul_reg_reg(Reg::RAX, Reg::RCX); }
                asm.push_reg(Reg::RAX);
            }
            opcode::I32_DIV_S | opcode::I64_DIV_S => {
                asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX);
                asm.cqo(); 
                asm.idiv_reg(Reg::RCX);
                asm.push_reg(Reg::RAX);
            }
            opcode::I32_REM_S | opcode::I64_REM_S => {
                asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX);
                asm.cqo();
                asm.idiv_reg(Reg::RCX);
                asm.push_reg(Reg::RDX);
            }
            opcode::I32_DIV_U | opcode::I64_DIV_U => {
                asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX);
                asm.mov_reg_imm64(Reg::RDX, 0); // Clear RDX for unsigned div
                asm.div_reg(Reg::RCX);
                asm.push_reg(Reg::RAX);
            }
            opcode::I32_REM_U | opcode::I64_REM_U => {
                asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX);
                asm.mov_reg_imm64(Reg::RDX, 0);
                asm.div_reg(Reg::RCX);
                asm.push_reg(Reg::RDX); // Remainder
            }
            opcode::I32_AND | opcode::I64_AND => {
                asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX);
                if opcode == opcode::I32_AND { asm.and_r32_r32(Reg::RAX, Reg::RCX); } else { asm.and_reg_reg(Reg::RAX, Reg::RCX); }
                asm.push_reg(Reg::RAX);
            }
            opcode::I32_OR | opcode::I64_OR => {
                asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX);
                if opcode == opcode::I32_OR { asm.or_r32_r32(Reg::RAX, Reg::RCX); } else { asm.or_reg_reg(Reg::RAX, Reg::RCX); }
                asm.push_reg(Reg::RAX);
            }
            opcode::I32_XOR | opcode::I64_XOR => {
                asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX);
                if opcode == opcode::I32_XOR { asm.xor_r32_r32(Reg::RAX, Reg::RCX); } else { asm.xor_reg_reg(Reg::RAX, Reg::RCX); }
                asm.push_reg(Reg::RAX);
            }
            opcode::I32_SHL | opcode::I64_SHL => {
                asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX);
                if opcode == opcode::I32_SHL { asm.shl_r32_cl(Reg::RAX); } else { asm.shl_reg_cl(Reg::RAX); }
                asm.push_reg(Reg::RAX);
            }
            opcode::I32_SHR_S | opcode::I64_SHR_S => {
                asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX);
                if opcode == opcode::I32_SHR_S { asm.sar_r32_cl(Reg::RAX); } else { asm.sar_reg_cl(Reg::RAX); }
                asm.push_reg(Reg::RAX);
            }
            opcode::I32_SHR_U | opcode::I64_SHR_U => {
                asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX);
                if opcode == opcode::I32_SHR_U { asm.shr_r32_cl(Reg::RAX); } else { asm.shr_reg_cl(Reg::RAX); }
                asm.push_reg(Reg::RAX);
            }
            opcode::I32_EQZ | opcode::I64_EQZ => {
                asm.pop_reg(Reg::RAX);
                asm.mov_reg_imm64(Reg::RCX, 0);
                asm.cmp_reg_reg(Reg::RAX, Reg::RCX);
                asm.xor_reg_reg(Reg::RBX, Reg::RBX);
                asm.setcc(0x4, Reg::RBX);
                asm.push_reg(Reg::RBX);
            }
            opcode::I32_EQ | opcode::I64_EQ => {
                asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX);
                asm.cmp_reg_reg(Reg::RAX, Reg::RCX);
                asm.xor_reg_reg(Reg::RBX, Reg::RBX);
                asm.setcc(0x4, Reg::RBX);
                asm.push_reg(Reg::RBX);
            }
            opcode::I32_NE | opcode::I64_NE => {
                asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX);
                asm.cmp_reg_reg(Reg::RAX, Reg::RCX);
                asm.xor_reg_reg(Reg::RBX, Reg::RBX);
                asm.setcc(0x5, Reg::RBX);
                asm.push_reg(Reg::RBX);
            }
            opcode::I32_LT_S | opcode::I64_LT_S => {
                asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX);
                asm.cmp_reg_reg(Reg::RAX, Reg::RCX);
                asm.xor_reg_reg(Reg::RBX, Reg::RBX);
                asm.setcc(0xC, Reg::RBX);
                asm.push_reg(Reg::RBX);
            }
            opcode::I32_GT_S | opcode::I64_GT_S => {
                asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX);
                asm.cmp_reg_reg(Reg::RAX, Reg::RCX);
                asm.xor_reg_reg(Reg::RBX, Reg::RBX);
                asm.setcc(0xF, Reg::RBX);
                asm.push_reg(Reg::RBX);
            }
            opcode::I32_LT_U | opcode::I64_LT_U => {
                asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX);
                asm.cmp_reg_reg(Reg::RAX, Reg::RCX);
                asm.xor_reg_reg(Reg::RBX, Reg::RBX);
                asm.setcc(0x2, Reg::RBX); // SETB (Below)
                asm.push_reg(Reg::RBX);
            }
            opcode::I32_GT_U | opcode::I64_GT_U => {
                asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX);
                asm.cmp_reg_reg(Reg::RAX, Reg::RCX);
                asm.xor_reg_reg(Reg::RBX, Reg::RBX);
                asm.setcc(0x7, Reg::RBX); // SETA (Above)
                asm.push_reg(Reg::RBX);
            }
            opcode::I32_LE_S | opcode::I64_LE_S => {
                asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX);
                asm.cmp_reg_reg(Reg::RAX, Reg::RCX);
                asm.xor_reg_reg(Reg::RBX, Reg::RBX);
                asm.setcc(0xE, Reg::RBX);
                asm.push_reg(Reg::RBX);
            }
            opcode::I32_LE_U | opcode::I64_LE_U => {
                asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX);
                asm.cmp_reg_reg(Reg::RAX, Reg::RCX);
                asm.xor_reg_reg(Reg::RBX, Reg::RBX);
                asm.setcc(0x6, Reg::RBX); // SETBE
                asm.push_reg(Reg::RBX);
            }
            opcode::I32_GE_S | opcode::I64_GE_S => {
                asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX);
                asm.cmp_reg_reg(Reg::RAX, Reg::RCX);
                asm.xor_reg_reg(Reg::RBX, Reg::RBX);
                asm.setcc(0xD, Reg::RBX);
                asm.push_reg(Reg::RBX);
            }
            opcode::I32_GE_U | opcode::I64_GE_U => {
                asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX);
                asm.cmp_reg_reg(Reg::RAX, Reg::RCX);
                asm.xor_reg_reg(Reg::RBX, Reg::RBX);
                asm.setcc(0x3, Reg::RBX); // SETAE
                asm.push_reg(Reg::RBX);
            }
            opcode::I32_WRAP_I64 => {
                asm.pop_reg(Reg::RAX);
                asm.mov_reg_imm64(Reg::RCX, 32);
                asm.shl_reg_cl(Reg::RAX);
                asm.shr_reg_cl(Reg::RAX);
                asm.push_reg(Reg::RAX);
            }
            opcode::I64_EXTEND_I32_U => {
                asm.pop_reg(Reg::RAX);
                // Zero extend 32->64 using 32-bit move
                asm.mov_reg_reg(Reg::RAX, Reg::RAX); 
                // Wait, mov_reg_reg emits REX.W (64 bit). It doesn't zero extend.
                // We need mov r32, r32 (0x89 without REX.W).
                // Or simply `add_r32_r32(RAX, 0)`? No, `add` changes flags.
                // `mov` is best.
                // I didn't add `mov_r32_r32`.
                // I can use `shl/shr` trick again?
                asm.mov_reg_imm64(Reg::RCX, 32);
                asm.shl_reg_cl(Reg::RAX);
                asm.shr_reg_cl(Reg::RAX);
                asm.push_reg(Reg::RAX);
            }
            opcode::I64_EXTEND_I32_S => {
                asm.pop_reg(Reg::RAX);
                // Sign extend 32->64. `movsxd rax, eax` (0x63). I don't have it.
                // Use shift: shl 32, sar 32.
                asm.mov_reg_imm64(Reg::RCX, 32);
                asm.shl_reg_cl(Reg::RAX);
                asm.sar_reg_cl(Reg::RAX);
                asm.push_reg(Reg::RAX);
            }
            opcode::I32_LOAD => {
                let memarg = MemArg::read(&mut wasm)?;
                asm.pop_reg(Reg::RBX);
                asm.mov_r32_mem_base_idx(Reg::RAX, Reg::R15, Reg::RBX, memarg.offset as i32);
                asm.push_reg(Reg::RAX);
            }
            opcode::I64_LOAD => {
                let memarg = MemArg::read(&mut wasm)?;
                asm.pop_reg(Reg::RBX);
                asm.mov_r64_mem_base_idx(Reg::RAX, Reg::R15, Reg::RBX, memarg.offset as i32);
                asm.push_reg(Reg::RAX);
            }
            opcode::I32_LOAD8_U | opcode::I64_LOAD8_U => {
                let memarg = MemArg::read(&mut wasm)?;
                asm.pop_reg(Reg::RBX);
                asm.movzx_r8_mem_base_idx(Reg::RAX, Reg::R15, Reg::RBX, memarg.offset as i32);
                asm.push_reg(Reg::RAX);
            }
            opcode::I32_LOAD8_S | opcode::I64_LOAD8_S => {
                let memarg = MemArg::read(&mut wasm)?;
                asm.pop_reg(Reg::RBX);
                asm.movsx_r8_mem_base_idx(Reg::RAX, Reg::R15, Reg::RBX, memarg.offset as i32);
                asm.push_reg(Reg::RAX);
            }
            opcode::I32_LOAD16_U | opcode::I64_LOAD16_U => {
                let memarg = MemArg::read(&mut wasm)?;
                asm.pop_reg(Reg::RBX);
                asm.movzx_r16_mem_base_idx(Reg::RAX, Reg::R15, Reg::RBX, memarg.offset as i32);
                asm.push_reg(Reg::RAX);
            }
            opcode::I32_LOAD16_S | opcode::I64_LOAD16_S => {
                let memarg = MemArg::read(&mut wasm)?;
                asm.pop_reg(Reg::RBX);
                asm.movsx_r16_mem_base_idx(Reg::RAX, Reg::R15, Reg::RBX, memarg.offset as i32);
                asm.push_reg(Reg::RAX);
            }
            opcode::I32_STORE => {
                let memarg = MemArg::read(&mut wasm)?;
                asm.pop_reg(Reg::RAX);
                asm.pop_reg(Reg::RBX);
                asm.mov_mem_base_idx_r32(Reg::R15, Reg::RBX, memarg.offset as i32, Reg::RAX);
            }
            opcode::I64_STORE => {
                let memarg = MemArg::read(&mut wasm)?;
                asm.pop_reg(Reg::RAX);
                asm.pop_reg(Reg::RBX);
                asm.mov_mem_base_idx_r64(Reg::R15, Reg::RBX, memarg.offset as i32, Reg::RAX);
            }
            opcode::I32_STORE8 => {
                let memarg = MemArg::read(&mut wasm)?;
                asm.pop_reg(Reg::RAX);
                asm.pop_reg(Reg::RBX);
                asm.mov_mem_base_idx_r8(Reg::R15, Reg::RBX, memarg.offset as i32, Reg::RAX);
            }
            opcode::I32_STORE16 => {
                let memarg = MemArg::read(&mut wasm)?;
                asm.pop_reg(Reg::RAX);
                asm.pop_reg(Reg::RBX);
                asm.mov_mem_base_idx_r16(Reg::R15, Reg::RBX, memarg.offset as i32, Reg::RAX);
            }
            opcode::LOCAL_GET => {
                let idx = wasm.read_var_u32()?;
                let offset = -8 * ((idx as i32) + 1);
                asm.mov_reg_mem(Reg::RAX, Reg::RBP, offset);
                asm.push_reg(Reg::RAX);
            }
            opcode::LOCAL_SET => {
                let idx = wasm.read_var_u32()?;
                let offset = -8 * ((idx as i32) + 1);
                asm.pop_reg(Reg::RAX);
                asm.mov_mem_reg(Reg::RBP, offset, Reg::RAX);
            }
            opcode::LOCAL_TEE => {
                let idx = wasm.read_var_u32()?;
                let offset = -8 * ((idx as i32) + 1);
                asm.mov_reg_mem(Reg::RAX, Reg::RSP, 0);
                asm.mov_mem_reg(Reg::RBP, offset, Reg::RAX);
            }
            opcode::DROP => {
                asm.pop_reg(Reg::RAX);
            }
            _ => {
                return Err(ValidationError::InvalidInstr(opcode));
            }
        }
    }

    let result_count = func.function_type.returns.valtypes.len();
    for i in (0..result_count).rev() {
        asm.pop_reg(Reg::RAX);
        asm.mov_mem_reg(Reg::RDX, (i * 8) as i32, Reg::RAX);
    }

    asm.pop_reg(Reg::R14);
    asm.pop_reg(Reg::R15);
    asm.mov_reg_reg(Reg::RSP, Reg::RBP);
    asm.pop_reg(Reg::RBP);
    asm.ret();

    Ok(asm.buf)
}