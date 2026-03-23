use crate::context::Ring3Context;
use crate::syscall::{syscall1, syscall2};

const SYS_MEMORY_GROW: u64 = 200;
const SYS_MEMORY_SIZE: u64 = 201;

#[no_mangle]
pub extern "C" fn memory_size(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let res = syscall1(SYS_MEMORY_SIZE, 0);
        let result_sp = sp.sub(1);
        *result_sp = res as u128;
        result_sp
    }
}

#[no_mangle]
pub extern "C" fn memory_grow(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let msg: [u8; 5] = [b'M', b'G', b'R', b'W', b'\n'];
        crate::syscall::syscall3(999, msg.as_ptr() as u64, 5, 0);
        let n = (*sp.add(0)) as u32;
        let res = syscall2(SYS_MEMORY_GROW, n as u64, 0);

        let result_sp = sp.add(1).sub(1);
        *result_sp = res as u128;

        // Update memory_size in context
        if res != u64::MAX {
            let new_size = (res as usize + n as usize) * 65536;
            ctx.memory_size = new_size;
            // Debug: print old and new size via serial
            let msg = b"R3 mem_grow: new_size=";
            crate::syscall::syscall3(999, msg.as_ptr() as u64, msg.len() as u64, 0);
            let hex = b"0123456789abcdef";
            let mut buf = [b'0'; 16];
            for i in 0..16 {
                buf[i] = hex[((new_size as u64 >> (60 - i * 4)) & 0xf) as usize];
            }
            crate::syscall::syscall3(999, buf.as_ptr() as u64, 16, 0);
            let msg2 = b" ctx.mem_size=";
            crate::syscall::syscall3(999, msg2.as_ptr() as u64, msg2.len() as u64, 0);
            let val = ctx.memory_size as u64;
            for i in 0..16 {
                buf[i] = hex[((val >> (60 - i * 4)) & 0xf) as usize];
            }
            crate::syscall::syscall3(999, buf.as_ptr() as u64, 16, 0);
            crate::syscall::syscall3(999, b"\n".as_ptr() as u64, 1, 0);
        }

        result_sp
    }
}

/// C ABI: called by AOT compiler with individual register args (RDI=ctx, RSI=d, RDX=s, RCX=n)
#[no_mangle]
pub extern "C" fn memory_copy(ctx: &Ring3Context, d: i32, s: i32, n: u32) {
    unsafe {
        let src = ctx.memory_base.add(s as usize);
        let dst = ctx.memory_base.add(d as usize);
        core::ptr::copy(src, dst, n as usize);
    }
}

/// C ABI: called by AOT compiler with individual register args (RDI=ctx, RSI=d, RDX=val, RCX=n)
#[no_mangle]
pub extern "C" fn memory_fill(ctx: &Ring3Context, d: i32, val: u32, n: u32) {
    unsafe {
        let dst = ctx.memory_base.add(d as usize);
        core::ptr::write_bytes(dst, val as u8, n as usize);
    }
}
