use crate::boot::TaskStateSegment;
use core::sync::atomic::{AtomicU64, Ordering};

pub static mut BASE_TSS: TaskStateSegment = TaskStateSegment {
    reserved1: 0,
    rsp0: 0,
    rsp1: 0,
    rsp2: 0,
    reserved2: 0,
    ist1: 0,
    ist2: 0,
    ist3: 0,
    ist4: 0,
    ist5: 0,
    ist6: 0,
    ist7: 0,
    reserved3: 0,
    reserved4: 0,
    iopb_offset: 0,
};

#[repr(C, packed)]
struct Descriptor {
    size: u16,
    offset: u64,
}

pub fn set_tss(kernel_stack: u64) {
    unsafe {
        // We know where the TSS is because we statically allocated it in `swiftboot` logic or here?
        // Actually, `swiftboot` creates its own GDT and TSS.
        // The Kernel just inherits it.
        // BUT, `kernel/src/boot.rs` says `pub tss: u16`. That's the Selector.
        // We don't know the address unless we parse the GDT (which we do below).
        // OR we can rely on `BASE_TSS` if we reloaded the GDT (which we reverted).
        
        // Since we reverted GDT reloading, we are using the Bootloader's GDT and TSS.
        // We MUST find the Bootloader's TSS address via SGDT/STR logic.
        
        // 1. Get the current Task Register (TR) selector
        let tr: u16;
        core::arch::asm!("str {:x}", out(reg) tr);
        
        // 2. Get the GDT base address
        let mut gdt_ptr = Descriptor { size: 0, offset: 0 };
        core::arch::asm!("sgdt [{}]", in(reg) &mut gdt_ptr, options(nostack, preserves_flags));
        
        let gdt_base = gdt_ptr.offset;
        let tr_index = tr >> 3; // Selector index (TR / 8)
        
        let tss_desc_low_ptr = (gdt_base + (tr_index as u64 * 8)) as *mut u64;
        let tss_desc_high_ptr = (gdt_base + (tr_index as u64 * 8) + 8) as *mut u64;
        
        let low = *tss_desc_low_ptr;
        let high = *tss_desc_high_ptr;
        
        let mut base = 0u64;
        base |= (low >> 16) & 0xFFFF;          // Base 0-15
        base |= ((low >> 32) & 0xFF) << 16;    // Base 16-23
        base |= ((low >> 56) & 0xFF) << 24;    // Base 24-31
        base |= (high & 0xFFFFFFFF) << 32;     // Base 32-63
        
        // 5. Cast to TSS struct and update RSP0
        let tss_struct = base as *mut TaskStateSegment;
        (*tss_struct).rsp0 = kernel_stack;
    }
}