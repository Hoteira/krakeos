use crate::arch::x86_64::acpi::Madt;
use crate::arch::x86_64::apic;
use crate::boot::TaskStateSegment;
use crate::memory::paging::HHDM_OFFSET;
use crate::memory::pmm;
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use crate::debugln;

pub static CPU_COUNT: AtomicUsize = AtomicUsize::new(1);
pub static READY_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Per-AP GDT base virtual address, written by BSP in boot_ap() before SIPI,
/// read by the AP in init_ap_gdt_tss() before READY_COUNT is incremented.
/// Sequential AP boot makes this race-free.
pub static AP_GDT_BASE: AtomicU64 = AtomicU64::new(0);
/// Per-AP GDT limit (size - 1), paired with AP_GDT_BASE.
pub static AP_GDT_LIMIT: AtomicU64 = AtomicU64::new(0);

// Address where trampoline will be placed (0x8000)
const TRAMPOLINE_ADDR: u64 = 0x8000;

#[repr(C, packed)]
struct TrampolineArgs {
    pml4: u32,
    entry_point: u64,
    stack_ptr: u64,
}

pub fn init(madt_ptr: u64) {
    let madt = unsafe { &*(madt_ptr as *const Madt) };

    // 1. Prepare trampoline in low memory (0x8000)
    setup_trampoline();

    // 2. Discover APs and boot them
    let mut offset = core::mem::size_of::<Madt>();
    while offset < madt.header.length as usize {
        let entry_ptr = (madt_ptr + offset as u64) as *const u8;
        let entry_type = unsafe { *entry_ptr };
        let entry_len = unsafe { *entry_ptr.add(1) };

        if entry_type == 0 { // Processor Local APIC
            let lapic_id = unsafe { *entry_ptr.add(3) };
            let flags = unsafe { core::ptr::read_unaligned(entry_ptr.add(4) as *const u32) };

            let is_enabled = (flags & 1) != 0;
            if is_enabled && lapic_id != 0 {
                boot_ap(lapic_id);
            }
        }
        offset += entry_len as usize;
    }

    debugln!("SMP: Booted {} total cores.", CPU_COUNT.load(Ordering::SeqCst));
}

fn setup_trampoline() {
    // Machine code: 16-bit Real Mode -> 32-bit Protected Mode -> 64-bit Long Mode
    // Loads system page tables, then calls ap_entrance via stack data.
    let trampoline_code: [u8; 133] = [
        0xfa, 0xfc, 0x31, 0xc0, 0x8e, 0xd8, 0x8e, 0xc0, 0x8e, 0xd0, 0x0f, 0x01,
        0x16, 0xa0, 0x80, 0x0f, 0x20, 0xc0, 0x66, 0x83, 0xc8, 0x01, 0x0f, 0x22,
        0xc0, 0x66, 0xea, 0x21, 0x80, 0x00, 0x00, 0x08, 0x00, 0x66, 0xb8, 0x10,
        0x00, 0x8e, 0xd8, 0x8e, 0xc0, 0x8e, 0xd0, 0x0f, 0x20, 0xe0, 0x83, 0xc8,
        0x20, 0x0f, 0x22, 0xe0, 0xa1, 0xc0, 0x80, 0x00, 0x00, 0x0f, 0x22, 0xd8,
        0xb9, 0x80, 0x00, 0x00, 0xc0, 0x0f, 0x32, 0x0d, 0x00, 0x01, 0x00, 0x00,
        0x0f, 0x30, 0x0f, 0x20, 0xc0, 0x0d, 0x00, 0x00, 0x00, 0x80, 0x0f, 0x22,
        0xc0, 0x0f, 0x01, 0x15, 0xb0, 0x80, 0x00, 0x00, 0xea, 0x63, 0x80, 0x00,
        0x00, 0x18, 0x00, 0x66, 0xb8, 0x00, 0x00, 0x8e, 0xd8, 0x8e, 0xc0, 0x8e,
        0xd0, 0x8e, 0xe0, 0x8e, 0xe8, 0x48, 0x8b, 0x24, 0x25, 0xc8, 0x80, 0x00,
        0x00, 0x48, 0x8b, 0x04, 0x25, 0xd0, 0x80, 0x00, 0x00, 0xff, 0xd0, 0xeb,
        0xfe
    ];

    unsafe {
        let dest = (TRAMPOLINE_ADDR + HHDM_OFFSET) as *mut u8;
        core::ptr::copy_nonoverlapping(trampoline_code.as_ptr(), dest, trampoline_code.len());

        let cr3: u64;
        core::arch::asm!("mov {}, cr3", out(reg) cr3);
        core::ptr::write_volatile((TRAMPOLINE_ADDR + 0xC0 + HHDM_OFFSET) as *mut u32, cr3 as u32);

        core::ptr::write_volatile((TRAMPOLINE_ADDR + 0xD0 + HHDM_OFFSET) as *mut u64, ap_entrance as u64);

        setup_temp_gdts();

        crate::memory::vmm::map_page(
            TRAMPOLINE_ADDR,
            crate::PhysAddr::new(TRAMPOLINE_ADDR),
            crate::memory::paging::PAGE_PRESENT | crate::memory::paging::PAGE_WRITABLE,
            None
        );
    }
}

fn setup_temp_gdts() {
    unsafe {
        let gdt_base = TRAMPOLINE_ADDR + 0x100 + HHDM_OFFSET;
        let gdt_ptr = gdt_base as *mut u64;

        *gdt_ptr.add(0) = 0;                        // Null
        *gdt_ptr.add(1) = 0x00CF9A000000FFFF;       // Code 32 (0x08)
        *gdt_ptr.add(2) = 0x00CF92000000FFFF;       // Data 32 (0x10)
        *gdt_ptr.add(3) = 0x00209A0000000000;       // Code 64 (0x18)

        // 16-bit GDT Descriptor at 0x80A0
        let desc16 = (TRAMPOLINE_ADDR + 0xA0 + HHDM_OFFSET) as *mut u16;
        *desc16 = 31;
        core::ptr::write_unaligned(desc16.add(1) as *mut u32, (TRAMPOLINE_ADDR + 0x100) as u32);

        // 64-bit GDT Descriptor at 0x80B0
        let desc64 = (TRAMPOLINE_ADDR + 0xB0 + HHDM_OFFSET) as *mut u16;
        *desc64 = 31;
        core::ptr::write_unaligned(desc64.add(1) as *mut u64, TRAMPOLINE_ADDR + 0x100);
    }
}

/// Build a 64-bit system segment descriptor (16 bytes = two GDT slots) for a TSS.
/// Returns (low_u64, high_u64) to write at GDT[idx] and GDT[idx+1].
fn make_tss_descriptor(base: u64, limit: u32) -> (u64, u64) {
    let limit = limit as u64;
    let low = (limit & 0xFFFF)                          // limit[15:0]  at bits 15:0
        | ((base & 0xFFFF) << 16)                       // base[15:0]   at bits 31:16
        | (((base >> 16) & 0xFF) << 32)                 // base[23:16]  at bits 39:32
        | (0x89u64 << 40)                               // P=1, DPL=0, S=0, Type=0x9 (available 64-bit TSS)
        | (((limit >> 16) & 0xF) << 48)                 // limit[19:16] at bits 51:48
        | (((base >> 24) & 0xFF) << 56);                // base[31:24]  at bits 63:56
    let high = base >> 32;                              // base[63:32]  at bits 31:0 of high word
    (low, high)
}

fn boot_ap(lapic_id: u8) {
    // 1. Allocate kernel stack for this AP
    let stack_phys = pmm::allocate_frames(16).expect("SMP: OOM for AP stack");
    let stack_top = stack_phys + (16 * 4096) + HHDM_OFFSET;

    unsafe {
        core::ptr::write_volatile((TRAMPOLINE_ADDR + 0xC8 + HHDM_OFFSET) as *mut u64, stack_top);
    }

    // 2. Allocate a per-AP GDT (one page) and copy the BSP's code/data segment entries
    let ap_gdt_phys = pmm::allocate_frame().expect("SMP: OOM for AP GDT");
    let ap_gdt_virt = ap_gdt_phys + HHDM_OFFSET;

    let bsp_gdt_base = crate::arch::x86_64::gdt::bsp_gdt_base();

    // Copy entries 0-4: null, kernel_code_64, kernel_data, user_data, user_code_64
    unsafe {
        core::ptr::copy_nonoverlapping(
            bsp_gdt_base as *const u64,
            ap_gdt_virt as *mut u64,
            5,
        );
    }

    // 3. Allocate and initialise a per-AP TSS
    let tss_phys = pmm::allocate_frame().expect("SMP: OOM for AP TSS");
    let tss_virt = tss_phys + HHDM_OFFSET;

    unsafe {
        core::ptr::write_bytes(tss_virt as *mut u8, 0, core::mem::size_of::<TaskStateSegment>());
        let tss = tss_virt as *mut TaskStateSegment;

        // RSP0 is updated by set_tss() on every task-switch to a user thread;
        // initialise to the AP's own kernel stack so the first interrupt is safe.
        (*tss).rsp0 = stack_top;

        // iopb_offset past end-of-TSS means "no IOPB" (all ports kernel-only)
        (*tss).iopb_offset = core::mem::size_of::<TaskStateSegment>() as u16;

        // IST stacks: IST1 = #DF (IDT entry 8), IST3 = #GP (IDT entry 13)
        let ist1 = pmm::allocate_frame().expect("SMP: OOM for AP IST1") + 4096 + HHDM_OFFSET;
        let ist2 = pmm::allocate_frame().expect("SMP: OOM for AP IST2") + 4096 + HHDM_OFFSET;
        let ist3 = pmm::allocate_frame().expect("SMP: OOM for AP IST3") + 4096 + HHDM_OFFSET;
        (*tss).ist1 = ist1;
        (*tss).ist2 = ist2;
        (*tss).ist3 = ist3;

        // Write 16-byte TSS descriptor at GDT slot 5 (selector 0x28)
        let tss_limit = (core::mem::size_of::<TaskStateSegment>() - 1) as u32;
        let (desc_low, desc_high) = make_tss_descriptor(tss_virt, tss_limit);
        let gdt_entries = ap_gdt_virt as *mut u64;
        *gdt_entries.add(5) = desc_low;
        *gdt_entries.add(6) = desc_high;
    }

    // Publish for init_ap_gdt_tss() — AP reads these before signalling READY_COUNT
    AP_GDT_BASE.store(ap_gdt_virt, Ordering::SeqCst);
    // 7 entries (0..=6) × 8 bytes; GDT limit = byte-size − 1 = 55
    AP_GDT_LIMIT.store(7 * 8 - 1, Ordering::SeqCst);

    // 4. IPI sequence: INIT → wait 10 ms → SIPI
    apic::send_ipi(lapic_id, 0x00000500); // INIT IPI
    for _ in 0..1000000 { core::hint::spin_loop(); }
    apic::send_ipi(lapic_id, 0x00000608); // SIPI (vector 0x08 → physical 0x8000)

    // 5. Wait for the AP to complete full initialisation (GDT, IDT, MSRs loaded)
    let timeout = 10_000_000;
    let mut success = false;
    for _ in 0..timeout {
        if READY_COUNT.load(Ordering::SeqCst) > CPU_COUNT.load(Ordering::SeqCst) - 1 {
            success = true;
            break;
        }
        core::hint::spin_loop();
    }

    if success {
        CPU_COUNT.fetch_add(1, Ordering::SeqCst);
    } else {
        debugln!("SMP: AP {} failed to boot!", lapic_id);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ap_entrance() -> ! {
    let id = apic::get_id();

    // Initialize per-CPU state (GS base) before doing anything else
    crate::task::cpu::init_per_cpu(id as u32, id);

    // Load the per-AP GDT and TSS BEFORE signalling READY_COUNT.
    // The BSP stores AP_GDT_BASE/LIMIT before the SIPI and advances to
    // the next AP as soon as READY_COUNT increments, so we must not
    // signal until we are done reading those statics.
    crate::arch::x86_64::gdt::init_ap_gdt_tss();

    // Load the shared IDT (read-only; safe to share across all CPUs)
    unsafe {
        crate::arch::x86_64::idt::IDT.load();
    }

    // Initialise per-CPU MSRs (NX, PAT, SYSCALL/SYSRET targets)
    crate::arch::x86_64::init_syscall_msrs();
    crate::arch::x86_64::init_pat();
    // crate::arch::x86_64::init_fpu(); // Stub

    crate::arch::x86_64::apic::enable_local_apic();

    // AP is fully initialised — signal the BSP
    READY_COUNT.fetch_add(1, Ordering::SeqCst);

    unsafe { core::arch::asm!("sti"); }

    debugln!("SMP: Core ID {} online.", id);

    // Idle: the BSP's timer broadcasts will preempt this hlt and drive schedule()
    loop {
        unsafe { core::arch::asm!("hlt"); }
    }
}
