use crate::context::Ring3Context;

#[no_mangle]
pub extern "C" fn i32_div_s(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let b = (*sp.add(0)) as i32;
        let a = (*sp.add(1)) as i32;
        if b == 0 { crate::traps::trap_div_zero(_ctx, sp); }
        if a == i32::MIN && b == -1 { crate::traps::trap_int_overflow(_ctx, sp); }
        let res = a / b;
        let result_sp = sp.add(2).sub(1);
        *result_sp = (res as u32) as u128;
        result_sp
    }
}

#[no_mangle]
pub extern "C" fn i32_div_u(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let b = (*sp.add(0)) as u32;
        let a = (*sp.add(1)) as u32;
        if b == 0 { crate::traps::trap_div_zero(_ctx, sp); }
        let res = a / b;
        let result_sp = sp.add(2).sub(1);
        *result_sp = res as u128;
        result_sp
    }
}

#[no_mangle]
pub extern "C" fn i32_rem_s(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let b = (*sp.add(0)) as i32;
        let a = (*sp.add(1)) as i32;
        if b == 0 { crate::traps::trap_div_zero(_ctx, sp); }
        let res = a.checked_rem(b).unwrap_or(0);
        let result_sp = sp.add(2).sub(1);
        *result_sp = (res as u32) as u128;
        result_sp
    }
}

#[no_mangle]
pub extern "C" fn i32_rem_u(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let b = (*sp.add(0)) as u32;
        let a = (*sp.add(1)) as u32;
        if b == 0 { crate::traps::trap_div_zero(_ctx, sp); }
        let res = a % b;
        let result_sp = sp.add(2).sub(1);
        *result_sp = res as u128;
        result_sp
    }
}

#[no_mangle]
pub extern "C" fn i64_div_s(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let b = (*sp.add(0)) as i64;
        let a = (*sp.add(1)) as i64;
        if b == 0 { crate::traps::trap_div_zero(_ctx, sp); }
        if a == i64::MIN && b == -1 { crate::traps::trap_int_overflow(_ctx, sp); }
        let res = a / b;
        let result_sp = sp.add(2).sub(1);
        *result_sp = res as u64 as u128;
        result_sp
    }
}

#[no_mangle]
pub extern "C" fn i64_div_u(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let b = (*sp.add(0)) as u64;
        let a = (*sp.add(1)) as u64;
        if b == 0 { crate::traps::trap_div_zero(_ctx, sp); }
        let res = a / b;
        let result_sp = sp.add(2).sub(1);
        *result_sp = res as u128;
        result_sp
    }
}

#[no_mangle]
pub extern "C" fn i64_rem_s(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let b = (*sp.add(0)) as i64;
        let a = (*sp.add(1)) as i64;
        if b == 0 { crate::traps::trap_div_zero(_ctx, sp); }
        let res = a.checked_rem(b).unwrap_or(0);
        let result_sp = sp.add(2).sub(1);
        *result_sp = res as u64 as u128;
        result_sp
    }
}

#[no_mangle]
pub extern "C" fn i64_rem_u(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let b = (*sp.add(0)) as u64;
        let a = (*sp.add(1)) as u64;
        if b == 0 { crate::traps::trap_div_zero(_ctx, sp); }
        let res = a % b;
        let result_sp = sp.add(2).sub(1);
        *result_sp = res as u128;
        result_sp
    }
}
