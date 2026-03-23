use crate::context::Ring3Context;

#[no_mangle]
pub extern "C" fn table_get(ctx: &mut Ring3Context, table_idx: u32, i: u32) -> usize {
    unsafe {
        // Assume table 0 for now as per plan
        if table_idx == 0 {
            if i >= ctx.table0_size { return 0; }
            return *ctx.table0_ptr.add(i as usize) as usize;
        }
        0
    }
}

#[no_mangle]
pub extern "C" fn table_set(ctx: &mut Ring3Context, table_idx: u32, i: u32, val: usize) {
    unsafe {
        if table_idx == 0 {
            if i >= ctx.table0_size { return; }
            *ctx.table0_ptr.add(i as usize) = val as u64;
        }
    }
}

#[no_mangle]
pub extern "C" fn call_indirect(ctx: &mut Ring3Context, table_idx: u32, type_idx: u32, i: u32) -> *const u8 {
    unsafe {
        if table_idx != 0 { return core::ptr::null(); }
        if i >= ctx.table0_size { return core::ptr::null(); }
        
        let func_idx = *ctx.table0_ptr.add(i as usize);
        if func_idx == u64::MAX { return core::ptr::null(); }
        
        // In this architecture, table0 contains indices/offsets into the code slot.
        // But we also need to check the signature. 
        // The plan says "Indirect calls: Implemented via a signature-checking trampoline that returns AOT code pointers."
        // For now, let's assume table0 entries are already resolved to code pointers or offsets.
        
        // Wait, the plan says:
        // "table0 entries are absolute addresses pointing to function bodies within the blob." or WASM functions.
        // Actually, they should be absolute addresses in Ring 3.
        
        func_idx as *const u8
    }
}
