use crate::wasm::interpreter::Interpreter;
use crate::rust_alloc::string::String;

pub mod file;
pub mod proc;
pub mod clock;
pub mod poll;
pub mod types;

pub struct Wasi;

impl Wasi {
    pub fn register(interpreter: &mut Interpreter) {
        let mod_name_preview = "wasi_snapshot_preview1";
        let mod_name_unstable = "wasi_unstable";

        // 1. Ensure FDs 0, 1, 2 are occupied (Standard I/O)
        for i in 0..3 {
            let mut found = false;
            for (f, _) in &interpreter.fd_paths { if *f == i { found = true; break; } }
            if !found {
                if let Ok(file) = crate::fs::File::open("@0xE0/user.elf") {
                    let fd = file.as_raw_fd();
                    if fd == i {
                        interpreter.fd_paths.push((fd, String::from("/dev/stdio")));
                        core::mem::forget(file); 
                    } else {
                        // If it's not the FD we wanted, just let it close
                    }
                }
            }
        }

        // 2. Open Root as FD 3 (The first pre-opened directory)
        let mut root_found = false;
        for (f, _) in &interpreter.fd_paths { if *f == 3 { root_found = true; break; } }
        if !root_found {
            if let Ok(root) = crate::fs::File::open("@0xE0/") {
                let fd = root.as_raw_fd();
                if fd == 3 {
                    interpreter.fd_paths.push((fd, String::from("/")));
                    core::mem::forget(root);
                }
            }
        }

        Self::register_module(interpreter, mod_name_preview);
        Self::register_module(interpreter, mod_name_unstable);
    }

    fn register_module(interpreter: &mut Interpreter, mod_name: &str) {
        proc::register(interpreter, mod_name);
        clock::register(interpreter, mod_name);
        file::register(interpreter, mod_name);
        poll::register(interpreter, mod_name);
    }
}
