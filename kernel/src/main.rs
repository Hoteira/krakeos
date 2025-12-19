#![feature(abi_x86_interrupt)]
#![no_std]
#![no_main]

pub mod boot;
mod interrupts;
mod drivers;
mod fs;
mod memory;
mod tss;
pub mod debug;
pub mod window_manager;

extern crate alloc;

use crate::boot::{BootInfo, BOOT_INFO};
use crate::fs::vfs::FileSystem; 
use crate::fs::ext2::fs::Ext2; // Added Ext2 for font loading
use core::arch::asm;
use window_manager::display::DISPLAY_SERVER;
use crate::memory::pmm;

// MSRs for SYSCALL/SYSRET and PAT
const EFER_MSR: u32 = 0xC0000080; // Extended Feature Enable Register
const STAR_MSR: u32 = 0xC0000081; // SYSCALL Target Address Register
const LSTAR_MSR: u32 = 0xC0000082; // Long Mode SYSCALL Target Address Register
const SFMASK_MSR: u32 = 0xC0000084; // SYSCALL Flag Mask
const PAT_MSR: u32 = 0x277;       // Page Attribute Table MSR

#[unsafe(no_mangle)]
#[unsafe(link_section = ".start")]
pub extern "C" fn _start(bootinfo_ptr: *const BootInfo) -> ! {
    unsafe {
        asm!("cli" );
    }

    unsafe { 
        *(&raw mut BOOT_INFO) = bootinfo_ptr.read();
    };

    // Load IDT early to catch exceptions
    load_idt();

    // Initialize Memory Subsystem
    memory::init();
    
    // Allocate heap memory from PMM
    let heap_size = 0xA0_0000; // 16MB
    let heap_pages = heap_size / 4096;
    let heap_phys_addr = pmm::allocate_frames(heap_pages, 0 /* kernel pid */)
        .expect("Failed to allocate heap memory from PMM");
    
    // Move heap to allocated physical address
    std::memory::heap::init_heap(heap_phys_addr as *mut u8, heap_size);
    
    // Configure PAT for Write-Combining on PAT4
    //init_pat();

    fs::dma::init();
    crate::fs::virtio::init();
    
    crate::fs::vfs::init();
    
    unsafe {
        (*(&raw mut window_manager::events::GLOBAL_EVENT_QUEUE)).init();
    }

    tss::init_ists();

    interrupts::task::TASK_MANAGER.lock().init();
    
    unsafe { (*(&raw mut DISPLAY_SERVER)).init(); }

    let first_user_task = interrupts::task::TASK_MANAGER.lock()
        .tasks.iter()
        .find(|t| t.state == interrupts::task::TaskState::Ready && t.kernel_stack != 0)
        .map(|t| t.kernel_stack);

    if let Some(kstack) = first_user_task {
        tss::set_tss(kstack);
    }

    drivers::periferics::mouse::init_mouse();

    crate::debugln!("Mounting Ext2...");
    match Ext2::new(0xE0, 16384) {
        Ok(fs) => crate::fs::vfs::mount(0xE0, fs),
        Err(e) => {
            crate::debugln!("Failed to mount Ext2: {}", e);
            loop {}
        }
    }

    crate::debugln!("Opening user file...");
    let mut node = match crate::fs::vfs::open(0xE0, "user") {
        Ok(node) => {
            crate::debugln!("File opened! Size: {}", node.size());
            node
        },
        Err(e) => {
            crate::debugln!("Failed to open file: {}", e);
            loop {}
        }
    };
    
    let size = node.size() as usize;
    let pages = (size + 4095) / 4096;
    let phys_addr = memory::pmm::allocate_frames(pages, 0).expect("Failed to allocate PMM for ELF");

        unsafe {

            let buf = core::slice::from_raw_parts_mut(phys_addr as *mut u8, size);

            let n = node.read(0, buf).unwrap();

    

            let pml4 = memory::vmm::new_user_pml4();

            

            match crate::fs::elf::load_elf(&buf[0..n], pml4) {

                Ok(entry) => {

                    crate::debugln!("ELF loaded successfully. Entry: {:#x}", entry);

                    memory::pmm::free_frame(phys_addr);

                    let _ = interrupts::task::TASK_MANAGER.lock().add_user_task(entry, pml4, None);

                },

                Err(e) => {

                    crate::debugln!("Failed to load ELF: {}", e);

                    loop {}

                }

            }

        }

        init_syscall_msrs(); // Initialize SYSCALL MSRs

        crate::debugln!("Kernel initialized, entering idle loop...");

        unsafe { asm!("sti"); }

        loop {}

    }

    

    fn init_pat() {

        unsafe {

            let mut pat = rdmsr(PAT_MSR);

    

            // PAT Layout: 8 entries of 1 byte each (bits 0-2 are memory type, others reserved/0).

            // We want Index 4 (Bits 32-39) to be Write-Combining (0x01).

            // Clear index 4

            pat &= !(0xFFu64 << 32);

            // Set index 4 to 0x01 (WC)

            pat |= (0x01u64 << 32);

    

            wrmsr(PAT_MSR, pat);

    

            // Flush TLB to ensure new attributes take effect

            let cr3: u64;

            asm!("mov {}, cr3", out(reg) cr3);

            asm!("mov cr3, {}", in(reg) cr3);

        }

    }

    

    /// Reads an MSR.

    unsafe fn rdmsr(msr: u32) -> u64 {

        let (low, high): (u32, u32);

        unsafe { asm!("rdmsr", in("ecx") msr, out("eax") low, out("edx") high) };

        ((high as u64) << 32) | (low as u64)

    }

    

    /// Writes an MSR.

    unsafe fn wrmsr(msr: u32, value: u64) {

        let low = value as u32;

        let high = (value >> 32) as u32;

        unsafe { asm!("wrmsr", in("ecx") msr, in("eax") low, in("edx") high) };

    }

    

    fn init_syscall_msrs() {

        unsafe {

            // 1. Enable SYSCALL/SYSRET in EFER MSR

            let mut efer = rdmsr(EFER_MSR);

            efer |= 1;

            wrmsr(EFER_MSR, efer);

    

            let sysret_cs_base = 0x20;

    

            let syscall_cs_base = 0x8;

    

            let star_value = ((sysret_cs_base as u64) << 48) | ((syscall_cs_base as u64) << 32);

            wrmsr(STAR_MSR, star_value);

            wrmsr(LSTAR_MSR, interrupts::syscalls::syscall_entry as u64);

    

            let rflags_mask = (1 << 9) | (1 << 8); // IF and TF

            wrmsr(SFMASK_MSR, rflags_mask);

        }

    }

    

    

    pub fn load_idt() {

        unsafe {

            (*(&raw mut interrupts::idt::IDT)).init();

    

            (*(&raw mut interrupts::idt::IDT)).processor_exceptions();

            (*(&raw mut interrupts::idt::IDT)).hardware_interrupts();

    

            // 3. Load the IDT into the CPU

    

            (*(&raw mut interrupts::idt::IDT)).load();

            (*(&raw mut interrupts::pic::PICS)).init();

        }

    }

    

    #[panic_handler]

    fn panic(info: &core::panic::PanicInfo) -> ! {

        crate::debugln!("KERNEL PANIC: {}", info);

        loop {}

    }
