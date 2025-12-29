#![no_std]
#![no_main]

extern crate alloc;

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let heap_size = 1024 * 1024; 
    let heap_ptr = std::memory::malloc(heap_size);
    std::memory::heap::init_heap(heap_ptr as *mut u8, heap_size);

    let mut buf = [0u8; 1024];
    loop {
        let n = std::os::file_read(0, &mut buf); 
        if n == 0 { break; } 
        std::os::file_write(1, &buf[0..n]); 
    }
    
    std::os::exit(0);
}
