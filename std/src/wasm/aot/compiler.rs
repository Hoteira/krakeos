use super::emitter::{Assembler, Reg};
use super::memory::ExecutableBuffer;
use crate::rust_alloc::vec::Vec;
use crate::rust_alloc::collections::BTreeMap;
use crate::wasm::core::reader::{WasmReader, WasmReadable};
use crate::wasm::core::reader::types::{opcode, BlockType, memarg::MemArg, FuncType, ValType, RefType};
use crate::wasm::core::indices::LabelIdx;
use crate::wasm::execution::store::instances::{WasmFuncInst, ModuleInst};
use crate::wasm::execution::store::addrs::FuncAddr;
use crate::wasm::execution::config::Config;
use crate::wasm::core::error::ValidationError;

struct ControlBlock {
    kind: u8, // 0=Block, 1=Loop, 2=If
    label_offset: Option<usize>, 
    br_patches: Vec<usize>,
    br_table_patches: Vec<(usize, usize)>,
    else_patch: Option<usize>,
    stack_height: usize,
    arity: usize,
}

pub fn compile_function<T: Config>(
    func: &WasmFuncInst,
    module: &ModuleInst,
    module_addr: usize,
    func_types: &BTreeMap<FuncAddr, FuncType>,
) -> Result<ExecutableBuffer, ValidationError> {
    let mut asm = Assembler::new();
    let mut wasm = WasmReader::new(module.wasm_bytecode);
    
    wasm.move_start_to(func.code_expr).map_err(|_| ValidationError::Eof)?;

    // Prologue
    asm.push_reg(Reg::RBP);
    asm.mov_reg_reg(Reg::RBP, Reg::RSP);

    // Save R15 (Mem Base), R14 (Store/VM Context), R13 (Result Ptr), RBX (Scratch)
    asm.push_reg(Reg::R15);
    asm.push_reg(Reg::R14);
    asm.push_reg(Reg::R13);
    asm.push_reg(Reg::RBX);
    
    // Move RCX (arg 4: Mem Base) to R15
    asm.mov_reg_reg(Reg::R15, Reg::RCX);
    // Move RDI (arg 1: Store/VM Context) to R14
    asm.mov_reg_reg(Reg::R14, Reg::RDI);
    // Move RDX (arg 3: Result Ptr) to R13
    asm.mov_reg_reg(Reg::R13, Reg::RDX);

    let param_count = func.function_type.params.valtypes.len();
    for i in 0..param_count {
        asm.mov_reg_mem(Reg::RAX, Reg::RSI, (i * 8) as i32);
        asm.push_reg(Reg::RAX);
    }

    for _ in 0..func.locals.len() {
        asm.mov_reg_imm64(Reg::RAX, 0);
        asm.push_reg(Reg::RAX);
    }

    let mut stack_depth = param_count + func.locals.len();

    let mut ctrl_stack: Vec<ControlBlock> = Vec::new();
    ctrl_stack.push(ControlBlock { 
        kind: 0, 
        label_offset: None, 
        br_patches: Vec::new(), 
        br_table_patches: Vec::new(), 
        else_patch: None, 
        stack_height: stack_depth,
        arity: func.function_type.returns.valtypes.len() 
    });

    let get_arity = |ty: BlockType, kind: u8, module: &ModuleInst| -> usize {
        match ty {
            BlockType::Empty => 0,
            BlockType::Returns(_) => if kind == 1 { 0 } else { 1 },
            BlockType::Type(idx) => {
                let func_ty = &module.types[idx as usize];
                if kind == 1 { func_ty.params.valtypes.len() } else { func_ty.returns.valtypes.len() }
            }
        }
    };

    loop {
        let pc = wasm.pc as u64;
        let opcode = wasm.read_u8().map_err(|_| ValidationError::Eof)?;
        
        // Debug trace
        asm.call_debug(pc, opcode as u64);

        match opcode {
            opcode::END => {
                let block = ctrl_stack.pop().unwrap();
                let current_off = asm.current_offset();
                for patch in block.br_patches {
                    let rel = (current_off as isize) - (patch as isize) - 4;
                    asm.patch_i32(patch, rel as i32);
                }
                for (patch, base) in block.br_table_patches {
                    let rel = (current_off as isize) - (base as isize);
                    asm.patch_i32(patch, rel as i32);
                }
                if block.kind == 2 {
                    if let Some(else_p) = block.else_patch {
                        let rel = (current_off as isize) - (else_p as isize) - 4;
                        asm.patch_i32(else_p, rel as i32);
                    }
                }
                if ctrl_stack.is_empty() { break; }
            }
            opcode::RETURN => {
                let target = &mut ctrl_stack[0];
                let drop_count = stack_depth - target.stack_height - target.arity;
                if drop_count > 0 {
                    if target.arity > 0 {
                        asm.pop_reg(Reg::RAX);
                        asm.mov_reg_imm64(Reg::RCX, (drop_count * 8) as u64);
                        asm.add_reg_reg(Reg::RSP, Reg::RCX);
                        asm.push_reg(Reg::RAX);
                    } else {
                        asm.mov_reg_imm64(Reg::RCX, (drop_count * 8) as u64);
                        asm.add_reg_reg(Reg::RSP, Reg::RCX);
                    }
                }
                let patch = asm.current_offset() + 1;
                asm.jmp_rel32(0);
                target.br_patches.push(patch);
            }
            opcode::UNREACHABLE => { asm.ud2(); }
            opcode::NOP => { }
            opcode::BLOCK => {
                let ty = BlockType::read(&mut wasm)?;
                let arity = get_arity(ty, 0, module);
                ctrl_stack.push(ControlBlock { kind: 0, label_offset: None, br_patches: Vec::new(), br_table_patches: Vec::new(), else_patch: None, stack_height: stack_depth, arity });
            }
            opcode::LOOP => {
                let ty = BlockType::read(&mut wasm)?;
                let arity = get_arity(ty, 1, module);
                let start_off = asm.current_offset();
                ctrl_stack.push(ControlBlock { kind: 1, label_offset: Some(start_off), br_patches: Vec::new(), br_table_patches: Vec::new(), else_patch: None, stack_height: stack_depth, arity });
            }
            opcode::IF => {
                let ty = BlockType::read(&mut wasm)?;
                let arity = get_arity(ty, 2, module);
                asm.pop_reg(Reg::RAX); stack_depth -= 1;
                asm.mov_reg_imm64(Reg::RCX, 0);
                asm.cmp_reg_reg(Reg::RAX, Reg::RCX);
                let patch_off = asm.current_offset() + 2;
                asm.jcc_rel32(0x4, 0); 
                ctrl_stack.push(ControlBlock { kind: 2, label_offset: None, br_patches: Vec::new(), br_table_patches: Vec::new(), else_patch: Some(patch_off), stack_height: stack_depth, arity });
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
                stack_depth = block.stack_height;
            }
            opcode::BR => {
                let depth = wasm.read_var_u32()?;
                let target_idx = ctrl_stack.len() - 1 - (depth as usize);
                let target = &mut ctrl_stack[target_idx];
                let drop_count = stack_depth - target.stack_height - target.arity;
                if drop_count > 0 {
                    if target.arity > 0 {
                        asm.pop_reg(Reg::RAX);
                        asm.mov_reg_imm64(Reg::RCX, (drop_count * 8) as u64);
                        asm.add_reg_reg(Reg::RSP, Reg::RCX);
                        asm.push_reg(Reg::RAX);
                    } else {
                        asm.mov_reg_imm64(Reg::RCX, (drop_count * 8) as u64);
                        asm.add_reg_reg(Reg::RSP, Reg::RCX);
                    }
                }
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
                asm.pop_reg(Reg::RAX); stack_depth -= 1;
                asm.mov_reg_imm64(Reg::RCX, 0);
                asm.cmp_reg_reg(Reg::RAX, Reg::RCX);
                let skip_branch = asm.current_offset() + 2;
                asm.jcc_rel32(0x4, 0); 
                let target_idx = ctrl_stack.len() - 1 - (depth as usize);
                let target = &mut ctrl_stack[target_idx];
                let drop_count = stack_depth - target.stack_height - target.arity;
                if drop_count > 0 {
                    if target.arity > 0 {
                        asm.pop_reg(Reg::RAX);
                        asm.mov_reg_imm64(Reg::RCX, (drop_count * 8) as u64);
                        asm.add_reg_reg(Reg::RSP, Reg::RCX);
                        asm.push_reg(Reg::RAX);
                    } else {
                        asm.mov_reg_imm64(Reg::RCX, (drop_count * 8) as u64);
                        asm.add_reg_reg(Reg::RSP, Reg::RCX);
                    }
                }
                if target.kind == 1 {
                    let off = target.label_offset.unwrap();
                    let rel = (off as isize) - (asm.current_offset() as isize) - 5;
                    asm.jmp_rel32(rel as i32);
                } else {
                    let patch = asm.current_offset() + 1;
                    asm.jmp_rel32(0);
                    target.br_patches.push(patch);
                }
                let current = asm.current_offset();
                let rel = (current as isize) - (skip_branch as isize) - 4;
                asm.patch_i32(skip_branch, rel as i32);
            }
            opcode::BR_TABLE => {
                let targets = wasm.read_vec(|w| w.read_var_u32().map(|v| v as LabelIdx))?;
                let default_target = wasm.read_var_u32()? as LabelIdx;
                
                asm.pop_reg(Reg::RAX); stack_depth -= 1;
                asm.mov_reg_imm64(Reg::RCX, targets.len() as u64);
                asm.cmp_reg_reg(Reg::RAX, Reg::RCX);
                
                let default_patch = asm.current_offset() + 2;
                asm.jcc_rel32(0x3, 0); // JAE default

                // LEA RDX, [RIP + ?]
                let lea_patch_off = asm.current_offset() + 3;
                asm.lea_rip_reg(Reg::RDX, 0); // Placeholder
                
                asm.movsxd_r64_mem_base_idx_scale4(Reg::RCX, Reg::RDX, Reg::RAX, 0);
                asm.add_reg_reg(Reg::RDX, Reg::RCX);
                asm.jmp_reg(Reg::RDX);

                // Stubs generation
                let mut table_entries = Vec::new(); // (is_stub, offset_or_0)

                for label in targets.iter() { // Iterate to avoid moving targets
                    let target_idx = ctrl_stack.len() - 1 - (*label as usize);
                    let target = &mut ctrl_stack[target_idx];
                    
                    let drop_count = stack_depth - target.stack_height - target.arity;
                    
                    if drop_count > 0 {
                        let stub_start = asm.current_offset();
                        if target.arity > 0 {
                            asm.pop_reg(Reg::RAX);
                            asm.mov_reg_imm64(Reg::RCX, (drop_count * 8) as u64);
                            asm.add_reg_reg(Reg::RSP, Reg::RCX);
                            asm.push_reg(Reg::RAX);
                        } else {
                            asm.mov_reg_imm64(Reg::RCX, (drop_count * 8) as u64);
                            asm.add_reg_reg(Reg::RSP, Reg::RCX);
                        }
                        
                        if target.kind == 1 {
                            let off = target.label_offset.unwrap();
                            let rel = (off as isize) - (asm.current_offset() as isize) - 5;
                            asm.jmp_rel32(rel as i32);
                        } else {
                            let patch = asm.current_offset() + 1;
                            asm.jmp_rel32(0);
                            target.br_patches.push(patch);
                        }
                        table_entries.push((true, stub_start));
                    } else {
                        table_entries.push((false, 0));
                    }
                }

                let table_start = asm.current_offset();
                
                // Patch LEA
                let lea_rel = (table_start as isize) - (lea_patch_off as isize) - 4;
                asm.patch_i32(lea_patch_off, lea_rel as i32);

                // Emit Table
                for (i, (is_stub, stub_off)) in table_entries.into_iter().enumerate() {
                    if is_stub {
                        let rel = (stub_off as isize) - (table_start as isize);
                        asm.buf.emit_u32(rel as u32);
                    } else {
                        let label = targets[i];
                        let target_idx = ctrl_stack.len() - 1 - (label as usize);
                        let target = &mut ctrl_stack[target_idx];
                        
                        if target.kind == 1 {
                            let off = target.label_offset.unwrap();
                            let rel = (off as isize) - (table_start as isize);
                            asm.buf.emit_u32(rel as u32);
                        } else {
                            let patch_off = asm.current_offset();
                            asm.buf.emit_u32(0);
                            target.br_table_patches.push((patch_off, table_start));
                        }
                    }
                }
                
                // Default target logic
                let default_loc = asm.current_offset();
                let rel = (default_loc as isize) - (default_patch as isize) - 4;
                asm.patch_i32(default_patch, rel as i32);
                
                let target_idx = ctrl_stack.len() - 1 - (default_target as usize);
                let target = &mut ctrl_stack[target_idx];
                
                let drop_count = stack_depth - target.stack_height - target.arity;
                if drop_count > 0 {
                    if target.arity > 0 {
                        asm.pop_reg(Reg::RAX);
                        asm.mov_reg_imm64(Reg::RCX, (drop_count * 8) as u64);
                        asm.add_reg_reg(Reg::RSP, Reg::RCX);
                        asm.push_reg(Reg::RAX);
                    } else {
                        asm.mov_reg_imm64(Reg::RCX, (drop_count * 8) as u64);
                        asm.add_reg_reg(Reg::RSP, Reg::RCX);
                    }
                }

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
            opcode::SELECT | opcode::SELECT_T => {
                if opcode == opcode::SELECT_T { let _ = wasm.read_vec(ValType::read)?; }
                asm.pop_reg(Reg::RCX);
                asm.pop_reg(Reg::RBX);
                asm.pop_reg(Reg::RAX);
                stack_depth -= 2;
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
                stack_depth += 1;
            }
            opcode::GLOBAL_SET => {
                let global_idx = wasm.read_var_u32()?;
                asm.pop_reg(Reg::RDX); stack_depth -= 1;
                asm.mov_reg_reg(Reg::RDI, Reg::R14);
                asm.mov_reg_imm64(Reg::RSI, global_idx as u64);
                asm.mov_reg_imm64(Reg::RCX, module_addr as u64);
                let trampoline = crate::wasm::aot::trampoline::aot_global_set::<T> as usize;
                asm.mov_reg_imm64(Reg::RAX, trampoline as u64);
                asm.call_reg(Reg::RAX);
            }
            opcode::MEMORY_SIZE | opcode::MEMORY_GROW => {
                let mem_idx = wasm.read_u8()?;
                if opcode == opcode::MEMORY_GROW { asm.pop_reg(Reg::RDX); stack_depth -= 1; }
                asm.mov_reg_reg(Reg::RDI, Reg::R14);
                asm.mov_reg_imm64(Reg::RSI, mem_idx as u64);
                if opcode == opcode::MEMORY_GROW { asm.mov_reg_imm64(Reg::RCX, module_addr as u64); } else { asm.mov_reg_imm64(Reg::RDX, module_addr as u64); }
                let trampoline = if opcode == opcode::MEMORY_GROW { crate::wasm::aot::trampoline::aot_memory_grow::<T> as usize } else { crate::wasm::aot::trampoline::aot_memory_size::<T> as usize };
                asm.mov_reg_imm64(Reg::RAX, trampoline as u64);
                asm.call_reg(Reg::RAX);
                asm.push_reg(Reg::RAX);
                stack_depth += 1;
            }
            opcode::CALL => {
                let func_idx = wasm.read_var_u32()?;
                let func_addr = module.func_addrs[func_idx as usize];
                let func_ty = func_types.get(&func_addr).expect("FuncType not found");
                let n_args = func_ty.params.valtypes.len();
                let n_results = func_ty.returns.valtypes.len();
                for i in 0..n_args { asm.pop_reg(Reg::RAX); asm.mov_mem_reg(Reg::RSP, -8 * ((i as i32) + 1), Reg::RAX); }
                if n_args > 0 { asm.mov_reg_imm64(Reg::RAX, (n_args * 8) as u64); asm.sub_reg_reg(Reg::RSP, Reg::RAX); }
                stack_depth -= n_args;
                if n_results > 0 { asm.mov_reg_imm64(Reg::RAX, (n_results * 8) as u64); asm.sub_reg_reg(Reg::RSP, Reg::RAX); }
                asm.mov_reg_reg(Reg::RDI, Reg::R14);
                asm.mov_reg_imm64(Reg::RSI, func_addr as u64);
                asm.mov_reg_reg(Reg::RCX, Reg::RSP);
                asm.mov_reg_reg(Reg::RDX, Reg::RSP);
                if n_results > 0 { asm.mov_reg_imm64(Reg::RAX, (n_results * 8) as u64); asm.add_reg_reg(Reg::RDX, Reg::RAX); }
                let trampoline = crate::wasm::aot::trampoline::aot_invoke_trampoline::<T> as usize;
                asm.mov_reg_imm64(Reg::RAX, trampoline as u64);
                asm.call_reg(Reg::RAX);
                asm.mov_reg_reg(Reg::RDI, Reg::R14);
                asm.mov_reg_imm64(Reg::RSI, module_addr as u64);
                let get_mem_base = crate::wasm::aot::trampoline::aot_get_mem_base::<T> as usize;
                asm.mov_reg_imm64(Reg::RAX, get_mem_base as u64);
                asm.call_reg(Reg::RAX);
                asm.mov_reg_reg(Reg::R15, Reg::RAX);
                for i in 0..n_results { asm.mov_reg_mem(Reg::RAX, Reg::RSP, (i * 8) as i32); asm.mov_mem_reg(Reg::RSP, ((i * 8) + (n_args * 8)) as i32, Reg::RAX); }
                if n_args > 0 { asm.mov_reg_imm64(Reg::RAX, (n_args * 8) as u64); asm.add_reg_reg(Reg::RSP, Reg::RAX); }
                stack_depth += n_results;
            }
            opcode::CALL_INDIRECT => {
                let type_idx = wasm.read_var_u32()?;
                let table_idx = wasm.read_var_u32()?;
                let func_ty = module.types[type_idx as usize].clone();
                let n_args = func_ty.params.valtypes.len();
                let n_results = func_ty.returns.valtypes.len();
                asm.pop_reg(Reg::RCX); stack_depth -= 1;
                for i in 0..n_args { asm.pop_reg(Reg::RAX); asm.mov_mem_reg(Reg::RSP, -8 * ((i as i32) + 1), Reg::RAX); }
                if n_args > 0 { asm.mov_reg_imm64(Reg::RAX, (n_args * 8) as u64); asm.sub_reg_reg(Reg::RSP, Reg::RAX); }
                stack_depth -= n_args;
                if n_results > 0 { asm.mov_reg_imm64(Reg::RAX, (n_results * 8) as u64); asm.sub_reg_reg(Reg::RSP, Reg::RAX); }
                asm.mov_reg_reg(Reg::RDI, Reg::R14);
                asm.mov_reg_imm64(Reg::RSI, type_idx as u64);
                asm.mov_reg_imm64(Reg::RDX, table_idx as u64);
                asm.mov_reg_reg(Reg::R8, Reg::RSP);
                if n_results > 0 { asm.mov_reg_imm64(Reg::RAX, (n_results * 8) as u64); asm.add_reg_reg(Reg::R8, Reg::RAX); }
                asm.mov_reg_reg(Reg::R9, Reg::RSP);
                asm.mov_reg_imm64(Reg::RAX, module_addr as u64);
                asm.push_reg(Reg::RAX);
                let trampoline = crate::wasm::aot::trampoline::aot_call_indirect::<T> as usize;
                asm.mov_reg_imm64(Reg::RAX, trampoline as u64);
                asm.call_reg(Reg::RAX);
                asm.pop_reg(Reg::RAX); 
                asm.mov_reg_reg(Reg::RDI, Reg::R14);
                asm.mov_reg_imm64(Reg::RSI, module_addr as u64);
                let get_mem_base = crate::wasm::aot::trampoline::aot_get_mem_base::<T> as usize;
                asm.mov_reg_imm64(Reg::RAX, get_mem_base as u64);
                asm.call_reg(Reg::RAX);
                asm.mov_reg_reg(Reg::R15, Reg::RAX);
                for i in 0..n_results { asm.mov_reg_mem(Reg::RAX, Reg::RSP, (i * 8) as i32); asm.mov_mem_reg(Reg::RSP, ((i * 8) + (n_args * 8)) as i32, Reg::RAX); }
                if n_args > 0 { asm.mov_reg_imm64(Reg::RAX, (n_args * 8) as u64); asm.add_reg_reg(Reg::RSP, Reg::RAX); }
                stack_depth += n_results;
            }
            opcode::I32_CONST | opcode::I64_CONST | opcode::F32_CONST | opcode::F64_CONST => {
                if opcode == opcode::I32_CONST { asm.mov_reg_imm64(Reg::RAX, wasm.read_var_i32()? as u64); }
                else if opcode == opcode::I64_CONST { asm.mov_reg_imm64(Reg::RAX, wasm.read_var_i64()? as u64); }
                else if opcode == opcode::F32_CONST { asm.mov_reg_imm64(Reg::RAX, wasm.read_f32()? as u64); }
                else { asm.mov_reg_imm64(Reg::RAX, wasm.read_f64()?); }
                asm.push_reg(Reg::RAX);
                stack_depth += 1;
            }
            opcode::I32_ADD | opcode::I64_ADD | opcode::I32_SUB | opcode::I64_SUB | opcode::I32_MUL | opcode::I64_MUL |
            opcode::I32_DIV_S | opcode::I64_DIV_S | opcode::I32_REM_S | opcode::I64_REM_S | opcode::I32_DIV_U | opcode::I64_DIV_U |
            opcode::I32_REM_U | opcode::I64_REM_U | opcode::I32_AND | opcode::I64_AND | opcode::I32_OR | opcode::I64_OR |
            opcode::I32_XOR | opcode::I64_XOR | opcode::I32_SHL | opcode::I64_SHL | opcode::I32_SHR_S | opcode::I64_SHR_S |
            opcode::I32_SHR_U | opcode::I64_SHR_U | opcode::I32_ROTL | opcode::I64_ROTL | opcode::I32_ROTR | opcode::I64_ROTR |
            opcode::F32_ADD | opcode::F64_ADD | opcode::F32_SUB | opcode::F64_SUB | opcode::F32_MUL | opcode::F64_MUL |
            opcode::F32_DIV | opcode::F64_DIV | opcode::F32_MIN | opcode::F64_MIN | opcode::F32_MAX | opcode::F64_MAX |
            opcode::F32_COPYSIGN | opcode::F64_COPYSIGN |
            opcode::I32_EQ | opcode::I64_EQ | opcode::I32_NE | opcode::I64_NE | opcode::I32_LT_S | opcode::I64_LT_S |
            opcode::I32_GT_S | opcode::I64_GT_S | opcode::I32_LT_U | opcode::I64_LT_U | opcode::I32_GT_U | opcode::I64_GT_U |
            opcode::I32_LE_S | opcode::I64_LE_S | opcode::I32_LE_U | opcode::I64_LE_U | opcode::I32_GE_S | opcode::I64_GE_S |
            opcode::I32_GE_U | opcode::I64_GE_U | opcode::F32_EQ | opcode::F64_EQ | opcode::F32_NE | opcode::F64_NE |
            opcode::F32_LT | opcode::F64_LT | opcode::F32_GT | opcode::F64_GT | opcode::F32_LE | opcode::F64_LE |
            opcode::F32_GE | opcode::F64_GE
            => {
                stack_depth -= 1;
                if opcode == opcode::I32_ADD { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.add_r32_r32(Reg::RAX, Reg::RCX); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::I64_ADD { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.add_reg_reg(Reg::RAX, Reg::RCX); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::I32_SUB { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.sub_r32_r32(Reg::RAX, Reg::RCX); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::I64_SUB { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.sub_reg_reg(Reg::RAX, Reg::RCX); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::I32_MUL { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.imul_r32_r32(Reg::RAX, Reg::RCX); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::I64_MUL { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.imul_reg_reg(Reg::RAX, Reg::RCX); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::I32_DIV_S | opcode::I64_DIV_S { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.cqo(); asm.idiv_reg(Reg::RCX); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::I32_REM_S | opcode::I64_REM_S { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.cqo(); asm.idiv_reg(Reg::RCX); asm.push_reg(Reg::RDX); }
                else if opcode == opcode::I32_DIV_U | opcode::I64_DIV_U { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.mov_reg_imm64(Reg::RDX, 0); asm.div_reg(Reg::RCX); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::I32_REM_U | opcode::I64_REM_U { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.mov_reg_imm64(Reg::RDX, 0); asm.div_reg(Reg::RCX); asm.push_reg(Reg::RDX); }
                else if opcode == opcode::I32_AND { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.and_r32_r32(Reg::RAX, Reg::RCX); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::I64_AND { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.and_reg_reg(Reg::RAX, Reg::RCX); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::I32_OR { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.or_r32_r32(Reg::RAX, Reg::RCX); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::I64_OR { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.or_reg_reg(Reg::RAX, Reg::RCX); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::I32_XOR { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.xor_r32_r32(Reg::RAX, Reg::RCX); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::I64_XOR { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.xor_reg_reg(Reg::RAX, Reg::RCX); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::I32_SHL { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.shl_r32_cl(Reg::RAX); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::I64_SHL { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.shl_reg_cl(Reg::RAX); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::I32_SHR_S { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.sar_r32_cl(Reg::RAX); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::I64_SHR_S { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.sar_reg_cl(Reg::RAX); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::I32_SHR_U { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.shr_r32_cl(Reg::RAX); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::I64_SHR_U { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.shr_reg_cl(Reg::RAX); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::I32_ROTL { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.rotl_r32_cl(Reg::RAX); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::I64_ROTL { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.rotl_reg_cl(Reg::RAX); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::I32_ROTR { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.rotr_r32_cl(Reg::RAX); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::I64_ROTR { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.rotr_reg_cl(Reg::RAX); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::I32_EQ | opcode::I64_EQ { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.cmp_reg_reg(Reg::RAX, Reg::RCX); asm.xor_reg_reg(Reg::RBX, Reg::RBX); asm.setcc(0x4, Reg::RBX); asm.push_reg(Reg::RBX); }
                else if opcode == opcode::I32_NE | opcode::I64_NE { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.cmp_reg_reg(Reg::RAX, Reg::RCX); asm.xor_reg_reg(Reg::RBX, Reg::RBX); asm.setcc(0x5, Reg::RBX); asm.push_reg(Reg::RBX); }
                else if opcode == opcode::I32_LT_S | opcode::I64_LT_S { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.cmp_reg_reg(Reg::RAX, Reg::RCX); asm.xor_reg_reg(Reg::RBX, Reg::RBX); asm.setcc(0xC, Reg::RBX); asm.push_reg(Reg::RBX); }
                else if opcode == opcode::I32_GT_S | opcode::I64_GT_S { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.cmp_reg_reg(Reg::RAX, Reg::RCX); asm.xor_reg_reg(Reg::RBX, Reg::RBX); asm.setcc(0xF, Reg::RBX); asm.push_reg(Reg::RBX); }
                else if opcode == opcode::I32_LT_U | opcode::I64_LT_U { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.cmp_reg_reg(Reg::RAX, Reg::RCX); asm.xor_reg_reg(Reg::RBX, Reg::RBX); asm.setcc(0x2, Reg::RBX); asm.push_reg(Reg::RBX); }
                else if opcode == opcode::I32_GT_U | opcode::I64_GT_U { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.cmp_reg_reg(Reg::RAX, Reg::RCX); asm.xor_reg_reg(Reg::RBX, Reg::RBX); asm.setcc(0x7, Reg::RBX); asm.push_reg(Reg::RBX); }
                else if opcode == opcode::I32_LE_S | opcode::I64_LE_S { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.cmp_reg_reg(Reg::RAX, Reg::RCX); asm.xor_reg_reg(Reg::RBX, Reg::RBX); asm.setcc(0xE, Reg::RBX); asm.push_reg(Reg::RBX); }
                else if opcode == opcode::I32_LE_U | opcode::I64_LE_U { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.cmp_reg_reg(Reg::RAX, Reg::RCX); asm.xor_reg_reg(Reg::RBX, Reg::RBX); asm.setcc(0x6, Reg::RBX); asm.push_reg(Reg::RBX); }
                else if opcode == opcode::I32_GE_S | opcode::I64_GE_S { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.cmp_reg_reg(Reg::RAX, Reg::RCX); asm.xor_reg_reg(Reg::RBX, Reg::RBX); asm.setcc(0xD, Reg::RBX); asm.push_reg(Reg::RBX); }
                else if opcode == opcode::I32_GE_U | opcode::I64_GE_U { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.cmp_reg_reg(Reg::RAX, Reg::RCX); asm.xor_reg_reg(Reg::RBX, Reg::RBX); asm.setcc(0x3, Reg::RBX); asm.push_reg(Reg::RBX); }
                else if opcode == opcode::F32_ADD { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.movq_xmm_reg(1, Reg::RCX); asm.movq_xmm_reg(0, Reg::RAX); asm.addss_xmm_xmm(0, 1); asm.movq_reg_xmm(Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::F64_ADD { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.movq_xmm_reg(1, Reg::RCX); asm.movq_xmm_reg(0, Reg::RAX); asm.addsd_xmm_xmm(0, 1); asm.movq_reg_xmm(Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::F32_SUB { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.movq_xmm_reg(1, Reg::RCX); asm.movq_xmm_reg(0, Reg::RAX); asm.subss_xmm_xmm(0, 1); asm.movq_reg_xmm(Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::F64_SUB { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.movq_xmm_reg(1, Reg::RCX); asm.movq_xmm_reg(0, Reg::RAX); asm.subsd_xmm_xmm(0, 1); asm.movq_reg_xmm(Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::F32_MUL { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.movq_xmm_reg(1, Reg::RCX); asm.movq_xmm_reg(0, Reg::RAX); asm.mulss_xmm_xmm(0, 1); asm.movq_reg_xmm(Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::F64_MUL { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.movq_xmm_reg(1, Reg::RCX); asm.movq_xmm_reg(0, Reg::RAX); asm.mulsd_xmm_xmm(0, 1); asm.movq_reg_xmm(Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::F32_DIV { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.movq_xmm_reg(1, Reg::RCX); asm.movq_xmm_reg(0, Reg::RAX); asm.divss_xmm_xmm(0, 1); asm.movq_reg_xmm(Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::F64_DIV { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.movq_xmm_reg(1, Reg::RCX); asm.movq_xmm_reg(0, Reg::RAX); asm.divsd_xmm_xmm(0, 1); asm.movq_reg_xmm(Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::F32_SQRT { asm.push_reg(Reg::RAX); stack_depth += 1; asm.pop_reg(Reg::RAX); asm.movq_xmm_reg(0, Reg::RAX); asm.sqrtss_xmm_xmm(0, 0); asm.movq_reg_xmm(Reg::RAX, 0); asm.push_reg(Reg::RAX); } // Oops, SQRT is unary
                else if opcode == opcode::F64_SQRT { asm.push_reg(Reg::RAX); stack_depth += 1; asm.pop_reg(Reg::RAX); asm.movq_xmm_reg(0, Reg::RAX); asm.sqrtsd_xmm_xmm(0, 0); asm.movq_reg_xmm(Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::F32_MIN { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.movq_xmm_reg(1, Reg::RCX); asm.movq_xmm_reg(0, Reg::RAX); asm.minss_xmm_xmm(0, 1); asm.movq_reg_xmm(Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::F64_MIN { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.movq_xmm_reg(1, Reg::RCX); asm.movq_xmm_reg(0, Reg::RAX); asm.minsd_xmm_xmm(0, 1); asm.movq_reg_xmm(Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::F32_MAX { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.movq_xmm_reg(1, Reg::RCX); asm.movq_xmm_reg(0, Reg::RAX); asm.maxss_xmm_xmm(0, 1); asm.movq_reg_xmm(Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::F64_MAX { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.movq_xmm_reg(1, Reg::RCX); asm.movq_xmm_reg(0, Reg::RAX); asm.maxsd_xmm_xmm(0, 1); asm.movq_reg_xmm(Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::F32_COPYSIGN { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.movd_xmm_r32(1, Reg::RCX); asm.movd_xmm_r32(0, Reg::RAX); asm.mov_reg_imm64(Reg::RDX, 0x80000000); asm.movd_xmm_r32(2, Reg::RDX); asm.andps_xmm_xmm(1, 2); asm.mov_reg_imm64(Reg::RDX, 0x7FFFFFFF); asm.movd_xmm_r32(3, Reg::RDX); asm.andps_xmm_xmm(0, 3); asm.orps_xmm_xmm(0, 1); asm.movd_r32_xmm(Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::F64_COPYSIGN { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.movq_xmm_reg(1, Reg::RCX); asm.movq_xmm_reg(0, Reg::RAX); asm.mov_reg_imm64(Reg::RDX, 0x8000000000000000); asm.movq_xmm_reg(2, Reg::RDX); asm.andps_xmm_xmm(1, 2); asm.mov_reg_imm64(Reg::RDX, 0x7FFFFFFFFFFFFFFF); asm.movq_xmm_reg(3, Reg::RDX); asm.andps_xmm_xmm(0, 3); asm.orps_xmm_xmm(0, 1); asm.movq_reg_xmm(Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::F32_EQ { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.movq_xmm_reg(1, Reg::RCX); asm.movq_xmm_reg(0, Reg::RAX); asm.ucomiss_xmm_xmm(0, 1); asm.setcc(0x4, Reg::RAX); asm.setcc(0xB, Reg::RCX); asm.and_reg_reg(Reg::RAX, Reg::RCX); asm.movzx_r8_mem_base_idx(Reg::RAX, Reg::RAX, Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::F64_EQ { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.movq_xmm_reg(1, Reg::RCX); asm.movq_xmm_reg(0, Reg::RAX); asm.ucomisd_xmm_xmm(0, 1); asm.setcc(0x4, Reg::RAX); asm.setcc(0xB, Reg::RCX); asm.and_reg_reg(Reg::RAX, Reg::RCX); asm.movzx_r8_mem_base_idx(Reg::RAX, Reg::RAX, Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::F32_NE { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.movq_xmm_reg(1, Reg::RCX); asm.movq_xmm_reg(0, Reg::RAX); asm.ucomiss_xmm_xmm(0, 1); asm.setcc(0x5, Reg::RAX); asm.setcc(0xA, Reg::RCX); asm.or_reg_reg(Reg::RAX, Reg::RCX); asm.movzx_r8_mem_base_idx(Reg::RAX, Reg::RAX, Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::F64_NE { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.movq_xmm_reg(1, Reg::RCX); asm.movq_xmm_reg(0, Reg::RAX); asm.ucomisd_xmm_xmm(0, 1); asm.setcc(0x5, Reg::RAX); asm.setcc(0xA, Reg::RCX); asm.or_reg_reg(Reg::RAX, Reg::RCX); asm.movzx_r8_mem_base_idx(Reg::RAX, Reg::RAX, Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::F32_LT { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.movq_xmm_reg(1, Reg::RCX); asm.movq_xmm_reg(0, Reg::RAX); asm.ucomiss_xmm_xmm(0, 1); asm.setcc(0x2, Reg::RAX); asm.setcc(0xB, Reg::RCX); asm.and_reg_reg(Reg::RAX, Reg::RCX); asm.movzx_r8_mem_base_idx(Reg::RAX, Reg::RAX, Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::F64_LT { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.movq_xmm_reg(1, Reg::RCX); asm.movq_xmm_reg(0, Reg::RAX); asm.ucomisd_xmm_xmm(0, 1); asm.setcc(0x2, Reg::RAX); asm.setcc(0xB, Reg::RCX); asm.and_reg_reg(Reg::RAX, Reg::RCX); asm.movzx_r8_mem_base_idx(Reg::RAX, Reg::RAX, Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::F32_GT { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.movq_xmm_reg(1, Reg::RCX); asm.movq_xmm_reg(0, Reg::RAX); asm.ucomiss_xmm_xmm(0, 1); asm.setcc(0x7, Reg::RAX); asm.movzx_r8_mem_base_idx(Reg::RAX, Reg::RAX, Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::F64_GT { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.movq_xmm_reg(1, Reg::RCX); asm.movq_xmm_reg(0, Reg::RAX); asm.ucomisd_xmm_xmm(0, 1); asm.setcc(0x7, Reg::RAX); asm.movzx_r8_mem_base_idx(Reg::RAX, Reg::RAX, Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::F32_LE { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.movq_xmm_reg(1, Reg::RCX); asm.movq_xmm_reg(0, Reg::RAX); asm.ucomiss_xmm_xmm(0, 1); asm.setcc(0x6, Reg::RAX); asm.setcc(0xB, Reg::RCX); asm.and_reg_reg(Reg::RAX, Reg::RCX); asm.movzx_r8_mem_base_idx(Reg::RAX, Reg::RAX, Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::F64_LE { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.movq_xmm_reg(1, Reg::RCX); asm.movq_xmm_reg(0, Reg::RAX); asm.ucomisd_xmm_xmm(0, 1); asm.setcc(0x6, Reg::RAX); asm.setcc(0xB, Reg::RCX); asm.and_reg_reg(Reg::RAX, Reg::RCX); asm.movzx_r8_mem_base_idx(Reg::RAX, Reg::RAX, Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::F32_GE { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.movq_xmm_reg(1, Reg::RCX); asm.movq_xmm_reg(0, Reg::RAX); asm.ucomiss_xmm_xmm(0, 1); asm.setcc(0x3, Reg::RAX); asm.movzx_r8_mem_base_idx(Reg::RAX, Reg::RAX, Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::F64_GE { asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RAX); asm.movq_xmm_reg(1, Reg::RCX); asm.movq_xmm_reg(0, Reg::RAX); asm.ucomisd_xmm_xmm(0, 1); asm.setcc(0x3, Reg::RAX); asm.movzx_r8_mem_base_idx(Reg::RAX, Reg::RAX, Reg::RAX, 0); asm.push_reg(Reg::RAX); }
            }
            // Unary Ops: Pop 1, Push 1 -> Net 0
            opcode::I32_CLZ | opcode::I64_CLZ | opcode::I32_CTZ | opcode::I64_CTZ | opcode::I32_POPCNT | opcode::I64_POPCNT |
            opcode::F32_ABS | opcode::F32_NEG | opcode::F32_CEIL | opcode::F32_FLOOR | opcode::F32_TRUNC | opcode::F32_NEAREST | opcode::F32_SQRT |
            opcode::F64_ABS | opcode::F64_NEG | opcode::F64_CEIL | opcode::F64_FLOOR | opcode::F64_TRUNC | opcode::F64_NEAREST | opcode::F64_SQRT |
            opcode::I32_WRAP_I64 | opcode::I64_EXTEND_I32_S | opcode::I64_EXTEND_I32_U |
            opcode::I32_TRUNC_F32_S | opcode::I32_TRUNC_F32_U | opcode::I32_TRUNC_F64_S | opcode::I32_TRUNC_F64_U |
            opcode::I64_TRUNC_F32_S | opcode::I64_TRUNC_F32_U | opcode::I64_TRUNC_F64_S | opcode::I64_TRUNC_F64_U |
            opcode::F32_CONVERT_I32_S | opcode::F32_CONVERT_I32_U | opcode::F32_CONVERT_I64_S | opcode::F32_CONVERT_I64_U |
            opcode::F64_CONVERT_I32_S | opcode::F64_CONVERT_I32_U | opcode::F64_CONVERT_I64_S | opcode::F64_CONVERT_I64_U |
            opcode::F32_DEMOTE_F64 | opcode::F64_PROMOTE_F32 |
            opcode::I32_EQZ | opcode::I64_EQZ | 
            opcode::I32_EXTEND8_S | opcode::I32_EXTEND16_S | opcode::I64_EXTEND8_S | opcode::I64_EXTEND16_S | opcode::I64_EXTEND32_S
            => {
                if opcode == opcode::I32_CLZ { asm.pop_reg(Reg::RAX); asm.lzcnt_r32_r32(Reg::RAX, Reg::RAX); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::I64_CLZ { asm.pop_reg(Reg::RAX); asm.lzcnt_reg_reg(Reg::RAX, Reg::RAX); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::I32_CTZ { asm.pop_reg(Reg::RAX); asm.tzcnt_r32_r32(Reg::RAX, Reg::RAX); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::I64_CTZ { asm.pop_reg(Reg::RAX); asm.tzcnt_reg_reg(Reg::RAX, Reg::RAX); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::I32_POPCNT { asm.pop_reg(Reg::RAX); asm.popcnt_r32_r32(Reg::RAX, Reg::RAX); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::I64_POPCNT { asm.pop_reg(Reg::RAX); asm.popcnt_reg_reg(Reg::RAX, Reg::RAX); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::I32_EQZ | opcode::I64_EQZ { asm.pop_reg(Reg::RAX); asm.mov_reg_imm64(Reg::RCX, 0); asm.cmp_reg_reg(Reg::RAX, Reg::RCX); asm.xor_reg_reg(Reg::RBX, Reg::RBX); asm.setcc(0x4, Reg::RBX); asm.push_reg(Reg::RBX); }
                else if opcode == opcode::I32_WRAP_I64 { asm.pop_reg(Reg::RAX); asm.mov_reg_imm64(Reg::RCX, 32); asm.shl_reg_cl(Reg::RAX); asm.shr_reg_cl(Reg::RAX); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::I64_EXTEND_I32_S { asm.pop_reg(Reg::RAX); asm.mov_reg_imm64(Reg::RCX, 32); asm.shl_reg_cl(Reg::RAX); asm.sar_reg_cl(Reg::RAX); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::I64_EXTEND_I32_U { asm.pop_reg(Reg::RAX); asm.mov_reg_imm64(Reg::RCX, 32); asm.shl_reg_cl(Reg::RAX); asm.shr_reg_cl(Reg::RAX); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::I32_TRUNC_F32_S { asm.pop_reg(Reg::RAX); asm.movq_xmm_reg(0, Reg::RAX); asm.cvttss2si_r32_xmm(Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::I32_TRUNC_F64_S { asm.pop_reg(Reg::RAX); asm.movq_xmm_reg(0, Reg::RAX); asm.cvttsd2si_r32_xmm(Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::I64_TRUNC_F32_S { asm.pop_reg(Reg::RAX); asm.movq_xmm_reg(0, Reg::RAX); asm.cvttss2si_reg_xmm(Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::I64_TRUNC_F64_S { asm.pop_reg(Reg::RAX); asm.movq_xmm_reg(0, Reg::RAX); asm.cvttsd2si_reg_xmm(Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::F32_CONVERT_I32_S { asm.pop_reg(Reg::RAX); asm.cvtsi2ss_xmm_r32(0, Reg::RAX); asm.movd_r32_xmm(Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::F32_CONVERT_I64_S { asm.pop_reg(Reg::RAX); asm.cvtsi2ss_xmm_reg(0, Reg::RAX); asm.movd_r32_xmm(Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::F64_CONVERT_I32_S { asm.pop_reg(Reg::RAX); asm.cvtsi2sd_xmm_r32(0, Reg::RAX); asm.movq_reg_xmm(Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::F64_CONVERT_I64_S { asm.pop_reg(Reg::RAX); asm.cvtsi2sd_xmm_reg(0, Reg::RAX); asm.movq_reg_xmm(Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::F32_DEMOTE_F64 { asm.pop_reg(Reg::RAX); asm.movq_xmm_reg(0, Reg::RAX); asm.cvtsd2ss_xmm_xmm(0, 0); asm.movd_r32_xmm(Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::F64_PROMOTE_F32 { asm.pop_reg(Reg::RAX); asm.movd_xmm_r32(0, Reg::RAX); asm.cvtss2sd_xmm_xmm(0, 0); asm.movq_reg_xmm(Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::F32_ABS { asm.pop_reg(Reg::RAX); asm.mov_reg_imm64(Reg::RCX, 0x7FFFFFFF); asm.movd_xmm_r32(1, Reg::RCX); asm.movd_xmm_r32(0, Reg::RAX); asm.andps_xmm_xmm(0, 1); asm.movd_r32_xmm(Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::F64_ABS { asm.pop_reg(Reg::RAX); asm.mov_reg_imm64(Reg::RCX, 0x7FFFFFFFFFFFFFFF); asm.movq_xmm_reg(1, Reg::RCX); asm.movq_xmm_reg(0, Reg::RAX); asm.andps_xmm_xmm(0, 1); asm.movq_reg_xmm(Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::F32_NEG { asm.pop_reg(Reg::RAX); asm.mov_reg_imm64(Reg::RCX, 0x80000000); asm.movd_xmm_r32(1, Reg::RCX); asm.movd_xmm_r32(0, Reg::RAX); asm.xorps_xmm_xmm(0, 1); asm.movd_r32_xmm(Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::F64_NEG { asm.pop_reg(Reg::RAX); asm.mov_reg_imm64(Reg::RCX, 0x8000000000000000); asm.movq_xmm_reg(1, Reg::RCX); asm.movq_xmm_reg(0, Reg::RAX); asm.xorps_xmm_xmm(0, 1); asm.movq_reg_xmm(Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::F32_SQRT { asm.pop_reg(Reg::RAX); asm.movq_xmm_reg(0, Reg::RAX); asm.sqrtss_xmm_xmm(0, 0); asm.movq_reg_xmm(Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::F64_SQRT { asm.pop_reg(Reg::RAX); asm.movq_xmm_reg(0, Reg::RAX); asm.sqrtsd_xmm_xmm(0, 0); asm.movq_reg_xmm(Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::F32_CEIL { asm.pop_reg(Reg::RAX); asm.movq_xmm_reg(0, Reg::RAX); asm.roundss_xmm_xmm(0, 0, 2); asm.movq_reg_xmm(Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::F64_CEIL { asm.pop_reg(Reg::RAX); asm.movq_xmm_reg(0, Reg::RAX); asm.roundsd_xmm_xmm(0, 0, 2); asm.movq_reg_xmm(Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::F32_FLOOR { asm.pop_reg(Reg::RAX); asm.movq_xmm_reg(0, Reg::RAX); asm.roundss_xmm_xmm(0, 0, 1); asm.movq_reg_xmm(Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::F64_FLOOR { asm.pop_reg(Reg::RAX); asm.movq_xmm_reg(0, Reg::RAX); asm.roundsd_xmm_xmm(0, 0, 1); asm.movq_reg_xmm(Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::F32_TRUNC { asm.pop_reg(Reg::RAX); asm.movq_xmm_reg(0, Reg::RAX); asm.roundss_xmm_xmm(0, 0, 3); asm.movq_reg_xmm(Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::F64_TRUNC { asm.pop_reg(Reg::RAX); asm.movq_xmm_reg(0, Reg::RAX); asm.roundsd_xmm_xmm(0, 0, 3); asm.movq_reg_xmm(Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::F32_NEAREST { asm.pop_reg(Reg::RAX); asm.movq_xmm_reg(0, Reg::RAX); asm.roundss_xmm_xmm(0, 0, 0); asm.movq_reg_xmm(Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::F64_NEAREST { asm.pop_reg(Reg::RAX); asm.movq_xmm_reg(0, Reg::RAX); asm.roundsd_xmm_xmm(0, 0, 0); asm.movq_reg_xmm(Reg::RAX, 0); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::I32_EXTEND8_S | opcode::I64_EXTEND8_S { asm.pop_reg(Reg::RAX); asm.mov_reg_imm64(Reg::RCX, 56); asm.shl_reg_cl(Reg::RAX); asm.sar_reg_cl(Reg::RAX); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::I32_EXTEND16_S | opcode::I64_EXTEND16_S { asm.pop_reg(Reg::RAX); asm.mov_reg_imm64(Reg::RCX, 48); asm.shl_reg_cl(Reg::RAX); asm.sar_reg_cl(Reg::RAX); asm.push_reg(Reg::RAX); }
                else if opcode == opcode::I64_EXTEND32_S { asm.pop_reg(Reg::RAX); asm.mov_reg_imm64(Reg::RCX, 32); asm.shl_reg_cl(Reg::RAX); asm.sar_reg_cl(Reg::RAX); asm.push_reg(Reg::RAX); }
                else { } 
            }
            opcode::I32_LOAD | opcode::I64_LOAD | opcode::F32_LOAD | opcode::F64_LOAD |
            opcode::I32_LOAD8_S | opcode::I32_LOAD8_U | opcode::I32_LOAD16_S | opcode::I32_LOAD16_U |
            opcode::I64_LOAD8_S | opcode::I64_LOAD8_U | opcode::I64_LOAD16_S | opcode::I64_LOAD16_U |
            opcode::I64_LOAD32_S | opcode::I64_LOAD32_U 
            => {
                let memarg = MemArg::read(&mut wasm)?;
                asm.pop_reg(Reg::RBX);
                // All loads pop 1 (addr) push 1 (val) -> Net 0
                if opcode == opcode::I32_LOAD { asm.mov_r32_mem_base_idx(Reg::RAX, Reg::R15, Reg::RBX, memarg.offset as i32); }
                else if opcode == opcode::I64_LOAD { asm.mov_r64_mem_base_idx(Reg::RAX, Reg::R15, Reg::RBX, memarg.offset as i32); }
                else if opcode == opcode::I32_LOAD8_S { asm.movsx_r8_mem_base_idx(Reg::RAX, Reg::R15, Reg::RBX, memarg.offset as i32); }
                else if opcode == opcode::I32_LOAD8_U { asm.movzx_r8_mem_base_idx(Reg::RAX, Reg::R15, Reg::RBX, memarg.offset as i32); }
                else if opcode == opcode::I32_LOAD16_S { asm.movsx_r16_mem_base_idx(Reg::RAX, Reg::R15, Reg::RBX, memarg.offset as i32); }
                else if opcode == opcode::I32_LOAD16_U { asm.movzx_r16_mem_base_idx(Reg::RAX, Reg::R15, Reg::RBX, memarg.offset as i32); }
                else if opcode == opcode::I64_LOAD8_S { asm.movsx_r8_mem_base_idx(Reg::RAX, Reg::R15, Reg::RBX, memarg.offset as i32); }
                else if opcode == opcode::I64_LOAD8_U { asm.movzx_r8_mem_base_idx(Reg::RAX, Reg::R15, Reg::RBX, memarg.offset as i32); }
                else if opcode == opcode::I64_LOAD16_S { asm.movsx_r16_mem_base_idx(Reg::RAX, Reg::R15, Reg::RBX, memarg.offset as i32); }
                else if opcode == opcode::I64_LOAD16_U { asm.movzx_r16_mem_base_idx(Reg::RAX, Reg::R15, Reg::RBX, memarg.offset as i32); }
                else if opcode == opcode::I64_LOAD32_S { asm.movsxd_r64_mem_base_idx(Reg::RAX, Reg::R15, Reg::RBX, memarg.offset as i32); }
                else if opcode == opcode::I64_LOAD32_U { asm.mov_r32_mem_base_idx(Reg::RAX, Reg::R15, Reg::RBX, memarg.offset as i32); }
                else if opcode == opcode::F32_LOAD { asm.mov_r32_mem_base_idx(Reg::RAX, Reg::R15, Reg::RBX, memarg.offset as i32); }
                else if opcode == opcode::F64_LOAD { asm.mov_r64_mem_base_idx(Reg::RAX, Reg::R15, Reg::RBX, memarg.offset as i32); }
                asm.push_reg(Reg::RAX);
            }
            opcode::I32_STORE | opcode::I64_STORE | opcode::F32_STORE | opcode::F64_STORE |
            opcode::I32_STORE8 | opcode::I32_STORE16 | opcode::I64_STORE8 | opcode::I64_STORE16 | opcode::I64_STORE32 
            => {
                let memarg = MemArg::read(&mut wasm)?;
                asm.pop_reg(Reg::RAX);
                asm.pop_reg(Reg::RBX);
                stack_depth -= 2;
                if opcode == opcode::I32_STORE { asm.mov_mem_base_idx_r32(Reg::R15, Reg::RBX, memarg.offset as i32, Reg::RAX); }
                else if opcode == opcode::I64_STORE { asm.mov_mem_base_idx_r64(Reg::R15, Reg::RBX, memarg.offset as i32, Reg::RAX); }
                else if opcode == opcode::F32_STORE { asm.mov_mem_base_idx_r32(Reg::R15, Reg::RBX, memarg.offset as i32, Reg::RAX); }
                else if opcode == opcode::F64_STORE { asm.mov_mem_base_idx_r64(Reg::R15, Reg::RBX, memarg.offset as i32, Reg::RAX); }
                else if opcode == opcode::I32_STORE8 | opcode::I64_STORE8 { asm.mov_mem_base_idx_r8(Reg::R15, Reg::RBX, memarg.offset as i32, Reg::RAX); }
                else if opcode == opcode::I32_STORE16 | opcode::I64_STORE16 { asm.mov_mem_base_idx_r16(Reg::R15, Reg::RBX, memarg.offset as i32, Reg::RAX); }
                else if opcode == opcode::I64_STORE32 { asm.mov_mem_base_idx_r32(Reg::R15, Reg::RBX, memarg.offset as i32, Reg::RAX); }
            }
            opcode::LOCAL_GET => {
                let idx = wasm.read_var_u32()?;
                let offset = -8 * ((idx as i32) + 1);
                asm.mov_reg_mem(Reg::RAX, Reg::RBP, offset);
                asm.push_reg(Reg::RAX);
                stack_depth += 1;
            }
            opcode::LOCAL_SET => {
                let idx = wasm.read_var_u32()?;
                let offset = -8 * ((idx as i32) + 1);
                asm.pop_reg(Reg::RAX);
                asm.mov_mem_reg(Reg::RBP, offset, Reg::RAX);
                stack_depth -= 1;
            }
            opcode::LOCAL_TEE => {
                let idx = wasm.read_var_u32()?;
                let offset = -8 * ((idx as i32) + 1);
                asm.mov_reg_mem(Reg::RAX, Reg::RSP, 0);
                asm.mov_mem_reg(Reg::RBP, offset, Reg::RAX);
            }
            opcode::DROP => { asm.pop_reg(Reg::RAX); stack_depth -= 1; }
            opcode::REF_NULL => { let _ = RefType::read(&mut wasm)?; asm.mov_reg_imm64(Reg::RAX, 0); asm.push_reg(Reg::RAX); stack_depth += 1; }
            opcode::REF_FUNC => { let func_idx = wasm.read_var_u32()?; let func_addr = module.func_addrs[func_idx as usize]; asm.mov_reg_imm64(Reg::RAX, func_addr as u64); asm.push_reg(Reg::RAX); stack_depth += 1; }
            opcode::REF_IS_NULL => {
                asm.pop_reg(Reg::RAX);
                asm.mov_reg_imm64(Reg::RCX, 0);
                asm.cmp_reg_reg(Reg::RAX, Reg::RCX);
                asm.xor_reg_reg(Reg::RBX, Reg::RBX);
                asm.setcc(0x4, Reg::RBX); // SETE
                asm.push_reg(Reg::RBX);
            }
            opcode::FC_EXTENSIONS => {
                let sub_opcode = wasm.read_var_u32()?;
                use crate::wasm::core::reader::types::opcode::fc_extensions::*;
                match sub_opcode {
                    I32_TRUNC_SAT_F32_S | I32_TRUNC_SAT_F32_U | I32_TRUNC_SAT_F64_S | I32_TRUNC_SAT_F64_U |
                    I64_TRUNC_SAT_F32_S | I64_TRUNC_SAT_F32_U | I64_TRUNC_SAT_F64_S | I64_TRUNC_SAT_F64_U 
                    => {
                        asm.pop_reg(Reg::RAX);
                        if sub_opcode == I32_TRUNC_SAT_F32_S { asm.movd_xmm_r32(0, Reg::RAX); let t = crate::wasm::aot::trampoline::aot_i32_trunc_sat_f32_s as usize; asm.mov_reg_imm64(Reg::RAX, t as u64); }
                        else if sub_opcode == I32_TRUNC_SAT_F32_U { asm.movd_xmm_r32(0, Reg::RAX); let t = crate::wasm::aot::trampoline::aot_i32_trunc_sat_f32_u as usize; asm.mov_reg_imm64(Reg::RAX, t as u64); }
                        else if sub_opcode == I32_TRUNC_SAT_F64_S { asm.movq_xmm_reg(0, Reg::RAX); let t = crate::wasm::aot::trampoline::aot_i32_trunc_sat_f64_s as usize; asm.mov_reg_imm64(Reg::RAX, t as u64); }
                        else if sub_opcode == I32_TRUNC_SAT_F64_U { asm.movq_xmm_reg(0, Reg::RAX); let t = crate::wasm::aot::trampoline::aot_i32_trunc_sat_f64_u as usize; asm.mov_reg_imm64(Reg::RAX, t as u64); }
                        else if sub_opcode == I64_TRUNC_SAT_F32_S { asm.movd_xmm_r32(0, Reg::RAX); let t = crate::wasm::aot::trampoline::aot_i64_trunc_sat_f32_s as usize; asm.mov_reg_imm64(Reg::RAX, t as u64); }
                        else if sub_opcode == I64_TRUNC_SAT_F32_U { asm.movd_xmm_r32(0, Reg::RAX); let t = crate::wasm::aot::trampoline::aot_i64_trunc_sat_f32_u as usize; asm.mov_reg_imm64(Reg::RAX, t as u64); }
                        else if sub_opcode == I64_TRUNC_SAT_F64_S { asm.movq_xmm_reg(0, Reg::RAX); let t = crate::wasm::aot::trampoline::aot_i64_trunc_sat_f64_s as usize; asm.mov_reg_imm64(Reg::RAX, t as u64); }
                        else { asm.movq_xmm_reg(0, Reg::RAX); let t = crate::wasm::aot::trampoline::aot_i64_trunc_sat_f64_u as usize; asm.mov_reg_imm64(Reg::RAX, t as u64); }
                        asm.call_reg(Reg::RAX);
                        asm.push_reg(Reg::RAX);
                    }
                    MEMORY_COPY => {
                        let dst_mem = wasm.read_u8()?;
                        let src_mem = wasm.read_u8()?;
                        asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RDX); asm.pop_reg(Reg::RSI); stack_depth -= 3;
                        asm.mov_reg_reg(Reg::RDI, Reg::R14); 
                        asm.mov_reg_imm64(Reg::R8, dst_mem as u64);
                        asm.mov_reg_imm64(Reg::R9, src_mem as u64);
                        asm.mov_reg_imm64(Reg::RAX, module_addr as u64);
                        asm.push_reg(Reg::RAX);
                        let t = crate::wasm::aot::trampoline::aot_memory_copy::<T> as usize;
                        asm.mov_reg_imm64(Reg::RAX, t as u64);
                        asm.call_reg(Reg::RAX);
                        asm.pop_reg(Reg::RAX);
                    }
                    MEMORY_FILL => {
                        let mem_idx = wasm.read_u8()?;
                        asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RDX); asm.pop_reg(Reg::RSI); stack_depth -= 3;
                        asm.mov_reg_reg(Reg::RDI, Reg::R14);
                        asm.mov_reg_imm64(Reg::R8, mem_idx as u64);
                        asm.mov_reg_imm64(Reg::R9, module_addr as u64);
                        let t = crate::wasm::aot::trampoline::aot_memory_fill::<T> as usize;
                        asm.mov_reg_imm64(Reg::RAX, t as u64);
                        asm.call_reg(Reg::RAX);
                    }
                    MEMORY_INIT => {
                        let data_idx = wasm.read_var_u32()?;
                        let mem_idx = wasm.read_u8()?;
                        asm.pop_reg(Reg::RCX); asm.pop_reg(Reg::RDX); asm.pop_reg(Reg::RSI); stack_depth -= 3;
                        asm.mov_reg_reg(Reg::RDI, Reg::R14);
                        asm.mov_reg_imm64(Reg::R8, data_idx as u64);
                        asm.mov_reg_imm64(Reg::R9, mem_idx as u64);
                        asm.mov_reg_imm64(Reg::RAX, module_addr as u64);
                        asm.push_reg(Reg::RAX);
                        let t = crate::wasm::aot::trampoline::aot_memory_init::<T> as usize;
                        asm.mov_reg_imm64(Reg::RAX, t as u64);
                        asm.call_reg(Reg::RAX);
                        asm.pop_reg(Reg::RAX);
                    }
                    DATA_DROP => {
                        let data_idx = wasm.read_var_u32()?;
                        asm.mov_reg_reg(Reg::RDI, Reg::R14);
                        asm.mov_reg_imm64(Reg::RSI, data_idx as u64);
                        asm.mov_reg_imm64(Reg::RDX, module_addr as u64);
                        let t = crate::wasm::aot::trampoline::aot_data_drop::<T> as usize;
                        asm.mov_reg_imm64(Reg::RAX, t as u64);
                        asm.call_reg(Reg::RAX);
                    }
                    _ => return Err(ValidationError::InvalidMultiByteInstr(opcode, sub_opcode)),
                }
            }
            _ => { return Err(ValidationError::InvalidInstr(opcode)); }
        }
    }

    let result_count = func.function_type.returns.valtypes.len();
    for i in (0..result_count).rev() {
        asm.pop_reg(Reg::RAX);
        asm.mov_mem_reg(Reg::R13, (i * 8) as i32, Reg::RAX);
    }

    // Restore callee-saved registers using RBP frame pointer
    // This is safe even if RSP is not balanced (e.g. after a branch to end)
    // Pushes: RBP, R15, R14, R13, RBX -> offsets -8, -16, -24, -32, -40
    asm.mov_reg_mem(Reg::RBX, Reg::RBP, -32);
    asm.mov_reg_mem(Reg::R13, Reg::RBP, -24);
    asm.mov_reg_mem(Reg::R14, Reg::RBP, -16);
    asm.mov_reg_mem(Reg::R15, Reg::RBP, -8);
    asm.mov_reg_reg(Reg::RSP, Reg::RBP);
    asm.pop_reg(Reg::RBP);
    asm.ret();

    Ok(asm.buf)
}
