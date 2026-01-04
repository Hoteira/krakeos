#![no_std]
#![no_main]

extern crate alloc;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn main(_argc: i32, _argv: *const *const u8) -> i32 {

    let mut buf = [0u8; 1024];
    loop {
        let n = std::os::file_read(0, &mut buf); 
        if n == 0 { break; } 
        std::os::file_write(1, &buf[0..n]); 
    }
    
    0
}
