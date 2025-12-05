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

extern crate alloc;

use crate::boot::{BootInfo, BOOT_INFO};
use crate::fs::vfs::FileSystem; 
use crate::fs::ext2::fs::Ext2; // Added Ext2 for font loading
use core::arch::asm;

#[unsafe(no_mangle)]
#[unsafe(link_section = ".start")]
pub extern "C" fn _start(bootinfo_ptr: *const BootInfo) -> ! {
    unsafe {
        asm!("cli" );
    }

    unsafe { 
        *(&raw mut BOOT_INFO) = bootinfo_ptr.read();
    };

    memory::init();

    // Initialize Video (Framebuffer)
    drivers::video::framebuffer::init();
    
    // Draw Blue Screen
    drivers::video::framebuffer::clear_screen(0xFF0000FF); // ARGB (Blue)

    fs::dma::init();
    std::memory::heap::init_heap(0x30_0000 as *mut u8, 0x100_0000);

    debugln!("[KERNEL] Hello World!");

    interrupts::task::TASK_MANAGER.lock().init();

    interrupts::task::TASK_MANAGER.lock().add_task(test_task as u64, None);
    
    debugln!("[KERNEL] Initializing Ext2...");

    match Ext2::new(0, 16384) {
        Ok(mut fs) => {
            debugln!("[EXT2] Superblock found!");
            debugln!(" - Magic: {:#x}", fs.superblock.magic + 0);
            
            match fs.root() {
                Ok(mut root) => {
                     match root.find("user") {
                         Ok(mut user_file) => {
                             let mut elf_buf = alloc::vec![0u8; user_file.size() as usize];
                             if let Ok(bytes) = user_file.read(0, &mut elf_buf) {
                                 match crate::fs::elf::load_elf(&elf_buf[0..bytes]) {
                                     Ok(entry_point) => {
                                         debugln!("[ELF] Loaded user program. Entry: {:#x}", entry_point);
                                         interrupts::task::TASK_MANAGER.lock().add_user_task(entry_point, None);
                                     }
                                     Err(e) => debugln!("[ELF] Load failed: {}", e),
                                 }
                             }
                         }
                         Err(e) => debugln!("[EXT2] Failed to find user binary: {}", e),
                     }
                }
                Err(e) => debugln!("[EXT2] Root error: {}", e),
            }
        }
        Err(e) => debugln!("[EXT2] Failed to mount Disk 0 @ 16384: {}", e),
    }

    debugln!("[KERNEL] Starting Kernel Tasks...");
    load_idt();

    loop {}
}

fn test_task() {
    loop {
        debugln!(".");
        for _ in 0..10000000 { unsafe { core::arch::asm!("nop") } }
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

        core::arch::asm!("sti"); // Interrupts disabled for stability during mounting
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    debugln!("[KERNEL_PANIC]\n{}", info);
    loop {}
}


