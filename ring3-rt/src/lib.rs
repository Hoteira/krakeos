#![no_std]
#![no_main]
#![feature(naked_functions)]

mod context;
mod syscall;
mod traps;
mod helpers;
mod simd;
mod memory;
mod globals;
mod tables;
mod store_ops;
mod wasi_fs;
mod wasi_proc;
mod wasi_env;
mod wasi_net;
mod float_helpers;
mod krakeos;

pub use context::Ring3Context;

#[no_mangle]
#[link_section = ".jump_table"]
pub static JUMP_TABLE: [unsafe extern "C" fn(&mut Ring3Context, *mut u128) -> *mut u128; 1024] = {
    let mut table: [unsafe extern "C" fn(&mut Ring3Context, *mut u128) -> *mut u128; 1024] = [traps::trap_generic; 1024];
    
    table[0] = traps::trap_generic;
    table[1] = traps::trap_oob;
    table[2] = traps::trap_fuel;
    table[3] = traps::trap_div_zero;
    table[4] = traps::trap_int_overflow;
    table[5] = traps::trap_indirect;
    table[6] = traps::trap_unreachable;
    table[7] = traps::trap_stack_overflow;
    table[8] = traps::trap_host;
    table[9] = traps::trap_unimplemented_fc;
    table[10] = traps::trap_unimplemented_simd;
    table[11] = traps::trap_unimplemented_atomic;
    
    table[12] = helpers::i32_div_s;
    table[13] = helpers::i32_div_u;
    table[14] = helpers::i32_rem_s;
    table[15] = helpers::i32_rem_u;
    table[16] = helpers::i64_div_s;
    table[17] = helpers::i64_div_u;
    table[18] = helpers::i64_rem_s;
    table[19] = helpers::i64_rem_u;

    // Float min/max helpers (C ABI: args in XMM0/XMM1, result in XMM0)
    table[32] = unsafe { core::mem::transmute(float_helpers::f32_min as *const ()) };  // F32Min = 32
    table[33] = unsafe { core::mem::transmute(float_helpers::f32_max as *const ()) };  // F32Max = 33
    table[34] = unsafe { core::mem::transmute(float_helpers::f64_min as *const ()) };  // F64Min = 34
    table[35] = unsafe { core::mem::transmute(float_helpers::f64_max as *const ()) };  // F64Max = 35

    // Saturating truncation helpers (C ABI: arg in XMM0, result in RAX)
    table[42] = unsafe { core::mem::transmute(float_helpers::i32_trunc_sat_f32_s as *const ()) };  // I32TruncSatF32S = 42
    table[43] = unsafe { core::mem::transmute(float_helpers::i32_trunc_sat_f32_u as *const ()) };  // I32TruncSatF32U = 43
    table[44] = unsafe { core::mem::transmute(float_helpers::i32_trunc_sat_f64_s as *const ()) };  // I32TruncSatF64S = 44
    table[45] = unsafe { core::mem::transmute(float_helpers::i32_trunc_sat_f64_u as *const ()) };  // I32TruncSatF64U = 45
    table[46] = unsafe { core::mem::transmute(float_helpers::i64_trunc_sat_f32_s as *const ()) };  // I64TruncSatF32S = 46
    table[47] = unsafe { core::mem::transmute(float_helpers::i64_trunc_sat_f32_u as *const ()) };  // I64TruncSatF32U = 47
    table[48] = unsafe { core::mem::transmute(float_helpers::i64_trunc_sat_f64_s as *const ()) };  // I64TruncSatF64S = 48
    table[49] = unsafe { core::mem::transmute(float_helpers::i64_trunc_sat_f64_u as *const ()) };  // I64TruncSatF64U = 49

    // Store-dependent bulk ops — forwarded to kernel via syscall (C ABI)
    table[50] = unsafe { core::mem::transmute(store_ops::ring3_memory_init as *const ()) };   // MemoryInit = 50
    table[51] = unsafe { core::mem::transmute(store_ops::ring3_data_drop as *const ()) };     // DataDrop = 51
    // Pure memory ops — handled directly in ring3 (C ABI)
    table[52] = unsafe { core::mem::transmute(memory::memory_copy as *const ()) };   // MemoryCopy = 52
    table[53] = unsafe { core::mem::transmute(memory::memory_fill as *const ()) };   // MemoryFill = 53
    // Table ops — forwarded to kernel via syscall (C ABI)
    table[54] = unsafe { core::mem::transmute(store_ops::ring3_table_init as *const ()) };    // TableInit = 54
    table[55] = unsafe { core::mem::transmute(store_ops::ring3_elem_drop as *const ()) };     // ElemDrop = 55
    table[56] = unsafe { core::mem::transmute(store_ops::ring3_table_copy as *const ()) };    // TableCopy = 56
    table[57] = unsafe { core::mem::transmute(store_ops::ring3_table_grow as *const ()) };    // TableGrow = 57
    table[58] = unsafe { core::mem::transmute(store_ops::ring3_table_size as *const ()) };    // TableSize = 58
    table[59] = unsafe { core::mem::transmute(store_ops::ring3_table_fill as *const ()) };    // TableFill = 59
    // sp-convention ops
    table[60] = memory::memory_size;   // MemorySize = 60
    table[61] = memory::memory_grow;   // MemoryGrow = 61

    // Call dispatch (must match AotTrampoline enum indices)
    table[66] = unsafe { core::mem::transmute(tables::call_indirect as *const ()) };  // CallIndirect = 66
    table[67] = unsafe { core::mem::transmute(wasi_fs::call_host_dispatch as *const ()) };  // CallHost = 67

    // WASI stubs (accessed via call_host_dispatch, indices match import_stub_table in store/mod.rs)
    table[90] = wasi_fs::wasi_fd_write;
    table[91] = wasi_fs::wasi_fd_read;
    table[92] = wasi_fs::wasi_fd_close;
    table[999] = wasi_fs::wasi_serial_print;
    table[1023] = unsafe { core::mem::transmute(traps::process_exit as *const ()) };

    table
};

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    unsafe { syscall::syscall1(60, 99); } // SYS_EXIT(99)
    loop {}
}

#[no_mangle]
pub extern "C" fn _blob_start() {
    // Placeholder entry point
}
