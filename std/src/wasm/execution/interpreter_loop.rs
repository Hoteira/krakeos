use super::{little_endian::LittleEndianBytes, store::Store};
use crate::rust_alloc::{vec, vec::Vec};
use crate::unreachable_validated;
use crate::wasm::execution::config::Config;
use crate::wasm::{
    core::indices::{DataIdx, ElemIdx, GlobalIdx},
    core::{
        indices::{FuncIdx, LabelIdx, LocalIdx, TableIdx, TypeIdx},
        reader::{
            types::{memarg::MemArg, BlockType},
            WasmReadable, WasmReader,
        },
        sidetable::Sidetable,
    },
    execution::assert_validated::UnwrapValidatedExt,
    execution::resumable::Resumable,
    execution::store::addrs::{AddrVec, DataAddr, ElemAddr, MemAddr, ModuleAddr, TableAddr},
    execution::store::instances::{FuncInst, ModuleInst},
    execution::store::HaltExecutionError,
    execution::value::{self, Ref, F32, F64},
    execution::value_stack::Stack,
    RefType, RuntimeError, TrapError, ValType, Value,
};
use core::{
    num::NonZeroU32,
    {
        array,
        iter::zip,
        ops::Neg,
    },
};
pub fn run<T: Config>(
    resumable: &mut Resumable,
    store: &mut Store<T>,
) -> Result<Option<NonZeroU32>, RuntimeError> {
    let stack = &mut resumable.stack;
    let mut current_func_addr = resumable.current_func_addr;
    let pc = resumable.pc;
    let mut stp = resumable.stp;
    let func_inst = store.functions.get(current_func_addr);
    let FuncInst::WasmFunc(wasm_func_inst) = &func_inst else {
        unreachable!(
            "the interpreter loop shall only be executed with native wasm functions as root call"
        );
    };
    let mut current_module = wasm_func_inst.module_addr;
    let wasm = &mut WasmReader::new(store.modules.get(current_module).wasm_bytecode);
    let mut current_function_end_marker =
        wasm_func_inst.code_expr.from() + wasm_func_inst.code_expr.len();
    wasm.pc = pc;
    use crate::wasm::core::reader::types::opcode::*;
    loop {
        store
            .user_data
            .instruction_hook(store.modules.get(current_module).wasm_bytecode, wasm.pc);
        let prev_pc = wasm.pc;
        macro_rules! decrement_fuel {
            ($cost:expr) => {
                if let Some(fuel) = &mut resumable.maybe_fuel {
                    if *fuel >= $cost {
                        *fuel -= $cost;
                    } else {
                        resumable.current_func_addr = current_func_addr;
                        resumable.pc = prev_pc;  
                        resumable.stp = stp;
                        return Ok(NonZeroU32::new($cost-*fuel));
                    }
                }
            }
        }
        let first_instr_byte = match wasm.read_u8() {
            Ok(b) => b,
            Err(e) => {
                crate::debugln!("WASM Interpreter error (fetch) at PC {:#x}: {:?}", wasm.pc, e);
                return Err(TrapError::ReachedUnreachable.into());
            }
        };
        #[cfg(debug_assertions)]
        trace!(
            "Executing instruction {}",
            crate::wasm::core::reader::types::opcode::opcode_byte_to_str(first_instr_byte)
        );
        match first_instr_byte {
            NOP => {
                decrement_fuel!(T::get_flat_cost(NOP));
                trace!("Instruction: NOP");
            }
            END => {
                if wasm.pc != current_function_end_marker {
                    continue;
                }
                let (maybe_return_func_addr, maybe_return_address, maybe_return_stp) =
                    stack.pop_call_frame();
                if stack.call_frame_count() == 0 {
                    break;
                }
                trace!("end of function reached, returning to previous call frame");
                current_func_addr = maybe_return_func_addr;
                let FuncInst::WasmFunc(current_wasm_func_inst) =
                    store.functions.get(current_func_addr)
                else {
                    unreachable!("function addresses on the stack always correspond to native wasm functions")
                };
                current_module = current_wasm_func_inst.module_addr;
                wasm.full_wasm_binary = store.modules.get(current_module).wasm_bytecode;
                wasm.pc = maybe_return_address;
                stp = maybe_return_stp;
                current_function_end_marker = current_wasm_func_inst.code_expr.from()
                    + current_wasm_func_inst.code_expr.len();
                trace!("Instruction: END");
            }
            IF => {
                decrement_fuel!(T::get_flat_cost(IF));
                wasm.read_var_u32().unwrap_validated();
                let test_val: i32 = stack.pop_value().try_into().unwrap_validated();
                if test_val != 0 {
                    stp += 1;
                } else {
                    do_sidetable_control_transfer::<T>(wasm, stack, &mut stp, &store.modules.get(current_module).sidetable)?;
                }
                trace!("Instruction: IF");
            }
            ELSE => {
                decrement_fuel!(T::get_flat_cost(ELSE));
                do_sidetable_control_transfer::<T>(wasm, stack, &mut stp, &store.modules.get(current_module).sidetable)?;
            }
            BR_IF => {
                decrement_fuel!(T::get_flat_cost(BR_IF));
                wasm.read_var_u32().unwrap_validated();
                let test_val: i32 = stack.pop_value().try_into().unwrap_validated();
                if test_val != 0 {
                    do_sidetable_control_transfer::<T>(wasm, stack, &mut stp, &store.modules.get(current_module).sidetable)?;
                } else {
                    stp += 1;
                }
                trace!("Instruction: BR_IF");
            }
            BR_TABLE => {
                decrement_fuel!(T::get_flat_cost(BR_TABLE));
                let label_vec = wasm
                    .read_vec(|wasm| wasm.read_var_u32().map(|v| v as LabelIdx))
                    .unwrap_validated();
                wasm.read_var_u32().unwrap_validated();
                let case_val_i32: i32 = stack.pop_value().try_into().unwrap_validated();
                let case_val = case_val_i32 as usize;
                if case_val >= label_vec.len() {
                    stp += label_vec.len();
                } else {
                    stp += case_val;
                }
                do_sidetable_control_transfer::<T>(wasm, stack, &mut stp, &store.modules.get(current_module).sidetable)?;
            }
            BR => {
                decrement_fuel!(T::get_flat_cost(BR));
                wasm.read_var_u32().unwrap_validated();
                do_sidetable_control_transfer::<T>(wasm, stack, &mut stp, &store.modules.get(current_module).sidetable)?;
            }
            BLOCK => {
                decrement_fuel!(T::get_flat_cost(BLOCK));
                BlockType::read(wasm).unwrap_validated();
            }
            LOOP => {
                decrement_fuel!(T::get_flat_cost(LOOP));
                BlockType::read(wasm).unwrap_validated();
            }
            RETURN => {
                decrement_fuel!(T::get_flat_cost(RETURN));
                do_sidetable_control_transfer::<T>(wasm, stack, &mut stp, &store.modules.get(current_module).sidetable)?;
            }
            CALL => {
                decrement_fuel!(T::get_flat_cost(CALL));
                let local_func_idx = wasm.read_var_u32().unwrap_validated() as FuncIdx;
                let func_to_call_addr = {
                    let FuncInst::WasmFunc(current_wasm_func_inst) =
                        store.functions.get(current_func_addr)
                    else {
                        unreachable!()
                    };
                    store.modules.get(current_wasm_func_inst.module_addr).func_addrs[local_func_idx]
                };
                let func_to_call_ty = store.functions.get(func_to_call_addr).ty();
                trace!("Instruction: call [{func_to_call_addr:?}]");
                match store.functions.get(func_to_call_addr) {
                    FuncInst::HostFunc(host_func_to_call_inst) => {
                        let hostcode = host_func_to_call_inst.hostcode;
                        let params = stack
                            .pop_tail_iter(func_to_call_ty.params.valtypes.len())
                            .collect();
                        store.caller_module = Some(current_module);
                        let returns = hostcode(store, params);
                        store.caller_module = None;
                        let returns = returns.map_err(|HaltExecutionError(code)| {
                            RuntimeError::HostFunctionHaltedExecution(code)
                        })?;
                        if returns.len() != func_to_call_ty.returns.valtypes.len() {
                            return Err(RuntimeError::HostFunctionSignatureMismatch);
                        }
                        for (value, ty) in zip(returns, func_to_call_ty.returns.valtypes) {
                            if value.to_ty() != ty {
                                return Err(RuntimeError::HostFunctionSignatureMismatch);
                            }
                            stack.push_value::<T>(value)?;
                        }
                    }
                    FuncInst::WasmFunc(wasm_func_to_call_inst) => {
                        let remaining_locals = &wasm_func_to_call_inst.locals;
                        stack.push_call_frame::<T>(
                            current_func_addr,
                            &func_to_call_ty,
                            remaining_locals,
                            wasm.pc,
                            stp,
                        )?;
                        current_func_addr = func_to_call_addr;
                        current_module = wasm_func_to_call_inst.module_addr;
                        wasm.full_wasm_binary = store.modules.get(current_module).wasm_bytecode;
                        wasm.move_start_to(wasm_func_to_call_inst.code_expr)
                            .expect("code expression spans to always be valid");
                        stp = wasm_func_to_call_inst.stp;
                        current_function_end_marker = wasm_func_to_call_inst.code_expr.from()
                            + wasm_func_to_call_inst.code_expr.len();
                    }
                    FuncInst::AotFunc(aot_func_inst) => {
                        let code_ptr = aot_func_inst.code.ptr();
                        let params = stack
                            .pop_tail_iter(func_to_call_ty.params.valtypes.len())
                            .collect::<Vec<_>>();
                        
                        let mut raw_params: Vec<u64> = params.iter().map(|v| match v {
                            Value::I32(i) => *i as u64,
                            Value::I64(i) => *i,
                            Value::F32(f) => f.to_bits() as u64,
                            Value::F64(f) => f.to_bits(),
                            Value::Ref(r) => match r {
                                Ref::Null(_) => 0,
                                Ref::Func(addr) => *addr as u64,
                                Ref::Extern(addr) => addr.0 as u64,
                            },
                            Value::V128(_) => 0,
                        }).collect();
                        
                        let func_ptr: extern "C" fn(*mut (), *const u64, *mut u64, u64) = unsafe { core::mem::transmute(code_ptr) };
                        let result_count = func_to_call_ty.returns.valtypes.len();
                        let mut raw_results = vec![0u64; result_count];
                        let mem_base = store.get_wasm_base_ptr() as u64;
                        let store_ptr = store as *mut Store<T> as *mut ();
                        
                        func_ptr(store_ptr, raw_params.as_ptr(), raw_results.as_mut_ptr(), mem_base);
                        
                        for (i, &raw) in raw_results.iter().enumerate() {
                            let ty = func_to_call_ty.returns.valtypes[i];
                            let val = match ty {
                                ValType::NumType(crate::wasm::NumType::I32) => Value::I32(raw as u32),
                                ValType::NumType(crate::wasm::NumType::I64) => Value::I64(raw),
                                ValType::NumType(crate::wasm::NumType::F32) => Value::F32(crate::wasm::execution::value::F32::from_bits(raw as u32)),
                                ValType::NumType(crate::wasm::NumType::F64) => Value::F64(crate::wasm::execution::value::F64::from_bits(raw)),
                                _ => Value::I64(0),
                            };
                            stack.push_value::<T>(val)?;
                        }
                    }
                }
                trace!("Instruction: CALL");
            }
            CALL_INDIRECT => {
                decrement_fuel!(T::get_flat_cost(CALL_INDIRECT));
                let given_type_idx = wasm.read_var_u32().unwrap_validated() as TypeIdx;
                let table_idx = wasm.read_var_u32().unwrap_validated() as TableIdx;
                let table_addr = *store
                    .modules
                    .get(current_module)
                    .table_addrs
                    .get(table_idx)
                    .unwrap_validated();
                let tab = store.tables.get(table_addr);
                let func_ty = store
                    .modules
                    .get(current_module)
                    .types
                    .get(given_type_idx)
                    .unwrap_validated();
                let i: u32 = stack.pop_value().try_into().unwrap_validated();
                let r = tab
                    .elem
                    .get(i as usize)
                    .ok_or_else(|| {
                        crate::debugln!("WASM Trap: TableAccessOutOfBounds (i={}) at PC {:#x}", i, wasm.pc);
                        TrapError::TableAccessOutOfBounds
                    })
                    .and_then(|r| {
                        if matches!(r, Ref::Null(_)) {
                            trace!("table_idx ({table_idx}) --- element index in table ({i})");
                            crate::debugln!("WASM Trap: UninitializedElement at PC {:#x}", wasm.pc);
                            Err(TrapError::UninitializedElement)
                        } else {
                            Ok(r)
                        }
                    })?;
                let func_to_call_addr = match *r {
                    Ref::Func(func_addr) => func_addr,
                    Ref::Null(_) => return Err(TrapError::IndirectCallNullFuncRef.into()),
                    Ref::Extern(_) => unreachable_validated!(),
                };
                let func_to_call_ty = store.functions.get(func_to_call_addr).ty();
                if *func_ty != func_to_call_ty {
                    return Err(TrapError::SignatureMismatch.into());
                }
                trace!("Instruction: call [{func_to_call_addr:?}]");
                match store.functions.get(func_to_call_addr) {
                    FuncInst::HostFunc(host_func_to_call_inst) => {
                        let hostcode = host_func_to_call_inst.hostcode;
                        let params = stack
                            .pop_tail_iter(func_to_call_ty.params.valtypes.len())
                            .collect();
                        store.caller_module = Some(current_module);
                        let returns = hostcode(store, params);
                        store.caller_module = None;
                        let returns = returns.map_err(|HaltExecutionError(code)| {
                            RuntimeError::HostFunctionHaltedExecution(code)
                        })?;
                        if returns.len() != func_to_call_ty.returns.valtypes.len() {
                            return Err(RuntimeError::HostFunctionSignatureMismatch);
                        }
                        for (value, ty) in zip(returns, func_to_call_ty.returns.valtypes) {
                            if value.to_ty() != ty {
                                return Err(RuntimeError::HostFunctionSignatureMismatch);
                            }
                            stack.push_value::<T>(value)?;
                        }
                    }
                    FuncInst::WasmFunc(wasm_func_to_call_inst) => {
                        let remaining_locals = &wasm_func_to_call_inst.locals;
                        stack.push_call_frame::<T>(
                            current_func_addr,
                            &func_to_call_ty,
                            remaining_locals,
                            wasm.pc,
                            stp,
                        )?;
                        current_func_addr = func_to_call_addr;
                        current_module = wasm_func_to_call_inst.module_addr;
                        wasm.full_wasm_binary = store.modules.get(current_module).wasm_bytecode;
                        wasm.move_start_to(wasm_func_to_call_inst.code_expr)
                            .expect("code expression spans to always be valid");
                        stp = wasm_func_to_call_inst.stp;
                        current_function_end_marker = wasm_func_to_call_inst.code_expr.from()
                            + wasm_func_to_call_inst.code_expr.len();
                    }
                    FuncInst::AotFunc(aot_func_inst) => {
                        let code_ptr = aot_func_inst.code.ptr();
                        let params = stack
                            .pop_tail_iter(func_to_call_ty.params.valtypes.len())
                            .collect::<Vec<_>>();
                        
                        let mut raw_params: Vec<u64> = params.iter().map(|v| match v {
                            Value::I32(i) => *i as u64,
                            Value::I64(i) => *i,
                            Value::F32(f) => f.to_bits() as u64,
                            Value::F64(f) => f.to_bits(),
                            Value::Ref(r) => match r {
                                Ref::Null(_) => 0,
                                Ref::Func(addr) => *addr as u64,
                                Ref::Extern(addr) => addr.0 as u64,
                            },
                            Value::V128(_) => 0,
                        }).collect();
                        
                        let func_ptr: extern "C" fn(*mut (), *const u64, *mut u64, u64) = unsafe { core::mem::transmute(code_ptr) };
                        let result_count = func_to_call_ty.returns.valtypes.len();
                        let mut raw_results = vec![0u64; result_count];
                        let mem_base = store.get_wasm_base_ptr() as u64;
                        
                        func_ptr(core::ptr::null_mut(), raw_params.as_ptr(), raw_results.as_mut_ptr(), mem_base);
                        
                        for (i, &raw) in raw_results.iter().enumerate() {
                            let ty = func_to_call_ty.returns.valtypes[i];
                            let val = match ty {
                                ValType::NumType(crate::wasm::NumType::I32) => Value::I32(raw as u32),
                                ValType::NumType(crate::wasm::NumType::I64) => Value::I64(raw),
                                ValType::NumType(crate::wasm::NumType::F32) => Value::F32(crate::wasm::execution::value::F32::from_bits(raw as u32)),
                                ValType::NumType(crate::wasm::NumType::F64) => Value::F64(crate::wasm::execution::value::F64::from_bits(raw)),
                                _ => Value::I64(0),
                            };
                            stack.push_value::<T>(val)?;
                        }
                    }
                }
                trace!("Instruction: CALL_INDIRECT");
            }
            DROP => {
                decrement_fuel!(T::get_flat_cost(DROP));
                stack.pop_value();
                trace!("Instruction: DROP");
            }
            SELECT => {
                decrement_fuel!(T::get_flat_cost(SELECT));
                let test_val: i32 = stack.pop_value().try_into().unwrap_validated();
                let val2 = stack.pop_value();
                let val1 = stack.pop_value();
                if test_val != 0 {
                    stack.push_value::<T>(val1)?;
                } else {
                    stack.push_value::<T>(val2)?;
                }
                trace!("Instruction: SELECT");
            }
            SELECT_T => {
                decrement_fuel!(T::get_flat_cost(SELECT_T));
                let _type_vec = wasm.read_vec(ValType::read).unwrap_validated();
                let test_val: i32 = stack.pop_value().try_into().unwrap_validated();
                let val2 = stack.pop_value();
                let val1 = stack.pop_value();
                if test_val != 0 {
                    stack.push_value::<T>(val1)?;
                } else {
                    stack.push_value::<T>(val2)?;
                }
                trace!("Instruction: SELECT_T");
            }
            LOCAL_GET => {
                decrement_fuel!(T::get_flat_cost(LOCAL_GET));
                let local_idx = wasm.read_var_u32().unwrap_validated() as LocalIdx;
                let value = *stack.get_local(local_idx);
                stack.push_value::<T>(value)?;
                trace!("Instruction: local.get {} [] -> [t]", local_idx);
            }
            LOCAL_SET => {
                decrement_fuel!(T::get_flat_cost(LOCAL_SET));
                let local_idx = wasm.read_var_u32().unwrap_validated() as LocalIdx;
                let value = stack.pop_value();
                *stack.get_local_mut(local_idx) = value;
                trace!("Instruction: local.set {} [t] -> []", local_idx);
            }
            LOCAL_TEE => {
                decrement_fuel!(T::get_flat_cost(LOCAL_TEE));
                let local_idx = wasm.read_var_u32().unwrap_validated() as LocalIdx;
                let value = stack.peek_value().unwrap_validated();
                *stack.get_local_mut(local_idx) = value;
                trace!("Instruction: local.tee {} [t] -> [t]", local_idx);
            }
            GLOBAL_GET => {
                decrement_fuel!(T::get_flat_cost(GLOBAL_GET));
                let global_idx = wasm.read_var_u32().unwrap_validated() as GlobalIdx;
                let global_addr = *store
                    .modules
                    .get(current_module)
                    .global_addrs
                    .get(global_idx)
                    .unwrap_validated();
                let global = store.globals.get(global_addr);
                stack.push_value::<T>(global.value)?;
                trace!(
                    "Instruction: global.get '{}' [<GLOBAL>] -> [{:?}]",
                    global_idx,
                    global.value
                );
            }
            GLOBAL_SET => {
                decrement_fuel!(T::get_flat_cost(GLOBAL_SET));
                let global_idx = wasm.read_var_u32().unwrap_validated() as GlobalIdx;
                let global_addr = *store
                    .modules
                    .get(current_module)
                    .global_addrs
                    .get(global_idx)
                    .unwrap_validated();
                let global = store.globals.get_mut(global_addr);
                global.value = stack.pop_value();
                trace!("Instruction: GLOBAL_SET");
            }
            TABLE_GET => {
                decrement_fuel!(T::get_flat_cost(TABLE_GET));
                let table_idx = wasm.read_var_u32().unwrap_validated() as TableIdx;
                let table_addr = *store
                    .modules
                    .get(current_module)
                    .table_addrs
                    .get(table_idx)
                    .unwrap_validated();
                let tab = store.tables.get(table_addr);
                let i: i32 = stack.pop_value().try_into().unwrap_validated();
                let val = tab
                    .elem
                    .get(i as usize)
                    .ok_or(TrapError::TableOrElementAccessOutOfBounds)?;
                stack.push_value::<T>((*val).into())?;
                trace!(
                    "Instruction: table.get '{}' [{}] -> [{}]",
                    table_idx,
                    i,
                    val
                );
            }
            TABLE_SET => {
                decrement_fuel!(T::get_flat_cost(TABLE_SET));
                let table_idx = wasm.read_var_u32().unwrap_validated() as TableIdx;
                let table_addr = *store
                    .modules
                    .get(current_module)
                    .table_addrs
                    .get(table_idx)
                    .unwrap_validated();
                let tab = store.tables.get_mut(table_addr);
                let val: Ref = stack.pop_value().try_into().unwrap_validated();
                let i: i32 = stack.pop_value().try_into().unwrap_validated();
                tab.elem
                    .get_mut(i as usize)
                    .ok_or_else(|| {
                        crate::debugln!("WASM Trap: TableOrElementAccessOutOfBounds at PC {:#x}", wasm.pc);
                        TrapError::TableOrElementAccessOutOfBounds
                    })
                    .map(|r| *r = val)?;
                trace!(
                    "Instruction: table.set '{}' [{} {}] -> []",
                    table_idx,
                    i,
                    val
                );
            }
            UNREACHABLE => {
                crate::debugln!("WASM Trap: ReachedUnreachable at PC {:#x}", wasm.pc);
                return Err(TrapError::ReachedUnreachable.into());
            }
            I32_LOAD => {
                decrement_fuel!(T::get_flat_cost(I32_LOAD));
                let memarg = MemArg::read(wasm).unwrap_validated();
                let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
                let mem_addr = *store
                    .modules
                    .get(current_module)
                    .mem_addrs
                    .first()
                    .unwrap_validated();
                let mem_inst = store.memories.get(mem_addr);
                let idx = calculate_mem_address(&memarg, relative_address)?;
                let data = mem_inst.mem.load(idx).map_err(|e| {
                    crate::debugln!("WASM Trap: MemoryAccessOutOfBounds (load i32, addr={:#x}) at PC {:#x}", idx, wasm.pc);
                    e
                })?;
                stack.push_value::<T>(Value::I32(data))?;
                trace!("Instruction: i32.load [{relative_address}] -> [{data}]");
            }
            I64_LOAD => {
                decrement_fuel!(T::get_flat_cost(I64_LOAD));
                let memarg = MemArg::read(wasm).unwrap_validated();
                let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
                let mem_addr = *store
                    .modules
                    .get(current_module)
                    .mem_addrs
                    .first()
                    .unwrap_validated();
                let mem = store.memories.get(mem_addr);
                let idx = calculate_mem_address(&memarg, relative_address)?;
                let data = mem.mem.load(idx)?;
                stack.push_value::<T>(Value::I64(data))?;
                trace!("Instruction: i64.load [{relative_address}] -> [{data}]");
            }
            F32_LOAD => {
                decrement_fuel!(T::get_flat_cost(F32_LOAD));
                let memarg = MemArg::read(wasm).unwrap_validated();
                let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
                let mem_addr = *store
                    .modules
                    .get(current_module)
                    .mem_addrs
                    .first()
                    .unwrap_validated();
                let mem = store.memories.get(mem_addr);
                let idx = calculate_mem_address(&memarg, relative_address)?;
                let data = mem.mem.load(idx)?;
                stack.push_value::<T>(Value::F32(data))?;
                trace!("Instruction: f32.load [{relative_address}] -> [{data}]");
            }
            F64_LOAD => {
                decrement_fuel!(T::get_flat_cost(F64_LOAD));
                let memarg = MemArg::read(wasm).unwrap_validated();
                let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
                let mem_addr = *store
                    .modules
                    .get(current_module)
                    .mem_addrs
                    .first()
                    .unwrap_validated();
                let mem = store.memories.get(mem_addr);
                let idx = calculate_mem_address(&memarg, relative_address)?;
                let data = mem.mem.load(idx)?;
                stack.push_value::<T>(Value::F64(data))?;
                trace!("Instruction: f64.load [{relative_address}] -> [{data}]");
            }
            I32_LOAD8_S => {
                decrement_fuel!(T::get_flat_cost(I32_LOAD8_S));
                let memarg = MemArg::read(wasm).unwrap_validated();
                let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
                let mem_addr = *store
                    .modules
                    .get(current_module)
                    .mem_addrs
                    .first()
                    .unwrap_validated();
                let mem = store.memories.get(mem_addr);
                let idx = calculate_mem_address(&memarg, relative_address)?;
                let data: i8 = mem.mem.load(idx)?;
                stack.push_value::<T>(Value::I32(data as u32))?;
                trace!("Instruction: i32.load8_s [{relative_address}] -> [{data}]");
            }
            I32_LOAD8_U => {
                decrement_fuel!(T::get_flat_cost(I32_LOAD8_U));
                let memarg = MemArg::read(wasm).unwrap_validated();
                let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
                let mem_addr = *store
                    .modules
                    .get(current_module)
                    .mem_addrs
                    .first()
                    .unwrap_validated();
                let mem = store.memories.get(mem_addr);
                let idx = calculate_mem_address(&memarg, relative_address)?;
                let data: u8 = mem.mem.load(idx)?;
                stack.push_value::<T>(Value::I32(data as u32))?;
                trace!("Instruction: i32.load8_u [{relative_address}] -> [{data}]");
            }
            I32_LOAD16_S => {
                decrement_fuel!(T::get_flat_cost(I32_LOAD16_S));
                let memarg = MemArg::read(wasm).unwrap_validated();
                let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
                let mem_addr = *store
                    .modules
                    .get(current_module)
                    .mem_addrs
                    .first()
                    .unwrap_validated();
                let mem = store.memories.get(mem_addr);
                let idx = calculate_mem_address(&memarg, relative_address)?;
                let data: i16 = mem.mem.load(idx)?;
                stack.push_value::<T>(Value::I32(data as u32))?;
                trace!("Instruction: i32.load16_s [{relative_address}] -> [{data}]");
            }
            I32_LOAD16_U => {
                decrement_fuel!(T::get_flat_cost(I32_LOAD16_U));
                let memarg = MemArg::read(wasm).unwrap_validated();
                let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
                let mem_addr = *store
                    .modules
                    .get(current_module)
                    .mem_addrs
                    .first()
                    .unwrap_validated();
                let mem = store.memories.get(mem_addr);
                let idx = calculate_mem_address(&memarg, relative_address)?;
                let data: u16 = mem.mem.load(idx)?;
                stack.push_value::<T>(Value::I32(data as u32))?;
                trace!("Instruction: i32.load16_u [{relative_address}] -> [{data}]");
            }
            I64_LOAD8_S => {
                decrement_fuel!(T::get_flat_cost(I64_LOAD8_S));
                let memarg = MemArg::read(wasm).unwrap_validated();
                let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
                let mem_addr = *store
                    .modules
                    .get(current_module)
                    .mem_addrs
                    .first()
                    .unwrap_validated();
                let mem = store.memories.get(mem_addr);
                let idx = calculate_mem_address(&memarg, relative_address)?;
                let data: i8 = mem.mem.load(idx)?;
                stack.push_value::<T>(Value::I64(data as u64))?;
                trace!("Instruction: i64.load8_s [{relative_address}] -> [{data}]");
            }
            I64_LOAD8_U => {
                decrement_fuel!(T::get_flat_cost(I64_LOAD8_U));
                let memarg = MemArg::read(wasm).unwrap_validated();
                let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
                let mem_addr = *store
                    .modules
                    .get(current_module)
                    .mem_addrs
                    .first()
                    .unwrap_validated();
                let mem = store.memories.get(mem_addr);
                let idx = calculate_mem_address(&memarg, relative_address)?;
                let data: u8 = mem.mem.load(idx)?;
                stack.push_value::<T>(Value::I64(data as u64))?;
                trace!("Instruction: i64.load8_u [{relative_address}] -> [{data}]");
            }
            I64_LOAD16_S => {
                decrement_fuel!(T::get_flat_cost(I64_LOAD16_S));
                let memarg = MemArg::read(wasm).unwrap_validated();
                let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
                let mem_addr = *store
                    .modules
                    .get(current_module)
                    .mem_addrs
                    .first()
                    .unwrap_validated();
                let mem = store.memories.get(mem_addr);
                let idx = calculate_mem_address(&memarg, relative_address)?;
                let data: i16 = mem.mem.load(idx)?;
                stack.push_value::<T>(Value::I64(data as u64))?;
                trace!("Instruction: i64.load16_s [{relative_address}] -> [{data}]");
            }
            I64_LOAD16_U => {
                decrement_fuel!(T::get_flat_cost(I64_LOAD16_U));
                let memarg = MemArg::read(wasm).unwrap_validated();
                let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
                let mem_addr = *store
                    .modules
                    .get(current_module)
                    .mem_addrs
                    .first()
                    .unwrap_validated();
                let mem = store.memories.get(mem_addr);
                let idx = calculate_mem_address(&memarg, relative_address)?;
                let data: u16 = mem.mem.load(idx)?;
                stack.push_value::<T>(Value::I64(data as u64))?;
                trace!("Instruction: i64.load16_u [{relative_address}] -> [{data}]");
            }
            I64_LOAD32_S => {
                decrement_fuel!(T::get_flat_cost(I64_LOAD32_S));
                let memarg = MemArg::read(wasm).unwrap_validated();
                let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
                let mem_addr = *store
                    .modules
                    .get(current_module)
                    .mem_addrs
                    .first()
                    .unwrap_validated();
                let mem = store.memories.get(mem_addr);
                let idx = calculate_mem_address(&memarg, relative_address)?;
                let data: i32 = mem.mem.load(idx)?;
                stack.push_value::<T>(Value::I64(data as u64))?;
                trace!("Instruction: i64.load32_s [{relative_address}] -> [{data}]");
            }
            I64_LOAD32_U => {
                decrement_fuel!(T::get_flat_cost(I64_LOAD32_U));
                let memarg = MemArg::read(wasm).unwrap_validated();
                let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
                let mem_addr = *store
                    .modules
                    .get(current_module)
                    .mem_addrs
                    .first()
                    .unwrap_validated();
                let mem = store.memories.get(mem_addr);
                let idx = calculate_mem_address(&memarg, relative_address)?;
                let data: u32 = mem.mem.load(idx)?;
                stack.push_value::<T>(Value::I64(data as u64))?;
                trace!("Instruction: i64.load32_u [{relative_address}] -> [{data}]");
            }
            I32_STORE => {
                decrement_fuel!(T::get_flat_cost(I32_STORE));
                let memarg = MemArg::read(wasm).unwrap_validated();
                let data_to_store: u32 = stack.pop_value().try_into().unwrap_validated();
                let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
                let mem_addr = *store
                    .modules
                    .get(current_module)
                    .mem_addrs
                    .first()
                    .unwrap_validated();
                let mem = store.memories.get(mem_addr);
                let idx = calculate_mem_address(&memarg, relative_address)?;
                mem.mem.store(idx, data_to_store).map_err(|e| {
                    crate::debugln!("WASM Trap: MemoryAccessOutOfBounds (store i32, addr={:#x}) at PC {:#x}", idx, wasm.pc);
                    e
                })?;
                trace!("Instruction: i32.store [{relative_address} {data_to_store}] -> []");
            }
            I64_STORE => {
                decrement_fuel!(T::get_flat_cost(I64_STORE));
                let memarg = MemArg::read(wasm).unwrap_validated();
                let data_to_store: u64 = stack.pop_value().try_into().unwrap_validated();
                let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
                let mem_addr = *store
                    .modules
                    .get(current_module)
                    .mem_addrs
                    .first()
                    .unwrap_validated();
                let mem = store.memories.get(mem_addr);
                let idx = calculate_mem_address(&memarg, relative_address)?;
                mem.mem.store(idx, data_to_store)?;
                trace!("Instruction: i64.store [{relative_address} {data_to_store}] -> []");
            }
            F32_STORE => {
                decrement_fuel!(T::get_flat_cost(F32_STORE));
                let memarg = MemArg::read(wasm).unwrap_validated();
                let data_to_store: F32 = stack.pop_value().try_into().unwrap_validated();
                let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
                let mem_addr = *store
                    .modules
                    .get(current_module)
                    .mem_addrs
                    .first()
                    .unwrap_validated();
                let mem = store.memories.get(mem_addr);
                let idx = calculate_mem_address(&memarg, relative_address)?;
                mem.mem.store(idx, data_to_store)?;
                trace!("Instruction: f32.store [{relative_address} {data_to_store}] -> []");
            }
            F64_STORE => {
                decrement_fuel!(T::get_flat_cost(F64_STORE));
                let memarg = MemArg::read(wasm).unwrap_validated();
                let data_to_store: F64 = stack.pop_value().try_into().unwrap_validated();
                let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
                let mem_addr = *store
                    .modules
                    .get(current_module)
                    .mem_addrs
                    .first()
                    .unwrap_validated();
                let mem = store.memories.get(mem_addr);
                let idx = calculate_mem_address(&memarg, relative_address)?;
                mem.mem.store(idx, data_to_store)?;
                trace!("Instruction: f64.store [{relative_address} {data_to_store}] -> []");
            }
            I32_STORE8 => {
                decrement_fuel!(T::get_flat_cost(I32_STORE8));
                let memarg = MemArg::read(wasm).unwrap_validated();
                let data_to_store: i32 = stack.pop_value().try_into().unwrap_validated();
                let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
                let wrapped_data = data_to_store as i8;
                let mem_addr = *store
                    .modules
                    .get(current_module)
                    .mem_addrs
                    .first()
                    .unwrap_validated();
                let mem = store.memories.get(mem_addr);
                let idx = calculate_mem_address(&memarg, relative_address)?;
                mem.mem.store(idx, wrapped_data)?;
                trace!("Instruction: i32.store8 [{relative_address} {wrapped_data}] -> []");
            }
            I32_STORE16 => {
                decrement_fuel!(T::get_flat_cost(I32_STORE16));
                let memarg = MemArg::read(wasm).unwrap_validated();
                let data_to_store: i32 = stack.pop_value().try_into().unwrap_validated();
                let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
                let wrapped_data = data_to_store as i16;
                let mem_addr = *store
                    .modules
                    .get(current_module)
                    .mem_addrs
                    .first()
                    .unwrap_validated();
                let mem = store.memories.get(mem_addr);
                let idx = calculate_mem_address(&memarg, relative_address)?;
                mem.mem.store(idx, wrapped_data)?;
                trace!("Instruction: i32.store16 [{relative_address} {data_to_store}] -> []");
            }
            I64_STORE8 => {
                decrement_fuel!(T::get_flat_cost(I64_STORE8));
                let memarg = MemArg::read(wasm).unwrap_validated();
                let data_to_store: i64 = stack.pop_value().try_into().unwrap_validated();
                let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
                let wrapped_data = data_to_store as i8;
                let mem_addr = *store
                    .modules
                    .get(current_module)
                    .mem_addrs
                    .first()
                    .unwrap_validated();
                let mem = store.memories.get(mem_addr);
                let idx = calculate_mem_address(&memarg, relative_address)?;
                mem.mem.store(idx, wrapped_data)?;
                trace!("Instruction: i64.store8 [{relative_address} {data_to_store}] -> []");
            }
            I64_STORE16 => {
                decrement_fuel!(T::get_flat_cost(I64_STORE16));
                let memarg = MemArg::read(wasm).unwrap_validated();
                let data_to_store: i64 = stack.pop_value().try_into().unwrap_validated();
                let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
                let wrapped_data = data_to_store as i16;
                let mem_addr = *store
                    .modules
                    .get(current_module)
                    .mem_addrs
                    .first()
                    .unwrap_validated();
                let mem = store.memories.get(mem_addr);
                let idx = calculate_mem_address(&memarg, relative_address)?;
                mem.mem.store(idx, wrapped_data)?;
                trace!("Instruction: i64.store16 [{relative_address} {data_to_store}] -> []");
            }
            I64_STORE32 => {
                decrement_fuel!(T::get_flat_cost(I64_STORE32));
                let memarg = MemArg::read(wasm).unwrap_validated();
                let data_to_store: i64 = stack.pop_value().try_into().unwrap_validated();
                let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
                let wrapped_data = data_to_store as i32;
                let mem_addr = *store
                    .modules
                    .get(current_module)
                    .mem_addrs
                    .first()
                    .unwrap_validated();
                let mem = store.memories.get(mem_addr);
                let idx = calculate_mem_address(&memarg, relative_address)?;
                mem.mem.store(idx, wrapped_data)?;
                trace!("Instruction: i64.store32 [{relative_address} {data_to_store}] -> []");
            }
            MEMORY_SIZE => {
                decrement_fuel!(T::get_flat_cost(MEMORY_SIZE));
                let mem_idx = wasm.read_u8().unwrap_validated() as usize;
                let mem_addr = *store
                    .modules
                    .get(current_module)
                    .mem_addrs
                    .get(mem_idx)
                    .unwrap_validated();
                let mem = store.memories.get(mem_addr);
                let size = mem.size() as u32;
                stack.push_value::<T>(Value::I32(size))?;
                trace!("Instruction: memory.size [] -> [{}]", size);
            }
            MEMORY_GROW => {
                let mem_idx = wasm.read_u8().unwrap_validated() as usize;
                let mem_addr = *store
                    .modules
                    .get(current_module)
                    .mem_addrs
                    .get(mem_idx)
                    .unwrap_validated();
                let mem = store.memories.get_mut(mem_addr);
                let sz: u32 = mem.size() as u32;
                let n: u32 = stack.pop_value().try_into().unwrap_validated();
                let cost = T::get_flat_cost(MEMORY_GROW) + n * T::get_cost_per_element(MEMORY_GROW);
                if let Some(fuel) = &mut resumable.maybe_fuel {
                    if *fuel >= cost {
                        *fuel -= cost;
                    } else {
                        stack.push_value::<T>(Value::I32(n)).unwrap_validated();
                        resumable.current_func_addr = current_func_addr;
                        resumable.pc = prev_pc;
                        resumable.stp = stp;
                        return Ok(NonZeroU32::new(cost - *fuel));
                    }
                }
                let pushed_value = match mem.grow(n) {
                    Ok(_) => sz,
                    Err(_) => u32::MAX,
                };
                stack.push_value::<T>(Value::I32(pushed_value))?;
                trace!("Instruction: memory.grow [{}] -> [{}]", n, pushed_value);
            }
            I32_CONST => {
                decrement_fuel!(T::get_flat_cost(I32_CONST));
                let constant = wasm.read_var_i32().unwrap_validated();
                trace!("Instruction: i32.const [] -> [{constant}]");
                stack.push_value::<T>(constant.into())?;
            }
            F32_CONST => {
                decrement_fuel!(T::get_flat_cost(F32_CONST));
                let constant = F32::from_bits(wasm.read_f32().unwrap_validated());
                trace!("Instruction: f32.const [] -> [{constant:.7}]");
                stack.push_value::<T>(constant.into())?;
            }
            I32_EQZ => {
                decrement_fuel!(T::get_flat_cost(I32_EQZ));
                let v1: i32 = stack.pop_value().try_into().unwrap_validated();
                let res = if v1 == 0 { 1 } else { 0 };
                trace!("Instruction: i32.eqz [{v1}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I32_EQ => {
                decrement_fuel!(T::get_flat_cost(I32_EQ));
                let v2: i32 = stack.pop_value().try_into().unwrap_validated();
                let v1: i32 = stack.pop_value().try_into().unwrap_validated();
                let res = if v1 == v2 { 1 } else { 0 };
                trace!("Instruction: i32.eq [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I32_NE => {
                decrement_fuel!(T::get_flat_cost(I32_NE));
                let v2: i32 = stack.pop_value().try_into().unwrap_validated();
                let v1: i32 = stack.pop_value().try_into().unwrap_validated();
                let res = if v1 != v2 { 1 } else { 0 };
                trace!("Instruction: i32.ne [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I32_LT_S => {
                decrement_fuel!(T::get_flat_cost(I32_LT_S));
                let v2: i32 = stack.pop_value().try_into().unwrap_validated();
                let v1: i32 = stack.pop_value().try_into().unwrap_validated();
                let res = if v1 < v2 { 1 } else { 0 };
                trace!("Instruction: i32.lt_s [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I32_LT_U => {
                decrement_fuel!(T::get_flat_cost(I32_LT_U));
                let v2: i32 = stack.pop_value().try_into().unwrap_validated();
                let v1: i32 = stack.pop_value().try_into().unwrap_validated();
                let res = if (v1 as u32) < (v2 as u32) { 1 } else { 0 };
                trace!("Instruction: i32.lt_u [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I32_GT_S => {
                decrement_fuel!(T::get_flat_cost(I32_GT_S));
                let v2: i32 = stack.pop_value().try_into().unwrap_validated();
                let v1: i32 = stack.pop_value().try_into().unwrap_validated();
                let res = if v1 > v2 { 1 } else { 0 };
                trace!("Instruction: i32.gt_s [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I32_GT_U => {
                decrement_fuel!(T::get_flat_cost(I32_GT_U));
                let v2: i32 = stack.pop_value().try_into().unwrap_validated();
                let v1: i32 = stack.pop_value().try_into().unwrap_validated();
                let res = if (v1 as u32) > (v2 as u32) { 1 } else { 0 };
                trace!("Instruction: i32.gt_u [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I32_LE_S => {
                decrement_fuel!(T::get_flat_cost(I32_LE_S));
                let v2: i32 = stack.pop_value().try_into().unwrap_validated();
                let v1: i32 = stack.pop_value().try_into().unwrap_validated();
                let res = if v1 <= v2 { 1 } else { 0 };
                trace!("Instruction: i32.le_s [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I32_LE_U => {
                decrement_fuel!(T::get_flat_cost(I32_LE_U));
                let v2: i32 = stack.pop_value().try_into().unwrap_validated();
                let v1: i32 = stack.pop_value().try_into().unwrap_validated();
                let res = if (v1 as u32) <= (v2 as u32) { 1 } else { 0 };
                trace!("Instruction: i32.le_u [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I32_GE_S => {
                decrement_fuel!(T::get_flat_cost(I32_GE_S));
                let v2: i32 = stack.pop_value().try_into().unwrap_validated();
                let v1: i32 = stack.pop_value().try_into().unwrap_validated();
                let res = if v1 >= v2 { 1 } else { 0 };
                trace!("Instruction: i32.ge_s [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I32_GE_U => {
                decrement_fuel!(T::get_flat_cost(I32_GE_U));
                let v2: i32 = stack.pop_value().try_into().unwrap_validated();
                let v1: i32 = stack.pop_value().try_into().unwrap_validated();
                let res = if (v1 as u32) >= (v2 as u32) { 1 } else { 0 };
                trace!("Instruction: i32.ge_u [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I64_EQZ => {
                decrement_fuel!(T::get_flat_cost(I64_EQZ));
                let v1: i64 = stack.pop_value().try_into().unwrap_validated();
                let res = if v1 == 0 { 1 } else { 0 };
                trace!("Instruction: i64.eqz [{v1}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I64_EQ => {
                decrement_fuel!(T::get_flat_cost(I64_EQ));
                let v2: i64 = stack.pop_value().try_into().unwrap_validated();
                let v1: i64 = stack.pop_value().try_into().unwrap_validated();
                let res = if v1 == v2 { 1 } else { 0 };
                trace!("Instruction: i64.eq [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I64_NE => {
                decrement_fuel!(T::get_flat_cost(I64_NE));
                let v2: i64 = stack.pop_value().try_into().unwrap_validated();
                let v1: i64 = stack.pop_value().try_into().unwrap_validated();
                let res = if v1 != v2 { 1 } else { 0 };
                trace!("Instruction: i64.ne [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I64_LT_S => {
                decrement_fuel!(T::get_flat_cost(I64_LT_S));
                let v2: i64 = stack.pop_value().try_into().unwrap_validated();
                let v1: i64 = stack.pop_value().try_into().unwrap_validated();
                let res = if v1 < v2 { 1 } else { 0 };
                trace!("Instruction: i64.lt_s [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I64_LT_U => {
                decrement_fuel!(T::get_flat_cost(I64_LT_U));
                let v2: i64 = stack.pop_value().try_into().unwrap_validated();
                let v1: i64 = stack.pop_value().try_into().unwrap_validated();
                let res = if (v1 as u64) < (v2 as u64) { 1 } else { 0 };
                trace!("Instruction: i64.lt_u [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I64_GT_S => {
                decrement_fuel!(T::get_flat_cost(I64_GT_S));
                let v2: i64 = stack.pop_value().try_into().unwrap_validated();
                let v1: i64 = stack.pop_value().try_into().unwrap_validated();
                let res = if v1 > v2 { 1 } else { 0 };
                trace!("Instruction: i64.gt_s [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I64_GT_U => {
                decrement_fuel!(T::get_flat_cost(I64_GT_U));
                let v2: i64 = stack.pop_value().try_into().unwrap_validated();
                let v1: i64 = stack.pop_value().try_into().unwrap_validated();
                let res = if (v1 as u64) > (v2 as u64) { 1 } else { 0 };
                trace!("Instruction: i64.gt_u [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I64_LE_S => {
                decrement_fuel!(T::get_flat_cost(I64_LE_S));
                let v2: i64 = stack.pop_value().try_into().unwrap_validated();
                let v1: i64 = stack.pop_value().try_into().unwrap_validated();
                let res = if v1 <= v2 { 1 } else { 0 };
                trace!("Instruction: i64.le_s [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I64_LE_U => {
                decrement_fuel!(T::get_flat_cost(I64_LE_U));
                let v2: i64 = stack.pop_value().try_into().unwrap_validated();
                let v1: i64 = stack.pop_value().try_into().unwrap_validated();
                let res = if (v1 as u64) <= (v2 as u64) { 1 } else { 0 };
                trace!("Instruction: i64.le_u [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I64_GE_S => {
                decrement_fuel!(T::get_flat_cost(I64_GE_S));
                let v2: i64 = stack.pop_value().try_into().unwrap_validated();
                let v1: i64 = stack.pop_value().try_into().unwrap_validated();
                let res = if v1 >= v2 { 1 } else { 0 };
                trace!("Instruction: i64.ge_s [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I64_GE_U => {
                decrement_fuel!(T::get_flat_cost(I64_GE_U));
                let v2: i64 = stack.pop_value().try_into().unwrap_validated();
                let v1: i64 = stack.pop_value().try_into().unwrap_validated();
                let res = if (v1 as u64) >= (v2 as u64) { 1 } else { 0 };
                trace!("Instruction: i64.ge_u [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            F32_EQ => {
                decrement_fuel!(T::get_flat_cost(F32_EQ));
                let v2: value::F32 = stack.pop_value().try_into().unwrap_validated();
                let v1: value::F32 = stack.pop_value().try_into().unwrap_validated();
                let res = if v1 == v2 { 1 } else { 0 };
                trace!("Instruction: f32.eq [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            F32_NE => {
                decrement_fuel!(T::get_flat_cost(F32_NE));
                let v2: value::F32 = stack.pop_value().try_into().unwrap_validated();
                let v1: value::F32 = stack.pop_value().try_into().unwrap_validated();
                let res = if v1 != v2 { 1 } else { 0 };
                trace!("Instruction: f32.ne [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            F32_LT => {
                decrement_fuel!(T::get_flat_cost(F32_LT));
                let v2: value::F32 = stack.pop_value().try_into().unwrap_validated();
                let v1: value::F32 = stack.pop_value().try_into().unwrap_validated();
                let res = if v1 < v2 { 1 } else { 0 };
                trace!("Instruction: f32.lt [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            F32_GT => {
                decrement_fuel!(T::get_flat_cost(F32_GT));
                let v2: value::F32 = stack.pop_value().try_into().unwrap_validated();
                let v1: value::F32 = stack.pop_value().try_into().unwrap_validated();
                let res = if v1 > v2 { 1 } else { 0 };
                trace!("Instruction: f32.gt [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            F32_LE => {
                decrement_fuel!(T::get_flat_cost(F32_LE));
                let v2: value::F32 = stack.pop_value().try_into().unwrap_validated();
                let v1: value::F32 = stack.pop_value().try_into().unwrap_validated();
                let res = if v1 <= v2 { 1 } else { 0 };
                trace!("Instruction: f32.le [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            F32_GE => {
                decrement_fuel!(T::get_flat_cost(F32_GE));
                let v2: value::F32 = stack.pop_value().try_into().unwrap_validated();
                let v1: value::F32 = stack.pop_value().try_into().unwrap_validated();
                let res = if v1 >= v2 { 1 } else { 0 };
                trace!("Instruction: f32.ge [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            F64_EQ => {
                decrement_fuel!(T::get_flat_cost(F64_EQ));
                let v2: value::F64 = stack.pop_value().try_into().unwrap_validated();
                let v1: value::F64 = stack.pop_value().try_into().unwrap_validated();
                let res = if v1 == v2 { 1 } else { 0 };
                trace!("Instruction: f64.eq [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            F64_NE => {
                decrement_fuel!(T::get_flat_cost(F64_NE));
                let v2: value::F64 = stack.pop_value().try_into().unwrap_validated();
                let v1: value::F64 = stack.pop_value().try_into().unwrap_validated();
                let res = if v1 != v2 { 1 } else { 0 };
                trace!("Instruction: f64.ne [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            F64_LT => {
                decrement_fuel!(T::get_flat_cost(F64_LT));
                let v2: value::F64 = stack.pop_value().try_into().unwrap_validated();
                let v1: value::F64 = stack.pop_value().try_into().unwrap_validated();
                let res = if v1 < v2 { 1 } else { 0 };
                trace!("Instruction: f64.lt [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            F64_GT => {
                decrement_fuel!(T::get_flat_cost(F64_GT));
                let v2: value::F64 = stack.pop_value().try_into().unwrap_validated();
                let v1: value::F64 = stack.pop_value().try_into().unwrap_validated();
                let res = if v1 > v2 { 1 } else { 0 };
                trace!("Instruction: f64.gt [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            F64_LE => {
                decrement_fuel!(T::get_flat_cost(F64_LE));
                let v2: value::F64 = stack.pop_value().try_into().unwrap_validated();
                let v1: value::F64 = stack.pop_value().try_into().unwrap_validated();
                let res = if v1 <= v2 { 1 } else { 0 };
                trace!("Instruction: f64.le [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            F64_GE => {
                decrement_fuel!(T::get_flat_cost(F64_GE));
                let v2: value::F64 = stack.pop_value().try_into().unwrap_validated();
                let v1: value::F64 = stack.pop_value().try_into().unwrap_validated();
                let res = if v1 >= v2 { 1 } else { 0 };
                trace!("Instruction: f64.ge [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I32_CLZ => {
                decrement_fuel!(T::get_flat_cost(I32_CLZ));
                let v1: i32 = stack.pop_value().try_into().unwrap_validated();
                let res = v1.leading_zeros() as i32;
                trace!("Instruction: i32.clz [{v1}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I32_CTZ => {
                decrement_fuel!(T::get_flat_cost(I32_CTZ));
                let v1: i32 = stack.pop_value().try_into().unwrap_validated();
                let res = v1.trailing_zeros() as i32;
                trace!("Instruction: i32.ctz [{v1}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I32_POPCNT => {
                decrement_fuel!(T::get_flat_cost(I32_POPCNT));
                let v1: i32 = stack.pop_value().try_into().unwrap_validated();
                let res = v1.count_ones() as i32;
                trace!("Instruction: i32.popcnt [{v1}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I64_CONST => {
                decrement_fuel!(T::get_flat_cost(I64_CONST));
                let constant = wasm.read_var_i64().unwrap_validated();
                trace!("Instruction: i64.const [] -> [{constant}]");
                stack.push_value::<T>(constant.into())?;
            }
            F64_CONST => {
                decrement_fuel!(T::get_flat_cost(F64_CONST));
                let constant = F64::from_bits(wasm.read_f64().unwrap_validated());
                trace!("Instruction: f64.const [] -> [{constant}]");
                stack.push_value::<T>(constant.into())?;
            }
            I32_ADD => {
                decrement_fuel!(T::get_flat_cost(I32_ADD));
                let v1: i32 = stack.pop_value().try_into().unwrap_validated();
                let v2: i32 = stack.pop_value().try_into().unwrap_validated();
                let res = v1.wrapping_add(v2);
                trace!("Instruction: i32.add [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I32_SUB => {
                decrement_fuel!(T::get_flat_cost(I32_SUB));
                let v2: i32 = stack.pop_value().try_into().unwrap_validated();
                let v1: i32 = stack.pop_value().try_into().unwrap_validated();
                let res = v1.wrapping_sub(v2);
                trace!("Instruction: i32.sub [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I32_MUL => {
                decrement_fuel!(T::get_flat_cost(I32_MUL));
                let v1: i32 = stack.pop_value().try_into().unwrap_validated();
                let v2: i32 = stack.pop_value().try_into().unwrap_validated();
                let res = v1.wrapping_mul(v2);
                trace!("Instruction: i32.mul [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I32_DIV_S => {
                decrement_fuel!(T::get_flat_cost(I32_DIV_S));
                let dividend: i32 = stack.pop_value().try_into().unwrap_validated();
                let divisor: i32 = stack.pop_value().try_into().unwrap_validated();
                if dividend == 0 {
                    crate::debugln!("WASM Trap: DivideBy0 at PC {:#x}", wasm.pc);
                    return Err(TrapError::DivideBy0.into());
                }
                if divisor == i32::MIN && dividend == -1 {
                    crate::debugln!("WASM Trap: UnrepresentableResult at PC {:#x}", wasm.pc);
                    return Err(TrapError::UnrepresentableResult.into());
                }
                let res = divisor / dividend;
                trace!("Instruction: i32.div_s [{divisor} {dividend}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I32_DIV_U => {
                decrement_fuel!(T::get_flat_cost(I32_DIV_U));
                let dividend: i32 = stack.pop_value().try_into().unwrap_validated();
                let divisor: i32 = stack.pop_value().try_into().unwrap_validated();
                let dividend = dividend as u32;
                let divisor = divisor as u32;
                if dividend == 0 {
                    crate::debugln!("WASM Trap: DivideBy0 at PC {:#x}", wasm.pc);
                    return Err(TrapError::DivideBy0.into());
                }
                let res = (divisor / dividend) as i32;
                trace!("Instruction: i32.div_u [{divisor} {dividend}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I32_REM_S => {
                decrement_fuel!(T::get_flat_cost(I32_REM_S));
                let dividend: i32 = stack.pop_value().try_into().unwrap_validated();
                let divisor: i32 = stack.pop_value().try_into().unwrap_validated();
                if dividend == 0 {
                    crate::debugln!("WASM Trap: DivideBy0 at PC {:#x}", wasm.pc);
                    return Err(TrapError::DivideBy0.into());
                }
                let res = divisor.checked_rem(dividend);
                let res = res.unwrap_or_default();
                trace!("Instruction: i32.rem_s [{divisor} {dividend}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I64_CLZ => {
                decrement_fuel!(T::get_flat_cost(I64_CLZ));
                let v1: i64 = stack.pop_value().try_into().unwrap_validated();
                let res = v1.leading_zeros() as i64;
                trace!("Instruction: i64.clz [{v1}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I64_CTZ => {
                decrement_fuel!(T::get_flat_cost(I64_CTZ));
                let v1: i64 = stack.pop_value().try_into().unwrap_validated();
                let res = v1.trailing_zeros() as i64;
                trace!("Instruction: i64.ctz [{v1}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I64_POPCNT => {
                decrement_fuel!(T::get_flat_cost(I64_POPCNT));
                let v1: i64 = stack.pop_value().try_into().unwrap_validated();
                let res = v1.count_ones() as i64;
                trace!("Instruction: i64.popcnt [{v1}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I64_ADD => {
                decrement_fuel!(T::get_flat_cost(I64_ADD));
                let v1: i64 = stack.pop_value().try_into().unwrap_validated();
                let v2: i64 = stack.pop_value().try_into().unwrap_validated();
                let res = v1.wrapping_add(v2);
                trace!("Instruction: i64.add [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I64_SUB => {
                decrement_fuel!(T::get_flat_cost(I64_SUB));
                let v2: i64 = stack.pop_value().try_into().unwrap_validated();
                let v1: i64 = stack.pop_value().try_into().unwrap_validated();
                let res = v1.wrapping_sub(v2);
                trace!("Instruction: i64.sub [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I64_MUL => {
                decrement_fuel!(T::get_flat_cost(I64_MUL));
                let v1: i64 = stack.pop_value().try_into().unwrap_validated();
                let v2: i64 = stack.pop_value().try_into().unwrap_validated();
                let res = v1.wrapping_mul(v2);
                trace!("Instruction: i64.mul [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I64_DIV_S => {
                decrement_fuel!(T::get_flat_cost(I64_DIV_S));
                let dividend: i64 = stack.pop_value().try_into().unwrap_validated();
                let divisor: i64 = stack.pop_value().try_into().unwrap_validated();
                if dividend == 0 {
                    return Err(TrapError::DivideBy0.into());
                }
                if divisor == i64::MIN && dividend == -1 {
                    return Err(TrapError::UnrepresentableResult.into());
                }
                let res = divisor / dividend;
                trace!("Instruction: i64.div_s [{divisor} {dividend}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I64_DIV_U => {
                decrement_fuel!(T::get_flat_cost(I64_DIV_U));
                let dividend: i64 = stack.pop_value().try_into().unwrap_validated();
                let divisor: i64 = stack.pop_value().try_into().unwrap_validated();
                let dividend = dividend as u64;
                let divisor = divisor as u64;
                if dividend == 0 {
                    return Err(TrapError::DivideBy0.into());
                }
                let res = (divisor / dividend) as i64;
                trace!("Instruction: i64.div_u [{divisor} {dividend}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I64_REM_S => {
                decrement_fuel!(T::get_flat_cost(I64_REM_S));
                let dividend: i64 = stack.pop_value().try_into().unwrap_validated();
                let divisor: i64 = stack.pop_value().try_into().unwrap_validated();
                if dividend == 0 {
                    return Err(TrapError::DivideBy0.into());
                }
                let res = divisor.checked_rem(dividend);
                let res = res.unwrap_or_default();
                trace!("Instruction: i64.rem_s [{divisor} {dividend}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I64_REM_U => {
                decrement_fuel!(T::get_flat_cost(I64_REM_U));
                let dividend: i64 = stack.pop_value().try_into().unwrap_validated();
                let divisor: i64 = stack.pop_value().try_into().unwrap_validated();
                let dividend = dividend as u64;
                let divisor = divisor as u64;
                if dividend == 0 {
                    return Err(TrapError::DivideBy0.into());
                }
                let res = (divisor % dividend) as i64;
                trace!("Instruction: i64.rem_u [{divisor} {dividend}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I64_AND => {
                decrement_fuel!(T::get_flat_cost(I64_AND));
                let v2: i64 = stack.pop_value().try_into().unwrap_validated();
                let v1: i64 = stack.pop_value().try_into().unwrap_validated();
                let res = v1 & v2;
                trace!("Instruction: i64.and [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I64_OR => {
                decrement_fuel!(T::get_flat_cost(I64_OR));
                let v2: i64 = stack.pop_value().try_into().unwrap_validated();
                let v1: i64 = stack.pop_value().try_into().unwrap_validated();
                let res = v1 | v2;
                trace!("Instruction: i64.or [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I64_XOR => {
                decrement_fuel!(T::get_flat_cost(I64_XOR));
                let v2: i64 = stack.pop_value().try_into().unwrap_validated();
                let v1: i64 = stack.pop_value().try_into().unwrap_validated();
                let res = v1 ^ v2;
                trace!("Instruction: i64.xor [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I64_SHL => {
                decrement_fuel!(T::get_flat_cost(I64_SHL));
                let v2: i64 = stack.pop_value().try_into().unwrap_validated();
                let v1: i64 = stack.pop_value().try_into().unwrap_validated();
                let res = v1.wrapping_shl((v2 & 63) as u32);
                trace!("Instruction: i64.shl [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I64_SHR_S => {
                decrement_fuel!(T::get_flat_cost(I64_SHR_S));
                let v2: i64 = stack.pop_value().try_into().unwrap_validated();
                let v1: i64 = stack.pop_value().try_into().unwrap_validated();
                let res = v1.wrapping_shr((v2 & 63) as u32);
                trace!("Instruction: i64.shr_s [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I64_SHR_U => {
                decrement_fuel!(T::get_flat_cost(I64_SHR_U));
                let v2: i64 = stack.pop_value().try_into().unwrap_validated();
                let v1: i64 = stack.pop_value().try_into().unwrap_validated();
                let res = (v1 as u64).wrapping_shr((v2 & 63) as u32);
                trace!("Instruction: i64.shr_u [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I64_ROTL => {
                decrement_fuel!(T::get_flat_cost(I64_ROTL));
                let v2: i64 = stack.pop_value().try_into().unwrap_validated();
                let v1: i64 = stack.pop_value().try_into().unwrap_validated();
                let res = v1.rotate_left((v2 & 63) as u32);
                trace!("Instruction: i64.rotl [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I64_ROTR => {
                decrement_fuel!(T::get_flat_cost(I64_ROTR));
                let v2: i64 = stack.pop_value().try_into().unwrap_validated();
                let v1: i64 = stack.pop_value().try_into().unwrap_validated();
                let res = v1.rotate_right((v2 & 63) as u32);
                trace!("Instruction: i64.rotr [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I32_REM_U => {
                decrement_fuel!(T::get_flat_cost(I32_REM_U));
                let dividend: i32 = stack.pop_value().try_into().unwrap_validated();
                let divisor: i32 = stack.pop_value().try_into().unwrap_validated();
                let dividend = dividend as u32;
                let divisor = divisor as u32;
                if dividend == 0 {
                    return Err(TrapError::DivideBy0.into());
                }
                let res = divisor.checked_rem(dividend);
                let res = res.unwrap_or_default() as i32;
                trace!("Instruction: i32.rem_u [{divisor} {dividend}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I32_AND => {
                decrement_fuel!(T::get_flat_cost(I32_AND));
                let v1: i32 = stack.pop_value().try_into().unwrap_validated();
                let v2: i32 = stack.pop_value().try_into().unwrap_validated();
                let res = v1 & v2;
                trace!("Instruction: i32.and [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I32_OR => {
                decrement_fuel!(T::get_flat_cost(I32_OR));
                let v1: i32 = stack.pop_value().try_into().unwrap_validated();
                let v2: i32 = stack.pop_value().try_into().unwrap_validated();
                let res = v1 | v2;
                trace!("Instruction: i32.or [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I32_XOR => {
                decrement_fuel!(T::get_flat_cost(I32_XOR));
                let v1: i32 = stack.pop_value().try_into().unwrap_validated();
                let v2: i32 = stack.pop_value().try_into().unwrap_validated();
                let res = v1 ^ v2;
                trace!("Instruction: i32.xor [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I32_SHL => {
                decrement_fuel!(T::get_flat_cost(I32_SHL));
                let v1: i32 = stack.pop_value().try_into().unwrap_validated();
                let v2: i32 = stack.pop_value().try_into().unwrap_validated();
                let res = v2.wrapping_shl(v1 as u32);
                trace!("Instruction: i32.shl [{v2} {v1}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I32_SHR_S => {
                decrement_fuel!(T::get_flat_cost(I32_SHR_S));
                let v1: i32 = stack.pop_value().try_into().unwrap_validated();
                let v2: i32 = stack.pop_value().try_into().unwrap_validated();
                let res = v2.wrapping_shr(v1 as u32);
                trace!("Instruction: i32.shr_s [{v2} {v1}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I32_SHR_U => {
                decrement_fuel!(T::get_flat_cost(I32_SHR_U));
                let v1: i32 = stack.pop_value().try_into().unwrap_validated();
                let v2: i32 = stack.pop_value().try_into().unwrap_validated();
                let res = (v2 as u32).wrapping_shr(v1 as u32) as i32;
                trace!("Instruction: i32.shr_u [{v2} {v1}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I32_ROTL => {
                decrement_fuel!(T::get_flat_cost(I32_ROTL));
                let v1: i32 = stack.pop_value().try_into().unwrap_validated();
                let v2: i32 = stack.pop_value().try_into().unwrap_validated();
                let res = v2.rotate_left(v1 as u32);
                trace!("Instruction: i32.rotl [{v2} {v1}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I32_ROTR => {
                decrement_fuel!(T::get_flat_cost(I32_ROTR));
                let v1: i32 = stack.pop_value().try_into().unwrap_validated();
                let v2: i32 = stack.pop_value().try_into().unwrap_validated();
                let res = v2.rotate_right(v1 as u32);
                trace!("Instruction: i32.rotr [{v2} {v1}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            F32_ABS => {
                decrement_fuel!(T::get_flat_cost(F32_ABS));
                let v1: value::F32 = stack.pop_value().try_into().unwrap_validated();
                let res: value::F32 = v1.abs();
                trace!("Instruction: f32.abs [{v1}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            F32_NEG => {
                decrement_fuel!(T::get_flat_cost(F32_NEG));
                let v1: value::F32 = stack.pop_value().try_into().unwrap_validated();
                let res: value::F32 = v1.neg();
                trace!("Instruction: f32.neg [{v1}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            F32_CEIL => {
                decrement_fuel!(T::get_flat_cost(F32_CEIL));
                let v1: value::F32 = stack.pop_value().try_into().unwrap_validated();
                let res: value::F32 = v1.ceil();
                trace!("Instruction: f32.ceil [{v1}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            F32_FLOOR => {
                decrement_fuel!(T::get_flat_cost(F32_FLOOR));
                let v1: value::F32 = stack.pop_value().try_into().unwrap_validated();
                let res: value::F32 = v1.floor();
                trace!("Instruction: f32.floor [{v1}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            F32_TRUNC => {
                decrement_fuel!(T::get_flat_cost(F32_TRUNC));
                let v1: value::F32 = stack.pop_value().try_into().unwrap_validated();
                let res: value::F32 = v1.trunc();
                trace!("Instruction: f32.trunc [{v1}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            F32_NEAREST => {
                decrement_fuel!(T::get_flat_cost(F32_NEAREST));
                let v1: value::F32 = stack.pop_value().try_into().unwrap_validated();
                let res: value::F32 = v1.nearest();
                trace!("Instruction: f32.nearest [{v1}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            F32_SQRT => {
                decrement_fuel!(T::get_flat_cost(F32_SQRT));
                let v1: value::F32 = stack.pop_value().try_into().unwrap_validated();
                let res: value::F32 = v1.sqrt();
                trace!("Instruction: f32.sqrt [{v1}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            F32_ADD => {
                decrement_fuel!(T::get_flat_cost(F32_ADD));
                let v2: value::F32 = stack.pop_value().try_into().unwrap_validated();
                let v1: value::F32 = stack.pop_value().try_into().unwrap_validated();
                let res: value::F32 = v1 + v2;
                trace!("Instruction: f32.add [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            F32_SUB => {
                decrement_fuel!(T::get_flat_cost(F32_SUB));
                let v2: value::F32 = stack.pop_value().try_into().unwrap_validated();
                let v1: value::F32 = stack.pop_value().try_into().unwrap_validated();
                let res: value::F32 = v1 - v2;
                trace!("Instruction: f32.sub [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            F32_MUL => {
                decrement_fuel!(T::get_flat_cost(F32_MUL));
                let v2: value::F32 = stack.pop_value().try_into().unwrap_validated();
                let v1: value::F32 = stack.pop_value().try_into().unwrap_validated();
                let res: value::F32 = v1 * v2;
                trace!("Instruction: f32.mul [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            F32_DIV => {
                decrement_fuel!(T::get_flat_cost(F32_DIV));
                let v2: value::F32 = stack.pop_value().try_into().unwrap_validated();
                let v1: value::F32 = stack.pop_value().try_into().unwrap_validated();
                let res: value::F32 = v1 / v2;
                trace!("Instruction: f32.div [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            F32_MIN => {
                decrement_fuel!(T::get_flat_cost(F32_MIN));
                let v2: value::F32 = stack.pop_value().try_into().unwrap_validated();
                let v1: value::F32 = stack.pop_value().try_into().unwrap_validated();
                let res: value::F32 = v1.min(v2);
                trace!("Instruction: f32.min [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            F32_MAX => {
                decrement_fuel!(T::get_flat_cost(F32_MAX));
                let v2: value::F32 = stack.pop_value().try_into().unwrap_validated();
                let v1: value::F32 = stack.pop_value().try_into().unwrap_validated();
                let res: value::F32 = v1.max(v2);
                trace!("Instruction: f32.max [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            F32_COPYSIGN => {
                decrement_fuel!(T::get_flat_cost(F32_COPYSIGN));
                let v2: value::F32 = stack.pop_value().try_into().unwrap_validated();
                let v1: value::F32 = stack.pop_value().try_into().unwrap_validated();
                let res: value::F32 = v1.copysign(v2);
                trace!("Instruction: f32.copysign [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            F64_ABS => {
                decrement_fuel!(T::get_flat_cost(F64_ABS));
                let v1: value::F64 = stack.pop_value().try_into().unwrap_validated();
                let res: value::F64 = v1.abs();
                trace!("Instruction: f64.abs [{v1}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            F64_NEG => {
                decrement_fuel!(T::get_flat_cost(F64_NEG));
                let v1: value::F64 = stack.pop_value().try_into().unwrap_validated();
                let res: value::F64 = v1.neg();
                trace!("Instruction: f64.neg [{v1}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            F64_CEIL => {
                decrement_fuel!(T::get_flat_cost(F64_CEIL));
                let v1: value::F64 = stack.pop_value().try_into().unwrap_validated();
                let res: value::F64 = v1.ceil();
                trace!("Instruction: f64.ceil [{v1}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            F64_FLOOR => {
                decrement_fuel!(T::get_flat_cost(F64_FLOOR));
                let v1: value::F64 = stack.pop_value().try_into().unwrap_validated();
                let res: value::F64 = v1.floor();
                trace!("Instruction: f64.floor [{v1}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            F64_TRUNC => {
                decrement_fuel!(T::get_flat_cost(F64_TRUNC));
                let v1: value::F64 = stack.pop_value().try_into().unwrap_validated();
                let res: value::F64 = v1.trunc();
                trace!("Instruction: f64.trunc [{v1}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            F64_NEAREST => {
                decrement_fuel!(T::get_flat_cost(F64_NEAREST));
                let v1: value::F64 = stack.pop_value().try_into().unwrap_validated();
                let res: value::F64 = v1.nearest();
                trace!("Instruction: f64.nearest [{v1}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            F64_SQRT => {
                decrement_fuel!(T::get_flat_cost(F64_SQRT));
                let v1: value::F64 = stack.pop_value().try_into().unwrap_validated();
                let res: value::F64 = v1.sqrt();
                trace!("Instruction: f64.sqrt [{v1}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            F64_ADD => {
                decrement_fuel!(T::get_flat_cost(F64_ADD));
                let v2: value::F64 = stack.pop_value().try_into().unwrap_validated();
                let v1: value::F64 = stack.pop_value().try_into().unwrap_validated();
                let res: value::F64 = v1 + v2;
                trace!("Instruction: f64.add [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            F64_SUB => {
                decrement_fuel!(T::get_flat_cost(F64_SUB));
                let v2: value::F64 = stack.pop_value().try_into().unwrap_validated();
                let v1: value::F64 = stack.pop_value().try_into().unwrap_validated();
                let res: value::F64 = v1 - v2;
                trace!("Instruction: f64.sub [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            F64_MUL => {
                decrement_fuel!(T::get_flat_cost(F64_MUL));
                let v2: value::F64 = stack.pop_value().try_into().unwrap_validated();
                let v1: value::F64 = stack.pop_value().try_into().unwrap_validated();
                let res: value::F64 = v1 * v2;
                trace!("Instruction: f64.mul [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            F64_DIV => {
                decrement_fuel!(T::get_flat_cost(F64_DIV));
                let v2: value::F64 = stack.pop_value().try_into().unwrap_validated();
                let v1: value::F64 = stack.pop_value().try_into().unwrap_validated();
                let res: value::F64 = v1 / v2;
                trace!("Instruction: f64.div [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            F64_MIN => {
                decrement_fuel!(T::get_flat_cost(F64_MIN));
                let v2: value::F64 = stack.pop_value().try_into().unwrap_validated();
                let v1: value::F64 = stack.pop_value().try_into().unwrap_validated();
                let res: value::F64 = v1.min(v2);
                trace!("Instruction: f64.min [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            F64_MAX => {
                decrement_fuel!(T::get_flat_cost(F64_MAX));
                let v2: value::F64 = stack.pop_value().try_into().unwrap_validated();
                let v1: value::F64 = stack.pop_value().try_into().unwrap_validated();
                let res: value::F64 = v1.max(v2);
                trace!("Instruction: f64.max [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            F64_COPYSIGN => {
                decrement_fuel!(T::get_flat_cost(F64_COPYSIGN));
                let v2: value::F64 = stack.pop_value().try_into().unwrap_validated();
                let v1: value::F64 = stack.pop_value().try_into().unwrap_validated();
                let res: value::F64 = v1.copysign(v2);
                trace!("Instruction: f64.copysign [{v1} {v2}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I32_WRAP_I64 => {
                decrement_fuel!(T::get_flat_cost(I32_WRAP_I64));
                let v: i64 = stack.pop_value().try_into().unwrap_validated();
                let res: i32 = v as i32;
                trace!("Instruction: i32.wrap_i64 [{v}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I32_TRUNC_F32_S => {
                decrement_fuel!(T::get_flat_cost(I32_TRUNC_F32_S));
                let v: value::F32 = stack.pop_value().try_into().unwrap_validated();
                if v.is_infinity() {
                    return Err(TrapError::UnrepresentableResult.into());
                }
                if v.is_nan() {
                    return Err(TrapError::BadConversionToInteger.into());
                }
                if v >= value::F32(2147483648.0) || v <= value::F32(-2147483904.0) {
                    return Err(TrapError::UnrepresentableResult.into());
                }
                let res: i32 = v.as_i32();
                trace!("Instruction: i32.trunc_f32_s [{v:.7}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I32_TRUNC_F32_U => {
                decrement_fuel!(T::get_flat_cost(I32_TRUNC_F32_U));
                let v: value::F32 = stack.pop_value().try_into().unwrap_validated();
                if v.is_infinity() {
                    return Err(TrapError::UnrepresentableResult.into());
                }
                if v.is_nan() {
                    return Err(TrapError::BadConversionToInteger.into());
                }
                if v >= value::F32(4294967296.0) || v <= value::F32(-1.0) {
                    return Err(TrapError::UnrepresentableResult.into());
                }
                let res: i32 = v.as_u32() as i32;
                trace!("Instruction: i32.trunc_f32_u [{v:.7}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I32_TRUNC_F64_S => {
                decrement_fuel!(T::get_flat_cost(I32_TRUNC_F64_S));
                let v: value::F64 = stack.pop_value().try_into().unwrap_validated();
                if v.is_infinity() {
                    return Err(TrapError::UnrepresentableResult.into());
                }
                if v.is_nan() {
                    return Err(TrapError::BadConversionToInteger.into());
                }
                if v >= value::F64(2147483648.0) || v <= value::F64(-2147483649.0) {
                    return Err(TrapError::UnrepresentableResult.into());
                }
                let res: i32 = v.as_i32();
                trace!("Instruction: i32.trunc_f64_s [{v:.7}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I32_TRUNC_F64_U => {
                decrement_fuel!(T::get_flat_cost(I32_TRUNC_F64_U));
                let v: value::F64 = stack.pop_value().try_into().unwrap_validated();
                if v.is_infinity() {
                    return Err(TrapError::UnrepresentableResult.into());
                }
                if v.is_nan() {
                    return Err(TrapError::BadConversionToInteger.into());
                }
                if v >= value::F64(4294967296.0) || v <= value::F64(-1.0) {
                    return Err(TrapError::UnrepresentableResult.into());
                }
                let res: i32 = v.as_u32() as i32;
                trace!("Instruction: i32.trunc_f32_u [{v:.7}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I64_EXTEND_I32_S => {
                decrement_fuel!(T::get_flat_cost(I64_EXTEND_I32_S));
                let v: i32 = stack.pop_value().try_into().unwrap_validated();
                let res: i64 = v as i64;
                trace!("Instruction: i64.extend_i32_s [{v}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I64_EXTEND_I32_U => {
                decrement_fuel!(T::get_flat_cost(I64_EXTEND_I32_U));
                let v: i32 = stack.pop_value().try_into().unwrap_validated();
                let res: i64 = v as u32 as i64;
                trace!("Instruction: i64.extend_i32_u [{v}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I64_TRUNC_F32_S => {
                decrement_fuel!(T::get_flat_cost(I64_TRUNC_F32_S));
                let v: value::F32 = stack.pop_value().try_into().unwrap_validated();
                if v.is_infinity() {
                    return Err(TrapError::UnrepresentableResult.into());
                }
                if v.is_nan() {
                    return Err(TrapError::BadConversionToInteger.into());
                }
                if v >= value::F32(9223372036854775808.0) || v <= value::F32(-9223373136366403584.0)
                {
                    return Err(TrapError::UnrepresentableResult.into());
                }
                let res: i64 = v.as_i64();
                trace!("Instruction: i64.trunc_f32_s [{v:.7}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I64_TRUNC_F32_U => {
                decrement_fuel!(T::get_flat_cost(I64_TRUNC_F32_U));
                let v: value::F32 = stack.pop_value().try_into().unwrap_validated();
                if v.is_infinity() {
                    return Err(TrapError::UnrepresentableResult.into());
                }
                if v.is_nan() {
                    return Err(TrapError::BadConversionToInteger.into());
                }
                if v >= value::F32(18446744073709551616.0) || v <= value::F32(-1.0) {
                    return Err(TrapError::UnrepresentableResult.into());
                }
                let res: i64 = v.as_u64() as i64;
                trace!("Instruction: i64.trunc_f32_u [{v:.7}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I64_TRUNC_F64_S => {
                decrement_fuel!(T::get_flat_cost(I64_TRUNC_F64_S));
                let v: value::F64 = stack.pop_value().try_into().unwrap_validated();
                if v.is_infinity() {
                    return Err(TrapError::UnrepresentableResult.into());
                }
                if v.is_nan() {
                    return Err(TrapError::BadConversionToInteger.into());
                }
                if v >= value::F64(9223372036854775808.0) || v <= value::F64(-9223372036854777856.0)
                {
                    return Err(TrapError::UnrepresentableResult.into());
                }
                let res: i64 = v.as_i64();
                trace!("Instruction: i64.trunc_f64_s [{v:.17}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I64_TRUNC_F64_U => {
                decrement_fuel!(T::get_flat_cost(I64_TRUNC_F64_U));
                let v: value::F64 = stack.pop_value().try_into().unwrap_validated();
                if v.is_infinity() {
                    return Err(TrapError::UnrepresentableResult.into());
                }
                if v.is_nan() {
                    return Err(TrapError::BadConversionToInteger.into());
                }
                if v >= value::F64(18446744073709551616.0) || v <= value::F64(-1.0) {
                    return Err(TrapError::UnrepresentableResult.into());
                }
                let res: i64 = v.as_u64() as i64;
                trace!("Instruction: i64.trunc_f64_u [{v:.17}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            F32_CONVERT_I32_S => {
                decrement_fuel!(T::get_flat_cost(F32_CONVERT_I32_S));
                let v: i32 = stack.pop_value().try_into().unwrap_validated();
                let res: value::F32 = value::F32(v as f32);
                trace!("Instruction: f32.convert_i32_s [{v}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            F32_CONVERT_I32_U => {
                decrement_fuel!(T::get_flat_cost(F32_CONVERT_I32_U));
                let v: i32 = stack.pop_value().try_into().unwrap_validated();
                let res: value::F32 = value::F32(v as u32 as f32);
                trace!("Instruction: f32.convert_i32_u [{v}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            F32_CONVERT_I64_S => {
                decrement_fuel!(T::get_flat_cost(F32_CONVERT_I64_S));
                let v: i64 = stack.pop_value().try_into().unwrap_validated();
                let res: value::F32 = value::F32(v as f32);
                trace!("Instruction: f32.convert_i64_s [{v}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            F32_CONVERT_I64_U => {
                decrement_fuel!(T::get_flat_cost(F32_CONVERT_I64_U));
                let v: i64 = stack.pop_value().try_into().unwrap_validated();
                let res: value::F32 = value::F32(v as u64 as f32);
                trace!("Instruction: f32.convert_i64_u [{v}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            F32_DEMOTE_F64 => {
                decrement_fuel!(T::get_flat_cost(F32_DEMOTE_F64));
                let v: value::F64 = stack.pop_value().try_into().unwrap_validated();
                let res: value::F32 = v.as_f32();
                trace!("Instruction: f32.demote_f64 [{v:.17}] -> [{res:.7}]");
                stack.push_value::<T>(res.into())?;
            }
            F64_CONVERT_I32_S => {
                decrement_fuel!(T::get_flat_cost(F64_CONVERT_I32_S));
                let v: i32 = stack.pop_value().try_into().unwrap_validated();
                let res: value::F64 = value::F64(v as f64);
                trace!("Instruction: f64.convert_i32_s [{v}] -> [{res:.17}]");
                stack.push_value::<T>(res.into())?;
            }
            F64_CONVERT_I32_U => {
                decrement_fuel!(T::get_flat_cost(F64_CONVERT_I32_U));
                let v: i32 = stack.pop_value().try_into().unwrap_validated();
                let res: value::F64 = value::F64(v as u32 as f64);
                trace!("Instruction: f64.convert_i32_u [{v}] -> [{res:.17}]");
                stack.push_value::<T>(res.into())?;
            }
            F64_CONVERT_I64_S => {
                decrement_fuel!(T::get_flat_cost(F64_CONVERT_I64_S));
                let v: i64 = stack.pop_value().try_into().unwrap_validated();
                let res: value::F64 = value::F64(v as f64);
                trace!("Instruction: f64.convert_i64_s [{v}] -> [{res:.17}]");
                stack.push_value::<T>(res.into())?;
            }
            F64_CONVERT_I64_U => {
                decrement_fuel!(T::get_flat_cost(F64_CONVERT_I64_U));
                let v: i64 = stack.pop_value().try_into().unwrap_validated();
                let res: value::F64 = value::F64(v as u64 as f64);
                trace!("Instruction: f64.convert_i64_u [{v}] -> [{res:.17}]");
                stack.push_value::<T>(res.into())?;
            }
            F64_PROMOTE_F32 => {
                decrement_fuel!(T::get_flat_cost(F64_PROMOTE_F32));
                let v: value::F32 = stack.pop_value().try_into().unwrap_validated();
                let res: value::F64 = v.as_f64();
                trace!("Instruction: f64.promote_f32 [{v:.7}] -> [{res:.17}]");
                stack.push_value::<T>(res.into())?;
            }
            I32_REINTERPRET_F32 => {
                decrement_fuel!(T::get_flat_cost(I32_REINTERPRET_F32));
                let v: value::F32 = stack.pop_value().try_into().unwrap_validated();
                let res: i32 = v.reinterpret_as_i32();
                trace!("Instruction: i32.reinterpret_f32 [{v:.7}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            I64_REINTERPRET_F64 => {
                decrement_fuel!(T::get_flat_cost(I64_REINTERPRET_F64));
                let v: value::F64 = stack.pop_value().try_into().unwrap_validated();
                let res: i64 = v.reinterpret_as_i64();
                trace!("Instruction: i64.reinterpret_f64 [{v:.17}] -> [{res}]");
                stack.push_value::<T>(res.into())?;
            }
            F32_REINTERPRET_I32 => {
                decrement_fuel!(T::get_flat_cost(F32_REINTERPRET_I32));
                let v1: i32 = stack.pop_value().try_into().unwrap_validated();
                let res: value::F32 = value::F32::from_bits(v1 as u32);
                trace!("Instruction: f32.reinterpret_i32 [{v1}] -> [{res:.7}]");
                stack.push_value::<T>(res.into())?;
            }
            F64_REINTERPRET_I64 => {
                decrement_fuel!(T::get_flat_cost(F64_REINTERPRET_I64));
                let v1: i64 = stack.pop_value().try_into().unwrap_validated();
                let res: value::F64 = value::F64::from_bits(v1 as u64);
                trace!("Instruction: f64.reinterpret_i64 [{v1}] -> [{res:.17}]");
                stack.push_value::<T>(res.into())?;
            }
            REF_NULL => {
                decrement_fuel!(T::get_flat_cost(REF_NULL));
                let reftype = RefType::read(wasm).unwrap_validated();
                stack.push_value::<T>(Value::Ref(Ref::Null(reftype)))?;
                trace!("Instruction: ref.null '{:?}' -> [{:?}]", reftype, reftype);
            }
            REF_IS_NULL => {
                decrement_fuel!(T::get_flat_cost(REF_IS_NULL));
                let rref: Ref = stack.pop_value().try_into().unwrap_validated();
                let is_null = matches!(rref, Ref::Null(_));
                let res = if is_null { 1 } else { 0 };
                trace!("Instruction: ref.is_null [{}] -> [{}]", rref, res);
                stack.push_value::<T>(Value::I32(res))?;
            }
            REF_FUNC => {
                decrement_fuel!(T::get_flat_cost(REF_FUNC));
                let func_idx = wasm.read_var_u32().unwrap_validated() as FuncIdx;
                let func_addr = *store
                    .modules
                    .get(current_module)
                    .func_addrs
                    .get(func_idx)
                    .unwrap_validated();
                stack.push_value::<T>(Value::Ref(Ref::Func(func_addr)))?;
            }
            I32_EXTEND8_S => {
                decrement_fuel!(T::get_flat_cost(I32_EXTEND8_S));
                let mut v: u32 = stack.pop_value().try_into().unwrap_validated();
                if v | 0xFF != 0xFF {
                    trace!("Number v ({}) not contained in 8 bits, truncating", v);
                    v &= 0xFF;
                }
                let res = if v | 0x7F != 0x7F { v | 0xFFFFFF00 } else { v };
                stack.push_value::<T>(res.into())?;
                trace!("Instruction i32.extend8_s [{}] -> [{}]", v, res);
            }
            I32_EXTEND16_S => {
                decrement_fuel!(T::get_flat_cost(I32_EXTEND16_S));
                let mut v: u32 = stack.pop_value().try_into().unwrap_validated();
                if v | 0xFFFF != 0xFFFF {
                    trace!("Number v ({}) not contained in 16 bits, truncating", v);
                    v &= 0xFFFF;
                }
                let res = if v | 0x7FFF != 0x7FFF {
                    v | 0xFFFF0000
                } else {
                    v
                };
                stack.push_value::<T>(res.into())?;
                trace!("Instruction i32.extend16_s [{}] -> [{}]", v, res);
            }
            I64_EXTEND8_S => {
                decrement_fuel!(T::get_flat_cost(I64_EXTEND8_S));
                let mut v: u64 = stack.pop_value().try_into().unwrap_validated();
                if v | 0xFF != 0xFF {
                    trace!("Number v ({}) not contained in 8 bits, truncating", v);
                    v &= 0xFF;
                }
                let res = if v | 0x7F != 0x7F {
                    v | 0xFFFFFFFF_FFFFFF00
                } else {
                    v
                };
                stack.push_value::<T>(res.into())?;
                trace!("Instruction i64.extend8_s [{}] -> [{}]", v, res);
            }
            I64_EXTEND16_S => {
                decrement_fuel!(T::get_flat_cost(I64_EXTEND16_S));
                let mut v: u64 = stack.pop_value().try_into().unwrap_validated();
                if v | 0xFFFF != 0xFFFF {
                    trace!("Number v ({}) not contained in 16 bits, truncating", v);
                    v &= 0xFFFF;
                }
                let res = if v | 0x7FFF != 0x7FFF {
                    v | 0xFFFFFFFF_FFFF0000
                } else {
                    v
                };
                stack.push_value::<T>(res.into())?;
                trace!("Instruction i64.extend16_s [{}] -> [{}]", v, res);
            }
            I64_EXTEND32_S => {
                decrement_fuel!(T::get_flat_cost(I64_EXTEND32_S));
                let mut v: u64 = stack.pop_value().try_into().unwrap_validated();
                if v | 0xFFFF_FFFF != 0xFFFF_FFFF {
                    trace!("Number v ({}) not contained in 32 bits, truncating", v);
                    v &= 0xFFFF_FFFF;
                }
                let res = if v | 0x7FFF_FFFF != 0x7FFF_FFFF {
                    v | 0xFFFFFFFF_00000000
                } else {
                    v
                };
                stack.push_value::<T>(res.into())?;
                trace!("Instruction i64.extend32_s [{}] -> [{}]", v, res);
            }
            FD_EXTENSIONS => {
                let second_instr = wasm.read_var_u32().unwrap_or_else(|e| {
                    panic!("WASM Interpreter error (FD) at PC {:#x}: {:?}", wasm.pc, e);
                });
                #[cfg(debug_assertions)]
                crate::wasm::core::utils::print_beautiful_fd_extension(second_instr, wasm.pc);
                #[cfg(not(debug_assertions))]
                trace!(
                    "Read instruction byte {second_instr} at wasm_binary[{}]",
                    wasm.pc
                );

                decrement_fuel!(T::get_fd_extension_flat_cost(second_instr));
                crate::wasm::execution::simd_instructions::execute_simd_instruction(
                    second_instr,
                    stack,
                    wasm,
                    store,
                    current_module,
                )?;
            }
            /* 
                use crate::wasm::core::reader::types::opcode::fd_extensions::*;
                use crate::wasm::execution::simd_utils::*;
                
                // Helper closure to pop a V128 (as [u8; 16])
                let mut pop_v128 = || -> [u8; 16] {
                    stack.pop_value().try_into().unwrap_validated()
                };

                match second_instr {
                    V128_LOAD => {
                        decrement_fuel!(T::get_fd_extension_flat_cost(V128_LOAD));
                        let memarg = MemArg::read(wasm).unwrap_validated();
                        let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
                        let mem_addr = *store.modules.get(current_module).mem_addrs.first().unwrap_validated();
                        let mem_inst = store.memories.get(mem_addr);
                        let idx = calculate_mem_address(&memarg, relative_address)?;
                        let data = mem_inst.mem.load_bytes::<16>(idx).map_err(|e| {
                            crate::debugln!("WASM Trap: MemoryAccessOutOfBounds (load v128) at PC {:#x}", wasm.pc);
                            e
                        })?;
                        stack.push_value::<T>(Value::V128(data))?;
                    }
                    V128_LOAD8X8_S => {
                        decrement_fuel!(T::get_fd_extension_flat_cost(V128_LOAD8X8_S));
                        let memarg = MemArg::read(wasm).unwrap_validated();
                        let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
                        let mem_addr = *store.modules.get(current_module).mem_addrs.first().unwrap_validated();
                        let mem_inst = store.memories.get(mem_addr);
                        let idx = calculate_mem_address(&memarg, relative_address)?;
                        let data = mem_inst.mem.load_bytes::<8>(idx).map_err(|e| { e })?;
                        let lanes = to_lanes_8::<1, 8, i8>(data);
                        let mut res = [0i16; 8];
                        for i in 0..8 { res[i] = lanes[i] as i16; }
                        stack.push_value::<T>(Value::V128(from_lanes::<2, 8, i16>(res)))?;
                    }
                    V128_LOAD8X8_U => {
                        decrement_fuel!(T::get_fd_extension_flat_cost(V128_LOAD8X8_U));
                        let memarg = MemArg::read(wasm).unwrap_validated();
                        let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
                        let mem_addr = *store.modules.get(current_module).mem_addrs.first().unwrap_validated();
                        let mem_inst = store.memories.get(mem_addr);
                        let idx = calculate_mem_address(&memarg, relative_address)?;
                        let data = mem_inst.mem.load_bytes::<8>(idx).map_err(|e| { e })?;
                        let lanes = to_lanes_8::<1, 8, u8>(data);
                        let mut res = [0i16; 8];
                        for i in 0..8 { res[i] = lanes[i] as i16; }
                        stack.push_value::<T>(Value::V128(from_lanes::<2, 8, i16>(res)))?;
                    }
                    V128_LOAD16X4_S => {
                        decrement_fuel!(T::get_fd_extension_flat_cost(V128_LOAD16X4_S));
                        let memarg = MemArg::read(wasm).unwrap_validated();
                        let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
                        let mem_addr = *store.modules.get(current_module).mem_addrs.first().unwrap_validated();
                        let mem_inst = store.memories.get(mem_addr);
                        let idx = calculate_mem_address(&memarg, relative_address)?;
                        let data = mem_inst.mem.load_bytes::<8>(idx).map_err(|e| { e })?;
                        let lanes = to_lanes_8::<2, 4, i16>(data);
                        let mut res = [0i32; 4];
                        for i in 0..4 { res[i] = lanes[i] as i32; }
                        stack.push_value::<T>(Value::V128(from_lanes::<4, 4, i32>(res)))?;
                    }
                    V128_LOAD16X4_U => {
                        decrement_fuel!(T::get_fd_extension_flat_cost(V128_LOAD16X4_U));
                        let memarg = MemArg::read(wasm).unwrap_validated();
                        let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
                        let mem_addr = *store.modules.get(current_module).mem_addrs.first().unwrap_validated();
                        let mem_inst = store.memories.get(mem_addr);
                        let idx = calculate_mem_address(&memarg, relative_address)?;
                        let data = mem_inst.mem.load_bytes::<8>(idx).map_err(|e| { e })?;
                        let lanes = to_lanes_8::<2, 4, u16>(data);
                        let mut res = [0i32; 4];
                        for i in 0..4 { res[i] = lanes[i] as i32; }
                        stack.push_value::<T>(Value::V128(from_lanes::<4, 4, i32>(res)))?;
                    }
                    V128_LOAD32X2_S => {
                        decrement_fuel!(T::get_fd_extension_flat_cost(V128_LOAD32X2_S));
                        let memarg = MemArg::read(wasm).unwrap_validated();
                        let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
                        let mem_addr = *store.modules.get(current_module).mem_addrs.first().unwrap_validated();
                        let mem_inst = store.memories.get(mem_addr);
                        let idx = calculate_mem_address(&memarg, relative_address)?;
                        let data = mem_inst.mem.load_bytes::<8>(idx).map_err(|e| { e })?;
                        let lanes = to_lanes_8::<4, 2, i32>(data);
                        let mut res = [0i64; 2];
                        for i in 0..2 { res[i] = lanes[i] as i64; }
                        stack.push_value::<T>(Value::V128(from_lanes::<8, 2, i64>(res)))?;
                    }
                    V128_LOAD32X2_U => {
                        decrement_fuel!(T::get_fd_extension_flat_cost(V128_LOAD32X2_U));
                        let memarg = MemArg::read(wasm).unwrap_validated();
                        let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
                        let mem_addr = *store.modules.get(current_module).mem_addrs.first().unwrap_validated();
                        let mem_inst = store.memories.get(mem_addr);
                        let idx = calculate_mem_address(&memarg, relative_address)?;
                        let data = mem_inst.mem.load_bytes::<8>(idx).map_err(|e| { e })?;
                        let lanes = to_lanes_8::<4, 2, u32>(data);
                        let mut res = [0i64; 2];
                        for i in 0..2 { res[i] = lanes[i] as i64; }
                        stack.push_value::<T>(Value::V128(from_lanes::<8, 2, i64>(res)))?;
                    }
                    V128_LOAD8_SPLAT => {
                        decrement_fuel!(T::get_fd_extension_flat_cost(V128_LOAD8_SPLAT));
                        let memarg = MemArg::read(wasm).unwrap_validated();
                        let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
                        let mem_addr = *store.modules.get(current_module).mem_addrs.first().unwrap_validated();
                        let mem_inst = store.memories.get(mem_addr);
                        let idx = calculate_mem_address(&memarg, relative_address)?;
                        let data = mem_inst.mem.load_bytes::<1>(idx).map_err(|e| { e })?;
                        stack.push_value::<T>(Value::V128(splat(data)))?;
                    }
                    V128_LOAD16_SPLAT => {
                        decrement_fuel!(T::get_fd_extension_flat_cost(V128_LOAD16_SPLAT));
                        let memarg = MemArg::read(wasm).unwrap_validated();
                        let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
                        let mem_addr = *store.modules.get(current_module).mem_addrs.first().unwrap_validated();
                        let mem_inst = store.memories.get(mem_addr);
                        let idx = calculate_mem_address(&memarg, relative_address)?;
                        let data = mem_inst.mem.load_bytes::<2>(idx).map_err(|e| { e })?;
                        stack.push_value::<T>(Value::V128(splat(data)))?;
                    }
                    V128_LOAD32_SPLAT => {
                        decrement_fuel!(T::get_fd_extension_flat_cost(V128_LOAD32_SPLAT));
                        let memarg = MemArg::read(wasm).unwrap_validated();
                        let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
                        let mem_addr = *store.modules.get(current_module).mem_addrs.first().unwrap_validated();
                        let mem_inst = store.memories.get(mem_addr);
                        let idx = calculate_mem_address(&memarg, relative_address)?;
                        let data = mem_inst.mem.load_bytes::<4>(idx).map_err(|e| { e })?;
                        stack.push_value::<T>(Value::V128(splat(data)))?;
                    }
                    V128_LOAD64_SPLAT => {
                        decrement_fuel!(T::get_fd_extension_flat_cost(V128_LOAD64_SPLAT));
                        let memarg = MemArg::read(wasm).unwrap_validated();
                        let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
                        let mem_addr = *store.modules.get(current_module).mem_addrs.first().unwrap_validated();
                        let mem_inst = store.memories.get(mem_addr);
                        let idx = calculate_mem_address(&memarg, relative_address)?;
                        let data = mem_inst.mem.load_bytes::<8>(idx).map_err(|e| { e })?;
                        stack.push_value::<T>(Value::V128(splat(data)))?;
                    }
                    V128_LOAD32_ZERO => {
                        decrement_fuel!(T::get_fd_extension_flat_cost(V128_LOAD32_ZERO));
                        let memarg = MemArg::read(wasm).unwrap_validated();
                        let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
                        let mem_addr = *store.modules.get(current_module).mem_addrs.first().unwrap_validated();
                        let mem_inst = store.memories.get(mem_addr);
                        let idx = calculate_mem_address(&memarg, relative_address)?;
                        let data = mem_inst.mem.load_bytes::<4>(idx).map_err(|e| { e })?;
                        let mut res = [0u8; 16];
                        res[0..4].copy_from_slice(&data);
                        stack.push_value::<T>(Value::V128(res))?;
                    }
                    V128_LOAD64_ZERO => {
                        decrement_fuel!(T::get_fd_extension_flat_cost(V128_LOAD64_ZERO));
                        let memarg = MemArg::read(wasm).unwrap_validated();
                        let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
                        let mem_addr = *store.modules.get(current_module).mem_addrs.first().unwrap_validated();
                        let mem_inst = store.memories.get(mem_addr);
                        let idx = calculate_mem_address(&memarg, relative_address)?;
                        let data = mem_inst.mem.load_bytes::<8>(idx).map_err(|e| { e })?;
                        let mut res = [0u8; 16];
                        res[0..8].copy_from_slice(&data);
                        stack.push_value::<T>(Value::V128(res))?;
                    }
                    V128_STORE => {
                        decrement_fuel!(T::get_fd_extension_flat_cost(V128_STORE));
                        let memarg = MemArg::read(wasm).unwrap_validated();
                        let val: [u8; 16] = pop_v128();
                        let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
                        let mem_addr = *store.modules.get(current_module).mem_addrs.first().unwrap_validated();
                        let mem_inst = store.memories.get(mem_addr);
                        let idx = calculate_mem_address(&memarg, relative_address)?;
                        mem_inst.mem.store_bytes(idx, val).map_err(|e| { e })?;
                    }
                    V128_LOAD8_LANE => {
                        decrement_fuel!(T::get_fd_extension_flat_cost(V128_LOAD8_LANE));
                        let memarg = MemArg::read(wasm).unwrap_validated();
                        let lane_idx = wasm.read_u8().unwrap_validated();
                        let mut val: [u8; 16] = pop_v128();
                        let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
                        let mem_addr = *store.modules.get(current_module).mem_addrs.first().unwrap_validated();
                        let mem_inst = store.memories.get(mem_addr);
                        let idx = calculate_mem_address(&memarg, relative_address)?;
                        let byte = mem_inst.mem.load_bytes::<1>(idx).map_err(|e| { e })?;
                        val[lane_idx as usize] = byte[0];
                        stack.push_value::<T>(Value::V128(val))?;
                    }
                    V128_LOAD16_LANE => {
                        decrement_fuel!(T::get_fd_extension_flat_cost(V128_LOAD16_LANE));
                        let memarg = MemArg::read(wasm).unwrap_validated();
                        let lane_idx = wasm.read_u8().unwrap_validated();
                        let mut val: [u8; 16] = pop_v128();
                        let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
                        let mem_addr = *store.modules.get(current_module).mem_addrs.first().unwrap_validated();
                        let mem_inst = store.memories.get(mem_addr);
                        let idx = calculate_mem_address(&memarg, relative_address)?;
                        let bytes = mem_inst.mem.load_bytes::<2>(idx).map_err(|e| { e })?;
                        let offset = (lane_idx as usize) * 2;
                        val[offset..offset+2].copy_from_slice(&bytes);
                        stack.push_value::<T>(Value::V128(val))?;
                    }
                    V128_LOAD32_LANE => {
                        decrement_fuel!(T::get_fd_extension_flat_cost(V128_LOAD32_LANE));
                        let memarg = MemArg::read(wasm).unwrap_validated();
                        let lane_idx = wasm.read_u8().unwrap_validated();
                        let mut val: [u8; 16] = pop_v128();
                        let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
                        let mem_addr = *store.modules.get(current_module).mem_addrs.first().unwrap_validated();
                        let mem_inst = store.memories.get(mem_addr);
                        let idx = calculate_mem_address(&memarg, relative_address)?;
                        let bytes = mem_inst.mem.load_bytes::<4>(idx).map_err(|e| { e })?;
                        let offset = (lane_idx as usize) * 4;
                        val[offset..offset+4].copy_from_slice(&bytes);
                        stack.push_value::<T>(Value::V128(val))?;
                    }
                    V128_LOAD64_LANE => {
                        decrement_fuel!(T::get_fd_extension_flat_cost(V128_LOAD64_LANE));
                        let memarg = MemArg::read(wasm).unwrap_validated();
                        let lane_idx = wasm.read_u8().unwrap_validated();
                        let mut val: [u8; 16] = pop_v128();
                        let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
                        let mem_addr = *store.modules.get(current_module).mem_addrs.first().unwrap_validated();
                        let mem_inst = store.memories.get(mem_addr);
                        let idx = calculate_mem_address(&memarg, relative_address)?;
                        let bytes = mem_inst.mem.load_bytes::<8>(idx).map_err(|e| { e })?;
                        let offset = (lane_idx as usize) * 8;
                        val[offset..offset+8].copy_from_slice(&bytes);
                        stack.push_value::<T>(Value::V128(val))?;
                    }
                    V128_STORE8_LANE => {
                        decrement_fuel!(T::get_fd_extension_flat_cost(V128_STORE8_LANE));
                        let memarg = MemArg::read(wasm).unwrap_validated();
                        let lane_idx = wasm.read_u8().unwrap_validated();
                        let val: [u8; 16] = pop_v128();
                        let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
                        let mem_addr = *store.modules.get(current_module).mem_addrs.first().unwrap_validated();
                        let mem_inst = store.memories.get(mem_addr);
                        let idx = calculate_mem_address(&memarg, relative_address)?;
                        let byte = [val[lane_idx as usize]];
                        mem_inst.mem.store_bytes(idx, byte).map_err(|e| { e })?;
                    }
                    V128_STORE16_LANE => {
                        decrement_fuel!(T::get_fd_extension_flat_cost(V128_STORE16_LANE));
                        let memarg = MemArg::read(wasm).unwrap_validated();
                        let lane_idx = wasm.read_u8().unwrap_validated();
                        let val: [u8; 16] = pop_v128();
                        let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
                        let mem_addr = *store.modules.get(current_module).mem_addrs.first().unwrap_validated();
                        let mem_inst = store.memories.get(mem_addr);
                        let idx = calculate_mem_address(&memarg, relative_address)?;
                        let offset = (lane_idx as usize) * 2;
                        let mut bytes = [0u8; 2];
                        bytes.copy_from_slice(&val[offset..offset+2]);
                        mem_inst.mem.store_bytes(idx, bytes).map_err(|e| { e })?;
                    }
                    V128_STORE32_LANE => {
                        decrement_fuel!(T::get_fd_extension_flat_cost(V128_STORE32_LANE));
                        let memarg = MemArg::read(wasm).unwrap_validated();
                        let lane_idx = wasm.read_u8().unwrap_validated();
                        let val: [u8; 16] = pop_v128();
                        let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
                        let mem_addr = *store.modules.get(current_module).mem_addrs.first().unwrap_validated();
                        let mem_inst = store.memories.get(mem_addr);
                        let idx = calculate_mem_address(&memarg, relative_address)?;
                        let offset = (lane_idx as usize) * 4;
                        let mut bytes = [0u8; 4];
                        bytes.copy_from_slice(&val[offset..offset+4]);
                        mem_inst.mem.store_bytes(idx, bytes).map_err(|e| { e })?;
                    }
                    V128_STORE64_LANE => {
                        decrement_fuel!(T::get_fd_extension_flat_cost(V128_STORE64_LANE));
                        let memarg = MemArg::read(wasm).unwrap_validated();
                        let lane_idx = wasm.read_u8().unwrap_validated();
                        let val: [u8; 16] = pop_v128();
                        let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
                        let mem_addr = *store.modules.get(current_module).mem_addrs.first().unwrap_validated();
                        let mem_inst = store.memories.get(mem_addr);
                        let idx = calculate_mem_address(&memarg, relative_address)?;
                        let offset = (lane_idx as usize) * 8;
                        let mut bytes = [0u8; 8];
                        bytes.copy_from_slice(&val[offset..offset+8]);
                        mem_inst.mem.store_bytes(idx, bytes).map_err(|e| { e })?;
                    }
                    V128_CONST => {
                        let mut data = [0; 16];
                        for byte_ref in &mut data {
                            *byte_ref = wasm.read_u8().unwrap_validated();
                        }
                        stack.push_value::<T>(Value::V128(data))?;
                    }
                    I8X16_SHUFFLE => {
                        decrement_fuel!(T::get_fd_extension_flat_cost(I8X16_SHUFFLE));
                        let mut lanes = [0u8; 16];
                        for byte_ref in &mut lanes {
                            *byte_ref = wasm.read_u8().unwrap_validated();
                        }
                        let v2: [u8; 16] = pop_v128();
                        let v1: [u8; 16] = pop_v128();
                        stack.push_value::<T>(Value::V128(i8x16_shuffle(v1, v2, lanes)))?;
                    }
                    I8X16_EXTRACT_LANE_S => {
                        decrement_fuel!(T::get_fd_extension_flat_cost(I8X16_EXTRACT_LANE_S));
                        let lane_idx = wasm.read_u8().unwrap_validated();
                        let val: [u8; 16] = pop_v128();
                        let res = val[lane_idx as usize] as i8 as i32;
                        stack.push_value::<T>(Value::I32(res as u32))?;
                    }
                    I8X16_EXTRACT_LANE_U => {
                        decrement_fuel!(T::get_fd_extension_flat_cost(I8X16_EXTRACT_LANE_U));
                        let lane_idx = wasm.read_u8().unwrap_validated();
                        let val: [u8; 16] = pop_v128();
                        let res = val[lane_idx as usize] as u32;
                        stack.push_value::<T>(Value::I32(res))?;
                    }
                    I8X16_REPLACE_LANE => {
                        decrement_fuel!(T::get_fd_extension_flat_cost(I8X16_REPLACE_LANE));
                        let lane_idx = wasm.read_u8().unwrap_validated();
                        let x: u32 = stack.pop_value().try_into().unwrap_validated();
                        let mut val: [u8; 16] = pop_v128();
                        val[lane_idx as usize] = x as u8;
                        stack.push_value::<T>(Value::V128(val))?;
                    }
                    I16X8_EXTRACT_LANE_S => {
                        decrement_fuel!(T::get_fd_extension_flat_cost(I16X8_EXTRACT_LANE_S));
                        let lane_idx = wasm.read_u8().unwrap_validated();
                        let val: [u8; 16] = pop_v128();
                        let lanes = to_lanes::<2, 8, i16>(val);
                        let res = lanes[lane_idx as usize] as i32;
                        stack.push_value::<T>(Value::I32(res as u32))?;
                    }
                    I16X8_EXTRACT_LANE_U => {
                        decrement_fuel!(T::get_fd_extension_flat_cost(I16X8_EXTRACT_LANE_U));
                        let lane_idx = wasm.read_u8().unwrap_validated();
                        let val: [u8; 16] = pop_v128();
                        let lanes = to_lanes::<2, 8, u16>(val);
                        let res = lanes[lane_idx as usize] as u32;
                        stack.push_value::<T>(Value::I32(res))?;
                    }
                    I16X8_REPLACE_LANE => {
                        decrement_fuel!(T::get_fd_extension_flat_cost(I16X8_REPLACE_LANE));
                        let lane_idx = wasm.read_u8().unwrap_validated();
                        let x: u32 = stack.pop_value().try_into().unwrap_validated();
                        let val: [u8; 16] = pop_v128();
                        let mut lanes = to_lanes::<2, 8, u16>(val);
                        lanes[lane_idx as usize] = x as u16;
                        stack.push_value::<T>(Value::V128(from_lanes::<2, 8, u16>(lanes)))?;
                    }
                    I32X4_EXTRACT_LANE => {
                        decrement_fuel!(T::get_fd_extension_flat_cost(I32X4_EXTRACT_LANE));
                        let lane_idx = wasm.read_u8().unwrap_validated();
                        let val: [u8; 16] = pop_v128();
                        let lanes = to_lanes::<4, 4, u32>(val);
                        let res = lanes[lane_idx as usize];
                        stack.push_value::<T>(Value::I32(res))?;
                    }
                    I32X4_REPLACE_LANE => {
                        decrement_fuel!(T::get_fd_extension_flat_cost(I32X4_REPLACE_LANE));
                        let lane_idx = wasm.read_u8().unwrap_validated();
                        let x: u32 = stack.pop_value().try_into().unwrap_validated();
                        let val: [u8; 16] = pop_v128();
                        let mut lanes = to_lanes::<4, 4, u32>(val);
                        lanes[lane_idx as usize] = x;
                        stack.push_value::<T>(Value::V128(from_lanes::<4, 4, u32>(lanes)))?;
                    }
                    I64X2_EXTRACT_LANE => {
                        decrement_fuel!(T::get_fd_extension_flat_cost(I64X2_EXTRACT_LANE));
                        let lane_idx = wasm.read_u8().unwrap_validated();
                        let val: [u8; 16] = pop_v128();
                        let lanes = to_lanes::<8, 2, u64>(val);
                        let res = lanes[lane_idx as usize];
                        stack.push_value::<T>(Value::I64(res))?;
                    }
                    I64X2_REPLACE_LANE => {
                        decrement_fuel!(T::get_fd_extension_flat_cost(I64X2_REPLACE_LANE));
                        let lane_idx = wasm.read_u8().unwrap_validated();
                        let x: u64 = stack.pop_value().try_into().unwrap_validated();
                        let val: [u8; 16] = pop_v128();
                        let mut lanes = to_lanes::<8, 2, u64>(val);
                        lanes[lane_idx as usize] = x;
                        stack.push_value::<T>(Value::V128(from_lanes::<8, 2, u64>(lanes)))?;
                    }
                    F32X4_EXTRACT_LANE => {
                        decrement_fuel!(T::get_fd_extension_flat_cost(F32X4_EXTRACT_LANE));
                        let lane_idx = wasm.read_u8().unwrap_validated();
                        let val: [u8; 16] = pop_v128();
                        let lanes = to_lanes::<4, 4, F32>(val);
                        let res = lanes[lane_idx as usize];
                        stack.push_value::<T>(Value::F32(res))?;
                    }
                    F32X4_REPLACE_LANE => {
                        decrement_fuel!(T::get_fd_extension_flat_cost(F32X4_REPLACE_LANE));
                        let lane_idx = wasm.read_u8().unwrap_validated();
                        let x: F32 = stack.pop_value().try_into().unwrap_validated();
                        let val: [u8; 16] = pop_v128();
                        let mut lanes = to_lanes::<4, 4, F32>(val);
                        lanes[lane_idx as usize] = x;
                        stack.push_value::<T>(Value::V128(from_lanes::<4, 4, F32>(lanes)))?;
                    }
                    F64X2_EXTRACT_LANE => {
                        decrement_fuel!(T::get_fd_extension_flat_cost(F64X2_EXTRACT_LANE));
                        let lane_idx = wasm.read_u8().unwrap_validated();
                        let val: [u8; 16] = pop_v128();
                        let lanes = to_lanes::<8, 2, F64>(val);
                        let res = lanes[lane_idx as usize];
                        stack.push_value::<T>(Value::F64(res))?;
                    }
                    F64X2_REPLACE_LANE => {
                        decrement_fuel!(T::get_fd_extension_flat_cost(F64X2_REPLACE_LANE));
                        let lane_idx = wasm.read_u8().unwrap_validated();
                        let x: F64 = stack.pop_value().try_into().unwrap_validated();
                        let val: [u8; 16] = pop_v128();
                        let mut lanes = to_lanes::<8, 2, F64>(val);
                        lanes[lane_idx as usize] = x;
                        stack.push_value::<T>(Value::V128(from_lanes::<8, 2, F64>(lanes)))?;
                    }
                    I8X16_SPLAT => {
                        decrement_fuel!(T::get_fd_extension_flat_cost(I8X16_SPLAT));
                        let x: i32 = stack.pop_value().try_into().unwrap_validated();
                        stack.push_value::<T>(Value::V128(splat([x as u8])))?;
                    }
                    I16X8_SPLAT => {
                        decrement_fuel!(T::get_fd_extension_flat_cost(I16X8_SPLAT));
                        let x: i32 = stack.pop_value().try_into().unwrap_validated();
                        stack.push_value::<T>(Value::V128(splat((x as u16).to_le_bytes())))?;
                    }
                    I32X4_SPLAT => {
                        decrement_fuel!(T::get_fd_extension_flat_cost(I32X4_SPLAT));
                        let x: i32 = stack.pop_value().try_into().unwrap_validated();
                        stack.push_value::<T>(Value::V128(splat((x as u32).to_le_bytes())))?;
                    }
                    I64X2_SPLAT => {
                        decrement_fuel!(T::get_fd_extension_flat_cost(I64X2_SPLAT));
                        let x: i64 = stack.pop_value().try_into().unwrap_validated();
                        stack.push_value::<T>(Value::V128(splat((x as u64).to_le_bytes())))?;
                    }
                    F32X4_SPLAT => {
                        decrement_fuel!(T::get_fd_extension_flat_cost(F32X4_SPLAT));
                        let x: F32 = stack.pop_value().try_into().unwrap_validated();
                        stack.push_value::<T>(Value::V128(splat(x.to_le_bytes())))?;
                    }
                    F64X2_SPLAT => {
                        decrement_fuel!(T::get_fd_extension_flat_cost(F64X2_SPLAT));
                        let x: F64 = stack.pop_value().try_into().unwrap_validated();
                        stack.push_value::<T>(Value::V128(splat(x.to_le_bytes())))?;
                    }
                    I8X16_EQ => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i8x16_eq(v1, v2)))?; },
                    I8X16_NE => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i8x16_ne(v1, v2)))?; },
                    I8X16_LT_S => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i8x16_lt_s(v1, v2)))?; },
                    I8X16_LT_U => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i8x16_lt_u(v1, v2)))?; },
                    I8X16_GT_S => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i8x16_gt_s(v1, v2)))?; },
                    I8X16_GT_U => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i8x16_gt_u(v1, v2)))?; },
                    I8X16_LE_S => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i8x16_le_s(v1, v2)))?; },
                    I8X16_LE_U => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i8x16_le_u(v1, v2)))?; },
                    I8X16_GE_S => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i8x16_ge_s(v1, v2)))?; },
                    I8X16_GE_U => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i8x16_ge_u(v1, v2)))?; },
                    I16X8_EQ => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i16x8_eq(v1, v2)))?; },
                    I16X8_NE => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i16x8_ne(v1, v2)))?; },
                    I16X8_LT_S => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i16x8_lt_s(v1, v2)))?; },
                    I16X8_LT_U => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i16x8_lt_u(v1, v2)))?; },
                    I16X8_GT_S => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i16x8_gt_s(v1, v2)))?; },
                    I16X8_GT_U => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i16x8_gt_u(v1, v2)))?; },
                    I16X8_LE_S => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i16x8_le_s(v1, v2)))?; },
                    I16X8_LE_U => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i16x8_le_u(v1, v2)))?; },
                    I16X8_GE_S => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i16x8_ge_s(v1, v2)))?; },
                    I16X8_GE_U => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i16x8_ge_u(v1, v2)))?; },
                    I32X4_EQ => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i32x4_eq(v1, v2)))?; },
                    I32X4_NE => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i32x4_ne(v1, v2)))?; },
                    I32X4_LT_S => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i32x4_lt_s(v1, v2)))?; },
                    I32X4_LT_U => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i32x4_lt_u(v1, v2)))?; },
                    I32X4_GT_S => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i32x4_gt_s(v1, v2)))?; },
                    I32X4_GT_U => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i32x4_gt_u(v1, v2)))?; },
                    I32X4_LE_S => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i32x4_le_s(v1, v2)))?; },
                    I32X4_LE_U => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i32x4_le_u(v1, v2)))?; },
                    I32X4_GE_S => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i32x4_ge_s(v1, v2)))?; },
                    I32X4_GE_U => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i32x4_ge_u(v1, v2)))?; },
                    I64X2_EQ => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i64x2_eq(v1, v2)))?; },
                    I64X2_NE => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i64x2_ne(v1, v2)))?; },
                    I64X2_LT_S => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i64x2_lt_s(v1, v2)))?; },
                    I64X2_GT_S => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i64x2_gt_s(v1, v2)))?; },
                    I64X2_LE_S => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i64x2_le_s(v1, v2)))?; },
                    I64X2_GE_S => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i64x2_ge_s(v1, v2)))?; },
                    F32X4_EQ => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f32x4_eq(v1, v2)))?; },
                    F32X4_NE => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f32x4_ne(v1, v2)))?; },
                    F32X4_LT => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f32x4_lt(v1, v2)))?; },
                    F32X4_GT => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f32x4_gt(v1, v2)))?; },
                    F32X4_LE => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f32x4_le(v1, v2)))?; },
                    F32X4_GE => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f32x4_ge(v1, v2)))?; },
                    F64X2_EQ => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f64x2_eq(v1, v2)))?; },
                    F64X2_NE => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f64x2_ne(v1, v2)))?; },
                    F64X2_LT => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f64x2_lt(v1, v2)))?; },
                    F64X2_GT => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f64x2_gt(v1, v2)))?; },
                    F64X2_LE => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f64x2_le(v1, v2)))?; },
                    F64X2_GE => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f64x2_ge(v1, v2)))?; },
                    V128_NOT => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(v128_not(v1)))?; },
                    V128_AND => {
                        let v2 = pop_v128();
                        let v1 = pop_v128();
                        stack.push_value::<T>(Value::V128(v128_and(v1, v2)))?;
                    }
                    V128_ANDNOT => {
                        let v2 = pop_v128();
                        let v1 = pop_v128();
                        stack.push_value::<T>(Value::V128(v128_andnot(v1, v2)))?;
                    }
                    V128_OR => {
                        let v2 = pop_v128();
                        let v1 = pop_v128();
                        stack.push_value::<T>(Value::V128(v128_or(v1, v2)))?;
                    }
                    V128_XOR => {
                        let v2 = pop_v128();
                        let v1 = pop_v128();
                        stack.push_value::<T>(Value::V128(v128_xor(v1, v2)))?;
                    }
                    V128_BITSELECT => {
                        let c = pop_v128();
                        let v2 = pop_v128();
                        let v1 = pop_v128();
                        stack.push_value::<T>(Value::V128(v128_bitselect(v1, v2, c)))?;
                    }
                    V128_ANY_TRUE => { let v1 = pop_v128(); stack.push_value::<T>(Value::I32(if v128_any_true(v1) { 1 } else { 0 }))?; },
                    I8X16_ALL_TRUE => { let v1 = pop_v128(); stack.push_value::<T>(Value::I32(if i8x16_all_true(v1) { 1 } else { 0 }))?; },
                    I8X16_BITMASK => { let v1 = pop_v128(); stack.push_value::<T>(Value::I32(i8x16_bitmask(v1) as u32))?; },
                    I16X8_ALL_TRUE => { let v1 = pop_v128(); stack.push_value::<T>(Value::I32(if i16x8_all_true(v1) { 1 } else { 0 }))?; },
                    I16X8_BITMASK => { let v1 = pop_v128(); stack.push_value::<T>(Value::I32(i16x8_bitmask(v1) as u32))?; },
                    I32X4_ALL_TRUE => { let v1 = pop_v128(); stack.push_value::<T>(Value::I32(if i32x4_all_true(v1) { 1 } else { 0 }))?; },
                    I32X4_BITMASK => { let v1 = pop_v128(); stack.push_value::<T>(Value::I32(i32x4_bitmask(v1) as u32))?; },
                    I64X2_ALL_TRUE => { let v1 = pop_v128(); stack.push_value::<T>(Value::I32(if i64x2_all_true(v1) { 1 } else { 0 }))?; },
                    I64X2_BITMASK => { let v1 = pop_v128(); stack.push_value::<T>(Value::I32(i64x2_bitmask(v1) as u32))?; },
                    I8X16_ABS => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i8x16_abs(v1)))?; },
                    I8X16_NEG => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i8x16_neg(v1)))?; },
                    I8X16_POPCNT => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i8x16_popcnt(v1)))?; },
                    I8X16_ADD => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i8x16_add(v1, v2)))?; },
                    I8X16_SUB => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i8x16_sub(v1, v2)))?; },
                    I8X16_MIN_S => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i8x16_min_s(v1, v2)))?; },
                    I8X16_MIN_U => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i8x16_min_u(v1, v2)))?; },
                    I8X16_MAX_S => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i8x16_max_s(v1, v2)))?; },
                    I8X16_MAX_U => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i8x16_max_u(v1, v2)))?; },
                    I8X16_AVGR_U => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i8x16_avgr_u(v1, v2)))?; },
                    I8X16_ADD_SAT_S => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i8x16_add_sat_s(v1, v2)))?; },
                    I8X16_ADD_SAT_U => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i8x16_add_sat_u(v1, v2)))?; },
                    I8X16_SUB_SAT_S => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i8x16_sub_sat_s(v1, v2)))?; },
                    I8X16_SUB_SAT_U => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i8x16_sub_sat_u(v1, v2)))?; },
                    I16X8_ABS => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i16x8_abs(v1)))?; },
                    I16X8_NEG => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i16x8_neg(v1)))?; },
                    I16X8_ADD => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i16x8_add(v1, v2)))?; },
                    I16X8_SUB => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i16x8_sub(v1, v2)))?; },
                    I16X8_MUL => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i16x8_mul(v1, v2)))?; },
                    I16X8_MIN_S => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i16x8_min_s(v1, v2)))?; },
                    I16X8_MIN_U => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i16x8_min_u(v1, v2)))?; },
                    I16X8_MAX_S => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i16x8_max_s(v1, v2)))?; },
                    I16X8_MAX_U => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i16x8_max_u(v1, v2)))?; },
                    I16X8_AVGR_U => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i16x8_avgr_u(v1, v2)))?; },
                    I16X8_ADD_SAT_S => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i16x8_add_sat_s(v1, v2)))?; },
                    I16X8_ADD_SAT_U => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i16x8_add_sat_u(v1, v2)))?; },
                    I16X8_SUB_SAT_S => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i16x8_sub_sat_s(v1, v2)))?; },
                    I16X8_SUB_SAT_U => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i16x8_sub_sat_u(v1, v2)))?; },
                    I32X4_ABS => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i32x4_abs(v1)))?; },
                    I32X4_NEG => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i32x4_neg(v1)))?; },
                    I32X4_ADD => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i32x4_add(v1, v2)))?; },
                    I32X4_SUB => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i32x4_sub(v1, v2)))?; },
                    I32X4_MUL => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i32x4_mul(v1, v2)))?; },
                    I32X4_MIN_S => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i32x4_min_s(v1, v2)))?; },
                    I32X4_MIN_U => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i32x4_min_u(v1, v2)))?; },
                    I32X4_MAX_S => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i32x4_max_s(v1, v2)))?; },
                    I32X4_MAX_U => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i32x4_max_u(v1, v2)))?; },
                    I64X2_ABS => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i64x2_abs(v1)))?; },
                    I64X2_NEG => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i64x2_neg(v1)))?; },
                    I64X2_ADD => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i64x2_add(v1, v2)))?; },
                    I64X2_SUB => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i64x2_sub(v1, v2)))?; },
                    I64X2_MUL => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i64x2_mul(v1, v2)))?; },
                    F32X4_ABS => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f32x4_abs(v1)))?; },
                    F32X4_NEG => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f32x4_neg(v1)))?; },
                    F32X4_SQRT => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f32x4_sqrt(v1)))?; },
                    F32X4_CEIL => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f32x4_ceil(v1)))?; },
                    F32X4_FLOOR => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f32x4_floor(v1)))?; },
                    F32X4_TRUNC => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f32x4_trunc(v1)))?; },
                    F32X4_NEAREST => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f32x4_nearest(v1)))?; },
                    F32X4_ADD => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f32x4_add(v1, v2)))?; },
                    F32X4_SUB => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f32x4_sub(v1, v2)))?; },
                    F32X4_MUL => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f32x4_mul(v1, v2)))?; },
                    F32X4_DIV => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f32x4_div(v1, v2)))?; },
                    F32X4_MIN => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f32x4_min(v1, v2)))?; },
                    F32X4_MAX => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f32x4_max(v1, v2)))?; },
                    F32X4_PMIN => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f32x4_pmin(v1, v2)))?; },
                    F32X4_PMAX => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f32x4_pmax(v1, v2)))?; },
                    F64X2_ABS => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f64x2_abs(v1)))?; },
                    F64X2_NEG => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f64x2_neg(v1)))?; },
                    F64X2_SQRT => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f64x2_sqrt(v1)))?; },
                    F64X2_CEIL => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f64x2_ceil(v1)))?; },
                    F64X2_FLOOR => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f64x2_floor(v1)))?; },
                    F64X2_TRUNC => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f64x2_trunc(v1)))?; },
                    F64X2_NEAREST => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f64x2_nearest(v1)))?; },
                    F64X2_ADD => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f64x2_add(v1, v2)))?; },
                    F64X2_SUB => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f64x2_sub(v1, v2)))?; },
                    F64X2_MUL => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f64x2_mul(v1, v2)))?; },
                    F64X2_DIV => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f64x2_div(v1, v2)))?; },
                    F64X2_MIN => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f64x2_min(v1, v2)))?; },
                    F64X2_MAX => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f64x2_max(v1, v2)))?; },
                    F64X2_PMIN => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f64x2_pmin(v1, v2)))?; },
                    F64X2_PMAX => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f64x2_pmax(v1, v2)))?; },
                    I8X16_SHL => { let v2: i32 = stack.pop_value().try_into().unwrap_validated(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i8x16_shl(v1, v2 as u32)))?; },
                    I8X16_SHR_S => { let v2: i32 = stack.pop_value().try_into().unwrap_validated(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i8x16_shr_s(v1, v2 as u32)))?; },
                    I8X16_SHR_U => { let v2: i32 = stack.pop_value().try_into().unwrap_validated(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i8x16_shr_u(v1, v2 as u32)))?; },
                    I16X8_SHL => { let v2: i32 = stack.pop_value().try_into().unwrap_validated(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i16x8_shl(v1, v2 as u32)))?; },
                    I16X8_SHR_S => { let v2: i32 = stack.pop_value().try_into().unwrap_validated(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i16x8_shr_s(v1, v2 as u32)))?; },
                    I16X8_SHR_U => { let v2: i32 = stack.pop_value().try_into().unwrap_validated(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i16x8_shr_u(v1, v2 as u32)))?; },
                    I32X4_SHL => { let v2: i32 = stack.pop_value().try_into().unwrap_validated(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i32x4_shl(v1, v2 as u32)))?; },
                    I32X4_SHR_S => { let v2: i32 = stack.pop_value().try_into().unwrap_validated(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i32x4_shr_s(v1, v2 as u32)))?; },
                    I32X4_SHR_U => { let v2: i32 = stack.pop_value().try_into().unwrap_validated(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i32x4_shr_u(v1, v2 as u32)))?; },
                    I64X2_SHL => { let v2: i32 = stack.pop_value().try_into().unwrap_validated(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i64x2_shl(v1, v2 as u32)))?; },
                    I64X2_SHR_S => { let v2: i32 = stack.pop_value().try_into().unwrap_validated(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i64x2_shr_s(v1, v2 as u32)))?; },
                    I64X2_SHR_U => { let v2: i32 = stack.pop_value().try_into().unwrap_validated(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i64x2_shr_u(v1, v2 as u32)))?; },
                    I8X16_NARROW_I16X8_S => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i8x16_narrow_i16x8_s(v1, v2)))?; },
                    I8X16_NARROW_I16X8_U => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i8x16_narrow_i16x8_u(v1, v2)))?; },
                    I16X8_NARROW_I32X4_S => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i16x8_narrow_i32x4_s(v1, v2)))?; },
                    I16X8_NARROW_I32X4_U => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i16x8_narrow_i32x4_u(v1, v2)))?; },
                    I16X8_EXTEND_LOW_I8X16_S => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i16x8_extend_low_i8x16_s(v1)))?; },
                    I16X8_EXTEND_HIGH_I8X16_S => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i16x8_extend_high_i8x16_s(v1)))?; },
                    I16X8_EXTEND_LOW_I8X16_U => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i16x8_extend_low_i8x16_u(v1)))?; },
                    I16X8_EXTEND_HIGH_I8X16_U => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i16x8_extend_high_i8x16_u(v1)))?; },
                    I32X4_EXTEND_LOW_I16X8_S => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i32x4_extend_low_i16x8_s(v1)))?; },
                    I32X4_EXTEND_HIGH_I16X8_S => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i32x4_extend_high_i16x8_s(v1)))?; },
                    I32X4_EXTEND_LOW_I16X8_U => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i32x4_extend_low_i16x8_u(v1)))?; },
                    I32X4_EXTEND_HIGH_I16X8_U => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i32x4_extend_high_i16x8_u(v1)))?; },
                    I64X2_EXTEND_LOW_I32X4_S => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i64x2_extend_low_i32x4_s(v1)))?; },
                    I64X2_EXTEND_HIGH_I32X4_S => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i64x2_extend_high_i32x4_s(v1)))?; },
                    I64X2_EXTEND_LOW_I32X4_U => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i64x2_extend_low_i32x4_u(v1)))?; },
                    I64X2_EXTEND_HIGH_I32X4_U => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i64x2_extend_high_i32x4_u(v1)))?; },
                    I16X8_EXTMUL_LOW_I8X16_S => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i16x8_extmul_low_i8x16_s(v1, v2)))?; },
                    I16X8_EXTMUL_HIGH_I8X16_S => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i16x8_extmul_high_i8x16_s(v1, v2)))?; },
                    I16X8_EXTMUL_LOW_I8X16_U => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i16x8_extmul_low_i8x16_u(v1, v2)))?; },
                    I16X8_EXTMUL_HIGH_I8X16_U => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i16x8_extmul_high_i8x16_u(v1, v2)))?; },
                    I32X4_EXTMUL_LOW_I16X8_S => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i32x4_extmul_low_i16x8_s(v1, v2)))?; },
                    I32X4_EXTMUL_HIGH_I16X8_S => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i32x4_extmul_high_i16x8_s(v1, v2)))?; },
                    I32X4_EXTMUL_LOW_I16X8_U => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i32x4_extmul_low_i16x8_u(v1, v2)))?; },
                    I32X4_EXTMUL_HIGH_I16X8_U => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i32x4_extmul_high_i16x8_u(v1, v2)))?; },
                    I64X2_EXTMUL_LOW_I32X4_S => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i64x2_extmul_low_i32x4_s(v1, v2)))?; },
                    I64X2_EXTMUL_HIGH_I32X4_S => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i64x2_extmul_high_i32x4_s(v1, v2)))?; },
                    I64X2_EXTMUL_LOW_I32X4_U => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i64x2_extmul_low_i32x4_u(v1, v2)))?; },
                    I64X2_EXTMUL_HIGH_I32X4_U => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i64x2_extmul_high_i32x4_u(v1, v2)))?; },
                    I16X8_EXTADD_PAIRWISE_I8X16_S => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i16x8_extadd_pairwise_i8x16_s(v1)))?; },
                    I16X8_EXTADD_PAIRWISE_I8X16_U => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i16x8_extadd_pairwise_i8x16_u(v1)))?; },
                    I32X4_EXTADD_PAIRWISE_I16X8_S => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i32x4_extadd_pairwise_i16x8_s(v1)))?; },
                    I32X4_EXTADD_PAIRWISE_I16X8_U => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i32x4_extadd_pairwise_i16x8_u(v1)))?; },
                    I32X4_DOT_I16X8_S => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i32x4_dot_i16x8_s(v1, v2)))?; },
                    I16X8_Q15MULRSAT_S => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i16x8_q15mulrsat_s(v1, v2)))?; },
                    I8X16_SWIZZLE => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i8x16_swizzle(v1, v2)))?; },
                    I32X4_TRUNC_SAT_F32X4_S => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i32x4_trunc_sat_f32x4_s(v1)))?; },
                    I32X4_TRUNC_SAT_F32X4_U => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i32x4_trunc_sat_f32x4_u(v1)))?; },
                    F32X4_CONVERT_I32X4_S => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f32x4_convert_i32x4_s(v1)))?; },
                    F32X4_CONVERT_I32X4_U => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f32x4_convert_i32x4_u(v1)))?; },
                    I32X4_TRUNC_SAT_F64X2_S_ZERO => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i32x4_trunc_sat_f64x2_s_zero(v1)))?; },
                    I32X4_TRUNC_SAT_F64X2_U_ZERO => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i32x4_trunc_sat_f64x2_u_zero(v1)))?; },
                    F64X2_CONVERT_LOW_I32X4_S => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f64x2_convert_low_i32x4_s(v1)))?; },
                    F64X2_CONVERT_LOW_I32X4_U => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f64x2_convert_low_i32x4_u(v1)))?; },
                    I8X16_RELAXED_SWIZZLE => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i8x16_swizzle(v1, v2)))?; },
                    F32X4_RELAXED_MADD | F64X2_RELAXED_MADD => {
                        let c = pop_v128();
                        let b = pop_v128();
                        let a = pop_v128();
                        if second_instr == F32X4_RELAXED_MADD {
                             stack.push_value::<T>(Value::V128(f32x4_add(f32x4_mul(a, b), c)))?;
                        } else {
                             stack.push_value::<T>(Value::V128(f64x2_add(f64x2_mul(a, b), c)))?;
                        }
                    }
                    F32X4_RELAXED_NMADD | F64X2_RELAXED_NMADD => {
                        let c = pop_v128();
                        let b = pop_v128();
                        let a = pop_v128();
                        if second_instr == F32X4_RELAXED_NMADD {
                             stack.push_value::<T>(Value::V128(f32x4_add(f32x4_neg(f32x4_mul(a, b)), c)))?;
                        } else {
                             stack.push_value::<T>(Value::V128(f64x2_add(f64x2_neg(f64x2_mul(a, b)), c)))?;
                        }
                    }
                    F32X4_RELAXED_MAX => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f32x4_max(v1, v2)))?; },
                    F32X4_RELAXED_MIN => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f32x4_min(v1, v2)))?; },
                    F64X2_RELAXED_MAX => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f64x2_max(v1, v2)))?; },
                    F64X2_RELAXED_MIN => { let v2 = pop_v128(); let v1 = pop_v128(); stack.push_value::<T>(Value::V128(f64x2_min(v1, v2)))?; },
                    I8X16_RELAXED_LANESELECT | I16X8_RELAXED_LANESELECT | I32X4_RELAXED_LANESELECT | I64X2_RELAXED_LANESELECT => {
                        let c = pop_v128();
                        let v2 = pop_v128();
                        let v1 = pop_v128();
                        stack.push_value::<T>(Value::V128(v128_bitselect(v1, v2, c)))?;
                    }
                    I32X4_RELAXED_TRUNC_F32X4_S => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i32x4_trunc_sat_f32x4_s(v1)))?; },
                    I32X4_RELAXED_TRUNC_F32X4_U => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i32x4_trunc_sat_f32x4_u(v1)))?; },
                    I32X4_RELAXED_TRUNC_F64X2_S_ZERO => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i32x4_trunc_sat_f64x2_s_zero(v1)))?; },
                    I32X4_RELAXED_TRUNC_F64X2_U_ZERO => { let v1 = pop_v128(); stack.push_value::<T>(Value::V128(i32x4_trunc_sat_f64x2_u_zero(v1)))?; },
                    _ => {
                        crate::debugln!("WASM Trap: Unimplemented FD_EXTENSION (SIMD) opcode {:#x} at PC {:#x}", second_instr, wasm.pc);
                        return Err(RuntimeError::Trap(TrapError::ReachedUnreachable));
                    }
                }
            }                    V128_AND => {
                        let v2 = stack.pop_value().try_into().unwrap_validated();
                        let v1 = stack.pop_value().try_into().unwrap_validated();
                        stack.push_value::<T>(Value::V128(v128_and(v1, v2)))?;
                    }
                    V128_ANDNOT => {
                        let v2 = stack.pop_value().try_into().unwrap_validated();
                        let v1 = stack.pop_value().try_into().unwrap_validated();
                        stack.push_value::<T>(Value::V128(v128_andnot(v1, v2)))?;
                    }
                    V128_OR => {
                        let v2 = stack.pop_value().try_into().unwrap_validated();
                        let v1 = stack.pop_value().try_into().unwrap_validated();
                        stack.push_value::<T>(Value::V128(v128_or(v1, v2)))?;
                    }
                    V128_XOR => {
                        let v2 = stack.pop_value().try_into().unwrap_validated();
                        let v1 = stack.pop_value().try_into().unwrap_validated();
                        stack.push_value::<T>(Value::V128(v128_xor(v1, v2)))?;
                    }
                    V128_BITSELECT => {
                        let c = stack.pop_value().try_into().unwrap_validated();
                        let v2 = stack.pop_value().try_into().unwrap_validated();
                        let v1 = stack.pop_value().try_into().unwrap_validated();
                        stack.push_value::<T>(Value::V128(v128_bitselect(v1, v2, c)))?;
                    }
                    V128_ANY_TRUE => stack.push_value::<T>(Value::I32(if v128_any_true(stack.pop_value().try_into().unwrap_validated()) { 1 } else { 0 }))?,
                    I8X16_ALL_TRUE => stack.push_value::<T>(Value::I32(if i8x16_all_true(stack.pop_value().try_into().unwrap_validated()) { 1 } else { 0 }))?,
                    I8X16_BITMASK => stack.push_value::<T>(Value::I32(i8x16_bitmask(stack.pop_value().try_into().unwrap_validated()) as u32))?,
                    I16X8_ALL_TRUE => stack.push_value::<T>(Value::I32(if i16x8_all_true(stack.pop_value().try_into().unwrap_validated()) { 1 } else { 0 }))?,
                    I16X8_BITMASK => stack.push_value::<T>(Value::I32(i16x8_bitmask(stack.pop_value().try_into().unwrap_validated()) as u32))?,
                    I32X4_ALL_TRUE => stack.push_value::<T>(Value::I32(if i32x4_all_true(stack.pop_value().try_into().unwrap_validated()) { 1 } else { 0 }))?,
                    I32X4_BITMASK => stack.push_value::<T>(Value::I32(i32x4_bitmask(stack.pop_value().try_into().unwrap_validated()) as u32))?,
                    I64X2_ALL_TRUE => stack.push_value::<T>(Value::I32(if i64x2_all_true(stack.pop_value().try_into().unwrap_validated()) { 1 } else { 0 }))?,
                    I64X2_BITMASK => stack.push_value::<T>(Value::I32(i64x2_bitmask(stack.pop_value().try_into().unwrap_validated()) as u32))?,
                    I8X16_ABS => stack.push_value::<T>(Value::V128(i8x16_abs(stack.pop_value().try_into().unwrap_validated())))?,
                    I8X16_NEG => stack.push_value::<T>(Value::V128(i8x16_neg(stack.pop_value().try_into().unwrap_validated())))?,
                    I8X16_POPCNT => stack.push_value::<T>(Value::V128(i8x16_popcnt(stack.pop_value().try_into().unwrap_validated())))?,
                    I8X16_ADD => stack.push_value::<T>(Value::V128(i8x16_add(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I8X16_SUB => stack.push_value::<T>(Value::V128(i8x16_sub(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I8X16_MIN_S => stack.push_value::<T>(Value::V128(i8x16_min_s(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I8X16_MIN_U => stack.push_value::<T>(Value::V128(i8x16_min_u(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I8X16_MAX_S => stack.push_value::<T>(Value::V128(i8x16_max_s(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I8X16_MAX_U => stack.push_value::<T>(Value::V128(i8x16_max_u(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I8X16_AVGR_U => stack.push_value::<T>(Value::V128(i8x16_avgr_u(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I8X16_ADD_SAT_S => stack.push_value::<T>(Value::V128(i8x16_add_sat_s(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I8X16_ADD_SAT_U => stack.push_value::<T>(Value::V128(i8x16_add_sat_u(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I8X16_SUB_SAT_S => stack.push_value::<T>(Value::V128(i8x16_sub_sat_s(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I8X16_SUB_SAT_U => stack.push_value::<T>(Value::V128(i8x16_sub_sat_u(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I16X8_ABS => stack.push_value::<T>(Value::V128(i16x8_abs(stack.pop_value().try_into().unwrap_validated())))?,
                    I16X8_NEG => stack.push_value::<T>(Value::V128(i16x8_neg(stack.pop_value().try_into().unwrap_validated())))?,
                    I16X8_ADD => stack.push_value::<T>(Value::V128(i16x8_add(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I16X8_SUB => stack.push_value::<T>(Value::V128(i16x8_sub(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I16X8_MUL => stack.push_value::<T>(Value::V128(i16x8_mul(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I16X8_MIN_S => stack.push_value::<T>(Value::V128(i16x8_min_s(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I16X8_MIN_U => stack.push_value::<T>(Value::V128(i16x8_min_u(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I16X8_MAX_S => stack.push_value::<T>(Value::V128(i16x8_max_s(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I16X8_MAX_U => stack.push_value::<T>(Value::V128(i16x8_max_u(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I16X8_AVGR_U => stack.push_value::<T>(Value::V128(i16x8_avgr_u(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I16X8_ADD_SAT_S => stack.push_value::<T>(Value::V128(i16x8_add_sat_s(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I16X8_ADD_SAT_U => stack.push_value::<T>(Value::V128(i16x8_add_sat_u(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I16X8_SUB_SAT_S => stack.push_value::<T>(Value::V128(i16x8_sub_sat_s(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I16X8_SUB_SAT_U => stack.push_value::<T>(Value::V128(i16x8_sub_sat_u(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I32X4_ABS => stack.push_value::<T>(Value::V128(i32x4_abs(stack.pop_value().try_into().unwrap_validated())))?,
                    I32X4_NEG => stack.push_value::<T>(Value::V128(i32x4_neg(stack.pop_value().try_into().unwrap_validated())))?,
                    I32X4_ADD => stack.push_value::<T>(Value::V128(i32x4_add(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I32X4_SUB => stack.push_value::<T>(Value::V128(i32x4_sub(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I32X4_MUL => stack.push_value::<T>(Value::V128(i32x4_mul(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I32X4_MIN_S => stack.push_value::<T>(Value::V128(i32x4_min_s(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I32X4_MIN_U => stack.push_value::<T>(Value::V128(i32x4_min_u(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I32X4_MAX_S => stack.push_value::<T>(Value::V128(i32x4_max_s(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I32X4_MAX_U => stack.push_value::<T>(Value::V128(i32x4_max_u(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I64X2_ABS => stack.push_value::<T>(Value::V128(i64x2_abs(stack.pop_value().try_into().unwrap_validated())))?,
                    I64X2_NEG => stack.push_value::<T>(Value::V128(i64x2_neg(stack.pop_value().try_into().unwrap_validated())))?,
                    I64X2_ADD => stack.push_value::<T>(Value::V128(i64x2_add(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I64X2_SUB => stack.push_value::<T>(Value::V128(i64x2_sub(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I64X2_MUL => stack.push_value::<T>(Value::V128(i64x2_mul(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    F32X4_ABS => stack.push_value::<T>(Value::V128(f32x4_abs(stack.pop_value().try_into().unwrap_validated())))?,
                    F32X4_NEG => stack.push_value::<T>(Value::V128(f32x4_neg(stack.pop_value().try_into().unwrap_validated())))?,
                    F32X4_SQRT => stack.push_value::<T>(Value::V128(f32x4_sqrt(stack.pop_value().try_into().unwrap_validated())))?,
                    F32X4_CEIL => stack.push_value::<T>(Value::V128(f32x4_ceil(stack.pop_value().try_into().unwrap_validated())))?,
                    F32X4_FLOOR => stack.push_value::<T>(Value::V128(f32x4_floor(stack.pop_value().try_into().unwrap_validated())))?,
                    F32X4_TRUNC => stack.push_value::<T>(Value::V128(f32x4_trunc(stack.pop_value().try_into().unwrap_validated())))?,
                    F32X4_NEAREST => stack.push_value::<T>(Value::V128(f32x4_nearest(stack.pop_value().try_into().unwrap_validated())))?,
                    F32X4_ADD => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f32x4_add(v1, v2)))?; },
                    F32X4_SUB => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f32x4_sub(v1, v2)))?; },
                    F32X4_MUL => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f32x4_mul(v1, v2)))?; },
                    F32X4_DIV => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f32x4_div(v1, v2)))?; },
                    F32X4_MIN => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f32x4_min(v1, v2)))?; },
                    F32X4_MAX => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f32x4_max(v1, v2)))?; },
                    F32X4_PMIN => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f32x4_pmin(v1, v2)))?; },
                    F32X4_PMAX => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f32x4_pmax(v1, v2)))?; },
                    F64X2_ABS => stack.push_value::<T>(Value::V128(f64x2_abs(stack.pop_value().try_into().unwrap_validated())))?,
                    F64X2_NEG => stack.push_value::<T>(Value::V128(f64x2_neg(stack.pop_value().try_into().unwrap_validated())))?,
                    F64X2_SQRT => stack.push_value::<T>(Value::V128(f64x2_sqrt(stack.pop_value().try_into().unwrap_validated())))?,
                    F64X2_CEIL => stack.push_value::<T>(Value::V128(f64x2_ceil(stack.pop_value().try_into().unwrap_validated())))?,
                    F64X2_FLOOR => stack.push_value::<T>(Value::V128(f64x2_floor(stack.pop_value().try_into().unwrap_validated())))?,
                    F64X2_TRUNC => stack.push_value::<T>(Value::V128(f64x2_trunc(stack.pop_value().try_into().unwrap_validated())))?,
                    F64X2_NEAREST => stack.push_value::<T>(Value::V128(f64x2_nearest(stack.pop_value().try_into().unwrap_validated())))?,
                    F64X2_ADD => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f64x2_add(v1, v2)))?; },
                    F64X2_SUB => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f64x2_sub(v1, v2)))?; },
                    F64X2_MUL => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f64x2_mul(v1, v2)))?; },
                    F64X2_DIV => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f64x2_div(v1, v2)))?; },
                    F64X2_MIN => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f64x2_min(v1, v2)))?; },
                    F64X2_MAX => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f64x2_max(v1, v2)))?; },
                    F64X2_PMIN => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f64x2_pmin(v1, v2)))?; },
                    F64X2_PMAX => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f64x2_pmax(v1, v2)))?; },
                    I8X16_SHL => { let v2: i32 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i8x16_shl(v1, v2 as u32)))?; },
                    I8X16_SHR_S => { let v2: i32 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i8x16_shr_s(v1, v2 as u32)))?; },
                    I8X16_SHR_U => { let v2: i32 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i8x16_shr_u(v1, v2 as u32)))?; },
                    I16X8_SHL => { let v2: i32 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i16x8_shl(v1, v2 as u32)))?; },
                    I16X8_SHR_S => { let v2: i32 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i16x8_shr_s(v1, v2 as u32)))?; },
                    I16X8_SHR_U => { let v2: i32 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i16x8_shr_u(v1, v2 as u32)))?; },
                    I32X4_SHL => { let v2: i32 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i32x4_shl(v1, v2 as u32)))?; },
                    I32X4_SHR_S => { let v2: i32 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i32x4_shr_s(v1, v2 as u32)))?; },
                    I32X4_SHR_U => { let v2: i32 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i32x4_shr_u(v1, v2 as u32)))?; },
                    I64X2_SHL => { let v2: i32 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i64x2_shl(v1, v2 as u32)))?; },
                    I64X2_SHR_S => { let v2: i32 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i64x2_shr_s(v1, v2 as u32)))?; },
                    I64X2_SHR_U => { let v2: i32 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i64x2_shr_u(v1, v2 as u32)))?; },
                    I8X16_NARROW_I16X8_S => stack.push_value::<T>(Value::V128(i8x16_narrow_i16x8_s(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I8X16_NARROW_I16X8_U => stack.push_value::<T>(Value::V128(i8x16_narrow_i16x8_u(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I16X8_NARROW_I32X4_S => stack.push_value::<T>(Value::V128(i16x8_narrow_i32x4_s(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I16X8_NARROW_I32X4_U => stack.push_value::<T>(Value::V128(i16x8_narrow_i32x4_u(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I16X8_EXTEND_LOW_I8X16_S => stack.push_value::<T>(Value::V128(i16x8_extend_low_i8x16_s(stack.pop_value().try_into().unwrap_validated())))?,
                    I16X8_EXTEND_HIGH_I8X16_S => stack.push_value::<T>(Value::V128(i16x8_extend_high_i8x16_s(stack.pop_value().try_into().unwrap_validated())))?,
                    I16X8_EXTEND_LOW_I8X16_U => stack.push_value::<T>(Value::V128(i16x8_extend_low_i8x16_u(stack.pop_value().try_into().unwrap_validated())))?,
                    I16X8_EXTEND_HIGH_I8X16_U => stack.push_value::<T>(Value::V128(i16x8_extend_high_i8x16_u(stack.pop_value().try_into().unwrap_validated())))?,
                    I32X4_EXTEND_LOW_I16X8_S => stack.push_value::<T>(Value::V128(i32x4_extend_low_i16x8_s(stack.pop_value().try_into().unwrap_validated())))?,
                    I32X4_EXTEND_HIGH_I16X8_S => stack.push_value::<T>(Value::V128(i32x4_extend_high_i16x8_s(stack.pop_value().try_into().unwrap_validated())))?,
                    I32X4_EXTEND_LOW_I16X8_U => stack.push_value::<T>(Value::V128(i32x4_extend_low_i16x8_u(stack.pop_value().try_into().unwrap_validated())))?,
                    I32X4_EXTEND_HIGH_I16X8_U => stack.push_value::<T>(Value::V128(i32x4_extend_high_i16x8_u(stack.pop_value().try_into().unwrap_validated())))?,
                    I64X2_EXTEND_LOW_I32X4_S => stack.push_value::<T>(Value::V128(i64x2_extend_low_i32x4_s(stack.pop_value().try_into().unwrap_validated())))?,
                    I64X2_EXTEND_HIGH_I32X4_S => stack.push_value::<T>(Value::V128(i64x2_extend_high_i32x4_s(stack.pop_value().try_into().unwrap_validated())))?,
                    I64X2_EXTEND_LOW_I32X4_U => stack.push_value::<T>(Value::V128(i64x2_extend_low_i32x4_u(stack.pop_value().try_into().unwrap_validated())))?,
                    I64X2_EXTEND_HIGH_I32X4_U => stack.push_value::<T>(Value::V128(i64x2_extend_high_i32x4_u(stack.pop_value().try_into().unwrap_validated())))?,
                    I16X8_EXTMUL_LOW_I8X16_S => stack.push_value::<T>(Value::V128(i16x8_extmul_low_i8x16_s(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I16X8_EXTMUL_HIGH_I8X16_S => stack.push_value::<T>(Value::V128(i16x8_extmul_high_i8x16_s(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I16X8_EXTMUL_LOW_I8X16_U => stack.push_value::<T>(Value::V128(i16x8_extmul_low_i8x16_u(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I16X8_EXTMUL_HIGH_I8X16_U => stack.push_value::<T>(Value::V128(i16x8_extmul_high_i8x16_u(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I32X4_EXTMUL_LOW_I16X8_S => stack.push_value::<T>(Value::V128(i32x4_extmul_low_i16x8_s(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I32X4_EXTMUL_HIGH_I16X8_S => stack.push_value::<T>(Value::V128(i32x4_extmul_high_i16x8_s(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I32X4_EXTMUL_LOW_I16X8_U => stack.push_value::<T>(Value::V128(i32x4_extmul_low_i16x8_u(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I32X4_EXTMUL_HIGH_I16X8_U => stack.push_value::<T>(Value::V128(i32x4_extmul_high_i16x8_u(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I64X2_EXTMUL_LOW_I32X4_S => stack.push_value::<T>(Value::V128(i64x2_extmul_low_i32x4_s(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I64X2_EXTMUL_HIGH_I32X4_S => stack.push_value::<T>(Value::V128(i64x2_extmul_high_i32x4_s(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I64X2_EXTMUL_LOW_I32X4_U => stack.push_value::<T>(Value::V128(i64x2_extmul_low_i32x4_u(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I64X2_EXTMUL_HIGH_I32X4_U => stack.push_value::<T>(Value::V128(i64x2_extmul_high_i32x4_u(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I16X8_EXTADD_PAIRWISE_I8X16_S => stack.push_value::<T>(Value::V128(i16x8_extadd_pairwise_i8x16_s(stack.pop_value().try_into().unwrap_validated())))?,
                    I16X8_EXTADD_PAIRWISE_I8X16_U => stack.push_value::<T>(Value::V128(i16x8_extadd_pairwise_i8x16_u(stack.pop_value().try_into().unwrap_validated())))?,
                    I32X4_EXTADD_PAIRWISE_I16X8_S => stack.push_value::<T>(Value::V128(i32x4_extadd_pairwise_i16x8_s(stack.pop_value().try_into().unwrap_validated())))?,
                    I32X4_EXTADD_PAIRWISE_I16X8_U => stack.push_value::<T>(Value::V128(i32x4_extadd_pairwise_i16x8_u(stack.pop_value().try_into().unwrap_validated())))?,
                    I32X4_DOT_I16X8_S => stack.push_value::<T>(Value::V128(i32x4_dot_i16x8_s(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I16X8_Q15MULRSAT_S => stack.push_value::<T>(Value::V128(i16x8_q15mulrsat_s(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I8X16_SWIZZLE => stack.push_value::<T>(Value::V128(i8x16_swizzle(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I32X4_TRUNC_SAT_F32X4_S => stack.push_value::<T>(Value::V128(i32x4_trunc_sat_f32x4_s(stack.pop_value().try_into().unwrap_validated())))?,
                    I32X4_TRUNC_SAT_F32X4_U => stack.push_value::<T>(Value::V128(i32x4_trunc_sat_f32x4_u(stack.pop_value().try_into().unwrap_validated())))?,
                    F32X4_CONVERT_I32X4_S => stack.push_value::<T>(Value::V128(f32x4_convert_i32x4_s(stack.pop_value().try_into().unwrap_validated())))?,
                    F32X4_CONVERT_I32X4_U => stack.push_value::<T>(Value::V128(f32x4_convert_i32x4_u(stack.pop_value().try_into().unwrap_validated())))?,
                    I32X4_TRUNC_SAT_F64X2_S_ZERO => stack.push_value::<T>(Value::V128(i32x4_trunc_sat_f64x2_s_zero(stack.pop_value().try_into().unwrap_validated())))?,
                    I32X4_TRUNC_SAT_F64X2_U_ZERO => stack.push_value::<T>(Value::V128(i32x4_trunc_sat_f64x2_u_zero(stack.pop_value().try_into().unwrap_validated())))?,
                    F64X2_CONVERT_LOW_I32X4_S => stack.push_value::<T>(Value::V128(f64x2_convert_low_i32x4_s(stack.pop_value().try_into().unwrap_validated())))?,
                    F64X2_CONVERT_LOW_I32X4_U => stack.push_value::<T>(Value::V128(f64x2_convert_low_i32x4_u(stack.pop_value().try_into().unwrap_validated())))?,
                    F32X4_RELAXED_MADD | F64X2_RELAXED_MADD => {
                        let c: [u8; 16] = stack.pop_value().try_into().unwrap_validated();
                        let b: [u8; 16] = stack.pop_value().try_into().unwrap_validated();
                        let a: [u8; 16] = stack.pop_value().try_into().unwrap_validated();
                        if second_instr == F32X4_RELAXED_MADD {
                             stack.push_value::<T>(Value::V128(f32x4_add(f32x4_mul(a, b), c)))?;
                        } else {
                             stack.push_value::<T>(Value::V128(f64x2_add(f64x2_mul(a, b), c)))?;
                        }
                    }
                    F32X4_RELAXED_NMADD | F64X2_RELAXED_NMADD => {
                        let c: [u8; 16] = stack.pop_value().try_into().unwrap_validated();
                        let b: [u8; 16] = stack.pop_value().try_into().unwrap_validated();
                        let a: [u8; 16] = stack.pop_value().try_into().unwrap_validated();
                        if second_instr == F32X4_RELAXED_NMADD {
                             stack.push_value::<T>(Value::V128(f32x4_add(f32x4_neg(f32x4_mul(a, b)), c)))?;
                        } else {
                             stack.push_value::<T>(Value::V128(f64x2_add(f64x2_neg(f64x2_mul(a, b)), c)))?;
                        }
                    }
                    F32X4_RELAXED_MAX => stack.push_value::<T>(Value::V128(f32x4_max(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    F32X4_RELAXED_MIN => stack.push_value::<T>(Value::V128(f32x4_min(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    F64X2_RELAXED_MAX => stack.push_value::<T>(Value::V128(f64x2_max(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    F64X2_RELAXED_MIN => stack.push_value::<T>(Value::V128(f64x2_min(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    I8X16_RELAXED_LANESELECT | I16X8_RELAXED_LANESELECT | I32X4_RELAXED_LANESELECT | I64X2_RELAXED_LANESELECT => {
                        let c = stack.pop_value().try_into().unwrap_validated();
                        let v2 = stack.pop_value().try_into().unwrap_validated();
                        let v1 = stack.pop_value().try_into().unwrap_validated();
                        stack.push_value::<T>(Value::V128(v128_bitselect(v1, v2, c)))?;
                    }
                    I32X4_RELAXED_TRUNC_F32X4_S => stack.push_value::<T>(Value::V128(i32x4_trunc_sat_f32x4_s(stack.pop_value().try_into().unwrap_validated())))?,
                    I32X4_RELAXED_TRUNC_F32X4_U => stack.push_value::<T>(Value::V128(i32x4_trunc_sat_f32x4_u(stack.pop_value().try_into().unwrap_validated())))?,
                    I32X4_RELAXED_TRUNC_F64X2_S_ZERO => stack.push_value::<T>(Value::V128(i32x4_trunc_sat_f64x2_s_zero(stack.pop_value().try_into().unwrap_validated())))?,
                    I32X4_RELAXED_TRUNC_F64X2_U_ZERO => stack.push_value::<T>(Value::V128(i32x4_trunc_sat_f64x2_u_zero(stack.pop_value().try_into().unwrap_validated())))?,
                    I8X16_RELAXED_SWIZZLE => stack.push_value::<T>(Value::V128(i8x16_swizzle(stack.pop_value().try_into().unwrap_validated(), stack.pop_value().try_into().unwrap_validated())))?,
                    _ => {
                        crate::debugln!("WASM Trap: Unimplemented FD_EXTENSION (SIMD) opcode {:#x} at PC {:#x}", second_instr, wasm.pc);
                        return Err(RuntimeError::Trap(TrapError::ReachedUnreachable));
                    }
                }
            }
            */
            FC_EXTENSIONS => {
                let second_instr = match wasm.read_var_u32() {
                    Ok(v) => v,
                    Err(e) => {
                        crate::debugln!("WASM Interpreter error (fetch FC) at PC {:#x}: {:?}", wasm.pc, e);
                        return Err(TrapError::ReachedUnreachable.into());
                    }
                };
                #[cfg(debug_assertions)]
                crate::wasm::core::utils::print_beautiful_fc_extension(second_instr, wasm.pc);
                #[cfg(not(debug_assertions))]
                trace!(
                    "Read instruction byte {second_instr} at wasm_binary[{}]",
                    wasm.pc
                );
                use crate::wasm::core::reader::types::opcode::fc_extensions::*;
                match second_instr {
                    I32_TRUNC_SAT_F32_S => {
                        decrement_fuel!(T::get_fc_extension_flat_cost(I32_TRUNC_SAT_F32_S));
                        let v: value::F32 = stack.pop_value().try_into().unwrap_validated();
                        let res = if v.is_nan() { 0 } else { v.0 as i32 };
                        stack.push_value::<T>(Value::I32(res as u32))?;
                        trace!("Instruction: i32.trunc_sat_f32_s");
                    }
                    I32_TRUNC_SAT_F32_U => {
                        decrement_fuel!(T::get_fc_extension_flat_cost(I32_TRUNC_SAT_F32_U));
                        let v: value::F32 = stack.pop_value().try_into().unwrap_validated();
                        let res = if v.is_nan() { 0 } else { v.0 as u32 };
                        stack.push_value::<T>(Value::I32(res))?;
                        trace!("Instruction: i32.trunc_sat_f32_u");
                    }
                    I32_TRUNC_SAT_F64_S => {
                        decrement_fuel!(T::get_fc_extension_flat_cost(I32_TRUNC_SAT_F64_S));
                        let v: value::F64 = stack.pop_value().try_into().unwrap_validated();
                        let res = if v.is_nan() { 0 } else { v.0 as i32 };
                        stack.push_value::<T>(Value::I32(res as u32))?;
                        trace!("Instruction: i32.trunc_sat_f64_s");
                    }
                    I32_TRUNC_SAT_F64_U => {
                        decrement_fuel!(T::get_fc_extension_flat_cost(I32_TRUNC_SAT_F64_U));
                        let v: value::F64 = stack.pop_value().try_into().unwrap_validated();
                        let res = if v.is_nan() { 0 } else { v.0 as u32 };
                        stack.push_value::<T>(Value::I32(res))?;
                        trace!("Instruction: i32.trunc_sat_f64_u");
                    }
                    I64_TRUNC_SAT_F32_S => {
                        decrement_fuel!(T::get_fc_extension_flat_cost(I64_TRUNC_SAT_F32_S));
                        let v: value::F32 = stack.pop_value().try_into().unwrap_validated();
                        let res = if v.is_nan() { 0 } else { v.0 as i64 };
                        stack.push_value::<T>(Value::I64(res as u64))?;
                        trace!("Instruction: i64.trunc_sat_f32_s");
                    }
                    I64_TRUNC_SAT_F32_U => {
                        decrement_fuel!(T::get_fc_extension_flat_cost(I64_TRUNC_SAT_F32_U));
                        let v: value::F32 = stack.pop_value().try_into().unwrap_validated();
                        let res = if v.is_nan() { 0 } else { v.0 as u64 };
                        stack.push_value::<T>(Value::I64(res))?;
                        trace!("Instruction: i64.trunc_sat_f32_u");
                    }
                    I64_TRUNC_SAT_F64_S => {
                        decrement_fuel!(T::get_fc_extension_flat_cost(I64_TRUNC_SAT_F64_S));
                        let v: value::F64 = stack.pop_value().try_into().unwrap_validated();
                        let res = if v.is_nan() { 0 } else { v.0 as i64 };
                        stack.push_value::<T>(Value::I64(res as u64))?;
                        trace!("Instruction: i64.trunc_sat_f64_s");
                    }
                    I64_TRUNC_SAT_F64_U => {
                        decrement_fuel!(T::get_fc_extension_flat_cost(I64_TRUNC_SAT_F64_U));
                        let v: value::F64 = stack.pop_value().try_into().unwrap_validated();
                        let res = if v.is_nan() { 0 } else { v.0 as u64 };
                        stack.push_value::<T>(Value::I64(res))?;
                        trace!("Instruction: i64.trunc_sat_f64_u");
                    }
                    MEMORY_INIT => {
                        let data_idx = wasm.read_var_u32().unwrap_validated() as DataIdx;
                        wasm.read_u8().unwrap_validated();
                        let n: i32 = stack.pop_value().try_into().unwrap_validated();
                        let s: i32 = stack.pop_value().try_into().unwrap_validated();
                        let d: i32 = stack.pop_value().try_into().unwrap_validated();
                        let cost = T::get_fc_extension_flat_cost(MEMORY_INIT) + (n as u32 * T::get_fc_extension_cost_per_element(MEMORY_INIT));
                        if let Some(fuel) = &mut resumable.maybe_fuel {
                            if *fuel >= cost {
                                *fuel -= cost;
                            } else {
                                stack.push_value::<T>(Value::I32(d as u32)).unwrap_validated();
                                stack.push_value::<T>(Value::I32(s as u32)).unwrap_validated();
                                stack.push_value::<T>(Value::I32(n as u32)).unwrap_validated();
                                resumable.current_func_addr = current_func_addr;
                                resumable.pc = prev_pc;
                                resumable.stp = stp;
                                return Ok(NonZeroU32::new(cost - *fuel));
                            }
                        }
                        memory_init(
                            &store.modules,
                            &mut store.memories,
                            &store.data,
                            current_module,
                            data_idx as usize,
                            0,
                            n as u32,
                            s,
                            d,
                        )?;
                        trace!("Instruction: memory.init");
                    }
                    DATA_DROP => {
                        decrement_fuel!(
                            T::get_fc_extension_flat_cost(DATA_DROP)
                        );
                        let data_idx = wasm.read_var_u32().unwrap_validated() as DataIdx;
                        data_drop(&store.modules, &mut store.data, current_module, data_idx as usize)?;
                        trace!("Instruction: data.drop");
                    }
                    MEMORY_COPY => {
                        wasm.read_u8().unwrap_validated();
                        wasm.read_u8().unwrap_validated();
                        let n: i32 = stack.pop_value().try_into().unwrap_validated();
                        let s: i32 = stack.pop_value().try_into().unwrap_validated();
                        let d: i32 = stack.pop_value().try_into().unwrap_validated();
                        let cost = T::get_fc_extension_flat_cost(MEMORY_COPY) + (n as u32 * T::get_fc_extension_cost_per_element(MEMORY_COPY));
                        if let Some(fuel) = &mut resumable.maybe_fuel {
                            if *fuel >= cost {
                                *fuel -= cost;
                            } else {
                                stack.push_value::<T>(Value::I32(d as u32)).unwrap_validated();
                                stack.push_value::<T>(Value::I32(s as u32)).unwrap_validated();
                                stack.push_value::<T>(Value::I32(n as u32)).unwrap_validated();
                                resumable.current_func_addr = current_func_addr;
                                resumable.pc = prev_pc;
                                resumable.stp = stp;
                                return Ok(NonZeroU32::new(cost - *fuel));
                            }
                        }
                        let mem_addr = *store
                            .modules
                            .get(current_module)
                            .mem_addrs
                            .first()
                            .unwrap_validated();
                        let mem = store.memories.get(mem_addr);
                        mem.mem.copy(
                            d.try_into().unwrap(),
                            &mem.mem,
                            s.try_into().unwrap(),
                            n.try_into().unwrap(),
                        )?;
                        trace!("Instruction: memory.copy");
                    }
                    MEMORY_FILL => {
                        wasm.read_u8().unwrap_validated();
                        let n: i32 = stack.pop_value().try_into().unwrap_validated();
                        let val: i32 = stack.pop_value().try_into().unwrap_validated();
                        let d: i32 = stack.pop_value().try_into().unwrap_validated();
                        let cost = T::get_fc_extension_flat_cost(MEMORY_FILL) + (n as u32 * T::get_fc_extension_cost_per_element(MEMORY_FILL));
                        if let Some(fuel) = &mut resumable.maybe_fuel {
                            if *fuel >= cost {
                                *fuel -= cost;
                            } else {
                                stack.push_value::<T>(Value::I32(d as u32)).unwrap_validated();
                                stack.push_value::<T>(Value::I32(val as u32)).unwrap_validated();
                                stack.push_value::<T>(Value::I32(n as u32)).unwrap_validated();
                                resumable.current_func_addr = current_func_addr;
                                resumable.pc = prev_pc;
                                resumable.stp = stp;
                                return Ok(NonZeroU32::new(cost - *fuel));
                            }
                        }
                        let mem_addr = *store
                            .modules
                            .get(current_module)
                            .mem_addrs
                            .first()
                            .unwrap_validated();
                        let mem = store.memories.get(mem_addr);
                        mem.mem.fill(
                            d.try_into().unwrap(),
                            val as u8,
                            n.try_into().unwrap(),
                        )?;
                        trace!("Instruction: memory.fill");
                    }
                    TABLE_INIT => {
                        let elem_idx = wasm.read_var_u32().unwrap_validated() as ElemIdx;
                        let table_idx = wasm.read_var_u32().unwrap_validated() as TableIdx;
                        let n: i32 = stack.pop_value().try_into().unwrap_validated();
                        let s: i32 = stack.pop_value().try_into().unwrap_validated();
                        let d: i32 = stack.pop_value().try_into().unwrap_validated();
                        let cost = T::get_fc_extension_flat_cost(TABLE_INIT) + (n as u32 * T::get_fc_extension_cost_per_element(TABLE_INIT));
                        if let Some(fuel) = &mut resumable.maybe_fuel {
                            if *fuel >= cost {
                                *fuel -= cost;
                            } else {
                                stack.push_value::<T>(Value::I32(d as u32)).unwrap_validated();
                                stack.push_value::<T>(Value::I32(s as u32)).unwrap_validated();
                                stack.push_value::<T>(Value::I32(n as u32)).unwrap_validated();
                                resumable.current_func_addr = current_func_addr;
                                resumable.pc = prev_pc;
                                resumable.stp = stp;
                                return Ok(NonZeroU32::new(cost - *fuel));
                            }
                        }
                        table_init(
                            &store.modules,
                            &mut store.tables,
                            &store.elements,
                            current_module,
                            elem_idx as usize,
                            table_idx as usize,
                            n as u32,
                            s,
                            d,
                        )?;
                        trace!("Instruction: table.init");
                    }
                    ELEM_DROP => {
                        decrement_fuel!(
                            T::get_fc_extension_flat_cost(ELEM_DROP)
                        );
                        let elem_idx = wasm.read_var_u32().unwrap_validated() as ElemIdx;
                        elem_drop(&store.modules, &mut store.elements, current_module, elem_idx as usize)?;
                        trace!("Instruction: elem.drop");
                    }
                    TABLE_COPY => {
                        let table_x_idx = wasm.read_var_u32().unwrap_validated() as TableIdx;
                        let table_y_idx = wasm.read_var_u32().unwrap_validated() as TableIdx;
                        let n: i32 = stack.pop_value().try_into().unwrap_validated();
                        let s: i32 = stack.pop_value().try_into().unwrap_validated();
                        let d: i32 = stack.pop_value().try_into().unwrap_validated();
                        let cost = T::get_fc_extension_flat_cost(TABLE_COPY) + (n as u32 * T::get_fc_extension_cost_per_element(TABLE_COPY));
                        if let Some(fuel) = &mut resumable.maybe_fuel {
                            if *fuel >= cost {
                                *fuel -= cost;
                            } else {
                                stack.push_value::<T>(Value::I32(d as u32)).unwrap_validated();
                                stack.push_value::<T>(Value::I32(s as u32)).unwrap_validated();
                                stack.push_value::<T>(Value::I32(n as u32)).unwrap_validated();
                                resumable.current_func_addr = current_func_addr;
                                resumable.pc = prev_pc;
                                resumable.stp = stp;
                                return Ok(NonZeroU32::new(cost - *fuel));
                            }
                        }
                        let n = n as usize;
                        let s = s as usize;
                        let d = d as usize;
                        let table_x_addr = *store
                            .modules
                            .get(current_module)
                            .table_addrs
                            .get(table_x_idx)
                            .unwrap_validated();
                        let table_y_addr = *store
                            .modules
                            .get(current_module)
                            .table_addrs
                            .get(table_y_idx)
                            .unwrap_validated();
                        if let Some((tab_x, tab_y)) =
                            store.tables.get_two_mut(table_x_addr, table_y_addr)
                        {
                            if s.checked_add(n).map_or(true, |end| end > tab_y.len())
                                || d.checked_add(n).map_or(true, |end| end > tab_x.len())
                            {
                                return Err(TrapError::TableOrElementAccessOutOfBounds.into());
                            }
                            tab_x.elem[d..d + n].copy_from_slice(&tab_y.elem[s..s + n]);
                        } else {
                            let tab = store.tables.get_mut(table_x_addr);
                            if s.checked_add(n).map_or(true, |end| end > tab.len())
                                || d.checked_add(n).map_or(true, |end| end > tab.len())
                            {
                                return Err(TrapError::TableOrElementAccessOutOfBounds.into());
                            }
                            tab.elem.copy_within(s..s + n, d);
                        }
                        trace!("Instruction: table.copy");
                    }
                    TABLE_GROW => {
                        let table_idx = wasm.read_var_u32().unwrap_validated() as TableIdx;
                        let table_addr = *store
                            .modules
                            .get(current_module)
                            .table_addrs
                            .get(table_idx)
                            .unwrap_validated();
                        let tab = store.tables.get_mut(table_addr);
                        let sz = tab.len() as u32;
                        let n: u32 = stack.pop_value().try_into().unwrap_validated();
                        let val: Ref = stack.pop_value().try_into().unwrap_validated();
                        let cost = T::get_fc_extension_flat_cost(TABLE_GROW) + (n as u32 * T::get_fc_extension_cost_per_element(TABLE_GROW));
                        if let Some(fuel) = &mut resumable.maybe_fuel {
                            if *fuel >= cost {
                                *fuel -= cost;
                            } else {
                                stack.push_value::<T>(Value::Ref(val)).unwrap_validated();
                                stack.push_value::<T>(Value::I32(n)).unwrap_validated();
                                resumable.current_func_addr = current_func_addr;
                                resumable.pc = prev_pc;
                                resumable.stp = stp;
                                return Ok(NonZeroU32::new(cost - *fuel));
                            }
                        }
                        let pushed_value = match tab.grow(n, val) {
                            Ok(_) => sz,
                            Err(_) => u32::MAX,
                        };
                        stack.push_value::<T>(Value::I32(pushed_value))?;
                        trace!("Instruction: table.grow");
                    }
                    TABLE_SIZE => {
                        decrement_fuel!(
                            T::get_fc_extension_flat_cost(TABLE_SIZE)
                        );
                        let table_idx = wasm.read_var_u32().unwrap_validated() as TableIdx;
                        let table_addr = *store
                            .modules
                            .get(current_module)
                            .table_addrs
                            .get(table_idx)
                            .unwrap_validated();
                        let tab = store.tables.get(table_addr);
                        let size = tab.len() as u32;
                        stack.push_value::<T>(Value::I32(size))?;
                        trace!("Instruction: table.size");
                    }
                    TABLE_FILL => {
                        let table_idx = wasm.read_var_u32().unwrap_validated() as TableIdx;
                        let table_addr = *store
                            .modules
                            .get(current_module)
                            .table_addrs
                            .get(table_idx)
                            .unwrap_validated();
                        let tab = store.tables.get_mut(table_addr);
                        let n: i32 = stack.pop_value().try_into().unwrap_validated();
                        let val: Ref = stack.pop_value().try_into().unwrap_validated();
                        let i: i32 = stack.pop_value().try_into().unwrap_validated();
                        let cost = T::get_fc_extension_flat_cost(TABLE_FILL) + (n as u32 * T::get_fc_extension_cost_per_element(TABLE_FILL));
                        if let Some(fuel) = &mut resumable.maybe_fuel {
                            if *fuel >= cost {
                                *fuel -= cost;
                            } else {
                                stack.push_value::<T>(Value::I32(i as u32)).unwrap_validated();
                                stack.push_value::<T>(Value::Ref(val)).unwrap_validated();
                                stack.push_value::<T>(Value::I32(n as u32)).unwrap_validated();
                                resumable.current_func_addr = current_func_addr;
                                resumable.pc = prev_pc;
                                resumable.stp = stp;
                                return Ok(NonZeroU32::new(cost - *fuel));
                            }
                        }
                        let n = n as usize;
                        let i = i as usize;
                        if i.checked_add(n).map_or(true, |end| end > tab.len()) {
                            return Err(TrapError::TableOrElementAccessOutOfBounds.into());
                        }
                        tab.elem[i..i + n].fill(val);
                        trace!("Instruction: table.fill");
                    }
                    _ => {
                        return Err(RuntimeError::Trap(TrapError::ReachedUnreachable));
                    }
                }
            }
            TRY => {
                decrement_fuel!(T::get_flat_cost(TRY));
                BlockType::read(wasm).unwrap_validated();
            }
            CATCH => {
                decrement_fuel!(T::get_flat_cost(CATCH));
                wasm.read_var_u32().unwrap_validated();
                do_sidetable_control_transfer::<T>(wasm, stack, &mut stp, &store.modules.get(current_module).sidetable)?;
            }
            CATCH_ALL => {
                decrement_fuel!(T::get_flat_cost(CATCH_ALL));
                do_sidetable_control_transfer::<T>(wasm, stack, &mut stp, &store.modules.get(current_module).sidetable)?;
            }
            DELEGATE => {
                decrement_fuel!(T::get_flat_cost(DELEGATE));
                wasm.read_var_u32().unwrap_validated();
            }
            THROW => {
                decrement_fuel!(T::get_flat_cost(THROW));
                wasm.read_var_u32().unwrap_validated();
                // TODO: Stack unwinding
                return Err(TrapError::ReachedUnreachable.into());
            }
            RETHROW => {
                decrement_fuel!(T::get_flat_cost(RETHROW));
                wasm.read_var_u32().unwrap_validated();
                // TODO: Stack unwinding
                return Err(TrapError::ReachedUnreachable.into());
            }
            RETURN_CALL => {
                decrement_fuel!(T::get_flat_cost(RETURN_CALL));
                let local_func_idx = wasm.read_var_u32().unwrap_validated() as FuncIdx;
                let func_to_call_addr = store.modules.get(current_module).func_addrs[local_func_idx];
                let func_to_call_ty = store.functions.get(func_to_call_addr).ty();
                let params: Vec<Value> = stack.pop_tail_iter(func_to_call_ty.params.valtypes.len()).collect();

                // Pop current frame and capture return info
                let (ret_func, ret_pc, ret_stp) = stack.pop_call_frame();

                // Push new args
                for param in params {
                    stack.push_value::<T>(param)?;
                }

                // Tail call logic
                match store.functions.get(func_to_call_addr) {
                    FuncInst::HostFunc(host_func) => {
                        let hostcode = host_func.hostcode;
                        let args = stack.pop_tail_iter(func_to_call_ty.params.valtypes.len()).collect();
                        store.caller_module = Some(current_module);
                        let returns = hostcode(store, args).map_err(|HaltExecutionError(code)| RuntimeError::HostFunctionHaltedExecution(code))?;
                        store.caller_module = None;
                        for ret in returns { stack.push_value::<T>(ret)?; }
                        
                        // Restore state to caller
                        current_func_addr = ret_func;
                        if stack.call_frame_count() == 0 {
                            // If no more frames, we are done
                            break;
                        }
                        let FuncInst::WasmFunc(ret_func_inst) = store.functions.get(ret_func) else { unreachable!() };
                        current_module = ret_func_inst.module_addr;
                        wasm.full_wasm_binary = store.modules.get(current_module).wasm_bytecode;
                        wasm.pc = ret_pc;
                        stp = ret_stp;
                        current_function_end_marker = ret_func_inst.code_expr.from() + ret_func_inst.code_expr.len();
                    }
                    FuncInst::WasmFunc(wasm_func) => {
                        stack.push_call_frame::<T>(
                            ret_func, // Correctly point to the original caller
                            &func_to_call_ty,
                            &wasm_func.locals,
                            ret_pc, 
                            ret_stp,
                        )?;
                        current_func_addr = func_to_call_addr;
                        current_module = wasm_func.module_addr;
                        wasm.full_wasm_binary = store.modules.get(current_module).wasm_bytecode;
                        wasm.move_start_to(wasm_func.code_expr).unwrap_validated();
                        stp = wasm_func.stp;
                        current_function_end_marker = wasm_func.code_expr.from() + wasm_func.code_expr.len();
                    }
                    FuncInst::AotFunc(aot_func_inst) => {
                        let code_ptr = aot_func_inst.code.ptr();
                        let params = stack.pop_tail_iter(func_to_call_ty.params.valtypes.len()).collect::<Vec<_>>();
                        
                        let mut raw_params: Vec<u64> = params.iter().map(|v| match v {
                            Value::I32(i) => *i as u64,
                            Value::I64(i) => *i,
                            Value::F32(f) => f.to_bits() as u64,
                            Value::F64(f) => f.to_bits(),
                            Value::Ref(r) => match r {
                                Ref::Null(_) => 0,
                                Ref::Func(addr) => *addr as u64,
                                Ref::Extern(addr) => addr.0 as u64,
                            },
                            Value::V128(_) => 0,
                        }).collect();
                        
                        let func_ptr: extern "C" fn(*mut (), *const u64, *mut u64, u64) = unsafe { core::mem::transmute(code_ptr) };
                        let result_count = func_to_call_ty.returns.valtypes.len();
                        let mut raw_results = vec![0u64; result_count];
                        let mem_base = store.get_wasm_base_ptr() as u64;
                        
                        func_ptr(core::ptr::null_mut(), raw_params.as_ptr(), raw_results.as_mut_ptr(), mem_base);
                        
                        for (i, &raw) in raw_results.iter().enumerate() {
                            let ty = func_to_call_ty.returns.valtypes[i];
                            let val = match ty {
                                ValType::NumType(crate::wasm::NumType::I32) => Value::I32(raw as u32),
                                ValType::NumType(crate::wasm::NumType::I64) => Value::I64(raw),
                                ValType::NumType(crate::wasm::NumType::F32) => Value::F32(crate::wasm::execution::value::F32::from_bits(raw as u32)),
                                ValType::NumType(crate::wasm::NumType::F64) => Value::F64(crate::wasm::execution::value::F64::from_bits(raw)),
                                _ => Value::I64(0),
                            };
                            stack.push_value::<T>(val)?;
                        }

                        // Restore state to caller
                        current_func_addr = ret_func;
                        if stack.call_frame_count() == 0 {
                            break;
                        }
                        let FuncInst::WasmFunc(ret_func_inst) = store.functions.get(ret_func) else { unreachable!() };
                        current_module = ret_func_inst.module_addr;
                        wasm.full_wasm_binary = store.modules.get(current_module).wasm_bytecode;
                        wasm.pc = ret_pc;
                        stp = ret_stp;
                        current_function_end_marker = ret_func_inst.code_expr.from() + ret_func_inst.code_expr.len();
                    }
                }
            }
            RETURN_CALL_INDIRECT => {
                decrement_fuel!(T::get_flat_cost(RETURN_CALL_INDIRECT));
                let type_idx = wasm.read_var_u32().unwrap_validated() as TypeIdx;
                let table_idx = wasm.read_var_u32().unwrap_validated() as TableIdx;
                let i: u32 = stack.pop_value().try_into().unwrap_validated();

                let table_addr = store.modules.get(current_module).table_addrs[table_idx];
                let tab = store.tables.get(table_addr);
                let r = tab.elem.get(i as usize).ok_or(TrapError::TableAccessOutOfBounds)?;

                let func_addr = match r {
                    Ref::Func(a) => *a,
                    _ => return Err(TrapError::IndirectCallNullFuncRef.into()),
                };

                // (Similar tail call logic as RETURN_CALL but with dynamic func_addr)
                let func_to_call_ty = store.functions.get(func_addr).ty();
                let params: Vec<Value> = stack.pop_tail_iter(func_to_call_ty.params.valtypes.len()).collect();
                
                let (ret_func, ret_pc, ret_stp) = stack.pop_call_frame();
                
                for param in params { stack.push_value::<T>(param)?; }

                match store.functions.get(func_addr) {
                    FuncInst::HostFunc(host_func) => {
                        let hostcode = host_func.hostcode;
                        let args = stack.pop_tail_iter(func_to_call_ty.params.valtypes.len()).collect();
                        store.caller_module = Some(current_module);
                        let returns = hostcode(store, args).map_err(|HaltExecutionError(code)| RuntimeError::HostFunctionHaltedExecution(code))?;
                        store.caller_module = None;
                        for ret in returns { stack.push_value::<T>(ret)?; }
                        
                        current_func_addr = ret_func;
                        if stack.call_frame_count() == 0 { break; }
                        let FuncInst::WasmFunc(ret_func_inst) = store.functions.get(ret_func) else { unreachable!() };
                        current_module = ret_func_inst.module_addr;
                        wasm.full_wasm_binary = store.modules.get(current_module).wasm_bytecode;
                        wasm.pc = ret_pc;
                        stp = ret_stp;
                        current_function_end_marker = ret_func_inst.code_expr.from() + ret_func_inst.code_expr.len();
                    }
                    FuncInst::WasmFunc(wasm_func) => {
                        stack.push_call_frame::<T>(ret_func, &func_to_call_ty, &wasm_func.locals, ret_pc, ret_stp)?;
                        current_func_addr = func_addr;
                        current_module = wasm_func.module_addr;
                        wasm.full_wasm_binary = store.modules.get(current_module).wasm_bytecode;
                        wasm.move_start_to(wasm_func.code_expr).unwrap_validated();
                        stp = wasm_func.stp;
                        current_function_end_marker = wasm_func.code_expr.from() + wasm_func.code_expr.len();
                    }
                    FuncInst::AotFunc(aot_func_inst) => {
                        let code_ptr = aot_func_inst.code.ptr();
                        let params = stack.pop_tail_iter(func_to_call_ty.params.valtypes.len()).collect::<Vec<_>>();
                        
                        let mut raw_params: Vec<u64> = params.iter().map(|v| match v {
                            Value::I32(i) => *i as u64,
                            Value::I64(i) => *i,
                            Value::F32(f) => f.to_bits() as u64,
                            Value::F64(f) => f.to_bits(),
                            Value::Ref(r) => match r {
                                Ref::Null(_) => 0,
                                Ref::Func(addr) => *addr as u64,
                                Ref::Extern(addr) => addr.0 as u64,
                            },
                            Value::V128(_) => 0,
                        }).collect();
                        
                        let func_ptr: extern "C" fn(*mut (), *const u64, *mut u64, u64) = unsafe { core::mem::transmute(code_ptr) };
                        let result_count = func_to_call_ty.returns.valtypes.len();
                        let mut raw_results = vec![0u64; result_count];
                        let mem_base = store.get_wasm_base_ptr() as u64;
                        
                        func_ptr(core::ptr::null_mut(), raw_params.as_ptr(), raw_results.as_mut_ptr(), mem_base);
                        
                        for (i, &raw) in raw_results.iter().enumerate() {
                            let ty = func_to_call_ty.returns.valtypes[i];
                            let val = match ty {
                                ValType::NumType(crate::wasm::NumType::I32) => Value::I32(raw as u32),
                                ValType::NumType(crate::wasm::NumType::I64) => Value::I64(raw),
                                ValType::NumType(crate::wasm::NumType::F32) => Value::F32(crate::wasm::execution::value::F32::from_bits(raw as u32)),
                                ValType::NumType(crate::wasm::NumType::F64) => Value::F64(crate::wasm::execution::value::F64::from_bits(raw)),
                                _ => Value::I64(0),
                            };
                            stack.push_value::<T>(val)?;
                        }

                        current_func_addr = ret_func;
                        if stack.call_frame_count() == 0 { break; }
                        let FuncInst::WasmFunc(ret_func_inst) = store.functions.get(ret_func) else { unreachable!() };
                        current_module = ret_func_inst.module_addr;
                        wasm.full_wasm_binary = store.modules.get(current_module).wasm_bytecode;
                        wasm.pc = ret_pc;
                        stp = ret_stp;
                        current_function_end_marker = ret_func_inst.code_expr.from() + ret_func_inst.code_expr.len();
                    }
                }
            }
            RETURN_CALL_REF => {
                decrement_fuel!(T::get_flat_cost(RETURN_CALL_REF));
                // TODO: Implement
                return Err(TrapError::ReachedUnreachable.into());
            }
            CALL_REF => {
                decrement_fuel!(T::get_flat_cost(CALL_REF));
                // TODO: Implement Typed Refs
                wasm.read_var_u32().unwrap_validated(); // type idx
                stack.pop_value(); // ref
                return Err(TrapError::ReachedUnreachable.into());
            }
            ATOMIC_PREFIX => {
                let sub = wasm.read_var_u32().unwrap_validated();
                match sub {
                    0x00 => { // notify
                        MemArg::read(wasm).unwrap_validated();
                        stack.pop_value();
                        stack.pop_value();
                        stack.push_value::<T>(Value::I32(0))?;
                    }
                    0x01 => { // wait32
                        MemArg::read(wasm).unwrap_validated();
                        stack.pop_value();
                        stack.pop_value();
                        stack.pop_value();
                        stack.push_value::<T>(Value::I32(0))?;
                    }
                    0x02 => { // wait64
                        MemArg::read(wasm).unwrap_validated();
                        stack.pop_value();
                        stack.pop_value();
                        stack.pop_value();
                        stack.push_value::<T>(Value::I32(0))?;
                    }
                    0x10 | 0x12 | 0x13 | 0x14 | 0x15 | 0x16 => { // load (various)
                        let memarg = MemArg::read(wasm).unwrap_validated();
                        let addr: u32 = stack.pop_value().try_into().unwrap_validated();
                        // Mapping to standard load for single-threaded interpreter
                        let mem_addr = store.modules.get(current_module).mem_addrs[0];
                        let mem = store.memories.get(mem_addr);
                        let idx = calculate_mem_address(&memarg, addr)?;
                        // Simple load (ignoring size/sign details for brevity in this block, ideally strictly match)
                        // Implementing i32.atomic.load (0x10)
                        if sub == 0x10 {
                            let val: i32 = mem.mem.load(idx)?;
                            stack.push_value::<T>(Value::I32(val as u32))?;
                        } else {
                            // Fallback for others
                            let val: i64 = mem.mem.load(idx)?;
                            stack.push_value::<T>(Value::I64(val as u64))?;
                        }
                    }
                    0x17 | 0x18 | 0x19 | 0x1A | 0x1B | 0x1C | 0x1D => { // store
                        let memarg = MemArg::read(wasm).unwrap_validated();
                        let val = stack.pop_value();
                        let addr: u32 = stack.pop_value().try_into().unwrap_validated();
                        let mem_addr = store.modules.get(current_module).mem_addrs[0];
                        let mem = store.memories.get(mem_addr);
                        let idx = calculate_mem_address(&memarg, addr)?;
                        if let Value::I32(v) = val {
                            mem.mem.store(idx, v)?;
                        } else if let Value::I64(v) = val {
                            mem.mem.store(idx, v)?;
                        }
                    }
                    // RMWs (add, sub, etc)
                    0x1E..=0x4F => {
                        let memarg = MemArg::read(wasm).unwrap_validated();
                        let val = stack.pop_value();
                        let addr: u32 = stack.pop_value().try_into().unwrap_validated();
                        // Placeholder for RMW - just load and store (not atomic in this stub)
                        let mem_addr = store.modules.get(current_module).mem_addrs[0];
                        let mem = store.memories.get(mem_addr);
                        let idx = calculate_mem_address(&memarg, addr)?;
                        // Return old value
                        let old: i32 = mem.mem.load(idx)?;
                        stack.push_value::<T>(Value::I32(old as u32))?;
                    }
                    _ => return Err(TrapError::ReachedUnreachable.into()),
                }
            }
            GC_PREFIX | GC_PREFIX_ALT => {
                let sub = wasm.read_var_u32().unwrap_validated();
                // GC Instructions stub
                match sub {
                    0x00 => { // struct.new
                        wasm.read_var_u32().unwrap_validated(); // type index
                        // Pop args, push struct ref
                        // unimplemented!
                        return Err(TrapError::ReachedUnreachable.into());
                    }
                    _ => return Err(TrapError::ReachedUnreachable.into()),
                }
            }
            0x00..=0x0A
            | 0x0C..=0x22
            | 0x24..=0x40
            | 0x45..=0xBF
            | 0xC0..=0xCF
            | 0xD1
            | 0xD3..=0xFB
            | 0xFD => {
                unreachable_validated!();
            }
        }
    }
    Ok(None)
}
fn do_sidetable_control_transfer<T: Config>(
    wasm: &mut WasmReader,
    stack: &mut Stack,
    stp: &mut usize,
    sidetable: &Sidetable,
) -> Result<(), RuntimeError> {
    let sidetable_entry = &sidetable[*stp];
    for _ in 0..sidetable_entry.popcnt {
        stack.pop_value();
    }
    if sidetable_entry.valcnt > 0 {
        let values_to_copy: crate::rust_alloc::vec::Vec<Value> =
            stack.pop_tail_iter(sidetable_entry.valcnt).collect();
        for val in values_to_copy {
            stack.push_value::<T>(val)?;
        }
    }
    wasm.pc = ((wasm.pc as isize) + sidetable_entry.delta_pc) as usize;
    *stp = ((*stp as isize) + sidetable_entry.delta_stp) as usize;
    Ok(())
}
fn calculate_mem_address(memarg: &MemArg, relative_address: u32) -> Result<usize, TrapError> {
    (memarg.offset as u64 + relative_address as u64)
        .try_into()
        .map_err(|_| TrapError::MemoryOrDataAccessOutOfBounds)
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn memory_init<'b>(
    modules: &AddrVec<ModuleAddr, ModuleInst<'b>>,
    memories: &mut AddrVec<MemAddr, crate::wasm::execution::store::instances::MemInst>,
    data: &AddrVec<DataAddr, crate::wasm::execution::store::instances::DataInst>,
    module_addr: ModuleAddr,
    data_idx: usize,
    mem_idx: usize,
    n: u32,
    s: i32,
    d: i32,
) -> Result<(), RuntimeError> {
    let module_inst = modules.get(module_addr);
    let mem_addr = module_inst.mem_addrs[mem_idx];
    let data_addr = module_inst.data_addrs[data_idx];
    let mem_inst = memories.get(mem_addr);
    let data_inst = data.get(data_addr);
    mem_inst.mem.init(
        d.try_into().unwrap(),
        &data_inst.data,
        s.try_into().unwrap(),
        n.try_into().unwrap(),
    )
}
pub(crate) fn data_drop<'b>(
    modules: &AddrVec<ModuleAddr, ModuleInst<'b>>,
    data: &mut AddrVec<DataAddr, crate::wasm::execution::store::instances::DataInst>,
    module_addr: ModuleAddr,
    data_idx: usize,
) -> Result<(), RuntimeError> {
    let module_inst = modules.get(module_addr);
    let data_addr = module_inst.data_addrs[data_idx];
    let data_inst = data.get_mut(data_addr);
    data_inst.data.clear();
    data_inst.data.shrink_to_fit();
    Ok(())
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn table_init<'b>(
    modules: &AddrVec<ModuleAddr, ModuleInst<'b>>,
    tables: &mut AddrVec<TableAddr, crate::wasm::execution::store::instances::TableInst>,
    elements: &AddrVec<ElemAddr, crate::wasm::execution::store::instances::ElemInst>,
    module_addr: ModuleAddr,
    elem_idx: usize,
    table_idx: usize,
    n: u32,
    s: i32,
    d: i32,
) -> Result<(), RuntimeError> {
    let module_inst = modules.get(module_addr);
    let table_addr = module_inst.table_addrs[table_idx];
    let elem_addr = module_inst.elem_addrs[elem_idx];
    let elem_inst = elements.get(elem_addr);
    let table_inst = tables.get_mut(table_addr);
    let n = n as usize;
    let s = s as usize;
    let d = d as usize;
    if s.checked_add(n).map_or(true, |end| end > elem_inst.len())
        || d.checked_add(n).map_or(true, |end| end > table_inst.len())
    {
        return Err(TrapError::TableOrElementAccessOutOfBounds.into());
    }
    table_inst.elem[d..d + n].copy_from_slice(&elem_inst.references[s..s + n]);
    Ok(())
}
pub(crate) fn elem_drop<'b>(
    modules: &AddrVec<ModuleAddr, ModuleInst<'b>>,
    elements: &mut AddrVec<ElemAddr, crate::wasm::execution::store::instances::ElemInst>,
    module_addr: ModuleAddr,
    elem_idx: usize,
) -> Result<(), RuntimeError> {
    let module_inst = modules.get(module_addr);
    let elem_addr = module_inst.elem_addrs[elem_idx];
    let elem_inst = elements.get_mut(elem_addr);
    elem_inst.references.clear();
    elem_inst.references.shrink_to_fit();
    Ok(())
}
#[inline(always)]
fn to_lanes<const M: usize, const N: usize, T: LittleEndianBytes<M>>(data: [u8; 16]) -> [T; N] {
    assert_eq!(M * N, 16);
    let mut lanes = data
        .chunks(M)
        .map(|chunk| T::from_le_bytes(chunk.try_into().unwrap()));
    array::from_fn(|_| lanes.next().unwrap())
}
#[inline(always)]
fn from_lanes<const M: usize, const N: usize, T: LittleEndianBytes<M>>(lanes: [T; N]) -> [u8; 16] {
    assert_eq!(M * N, 16);
    let mut bytes = lanes.into_iter().flat_map(T::to_le_bytes);
    array::from_fn(|_| bytes.next().unwrap())
}