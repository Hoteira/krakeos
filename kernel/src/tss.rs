use crate::boot::TaskStateSegment;


#[allow(dead_code)]
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

use crate::memory::{pmm, vmm, paging};

pub fn init_ists() {
    unsafe {
        // Find existing TSS base from GDT
        let tr: u16;
        core::arch::asm!("str {:x}", out(reg) tr);
        
        let mut gdt_ptr = Descriptor { size: 0, offset: 0 };
        core::arch::asm!("sgdt [{}]", in(reg) &mut gdt_ptr, options(nostack, preserves_flags));
        
        let gdt_base = gdt_ptr.offset;
        let tr_index = tr >> 3;
        
        // TSS descriptor is 16 bytes (2 entries)
        let tss_desc_low_ptr = (gdt_base + (tr_index as u64 * 8)) as *mut u64;
        let tss_desc_high_ptr = (gdt_base + (tr_index as u64 * 8) + 8) as *mut u64;
        
        let low = *tss_desc_low_ptr;
        let high = *tss_desc_high_ptr;
        
        let mut base = 0u64;
        base |= (low >> 16) & 0xFFFF;
        base |= ((low >> 32) & 0xFF) << 16;
        base |= ((low >> 56) & 0xFF) << 24;
        base |= (high & 0xFFFFFFFF) << 32;
        
        let tss_struct = base as *mut TaskStateSegment;

        // Allocate IST 1 (Double Fault)
        let ist1_frame = pmm::allocate_frame().expect("TSS: OOM for IST1");
        vmm::map_page(ist1_frame, ist1_frame, paging::PAGE_PRESENT | paging::PAGE_WRITABLE, None);
        (*tss_struct).ist1 = ist1_frame + 4096;

        // Allocate IST 2 (Page Fault)
        let ist2_frame = pmm::allocate_frame().expect("TSS: OOM for IST2");
        vmm::map_page(ist2_frame, ist2_frame, paging::PAGE_PRESENT | paging::PAGE_WRITABLE, None);
        (*tss_struct).ist2 = ist2_frame + 4096;
        
        crate::debugln!("[TSS] IST1 (DF) set to {:#x}", (*tss_struct).ist1 + 0);
        crate::debugln!("[TSS] IST2 (PF) set to {:#x}", (*tss_struct).ist2 + 0);
    }
}

pub fn set_tss(kernel_stack: u64) {
    unsafe {
        // We must lookup TSS every time if we don't store the pointer globally.
        // For performance, we could cache it, but let's be safe and lookup.
        // Actually, `init_ists` runs once. We can assume TR doesn't change.
        
        let tr: u16;
        core::arch::asm!("str {:x}", out(reg) tr);
        
        let mut gdt_ptr = Descriptor { size: 0, offset: 0 };
        core::arch::asm!("sgdt [{}]", in(reg) &mut gdt_ptr, options(nostack, preserves_flags));
        
        let gdt_base = gdt_ptr.offset;
        let tr_index = tr >> 3;
        
        let tss_desc_low_ptr = (gdt_base + (tr_index as u64 * 8)) as *mut u64;
        let tss_desc_high_ptr = (gdt_base + (tr_index as u64 * 8) + 8) as *mut u64;
        
        let low = *tss_desc_low_ptr;
        let high = *tss_desc_high_ptr;
        
        let mut base = 0u64;
        base |= (low >> 16) & 0xFFFF;
        base |= ((low >> 32) & 0xFF) << 16;
        base |= ((low >> 56) & 0xFF) << 24;
        base |= (high & 0xFFFFFFFF) << 32;
        
        let tss_struct = base as *mut TaskStateSegment;
        (*tss_struct).rsp0 = kernel_stack;
    }
}