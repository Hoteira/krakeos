#![no_std]

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

pub fn main() {
    let mut buf = [0u8; 4096];
    let args: alloc::vec::Vec<alloc::string::String> = std::env::args().collect();

    if args.len() <= 1 {
        // No arguments, read from stdin
        loop {
            let n = std::os::file_read(0, &mut buf);
            if n == 0 || n == usize::MAX { break; }
            sanitize_buffer(&mut buf[0..n]);
            std::os::file_write(1, &buf[0..n]);
        }
    } else {
        // Read from files
        for path in &args[1..] {
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

            match File::open(path) {
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
}
