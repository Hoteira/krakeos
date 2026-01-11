//! This module solely contains the actual interpretation loop that matches instructions, interpreting the WASM bytecode
//!
//!
//! # Note to Developer:
//!
//! 1. There must be only imports and one `impl` with one function (`run`) in it.
//! 2. This module must only use [`RuntimeError`] and never [`Error`](crate::core::error::ValidationError).

use core::{
    num::NonZeroU32,
    {
        array,
        iter::zip,
        ops::Neg,
    },
};

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

use crate::unreachable_validated;

use crate::wasm::execution::config::Config;

use super::{little_endian::LittleEndianBytes, store::Store};

/// Interprets wasm native functions. Wasm parameters and Wasm return values are passed on the stack.
/// Returns `Ok(None)` in case execution successfully terminates, `Ok(Some(required_fuel))` if execution
/// terminates due to insufficient fuel, indicating how much fuel is required to resume with `required_fuel`,
/// and `[Error::RuntimeError]` otherwise.
pub(super) fn run<T: Config>(
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

    // Start reading the function's instructions
    let wasm = &mut WasmReader::new(store.modules.get(current_module).wasm_bytecode);

    // local variable for holding where the function code ends (last END instr address + 1) to avoid lookup at every END instr
    let mut current_function_end_marker =
        wasm_func_inst.code_expr.from() + wasm_func_inst.code_expr.len();

    wasm.pc = pc;

    use crate::wasm::core::reader::types::opcode::*;
    loop {
        // call the instruction hook
        store
            .user_data
            .instruction_hook(store.modules.get(current_module).wasm_bytecode, wasm.pc);

        // convenience macro for fuel metering. records the interpreter state within resumable and returns with
        // Ok(required_fuel) if the fuel to execute the instruction is not enough
        let prev_pc = wasm.pc;
        macro_rules! decrement_fuel {
            ($cost:expr) => {
                if let Some(fuel) = &mut resumable.maybe_fuel {
                    if *fuel >= $cost {
                        *fuel -= $cost;
                    } else {
                        resumable.current_func_addr = current_func_addr;
                        resumable.pc = prev_pc; // the instruction was fetched already, we roll this back
                        resumable.stp = stp;
                        return Ok(NonZeroU32::new($cost-*fuel));
                    }
                }
            }
        }

        let first_instr_byte = wasm.read_u8().unwrap_validated();

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
                // There might be multiple ENDs in a single function. We want to
                // exit only when the outermost block (aka function block) ends.
                if wasm.pc != current_function_end_marker {
                    continue;
                }

                let (maybe_return_func_addr, maybe_return_address, maybe_return_stp) =
                    stack.pop_call_frame();

                // We finished this entire invocation if there is no call frame left. If there are
                // one or more call frames, we need to continue from where the callee was called
                // from.
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

                // TODO is this correct?
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
                //skip n of BR n
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
                //same as BR, except no need to skip n of BR n
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

                        let returns = hostcode(store, params);

                        let returns = returns.map_err(|HaltExecutionError| {
                            RuntimeError::HostFunctionHaltedExecution
                        })?;

                        // Verify that the return parameters match the host function parameters
                        // since we have no validation guarantees for host functions
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
                }
                trace!("Instruction: CALL");
            }

            // TODO: fix push_call_frame, because the func idx that you get from the table is global func idx
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
                    .ok_or(TrapError::TableAccessOutOfBounds)
                    .and_then(|r| {
                        if matches!(r, Ref::Null(_)) {
                            trace!("table_idx ({table_idx}) --- element index in table ({i})");
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
                        let returns = hostcode(store, params);

                        let returns = returns.map_err(|HaltExecutionError| {
                            RuntimeError::HostFunctionHaltedExecution
                        })?;

                        // Verify that the return parameters match the host function parameters
                        // since we have no validation guarantees for host functions
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
                    .ok_or(TrapError::TableOrElementAccessOutOfBounds)
                    .map(|r| *r = val)?;
                trace!(
                    "Instruction: table.set '{}' [{} {}] -> []",
                    table_idx,
                    i,
                    val
                );
            }
            UNREACHABLE => {
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
                let data = mem_inst.mem.load(idx)?;

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
                mem.mem.store(idx, data_to_store)?;

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
                // decrement fuel, but push n back if it fails
                let cost = T::get_flat_cost(MEMORY_GROW) + n * T::get_cost_per_element(MEMORY_GROW);
                if let Some(fuel) = &mut resumable.maybe_fuel {
                    if *fuel >= cost {
                        *fuel -= cost;
                    } else {
                        stack.push_value::<T>(Value::I32(n)).unwrap_validated(); // we are pushing back what was just popped, this can't panic.
                        resumable.current_func_addr = current_func_addr;
                        resumable.pc = wasm.pc - prev_pc; // the instruction was fetched already, we roll this back
                        resumable.stp = stp;
                        return Ok(NonZeroU32::new(cost - *fuel));
                    }
                }

                // TODO this instruction is non-deterministic w.r.t. spec, and can fail if the embedder wills it.
                // for now we execute it always according to the following match expr.
                // if the grow operation fails, err := Value::I32(2^32-1) is pushed to the stack per spec
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
                    return Err(TrapError::DivideBy0.into());
                }
                if divisor == i32::MIN && dividend == -1 {
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
            // https://webassembly.github.io/spec/core/exec/instructions.html#xref-syntax-instructions-syntax-instr-ref-mathsf-ref-func-x
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
                // Should we call instruction hook here as well? Multibyte instruction
                let second_instr = wasm.read_var_u32().unwrap_validated();

                #[cfg(debug_assertions)]
                crate::wasm::core::utils::print_beautiful_fd_extension(second_instr, wasm.pc);

                #[cfg(not(debug_assertions))]
                trace!(
                    "Read instruction byte {second_instr} at wasm_binary[{}]",
                    wasm.pc
                );

                use crate::wasm::core::reader::types::opcode::fd_extensions::*;

                match second_instr {
                    V128_CONST => {
                        let mut data = [0; 16];
                        for byte_ref in &mut data {
                            *byte_ref = wasm.read_u8().unwrap_validated();
                        }

                        stack.push_value::<T>(Value::V128(data))?;
                    }
                    // unimplemented instructions
                    0..=11 | 13.. => unreachable_validated!(),
                }
            }

            FC_EXTENSIONS => {
                let second_instr = wasm.read_var_u32().unwrap_validated();

                #[cfg(debug_assertions)]
                crate::wasm::core::utils::print_beautiful_fc_extension(second_instr, wasm.pc);

                #[cfg(not(debug_assertions))]
                trace!(
                    "Read instruction byte {second_instr} at wasm_binary[{}]",
                    wasm.pc
                );

                use crate::wasm::core::reader::types::opcode::fc_extensions::*;

                match second_instr {
                    MEMORY_INIT => {
                        decrement_fuel!(
                            T::get_fc_extension_flat_cost(MEMORY_INIT)
                        );
                        let data_idx = wasm.read_var_u32().unwrap_validated() as DataIdx;
                        // verify 0x00
                        wasm.read_u8().unwrap_validated();

                        let n: i32 = stack.pop_value().try_into().unwrap_validated();
                        let s: i32 = stack.pop_value().try_into().unwrap_validated();
                        let d: i32 = stack.pop_value().try_into().unwrap_validated();

                        decrement_fuel!(n as u32 * T::get_fc_extension_cost_per_element(MEMORY_INIT));

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
                        decrement_fuel!(
                            T::get_fc_extension_flat_cost(MEMORY_COPY)
                        );
                        // verify 0x00 0x00
                        wasm.read_u8().unwrap_validated();
                        wasm.read_u8().unwrap_validated();

                        let n: i32 = stack.pop_value().try_into().unwrap_validated();
                        let s: i32 = stack.pop_value().try_into().unwrap_validated();
                        let d: i32 = stack.pop_value().try_into().unwrap_validated();

                        decrement_fuel!(n as u32 * T::get_fc_extension_cost_per_element(MEMORY_COPY));

                        let mem_addr = *store
                            .modules
                            .get(current_module)
                            .mem_addrs
                            .first()
                            .unwrap_validated();

                        // we need to get two mutable references to the same memory instance
                        // creating a copy of the memory instance is not feasible as it is too large
                        // instead, we use `get_mut` twice with the same index, which is unsafe but
                        // we know that `copy` handles overlapping regions correctly
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
                        decrement_fuel!(
                            T::get_fc_extension_flat_cost(MEMORY_FILL)
                        );
                        // verify 0x00
                        wasm.read_u8().unwrap_validated();

                        let n: i32 = stack.pop_value().try_into().unwrap_validated();
                        let val: i32 = stack.pop_value().try_into().unwrap_validated();
                        let d: i32 = stack.pop_value().try_into().unwrap_validated();

                        decrement_fuel!(n as u32 * T::get_fc_extension_cost_per_element(MEMORY_FILL));

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
                        decrement_fuel!(
                            T::get_fc_extension_flat_cost(TABLE_INIT)
                        );
                        let elem_idx = wasm.read_var_u32().unwrap_validated() as ElemIdx;
                        let table_idx = wasm.read_var_u32().unwrap_validated() as TableIdx;

                        let n: i32 = stack.pop_value().try_into().unwrap_validated();
                        let s: i32 = stack.pop_value().try_into().unwrap_validated();
                        let d: i32 = stack.pop_value().try_into().unwrap_validated();

                        decrement_fuel!(n as u32 * T::get_fc_extension_cost_per_element(TABLE_INIT));

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
                        decrement_fuel!(
                            T::get_fc_extension_flat_cost(TABLE_COPY)
                        );
                        let table_x_idx = wasm.read_var_u32().unwrap_validated() as TableIdx;
                        let table_y_idx = wasm.read_var_u32().unwrap_validated() as TableIdx;

                        let n: i32 = stack.pop_value().try_into().unwrap_validated();
                        let s: i32 = stack.pop_value().try_into().unwrap_validated();
                        let d: i32 = stack.pop_value().try_into().unwrap_validated();

                        decrement_fuel!(n as u32 * T::get_fc_extension_cost_per_element(TABLE_COPY));

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
                            // copying within the same table
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
                        decrement_fuel!(
                            T::get_fc_extension_flat_cost(TABLE_GROW)
                        );
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

                        decrement_fuel!(n as u32 * T::get_fc_extension_cost_per_element(TABLE_GROW));

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
                        decrement_fuel!(
                            T::get_fc_extension_flat_cost(TABLE_FILL)
                        );
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

                        decrement_fuel!(n as u32 * T::get_fc_extension_cost_per_element(TABLE_FILL));

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

            0x00..=0x0A
            | 0x0C..=0x22
            | 0x24..=0x40
            | 0x45..=0xBF
            | 0xC0..=0xCF
            | 0xD1
            | 0xD3..=0xFC
            | 0xFE..=0xFF => {
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

    // pop operands
    for _ in 0..sidetable_entry.popcnt {
        stack.pop_value();
    }

    // copy branch args
    if sidetable_entry.valcnt > 0 {
        let values_to_copy: crate::rust_alloc::vec::Vec<Value> =
            stack.pop_tail_iter(sidetable_entry.valcnt).collect();
        for val in values_to_copy {
            stack.push_value::<T>(val)?;
        }
    }

    // update pointers
    // TODO checked add?
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

    // replace with empty vector to drop data
    // TODO optimization: use Option<Vec<u8>> instead of Vec<u8> to avoid allocation
    data_inst.data = crate::rust_alloc::vec::Vec::new();
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

    // replace with empty vector to drop elements
    // TODO optimization: use Option<Vec<Ref>> instead of Vec<Ref> to avoid allocation
    elem_inst.references = crate::rust_alloc::vec::Vec::new();
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