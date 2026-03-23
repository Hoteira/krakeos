use crate::context::Ring3Context;
use crate::syscall::{syscall1, syscall3};

const SYS_WRITE: u64 = 1;
const SYS_READ: u64 = 0;
const SYS_CLOSE: u64 = 3;
const SYS_LSEEK: u64 = 8;
const SYS_FSTAT: u64 = 5;

#[no_mangle]
pub extern "C" fn wasi_fd_write(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let nwritten_ptr = (*sp.add(0)) as u32;
        let iovs_len     = (*sp.add(1)) as u32;
        let iovs_offset  = (*sp.add(2)) as u32;
        let fd            = (*sp.add(3)) as i32;

        let mem = ctx.memory_base;
        let mut total_written: u32 = 0;

        for i in 0..iovs_len {
            let iov_addr = mem.add((iovs_offset + i * 8) as usize);
            let buf_offset = core::ptr::read_unaligned(iov_addr as *const u32);
            let buf_len    = core::ptr::read_unaligned(iov_addr.add(4) as *const u32);

            let buf_ptr = mem.add(buf_offset as usize);
            let ret = syscall3(SYS_WRITE, fd as u64, buf_ptr as u64, buf_len as u64);

            if ret > buf_len as u64 {
                // Error (ret is u64::MAX or similar)
                let result_sp = sp.add(4).sub(1);
                *result_sp = 8u128; // EBADF or similar
                return result_sp;
            }
            total_written += ret as u32;
        }

        let nwritten_addr = mem.add(nwritten_ptr as usize) as *mut u32;
        core::ptr::write_unaligned(nwritten_addr, total_written);

        let result_sp = sp.add(4).sub(1);
        *result_sp = 0u128; // SUCCESS
        result_sp
    }
}

#[no_mangle]
pub extern "C" fn wasi_fd_read(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let nread_ptr = (*sp.add(0)) as u32;
        let iovs_len  = (*sp.add(1)) as u32;
        let iovs_offset = (*sp.add(2)) as u32;
        let fd        = (*sp.add(3)) as i32;

        let mem = ctx.memory_base;
        let mut total_read: u32 = 0;

        for i in 0..iovs_len {
            let iov_addr = mem.add((iovs_offset + i * 8) as usize);
            let buf_offset = core::ptr::read_unaligned(iov_addr as *const u32);
            let buf_len    = core::ptr::read_unaligned(iov_addr.add(4) as *const u32);

            let buf_ptr = mem.add(buf_offset as usize);
            let ret = syscall3(SYS_READ, fd as u64, buf_ptr as u64, buf_len as u64);

            if ret > buf_len as u64 {
                let result_sp = sp.add(4).sub(1);
                *result_sp = 8u128;
                return result_sp;
            }
            total_read += ret as u32;
            if ret < buf_len as u64 { break; }
        }

        let nread_addr = mem.add(nread_ptr as usize) as *mut u32;
        core::ptr::write_unaligned(nread_addr, total_read);

        let result_sp = sp.add(4).sub(1);
        *result_sp = 0u128;
        result_sp
    }
}

#[no_mangle]
pub extern "C" fn wasi_fd_close(_ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let fd = (*sp.add(0)) as i32;
        let res = syscall1(SYS_CLOSE, fd as u64);
        let result_sp = sp.add(1).sub(1);
        *result_sp = (if res == u64::MAX { 8 } else { 0 }) as u128;
        result_sp
    }
}

#[no_mangle]
pub extern "C" fn call_host_dispatch(ctx: &mut Ring3Context, sp: *mut u128, idx: u64) -> *mut u128 {
    if idx >= ctx.num_imported_funcs as u64 {
        // Trap or return error
        unsafe {
            let result_sp = sp.sub(1);
            *result_sp = 1u128; // Generic error
            return result_sp;
        }
    }

    unsafe {
        let stub_idx = *ctx.import_stub_table.add(idx as usize);

        // u64::MAX sentinel = forward to kernel via SYS_WASM_HOST_CALL
        if stub_idx == u64::MAX {
            let new_sp = syscall3(
                300, // SYS_WASM_HOST_CALL
                ctx as *mut Ring3Context as u64,
                idx,
                sp as u64,
            );
            return new_sp as *mut u128;
        }

        let blob_base = ctx.blob_base;

        // The jump table at blob_base contains fixed-up absolute addresses
        let jump_table = blob_base as *const u64;
        let stub_addr = *jump_table.add(stub_idx as usize);

        if stub_addr == 0 {
             let result_sp = sp.sub(1);
             *result_sp = 1u128;
             return result_sp;
        }

        // Perform the call to the stub
        let stub_fn: unsafe extern "C" fn(&mut Ring3Context, *mut u128) -> *mut u128 = core::mem::transmute(stub_addr);
        stub_fn(ctx, sp)
    }
}

#[no_mangle]
pub extern "C" fn wasi_serial_print(ctx: &mut Ring3Context, sp: *mut u128) -> *mut u128 {
    unsafe {
        let len = (*sp.add(0)) as usize;
        let ptr = (*sp.add(1)) as u32;
        
        let mem = ctx.memory_base;
        let buf_ptr = mem.add(ptr as usize);
        
        crate::syscall::syscall3(999, buf_ptr as u64, len as u64, 0);
        
        // WASM serial_print: (ptr, len) -> void. Returns 0 results.
        let result_sp = sp.add(2);
        result_sp
    }
}
