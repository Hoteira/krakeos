use crate::context::Ring3Context;

#[no_mangle]
pub extern "C" fn global_get(ctx: &mut Ring3Context, idx: u32, out: *mut [u8; 16]) {
    unsafe {
        let ptr = ctx.globals_ptr.add(idx as usize * 16);
        core::ptr::copy_nonoverlapping(ptr, out as *mut u8, 16);
    }
}

#[no_mangle]
pub extern "C" fn global_set(ctx: &mut Ring3Context, idx: u32, data: *const [u8; 16]) {
    unsafe {
        let ptr = ctx.globals_ptr.add(idx as usize * 16);
        core::ptr::copy_nonoverlapping(data as *const u8, ptr, 16);
    }
}
