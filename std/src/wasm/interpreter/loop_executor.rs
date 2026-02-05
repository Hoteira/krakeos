use crate::wasm::common::little_endian::LittleEndianBytes;
use crate::wasm::interpreter::store::Store;
use crate::rust_alloc::{vec, vec::Vec};
use crate::wasm::common::assert_validated::{UnreachableValidatedExt, UnwrapValidatedExt};
use crate::wasm::common::config::Config;
use crate::wasm::common::reader::types::RefType;
use crate::wasm::common::runtime_error::{RuntimeError, TrapError};
use crate::wasm::common::reader::types::ValType;
use crate::wasm::common::value::{Value, F32, F64, Ref};
use crate::wasm::common::indices::{DataIdx, ElemIdx, GlobalIdx};
use crate::wasm::common::indices::{FuncIdx, LabelIdx, LocalIdx, TableIdx, TypeIdx};
use crate::wasm::common::reader::{WasmReadable, WasmReader};
use crate::wasm::common::reader::types::{BlockType, memarg::MemArg};
use crate::wasm::common::reader::span::Span;
use crate::wasm::common::sidetable::Sidetable;
use crate::wasm::interpreter::resumable::Resumable;
use crate::wasm::interpreter::store::HaltExecutionError;
use crate::wasm::interpreter::store::addrs::{AddrVec, DataAddr, ElemAddr, MemAddr, ModuleAddr, TableAddr};
use crate::wasm::interpreter::store::instances::{FuncInst, ModuleInst};
use crate::wasm::common::value_stack::Stack;
use core::{
    num::NonZeroU32,
    {array, iter::zip, ops::Neg},
};

pub fn run_const<'a, T: Config>(
    wasm: &mut WasmReader,
    stack: &mut Stack,
    module: ModuleAddr,
    store: &Store<'a, T>,
) -> Result<(), RuntimeError> {
    use crate::wasm::common::reader::types::opcode::*;
    loop {
        let first_instr_byte = wasm.read_u8().unwrap_validated();
        match first_instr_byte {
            END => {
                break;
            }
            GLOBAL_GET => {
                let global_idx = wasm.read_var_u32().unwrap_validated() as GlobalIdx;
                let global_addr = *store.modules.get(module).global_addrs.get(global_idx).unwrap_validated();
                let global = store.globals.get(global_addr);
                stack.push_value::<T>(global.value)?;
            }
            I32_CONST => {
                let constant = wasm.read_var_i32().unwrap_validated();
                stack.push_value::<T>(constant.into())?;
            }
            F32_CONST => {
                let constant = F32::from_bits(wasm.read_f32().unwrap_validated());
                stack.push_value::<T>(constant.into())?;
            }
            F64_CONST => {
                let constant = F64::from_bits(wasm.read_f64().unwrap_validated());
                stack.push_value::<T>(constant.into())?;
            }
            I64_CONST => {
                let constant = wasm.read_var_i64().unwrap_validated();
                stack.push_value::<T>(constant.into())?;
            }
            REF_NULL => {
                let reftype = RefType::read(wasm).unwrap_validated();
                stack.push_value::<T>(Value::Ref(Ref::Null(reftype)))?;
            }
            REF_FUNC => {
                let func_idx = wasm.read_var_u32().unwrap_validated() as usize;
                let func_addr = *store
                    .modules
                    .get(module)
                    .func_addrs
                    .get(func_idx)
                    .unwrap_validated();
                stack.push_value::<T>(Value::Ref(Ref::Func(func_addr)))?;
            }
            FD_EXTENSIONS => {
                use crate::wasm::common::reader::types::opcode::fd_extensions::*;
                match wasm.read_var_u32().unwrap_validated() {
                    V128_CONST => {
                        let mut data = [0; 16];
                        for byte_ref in &mut data {
                            *byte_ref = wasm.read_u8().unwrap_validated();
                        }
                        stack.push_value::<T>(Value::V128(data))?;
                    }
                    _ => crate::unreachable_validated!(),
                }
            }
            _ => {
                crate::unreachable_validated!();
            }
        }
    }
    Ok(())
}

pub fn run_const_span<'a, T: Config>(
    wasm_bytecode: &'a [u8],
    span: &Span,
    module_addr: ModuleAddr,
    store: &mut Store<'a, T>,
) -> Result<Option<Value>, RuntimeError> {
    let mut reader = WasmReader::new(wasm_bytecode);
    reader.move_start_to(*span).unwrap_validated();
    let mut stack = Stack::new();
    run_const(&mut reader, &mut stack, module_addr, store)?;
    Ok(stack.peek_value())
}

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
    use crate::wasm::common::reader::types::opcode::*;
    loop {
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
                        return Ok(NonZeroU32::new($cost - *fuel));
                    }
                }
            };
        }
        let first_instr_byte = match wasm.read_u8() {
            Ok(b) => b,
            Err(e) => {
                return Err(TrapError::ReachedUnreachable.into());
            }
        };
        match first_instr_byte {
            NOP => {
                decrement_fuel!(T::get_flat_cost(NOP));
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
                current_func_addr = maybe_return_func_addr;
                let FuncInst::WasmFunc(current_wasm_func_inst) =
                    store.functions.get(current_func_addr)
                else {
                    unreachable!(
                        "function addresses on the stack always correspond to native wasm functions"
                    )
                };
                current_module = current_wasm_func_inst.module_addr;
                wasm.full_wasm_binary = store.modules.get(current_module).wasm_bytecode;
                wasm.pc = maybe_return_address;
                stp = maybe_return_stp;
                current_function_end_marker = current_wasm_func_inst.code_expr.from()
                    + current_wasm_func_inst.code_expr.len();
            }
            IF => {
                decrement_fuel!(T::get_flat_cost(IF));
                wasm.read_var_u32().unwrap_validated();
                let test_val: i32 = stack.pop_value().try_into().unwrap_validated();
                if test_val != 0 {
                    stp += 1;
                } else {
                    do_sidetable_control_transfer::<T>(
                        wasm,
                        stack,
                        &mut stp,
                        &store.modules.get(current_module).sidetable,
                    )?;
                }
            }
            ELSE => {
                decrement_fuel!(T::get_flat_cost(ELSE));
                do_sidetable_control_transfer::<T>(
                    wasm,
                    stack,
                    &mut stp,
                    &store.modules.get(current_module).sidetable,
                )?;
            }
            BR_IF => {
                decrement_fuel!(T::get_flat_cost(BR_IF));
                wasm.read_var_u32().unwrap_validated();
                let test_val: i32 = stack.pop_value().try_into().unwrap_validated();
                if test_val != 0 {
                    do_sidetable_control_transfer::<T>(
                        wasm,
                        stack,
                        &mut stp,
                        &store.modules.get(current_module).sidetable,
                    )?;
                } else {
                    stp += 1;
                }
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
                do_sidetable_control_transfer::<T>(
                    wasm,
                    stack,
                    &mut stp,
                    &store.modules.get(current_module).sidetable,
                )?;
            }
            BR => {
                decrement_fuel!(T::get_flat_cost(BR));
                wasm.read_var_u32().unwrap_validated();
                do_sidetable_control_transfer::<T>(
                    wasm,
                    stack,
                    &mut stp,
                    &store.modules.get(current_module).sidetable,
                )?;
            }
            BLOCK | LOOP => {
                decrement_fuel!(T::get_flat_cost(first_instr_byte));
                BlockType::read(wasm).unwrap_validated();
            }
            RETURN => {
                decrement_fuel!(T::get_flat_cost(RETURN));
                do_sidetable_control_transfer::<T>(
                    wasm,
                    stack,
                    &mut stp,
                    &store.modules.get(current_module).sidetable,
                )?;
                
                // Immediately perform function exit logic
                let (maybe_return_func_addr, maybe_return_address, maybe_return_stp) =
                    stack.pop_call_frame();
                if stack.call_frame_count() == 0 {
                    break;
                }
                current_func_addr = maybe_return_func_addr;
                let FuncInst::WasmFunc(current_wasm_func_inst) =
                    store.functions.get(current_func_addr)
                else {
                    unreachable!(
                        "function addresses on the stack always correspond to native wasm functions"
                    )
                };
                current_module = current_wasm_func_inst.module_addr;
                wasm.full_wasm_binary = store.modules.get(current_module).wasm_bytecode;
                wasm.pc = maybe_return_address;
                stp = maybe_return_stp;
                current_function_end_marker = current_wasm_func_inst.code_expr.from()
                    + current_wasm_func_inst.code_expr.len();
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
                    store
                        .modules
                        .get(current_wasm_func_inst.module_addr)
                        .func_addrs[local_func_idx]
                };
                let func_to_call_ty = store.functions.get(func_to_call_addr).ty();
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
                }
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
                        TrapError::TableAccessOutOfBounds
                    })
                    .and_then(|r| {
                        if matches!(r, Ref::Null(_)) {
                            Err(TrapError::UninitializedElement)
                        } else {
                            Ok(r)
                        }
                    })?;
                let func_to_call_addr = match *r {
                    Ref::Func(func_addr) => func_addr,
                    Ref::Null(_) => return Err(TrapError::IndirectCallNullFuncRef.into()),
                    Ref::Extern(_) => crate::unreachable_validated!(),
                };
                let func_to_call_ty = store.functions.get(func_to_call_addr).ty();
                if *func_ty != func_to_call_ty {
                    return Err(TrapError::SignatureMismatch.into());
                }
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
                }
            }
            DROP => {
                decrement_fuel!(T::get_flat_cost(DROP));
                stack.pop_value();
            }
            SELECT | SELECT_T => {
                decrement_fuel!(T::get_flat_cost(first_instr_byte));
                if first_instr_byte == SELECT_T { wasm.read_vec(ValType::read).unwrap_validated(); }
                let test_val: i32 = stack.pop_value().try_into().unwrap_validated();
                let val2 = stack.pop_value();
                let val1 = stack.pop_value();
                if test_val != 0 { stack.push_value::<T>(val1)?; } else { stack.push_value::<T>(val2)?; }
            }
            LOCAL_GET => {
                decrement_fuel!(T::get_flat_cost(LOCAL_GET));
                let local_idx = wasm.read_var_u32().unwrap_validated() as LocalIdx;
                let value = *stack.get_local(local_idx);
                stack.push_value::<T>(value)?;
            }
            LOCAL_SET => {
                decrement_fuel!(T::get_flat_cost(LOCAL_SET));
                let local_idx = wasm.read_var_u32().unwrap_validated() as LocalIdx;
                let value = stack.pop_value();
                *stack.get_local_mut(local_idx) = value;
            }
            LOCAL_TEE => {
                decrement_fuel!(T::get_flat_cost(LOCAL_TEE));
                let local_idx = wasm.read_var_u32().unwrap_validated() as LocalIdx;
                let value = stack.peek_value().unwrap_validated();
                *stack.get_local_mut(local_idx) = value;
            }
            GLOBAL_GET => {
                decrement_fuel!(T::get_flat_cost(GLOBAL_GET));
                let global_idx = wasm.read_var_u32().unwrap_validated() as GlobalIdx;
                let global_addr = *store.modules.get(current_module).global_addrs.get(global_idx).unwrap_validated();
                let global = store.globals.get(global_addr);
                stack.push_value::<T>(global.value)?;
            }
            GLOBAL_SET => {
                decrement_fuel!(T::get_flat_cost(GLOBAL_SET));
                let global_idx = wasm.read_var_u32().unwrap_validated() as GlobalIdx;
                let global_addr = *store.modules.get(current_module).global_addrs.get(global_idx).unwrap_validated();
                let global = store.globals.get_mut(global_addr);
                global.value = stack.pop_value();
            }
            TABLE_GET => {
                decrement_fuel!(T::get_flat_cost(TABLE_GET));
                let table_idx = wasm.read_var_u32().unwrap_validated() as TableIdx;
                let table_addr = *store.modules.get(current_module).table_addrs.get(table_idx).unwrap_validated();
                let tab = store.tables.get(table_addr);
                let i: i32 = stack.pop_value().try_into().unwrap_validated();
                let val = tab.elem.get(i as usize).ok_or(TrapError::TableOrElementAccessOutOfBounds)?;
                stack.push_value::<T>((*val).into())?;
            }
            TABLE_SET => {
                decrement_fuel!(T::get_flat_cost(TABLE_SET));
                let table_idx = wasm.read_var_u32().unwrap_validated() as TableIdx;
                let table_addr = *store.modules.get(current_module).table_addrs.get(table_idx).unwrap_validated();
                let tab = store.tables.get_mut(table_addr);
                let val: Ref = stack.pop_value().try_into().unwrap_validated();
                let i: i32 = stack.pop_value().try_into().unwrap_validated();
                *tab.elem.get_mut(i as usize).ok_or(TrapError::TableOrElementAccessOutOfBounds)? = val;
            }
            I32_CONST => { decrement_fuel!(T::get_flat_cost(I32_CONST)); let c = wasm.read_var_i32().unwrap_validated(); stack.push_value::<T>(c.into())?; }
            I64_CONST => { decrement_fuel!(T::get_flat_cost(I64_CONST)); let c = wasm.read_var_i64().unwrap_validated(); stack.push_value::<T>(c.into())?; }
            F32_CONST => { decrement_fuel!(T::get_flat_cost(F32_CONST)); let c = F32::from_bits(wasm.read_f32().unwrap_validated()); stack.push_value::<T>(c.into())?; }
            F64_CONST => { decrement_fuel!(T::get_flat_cost(F64_CONST)); let c = F64::from_bits(wasm.read_f64().unwrap_validated()); stack.push_value::<T>(c.into())?; }
            I32_EQZ | I32_CLZ | I32_CTZ | I32_POPCNT => {
                decrement_fuel!(T::get_flat_cost(first_instr_byte));
                let v: i32 = stack.pop_value().try_into().unwrap_validated();
                let res = match first_instr_byte {
                    I32_EQZ => if v == 0 { 1 } else { 0 },
                    I32_CLZ => v.leading_zeros() as i32,
                    I32_CTZ => v.trailing_zeros() as i32,
                    I32_POPCNT => v.count_ones() as i32,
                    _ => unreachable!()
                };
                stack.push_value::<T>(res.into())?;
            }
            I64_EQZ | I64_CLZ | I64_CTZ | I64_POPCNT => {
                decrement_fuel!(T::get_flat_cost(first_instr_byte));
                let v: i64 = stack.pop_value().try_into().unwrap_validated();
                let res = match first_instr_byte {
                    I64_EQZ => if v == 0 { 1 } else { 0 },
                    I64_CLZ => v.leading_zeros() as i64,
                    I64_CTZ => v.trailing_zeros() as i64,
                    I64_POPCNT => v.count_ones() as i64,
                    _ => unreachable!()
                };
                if first_instr_byte == I64_EQZ {
                    stack.push_value::<T>(Value::I32(res as u32))?;
                } else {
                    stack.push_value::<T>(Value::I64(res as u64))?;
                }
            }
            I32_ADD | I32_SUB | I32_MUL | I32_DIV_S | I32_DIV_U | I32_REM_S | I32_REM_U | I32_AND | I32_OR | I32_XOR | I32_SHL | I32_SHR_S | I32_SHR_U | I32_ROTL | I32_ROTR => {
                decrement_fuel!(T::get_flat_cost(first_instr_byte));
                let v2: i32 = stack.pop_value().try_into().unwrap_validated();
                let v1: i32 = stack.pop_value().try_into().unwrap_validated();
                let res = match first_instr_byte {
                    I32_ADD => v1.wrapping_add(v2), I32_SUB => v1.wrapping_sub(v2), I32_MUL => v1.wrapping_mul(v2),
                    I32_DIV_S => if v2 == 0 { return Err(TrapError::DivideBy0.into()); } else if v1 == i32::MIN && v2 == -1 { return Err(TrapError::UnrepresentableResult.into()); } else { v1 / v2 },
                    I32_DIV_U => if v2 == 0 { return Err(TrapError::DivideBy0.into()); } else { ((v1 as u32) / (v2 as u32)) as i32 },
                    I32_REM_S => if v2 == 0 { return Err(TrapError::DivideBy0.into()); } else { v1.checked_rem(v2).unwrap_or(0) },
                    I32_REM_U => if v2 == 0 { return Err(TrapError::DivideBy0.into()); } else { ((v1 as u32) % (v2 as u32)) as i32 },
                    I32_AND => v1 & v2, I32_OR => v1 | v2, I32_XOR => v1 ^ v2,
                    I32_SHL => v1.wrapping_shl(v2 as u32), I32_SHR_S => v1.wrapping_shr(v2 as u32), I32_SHR_U => ((v1 as u32).wrapping_shr(v2 as u32)) as i32,
                    I32_ROTL => v1.rotate_left(v2 as u32), I32_ROTR => v1.rotate_right(v2 as u32),
                    _ => unreachable!()
                };
                stack.push_value::<T>(res.into())?;
            }
            I64_ADD | I64_SUB | I64_MUL | I64_DIV_S | I64_DIV_U | I64_REM_S | I64_REM_U | I64_AND | I64_OR | I64_XOR => {
                decrement_fuel!(T::get_flat_cost(first_instr_byte));
                let v2: i64 = stack.pop_value().try_into().unwrap_validated();
                let v1: i64 = stack.pop_value().try_into().unwrap_validated();
                let res = match first_instr_byte {
                    I64_ADD => v1.wrapping_add(v2), I64_SUB => v1.wrapping_sub(v2), I64_MUL => v1.wrapping_mul(v2),
                    I64_DIV_S => if v2 == 0 { return Err(TrapError::DivideBy0.into()); } else if v1 == i64::MIN && v2 == -1 { return Err(TrapError::UnrepresentableResult.into()); } else { v1 / v2 },
                    I64_DIV_U => if v2 == 0 { return Err(TrapError::DivideBy0.into()); } else { ((v1 as u64) / (v2 as u64)) as i64 },
                    I64_REM_S => if v2 == 0 { return Err(TrapError::DivideBy0.into()); } else { v1.checked_rem(v2).unwrap_or(0) },
                    I64_REM_U => if v2 == 0 { return Err(TrapError::DivideBy0.into()); } else { ((v1 as u64) % (v2 as u64)) as i64 },
                    I64_AND => v1 & v2, I64_OR => v1 | v2, I64_XOR => v1 ^ v2,
                    _ => unreachable!()
                };
                stack.push_value::<T>(res.into())?;
            }
            I64_SHL | I64_SHR_S | I64_SHR_U | I64_ROTL | I64_ROTR => {
                decrement_fuel!(T::get_flat_cost(first_instr_byte));
                let v2: i32 = stack.pop_value().try_into().unwrap_validated();
                let v1: i64 = stack.pop_value().try_into().unwrap_validated();
                let res = match first_instr_byte {
                    I64_SHL => v1.wrapping_shl(v2 as u32), I64_SHR_S => v1.wrapping_shr(v2 as u32), I64_SHR_U => ((v1 as u64).wrapping_shr(v2 as u32)) as i64,
                    I64_ROTL => v1.rotate_left(v2 as u32), I64_ROTR => v1.rotate_right(v2 as u32),
                    _ => unreachable!()
                };
                stack.push_value::<T>(res.into())?;
            }
            I32_EQ | I32_NE | I32_LT_S | I32_LT_U | I32_GT_S | I32_GT_U | I32_LE_S | I32_LE_U | I32_GE_S | I32_GE_U => {
                decrement_fuel!(T::get_flat_cost(first_instr_byte));
                let v2: i32 = stack.pop_value().try_into().unwrap_validated();
                let v1: i32 = stack.pop_value().try_into().unwrap_validated();
                let res = match first_instr_byte {
                    I32_EQ => v1 == v2, I32_NE => v1 != v2,
                    I32_LT_S => v1 < v2, I32_LT_U => (v1 as u32) < (v2 as u32),
                    I32_GT_S => v1 > v2, I32_GT_U => (v1 as u32) > (v2 as u32),
                    I32_LE_S => v1 <= v2, I32_LE_U => (v1 as u32) <= (v2 as u32),
                    I32_GE_S => v1 >= v2, I32_GE_U => (v1 as u32) >= (v2 as u32),
                    _ => unreachable!()
                };
                stack.push_value::<T>(Value::I32(if res { 1 } else { 0 }))?;
            }
            I64_EQ | I64_NE | I64_LT_S | I64_LT_U | I64_GT_S | I64_GT_U | I64_LE_S | I64_LE_U | I64_GE_S | I64_GE_U => {
                decrement_fuel!(T::get_flat_cost(first_instr_byte));
                let v2: i64 = stack.pop_value().try_into().unwrap_validated();
                let v1: i64 = stack.pop_value().try_into().unwrap_validated();
                let res = match first_instr_byte {
                    I64_EQ => v1 == v2, I64_NE => v1 != v2,
                    I64_LT_S => v1 < v2, I64_LT_U => (v1 as u64) < (v2 as u64),
                    I64_GT_S => v1 > v2, I64_GT_U => (v1 as u64) > (v2 as u64),
                    I64_LE_S => v1 <= v2, I64_LE_U => (v1 as u64) <= (v2 as u64),
                    I64_GE_S => v1 >= v2, I64_GE_U => (v1 as u64) >= (v2 as u64),
                    _ => unreachable!()
                };
                stack.push_value::<T>(Value::I32(if res { 1 } else { 0 }))?;
            }
            F32_EQ | F32_NE | F32_LT | F32_GT | F32_LE | F32_GE => {
                decrement_fuel!(T::get_flat_cost(first_instr_byte));
                let v2: F32 = stack.pop_value().try_into().unwrap_validated();
                let v1: F32 = stack.pop_value().try_into().unwrap_validated();
                let res = match first_instr_byte {
                    F32_EQ => v1.0 == v2.0, F32_NE => v1.0 != v2.0,
                    F32_LT => v1.0 < v2.0, F32_GT => v1.0 > v2.0,
                    F32_LE => v1.0 <= v2.0, F32_GE => v1.0 >= v2.0,
                    _ => unreachable!()
                };
                stack.push_value::<T>(Value::I32(if res { 1 } else { 0 }))?;
            }
            F64_EQ | F64_NE | F64_LT | F64_GT | F64_LE | F64_GE => {
                decrement_fuel!(T::get_flat_cost(first_instr_byte));
                let v2: F64 = stack.pop_value().try_into().unwrap_validated();
                let v1: F64 = stack.pop_value().try_into().unwrap_validated();
                let res = match first_instr_byte {
                    F64_EQ => v1.0 == v2.0, F64_NE => v1.0 != v2.0,
                    F64_LT => v1.0 < v2.0, F64_GT => v1.0 > v2.0,
                    F64_LE => v1.0 <= v2.0, F64_GE => v1.0 >= v2.0,
                    _ => unreachable!()
                };
                stack.push_value::<T>(Value::I32(if res { 1 } else { 0 }))?;
            }
            F32_ABS | F32_NEG | F32_CEIL | F32_FLOOR | F32_TRUNC | F32_NEAREST | F32_SQRT => {
                decrement_fuel!(T::get_flat_cost(first_instr_byte));
                let v: F32 = stack.pop_value().try_into().unwrap_validated();
                let res = match first_instr_byte {
                    F32_ABS => v.abs(), F32_NEG => v.neg(), F32_CEIL => v.ceil(),
                    F32_FLOOR => v.floor(), F32_TRUNC => v.trunc(), F32_NEAREST => v.nearest(),
                    F32_SQRT => v.sqrt(), _ => unreachable!()
                };
                stack.push_value::<T>(res.into())?;
            }
            F32_ADD | F32_SUB | F32_MUL | F32_DIV | F32_MIN | F32_MAX | F32_COPYSIGN => {
                decrement_fuel!(T::get_flat_cost(first_instr_byte));
                let v2: F32 = stack.pop_value().try_into().unwrap_validated();
                let v1: F32 = stack.pop_value().try_into().unwrap_validated();
                let res = match first_instr_byte {
                    F32_ADD => v1 + v2, F32_SUB => v1 - v2, F32_MUL => v1 * v2, F32_DIV => v1 / v2,
                    F32_MIN => v1.min(v2), F32_MAX => v1.max(v2), F32_COPYSIGN => v1.copysign(v2),
                    _ => unreachable!()
                };
                stack.push_value::<T>(res.into())?;
            }
            F64_ABS | F64_NEG | F64_CEIL | F64_FLOOR | F64_TRUNC | F64_NEAREST | F64_SQRT => {
                decrement_fuel!(T::get_flat_cost(first_instr_byte));
                let v: F64 = stack.pop_value().try_into().unwrap_validated();
                let res = match first_instr_byte {
                    F64_ABS => v.abs(), F64_NEG => v.neg(), F64_CEIL => v.ceil(),
                    F64_FLOOR => v.floor(), F64_TRUNC => v.trunc(), F64_NEAREST => v.nearest(),
                    F64_SQRT => v.sqrt(), _ => unreachable!()
                };
                stack.push_value::<T>(res.into())?;
            }
            F64_ADD | F64_SUB | F64_MUL | F64_DIV | F64_MIN | F64_MAX | F64_COPYSIGN => {
                decrement_fuel!(T::get_flat_cost(first_instr_byte));
                let v2: F64 = stack.pop_value().try_into().unwrap_validated();
                let v1: F64 = stack.pop_value().try_into().unwrap_validated();
                let res = match first_instr_byte {
                    F64_ADD => v1 + v2, F64_SUB => v1 - v2, F64_MUL => v1 * v2, F64_DIV => v1 / v2,
                    F64_MIN => v1.min(v2), F64_MAX => v1.max(v2), F64_COPYSIGN => v1.copysign(v2),
                    _ => unreachable!()
                };
                stack.push_value::<T>(res.into())?;
            }
            I32_WRAP_I64 | I32_TRUNC_F32_S | I32_TRUNC_F32_U | I32_TRUNC_F64_S | I32_TRUNC_F64_U | I32_REINTERPRET_F32 | I32_EXTEND8_S | I32_EXTEND16_S => {
                decrement_fuel!(T::get_flat_cost(first_instr_byte));
                let res = match first_instr_byte {
                    I32_WRAP_I64 => { let v: i64 = stack.pop_value().try_into().unwrap_validated(); (v as i32).into() }
                    I32_TRUNC_F32_S => { let v: F32 = stack.pop_value().try_into().unwrap_validated(); (v.0 as i32).into() }
                    I32_TRUNC_F32_U => { let v: F32 = stack.pop_value().try_into().unwrap_validated(); (v.0 as u32).into() }
                    I32_TRUNC_F64_S => { let v: F64 = stack.pop_value().try_into().unwrap_validated(); (v.0 as i32).into() }
                    I32_TRUNC_F64_U => { let v: F64 = stack.pop_value().try_into().unwrap_validated(); (v.0 as u32).into() }
                    I32_REINTERPRET_F32 => { let v: F32 = stack.pop_value().try_into().unwrap_validated(); v.reinterpret_as_i32().into() }
                    I32_EXTEND8_S => { let v: i32 = stack.pop_value().try_into().unwrap_validated(); (v as i8 as i32).into() }
                    I32_EXTEND16_S => { let v: i32 = stack.pop_value().try_into().unwrap_validated(); (v as i16 as i32).into() }
                    _ => unreachable!()
                };
                stack.push_value::<T>(res)?;
            }
            I64_EXTEND_I32_S | I64_EXTEND_I32_U | I64_TRUNC_F32_S | I64_TRUNC_F32_U | I64_TRUNC_F64_S | I64_TRUNC_F64_U | I64_REINTERPRET_F64 | I64_EXTEND8_S | I64_EXTEND16_S | I64_EXTEND32_S => {
                decrement_fuel!(T::get_flat_cost(first_instr_byte));
                let res = match first_instr_byte {
                    I64_EXTEND_I32_S => { let v: i32 = stack.pop_value().try_into().unwrap_validated(); (v as i64).into() }
                    I64_EXTEND_I32_U => { let v: i32 = stack.pop_value().try_into().unwrap_validated(); (v as u32 as i64).into() }
                    I64_TRUNC_F32_S => { let v: F32 = stack.pop_value().try_into().unwrap_validated(); (v.0 as i64).into() }
                    I64_TRUNC_F32_U => { let v: F32 = stack.pop_value().try_into().unwrap_validated(); (v.0 as u64 as i64).into() }
                    I64_TRUNC_F64_S => { let v: F64 = stack.pop_value().try_into().unwrap_validated(); (v.0 as i64).into() }
                    I64_TRUNC_F64_U => { let v: F64 = stack.pop_value().try_into().unwrap_validated(); (v.0 as u64 as i64).into() }
                    I64_REINTERPRET_F64 => { let v: F64 = stack.pop_value().try_into().unwrap_validated(); v.reinterpret_as_i64().into() }
                    I64_EXTEND8_S => { let v: i64 = stack.pop_value().try_into().unwrap_validated(); (v as i8 as i64).into() }
                    I64_EXTEND16_S => { let v: i64 = stack.pop_value().try_into().unwrap_validated(); (v as i16 as i64).into() }
                    I64_EXTEND32_S => { let v: i64 = stack.pop_value().try_into().unwrap_validated(); (v as i32 as i64).into() }
                    _ => unreachable!()
                };
                stack.push_value::<T>(res)?;
            }
            F32_CONVERT_I32_S | F32_CONVERT_I32_U | F32_CONVERT_I64_S | F32_CONVERT_I64_U | F32_DEMOTE_F64 | F32_REINTERPRET_I32 => {
                decrement_fuel!(T::get_flat_cost(first_instr_byte));
                let res = match first_instr_byte {
                    F32_CONVERT_I32_S => { let v: i32 = stack.pop_value().try_into().unwrap_validated(); F32(v as f32).into() }
                    F32_CONVERT_I32_U => { let v: i32 = stack.pop_value().try_into().unwrap_validated(); F32(v as u32 as f32).into() }
                    F32_CONVERT_I64_S => { let v: i64 = stack.pop_value().try_into().unwrap_validated(); F32(v as f32).into() }
                    F32_CONVERT_I64_U => { let v: i64 = stack.pop_value().try_into().unwrap_validated(); F32(v as u64 as f32).into() }
                    F32_DEMOTE_F64 => { let v: F64 = stack.pop_value().try_into().unwrap_validated(); F32(v.0 as f32).into() }
                    F32_REINTERPRET_I32 => { let v: i32 = stack.pop_value().try_into().unwrap_validated(); F32::from_bits(v as u32).into() }
                    _ => unreachable!()
                };
                stack.push_value::<T>(res)?;
            }
            F64_CONVERT_I32_S | F64_CONVERT_I32_U | F64_CONVERT_I64_S | F64_CONVERT_I64_U | F64_PROMOTE_F32 | F64_REINTERPRET_I64 => {
                decrement_fuel!(T::get_flat_cost(first_instr_byte));
                let res = match first_instr_byte {
                    F64_CONVERT_I32_S => { let v: i32 = stack.pop_value().try_into().unwrap_validated(); F64(v as f64).into() }
                    F64_CONVERT_I32_U => { let v: i32 = stack.pop_value().try_into().unwrap_validated(); F64(v as u32 as f64).into() }
                    F64_CONVERT_I64_S => { let v: i64 = stack.pop_value().try_into().unwrap_validated(); F64(v as f64).into() }
                    F64_CONVERT_I64_U => { let v: i64 = stack.pop_value().try_into().unwrap_validated(); F64(v as u64 as f64).into() }
                    F64_PROMOTE_F32 => { let v: F32 = stack.pop_value().try_into().unwrap_validated(); F64(v.0 as f64).into() }
                    F64_REINTERPRET_I64 => { let v: i64 = stack.pop_value().try_into().unwrap_validated(); F64::from_bits(v as u64).into() }
                    _ => unreachable!()
                };
                stack.push_value::<T>(res)?;
            }
            UNREACHABLE => { return Err(TrapError::ReachedUnreachable.into()); }
            MEMORY_SIZE => {
                decrement_fuel!(T::get_flat_cost(MEMORY_SIZE));
                wasm.read_u8().unwrap_validated();
                let mem_addr = *store.modules.get(current_module).mem_addrs.first().unwrap_validated();
                let size = store.memories.get(mem_addr).size() as u32;
                stack.push_value::<T>(Value::I32(size))?;
            }
            MEMORY_GROW => {
                wasm.read_u8().unwrap_validated();
                let mem_addr = *store.modules.get(current_module).mem_addrs.first().unwrap_validated();
                let sz: u32 = store.memories.get(mem_addr).size() as u32;
                let n: u32 = stack.pop_value().try_into().unwrap_validated();
                let cost = T::get_flat_cost(MEMORY_GROW) + n * T::get_cost_per_element(MEMORY_GROW);
                decrement_fuel!(cost);
                let res = match store.memories.get_mut(mem_addr).grow(n) {
                    Ok(_) => sz,
                    Err(_) => u32::MAX,
                };
                stack.push_value::<T>(Value::I32(res))?;
            }
            I32_LOAD | I64_LOAD | F32_LOAD | F64_LOAD | I32_LOAD8_S | I32_LOAD8_U | I32_LOAD16_S | I32_LOAD16_U | I64_LOAD8_S | I64_LOAD8_U | I64_LOAD16_S | I64_LOAD16_U | I64_LOAD32_S | I64_LOAD32_U => {
                decrement_fuel!(T::get_flat_cost(first_instr_byte));
                let memarg = MemArg::read(wasm).unwrap_validated();
                let rel: u32 = stack.pop_value().try_into().unwrap_validated();
                let mem_addr = *store.modules.get(current_module).mem_addrs.first().unwrap_validated();
                let mem = store.memories.get(mem_addr);
                let idx = calculate_mem_address(&memarg, rel)?;
                match first_instr_byte {
                    I32_LOAD => { let v: u32 = mem.mem.load(idx)?; stack.push_value::<T>(Value::I32(v))?; }
                    I64_LOAD => { let v: u64 = mem.mem.load(idx)?; stack.push_value::<T>(Value::I64(v))?; }
                    F32_LOAD => { let v: F32 = mem.mem.load(idx)?; stack.push_value::<T>(Value::F32(v))?; }
                    F64_LOAD => { let v: F64 = mem.mem.load(idx)?; stack.push_value::<T>(Value::F64(v))?; }
                    I32_LOAD8_S => { let v: i8 = mem.mem.load(idx)?; stack.push_value::<T>(Value::I32(v as u32))?; }
                    I32_LOAD8_U => { let v: u8 = mem.mem.load(idx)?; stack.push_value::<T>(Value::I32(v as u32))?; }
                    I32_LOAD16_S => { let v: i16 = mem.mem.load(idx)?; stack.push_value::<T>(Value::I32(v as u32))?; }
                    I32_LOAD16_U => { let v: u16 = mem.mem.load(idx)?; stack.push_value::<T>(Value::I32(v as u32))?; }
                    I64_LOAD8_S => { let v: i8 = mem.mem.load(idx)?; stack.push_value::<T>(Value::I64(v as u64))?; }
                    I64_LOAD8_U => { let v: u8 = mem.mem.load(idx)?; stack.push_value::<T>(Value::I64(v as u64))?; }
                    I64_LOAD16_S => { let v: i16 = mem.mem.load(idx)?; stack.push_value::<T>(Value::I64(v as u64))?; }
                    I64_LOAD16_U => { let v: u16 = mem.mem.load(idx)?; stack.push_value::<T>(Value::I64(v as u64))?; }
                    I64_LOAD32_S => { let v: i32 = mem.mem.load(idx)?; stack.push_value::<T>(Value::I64(v as u64))?; }
                    I64_LOAD32_U => { let v: u32 = mem.mem.load(idx)?; stack.push_value::<T>(Value::I64(v as u64))?; }
                    _ => unreachable!()
                }
            }
            I32_STORE | I64_STORE | F32_STORE | F64_STORE | I32_STORE8 | I32_STORE16 | I64_STORE8 | I64_STORE16 | I64_STORE32 => {
                decrement_fuel!(T::get_flat_cost(first_instr_byte));
                let memarg = MemArg::read(wasm).unwrap_validated();
                let val = stack.pop_value();
                let rel: u32 = stack.pop_value().try_into().unwrap_validated();
                let mem_addr = *store.modules.get(current_module).mem_addrs.first().unwrap_validated();
                let mem = store.memories.get_mut(mem_addr);
                let idx = calculate_mem_address(&memarg, rel)?;
                match first_instr_byte {
                    I32_STORE => { mem.mem.store(idx, u32::try_from(val).unwrap_validated())?; }
                    I64_STORE => { mem.mem.store(idx, u64::try_from(val).unwrap_validated())?; }
                    F32_STORE => { mem.mem.store(idx, F32::try_from(val).unwrap_validated())?; }
                    F64_STORE => { mem.mem.store(idx, F64::try_from(val).unwrap_validated())?; }
                    I32_STORE8 => { mem.mem.store(idx, (u32::try_from(val).unwrap_validated() as u8) as i8)?; }
                    I32_STORE16 => { mem.mem.store(idx, (u32::try_from(val).unwrap_validated() as u16) as i16)?; }
                    I64_STORE8 => { mem.mem.store(idx, (u64::try_from(val).unwrap_validated() as u8) as i8)?; }
                    I64_STORE16 => { mem.mem.store(idx, (u64::try_from(val).unwrap_validated() as u16) as i16)?; }
                    I64_STORE32 => { mem.mem.store(idx, (u64::try_from(val).unwrap_validated() as u32) as i32)?; }
                    _ => unreachable!()
                }
            }
            REF_NULL => { decrement_fuel!(T::get_flat_cost(REF_NULL)); let reftype = RefType::read(wasm).unwrap_validated(); stack.push_value::<T>(Value::Ref(Ref::Null(reftype)))?; }
            REF_IS_NULL => { decrement_fuel!(T::get_flat_cost(REF_IS_NULL)); let r: Ref = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::I32(if matches!(r, Ref::Null(_)) { 1 } else { 0 }))?; }
            REF_FUNC => { decrement_fuel!(T::get_flat_cost(REF_FUNC)); let func_idx = wasm.read_var_u32().unwrap_validated() as usize; let func_addr = store.modules.get(current_module).func_addrs[func_idx]; stack.push_value::<T>(Value::Ref(Ref::Func(func_addr)))?; }
            FC_EXTENSIONS => {
                let instr = wasm.read_var_u32().unwrap_validated();
                match instr {
                    0x00..=0x07 => { // saturating trunc
                        decrement_fuel!(T::get_flat_cost(FC_EXTENSIONS));
                        let res = match instr {
                            0x00 => { let v: F32 = stack.pop_value().try_into().unwrap_validated(); Value::I32(if v.0.is_nan() { 0 } else { v.0.min(i32::MAX as f32).max(i32::MIN as f32) as i32 as u32 }) }
                            0x01 => { let v: F32 = stack.pop_value().try_into().unwrap_validated(); Value::I32(if v.0.is_nan() { 0 } else { v.0.min(u32::MAX as f32).max(0.0) as u32 }) }
                            0x02 => { let v: F64 = stack.pop_value().try_into().unwrap_validated(); Value::I32(if v.0.is_nan() { 0 } else { v.0.min(i32::MAX as f64).max(i32::MIN as f64) as i32 as u32 }) }
                            0x03 => { let v: F64 = stack.pop_value().try_into().unwrap_validated(); Value::I32(if v.0.is_nan() { 0 } else { v.0.min(u32::MAX as f64).max(0.0) as u32 }) }
                            0x04 => { let v: F32 = stack.pop_value().try_into().unwrap_validated(); Value::I64(if v.0.is_nan() { 0 } else { v.0.min(i64::MAX as f32).max(i64::MIN as f32) as i64 as u64 }) }
                            0x05 => { let v: F32 = stack.pop_value().try_into().unwrap_validated(); Value::I64(if v.0.is_nan() { 0 } else { v.0.min(u64::MAX as f32).max(0.0) as u64 }) }
                            0x06 => { let v: F64 = stack.pop_value().try_into().unwrap_validated(); Value::I64(if v.0.is_nan() { 0 } else { v.0.min(i64::MAX as f64).max(i64::MIN as f64) as i64 as u64 }) }
                            0x07 => { let v: F64 = stack.pop_value().try_into().unwrap_validated(); Value::I64(if v.0.is_nan() { 0 } else { v.0.min(u64::MAX as f64).max(0.0) as u64 }) }
                            _ => unreachable!()
                        };
                        stack.push_value::<T>(res)?;
                    }
                    0x08 => { // memory.init
                        let data_idx = wasm.read_var_u32().unwrap_validated() as usize;
                        wasm.read_u8().unwrap_validated();
                        let n: u32 = stack.pop_value().try_into().unwrap_validated();
                        let s: u32 = stack.pop_value().try_into().unwrap_validated();
                        let d: u32 = stack.pop_value().try_into().unwrap_validated();
                        memory_init(&store.modules, &mut store.memories, &store.data, current_module, data_idx, 0, n, s as i32, d as i32)?;
                    }
                    0x09 => { // data.drop
                        let data_idx = wasm.read_var_u32().unwrap_validated() as usize;
                        data_drop(&store.modules, &mut store.data, current_module, data_idx)?;
                    }
                    0x0A => { // memory.copy
                        wasm.read_u8().unwrap_validated(); wasm.read_u8().unwrap_validated();
                        let n: u32 = stack.pop_value().try_into().unwrap_validated();
                        let s: u32 = stack.pop_value().try_into().unwrap_validated();
                        let d: u32 = stack.pop_value().try_into().unwrap_validated();
                        let mem_addr = *store.modules.get(current_module).mem_addrs.first().unwrap_validated();
                        let mem = store.memories.get(mem_addr);
                        mem.mem.copy(d as usize, &mem.mem, s as usize, n as usize)?;
                    }
                    0x0B => { // memory.fill
                        wasm.read_u8().unwrap_validated();
                        let n: u32 = stack.pop_value().try_into().unwrap_validated();
                        let val: u32 = stack.pop_value().try_into().unwrap_validated();
                        let d: u32 = stack.pop_value().try_into().unwrap_validated();
                        let mem_addr = *store.modules.get(current_module).mem_addrs.first().unwrap_validated();
                        let mem = store.memories.get(mem_addr);
                        mem.mem.fill(d as usize, val as u8, n as usize)?;
                    }
                    0x0C => { // table.init
                        let elem_idx = wasm.read_var_u32().unwrap_validated() as usize;
                        let table_idx = wasm.read_var_u32().unwrap_validated() as usize;
                        let n: u32 = stack.pop_value().try_into().unwrap_validated();
                        let s: u32 = stack.pop_value().try_into().unwrap_validated();
                        let d: u32 = stack.pop_value().try_into().unwrap_validated();
                        table_init(&store.modules, &mut store.tables, &store.elements, current_module, elem_idx, table_idx, n, s as i32, d as i32)?;
                    }
                    0x0D => { // elem.drop
                        let elem_idx = wasm.read_var_u32().unwrap_validated() as usize;
                        elem_drop(&store.modules, &mut store.elements, current_module, elem_idx)?;
                    }
                    0x0E => { // table.copy
                        let x = wasm.read_var_u32().unwrap_validated() as usize;
                        let y = wasm.read_var_u32().unwrap_validated() as usize;
                        let n: u32 = stack.pop_value().try_into().unwrap_validated();
                        let s: u32 = stack.pop_value().try_into().unwrap_validated();
                        let d: u32 = stack.pop_value().try_into().unwrap_validated();
                        let tx_addr = store.modules.get(current_module).table_addrs[x];
                        let ty_addr = store.modules.get(current_module).table_addrs[y];
                        
                        if tx_addr == ty_addr {
                            let t = store.tables.get_mut(tx_addr);
                            let d = d as usize;
                            let s = s as usize;
                            if d <= s {
                                for i in 0..n as usize { t.elem[d + i] = t.elem[s + i]; }
                            } else {
                                for i in (0..n as usize).rev() { t.elem[d + i] = t.elem[s + i]; }
                            }
                        } else {
                            let (tx, ty) = store.tables.get_two_mut(tx_addr, ty_addr).unwrap_validated();
                            for i in 0..n as usize { ty.elem[d as usize + i] = tx.elem[s as usize + i]; }
                        }
                    }
                    0x0F => { // table.grow
                        let x = wasm.read_var_u32().unwrap_validated() as usize;
                        let n: u32 = stack.pop_value().try_into().unwrap_validated();
                        let val: Ref = stack.pop_value().try_into().unwrap_validated();
                        let t_addr = store.modules.get(current_module).table_addrs[x];
                        let t = store.tables.get_mut(t_addr);
                        let sz = t.elem.len() as i32;
                        t.elem.extend(core::iter::repeat(val).take(n as usize));
                        stack.push_value::<T>(Value::I32(sz as u32))?;
                    }
                    0x10 => { // table.size
                        let x = wasm.read_var_u32().unwrap_validated() as usize;
                        let t_addr = store.modules.get(current_module).table_addrs[x];
                        stack.push_value::<T>(Value::I32(store.tables.get(t_addr).elem.len() as u32))?;
                    }
                    0x11 => { // table.fill
                        let x = wasm.read_var_u32().unwrap_validated() as usize;
                        let n: u32 = stack.pop_value().try_into().unwrap_validated();
                        let val: Ref = stack.pop_value().try_into().unwrap_validated();
                        let d: u32 = stack.pop_value().try_into().unwrap_validated();
                        let t_addr = store.modules.get(current_module).table_addrs[x];
                        let t = store.tables.get_mut(t_addr);
                        for i in 0..n as usize { t.elem[d as usize + i] = val; }
                    }
                    _ => { return Err(TrapError::ReachedUnreachable.into()); }
                }
            }
            FD_EXTENSIONS => {
                let instr = wasm.read_var_u32().unwrap_validated();
                super::simd_instructions::execute_simd_instruction::<T>(instr, stack, wasm, store, current_module)?;
            }
            ATOMIC_PREFIX => {
                let sub = wasm.read_var_u32().unwrap_validated();
                let memarg = MemArg::read(wasm).unwrap_validated();
                let mem_addr = store.modules.get(current_module).mem_addrs[0];
                match sub {
                    0x10 | 0x12 | 0x13 => { // i32.atomic.load
                        let addr: u32 = stack.pop_value().try_into().unwrap_validated();
                        let idx = calculate_mem_address(&memarg, addr)?;
                        let mem = store.memories.get(mem_addr);
                        let val: u32 = match sub {
                            0x10 => mem.mem.load(idx)?,
                            0x12 => mem.mem.load::<1, u8>(idx)? as u32,
                            0x13 => mem.mem.load::<2, u16>(idx)? as u32,
                            _ => unreachable!()
                        };
                        stack.push_value::<T>(Value::I32(val))?;
                    }
                    0x11 | 0x14 | 0x15 | 0x16 => { // i64.atomic.load
                        let addr: u32 = stack.pop_value().try_into().unwrap_validated();
                        let idx = calculate_mem_address(&memarg, addr)?;
                        let mem = store.memories.get(mem_addr);
                        let val: u64 = match sub {
                            0x11 => mem.mem.load(idx)?,
                            0x14 => mem.mem.load::<1, u8>(idx)? as u64,
                            0x15 => mem.mem.load::<2, u16>(idx)? as u64,
                            0x16 => mem.mem.load::<4, u32>(idx)? as u64,
                            _ => unreachable!()
                        };
                        stack.push_value::<T>(Value::I64(val))?;
                    }
                    0x17 | 0x19 | 0x1a => { // i32.atomic.store
                        let val: u32 = stack.pop_value().try_into().unwrap_validated();
                        let addr: u32 = stack.pop_value().try_into().unwrap_validated();
                        let idx = calculate_mem_address(&memarg, addr)?;
                        let mem = store.memories.get_mut(mem_addr);
                        match sub {
                            0x17 => mem.mem.store(idx, val)?,
                            0x19 => mem.mem.store::<1, u8>(idx, val as u8)?,
                            0x1a => mem.mem.store::<2, u16>(idx, val as u16)?,
                            _ => unreachable!()
                        };
                    }
                    0x18 | 0x1b | 0x1c | 0x1d => { // i64.atomic.store
                        let val: u64 = stack.pop_value().try_into().unwrap_validated();
                        let addr: u32 = stack.pop_value().try_into().unwrap_validated();
                        let idx = calculate_mem_address(&memarg, addr)?;
                        let mem = store.memories.get_mut(mem_addr);
                        match sub {
                            0x18 => mem.mem.store(idx, val)?,
                            0x1b => mem.mem.store::<1, u8>(idx, val as u8)?,
                            0x1c => mem.mem.store::<2, u16>(idx, val as u16)?,
                            0x1d => mem.mem.store::<4, u32>(idx, val as u32)?,
                            _ => unreachable!()
                        };
                    }
                    _ => {
                        // RMW operations
                        let is_i64 = match sub {
                            0x1f | 0x22 | 0x23 | 0x24 | // Add
                            0x26 | 0x29 | 0x2a | 0x2b | // Sub
                            0x2d | 0x30 | 0x31 | 0x32 | // And
                            0x34 | 0x37 | 0x38 | 0x39 | // Or
                            0x3b | 0x3e | 0x3f | 0x40 | // Xor
                            0x42 | 0x45 | 0x46 | 0x47 | // Xchg
                            0x49 | 0x4c | 0x4d | 0x4e    // Cmpxchg
                            => true,
                            _ => false,
                        };
                        let is_cmpxchg = sub >= 0x48 && sub <= 0x4e;
                        
                        let val: Value = stack.pop_value();
                        let expect: Option<Value> = if is_cmpxchg { Some(stack.pop_value()) } else { None };
                        let addr: u32 = stack.pop_value().try_into().unwrap_validated();
                        let idx = calculate_mem_address(&memarg, addr)?;
                        let mem = &mut store.memories.get_mut(mem_addr).mem;
                        
                        macro_rules! rmw {
                            ($t:ty, $v:expr, $op:expr) => {{
                                let old: $t = mem.load(idx)?;
                                let new = $op(old, $v as $t);
                                mem.store(idx, new)?;
                                Value::from(old)
                            }};
                        }

                        let res = match sub {
                            0x1e | 0x20 | 0x21 => rmw!(u32, u32::try_from(val).unwrap_validated(), |a: u32, b: u32| a.wrapping_add(b)),
                            0x1f | 0x22 | 0x23 | 0x24 => rmw!(u64, u64::try_from(val).unwrap_validated(), |a: u64, b: u64| a.wrapping_add(b)),
                            
                            0x25 | 0x27 | 0x28 => rmw!(u32, u32::try_from(val).unwrap_validated(), |a: u32, b: u32| a.wrapping_sub(b)),
                            0x26 | 0x29 | 0x2a | 0x2b => rmw!(u64, u64::try_from(val).unwrap_validated(), |a: u64, b: u64| a.wrapping_sub(b)),
                            
                            0x2c | 0x2e | 0x2f => rmw!(u32, u32::try_from(val).unwrap_validated(), |a: u32, b: u32| a & b),
                            0x2d | 0x30 | 0x31 | 0x32 => rmw!(u64, u64::try_from(val).unwrap_validated(), |a: u64, b: u64| a & b),
                            
                            0x33 | 0x35 | 0x36 => rmw!(u32, u32::try_from(val).unwrap_validated(), |a: u32, b: u32| a | b),
                            0x34 | 0x37 | 0x38 | 0x39 => rmw!(u64, u64::try_from(val).unwrap_validated(), |a: u64, b: u64| a | b),
                            
                            0x3a | 0x3c | 0x3d => rmw!(u32, u32::try_from(val).unwrap_validated(), |a: u32, b: u32| a ^ b),
                            0x3b | 0x3e | 0x3f | 0x40 => rmw!(u64, u64::try_from(val).unwrap_validated(), |a: u64, b: u64| a ^ b),
                            
                            0x41 | 0x43 | 0x44 => rmw!(u32, u32::try_from(val).unwrap_validated(), |a: u32, b: u32| b),
                            0x42 | 0x45 | 0x46 | 0x47 => rmw!(u64, u64::try_from(val).unwrap_validated(), |a: u64, b: u64| b),

                            0x48 | 0x4a | 0x4b => { // i32.atomic.rmw.cmpxchg
                                let v_new = u32::try_from(val).unwrap_validated();
                                let v_exp = u32::try_from(expect.unwrap()).unwrap_validated();
                                let old: u32 = mem.load(idx)?;
                                if old == v_exp { mem.store(idx, v_new)?; }
                                Value::I32(old)
                            }
                            0x49 | 0x4c | 0x4d | 0x4e => { // i64.atomic.rmw.cmpxchg
                                let v_new = u64::try_from(val).unwrap_validated();
                                let v_exp = u64::try_from(expect.unwrap()).unwrap_validated();
                                let old: u64 = mem.load(idx)?;
                                if old == v_exp { mem.store(idx, v_new)?; }
                                Value::I64(old)
                            }
                            _ => { return Err(TrapError::ReachedUnreachable.into()); }
                        };
                        stack.push_value::<T>(res)?;
                    }
                }
            }
            _ => { 
                return Err(TrapError::ReachedUnreachable.into());
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
    let entry = &sidetable[*stp];
    wasm.pc = (wasm.pc as isize + entry.delta_pc) as usize;
    *stp = (*stp as isize + entry.delta_stp) as usize;
    stack.remove_in_between(entry.popcnt, entry.valcnt);
    Ok(())
}

fn calculate_mem_address(memarg: &MemArg, relative_address: u32) -> Result<usize, TrapError> {
    (memarg.offset as u64 + relative_address as u64)
        .try_into()
        .map_err(|_| TrapError::MemoryOrDataAccessOutOfBounds)
}

pub fn memory_init(
    modules: &AddrVec<ModuleAddr, ModuleInst>,
    memories: &mut AddrVec<MemAddr, crate::wasm::interpreter::store::instances::MemInst>,
    data: &AddrVec<DataAddr, crate::wasm::interpreter::store::instances::DataInst>,
    module_addr: ModuleAddr,
    data_idx: usize,
    mem_idx: usize,
    n: u32,
    s: i32,
    d: i32,
) -> Result<(), RuntimeError> {
    let data_addr = modules.get(module_addr).data_addrs[data_idx];
    let mem_addr = modules.get(module_addr).mem_addrs[mem_idx];
    let data_inst = data.get(data_addr);
    let mem_inst = memories.get_mut(mem_addr);
    mem_inst.mem.init(d as usize, &data_inst.data, s as usize, n as usize)
}

pub fn data_drop(
    modules: &AddrVec<ModuleAddr, ModuleInst>,
    data: &mut AddrVec<DataAddr, crate::wasm::interpreter::store::instances::DataInst>,
    module_addr: ModuleAddr,
    data_idx: usize,
) -> Result<(), RuntimeError> {
    let data_addr = modules.get(module_addr).data_addrs[data_idx];
    data.get_mut(data_addr).data.clear();
    Ok(())
}

pub fn table_init(
    modules: &AddrVec<ModuleAddr, ModuleInst>,
    tables: &mut AddrVec<TableAddr, crate::wasm::interpreter::store::instances::TableInst>,
    elements: &AddrVec<ElemAddr, crate::wasm::interpreter::store::instances::ElemInst>,
    module_addr: ModuleAddr,
    elem_idx: usize,
    table_idx: usize,
    n: u32,
    s: i32,
    d: i32,
) -> Result<(), RuntimeError> {
    let elem_addr = modules.get(module_addr).elem_addrs[elem_idx];
    let table_addr = modules.get(module_addr).table_addrs[table_idx];
    let elem_inst = elements.get(elem_addr);
    let table_inst = tables.get_mut(table_addr);
    for i in 0..n as usize {
        let val = elem_inst.references.get((s as usize).wrapping_add(i)).ok_or(TrapError::TableOrElementAccessOutOfBounds)?;
        *table_inst.elem.get_mut((d as usize).wrapping_add(i)).ok_or(TrapError::TableOrElementAccessOutOfBounds)? = *val;
    }
    Ok(())
}

pub fn elem_drop(
    modules: &AddrVec<ModuleAddr, ModuleInst>,
    elements: &mut AddrVec<ElemAddr, crate::wasm::interpreter::store::instances::ElemInst>,
    module_addr: ModuleAddr,
    elem_idx: usize,
) -> Result<(), RuntimeError> {
    let elem_addr = modules.get(module_addr).elem_addrs[elem_idx];
    elements.get_mut(elem_addr).references.clear();
    Ok(())
}
