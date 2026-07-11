#![no_std]
#![no_main]

extern crate alloc;

use core::arch::global_asm;
use core::panic::PanicInfo;

global_asm!(include_str!("arch/boot.S"));
pub mod arch;
pub mod drivers;
pub mod fs;
pub mod sys;

pub use arch::*;
pub use drivers::*;
pub use sys::*;

use linked_list_allocator::LockedHeap;

#[global_allocator]
pub static ALLOCATOR: LockedHeap = LockedHeap::empty();
pub static mut ROOT_PAGE_TABLE: *mut paging::PageTable = core::ptr::null_mut();

unsafe extern "C" {
    static __kernel_end: u8;
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    crate::println!("KERNEL PANIC: {}", info);
    loop {}
}

#[no_mangle]
pub extern "C" fn main(_hart_id: usize, dtb_ptr: usize) -> ! {
    unsafe {
        core::arch::asm!("csrw stvec, {}", in(reg) trap::trap_vector as *const () as usize);
    }

    println!("Hello from a Rust RISC-V Kernel Workspace!");

    let fdt_parser = unsafe { fdt::FdtParser::from_ptr(dtb_ptr as *const u8).expect("Failed to parse FDT") };
    let (mem_start, mem_size) = fdt_parser.get_memory_region().expect("Could not find memory region in FDT");
    let mem_end = mem_start + mem_size;

    let kernel_end_addr = core::ptr::addr_of!(__kernel_end) as usize;

    unsafe {
        ALLOCATOR.lock().init(kernel_end_addr as *mut u8, mem_end - kernel_end_addr);
        
        let root_table_layout = core::alloc::Layout::from_size_align(4096, 4096).unwrap();
        let root_table_ptr = alloc::alloc::alloc_zeroed(root_table_layout) as *mut paging::PageTable;
        ROOT_PAGE_TABLE = root_table_ptr;

        paging::map_range(root_table_ptr, 0x10000000, 0x10000000, 4096, paging::PTE_R | paging::PTE_W);
        // Map VirtIO MMIO Region (0x10001000 - 0x10008000)
        paging::map_range(root_table_ptr, 0x10001000, 0x10001000, 0x8000, paging::PTE_R | paging::PTE_W);
        // Map Goldfish RTC (/dev/system/time reads it every frame)
        paging::map_range(root_table_ptr, crate::drivers::rtc::GOLDFISH_RTC_BASE, crate::drivers::rtc::GOLDFISH_RTC_BASE, 4096, paging::PTE_R | paging::PTE_W);
        
        let ram_size = mem_end - 0x80000000;
        paging::map_range(root_table_ptr, 0x80000000, 0x80000000, ram_size, paging::PTE_R | paging::PTE_W | paging::PTE_X);

        let satp_val = (8u64 << 60) | ((root_table_ptr as u64) >> 12);
        csr::write_satp(satp_val);
        csr::sfence_vma();
    }

    println!("MMU Activated (Identity Mapping)!");

    if virtio::init() {
        println!("VirtIO Block Initialized.");
        fs::mount();
        unsafe {
            core::arch::asm!("csrs sstatus, {}", in(reg) 1 << 13);
        }
    } else {
        println!("VirtIO Block Init Failed.");
    }

    if virtio_gpu::init() {
        println!("VirtIO GPU Initialized.");
    } else {
        println!("VirtIO GPU Init Failed.");
    }

    scheduler::Scheduler::init_main();
    println!("Loading /bin/wasm_runner.elf...");
    
    // Load /bin/wasm_runner.elf from disk
    let desc_idx = match fs::find_file("/bin/wasm_runner.elf") {
        Some(idx) => idx,
        None => {
            println!("/bin/wasm_runner.elf not found on disk!");
            loop {}
        }
    };
    
    let size = fs::get_file_size(desc_idx);
    println!("Found /bin/wasm_runner.elf, size: {} bytes", size);
    
    let mut elf_bytes = alloc::vec![0; size];
    let br = fs::read_file(desc_idx, 0, &mut elf_bytes);
    if br == size {
        sys::elf::load_elf_and_spawn(&elf_bytes);
    } else {
        println!("Failed to read /bin/wasm_runner.elf");
    }

    println!("Starting Scheduler...");
    
    csr::enable_timer_interrupt();
    csr::enable_global_interrupts();
    
    let next_tick = csr::read_time() + 10_000;
    sbi::set_timer(next_tick);

    loop {
        // Idle loop
        unsafe { core::arch::asm!("wfi"); }
    }
}
