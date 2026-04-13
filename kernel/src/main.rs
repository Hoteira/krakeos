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

    fs::dma::init();
    crate::fs::virtio::init();
    crate::fs::vfs::init();

    window_manager::events::GLOBAL_EVENT_QUEUE.lock().init();
    task::init();
    task::aot_worker::init();
    debugln!("SIGNPOST: TaskManager initialized.");

    debugln!("SIGNPOST: Calling DISPLAY_SERVER.init()...");
    DISPLAY_SERVER.lock().init();
    debugln!("SIGNPOST: DISPLAY_SERVER initialized.");
    DISPLAY_SERVER.lock().force_full_sync(true);

    debugln!("SIGNPOST: Drivers initialized.");

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
        arch::x86_64::apic::set_irq(12, 44);
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

    crate::debugln!("Mounting Ext2...");
    match Ext2::new(0xE0, 16384) {
        Ok(fs) => crate::fs::vfs::mount(0xE0, fs),
        Err(e) => {
            crate::debugln!("Failed to mount Ext2: {}", e);
            loop {}
        }
    }

    crate::debugln!("Spawning trivial test process (WASM)...");
    match crate::syscalls::spawn_process("/sys/bin/init.wasm", None, None, None, false) {
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
    crate::debugln!("[KERNEL PANIC] >> {}", info);
    loop {}
}
