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

use crate::memory::pmm;

pub fn init_ists() {
    unsafe {
        let tr: u16;
        core::arch::asm!("str {:x}", out(reg) tr);

        let mut gdt_ptr = Descriptor { size: 0, offset: 0 };
        core::arch::asm!("sgdt [{}]", in(reg) &mut gdt_ptr, options(nostack, preserves_flags));

        let gdt_base = gdt_ptr.offset;
        let tr_index = tr >> 3;

        let tss_desc_low_ptr = (gdt_base + (tr_index as u64 * 8)) as *mut u64;
        let tss_desc_high_ptr = (gdt_base + (tr_index as u64 * 8) + 8) as *mut u64;

        let low = core::ptr::read_unaligned(tss_desc_low_ptr);
        let high = core::ptr::read_unaligned(tss_desc_high_ptr);

        let mut base = 0u64;
        base |= (low >> 16) & 0xFFFF;
        base |= ((low >> 32) & 0xFF) << 16;
        base |= ((low >> 56) & 0xFF) << 24;
        base |= (high & 0xFFFFFFFF) << 32;

        let tss_struct = base as *mut TaskStateSegment;

        let ist1_frame = pmm::allocate_frame(0).expect("TSS: OOM for IST1");
        (*tss_struct).ist1 = ist1_frame + 4096 + crate::memory::paging::HHDM_OFFSET;

        let ist2_frame = pmm::allocate_frame(0).expect("TSS: OOM for IST2");
        (*tss_struct).ist2 = ist2_frame + 4096 + crate::memory::paging::HHDM_OFFSET;

        let ist3_frame = pmm::allocate_frame(0).expect("TSS: OOM for IST3");
        (*tss_struct).ist3 = ist3_frame + 4096 + crate::memory::paging::HHDM_OFFSET;
    }
}


pub fn set_tss(kernel_stack: u64) {
    unsafe {
        let tr: u16;
        core::arch::asm!("str {:x}", out(reg) tr);

        let mut gdt_ptr = Descriptor { size: 0, offset: 0 };
        core::arch::asm!("sgdt [{}]", in(reg) &mut gdt_ptr, options(nostack, preserves_flags));

        let gdt_base = gdt_ptr.offset;
        let tr_index = tr >> 3;

        let tss_desc_low_ptr = (gdt_base + (tr_index as u64 * 8)) as *mut u64;
        let tss_desc_high_ptr = (gdt_base + (tr_index as u64 * 8) + 8) as *mut u64;

        let low = core::ptr::read_unaligned(tss_desc_low_ptr);
        let high = core::ptr::read_unaligned(tss_desc_high_ptr);

        let mut base = 0u64;
        base |= (low >> 16) & 0xFFFF;
        base |= ((low >> 32) & 0xFF) << 16;
        base |= ((low >> 56) & 0xFF) << 24;
        base |= (high & 0xFFFFFFFF) << 32;

        let tss_struct = base as *mut TaskStateSegment;
        (*tss_struct).rsp0 = kernel_stack;
    }
}