use crate::context::Ring3Context;
use crate::syscall::{syscall2, syscall3, syscall5, syscall6};

const SYS_WASM_MEMORY_INIT: u64 = 350;
const SYS_WASM_DATA_DROP: u64 = 351;
const SYS_WASM_TABLE_INIT: u64 = 352;
const SYS_WASM_ELEM_DROP: u64 = 353;
const SYS_WASM_TABLE_COPY: u64 = 354;
const SYS_WASM_TABLE_GROW: u64 = 355;
const SYS_WASM_TABLE_SIZE: u64 = 356;
const SYS_WASM_TABLE_FILL: u64 = 357;

#[no_mangle]
pub extern "C" fn ring3_memory_init(ctx: &Ring3Context, d: i32, s: i32, n: u32, data_idx: u32) {
    unsafe {
        syscall5(SYS_WASM_MEMORY_INIT, ctx as *const _ as u64, d as u64, s as u64, n as u64, data_idx as u64);
    }
}

#[no_mangle]
pub extern "C" fn ring3_data_drop(ctx: &Ring3Context, data_idx: u32) {
    unsafe {
        syscall2(SYS_WASM_DATA_DROP, ctx as *const _ as u64, data_idx as u64);
    }
}

#[no_mangle]
pub extern "C" fn ring3_table_init(ctx: &Ring3Context, d: i32, s: i32, n: u32, table_idx: u32, elem_idx: u32) {
    unsafe {
        syscall6(SYS_WASM_TABLE_INIT, ctx as *const _ as u64, d as u64, s as u64, n as u64, table_idx as u64, elem_idx as u64);
    }
}

#[no_mangle]
pub extern "C" fn ring3_elem_drop(ctx: &Ring3Context, elem_idx: u32) {
    unsafe {
        syscall2(SYS_WASM_ELEM_DROP, ctx as *const _ as u64, elem_idx as u64);
    }
}

#[no_mangle]
pub extern "C" fn ring3_table_copy(ctx: &Ring3Context, d: i32, s: i32, n: u32, table_dst: u32, table_src: u32) {
    unsafe {
        syscall6(SYS_WASM_TABLE_COPY, ctx as *const _ as u64, d as u64, s as u64, n as u64, table_dst as u64, table_src as u64);
    }
}

#[no_mangle]
pub extern "C" fn ring3_table_grow(ctx: &Ring3Context, val: usize, n: u32, table_idx: u32) -> i32 {
    unsafe {
        syscall5(SYS_WASM_TABLE_GROW, ctx as *const _ as u64, val as u64, n as u64, table_idx as u64, 0) as i32
    }
}

#[no_mangle]
pub extern "C" fn ring3_table_size(ctx: &Ring3Context, table_idx: u32) -> u32 {
    unsafe {
        syscall2(SYS_WASM_TABLE_SIZE, ctx as *const _ as u64, table_idx as u64) as u32
    }
}

#[no_mangle]
pub extern "C" fn ring3_table_fill(ctx: &Ring3Context, d: i32, val: usize, n: u32, table_idx: u32) {
    unsafe {
        syscall5(SYS_WASM_TABLE_FILL, ctx as *const _ as u64, d as u64, val as u64, n as u64, table_idx as u64);
    }
}
