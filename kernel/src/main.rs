#![feature(abi_x86_interrupt)]
#![feature(str_as_str)]
#![no_std]
#![no_main]

extern crate alloc;

pub mod arch;
pub mod boot;
pub mod debug;
pub mod drivers;
pub mod fs;
pub mod memory;
pub mod net;
pub mod sync;
pub mod syscalls;
pub mod task;

use crate::boot::{BOOT_INFO, BootInfo};
use crate::fs::ext2::fs::Ext2;
use crate::memory::address::PhysAddr;
use crate::memory::paging::phys_to_virt;
use crate::memory::pmm;
use core::arch::asm;
use window_manager::display::DISPLAY_SERVER;

pub mod window_manager;

#[global_allocator]
static ALLOCATOR: std::allocator::Allocator = std::allocator::Allocator::new();

#[unsafe(naked)]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".start")]
pub unsafe extern "C" fn _start() -> ! {
    core::arch::naked_asm!(
        "cli",
        "mov rax, rsp",
        "mov rcx, 0xFFFF800000000000",
        "add rax, rcx",
        "and rax, -16",
        "mov rsp, rax",
        "call rust_main",
        "ud2"
    );
}

#[unsafe(no_mangle)]
pub extern "C" fn rust_main(bootinfo_ptr: u64) -> ! {
    unsafe {
        *(&raw mut BOOT_INFO) = *(bootinfo_ptr as *const BootInfo);
    };

    arch::x86_64::gdt::reload_gdt_high_half();

    // Enable SSE/SSE2 on the BSP — must happen before any heap use,
    // because the allocator and memcpy may emit movdqu/movaps.
    arch::x86_64::init_fpu();

    // Initialize per-CPU state for BSP (CPU 0)
    task::cpu::init_per_cpu(0, 0);

    debugln!("SIGNPOST: Initializing Memory...");
    memory::init();
    memory::pmm::discover_all_memory();

    debugln!("SIGNPOST: Initializing ISTs...");
    arch::x86_64::tss::init_ists();

    debugln!("SIGNPOST: Loading IDT...");
    load_idt();

    debugln!("SIGNPOST: Kernel fully initialized.");

    let heap_size = 0x400_0000; // 64 MiB
    let heap_pages = heap_size / 4096;
    let heap_phys_addr =
        pmm::allocate_frames(heap_pages as usize).expect("Failed to allocate heap memory from PMM");
    let heap_virt_ptr = phys_to_virt(PhysAddr::new(heap_phys_addr)).as_mut_ptr::<u8>();

    ALLOCATOR.init(heap_virt_ptr, heap_size as usize);

    debugln!("SIGNPOST: Heap initialized.");

    // Enumerate all PCI devices and build the device table before driver inits.
    drivers::registry::enumerate();

    fs::dma::init();
    crate::fs::virtio::init();
    if crate::fs::virtio::is_active() {
        drivers::registry::set_active(drivers::registry::DriverKind::VirtioBlock);
    }
    crate::fs::vfs::init();

    window_manager::events::GLOBAL_EVENT_QUEUE.lock().init();
    task::init();
    task::aot_worker::init();
    window_manager::render_worker::init();
    debugln!("SIGNPOST: TaskManager initialized.");

    debugln!("SIGNPOST: Calling DISPLAY_SERVER.init()...");
    DISPLAY_SERVER.lock().init();
    debugln!("SIGNPOST: DISPLAY_SERVER initialized.");
    DISPLAY_SERVER.lock().force_full_sync(true);
    drivers::registry::set_active(drivers::registry::DriverKind::VirtioGpu);

    debugln!("SIGNPOST: Drivers initialized.");

    // Init USB host controller — must come before PS/2 init so HID devices
    // are registered first; PS/2 drivers check has_keyboard/has_pointer().
    drivers::usb::xhci::init();

    drivers::peripherals::keyboard::init();
    drivers::peripherals::mouse::init_mouse();

    debugln!("SIGNPOST: Initializing APIC...");
    if let Some(madt) = arch::x86_64::acpi::get_madt() {
        arch::x86_64::apic::init(madt);
        unsafe {
            arch::x86_64::USING_APIC = true;
            // Mask the legacy PIC
            (*(&raw mut arch::x86_64::pic::PICS)).master.write_data(0xFF);
            (*(&raw mut arch::x86_64::pic::PICS)).slave.write_data(0xFF);
        }
        // Setup IOAPIC IRQs: 0/2=Timer, 1=Keyboard, 12=Mouse
        arch::x86_64::apic::set_irq(0, 32);
        arch::x86_64::apic::set_irq(2, 32);
        arch::x86_64::apic::set_irq(1, 33);
        arch::x86_64::apic::set_irq(11, crate::arch::x86_64::exceptions::NET_INT); // VirtIO Net
        arch::x86_64::apic::set_irq(12, 44);
        // VirtIO Block: route the device's ACTUAL INTx line (captured at init). QEMU
        // assigns this device IRQ 11, not the legacy 10 — the old hardcoded route sent
        // completions to the net handler, so the disk ISR never fired. Routed last so
        // it wins if it happens to share IRQ 11 with the (usually-absent) net device.
        let blk_irq = crate::fs::virtio::irq_line();
        if blk_irq != 0xFF {
            arch::x86_64::apic::set_irq(blk_irq, crate::fs::virtio::BLK_INT_VEC);
        }
    }

    // VirtIO network (via the virtio-drivers crate). Absent in the default QEMU
    // config, in which case init() returns Err and the system continues without net.
    match crate::drivers::network::virtio::init() {
        Ok(()) => println!("[net] VirtIO net online"),
        Err(e) => debugln!("[net] {}", e),
    }

    debugln!("SIGNPOST: Initializing HPET...");
    let mut hpet_initialized = false;
    if let Some(hpet) = arch::x86_64::acpi::get_hpet() {
        if drivers::peripherals::hpet::init(hpet) {
            hpet_initialized = true;
        }
    }

    if !hpet_initialized {
        drivers::peripherals::timer::init_pit(1000);
    }

    // Calibrate LAPIC timer now that HPET (or PIT fallback) is ready.
    // APs will call init_lapic_timer(32) with this value to get their own preemption clock.
    arch::x86_64::apic::calibrate_lapic_timer();

    // Boot Application Processors.  Must come AFTER LAPIC calibration so each
    // AP can call init_lapic_timer() with the BSP-measured tick count.
    // Must come AFTER task::init() so APs can find tasks in the run queues.
    if let Some(madt) = arch::x86_64::acpi::get_madt() {
        arch::x86_64::smp::init(madt);
    }

    crate::debugln!("Mounting Ext2...");
    match Ext2::new(0xE0, 16384) {
        Ok(fs) => crate::fs::vfs::mount(0xE0, fs),
        Err(e) => {
            crate::debugln!("Failed to mount Ext2: {}", e);
            loop {}
        }
    }

    crate::debugln!("Spawning trivial test process (WASM)...");
    match crate::syscalls::spawn_process("/sys/bin/init.wasm", None, None, None, true) {
        Ok(pid) => crate::debugln!("Trivial test process spawned with PID {}", pid),
        Err(e) => {
            crate::debugln!("Failed to spawn trivial test: {}", e);
            loop {}
        }
    }

    arch::x86_64::init_syscall_msrs();

    crate::debugln!("Kernel initialized, entering idle loop...");
    unsafe {
        asm!("sti");
    }

    loop {
        unsafe {
            asm!("int 0x81");
        }
        unsafe {
            asm!("hlt");
        }
    }
}

pub fn load_idt() {
    unsafe {
        (*(&raw mut arch::x86_64::idt::IDT)).init();
        (*(&raw mut arch::x86_64::idt::IDT)).processor_exceptions();
        (*(&raw mut arch::x86_64::idt::IDT)).hardware_interrupts();
        (*(&raw mut arch::x86_64::idt::IDT)).load();
        (*(&raw mut arch::x86_64::pic::PICS)).init();
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    let (file, line) = if let Some(loc) = info.location() {
        (loc.file(), loc.line())
    } else {
        ("unknown", 0)
    };
    crate::debugln!("[KERNEL PANIC] File: {}, Line: {}", file, line);
    crate::debugln!("Message: {}", info);
    loop {}
}
