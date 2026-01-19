
use crate::wasm::execution::store::Store;
use crate::wasm::execution::config::Config;
use crate::wasm::execution::{Stack, Value};
use crate::wasm::{RuntimeError, TrapError};
use crate::wasm::core::reader::{WasmReader, WasmReadable};
use crate::wasm::core::reader::types::memarg::MemArg;
use crate::wasm::core::reader::types::opcode::fd_extensions::*;
use crate::wasm::execution::simd_utils::*;
use crate::wasm::execution::assert_validated::UnwrapValidatedExt;
use crate::wasm::execution::little_endian::LittleEndianBytes;

fn calculate_mem_address(memarg: &MemArg, relative_address: u32) -> Result<usize, TrapError> {
    (memarg.offset as u64 + relative_address as u64)
        .try_into()
        .map_err(|_| TrapError::MemoryOrDataAccessOutOfBounds)
}

pub fn execute_simd_instruction<T: Config>(
    instr: u32,
    stack: &mut Stack,
    wasm: &mut WasmReader,
    store: &Store<T>,
    current_module: usize,
) -> Result<(), RuntimeError> {
    match instr {
        V128_LOAD => {
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
            let memarg = MemArg::read(wasm).unwrap_validated();
            let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
            let mem_addr = *store.modules.get(current_module).mem_addrs.first().unwrap_validated();
            let mem_inst = store.memories.get(mem_addr);
            let idx = calculate_mem_address(&memarg, relative_address)?;
            let data = mem_inst.mem.load_bytes::<1>(idx).map_err(|e| { e })?;
            stack.push_value::<T>(Value::V128(splat(data)))?;
        }
        V128_LOAD16_SPLAT => {
            let memarg = MemArg::read(wasm).unwrap_validated();
            let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
            let mem_addr = *store.modules.get(current_module).mem_addrs.first().unwrap_validated();
            let mem_inst = store.memories.get(mem_addr);
            let idx = calculate_mem_address(&memarg, relative_address)?;
            let data = mem_inst.mem.load_bytes::<2>(idx).map_err(|e| { e })?;
            stack.push_value::<T>(Value::V128(splat(data)))?;
        }
        V128_LOAD32_SPLAT => {
            let memarg = MemArg::read(wasm).unwrap_validated();
            let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
            let mem_addr = *store.modules.get(current_module).mem_addrs.first().unwrap_validated();
            let mem_inst = store.memories.get(mem_addr);
            let idx = calculate_mem_address(&memarg, relative_address)?;
            let data = mem_inst.mem.load_bytes::<4>(idx).map_err(|e| { e })?;
            stack.push_value::<T>(Value::V128(splat(data)))?;
        }
        V128_LOAD64_SPLAT => {
            let memarg = MemArg::read(wasm).unwrap_validated();
            let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
            let mem_addr = *store.modules.get(current_module).mem_addrs.first().unwrap_validated();
            let mem_inst = store.memories.get(mem_addr);
            let idx = calculate_mem_address(&memarg, relative_address)?;
            let data = mem_inst.mem.load_bytes::<8>(idx).map_err(|e| { e })?;
            stack.push_value::<T>(Value::V128(splat(data)))?;
        }
        V128_LOAD32_ZERO => {
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
            let memarg = MemArg::read(wasm).unwrap_validated();
            let val: [u8; 16] = stack.pop_value().try_into().unwrap_validated();
            let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
            let mem_addr = *store.modules.get(current_module).mem_addrs.first().unwrap_validated();
            let mem_inst = store.memories.get(mem_addr);
            let idx = calculate_mem_address(&memarg, relative_address)?;
            mem_inst.mem.store_bytes(idx, val).map_err(|e| { e })?;
        }
        V128_LOAD8_LANE => {
            let memarg = MemArg::read(wasm).unwrap_validated();
            let lane_idx = wasm.read_u8().unwrap_validated();
            let mut val: [u8; 16] = stack.pop_value().try_into().unwrap_validated();
            let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
            let mem_addr = *store.modules.get(current_module).mem_addrs.first().unwrap_validated();
            let mem_inst = store.memories.get(mem_addr);
            let idx = calculate_mem_address(&memarg, relative_address)?;
            let byte = mem_inst.mem.load_bytes::<1>(idx).map_err(|e| { e })?;
            val[lane_idx as usize] = byte[0];
            stack.push_value::<T>(Value::V128(val))?;
        }
        V128_LOAD16_LANE => {
            let memarg = MemArg::read(wasm).unwrap_validated();
            let lane_idx = wasm.read_u8().unwrap_validated();
            let mut val: [u8; 16] = stack.pop_value().try_into().unwrap_validated();
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
            let memarg = MemArg::read(wasm).unwrap_validated();
            let lane_idx = wasm.read_u8().unwrap_validated();
            let mut val: [u8; 16] = stack.pop_value().try_into().unwrap_validated();
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
            let memarg = MemArg::read(wasm).unwrap_validated();
            let lane_idx = wasm.read_u8().unwrap_validated();
            let mut val: [u8; 16] = stack.pop_value().try_into().unwrap_validated();
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
            let memarg = MemArg::read(wasm).unwrap_validated();
            let lane_idx = wasm.read_u8().unwrap_validated();
            let val: [u8; 16] = stack.pop_value().try_into().unwrap_validated();
            let relative_address: u32 = stack.pop_value().try_into().unwrap_validated();
            let mem_addr = *store.modules.get(current_module).mem_addrs.first().unwrap_validated();
            let mem_inst = store.memories.get(mem_addr);
            let idx = calculate_mem_address(&memarg, relative_address)?;
            let byte = [val[lane_idx as usize]];
            mem_inst.mem.store_bytes(idx, byte).map_err(|e| { e })?;
        }
        V128_STORE16_LANE => {
            let memarg = MemArg::read(wasm).unwrap_validated();
            let lane_idx = wasm.read_u8().unwrap_validated();
            let val: [u8; 16] = stack.pop_value().try_into().unwrap_validated();
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
            let memarg = MemArg::read(wasm).unwrap_validated();
            let lane_idx = wasm.read_u8().unwrap_validated();
            let val: [u8; 16] = stack.pop_value().try_into().unwrap_validated();
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
            let memarg = MemArg::read(wasm).unwrap_validated();
            let lane_idx = wasm.read_u8().unwrap_validated();
            let val: [u8; 16] = stack.pop_value().try_into().unwrap_validated();
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
            let mut lanes = [0u8; 16];
            for byte_ref in &mut lanes {
                *byte_ref = wasm.read_u8().unwrap_validated();
            }
            let v2: [u8; 16] = stack.pop_value().try_into().unwrap_validated();
            let v1: [u8; 16] = stack.pop_value().try_into().unwrap_validated();
            stack.push_value::<T>(Value::V128(i8x16_shuffle(v1, v2, lanes)))?;
        }
        I8X16_EXTRACT_LANE_S => {
            let lane_idx = wasm.read_u8().unwrap_validated();
            let val: [u8; 16] = stack.pop_value().try_into().unwrap_validated();
            let res = val[lane_idx as usize] as i8 as i32;
            stack.push_value::<T>(Value::I32(res as u32))?;
        }
        I8X16_EXTRACT_LANE_U => {
            let lane_idx = wasm.read_u8().unwrap_validated();
            let val: [u8; 16] = stack.pop_value().try_into().unwrap_validated();
            let res = val[lane_idx as usize] as u32;
            stack.push_value::<T>(Value::I32(res))?;
        }
        I8X16_REPLACE_LANE => {
            let lane_idx = wasm.read_u8().unwrap_validated();
            let x: u32 = stack.pop_value().try_into().unwrap_validated();
            let mut val: [u8; 16] = stack.pop_value().try_into().unwrap_validated();
            val[lane_idx as usize] = x as u8;
            stack.push_value::<T>(Value::V128(val))?;
        }
        I16X8_EXTRACT_LANE_S => {
            let lane_idx = wasm.read_u8().unwrap_validated();
            let val: [u8; 16] = stack.pop_value().try_into().unwrap_validated();
            let lanes = to_lanes::<2, 8, i16>(val);
            let res = lanes[lane_idx as usize] as i32;
            stack.push_value::<T>(Value::I32(res as u32))?;
        }
        I16X8_EXTRACT_LANE_U => {
            let lane_idx = wasm.read_u8().unwrap_validated();
            let val: [u8; 16] = stack.pop_value().try_into().unwrap_validated();
            let lanes = to_lanes::<2, 8, u16>(val);
            let res = lanes[lane_idx as usize] as u32;
            stack.push_value::<T>(Value::I32(res))?;
        }
        I16X8_REPLACE_LANE => {
            let lane_idx = wasm.read_u8().unwrap_validated();
            let x: u32 = stack.pop_value().try_into().unwrap_validated();
            let val: [u8; 16] = stack.pop_value().try_into().unwrap_validated();
            let mut lanes = to_lanes::<2, 8, u16>(val);
            lanes[lane_idx as usize] = x as u16;
            stack.push_value::<T>(Value::V128(from_lanes::<2, 8, u16>(lanes)))?;
        }
        I32X4_EXTRACT_LANE => {
            let lane_idx = wasm.read_u8().unwrap_validated();
            let val: [u8; 16] = stack.pop_value().try_into().unwrap_validated();
            let lanes = to_lanes::<4, 4, u32>(val);
            let res = lanes[lane_idx as usize];
            stack.push_value::<T>(Value::I32(res))?;
        }
        I32X4_REPLACE_LANE => {
            let lane_idx = wasm.read_u8().unwrap_validated();
            let x: u32 = stack.pop_value().try_into().unwrap_validated();
            let val: [u8; 16] = stack.pop_value().try_into().unwrap_validated();
            let mut lanes = to_lanes::<4, 4, u32>(val);
            lanes[lane_idx as usize] = x;
            stack.push_value::<T>(Value::V128(from_lanes::<4, 4, u32>(lanes)))?;
        }
        I64X2_EXTRACT_LANE => {
            let lane_idx = wasm.read_u8().unwrap_validated();
            let val: [u8; 16] = stack.pop_value().try_into().unwrap_validated();
            let lanes = to_lanes::<8, 2, u64>(val);
            let res = lanes[lane_idx as usize];
            stack.push_value::<T>(Value::I64(res))?;
        }
        I64X2_REPLACE_LANE => {
            let lane_idx = wasm.read_u8().unwrap_validated();
            let x: u64 = stack.pop_value().try_into().unwrap_validated();
            let val: [u8; 16] = stack.pop_value().try_into().unwrap_validated();
            let mut lanes = to_lanes::<8, 2, u64>(val);
            lanes[lane_idx as usize] = x;
            stack.push_value::<T>(Value::V128(from_lanes::<8, 2, u64>(lanes)))?;
        }
        F32X4_EXTRACT_LANE => {
            let lane_idx = wasm.read_u8().unwrap_validated();
            let val: [u8; 16] = stack.pop_value().try_into().unwrap_validated();
            let lanes = to_lanes::<4, 4, crate::wasm::execution::value::F32>(val);
            let res = lanes[lane_idx as usize];
            stack.push_value::<T>(Value::F32(res))?;
        }
        F32X4_REPLACE_LANE => {
            let lane_idx = wasm.read_u8().unwrap_validated();
            let x: crate::wasm::execution::value::F32 = stack.pop_value().try_into().unwrap_validated();
            let val: [u8; 16] = stack.pop_value().try_into().unwrap_validated();
            let mut lanes = to_lanes::<4, 4, crate::wasm::execution::value::F32>(val);
            lanes[lane_idx as usize] = x;
            stack.push_value::<T>(Value::V128(from_lanes::<4, 4, crate::wasm::execution::value::F32>(lanes)))?;
        }
        F64X2_EXTRACT_LANE => {
            let lane_idx = wasm.read_u8().unwrap_validated();
            let val: [u8; 16] = stack.pop_value().try_into().unwrap_validated();
            let lanes = to_lanes::<8, 2, crate::wasm::execution::value::F64>(val);
            let res = lanes[lane_idx as usize];
            stack.push_value::<T>(Value::F64(res))?;
        }
        F64X2_REPLACE_LANE => {
            let lane_idx = wasm.read_u8().unwrap_validated();
            let x: crate::wasm::execution::value::F64 = stack.pop_value().try_into().unwrap_validated();
            let val: [u8; 16] = stack.pop_value().try_into().unwrap_validated();
            let mut lanes = to_lanes::<8, 2, crate::wasm::execution::value::F64>(val);
            lanes[lane_idx as usize] = x;
            stack.push_value::<T>(Value::V128(from_lanes::<8, 2, crate::wasm::execution::value::F64>(lanes)))?;
        }
        I8X16_SPLAT => {
            let x: i32 = stack.pop_value().try_into().unwrap_validated();
            stack.push_value::<T>(Value::V128(splat([x as u8])))?;
        }
        I16X8_SPLAT => {
            let x: i32 = stack.pop_value().try_into().unwrap_validated();
            stack.push_value::<T>(Value::V128(splat((x as u16).to_le_bytes())))?;
        }
        I32X4_SPLAT => {
            let x: i32 = stack.pop_value().try_into().unwrap_validated();
            stack.push_value::<T>(Value::V128(splat((x as u32).to_le_bytes())))?;
        }
        I64X2_SPLAT => {
            let x: i64 = stack.pop_value().try_into().unwrap_validated();
            stack.push_value::<T>(Value::V128(splat((x as u64).to_le_bytes())))?;
        }
        F32X4_SPLAT => {
            let x: crate::wasm::execution::value::F32 = stack.pop_value().try_into().unwrap_validated();
            stack.push_value::<T>(Value::V128(splat(x.to_le_bytes())))?;
        }
        F64X2_SPLAT => {
            let x: crate::wasm::execution::value::F64 = stack.pop_value().try_into().unwrap_validated();
            stack.push_value::<T>(Value::V128(splat(x.to_le_bytes())))?;
        }
        I8X16_EQ => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i8x16_eq(v1, v2)))?; },
        I8X16_NE => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i8x16_ne(v1, v2)))?; },
        I8X16_LT_S => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i8x16_lt_s(v1, v2)))?; },
        I8X16_LT_U => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i8x16_lt_u(v1, v2)))?; },
        I8X16_GT_S => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i8x16_gt_s(v1, v2)))?; },
        I8X16_GT_U => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i8x16_gt_u(v1, v2)))?; },
        I8X16_LE_S => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i8x16_le_s(v1, v2)))?; },
        I8X16_LE_U => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i8x16_le_u(v1, v2)))?; },
        I8X16_GE_S => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i8x16_ge_s(v1, v2)))?; },
        I8X16_GE_U => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i8x16_ge_u(v1, v2)))?; },
        I16X8_EQ => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i16x8_eq(v1, v2)))?; },
        I16X8_NE => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i16x8_ne(v1, v2)))?; },
        I16X8_LT_S => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i16x8_lt_s(v1, v2)))?; },
        I16X8_LT_U => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i16x8_lt_u(v1, v2)))?; },
        I16X8_GT_S => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i16x8_gt_s(v1, v2)))?; },
        I16X8_GT_U => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i16x8_gt_u(v1, v2)))?; },
        I16X8_LE_S => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i16x8_le_s(v1, v2)))?; },
        I16X8_LE_U => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i16x8_le_u(v1, v2)))?; },
        I16X8_GE_S => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i16x8_ge_s(v1, v2)))?; },
        I16X8_GE_U => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i16x8_ge_u(v1, v2)))?; },
        I32X4_EQ => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i32x4_eq(v1, v2)))?; },
        I32X4_NE => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i32x4_ne(v1, v2)))?; },
        I32X4_LT_S => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i32x4_lt_s(v1, v2)))?; },
        I32X4_LT_U => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i32x4_lt_u(v1, v2)))?; },
        I32X4_GT_S => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i32x4_gt_s(v1, v2)))?; },
        I32X4_GT_U => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i32x4_gt_u(v1, v2)))?; },
        I32X4_LE_S => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i32x4_le_s(v1, v2)))?; },
        I32X4_LE_U => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i32x4_le_u(v1, v2)))?; },
        I32X4_GE_S => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i32x4_ge_s(v1, v2)))?; },
        I32X4_GE_U => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i32x4_ge_u(v1, v2)))?; },
        I64X2_EQ => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i64x2_eq(v1, v2)))?; },
        I64X2_NE => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i64x2_ne(v1, v2)))?; },
        I64X2_LT_S => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i64x2_lt_s(v1, v2)))?; },
        I64X2_GT_S => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i64x2_gt_s(v1, v2)))?; },
        I64X2_LE_S => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i64x2_le_s(v1, v2)))?; },
        I64X2_GE_S => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i64x2_ge_s(v1, v2)))?; },
        F32X4_EQ => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f32x4_eq(v1, v2)))?; },
        F32X4_NE => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f32x4_ne(v1, v2)))?; },
        F32X4_LT => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f32x4_lt(v1, v2)))?; },
        F32X4_GT => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f32x4_gt(v1, v2)))?; },
        F32X4_LE => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f32x4_le(v1, v2)))?; },
        F32X4_GE => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f32x4_ge(v1, v2)))?; },
        F64X2_EQ => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f64x2_eq(v1, v2)))?; },
        F64X2_NE => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f64x2_ne(v1, v2)))?; },
        F64X2_LT => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f64x2_lt(v1, v2)))?; },
        F64X2_GT => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f64x2_gt(v1, v2)))?; },
        F64X2_LE => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f64x2_le(v1, v2)))?; },
        F64X2_GE => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f64x2_ge(v1, v2)))?; },
        V128_NOT => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(v128_not(v1)))?; },
        V128_AND => {
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
        V128_ANY_TRUE => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::I32(if v128_any_true(v1) { 1 } else { 0 }))?; },
        I8X16_ALL_TRUE => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::I32(if i8x16_all_true(v1) { 1 } else { 0 }))?; },
        I8X16_BITMASK => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::I32(i8x16_bitmask(v1) as u32))?; },
        I16X8_ALL_TRUE => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::I32(if i16x8_all_true(v1) { 1 } else { 0 }))?; },
        I16X8_BITMASK => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::I32(i16x8_bitmask(v1) as u32))?; },
        I32X4_ALL_TRUE => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::I32(if i32x4_all_true(v1) { 1 } else { 0 }))?; },
        I32X4_BITMASK => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::I32(i32x4_bitmask(v1) as u32))?; },
        I64X2_ALL_TRUE => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::I32(if i64x2_all_true(v1) { 1 } else { 0 }))?; },
        I64X2_BITMASK => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::I32(i64x2_bitmask(v1) as u32))?; },
        I8X16_ABS => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i8x16_abs(v1)))?; },
        I8X16_NEG => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i8x16_neg(v1)))?; },
        I8X16_POPCNT => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i8x16_popcnt(v1)))?; },
        I8X16_ADD => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i8x16_add(v1, v2)))?; },
        I8X16_SUB => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i8x16_sub(v1, v2)))?; },
        I8X16_MIN_S => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i8x16_min_s(v1, v2)))?; },
        I8X16_MIN_U => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i8x16_min_u(v1, v2)))?; },
        I8X16_MAX_S => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i8x16_max_s(v1, v2)))?; },
        I8X16_MAX_U => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i8x16_max_u(v1, v2)))?; },
        I8X16_AVGR_U => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i8x16_avgr_u(v1, v2)))?; },
        I8X16_ADD_SAT_S => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i8x16_add_sat_s(v1, v2)))?; },
        I8X16_ADD_SAT_U => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i8x16_add_sat_u(v1, v2)))?; },
        I8X16_SUB_SAT_S => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i8x16_sub_sat_s(v1, v2)))?; },
        I8X16_SUB_SAT_U => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i8x16_sub_sat_u(v1, v2)))?; },
        I16X8_ABS => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i16x8_abs(v1)))?; },
        I16X8_NEG => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i16x8_neg(v1)))?; },
        I16X8_ADD => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i16x8_add(v1, v2)))?; },
        I16X8_SUB => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i16x8_sub(v1, v2)))?; },
        I16X8_MUL => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i16x8_mul(v1, v2)))?; },
        I16X8_MIN_S => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i16x8_min_s(v1, v2)))?; },
        I16X8_MIN_U => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i16x8_min_u(v1, v2)))?; },
        I16X8_MAX_S => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i16x8_max_s(v1, v2)))?; },
        I16X8_MAX_U => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i16x8_max_u(v1, v2)))?; },
        I16X8_AVGR_U => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i16x8_avgr_u(v1, v2)))?; },
        I16X8_ADD_SAT_S => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i16x8_add_sat_s(v1, v2)))?; },
        I16X8_ADD_SAT_U => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i16x8_add_sat_u(v1, v2)))?; },
        I16X8_SUB_SAT_S => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i16x8_sub_sat_s(v1, v2)))?; },
        I16X8_SUB_SAT_U => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i16x8_sub_sat_u(v1, v2)))?; },
        I32X4_ABS => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i32x4_abs(v1)))?; },
        I32X4_NEG => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i32x4_neg(v1)))?; },
        I32X4_ADD => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i32x4_add(v1, v2)))?; },
        I32X4_SUB => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i32x4_sub(v1, v2)))?; },
        I32X4_MUL => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i32x4_mul(v1, v2)))?; },
        I32X4_MIN_S => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i32x4_min_s(v1, v2)))?; },
        I32X4_MIN_U => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i32x4_min_u(v1, v2)))?; },
        I32X4_MAX_S => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i32x4_max_s(v1, v2)))?; },
        I32X4_MAX_U => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i32x4_max_u(v1, v2)))?; },
        I64X2_ABS => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i64x2_abs(v1)))?; },
        I64X2_NEG => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i64x2_neg(v1)))?; },
        I64X2_ADD => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i64x2_add(v1, v2)))?; },
        I64X2_SUB => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i64x2_sub(v1, v2)))?; },
        I64X2_MUL => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i64x2_mul(v1, v2)))?; },
        F32X4_ABS => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f32x4_abs(v1)))?; },
        F32X4_NEG => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f32x4_neg(v1)))?; },
        F32X4_SQRT => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f32x4_sqrt(v1)))?; },
        F32X4_CEIL => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f32x4_ceil(v1)))?; },
        F32X4_FLOOR => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f32x4_floor(v1)))?; },
        F32X4_TRUNC => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f32x4_trunc(v1)))?; },
        F32X4_NEAREST => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f32x4_nearest(v1)))?; },
        F32X4_ADD => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f32x4_add(v1, v2)))?; },
        F32X4_SUB => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f32x4_sub(v1, v2)))?; },
        F32X4_MUL => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f32x4_mul(v1, v2)))?; },
        F32X4_DIV => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f32x4_div(v1, v2)))?; },
        F32X4_MIN => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f32x4_min(v1, v2)))?; },
        F32X4_MAX => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f32x4_max(v1, v2)))?; },
        F32X4_PMIN => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f32x4_pmin(v1, v2)))?; },
        F32X4_PMAX => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f32x4_pmax(v1, v2)))?; },
        F64X2_ABS => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f64x2_abs(v1)))?; },
        F64X2_NEG => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f64x2_neg(v1)))?; },
        F64X2_SQRT => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f64x2_sqrt(v1)))?; },
        F64X2_CEIL => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f64x2_ceil(v1)))?; },
        F64X2_FLOOR => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f64x2_floor(v1)))?; },
        F64X2_TRUNC => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f64x2_trunc(v1)))?; },
        F64X2_NEAREST => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f64x2_nearest(v1)))?; },
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
        I8X16_NARROW_I16X8_S => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i8x16_narrow_i16x8_s(v1, v2)))?; },
        I8X16_NARROW_I16X8_U => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i8x16_narrow_i16x8_u(v1, v2)))?; },
        I16X8_NARROW_I32X4_S => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i16x8_narrow_i32x4_s(v1, v2)))?; },
        I16X8_NARROW_I32X4_U => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i16x8_narrow_i32x4_u(v1, v2)))?; },
        I16X8_EXTEND_LOW_I8X16_S => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i16x8_extend_low_i8x16_s(v1)))?; },
        I16X8_EXTEND_HIGH_I8X16_S => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i16x8_extend_high_i8x16_s(v1)))?; },
        I16X8_EXTEND_LOW_I8X16_U => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i16x8_extend_low_i8x16_u(v1)))?; },
        I16X8_EXTEND_HIGH_I8X16_U => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i16x8_extend_high_i8x16_u(v1)))?; },
        I32X4_EXTEND_LOW_I16X8_S => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i32x4_extend_low_i16x8_s(v1)))?; },
        I32X4_EXTEND_HIGH_I16X8_S => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i32x4_extend_high_i16x8_s(v1)))?; },
        I32X4_EXTEND_LOW_I16X8_U => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i32x4_extend_low_i16x8_u(v1)))?; },
        I32X4_EXTEND_HIGH_I16X8_U => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i32x4_extend_high_i16x8_u(v1)))?; },
        I64X2_EXTEND_LOW_I32X4_S => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i64x2_extend_low_i32x4_s(v1)))?; },
        I64X2_EXTEND_HIGH_I32X4_S => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i64x2_extend_high_i32x4_s(v1)))?; },
        I64X2_EXTEND_LOW_I32X4_U => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i64x2_extend_low_i32x4_u(v1)))?; },
        I64X2_EXTEND_HIGH_I32X4_U => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i64x2_extend_high_i32x4_u(v1)))?; },
        I16X8_EXTMUL_LOW_I8X16_S => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i16x8_extmul_low_i8x16_s(v1, v2)))?; },
        I16X8_EXTMUL_HIGH_I8X16_S => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i16x8_extmul_high_i8x16_s(v1, v2)))?; },
        I16X8_EXTMUL_LOW_I8X16_U => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i16x8_extmul_low_i8x16_u(v1, v2)))?; },
        I16X8_EXTMUL_HIGH_I8X16_U => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i16x8_extmul_high_i8x16_u(v1, v2)))?; },
        I32X4_EXTMUL_LOW_I16X8_S => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i32x4_extmul_low_i16x8_s(v1, v2)))?; },
        I32X4_EXTMUL_HIGH_I16X8_S => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i32x4_extmul_high_i16x8_s(v1, v2)))?; },
        I32X4_EXTMUL_LOW_I16X8_U => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i32x4_extmul_low_i16x8_u(v1, v2)))?; },
        I32X4_EXTMUL_HIGH_I16X8_U => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i32x4_extmul_high_i16x8_u(v1, v2)))?; },
        I64X2_EXTMUL_LOW_I32X4_S => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i64x2_extmul_low_i32x4_s(v1, v2)))?; },
        I64X2_EXTMUL_HIGH_I32X4_S => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i64x2_extmul_high_i32x4_s(v1, v2)))?; },
        I64X2_EXTMUL_LOW_I32X4_U => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i64x2_extmul_low_i32x4_u(v1, v2)))?; },
        I64X2_EXTMUL_HIGH_I32X4_U => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i64x2_extmul_high_i32x4_u(v1, v2)))?; },
        I16X8_EXTADD_PAIRWISE_I8X16_S => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i16x8_extadd_pairwise_i8x16_s(v1)))?; },
        I16X8_EXTADD_PAIRWISE_I8X16_U => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i16x8_extadd_pairwise_i8x16_u(v1)))?; },
        I32X4_EXTADD_PAIRWISE_I16X8_S => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i32x4_extadd_pairwise_i16x8_s(v1)))?; },
        I32X4_EXTADD_PAIRWISE_I16X8_U => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i32x4_extadd_pairwise_i16x8_u(v1)))?; },
        I32X4_DOT_I16X8_S => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i32x4_dot_i16x8_s(v1, v2)))?; },
        I16X8_Q15MULRSAT_S => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i16x8_q15mulrsat_s(v1, v2)))?; },
        I8X16_SWIZZLE => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i8x16_swizzle(v1, v2)))?; },
        I32X4_TRUNC_SAT_F32X4_S => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i32x4_trunc_sat_f32x4_s(v1)))?; },
        I32X4_TRUNC_SAT_F32X4_U => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i32x4_trunc_sat_f32x4_u(v1)))?; },
        F32X4_CONVERT_I32X4_S => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f32x4_convert_i32x4_s(v1)))?; },
        F32X4_CONVERT_I32X4_U => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f32x4_convert_i32x4_u(v1)))?; },
        I32X4_TRUNC_SAT_F64X2_S_ZERO => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i32x4_trunc_sat_f64x2_s_zero(v1)))?; },
        I32X4_TRUNC_SAT_F64X2_U_ZERO => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i32x4_trunc_sat_f64x2_u_zero(v1)))?; },
        F64X2_CONVERT_LOW_I32X4_S => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f64x2_convert_low_i32x4_s(v1)))?; },
        F64X2_CONVERT_LOW_I32X4_U => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f64x2_convert_low_i32x4_u(v1)))?; },
        I8X16_RELAXED_SWIZZLE => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i8x16_swizzle(v1, v2)))?; },
        F32X4_RELAXED_MADD | F64X2_RELAXED_MADD => {
            let c = stack.pop_value().try_into().unwrap_validated();
            let b = stack.pop_value().try_into().unwrap_validated();
            let a = stack.pop_value().try_into().unwrap_validated();
            if instr == F32X4_RELAXED_MADD {
                 stack.push_value::<T>(Value::V128(f32x4_add(f32x4_mul(a, b), c)))?;
            } else {
                 stack.push_value::<T>(Value::V128(f64x2_add(f64x2_mul(a, b), c)))?;
            }
        }
        F32X4_RELAXED_NMADD | F64X2_RELAXED_NMADD => {
            let c = stack.pop_value().try_into().unwrap_validated();
            let b = stack.pop_value().try_into().unwrap_validated();
            let a = stack.pop_value().try_into().unwrap_validated();
            if instr == F32X4_RELAXED_NMADD {
                 stack.push_value::<T>(Value::V128(f32x4_add(f32x4_neg(f32x4_mul(a, b)), c)))?;
            } else {
                 stack.push_value::<T>(Value::V128(f64x2_add(f64x2_neg(f64x2_mul(a, b)), c)))?;
            }
        }
        F32X4_RELAXED_MAX => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f32x4_max(v1, v2)))?; },
        F32X4_RELAXED_MIN => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f32x4_min(v1, v2)))?; },
        F64X2_RELAXED_MAX => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f64x2_max(v1, v2)))?; },
        F64X2_RELAXED_MIN => { let v2 = stack.pop_value().try_into().unwrap_validated(); let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(f64x2_min(v1, v2)))?; },
        I8X16_RELAXED_LANESELECT | I16X8_RELAXED_LANESELECT | I32X4_RELAXED_LANESELECT | I64X2_RELAXED_LANESELECT => {
            let c = stack.pop_value().try_into().unwrap_validated();
            let v2 = stack.pop_value().try_into().unwrap_validated();
            let v1 = stack.pop_value().try_into().unwrap_validated();
            stack.push_value::<T>(Value::V128(v128_bitselect(v1, v2, c)))?;
        }
        I32X4_RELAXED_TRUNC_F32X4_S => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i32x4_trunc_sat_f32x4_s(v1)))?; },
        I32X4_RELAXED_TRUNC_F32X4_U => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i32x4_trunc_sat_f32x4_u(v1)))?; },
        I32X4_RELAXED_TRUNC_F64X2_S_ZERO => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i32x4_trunc_sat_f64x2_s_zero(v1)))?; },
        I32X4_RELAXED_TRUNC_F64X2_U_ZERO => { let v1 = stack.pop_value().try_into().unwrap_validated(); stack.push_value::<T>(Value::V128(i32x4_trunc_sat_f64x2_u_zero(v1)))?; },
        _ => {
            crate::debugln!("WASM Trap: Unimplemented FD_EXTENSION (SIMD) opcode {:#x} at PC {:#x}", instr, wasm.pc);
            return Err(RuntimeError::Trap(TrapError::ReachedUnreachable));
        }
    }
    Ok(())
}

