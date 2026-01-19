#![no_std]
#![no_main]

extern crate alloc;
use std::fs::File;
use std::io::Read;

fn sanitize_buffer(buf: &mut [u8]) {
    for b in buf.iter_mut() {
        if *b == 0x1B {
            *b = b'.';
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn main(argc: i32, argv: *const *const u8) -> i32 {
    let mut buf = [0u8; 4096];

    if argc <= 1 {
        // No arguments, read from stdin
        loop {
            let n = std::os::file_read(0, &mut buf);
            if n == 0 || n == usize::MAX { break; }
            sanitize_buffer(&mut buf[0..n]);
            std::os::file_write(1, &buf[0..n]);
        }
    } else {
        // Read from files
        for i in 1..argc {
            let arg_ptr = unsafe { *argv.add(i as usize) };
            let c_str = unsafe { core::ffi::CStr::from_ptr(arg_ptr as *const i8) };
            let path = c_str.to_string_lossy();

            // Handle "-" as stdin
            if path == "-" {
                loop {
                    let n = std::os::file_read(0, &mut buf);
                    if n == 0 || n == usize::MAX { break; }
                    sanitize_buffer(&mut buf[0..n]);
                    std::os::file_write(1, &buf[0..n]);
                }
                continue;
            }

            match File::open(&path) {
                Ok(mut file) => {
                    loop {
                        match file.read(&mut buf) {
                            Ok(0) => break,
                            Ok(n) => {
                                sanitize_buffer(&mut buf[0..n]);
                                std::os::file_write(1, &buf[0..n]);
                            }
                            Err(_) => break,
                        }
                    }
                }
                Err(_) => {
                    let err = alloc::format!("cat: {}: No such file or directory\n", path);
                    std::os::file_write(2, err.as_bytes());
                }
            }
        }
    }

    0
}
