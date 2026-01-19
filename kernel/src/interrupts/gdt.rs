use crate::memory::paging::HHDM_OFFSET;
use core::arch::asm;

#[repr(C, packed)]
struct GdtDescriptor {
    size: u16,
    offset: u64,
}

pub fn reload_gdt_high_half() {
    unsafe {
        let mut gdtr = GdtDescriptor { size: 0, offset: 0 };


        asm!("sgdt [{}]", in(reg) &mut gdtr, options(nostack, preserves_flags));

        let old_gdt_phys = gdtr.offset;
        let new_gdt_virt = old_gdt_phys + HHDM_OFFSET;


        let tr: u16;
        asm!("str {:x}", out(reg) tr);


        let tr_idx = (tr >> 3) as usize;
        let gdt_ptr = new_gdt_virt as *mut u64;

        let tss_low_ptr = gdt_ptr.add(tr_idx);
        let tss_high_ptr = gdt_ptr.add(tr_idx + 1);

        let mut low = *tss_low_ptr;
        let high = *tss_high_ptr;


        let mut tss_base_phys = 0u64;
        tss_base_phys |= (low >> 16) & 0xFFFF;
        tss_base_phys |= ((low >> 32) & 0xFF) << 16;
        tss_base_phys |= ((low >> 56) & 0xFF) << 24;
        tss_base_phys |= high << 32;

        let tss_base_virt = tss_base_phys + HHDM_OFFSET;


        low &= 0x00FFFF000000FFFF;


        low |= (tss_base_virt & 0xFFFF) << 16;
        low |= ((tss_base_virt >> 16) & 0xFF) << 32;
        low |= ((tss_base_virt >> 24) & 0xFF) << 56;


        low &= !(1 << 41);


        let new_high = tss_base_virt >> 32;

        *tss_low_ptr = low;
        *tss_high_ptr = new_high;


        gdtr.offset = new_gdt_virt;


        asm!("lgdt [{}]", in(reg) &gdtr, options(nostack, preserves_flags));
        asm!("ltr {:x}", in(reg) tr, options(nostack, preserves_flags));
    }
}
