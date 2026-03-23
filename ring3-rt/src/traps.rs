use crate::context::Ring3Context;
use crate::syscall::syscall1;

const SYS_EXIT: u64 = 60;

fn print_str(s: &str) {
    unsafe {
        crate::syscall::syscall3(999, s.as_ptr() as u64, s.len() as u64, 0);
    }
}

fn print_hex(val: u64) {
    let mut buf = [b'0'; 18]; // "0x" + 16 hex digits
    buf[0] = b'0';
    buf[1] = b'x';
    let hex = b"0123456789abcdef";
    for i in 0..16 {
        buf[2 + i] = hex[((val >> (60 - i * 4)) & 0xf) as usize];
    }
    unsafe {
        crate::syscall::syscall3(999, buf.as_ptr() as u64, 18, 0);
    }
}

fn dump_ctx(ctx: &Ring3Context) {
    print_str("  blob_base=");
    print_hex(ctx.blob_base);
    print_str("\n  mem_base=");
    print_hex(ctx.memory_base as u64);
    print_str(" mem_size=");
    print_hex(ctx.memory_size as u64);
    print_str("\n  stack_base=");
    print_hex(ctx.stack_base as u64);
    print_str(" stack_limit=");
    print_hex(ctx.stack_limit as u64);
    print_str("\n  store=");
    print_hex(ctx._reserved0);
    print_str(" module_addr=");
    print_hex(ctx._reserved2 as u64);
    print_str("\n  globals_ptr=");
    print_hex(ctx.globals_ptr as u64);
    print_str(" num_imports=");
    print_hex(ctx.num_imported_funcs as u64);
    print_str("\n  trap_code_ptr=");
    print_hex(ctx.trap_code as u64);
    print_str(" import_stub_table=");
    print_hex(ctx.import_stub_table as u64);
    print_str("\n");
}

#[no_mangle]
pub extern "C" fn trap_generic(ctx: &mut Ring3Context, rbp_val: *mut u128) -> *mut u128 {
    print_str("AOT TRAP! (generic)\n  rbp=");
    print_hex(rbp_val as u64);
    print_str("\n  trap_code=");
    print_hex(unsafe { *ctx.trap_code } as u64);
    print_str("\n");
    dump_ctx(ctx);
    unsafe {
        syscall1(SYS_EXIT, 1);
    }
    loop {}
}

#[no_mangle]
pub extern "C" fn trap_oob(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    print_str("AOT TRAP! (oob) sp=");
    print_hex(sp as u64);
    print_str("\n");
    dump_ctx(ctx);
    unsafe {
        syscall1(SYS_EXIT, 2);
    }
    loop {}
}

#[no_mangle]
pub extern "C" fn trap_fuel(_ctx: &mut Ring3Context, _sp: *mut u128) -> *mut u128 {
    print_str("AOT TRAP! (fuel)\n");
    unsafe { syscall1(SYS_EXIT, 3); }
    loop {}
}

#[no_mangle]
pub extern "C" fn trap_div_zero(_ctx: &mut Ring3Context, _sp: *mut u128) -> *mut u128 {
    print_str("AOT TRAP! (div_zero)\n");
    unsafe {
        syscall1(SYS_EXIT, 4);
    }
    loop {}
}

#[no_mangle]
pub extern "C" fn trap_int_overflow(_ctx: &mut Ring3Context, _sp: *mut u128) -> *mut u128 {
    print_str("AOT TRAP! (int_overflow)\n");
    unsafe { syscall1(SYS_EXIT, 5); }
    loop {}
}

#[no_mangle]
pub extern "C" fn trap_indirect(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    print_str("AOT TRAP! (indirect) sp=");
    print_hex(sp as u64);
    print_str("\n");
    dump_ctx(ctx);
    unsafe {
        syscall1(SYS_EXIT, 6);
    }
    loop {}
}

#[no_mangle]
pub extern "C" fn trap_unreachable(_ctx: &mut Ring3Context, _sp: *mut u128) -> *mut u128 {
    print_str("AOT TRAP! (unreachable)\n");
    unsafe {
        syscall1(SYS_EXIT, 7);
    }
    loop {}
}

#[no_mangle]
pub extern "C" fn trap_stack_overflow(_ctx: &mut Ring3Context, _sp: *mut u128) -> *mut u128 {
    print_str("AOT TRAP! (stack_overflow)\n");
    unsafe {
        syscall1(SYS_EXIT, 8);
    }
    loop {}
}

#[no_mangle]
pub extern "C" fn trap_host(_ctx: &mut Ring3Context, _sp: *mut u128) -> *mut u128 {
    print_str("AOT TRAP! (host)\n");
    unsafe { syscall1(SYS_EXIT, 9); }
    loop {}
}

#[no_mangle]
pub extern "C" fn trap_unimplemented_fc(_ctx: &mut Ring3Context, _sp: *mut u128) -> *mut u128 {
    print_str("AOT TRAP! (unimplemented_fc)\n");
    unsafe { syscall1(SYS_EXIT, 10); }
    loop {}
}

#[no_mangle]
pub extern "C" fn trap_unimplemented_simd(_ctx: &mut Ring3Context, _sp: *mut u128) -> *mut u128 {
    print_str("AOT TRAP! (unimplemented_simd)\n");
    unsafe { syscall1(SYS_EXIT, 11); }
    loop {}
}

#[no_mangle]
pub extern "C" fn trap_unimplemented_atomic(_ctx: &mut Ring3Context, _sp: *mut u128) -> *mut u128 {
    print_str("AOT TRAP! (unimplemented_atomic)\n");
    unsafe { syscall1(SYS_EXIT, 12); }
    loop {}
}

/// Called when the AOT entry point function returns (via the return address we push on the stack).
/// Reads trap_code from the Ring3Context to determine exit code.
#[no_mangle]
pub extern "C" fn process_exit(ctx: *const Ring3Context) -> ! {
    let mut exit_code = 0;
    if !ctx.is_null() {
        unsafe {
            exit_code = *(*ctx).trap_code;
        }
    }
    unsafe { syscall1(SYS_EXIT, exit_code as u64); }
    loop {}
}
