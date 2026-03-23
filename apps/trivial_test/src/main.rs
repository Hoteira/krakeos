#![no_std]
#![no_main]

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() {
    let msg = "Hello from Ring 3 AOT!\n";
    
    #[cfg(target_arch = "wasm32")]
    unsafe {
        host_serial_print(msg.as_ptr(), msg.len());
    }

    #[cfg(target_arch = "x86_64")]
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") 999, // SERIAL_PRINT
            in("rdi") msg.as_ptr() as u64,
            in("rsi") msg.len() as u64,
            lateout("rax") _,
            lateout("rcx") _,
            lateout("r11") _,
        );
        
        core::arch::asm!(
            "syscall",
            in("rax") 60, // EXIT
            in("rdi") 0,  // code 0
            lateout("rax") _,
            lateout("rcx") _,
            lateout("r11") _,
        );
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn main() {
    _start();
}

#[unsafe(no_mangle)]
pub extern "C" fn run() {
    _start();
}

extern "C" {
    fn host_serial_print(ptr: *const u8, len: usize);
}
