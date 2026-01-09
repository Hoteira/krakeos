#![feature(abi_x86_interrupt)]
#![feature(str_as_str)]
#![no_std]
#![no_main]


extern crate alloc;

pub mod boot;
mod interrupts;
mod drivers;
mod fs;
mod memory;
mod tss;
pub mod debug;
pub mod window_manager;
pub mod sync;

use crate::boot::{BootInfo, BOOT_INFO};
use crate::fs::ext2::fs::Ext2;
use crate::memory::pmm;
use core::arch::asm;
use window_manager::display::DISPLAY_SERVER;
use crate::interrupts::gdt::reload_gdt_high_half;

const EFER_MSR: u32 = 0xC0000080;
const STAR_MSR: u32 = 0xC0000081;
const LSTAR_MSR: u32 = 0xC0000082;
const SFMASK_MSR: u32 = 0xC0000084;
const PAT_MSR: u32 = 0x277;
use crate::memory::paging::{phys_to_virt, active_level_4_table};
use crate::memory::address::PhysAddr;

#[unsafe(no_mangle)]
#[unsafe(link_section = ".start")]
pub extern "C" fn _start(bootinfo_ptr: u64) -> ! {
    unsafe { asm!("cli"); }

    unsafe { *(&raw mut BOOT_INFO) = *(bootinfo_ptr as *const BootInfo); };

    unsafe {
        let rsp: u64;
        asm!("mov {}, rsp", out(reg) rsp);
        let new_rsp = rsp + crate::memory::paging::HHDM_OFFSET;
        asm!("mov rsp, {}", in(reg) new_rsp);
    }

    reload_gdt_high_half();

    debugln!("SIGNPOST: Initializing Memory...");
    memory::init();

    unsafe {
        let pml4 = active_level_4_table();
        pml4[0].set_unused();
        let cr3: u64;
        asm!("mov {}, cr3", out(reg) cr3);
        asm!("mov cr3, {}", in(reg) cr3);
    }

    debugln!("SIGNPOST: Initializing ISTs...");
    crate::tss::init_ists();

    debugln!("SIGNPOST: Loading IDT...");
    load_idt();

    debugln!("SIGNPOST: Kernel fully initialized.");

    let heap_size = 0xA0_0000;
    let heap_pages = heap_size / 4096;
    let heap_phys_addr = pmm::allocate_frames(heap_pages as usize, 0).expect("Failed to allocate heap memory from PMM");
    let heap_virt_ptr = phys_to_virt(PhysAddr::new(heap_phys_addr)).as_mut_ptr::<u8>();
    
    
    crate::memory::allocator::init_heap(heap_virt_ptr, heap_size as usize);

    debugln!("SIGNPOST: Heap initialized.");

    fs::dma::init();
    crate::fs::virtio::init();
    crate::fs::vfs::init();

    window_manager::events::GLOBAL_EVENT_QUEUE.lock().init();
    interrupts::task::TASK_MANAGER.lock().init();

    unsafe { (*(&raw mut DISPLAY_SERVER)).init(); }

    debugln!("SIGNPOST: Drivers initialized.");

    drivers::periferics::mouse::init_mouse();
    drivers::periferics::timer::init_pit(100);

    crate::debugln!("Mounting Ext2...");
    match Ext2::new(0xE0, 16384) {
        Ok(fs) => crate::fs::vfs::mount(0xE0, fs),
        Err(e) => { crate::debugln!("Failed to mount Ext2: {}", e); loop {} }
    }

    crate::debugln!("Spawning init process...");
    match crate::interrupts::syscalls::spawn_process("@0xE0/user.elf", None, None) {
        Ok(pid) => crate::debugln!("Init process spawned with PID {}", pid),
        Err(e) => { crate::debugln!("Failed to spawn init: {}", e); loop {} }
    }

    init_syscall_msrs();

    crate::debugln!("Kernel initialized, entering idle loop...");
    unsafe { asm!("sti"); }

    loop {
        unsafe { asm!("hlt"); }
    }
}

fn init_pat() {
    unsafe {
        let mut pat = rdmsr(PAT_MSR);
        pat &= !(0xFFu64 << 32);
        pat |= 0x01u64 << 32;
        wrmsr(PAT_MSR, pat);
        let cr3: u64;
        asm!("mov {}, cr3", out(reg) cr3);
        asm!("mov cr3, {}", in(reg) cr3);
    }
}

unsafe fn rdmsr(msr: u32) -> u64 {
    let (low, high): (u32, u32);
    unsafe { asm!("rdmsr", in("ecx") msr, out("eax") low, out("edx") high) };
    ((high as u64) << 32) | (low as u64)
}

unsafe fn wrmsr(msr: u32, value: u64) {
    let low = value as u32;
    let high = (value >> 32) as u32;
    unsafe { asm!("wrmsr", in("ecx") msr, in("eax") low, in("edx") high) };
}

fn init_syscall_msrs() {
    unsafe {
        let mut efer = rdmsr(EFER_MSR);
        efer |= 1;
        wrmsr(EFER_MSR, efer);
        let sysret_cs_base = 0x20;
        let syscall_cs_base = 0x08;
        let star_value = ((sysret_cs_base as u64) << 48) | ((syscall_cs_base as u64) << 32);
        wrmsr(STAR_MSR, star_value);
        wrmsr(LSTAR_MSR, interrupts::syscalls::syscall_entry as u64);
        let rflags_mask = (1 << 9) | (1 << 8);
        wrmsr(SFMASK_MSR, rflags_mask);
    }
}

pub fn load_idt() {
    unsafe {
        (*(&raw mut interrupts::idt::IDT)).init();
        (*(&raw mut interrupts::idt::IDT)).processor_exceptions();
        (*(&raw mut interrupts::idt::IDT)).hardware_interrupts();
        (*(&raw mut interrupts::idt::IDT)).load();
        (*(&raw mut interrupts::pic::PICS)).init();
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    crate::debugln!("KERNEL PANIC: {}", info);
    loop {}
}