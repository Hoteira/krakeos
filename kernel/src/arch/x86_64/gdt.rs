use crate::memory::paging::HHDM_OFFSET;
use core::arch::asm;
use core::sync::atomic::Ordering;

#[repr(C, packed)]
struct GdtDescriptor {
    size: u16,
    offset: u64,
}

/// Return the virtual base address of the BSP GDT (after high-half relocation).
/// Called by `smp.rs` before SIPI to let each AP copy the code/data entries.
pub fn bsp_gdt_base() -> u64 {
    unsafe {
        let mut gdtr = GdtDescriptor { size: 0, offset: 0 };
        asm!("sgdt [{}]", in(reg) &mut gdtr, options(nostack, preserves_flags));
        gdtr.offset
    }
}

/// Load the per-AP GDT published by the BSP in `AP_GDT_BASE`/`AP_GDT_LIMIT`,
/// then load the TSS at selector 0x28 (GDT slot 5).
/// Must be called early in `ap_entrance()` before `READY_COUNT` is incremented.
pub fn init_ap_gdt_tss() {
    use crate::arch::x86_64::smp::{AP_GDT_BASE, AP_GDT_LIMIT};
    let base  = AP_GDT_BASE.load(Ordering::SeqCst);
    let limit = AP_GDT_LIMIT.load(Ordering::SeqCst) as u16;
    unsafe {
        let gdtr = GdtDescriptor { size: limit, offset: base };
        asm!("lgdt [{}]", in(reg) &gdtr, options(nostack, preserves_flags));
        // Reload segment registers with kernel selectors.
        // GS and FS are intentionally NOT reloaded here — GS_BASE is managed
        // via WRMSR (0xC0000101) by init_per_cpu() to point to the per-CPU
        // CpuLocal struct.  Loading GS from a flat GDT descriptor would set
        // GS_BASE = 0 and break gs:[24] / gs:[32] accesses in the scheduler.
        asm!(
            "mov ax, 0x10",  // kernel data (GDT slot 2)
            "mov ds, ax",
            "mov es, ax",
            "mov ss, ax",
            out("ax") _,
            options(nostack, preserves_flags)
        );
        // Load TSS at selector 0x28 (GDT slot 5, RPL=0)
        asm!("ltr {:x}", in(reg) 0x28u16, options(nostack, preserves_flags));
    }
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
