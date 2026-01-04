#![no_std]
#![no_main]

extern crate alloc;
use std::println;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn main(_argc: i32, _argv: *const *const u8) -> i32 {
    let mut ms = 1000;
    
    let args = std::args();
    if args.len() > 1 {
        if let Ok(parsed) = args[1].parse::<u64>() {
            ms = parsed;
        }
    }
    
    std::os::sleep(ms);
    0
}
