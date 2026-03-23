use crate::alloc::format;
use crate::alloc::vec::Vec;
use crate::math::FloatMath;
use crate::os::debug_print;
use crate::wasm::aot::runtime::Ring3Context;
use crate::wasm::common::runtime_error::RuntimeError;
use crate::wasm::common::value::{F32, F64, Ref, Value};
use crate::wasm::interpreter::resumable::RunState;
use crate::wasm::interpreter::simd_utils;
use crate::wasm::interpreter::store::Store;
use crate::wasm::interpreter::store::instances::FuncInst;

#[unsafe(no_mangle)]
pub extern "C" fn aot_trap() -> ! {
    crate::os::debug_print("AOT Trap: Generic\n");
    crate::os::exit(1);
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_trap_oob() -> ! {
    crate::os::debug_print("AOT Trap: Memory Out of Bounds\n");
    crate::os::exit(1);
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_trap_fuel() -> ! {
    crate::os::debug_print("AOT Trap: Fuel Exhausted\n");
    crate::os::exit(1);
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_trap_div_zero() -> ! {
    crate::os::debug_print("AOT Trap: Integer Divide by Zero\n");
    crate::os::exit(1);
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_trap_int_overflow() -> ! {
    crate::os::debug_print("AOT Trap: Integer Overflow\n");
    crate::os::exit(1);
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_trap_indirect() -> ! {
    crate::os::debug_print("AOT Trap: Indirect Call Signature Mismatch or Null\n");
    crate::os::exit(1);
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_trap_unreachable() -> ! {
    crate::os::debug_print(
        "AOT Trap: WASM Reached Unreachable Instruction. (Check for earlier [USER PANIC] messages!)\n",
    );
    crate::os::exit(1);
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_trap_stack_overflow() -> ! {
    crate::os::debug_print("AOT Trap: Stack Overflow\n");
    crate::os::exit(1);
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_trap_host(ctx: &Ring3Context) -> ! {
    let code = unsafe { *ctx.trap_code };
    if code == -1 {
        crate::os::exit(0);
    }
    crate::os::debug_print("AOT Trap: Host Function Error\n");
    crate::os::exit(code as u64);
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_trap_unimplemented_fc() -> ! {
    crate::os::debug_print("AOT Trap: Unimplemented FC Extension Instruction\n");
    crate::os::exit(1);
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_trap_unimplemented_simd() -> ! {
    crate::os::debug_print("AOT Trap: Unimplemented SIMD Instruction\n");
    crate::os::exit(1);
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_trap_unimplemented_atomic() -> ! {
    crate::os::debug_print("AOT Trap: Unimplemented Atomic Instruction\n");
    crate::os::exit(1);
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_i32_div_s(a: i32, b: i32) -> i32 {
    if b == 0 {
        unsafe {
            aot_trap_div_zero();
        }
    }
    if a == i32::MIN && b == -1 {
        unsafe {
            aot_trap_int_overflow();
        }
    }
    a / b
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_i32_div_u(a: u32, b: u32) -> u32 {
    if b == 0 {
        unsafe {
            aot_trap_div_zero();
        }
    }
    a / b
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_i32_rem_s(a: i32, b: i32) -> i32 {
    if b == 0 {
        unsafe {
            aot_trap_div_zero();
        }
    }
    a.checked_rem(b).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_i32_rem_u(a: u32, b: u32) -> u32 {
    if b == 0 {
        unsafe {
            aot_trap_div_zero();
        }
    }
    a % b
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_i64_div_s(a: i64, b: i64) -> i64 {
    if b == 0 {
        unsafe {
            aot_trap_div_zero();
        }
    }
    if a == i64::MIN && b == -1 {
        unsafe {
            aot_trap_int_overflow();
        }
    }
    a / b
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_i64_div_u(a: u64, b: u64) -> u64 {
    if b == 0 {
        unsafe {
            aot_trap_div_zero();
        }
    }
    a / b
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_i64_rem_s(a: i64, b: i64) -> i64 {
    if b == 0 {
        unsafe {
            aot_trap_div_zero();
        }
    }
    a.checked_rem(b).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_i64_rem_u(a: u64, b: u64) -> u64 {
    if b == 0 {
        unsafe {
            aot_trap_div_zero();
        }
    }
    a % b
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_i32_clz(a: u32) -> u32 {
    a.leading_zeros()
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_i32_ctz(a: u32) -> u32 {
    a.trailing_zeros()
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_i32_popcnt(a: u32) -> u32 {
    a.count_ones()
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_i64_clz(a: u64) -> u64 {
    a.leading_zeros() as u64
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_i64_ctz(a: u64) -> u64 {
    a.trailing_zeros() as u64
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_i64_popcnt(a: u64) -> u64 {
    a.count_ones() as u64
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_f32_eq(a: f32, b: f32) -> i32 {
    if a == b { 1 } else { 0 }
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_f32_ne(a: f32, b: f32) -> i32 {
    if a != b { 1 } else { 0 }
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_f32_lt(a: f32, b: f32) -> i32 {
    if a < b { 1 } else { 0 }
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_f32_gt(a: f32, b: f32) -> i32 {
    if a > b { 1 } else { 0 }
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_f32_le(a: f32, b: f32) -> i32 {
    if a <= b { 1 } else { 0 }
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_f32_ge(a: f32, b: f32) -> i32 {
    if a >= b { 1 } else { 0 }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_f64_eq(a: f64, b: f64) -> i32 {
    if a == b { 1 } else { 0 }
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_f64_ne(a: f64, b: f64) -> i32 {
    if a != b { 1 } else { 0 }
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_f64_lt(a: f64, b: f64) -> i32 {
    if a < b { 1 } else { 0 }
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_f64_gt(a: f64, b: f64) -> i32 {
    if a > b { 1 } else { 0 }
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_f64_le(a: f64, b: f64) -> i32 {
    if a <= b { 1 } else { 0 }
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_f64_ge(a: f64, b: f64) -> i32 {
    if a >= b { 1 } else { 0 }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_f32_abs(a: f32) -> f32 {
    a.abs()
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_f32_neg(a: f32) -> f32 {
    -a
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_f32_sqrt(a: f32) -> f32 {
    a.sqrt()
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_f32_ceil(a: f32) -> f32 {
    a.ceil()
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_f32_floor(a: f32) -> f32 {
    a.floor()
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_f32_trunc(a: f32) -> f32 {
    a.trunc()
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_f32_nearest(a: f32) -> f32 {
    F32(a).nearest().0
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_f64_abs(a: f64) -> f64 {
    a.abs()
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_f64_neg(a: f64) -> f64 {
    -a
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_f64_sqrt(a: f64) -> f64 {
    a.sqrt()
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_f64_ceil(a: f64) -> f64 {
    a.ceil()
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_f64_floor(a: f64) -> f64 {
    a.floor()
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_f64_trunc(a: f64) -> f64 {
    a.trunc()
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_f64_nearest(a: f64) -> f64 {
    F64(a).nearest().0
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_f32_min(a: f32, b: f32) -> f32 {
    let bits_a = a.to_bits();
    let bits_b = b.to_bits();
    if (bits_a & 0x7FFFFFFF) > 0x7F800000 {
        return a;
    }
    if (bits_b & 0x7FFFFFFF) > 0x7F800000 {
        return b;
    }
    if a < b {
        return a;
    }
    if b < a {
        return b;
    }
    f32::from_bits(bits_a | bits_b)
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_f32_max(a: f32, b: f32) -> f32 {
    let bits_a = a.to_bits();
    let bits_b = b.to_bits();
    if (bits_a & 0x7FFFFFFF) > 0x7F800000 {
        return a;
    }
    if (bits_b & 0x7FFFFFFF) > 0x7F800000 {
        return b;
    }
    if a > b {
        return a;
    }
    if b > a {
        return b;
    }
    f32::from_bits(bits_a & bits_b)
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_f32_copysign(a: f32, b: f32) -> f32 {
    a.copysign(b)
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_f64_min(a: f64, b: f64) -> f64 {
    let bits_a = a.to_bits();
    let bits_b = b.to_bits();
    if (bits_a & 0x7FFFFFFFFFFFFFFF) > 0x7FF0000000000000 {
        return a;
    }
    if (bits_b & 0x7FFFFFFFFFFFFFFF) > 0x7FF0000000000000 {
        return b;
    }
    if a < b {
        return a;
    }
    if b < a {
        return b;
    }
    f64::from_bits(bits_a | bits_b)
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_f64_max(a: f64, b: f64) -> f64 {
    let bits_a = a.to_bits();
    let bits_b = b.to_bits();
    if (bits_a & 0x7FFFFFFFFFFFFFFF) > 0x7FF0000000000000 {
        return a;
    }
    if (bits_b & 0x7FFFFFFFFFFFFFFF) > 0x7FF0000000000000 {
        return b;
    }
    if a > b {
        return a;
    }
    if b > a {
        return b;
    }
    f64::from_bits(bits_a & bits_b)
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_f64_copysign(a: f64, b: f64) -> f64 {
    a.copysign(b)
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_f32_convert_i64_u(a: u64) -> f32 {
    a as f32
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_f64_convert_i64_u(a: u64) -> f64 {
    a as f64
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_i32_trunc_f32_u(a: f32) -> u32 {
    a as u32
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_i32_trunc_f64_u(a: f64) -> u32 {
    a as u32
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_i64_trunc_f32_u(a: f32) -> u64 {
    a as u64
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_i64_trunc_f64_u(a: f64) -> u64 {
    a as u64
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_i32_trunc_sat_f32_s(a: f32) -> i32 {
    if a.is_nan() {
        0
    } else if a >= 2147483648.0 {
        i32::MAX
    } else if a < -2147483648.0 {
        i32::MIN
    } else {
        a as i32
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_i32_trunc_sat_f32_u(a: f32) -> u32 {
    if a.is_nan() {
        0
    } else if a >= 4294967296.0 {
        u32::MAX
    } else if a <= -1.0 {
        0
    } else {
        a as u32
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_i32_trunc_sat_f64_s(a: f64) -> i32 {
    if a.is_nan() {
        0
    } else if a >= 2147483648.0 {
        i32::MAX
    } else if a < -2147483648.0 {
        i32::MIN
    } else {
        a as i32
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_i32_trunc_sat_f64_u(a: f64) -> u32 {
    if a.is_nan() {
        0
    } else if a >= 4294967296.0 {
        u32::MAX
    } else if a <= -1.0 {
        0
    } else {
        a as u32
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_i64_trunc_sat_f32_s(a: f32) -> i64 {
    if a.is_nan() {
        0
    } else if a >= 9223372036854775808.0 {
        i64::MAX
    } else if a < -9223372036854775808.0 {
        i64::MIN
    } else {
        a as i64
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_i64_trunc_sat_f32_u(a: f32) -> u64 {
    if a.is_nan() {
        0
    } else if a >= 18446744073709551616.0 {
        u64::MAX
    } else if a <= -1.0 {
        0
    } else {
        a as u64
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_i64_trunc_sat_f64_s(a: f64) -> i64 {
    if a.is_nan() {
        0
    } else if a >= 9223372036854775808.0 {
        i64::MAX
    } else if a < -9223372036854775808.0 {
        i64::MIN
    } else {
        a as i64
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_i64_trunc_sat_f64_u(a: f64) -> u64 {
    if a.is_nan() {
        0
    } else if a >= 18446744073709551616.0 {
        u64::MAX
    } else if a <= -1.0 {
        0
    } else {
        a as u64
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_memory_init(ctx: &Ring3Context, d: i32, s: i32, n: u32, data_idx: u32) {
    let store = unsafe { &mut *(ctx.store as *mut Store<()>) };
    let module_addr = ctx.module_addr;
    crate::wasm::interpreter::loop_executor::memory_init(
        &store.modules,
        &mut store.memories,
        &store.data,
        module_addr,
        data_idx as usize,
        0,
        n,
        s,
        d,
    )
    .unwrap();
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_data_drop(ctx: &Ring3Context, data_idx: u32) {
    let store = unsafe { &mut *(ctx.store as *mut Store<()>) };
    let module_addr = ctx.module_addr;
    crate::wasm::interpreter::loop_executor::data_drop(
        &store.modules,
        &mut store.data,
        module_addr,
        data_idx as usize,
    )
    .unwrap();
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_memory_copy(ctx: &Ring3Context, d: i32, s: i32, n: u32) {
    let store = unsafe { &mut *(ctx.store as *mut Store<()>) };
    let module_addr = ctx.module_addr;
    let mem_addr = store.modules.get(module_addr).mem_addrs[0];
    let mem = store.memories.get(mem_addr);
    mem.mem
        .copy(d as usize, &mem.mem, s as usize, n as usize)
        .unwrap();
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_memory_fill(ctx: &Ring3Context, d: i32, val: u32, n: u32) {
    let store = unsafe { &mut *(ctx.store as *mut Store<()>) };
    let module_addr = ctx.module_addr;
    let mem_addr = store.modules.get(module_addr).mem_addrs[0];
    let mem = store.memories.get(mem_addr);
    mem.mem.fill(d as usize, val as u8, n as usize).unwrap();
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_table_init(
    ctx: &Ring3Context,
    d: i32,
    s: i32,
    n: u32,
    table_idx: u32,
    elem_idx: u32,
) {
    let store = unsafe { &mut *(ctx.store as *mut Store<()>) };
    let module_addr = ctx.module_addr;
    crate::wasm::interpreter::loop_executor::table_init(
        &store.modules,
        &mut store.tables,
        &store.elements,
        module_addr,
        elem_idx as usize,
        table_idx as usize,
        n,
        s,
        d,
    )
    .unwrap();
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_elem_drop(ctx: &Ring3Context, elem_idx: u32) {
    let store = unsafe { &mut *(ctx.store as *mut Store<()>) };
    let module_addr = ctx.module_addr;
    crate::wasm::interpreter::loop_executor::elem_drop(
        &store.modules,
        &mut store.elements,
        module_addr,
        elem_idx as usize,
    )
    .unwrap_or_else(|_| unsafe {
        aot_trap();
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_table_copy(
    ctx: &Ring3Context,
    d: i32,
    s: i32,
    n: u32,
    table_dst: u32,
    table_src: u32,
) {
    let store = unsafe { &mut *(ctx.store as *mut Store<()>) };
    let module_addr = ctx.module_addr;
    let tx_addr = store.modules.get(module_addr).table_addrs[table_src as usize];
    let ty_addr = store.modules.get(module_addr).table_addrs[table_dst as usize];

    if tx_addr == ty_addr {
        let t = store.tables.get_mut(tx_addr);
        let d = d as usize;
        let s = s as usize;
        if d <= s {
            for i in 0..n as usize {
                t.elem[d + i] = t.elem[s + i];
            }
        } else {
            for i in (0..n as usize).rev() {
                t.elem[d + i] = t.elem[s + i];
            }
        }
    } else {
        let (tx, ty) = store.tables.get_two_mut(tx_addr, ty_addr).unwrap();
        for i in 0..n as usize {
            ty.elem[d as usize + i] = tx.elem[s as usize + i];
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_table_grow(ctx: &Ring3Context, val: usize, n: u32, table_idx: u32) -> i32 {
    let store = unsafe { &mut *(ctx.store as *mut Store<()>) };
    let module_addr = ctx.module_addr;
    let t_addr = store.modules.get(module_addr).table_addrs[table_idx as usize];
    let t = store.tables.get_mut(t_addr);
    let sz = t.elem.len() as i32;
    t.elem
        .extend(core::iter::repeat(Ref::Func(val)).take(n as usize));
    sz
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_table_size(ctx: &Ring3Context, table_idx: u32) -> u32 {
    let store = unsafe { &*(ctx.store as *const Store<()>) };
    let module_addr = ctx.module_addr;
    let t_addr = store.modules.get(module_addr).table_addrs[table_idx as usize];
    store.tables.get(t_addr).elem.len() as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_table_fill(ctx: &Ring3Context, d: i32, val: usize, n: u32, table_idx: u32) {
    let store = unsafe { &mut *(ctx.store as *mut Store<()>) };
    let module_addr = ctx.module_addr;
    let t_addr = store.modules.get(module_addr).table_addrs[table_idx as usize];
    let t = store.tables.get_mut(t_addr);
    for i in 0..n as usize {
        t.elem[d as usize + i] = Ref::Func(val);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_memory_size(ctx: &Ring3Context) -> u32 {
    let store = unsafe { &*(ctx.store as *const Store<()>) };
    let module_addr = ctx.module_addr;
    let mem_addr = store.modules.get(module_addr).mem_addrs[0];
    store.memories.get(mem_addr).size() as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_memory_grow(ctx: &mut Ring3Context, n: u32) -> u32 {
    let store = unsafe { &mut *(ctx.store as *mut Store<()>) };
    let module_addr = ctx.module_addr;
    let mem_addr = store.modules.get(module_addr).mem_addrs[0];
    let old_size = store.memories.get(mem_addr).size() as u32;
    match store.memories.get_mut(mem_addr).grow(n) {
        Ok(_) => {
            let mem = &store.memories.get(mem_addr).mem;
            ctx.memory_base = mem.get_base_ptr();
            ctx.memory_size = mem.len();
            old_size
        }
        Err(_) => u32::MAX,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_global_get(ctx: &Ring3Context, idx: u32, out: *mut [u8; 16]) {
    let store = unsafe { &*(ctx.store as *const Store<()>) };
    let module_addr = ctx.module_addr;
    let global_addr = store.modules.get(module_addr).global_addrs[idx as usize];
    let val = store.globals.get(global_addr).value;
    unsafe {
        *(out as *mut u128) = val.to_u128();
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_global_set(ctx: &Ring3Context, idx: u32, data: *const [u8; 16]) {
    let store = unsafe { &mut *(ctx.store as *mut Store<()>) };
    let module_addr = ctx.module_addr;
    let global_addr = store.modules.get(module_addr).global_addrs[idx as usize];
    let global = store.globals.get_mut(global_addr);
    let ty = global.ty.ty;
    unsafe {
        global.value = Value::from_u128(*(data as *const u128), ty);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_table_get(ctx: &Ring3Context, table_idx: u32, i: u32) -> usize {
    let store = unsafe { &*(ctx.store as *const Store<()>) };
    let module_addr = ctx.module_addr;
    let table_addr = store.modules.get(module_addr).table_addrs[table_idx as usize];
    let tab = store.tables.get(table_addr);
    match tab.elem.get(i as usize).unwrap_or(&Ref::Null(
        crate::wasm::common::reader::types::RefType::FuncRef,
    )) {
        Ref::Func(addr) => *addr,
        _ => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_table_set(ctx: &Ring3Context, table_idx: u32, i: u32, val: usize) {
    let store = unsafe { &mut *(ctx.store as *mut Store<()>) };
    let module_addr = ctx.module_addr;
    let table_addr = store.modules.get(module_addr).table_addrs[table_idx as usize];
    let tab = store.tables.get_mut(table_addr);
    if let Some(slot) = tab.elem.get_mut(i as usize) {
        *slot = Ref::Func(val);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_call_indirect(
    ctx: &Ring3Context,
    table_idx: u32,
    type_idx: u32,
    i: u32,
) -> *const u8 {
    let store = unsafe { &mut *(ctx.store as *mut Store<()>) };
    let module_addr = ctx.module_addr;
    let table_addr = store.modules.get(module_addr).table_addrs[table_idx as usize];
    let tab = store.tables.get(table_addr);

    let r = tab
        .elem
        .get(i as usize)
        .unwrap_or_else(|| unsafe { aot_trap_oob() });
    let func_addr = match r {
        Ref::Func(addr) => *addr,
        _ => unsafe {
            crate::os::debug_print(&crate::alloc::format!(
                "AOT Trap: Table element {} is not a Ref::Func\n",
                i
            ));
            aot_trap_indirect()
        },
    };

    let func_inst = store.functions.get(func_addr);
    let expected_ty = &store.modules.get(module_addr).types[type_idx as usize];
    if func_inst.ty() != *expected_ty {
        unsafe {
            crate::os::debug_print(&crate::alloc::format!(
                "AOT Trap: Signature Mismatch. Expected {:?}, Got {:?}\n",
                expected_ty,
                func_inst.ty()
            ));
            aot_trap_indirect();
        }
    }

    match func_inst {
        FuncInst::WasmFunc(wasm_func) => {
            let ptr = wasm_func
                .aot_ptr
                .map(|p| p as *const u8)
                .unwrap_or_else(|| unsafe {
                    crate::os::debug_print("AOT Trap: aot_ptr is null\n");
                    aot_trap_indirect()
                });
            // Validate pointer is within a reasonable code range
            let code_base = store.code_base.unwrap_or(0);
            if code_base != 0 {
                let p = ptr as usize;
                // Allow up to 512MB from code_base to account for large binaries
                if p < code_base as usize || p >= code_base as usize + 512 * 1024 * 1024 {
                    crate::os::debug_print(&crate::alloc::format!(
                        "[aot_call_indirect] INVALID PTR={:#x} code_base={:#x} func_addr={} table={} i={}\n",
                        p,
                        code_base,
                        func_addr,
                        table_idx,
                        i
                    ));
                    unsafe { aot_trap_indirect() }
                }
            }
            ptr
        }
        _ => unsafe {
            crate::os::debug_print("AOT Trap: not a WasmFunc\n");
            aot_trap_indirect()
        },
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_call_host(ctx: &mut Ring3Context, func_idx: u32, sp: *mut u128) -> *mut u128 {
    if ctx.store.is_null() {
        unsafe {
            aot_trap();
        }
    }
    let store = unsafe { &mut *(ctx.store as *mut Store<()>) };
    let module_addr = ctx.module_addr;
    let func_addr = store.modules.get(module_addr).func_addrs[func_idx as usize];
    let func_inst = store.functions.get(func_addr);
    let ty = func_inst.ty();
    let n_params = ty.params.valtypes.len();

    let mut params = Vec::with_capacity(n_params);

    // Safety check: Ensure sp is not null and points to valid stack
    if sp.is_null() {
        unsafe {
            aot_trap();
        }
    }

    unsafe {
        for i in 0..n_params {
            let val_ptr = sp.add(n_params - 1 - i);
            let val_type = ty.params.valtypes[i];
            let val = Value::from_u128(*val_ptr, val_type);
            params.push(val);
        }
    }

    store.caller_module = Some(module_addr);
    let results = match func_inst {
        FuncInst::HostFunc(h) => {
            let hostcode = h.hostcode;
            match hostcode(store, params.clone()) {
                Ok(res) => res,
                Err(crate::wasm::interpreter::store::HaltExecutionError(code)) => {
                    unsafe {
                        *ctx.trap_code = if code == 0 { -1 } else { code };
                    }
                    return sp;
                }
            }
        }
        FuncInst::WasmFunc(_) => {
            let fuel = unsafe { *ctx.fuel };
            let run_state = store.invoke_unchecked(func_addr, params, Some(fuel));
            match run_state {
                Ok(RunState::Finished {
                    values,
                    maybe_remaining_fuel,
                }) => {
                    if let Some(rem) = maybe_remaining_fuel {
                        unsafe {
                            *ctx.fuel = rem;
                        }
                    }
                    values
                }
                Err(RuntimeError::HostFunctionHaltedExecution(code)) => {
                    unsafe {
                        *ctx.trap_code = if code == 0 { -1 } else { code };
                    }
                    return sp;
                }
                _ => unsafe { aot_trap_host(ctx) },
            }
        }
    };
    store.caller_module = None;

    // Sync memory state between ctx and Store.
    if let Some(&mem_addr) = store.modules.get(module_addr).mem_addrs.first() {
        let store_size = store.memories.get(mem_addr).mem.len();
        if ctx.memory_size > store_size {
            let new_pages = (ctx.memory_size / 65536) as u32;
            store.memories.get_mut(mem_addr).mem.set_pages(new_pages);
        } else if store_size > ctx.memory_size {
            ctx.memory_base = store.memories.get(mem_addr).mem.get_base_ptr();
            ctx.memory_size = store_size;
        }
    }

    let n_results = results.len();
    let mut current_sp = unsafe { sp.add(n_params) };
    for res in results {
        current_sp = unsafe { current_sp.sub(1) };
        unsafe {
            *current_sp = res.to_u128();
        }
    }

    /*crate::os::debug_print(&format!(
        "AOT: host idx={} n_params={} n_results={} sp={:p} returned_sp={:p}\n",
        func_idx, n_params, n_results, sp, current_sp
    ));*/

    current_sp
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_ref_func(ctx: &Ring3Context, func_idx: u32) -> usize {
    let store = unsafe { &*(ctx.store as *const Store<()>) };
    let module_addr = ctx.module_addr;
    store.modules.get(module_addr).func_addrs[func_idx as usize]
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_v128_load_lane(
    ctx: &Ring3Context,
    addr: u32,
    offset: u32,
    lane_idx: u8,
    size: u32,
    val: *mut [u8; 16],
) {
    let store = unsafe { &mut *(ctx.store as *mut Store<()>) };
    let module_addr = ctx.module_addr;
    let mem_addr = store.modules.get(module_addr).mem_addrs[0];
    let mem = store.memories.get(mem_addr);
    let idx = (addr as u64 + offset as u64) as usize;
    let val_mut = unsafe { &mut *val };
    match size {
        1 => {
            val_mut[lane_idx as usize] = mem.mem.load_bytes::<1>(idx).unwrap()[0];
        }
        2 => {
            val_mut[lane_idx as usize * 2..(lane_idx as usize + 1) * 2]
                .copy_from_slice(&mem.mem.load_bytes::<2>(idx).unwrap());
        }
        4 => {
            val_mut[lane_idx as usize * 4..(lane_idx as usize + 1) * 4]
                .copy_from_slice(&mem.mem.load_bytes::<4>(idx).unwrap());
        }
        8 => {
            val_mut[lane_idx as usize * 8..(lane_idx as usize + 1) * 8]
                .copy_from_slice(&mem.mem.load_bytes::<8>(idx).unwrap());
        }
        _ => unreachable!(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_v128_store_lane(
    ctx: &Ring3Context,
    addr: u32,
    offset: u32,
    lane_idx: u8,
    size: u32,
    val: *const [u8; 16],
) {
    let store = unsafe { &mut *(ctx.store as *mut Store<()>) };
    let module_addr = ctx.module_addr;
    let mem_addr = store.modules.get(module_addr).mem_addrs[0];
    let mem = store.memories.get_mut(mem_addr);
    let idx = (addr as u64 + offset as u64) as usize;
    let val_ref = unsafe { &*val };
    match size {
        1 => {
            mem.mem
                .store_bytes::<1>(idx, [val_ref[lane_idx as usize]])
                .unwrap();
        }
        2 => {
            let mut b = [0u8; 2];
            b.copy_from_slice(&val_ref[lane_idx as usize * 2..(lane_idx as usize + 1) * 2]);
            mem.mem.store_bytes::<2>(idx, b).unwrap();
        }
        4 => {
            let mut b = [0u8; 4];
            b.copy_from_slice(&val_ref[lane_idx as usize * 4..(lane_idx as usize + 1) * 4]);
            mem.mem.store_bytes::<4>(idx, b).unwrap();
        }
        8 => {
            let mut b = [0u8; 8];
            b.copy_from_slice(&val_ref[lane_idx as usize * 8..(lane_idx as usize + 1) * 8]);
            mem.mem.store_bytes::<8>(idx, b).unwrap();
        }
        _ => unreachable!(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_v128_and(a: *mut [u8; 16], b: *const [u8; 16]) {
    unsafe {
        *a = simd_utils::v128_and(*a, *b);
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_v128_or(a: *mut [u8; 16], b: *const [u8; 16]) {
    unsafe {
        *a = simd_utils::v128_or(*a, *b);
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_v128_xor(a: *mut [u8; 16], b: *const [u8; 16]) {
    unsafe {
        *a = simd_utils::v128_xor(*a, *b);
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_v128_bitselect(a: *mut [u8; 16], b: *const [u8; 16], c: *const [u8; 16]) {
    unsafe {
        *a = simd_utils::v128_bitselect(*a, *b, *c);
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_v128_eq_i8x16(a: *mut [u8; 16], b: *const [u8; 16]) {
    unsafe {
        *a = simd_utils::i8x16_eq(*a, *b);
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_v128_eq_i16x8(a: *mut [u8; 16], b: *const [u8; 16]) {
    unsafe {
        *a = simd_utils::i16x8_eq(*a, *b);
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_v128_eq_i32x4(a: *mut [u8; 16], b: *const [u8; 16]) {
    unsafe {
        *a = simd_utils::i32x4_eq(*a, *b);
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_v128_eq_i64x2(a: *mut [u8; 16], b: *const [u8; 16]) {
    unsafe {
        *a = simd_utils::i64x2_eq(*a, *b);
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_v128_eq_f32x4(a: *mut [u8; 16], b: *const [u8; 16]) {
    unsafe {
        *a = simd_utils::f32x4_eq(*a, *b);
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_v128_eq_f64x2(a: *mut [u8; 16], b: *const [u8; 16]) {
    unsafe {
        *a = simd_utils::f64x2_eq(*a, *b);
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_v128_any_true(a: *const [u8; 16]) -> i32 {
    if unsafe { simd_utils::v128_any_true(*a) } {
        1
    } else {
        0
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_v128_bitmask_i8x16(a: *const [u8; 16]) -> i32 {
    unsafe { simd_utils::i8x16_bitmask(*a) }
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_v128_i8x16_shuffle(a: *mut [u8; 16], b: *const [u8; 16], c: *const [u8; 16]) {
    unsafe {
        *a = simd_utils::i8x16_shuffle(*a, *b, *c);
    }
}

macro_rules! impl_simd_binop {
    ($name:ident, $func:ident) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(a: *mut [u8; 16], b: *const [u8; 16]) {
            unsafe {
                *a = simd_utils::$func(*a, *b);
            }
        }
    };
}

macro_rules! impl_simd_unop {
    ($name:ident, $func:ident) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(a: *mut [u8; 16]) {
            unsafe {
                *a = simd_utils::$func(*a);
            }
        }
    };
}

impl_simd_binop!(aot_i8x16_add, i8x16_add);
impl_simd_binop!(aot_i8x16_sub, i8x16_sub);
impl_simd_binop!(aot_i16x8_add, i16x8_add);
impl_simd_binop!(aot_i16x8_sub, i16x8_sub);
impl_simd_binop!(aot_i16x8_mul, i16x8_mul);
impl_simd_binop!(aot_i32x4_add, i32x4_add);
impl_simd_binop!(aot_i32x4_sub, i32x4_sub);
impl_simd_binop!(aot_i32x4_mul, i32x4_mul);
impl_simd_binop!(aot_i64x2_add, i64x2_add);
impl_simd_binop!(aot_i64x2_sub, i64x2_sub);
impl_simd_binop!(aot_i64x2_mul, i64x2_mul);

impl_simd_binop!(aot_f32x4_add, f32x4_add);
impl_simd_binop!(aot_f32x4_sub, f32x4_sub);
impl_simd_binop!(aot_f32x4_mul, f32x4_mul);
impl_simd_binop!(aot_f32x4_div, f32x4_div);
impl_simd_binop!(aot_f32x4_min, f32x4_min);
impl_simd_binop!(aot_f32x4_max, f32x4_max);
impl_simd_binop!(aot_f32x4_pmin, f32x4_pmin);
impl_simd_binop!(aot_f32x4_pmax, f32x4_pmax);

impl_simd_binop!(aot_f64x2_add, f64x2_add);
impl_simd_binop!(aot_f64x2_sub, f64x2_sub);
impl_simd_binop!(aot_f64x2_mul, f64x2_mul);
impl_simd_binop!(aot_f64x2_div, f64x2_div);
impl_simd_binop!(aot_f64x2_min, f64x2_min);
impl_simd_binop!(aot_f64x2_max, f64x2_max);
impl_simd_binop!(aot_f64x2_pmin, f64x2_pmin);
impl_simd_binop!(aot_f64x2_pmax, f64x2_pmax);

impl_simd_binop!(aot_i8x16_eq, i8x16_eq);
impl_simd_binop!(aot_i8x16_ne, i8x16_ne);
impl_simd_binop!(aot_i8x16_lt_s, i8x16_lt_s);
impl_simd_binop!(aot_i8x16_lt_u, i8x16_lt_u);
impl_simd_binop!(aot_i8x16_gt_s, i8x16_gt_s);
impl_simd_binop!(aot_i8x16_gt_u, i8x16_gt_u);
impl_simd_binop!(aot_i8x16_le_s, i8x16_le_s);
impl_simd_binop!(aot_i8x16_le_u, i8x16_le_u);
impl_simd_binop!(aot_i8x16_ge_s, i8x16_ge_s);
impl_simd_binop!(aot_i8x16_ge_u, i8x16_ge_u);

impl_simd_binop!(aot_i16x8_eq, i16x8_eq);
impl_simd_binop!(aot_i16x8_ne, i16x8_ne);
impl_simd_binop!(aot_i16x8_lt_s, i16x8_lt_s);
impl_simd_binop!(aot_i16x8_lt_u, i16x8_lt_u);
impl_simd_binop!(aot_i16x8_gt_s, i16x8_gt_s);
impl_simd_binop!(aot_i16x8_gt_u, i16x8_gt_u);
impl_simd_binop!(aot_i16x8_le_s, i16x8_le_s);
impl_simd_binop!(aot_i16x8_le_u, i16x8_le_u);
impl_simd_binop!(aot_i16x8_ge_s, i16x8_ge_s);
impl_simd_binop!(aot_i16x8_ge_u, i16x8_ge_u);

impl_simd_binop!(aot_i32x4_eq, i32x4_eq);
impl_simd_binop!(aot_i32x4_ne, i32x4_ne);
impl_simd_binop!(aot_i32x4_lt_s, i32x4_lt_s);
impl_simd_binop!(aot_i32x4_lt_u, i32x4_lt_u);
impl_simd_binop!(aot_i32x4_gt_s, i32x4_gt_s);
impl_simd_binop!(aot_i32x4_gt_u, i32x4_gt_u);
impl_simd_binop!(aot_i32x4_le_s, i32x4_le_s);
impl_simd_binop!(aot_i32x4_le_u, i32x4_le_u);
impl_simd_binop!(aot_i32x4_ge_s, i32x4_ge_s);
impl_simd_binop!(aot_i32x4_ge_u, i32x4_ge_u);

impl_simd_binop!(aot_i64x2_eq, i64x2_eq);
impl_simd_binop!(aot_i64x2_ne, i64x2_ne);
impl_simd_binop!(aot_i64x2_lt_s, i64x2_lt_s);
impl_simd_binop!(aot_i64x2_gt_s, i64x2_gt_s);
impl_simd_binop!(aot_i64x2_le_s, i64x2_le_s);
impl_simd_binop!(aot_i64x2_ge_s, i64x2_ge_s);

impl_simd_binop!(aot_f32x4_eq, f32x4_eq);
impl_simd_binop!(aot_f32x4_ne, f32x4_ne);
impl_simd_binop!(aot_f32x4_lt, f32x4_lt);
impl_simd_binop!(aot_f32x4_gt, f32x4_gt);
impl_simd_binop!(aot_f32x4_le, f32x4_le);
impl_simd_binop!(aot_f32x4_ge, f32x4_ge);

impl_simd_binop!(aot_f64x2_eq, f64x2_eq);
impl_simd_binop!(aot_f64x2_ne, f64x2_ne);
impl_simd_binop!(aot_f64x2_lt, f64x2_lt);
impl_simd_binop!(aot_f64x2_gt, f64x2_gt);
impl_simd_binop!(aot_f64x2_le, f64x2_le);
impl_simd_binop!(aot_f64x2_ge, f64x2_ge);

impl_simd_unop!(aot_i8x16_neg, i8x16_neg);
impl_simd_unop!(aot_i8x16_abs, i8x16_abs);
impl_simd_unop!(aot_i16x8_neg, i16x8_neg);
impl_simd_unop!(aot_i16x8_abs, i16x8_abs);
impl_simd_unop!(aot_i32x4_neg, i32x4_neg);
impl_simd_unop!(aot_i32x4_abs, i32x4_abs);
impl_simd_unop!(aot_i64x2_neg, i64x2_neg);
impl_simd_unop!(aot_i64x2_abs, i64x2_abs);

impl_simd_unop!(aot_f32x4_neg, f32x4_neg);
impl_simd_unop!(aot_f32x4_abs, f32x4_abs);
impl_simd_unop!(aot_f32x4_sqrt, f32x4_sqrt);
impl_simd_unop!(aot_f32x4_ceil, f32x4_ceil);
impl_simd_unop!(aot_f32x4_floor, f32x4_floor);
impl_simd_unop!(aot_f32x4_trunc, f32x4_trunc);
impl_simd_unop!(aot_f32x4_nearest, f32x4_nearest);

impl_simd_unop!(aot_f64x2_neg, f64x2_neg);
impl_simd_unop!(aot_f64x2_abs, f64x2_abs);
impl_simd_unop!(aot_f64x2_sqrt, f64x2_sqrt);
impl_simd_unop!(aot_f64x2_ceil, f64x2_ceil);
impl_simd_unop!(aot_f64x2_floor, f64x2_floor);
impl_simd_unop!(aot_f64x2_trunc, f64x2_trunc);
impl_simd_unop!(aot_f64x2_nearest, f64x2_nearest);

impl_simd_binop!(aot_v128_andnot, v128_andnot);

impl_simd_binop!(aot_i8x16_min_s, i8x16_min_s);
impl_simd_binop!(aot_i8x16_min_u, i8x16_min_u);
impl_simd_binop!(aot_i8x16_max_s, i8x16_max_s);
impl_simd_binop!(aot_i8x16_max_u, i8x16_max_u);
impl_simd_binop!(aot_i16x8_min_s, i16x8_min_s);
impl_simd_binop!(aot_i16x8_min_u, i16x8_min_u);
impl_simd_binop!(aot_i16x8_max_s, i16x8_max_s);
impl_simd_binop!(aot_i16x8_max_u, i16x8_max_u);
impl_simd_binop!(aot_i32x4_min_s, i32x4_min_s);
impl_simd_binop!(aot_i32x4_min_u, i32x4_min_u);
impl_simd_binop!(aot_i32x4_max_s, i32x4_max_s);
impl_simd_binop!(aot_i32x4_max_u, i32x4_max_u);

impl_simd_binop!(aot_i8x16_avgr_u, i8x16_avgr_u);
impl_simd_binop!(aot_i16x8_avgr_u, i16x8_avgr_u);

impl_simd_binop!(aot_i8x16_add_sat_s, i8x16_add_sat_s);
impl_simd_binop!(aot_i8x16_add_sat_u, i8x16_add_sat_u);
impl_simd_binop!(aot_i8x16_sub_sat_s, i8x16_sub_sat_s);
impl_simd_binop!(aot_i8x16_sub_sat_u, i8x16_sub_sat_u);
impl_simd_binop!(aot_i16x8_add_sat_s, i16x8_add_sat_s);
impl_simd_binop!(aot_i16x8_add_sat_u, i16x8_add_sat_u);
impl_simd_binop!(aot_i16x8_sub_sat_s, i16x8_sub_sat_s);
impl_simd_binop!(aot_i16x8_sub_sat_u, i16x8_sub_sat_u);

#[unsafe(no_mangle)]
pub extern "C" fn aot_i8x16_popcnt(a: *mut [u8; 16]) {
    unsafe {
        *a = simd_utils::i8x16_popcnt(*a);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_v128_load8x8_s(dst: *mut [u8; 16], src: u64) {
    let lanes = simd_utils::to_lanes_8::<1, 8, i8>(src.to_le_bytes());
    let mut res = [0i16; 8];
    for i in 0..8 {
        res[i] = lanes[i] as i16;
    }
    unsafe {
        *dst = simd_utils::from_lanes::<2, 8, i16>(res);
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_v128_load8x8_u(dst: *mut [u8; 16], src: u64) {
    let lanes = simd_utils::to_lanes_8::<1, 8, u8>(src.to_le_bytes());
    let mut res = [0i16; 8];
    for i in 0..8 {
        res[i] = lanes[i] as i16;
    }
    unsafe {
        *dst = simd_utils::from_lanes::<2, 8, i16>(res);
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_v128_load16x4_s(dst: *mut [u8; 16], src: u64) {
    let lanes = simd_utils::to_lanes_8::<2, 4, i16>(src.to_le_bytes());
    let mut res = [0i32; 4];
    for i in 0..4 {
        res[i] = lanes[i] as i32;
    }
    unsafe {
        *dst = simd_utils::from_lanes::<4, 4, i32>(res);
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_v128_load16x4_u(dst: *mut [u8; 16], src: u64) {
    let lanes = simd_utils::to_lanes_8::<2, 4, u16>(src.to_le_bytes());
    let mut res = [0i32; 4];
    for i in 0..4 {
        res[i] = lanes[i] as i32;
    }
    unsafe {
        *dst = simd_utils::from_lanes::<4, 4, i32>(res);
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_v128_load32x2_s(dst: *mut [u8; 16], src: u64) {
    let lanes = simd_utils::to_lanes_8::<4, 2, i32>(src.to_le_bytes());
    let mut res = [0i64; 2];
    for i in 0..2 {
        res[i] = lanes[i] as i64;
    }
    unsafe {
        *dst = simd_utils::from_lanes::<8, 2, i64>(res);
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_v128_load32x2_u(dst: *mut [u8; 16], src: u64) {
    let lanes = simd_utils::to_lanes_8::<4, 2, u32>(src.to_le_bytes());
    let mut res = [0i64; 2];
    for i in 0..2 {
        res[i] = lanes[i] as i64;
    }
    unsafe {
        *dst = simd_utils::from_lanes::<8, 2, i64>(res);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_v128_bitmask_i16x8(a: *const [u8; 16]) -> i32 {
    unsafe { simd_utils::i16x8_bitmask(*a) }
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_v128_bitmask_i32x4(a: *const [u8; 16]) -> i32 {
    unsafe { simd_utils::i32x4_bitmask(*a) }
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_v128_bitmask_i64x2(a: *const [u8; 16]) -> i32 {
    unsafe { simd_utils::i64x2_bitmask(*a) }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_v128_all_true_i8x16(a: *const [u8; 16]) -> i32 {
    if unsafe { simd_utils::i8x16_all_true(*a) } {
        1
    } else {
        0
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_v128_all_true_i16x8(a: *const [u8; 16]) -> i32 {
    if unsafe { simd_utils::i16x8_all_true(*a) } {
        1
    } else {
        0
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_v128_all_true_i32x4(a: *const [u8; 16]) -> i32 {
    if unsafe { simd_utils::i32x4_all_true(*a) } {
        1
    } else {
        0
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_v128_all_true_i64x2(a: *const [u8; 16]) -> i32 {
    if unsafe { simd_utils::i64x2_all_true(*a) } {
        1
    } else {
        0
    }
}

impl_simd_binop!(aot_i8x16_narrow_i16x8_s, i8x16_narrow_i16x8_s);
impl_simd_binop!(aot_i8x16_narrow_i16x8_u, i8x16_narrow_i16x8_u);
impl_simd_binop!(aot_i16x8_narrow_i32x4_s, i16x8_narrow_i32x4_s);
impl_simd_binop!(aot_i16x8_narrow_i32x4_u, i16x8_narrow_i32x4_u);

impl_simd_unop!(aot_i16x8_extend_low_i8x16_s, i16x8_extend_low_i8x16_s);
impl_simd_unop!(aot_i16x8_extend_high_i8x16_s, i16x8_extend_high_i8x16_s);
impl_simd_unop!(aot_i16x8_extend_low_i8x16_u, i16x8_extend_low_i8x16_u);
impl_simd_unop!(aot_i16x8_extend_high_i8x16_u, i16x8_extend_high_i8x16_u);
impl_simd_unop!(aot_i32x4_extend_low_i16x8_s, i32x4_extend_low_i16x8_s);
impl_simd_unop!(aot_i32x4_extend_high_i16x8_s, i32x4_extend_high_i16x8_s);
impl_simd_unop!(aot_i32x4_extend_low_i16x8_u, i32x4_extend_low_i16x8_u);
impl_simd_unop!(aot_i32x4_extend_high_i16x8_u, i32x4_extend_high_i16x8_u);
impl_simd_unop!(aot_i64x2_extend_low_i32x4_s, i64x2_extend_low_i32x4_s);
impl_simd_unop!(aot_i64x2_extend_high_i32x4_s, i64x2_extend_high_i32x4_s);
impl_simd_unop!(aot_i64x2_extend_low_i32x4_u, i64x2_extend_low_i32x4_u);
impl_simd_unop!(aot_i64x2_extend_high_i32x4_u, i64x2_extend_high_i32x4_u);

impl_simd_binop!(aot_i16x8_extmul_low_i8x16_s, i16x8_extmul_low_i8x16_s);
impl_simd_binop!(aot_i16x8_extmul_high_i8x16_s, i16x8_extmul_high_i8x16_s);
impl_simd_binop!(aot_i16x8_extmul_low_i8x16_u, i16x8_extmul_low_i8x16_u);
impl_simd_binop!(aot_i16x8_extmul_high_i8x16_u, i16x8_extmul_high_i8x16_u);
impl_simd_binop!(aot_i32x4_extmul_low_i16x8_s, i32x4_extmul_low_i16x8_s);
impl_simd_binop!(aot_i32x4_extmul_high_i16x8_s, i32x4_extmul_high_i16x8_s);
impl_simd_binop!(aot_i32x4_extmul_low_i16x8_u, i32x4_extmul_low_i16x8_u);
impl_simd_binop!(aot_i32x4_extmul_high_i16x8_u, i32x4_extmul_high_i16x8_u);
impl_simd_binop!(aot_i64x2_extmul_low_i32x4_s, i64x2_extmul_low_i32x4_s);
impl_simd_binop!(aot_i64x2_extmul_high_i32x4_s, i64x2_extmul_high_i32x4_s);
impl_simd_binop!(aot_i64x2_extmul_low_i32x4_u, i64x2_extmul_low_i32x4_u);
impl_simd_binop!(aot_i64x2_extmul_high_i32x4_u, i64x2_extmul_high_i32x4_u);

impl_simd_unop!(
    aot_i16x8_extadd_pairwise_i8x16_s,
    i16x8_extadd_pairwise_i8x16_s
);
impl_simd_unop!(
    aot_i16x8_extadd_pairwise_i8x16_u,
    i16x8_extadd_pairwise_i8x16_u
);
impl_simd_unop!(
    aot_i32x4_extadd_pairwise_i16x8_s,
    i32x4_extadd_pairwise_i16x8_s
);
impl_simd_unop!(
    aot_i32x4_extadd_pairwise_i16x8_u,
    i32x4_extadd_pairwise_i16x8_u
);

impl_simd_binop!(aot_i32x4_dot_i16x8_s, i32x4_dot_i16x8_s);
impl_simd_binop!(aot_i16x8_q15mulrsat_s, i16x8_q15mulrsat_s);

impl_simd_unop!(aot_i32x4_trunc_sat_f32x4_s, i32x4_trunc_sat_f32x4_s);
impl_simd_unop!(aot_i32x4_trunc_sat_f32x4_u, i32x4_trunc_sat_f32x4_u);
impl_simd_unop!(aot_f32x4_convert_i32x4_s, f32x4_convert_i32x4_s);
impl_simd_unop!(aot_f32x4_convert_i32x4_u, f32x4_convert_i32x4_u);
impl_simd_unop!(
    aot_i32x4_trunc_sat_f64x2_s_zero,
    i32x4_trunc_sat_f64x2_s_zero
);
impl_simd_unop!(
    aot_i32x4_trunc_sat_f64x2_u_zero,
    i32x4_trunc_sat_f64x2_u_zero
);
impl_simd_unop!(aot_f64x2_convert_low_i32x4_s, f64x2_convert_low_i32x4_s);
impl_simd_unop!(aot_f64x2_convert_low_i32x4_u, f64x2_convert_low_i32x4_u);

#[unsafe(no_mangle)]
pub extern "C" fn aot_v128_not(a: *mut [u8; 16]) {
    unsafe {
        *a = simd_utils::v128_not(*a);
    }
}

#[repr(transparent)]
struct SyncPtr(*const ());
unsafe impl Sync for SyncPtr {}

pub fn get_trampoline_table() -> *const usize {
    use crate::wasm::aot::runtime::AotTrampoline;
    static TABLE: [SyncPtr; AotTrampoline::Count as usize] = [
        SyncPtr(aot_trap as *const ()),
        SyncPtr(aot_trap_oob as *const ()),
        SyncPtr(aot_trap_fuel as *const ()),
        SyncPtr(aot_trap_div_zero as *const ()),
        SyncPtr(aot_trap_int_overflow as *const ()),
        SyncPtr(aot_trap_indirect as *const ()),
        SyncPtr(aot_trap_unreachable as *const ()),
        SyncPtr(aot_trap_stack_overflow as *const ()),
        SyncPtr(aot_trap_host as *const ()),
        SyncPtr(aot_trap_unimplemented_fc as *const ()),
        SyncPtr(aot_trap_unimplemented_simd as *const ()),
        SyncPtr(aot_trap_unimplemented_atomic as *const ()),
        SyncPtr(aot_i32_div_s as *const ()),
        SyncPtr(aot_i32_div_u as *const ()),
        SyncPtr(aot_i32_rem_s as *const ()),
        SyncPtr(aot_i32_rem_u as *const ()),
        SyncPtr(aot_i64_div_s as *const ()),
        SyncPtr(aot_i64_div_u as *const ()),
        SyncPtr(aot_i64_rem_s as *const ()),
        SyncPtr(aot_i64_rem_u as *const ()),
        SyncPtr(aot_f32_eq as *const ()),
        SyncPtr(aot_f32_ne as *const ()),
        SyncPtr(aot_f32_lt as *const ()),
        SyncPtr(aot_f32_gt as *const ()),
        SyncPtr(aot_f32_le as *const ()),
        SyncPtr(aot_f32_ge as *const ()),
        SyncPtr(aot_f64_eq as *const ()),
        SyncPtr(aot_f64_ne as *const ()),
        SyncPtr(aot_f64_lt as *const ()),
        SyncPtr(aot_f64_gt as *const ()),
        SyncPtr(aot_f64_le as *const ()),
        SyncPtr(aot_f64_ge as *const ()),
        SyncPtr(aot_f32_min as *const ()),
        SyncPtr(aot_f32_max as *const ()),
        SyncPtr(aot_f64_min as *const ()),
        SyncPtr(aot_f64_max as *const ()),
        SyncPtr(aot_f32_convert_i64_u as *const ()),
        SyncPtr(aot_f64_convert_i64_u as *const ()),
        SyncPtr(aot_i32_trunc_f32_u as *const ()),
        SyncPtr(aot_i32_trunc_f64_u as *const ()),
        SyncPtr(aot_i64_trunc_f32_u as *const ()),
        SyncPtr(aot_i64_trunc_f64_u as *const ()),
        SyncPtr(aot_i32_trunc_sat_f32_s as *const ()),
        SyncPtr(aot_i32_trunc_sat_f32_u as *const ()),
        SyncPtr(aot_i32_trunc_sat_f64_s as *const ()),
        SyncPtr(aot_i32_trunc_sat_f64_u as *const ()),
        SyncPtr(aot_i64_trunc_sat_f32_s as *const ()),
        SyncPtr(aot_i64_trunc_sat_f32_u as *const ()),
        SyncPtr(aot_i64_trunc_sat_f64_s as *const ()),
        SyncPtr(aot_i64_trunc_sat_f64_u as *const ()),
        SyncPtr(aot_memory_init as *const ()),
        SyncPtr(aot_data_drop as *const ()),
        SyncPtr(aot_memory_copy as *const ()),
        SyncPtr(aot_memory_fill as *const ()),
        SyncPtr(aot_table_init as *const ()),
        SyncPtr(aot_elem_drop as *const ()),
        SyncPtr(aot_table_copy as *const ()),
        SyncPtr(aot_table_grow as *const ()),
        SyncPtr(aot_table_size as *const ()),
        SyncPtr(aot_table_fill as *const ()),
        SyncPtr(aot_memory_size as *const ()),
        SyncPtr(aot_memory_grow as *const ()),
        SyncPtr(aot_global_get as *const ()),
        SyncPtr(aot_global_set as *const ()),
        SyncPtr(aot_table_get as *const ()),
        SyncPtr(aot_table_set as *const ()),
        SyncPtr(aot_call_indirect as *const ()),
        SyncPtr(aot_call_host as *const ()),
        SyncPtr(aot_ref_func as *const ()),
        SyncPtr(aot_v128_load_lane as *const ()),
        SyncPtr(aot_v128_store_lane as *const ()),
        SyncPtr(aot_v128_and as *const ()),
        SyncPtr(aot_v128_or as *const ()),
        SyncPtr(aot_v128_xor as *const ()),
        SyncPtr(aot_v128_bitselect as *const ()),
        SyncPtr(aot_v128_eq_i8x16 as *const ()),
        SyncPtr(aot_v128_eq_i16x8 as *const ()),
        SyncPtr(aot_v128_eq_i32x4 as *const ()),
        SyncPtr(aot_v128_eq_i64x2 as *const ()),
        SyncPtr(aot_v128_eq_f32x4 as *const ()),
        SyncPtr(aot_v128_eq_f64x2 as *const ()),
        SyncPtr(aot_v128_any_true as *const ()),
        SyncPtr(aot_v128_bitmask_i8x16 as *const ()),
        SyncPtr(aot_v128_i8x16_shuffle as *const ()),
        SyncPtr(aot_i8x16_add as *const ()),
        SyncPtr(aot_i8x16_sub as *const ()),
        SyncPtr(aot_i16x8_add as *const ()),
        SyncPtr(aot_i16x8_sub as *const ()),
        SyncPtr(aot_i16x8_mul as *const ()),
        SyncPtr(aot_i32x4_add as *const ()),
        SyncPtr(aot_i32x4_sub as *const ()),
        SyncPtr(aot_i32x4_mul as *const ()),
        SyncPtr(aot_i64x2_add as *const ()),
        SyncPtr(aot_i64x2_sub as *const ()),
        SyncPtr(aot_i64x2_mul as *const ()),
        SyncPtr(aot_f32x4_add as *const ()),
        SyncPtr(aot_f32x4_sub as *const ()),
        SyncPtr(aot_f32x4_mul as *const ()),
        SyncPtr(aot_f32x4_div as *const ()),
        SyncPtr(aot_f32x4_min as *const ()),
        SyncPtr(aot_f32x4_max as *const ()),
        SyncPtr(aot_f32x4_pmin as *const ()),
        SyncPtr(aot_f32x4_pmax as *const ()),
        SyncPtr(aot_f64x2_add as *const ()),
        SyncPtr(aot_f64x2_sub as *const ()),
        SyncPtr(aot_f64x2_mul as *const ()),
        SyncPtr(aot_f64x2_div as *const ()),
        SyncPtr(aot_f64x2_min as *const ()),
        SyncPtr(aot_f64x2_max as *const ()),
        SyncPtr(aot_f64x2_pmin as *const ()),
        SyncPtr(aot_f64x2_pmax as *const ()),
        SyncPtr(aot_i8x16_eq as *const ()),
        SyncPtr(aot_i8x16_ne as *const ()),
        SyncPtr(aot_i8x16_lt_s as *const ()),
        SyncPtr(aot_i8x16_lt_u as *const ()),
        SyncPtr(aot_i8x16_gt_s as *const ()),
        SyncPtr(aot_i8x16_gt_u as *const ()),
        SyncPtr(aot_i8x16_le_s as *const ()),
        SyncPtr(aot_i8x16_le_u as *const ()),
        SyncPtr(aot_i8x16_ge_s as *const ()),
        SyncPtr(aot_i8x16_ge_u as *const ()),
        SyncPtr(aot_i16x8_eq as *const ()),
        SyncPtr(aot_i16x8_ne as *const ()),
        SyncPtr(aot_i16x8_lt_s as *const ()),
        SyncPtr(aot_i16x8_lt_u as *const ()),
        SyncPtr(aot_i16x8_gt_s as *const ()),
        SyncPtr(aot_i16x8_gt_u as *const ()),
        SyncPtr(aot_i16x8_le_s as *const ()),
        SyncPtr(aot_i16x8_le_u as *const ()),
        SyncPtr(aot_i16x8_ge_s as *const ()),
        SyncPtr(aot_i16x8_ge_u as *const ()),
        SyncPtr(aot_i32x4_eq as *const ()),
        SyncPtr(aot_i32x4_ne as *const ()),
        SyncPtr(aot_i32x4_lt_s as *const ()),
        SyncPtr(aot_i32x4_lt_u as *const ()),
        SyncPtr(aot_i32x4_gt_s as *const ()),
        SyncPtr(aot_i32x4_gt_u as *const ()),
        SyncPtr(aot_i32x4_le_s as *const ()),
        SyncPtr(aot_i32x4_le_u as *const ()),
        SyncPtr(aot_i32x4_ge_s as *const ()),
        SyncPtr(aot_i32x4_ge_u as *const ()),
        SyncPtr(aot_i64x2_eq as *const ()),
        SyncPtr(aot_i64x2_ne as *const ()),
        SyncPtr(aot_i64x2_lt_s as *const ()),
        SyncPtr(aot_i64x2_gt_s as *const ()),
        SyncPtr(aot_i64x2_le_s as *const ()),
        SyncPtr(aot_i64x2_ge_s as *const ()),
        SyncPtr(aot_f32x4_eq as *const ()),
        SyncPtr(aot_f32x4_ne as *const ()),
        SyncPtr(aot_f32x4_lt as *const ()),
        SyncPtr(aot_f32x4_gt as *const ()),
        SyncPtr(aot_f32x4_le as *const ()),
        SyncPtr(aot_f32x4_ge as *const ()),
        SyncPtr(aot_f64x2_eq as *const ()),
        SyncPtr(aot_f64x2_ne as *const ()),
        SyncPtr(aot_f64x2_lt as *const ()),
        SyncPtr(aot_f64x2_gt as *const ()),
        SyncPtr(aot_f64x2_le as *const ()),
        SyncPtr(aot_f64x2_ge as *const ()),
        SyncPtr(aot_i8x16_neg as *const ()),
        SyncPtr(aot_i8x16_abs as *const ()),
        SyncPtr(aot_i16x8_neg as *const ()),
        SyncPtr(aot_i16x8_abs as *const ()),
        SyncPtr(aot_i32x4_neg as *const ()),
        SyncPtr(aot_i32x4_abs as *const ()),
        SyncPtr(aot_i64x2_neg as *const ()),
        SyncPtr(aot_i64x2_abs as *const ()),
        SyncPtr(aot_f32x4_neg as *const ()),
        SyncPtr(aot_f32x4_abs as *const ()),
        SyncPtr(aot_f32x4_sqrt as *const ()),
        SyncPtr(aot_f32x4_ceil as *const ()),
        SyncPtr(aot_f32x4_floor as *const ()),
        SyncPtr(aot_f32x4_trunc as *const ()),
        SyncPtr(aot_f32x4_nearest as *const ()),
        SyncPtr(aot_f64x2_neg as *const ()),
        SyncPtr(aot_f64x2_abs as *const ()),
        SyncPtr(aot_f64x2_sqrt as *const ()),
        SyncPtr(aot_f64x2_ceil as *const ()),
        SyncPtr(aot_f64x2_floor as *const ()),
        SyncPtr(aot_f64x2_trunc as *const ()),
        SyncPtr(aot_f64x2_nearest as *const ()),
        SyncPtr(aot_v128_andnot as *const ()),
        SyncPtr(aot_i8x16_min_s as *const ()),
        SyncPtr(aot_i8x16_min_u as *const ()),
        SyncPtr(aot_i8x16_max_s as *const ()),
        SyncPtr(aot_i8x16_max_u as *const ()),
        SyncPtr(aot_i16x8_min_s as *const ()),
        SyncPtr(aot_i16x8_min_u as *const ()),
        SyncPtr(aot_i16x8_max_s as *const ()),
        SyncPtr(aot_i16x8_max_u as *const ()),
        SyncPtr(aot_i32x4_min_s as *const ()),
        SyncPtr(aot_i32x4_min_u as *const ()),
        SyncPtr(aot_i32x4_max_s as *const ()),
        SyncPtr(aot_i32x4_max_u as *const ()),
        SyncPtr(aot_i8x16_avgr_u as *const ()),
        SyncPtr(aot_i16x8_avgr_u as *const ()),
        SyncPtr(aot_i8x16_add_sat_s as *const ()),
        SyncPtr(aot_i8x16_add_sat_u as *const ()),
        SyncPtr(aot_i8x16_sub_sat_s as *const ()),
        SyncPtr(aot_i8x16_sub_sat_u as *const ()),
        SyncPtr(aot_i16x8_add_sat_s as *const ()),
        SyncPtr(aot_i16x8_add_sat_u as *const ()),
        SyncPtr(aot_i16x8_sub_sat_s as *const ()),
        SyncPtr(aot_i16x8_sub_sat_u as *const ()),
        SyncPtr(aot_i8x16_popcnt as *const ()),
        SyncPtr(aot_v128_bitmask_i16x8 as *const ()),
        SyncPtr(aot_v128_bitmask_i32x4 as *const ()),
        SyncPtr(aot_v128_bitmask_i64x2 as *const ()),
        SyncPtr(aot_v128_all_true_i8x16 as *const ()),
        SyncPtr(aot_v128_all_true_i16x8 as *const ()),
        SyncPtr(aot_v128_all_true_i32x4 as *const ()),
        SyncPtr(aot_v128_all_true_i64x2 as *const ()),
        SyncPtr(aot_i8x16_narrow_i16x8_s as *const ()),
        SyncPtr(aot_i8x16_narrow_i16x8_u as *const ()),
        SyncPtr(aot_i16x8_narrow_i32x4_s as *const ()),
        SyncPtr(aot_i16x8_narrow_i32x4_u as *const ()),
        SyncPtr(aot_i16x8_extend_low_i8x16_s as *const ()),
        SyncPtr(aot_i16x8_extend_high_i8x16_s as *const ()),
        SyncPtr(aot_i16x8_extend_low_i8x16_u as *const ()),
        SyncPtr(aot_i16x8_extend_high_i8x16_u as *const ()),
        SyncPtr(aot_i32x4_extend_low_i16x8_s as *const ()),
        SyncPtr(aot_i32x4_extend_high_i16x8_s as *const ()),
        SyncPtr(aot_i32x4_extend_low_i16x8_u as *const ()),
        SyncPtr(aot_i32x4_extend_high_i16x8_u as *const ()),
        SyncPtr(aot_i64x2_extend_low_i32x4_s as *const ()),
        SyncPtr(aot_i64x2_extend_high_i32x4_s as *const ()),
        SyncPtr(aot_i64x2_extend_low_i32x4_u as *const ()),
        SyncPtr(aot_i64x2_extend_high_i32x4_u as *const ()),
        SyncPtr(aot_i16x8_extmul_low_i8x16_s as *const ()),
        SyncPtr(aot_i16x8_extmul_high_i8x16_s as *const ()),
        SyncPtr(aot_i16x8_extmul_low_i8x16_u as *const ()),
        SyncPtr(aot_i16x8_extmul_high_i8x16_u as *const ()),
        SyncPtr(aot_i32x4_extmul_low_i16x8_s as *const ()),
        SyncPtr(aot_i32x4_extmul_high_i16x8_s as *const ()),
        SyncPtr(aot_i32x4_extmul_low_i16x8_u as *const ()),
        SyncPtr(aot_i32x4_extmul_high_i16x8_u as *const ()),
        SyncPtr(aot_i64x2_extmul_low_i32x4_s as *const ()),
        SyncPtr(aot_i64x2_extmul_high_i32x4_s as *const ()),
        SyncPtr(aot_i64x2_extmul_low_i32x4_u as *const ()),
        SyncPtr(aot_i64x2_extmul_high_i32x4_u as *const ()),
        SyncPtr(aot_i16x8_extadd_pairwise_i8x16_s as *const ()),
        SyncPtr(aot_i16x8_extadd_pairwise_i8x16_u as *const ()),
        SyncPtr(aot_i32x4_extadd_pairwise_i16x8_s as *const ()),
        SyncPtr(aot_i32x4_extadd_pairwise_i16x8_u as *const ()),
        SyncPtr(aot_i32x4_dot_i16x8_s as *const ()),
        SyncPtr(aot_i16x8_q15mulrsat_s as *const ()),
        SyncPtr(aot_i32x4_trunc_sat_f32x4_s as *const ()),
        SyncPtr(aot_i32x4_trunc_sat_f32x4_u as *const ()),
        SyncPtr(aot_f32x4_convert_i32x4_s as *const ()),
        SyncPtr(aot_f32x4_convert_i32x4_u as *const ()),
        SyncPtr(aot_i32x4_trunc_sat_f64x2_s_zero as *const ()),
        SyncPtr(aot_i32x4_trunc_sat_f64x2_u_zero as *const ()),
        SyncPtr(aot_f64x2_convert_low_i32x4_s as *const ()),
        SyncPtr(aot_f64x2_convert_low_i32x4_u as *const ()),
        SyncPtr(aot_v128_not as *const ()),
        SyncPtr(aot_v128_load8x8_s as *const ()),
        SyncPtr(aot_v128_load8x8_u as *const ()),
        SyncPtr(aot_v128_load16x4_s as *const ()),
        SyncPtr(aot_v128_load16x4_u as *const ()),
        SyncPtr(aot_v128_load32x2_s as *const ()),
        SyncPtr(aot_v128_load32x2_u as *const ()),
    ];
    TABLE.as_ptr() as *const usize
}
