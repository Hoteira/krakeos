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
pub extern "C" fn call_indirect(ctx: &mut Ring3Context, table_idx: u32, type_idx: u32, i: u32) -> usize {
    unsafe {
        // Assume table 0 for now as per plan
        if table_idx == 0 {
            if i >= ctx.table0_size { return 0; }
            let entry = *ctx.table0_ptr.add(i as usize);

            // Check for host function magic
            if (entry & 0xFFFFFFFF00000000) == 0xDEADC0DE00000000 {
                // Return CallHost trampoline entry in the jump table (table[67])
                let jt = ctx.blob_base as *const usize;
                return *jt.add(67);
            }

            return entry as usize;
        }
        0
    }
}
