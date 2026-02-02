use crate::wasm::common::value::{F32, F64, Ref, Value};
use crate::math::FloatMath;
use crate::wasm::interpreter::simd_utils;
use crate::wasm::aot::runtime::AotContext;
use crate::wasm::interpreter::store::Store;
use crate::wasm::interpreter::store::instances::FuncInst;
use crate::rust_alloc::vec::Vec;

#[unsafe(no_mangle)]
pub extern "C" fn aot_trap() -> ! {
    panic!("AOT Trap");
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_i32_div_s(a: i32, b: i32) -> i32 {
    if b == 0 { unsafe { aot_trap(); } }
    if a == i32::MIN && b == -1 { unsafe { aot_trap(); } }
    a / b
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_i32_div_u(a: u32, b: u32) -> u32 {
    if b == 0 { unsafe { aot_trap(); } }
    a / b
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_i32_rem_s(a: i32, b: i32) -> i32 {
    if b == 0 { unsafe { aot_trap(); } }
    a.checked_rem(b).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_i32_rem_u(a: u32, b: u32) -> u32 {
    if b == 0 { unsafe { aot_trap(); } }
    a % b
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_i64_div_s(a: i64, b: i64) -> i64 {
    if b == 0 { unsafe { aot_trap(); } }
    if a == i64::MIN && b == -1 { unsafe { aot_trap(); } }
    a / b
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_i64_div_u(a: u64, b: u64) -> u64 {
    if b == 0 { unsafe { aot_trap(); } }
    a / b
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_i64_rem_s(a: i64, b: i64) -> i64 {
    if b == 0 { unsafe { aot_trap(); } }
    a.checked_rem(b).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_i64_rem_u(a: u64, b: u64) -> u64 {
    if b == 0 { unsafe { aot_trap(); } }
    a % b
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_i32_clz(a: u32) -> u32 { a.leading_zeros() }
#[unsafe(no_mangle)]
pub extern "C" fn aot_i32_ctz(a: u32) -> u32 { a.trailing_zeros() }
#[unsafe(no_mangle)]
pub extern "C" fn aot_i32_popcnt(a: u32) -> u32 { a.count_ones() }

#[unsafe(no_mangle)]
pub extern "C" fn aot_i64_clz(a: u64) -> u64 { a.leading_zeros() as u64 }
#[unsafe(no_mangle)]
pub extern "C" fn aot_i64_ctz(a: u64) -> u64 { a.trailing_zeros() as u64 }
#[unsafe(no_mangle)]
pub extern "C" fn aot_i64_popcnt(a: u64) -> u64 { a.count_ones() as u64 }

#[unsafe(no_mangle)]
pub extern "C" fn aot_f32_abs(a: f32) -> f32 { a.abs() }
#[unsafe(no_mangle)]
pub extern "C" fn aot_f32_neg(a: f32) -> f32 { -a }
#[unsafe(no_mangle)]
pub extern "C" fn aot_f32_sqrt(a: f32) -> f32 { a.sqrt() }
#[unsafe(no_mangle)]
pub extern "C" fn aot_f32_ceil(a: f32) -> f32 { a.ceil() }
#[unsafe(no_mangle)]
pub extern "C" fn aot_f32_floor(a: f32) -> f32 { a.floor() }
#[unsafe(no_mangle)]
pub extern "C" fn aot_f32_trunc(a: f32) -> f32 { a.trunc() }
#[unsafe(no_mangle)]
pub extern "C" fn aot_f32_nearest(a: f32) -> f32 { F32(a).nearest().0 }

#[unsafe(no_mangle)]
pub extern "C" fn aot_f64_abs(a: f64) -> f64 { a.abs() }
#[unsafe(no_mangle)]
pub extern "C" fn aot_f64_neg(a: f64) -> f64 { -a }
#[unsafe(no_mangle)]
pub extern "C" fn aot_f64_sqrt(a: f64) -> f64 { a.sqrt() }
#[unsafe(no_mangle)]
pub extern "C" fn aot_f64_ceil(a: f64) -> f64 { a.ceil() }
#[unsafe(no_mangle)]
pub extern "C" fn aot_f64_floor(a: f64) -> f64 { a.floor() }
#[unsafe(no_mangle)]
pub extern "C" fn aot_f64_trunc(a: f64) -> f64 { a.trunc() }
#[unsafe(no_mangle)]
pub extern "C" fn aot_f64_nearest(a: f64) -> f64 { F64(a).nearest().0 }

#[unsafe(no_mangle)]
pub extern "C" fn aot_f32_min(a: f32, b: f32) -> f32 { F32(a).min(F32(b)).0 }
#[unsafe(no_mangle)]
pub extern "C" fn aot_f32_max(a: f32, b: f32) -> f32 { F32(a).max(F32(b)).0 }
#[unsafe(no_mangle)]
pub extern "C" fn aot_f32_copysign(a: f32, b: f32) -> f32 { a.copysign(b) }

#[unsafe(no_mangle)]
pub extern "C" fn aot_f64_min(a: f64, b: f64) -> f64 { F64(a).min(F64(b)).0 }
#[unsafe(no_mangle)]
pub extern "C" fn aot_f64_max(a: f64, b: f64) -> f64 { F64(a).max(F64(b)).0 }
#[unsafe(no_mangle)]
pub extern "C" fn aot_f64_copysign(a: f64, b: f64) -> f64 { a.copysign(b) }

#[unsafe(no_mangle)]
pub extern "C" fn aot_i32_trunc_sat_f32_s(a: f32) -> i32 {
    if a.is_nan() { 0 }
    else if a >= 2147483648.0 { i32::MAX }
    else if a < -2147483648.0 { i32::MIN }
    else { a as i32 }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_i32_trunc_sat_f32_u(a: f32) -> u32 {
    if a.is_nan() { 0 }
    else if a >= 4294967296.0 { u32::MAX }
    else if a <= -1.0 { 0 }
    else { a as u32 }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_memory_size(ctx: &AotContext) -> u32 {
    let store = unsafe { &*(ctx.store as *const Store<()>) };
    let module_addr = 0; // Simplified
    let mem_addr = store.modules.get(module_addr).mem_addrs[0];
    store.memories.get(mem_addr).size() as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_memory_grow(ctx: &mut AotContext, n: u32) -> u32 {
    let store = unsafe { &mut *(ctx.store as *mut Store<()>) };
    let module_addr = 0;
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
pub extern "C" fn aot_global_get(ctx: &AotContext, idx: u32, out: *mut [u8; 16]) {
    let store = unsafe { &*(ctx.store as *const Store<()>) };
    let module_addr = 0; // Simplified
    let global_addr = store.modules.get(module_addr).global_addrs[idx as usize];
    let val = store.globals.get(global_addr).value;
    unsafe {
        *(out as *mut u128) = val.to_u128();
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_global_set(ctx: &AotContext, idx: u32, data: *const [u8; 16]) {
    let store = unsafe { &mut *(ctx.store as *mut Store<()>) };
    let module_addr = 0;
    let global_addr = store.modules.get(module_addr).global_addrs[idx as usize];
    let global = store.globals.get_mut(global_addr);
    let ty = global.ty.ty;
    unsafe {
        global.value = Value::from_u128(*(data as *const u128), ty);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_table_get(ctx: &AotContext, table_idx: u32, i: u32) -> usize {
    let store = unsafe { &*(ctx.store as *const Store<()>) };
    let module_addr = 0;
    let table_addr = store.modules.get(module_addr).table_addrs[table_idx as usize];
    let tab = store.tables.get(table_addr);
    match tab.elem.get(i as usize).unwrap_or(&Ref::Null(crate::wasm::common::reader::types::RefType::FuncRef)) {
        Ref::Func(addr) => *addr,
        _ => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_table_set(ctx: &AotContext, table_idx: u32, i: u32, val: usize) {
    let store = unsafe { &mut *(ctx.store as *mut Store<()>) };
    let module_addr = 0;
    let table_addr = store.modules.get(module_addr).table_addrs[table_idx as usize];
    let tab = store.tables.get_mut(table_addr);
    if let Some(slot) = tab.elem.get_mut(i as usize) {
        *slot = Ref::Func(val);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_call_indirect(ctx: &AotContext, table_idx: u32, type_idx: u32, i: u32) -> *const u8 {
    let store = unsafe { &mut *(ctx.store as *mut Store<()>) };
    let module_addr = 0; 
    let table_addr = store.modules.get(module_addr).table_addrs[table_idx as usize];
    let tab = store.tables.get(table_addr);
    
    let r = tab.elem.get(i as usize).unwrap_or_else(|| unsafe { aot_trap() });
    let func_addr = match r {
        Ref::Func(addr) => *addr,
        _ => unsafe { aot_trap() },
    };
    
    let func_inst = store.functions.get(func_addr);
    let expected_ty = &store.modules.get(module_addr).types[type_idx as usize];
    if func_inst.ty() != *expected_ty {
        unsafe { aot_trap(); }
    }
    
    match func_inst {
        FuncInst::WasmFunc(wasm_func) => {
            wasm_func.aot_ptr.map(|p| p as *const u8).unwrap_or(core::ptr::null())
        }
        _ => unsafe { aot_trap() },
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_call_host(ctx: &AotContext, func_idx: u32, sp: *mut u128) -> *mut u128 {
    let store = unsafe { &mut *(ctx.store as *mut Store<()>) };
    let module_addr = 0;
    let func_addr = store.modules.get(module_addr).func_addrs[func_idx as usize];
    let func_inst = store.functions.get(func_addr);
    let ty = func_inst.ty();
    
    let mut params = Vec::with_capacity(ty.params.valtypes.len());
    unsafe {
        for i in 0..ty.params.valtypes.len() {
            let val_ptr = sp.add(ty.params.valtypes.len() - 1 - i);
            let val_type = ty.params.valtypes[i];
            let val = Value::from_u128(*val_ptr, val_type);
            params.push(val);
        }
    }
    
    let host_code = match func_inst {
        FuncInst::HostFunc(h) => h.hostcode,
        _ => unsafe { aot_trap() },
    };
    
    store.caller_module = Some(module_addr);
    let results = host_code(store, params).unwrap_or_else(|_| unsafe { aot_trap() });
    store.caller_module = None;
    
    let mut current_sp = unsafe { sp.add(ty.params.valtypes.len()) };
    for res in results {
        current_sp = unsafe { current_sp.sub(1) };
        unsafe { *current_sp = res.to_u128(); }
    }
    
    current_sp
}

#[unsafe(no_mangle)]
pub extern "C" fn aot_v128_and(a: *mut [u8; 16], b: *const [u8; 16]) {
    unsafe { *a = simd_utils::v128_and(*a, *b); }
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_v128_or(a: *mut [u8; 16], b: *const [u8; 16]) {
    unsafe { *a = simd_utils::v128_or(*a, *b); }
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_v128_xor(a: *mut [u8; 16], b: *const [u8; 16]) {
    unsafe { *a = simd_utils::v128_xor(*a, *b); }
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_v128_bitselect(a: *mut [u8; 16], b: *const [u8; 16], c: *const [u8; 16]) {
    unsafe { *a = simd_utils::v128_bitselect(*a, *b, *c); }
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_v128_eq_i8x16(a: *mut [u8; 16], b: *const [u8; 16]) {
    unsafe { *a = simd_utils::i8x16_eq(*a, *b); }
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_v128_eq_i16x8(a: *mut [u8; 16], b: *const [u8; 16]) {
    unsafe { *a = simd_utils::i16x8_eq(*a, *b); }
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_v128_eq_i32x4(a: *mut [u8; 16], b: *const [u8; 16]) {
    unsafe { *a = simd_utils::i32x4_eq(*a, *b); }
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_v128_eq_i64x2(a: *mut [u8; 16], b: *const [u8; 16]) {
    unsafe { *a = simd_utils::i64x2_eq(*a, *b); }
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_v128_eq_f32x4(a: *mut [u8; 16], b: *const [u8; 16]) {
    unsafe { *a = simd_utils::f32x4_eq(*a, *b); }
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_v128_eq_f64x2(a: *mut [u8; 16], b: *const [u8; 16]) {
    unsafe { *a = simd_utils::f64x2_eq(*a, *b); }
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_v128_any_true(a: *const [u8; 16]) -> i32 {
    if unsafe { simd_utils::v128_any_true(*a) } { 1 } else { 0 }
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_v128_bitmask_i8x16(a: *const [u8; 16]) -> i32 {
    unsafe { simd_utils::i8x16_bitmask(*a) }
}
#[unsafe(no_mangle)]
pub extern "C" fn aot_v128_i8x16_shuffle(a: *mut [u8; 16], b: *const [u8; 16], c: *const [u8; 16]) {
    unsafe { *a = simd_utils::i8x16_shuffle(*a, *b, *c); }
}
