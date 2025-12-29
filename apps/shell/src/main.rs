#![no_std]
#![no_main]

extern crate alloc;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use alloc::string::ToString;
use std::{println};

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

const STDIN_FD: usize = 0;
const STDOUT_FD: usize = 1;
const STDERR_FD: usize = 2;

fn resolve_path(cwd: &str, path: &str) -> String {
    let trimmed_path = path.trim();
    if trimmed_path.is_empty() { return String::from(cwd); }

    let mut parts = Vec::new();
    
    if !trimmed_path.starts_with('@') {
        for part in cwd.split('/') {
            if !part.is_empty() {
                parts.push(part);
            }
        }
    }

    for part in trimmed_path.split('/') {
        if part.is_empty() || part == "." {
            continue;
        } else if part == ".." {
            if parts.len() > 1 {
                parts.pop();
            }
        } else {
            parts.push(part);
        }
    }
    
    if parts.is_empty() {
         return String::from("@0xE0");
    }

    let mut res = String::new();
    for (i, p) in parts.iter().enumerate() {
        if i > 0 { res.push('/'); }
        res.push_str(p);
    }
    res
}

#[derive(PartialEq)]
enum RedirectionType {
    None,
    Output(String), 
    Append(String), 
    Input(String),  
}

struct CommandSegment {
    cmd: String,
    args: Vec<String>,
    input_file: Option<String>,
    output_file: Option<String>,
    append_mode: bool,
}

fn parse_segment(segment: &str) -> CommandSegment {
    let mut parts = segment.split_whitespace();
    let cmd = parts.next().unwrap_or("").to_string();
    let mut args = Vec::new();
    let mut input_file = None;
    let mut output_file = None;
    let mut append_mode = false;
    
    let mut pending_redirect = RedirectionType::None;
    
    for part in parts {
        match pending_redirect {
            RedirectionType::Output(_) => {
                output_file = Some(part.to_string());
                pending_redirect = RedirectionType::None;
            },
            RedirectionType::Append(_) => {
                output_file = Some(part.to_string());
                append_mode = true;
                pending_redirect = RedirectionType::None;
            },
            RedirectionType::Input(_) => {
                input_file = Some(part.to_string());
                pending_redirect = RedirectionType::None;
            },
            RedirectionType::None => {
                if part == ">" {
                    pending_redirect = RedirectionType::Output(String::new());
                } else if part == ">>" {
                    pending_redirect = RedirectionType::Append(String::new());
                } else if part == "<" {
                    pending_redirect = RedirectionType::Input(String::new());
                } else {
                    args.push(part.to_string());
                }
            }
        }
    }
    
    CommandSegment { cmd, args, input_file, output_file, append_mode }
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let heap_size = 1024 * 1024; 
    let heap_ptr = std::memory::malloc(heap_size);
    std::memory::heap::init_heap(heap_ptr as *mut u8, heap_size);

    println!("Shell: Started (Pipes & Redirections Enabled)");
    std::os::file_write(STDOUT_FD, "\nWelcome to KrakeOS Shell \u{E8F0} \n> ".as_bytes());

    let mut cwd = String::from("@0xE0");
    let mut cmd_buffer = String::new();

    loop {
        let mut buf = [0u8; 1];
        let n = std::os::file_read(STDIN_FD, &mut buf);
        if n > 0 {
            let b = buf[0];
            let c = b as char;

            if b == b'\r' || b == b'\n' {
                std::os::file_write(STDOUT_FD, b"\n");
                let line = cmd_buffer.trim();
                
                if !line.is_empty() {
                    
                    let segments: Vec<&str> = line.split('|').collect();
                    let mut prev_pipe_read: Option<i32> = None;
                    let mut children_pids = Vec::new();
                    
                    for (i, segment) in segments.iter().enumerate() {
                        let parsed = parse_segment(segment);
                        if parsed.cmd.is_empty() { continue; }
                        
                        
                        let mut stdin_fd = 0;
                        let mut close_stdin = false;
                        
                        if let Some(infile) = parsed.input_file {
                            let path = resolve_path(&cwd, &infile);
                            if let Ok(f) = std::fs::File::open(&path) {
                                stdin_fd = f.as_raw_fd(); 
                                core::mem::forget(f); 
                                close_stdin = true;
                            } else {
                                let err = format!("Failed to open input: {}\n", path);
                                std::os::file_write(STDOUT_FD, err.as_bytes());
                                break; 
                            }
                        } else if let Some(fd) = prev_pipe_read {
                            stdin_fd = fd as usize;
                            close_stdin = true;
                        }
                        
                        
                        let mut stdout_fd = 1;
                        let mut close_stdout = false;
                        let mut next_pipe_read = None;
                        
                        if let Some(outfile) = parsed.output_file {
                            let path = resolve_path(&cwd, &outfile);
                            let res = if parsed.append_mode {
                                std::fs::File::open(&path).or_else(|_| std::fs::File::create(&path))
                            } else {
                                std::fs::File::create(&path)
                            };
                            
                            match res {
                                Ok(f) => {
                                    stdout_fd = f.as_raw_fd();
                                    if parsed.append_mode {
                                        std::os::file_seek(stdout_fd, 0, 2); 
                                    }
                                    core::mem::forget(f);
                                    close_stdout = true;
                                },
                                Err(_) => {
                                    let err = format!("Failed to open output: {}\n", path);
                                    std::os::file_write(STDOUT_FD, err.as_bytes());
                                    break;
                                }
                            }
                        } else if i < segments.len() - 1 {
                            
                            let mut fds = [0i32; 2];
                            if std::os::pipe(&mut fds) == 0 {
                                stdout_fd = fds[1] as usize;
                                next_pipe_read = Some(fds[0]);
                                close_stdout = true;
                                
                            } else {
                                std::os::file_write(STDOUT_FD, b"Pipe creation failed\n");
                                break;
                            }
                        }
                        
                        
                        
                        let is_builtin = match parsed.cmd.as_str() {
                            "cd" | "ls" | "pwd" | "help" | "clear" | "touch" | "mkdir" | "rm" | "mv" | "cp" | "sleep" => true,
                            _ => false
                        };
                        
                        if is_builtin {
                             
                             
                             
                             
                             
                             
                             
                             
                             
                             
                             
                             execute_builtin(&parsed.cmd, &parsed.args, &mut cwd, stdin_fd, stdout_fd);
                             
                             
                             
                        } else {
                            
                            let prog_path = if parsed.cmd.starts_with('@') {
                                parsed.cmd.clone()
                            } else {
                                let mut p = String::from("@0xE0/sys/bin/");
                                p.push_str(&parsed.cmd);
                                if !parsed.cmd.ends_with(".elf") {
                                    p.push_str(".elf");
                                }
                                p
                            };
                            
                            let map = [
                                (0, stdin_fd as u8),
                                (1, stdout_fd as u8),
                                (2, 2)
                            ];
                            
                            let pid = std::os::spawn_with_fds(&prog_path, &map);
                            if pid != usize::MAX {
                                children_pids.push(pid);
                            } else {
                                let err = format!("Command not found: {}\n", parsed.cmd);
                                std::os::file_write(STDOUT_FD, err.as_bytes());
                            }
                        }
                        
                        
                        if close_stdin && stdin_fd > 2 { std::os::file_close(stdin_fd); }
                        if close_stdout && stdout_fd > 2 { std::os::file_close(stdout_fd); }
                        
                        prev_pipe_read = next_pipe_read;
                    }
                    
                    
                    for pid in children_pids {
                        let exit_code = std::os::waitpid(pid);
                        let msg = format!("\n[{}]\n", exit_code);
                        std::os::file_write(STDOUT_FD, msg.as_bytes());
                    }
                }
                
                cmd_buffer.clear();
                std::os::file_write(STDOUT_FD, b"> ");
            } else if b == 0x08 || b == 0x7F { 
                if !cmd_buffer.is_empty() {
                    cmd_buffer.pop();
                    std::os::file_write(STDOUT_FD, b"\x08 \x08"); 
                }
            } else if c >= ' ' && c != '\x7F' { 
                cmd_buffer.push(c);
                std::os::file_write(STDOUT_FD, &[b]);
            }
        } else {
            std::os::yield_task();
        }
    }
}

fn execute_builtin(cmd: &str, args: &[String], cwd: &mut String, in_fd: usize, out_fd: usize) {
    if cmd == "help" {
        std::os::file_write(out_fd, b"Available commands: help, clear, ls, cd, pwd, touch, mkdir, rm, mv, cp, cat, sleep\n");
    } else if cmd == "sleep" {
        if !args.is_empty() {
            if let Ok(ms) = args[0].parse::<u64>() {
                std::os::sleep(ms);
            }
        }
    } else if cmd == "cat" {
        if args.is_empty() {
             
             
             
             let mut buf = [0u8; 1024];
             loop {
                 let n = std::os::file_read(in_fd, &mut buf);
                 if n == 0 { break; }
                 std::os::file_write(out_fd, &buf[0..n]);
             }
        } else {
             for arg in args {
                 let path = resolve_path(cwd, arg);
                 if let Ok(mut file) = std::fs::File::open(&path) {
                     let mut buf = [0u8; 1024];
                     loop {
                         match file.read(&mut buf) {
                             Ok(n) if n > 0 => { 
                                 std::os::file_write(out_fd, &buf[0..n]); 
                             },
                             _ => break,
                         }
                     }
                 } else {
                     let err = format!("cat: {}: No such file\n", path);
                     std::os::file_write(out_fd, err.as_bytes());
                 }
             }
        }
    } else if cmd == "clear" {
        std::os::file_write(out_fd, b"\x1B[2J\x1B[H");
    } else if cmd == "pwd" {
         std::os::file_write(out_fd, cwd.as_bytes());
         std::os::file_write(out_fd, b"\n");
    } else if cmd == "ls" {
        let target = if args.is_empty() { cwd.as_str() } else { &args[0] };
        let full_path = resolve_path(cwd, target);
        match std::fs::read_dir(&full_path) {
            Ok(entries) => {
                for entry in entries {
                    let mut line = String::new();
                    line.push_str("  ");
                    if entry.file_type == std::fs::FileType::Directory {
                        line.push_str("\x1B[1;94m\u{F07B} ");
                        line.push_str(&entry.name);
                        line.push_str("/\x1B[0m\n");
                    } else {
                        line.push_str("\x1B[37m\u{F016} ");
                        line.push_str(&entry.name);
                        line.push_str("\x1B[0m\n");
                    }
                    std::os::file_write(out_fd, line.as_bytes());
                }
            },
            Err(_) => {
                let err = format!("ls: cannot access '{}': No such file\n", full_path);
                std::os::file_write(out_fd, err.as_bytes());
            }
        }
    } else if cmd == "cd" {
        if args.is_empty() {
             *cwd = String::from("@0xE0");
        } else {
            let new_path = resolve_path(cwd, &args[0]);
            if std::fs::read_dir(&new_path).is_ok() {
                *cwd = new_path;
            } else {
                let err = format!("cd: {}: No such file\n", new_path);
                std::os::file_write(out_fd, err.as_bytes());
            }
        }
    } else if cmd == "touch" {
        if !args.is_empty() {
            let path = resolve_path(cwd, &args[0]);
            if let Err(_) = std::fs::File::create(&path) {
                let err = format!("touch: cannot create '{}'\n", path);
                std::os::file_write(out_fd, err.as_bytes());
            }
        }
    } else if cmd == "mkdir" {
        if !args.is_empty() {
            let path = resolve_path(cwd, &args[0]);
            if let Err(_) = std::fs::create_dir(&path) {
                let err = format!("mkdir: cannot create '{}'\n", path);
                std::os::file_write(out_fd, err.as_bytes());
            }
        }
    } else if cmd == "rm" {
        if !args.is_empty() {
            let path = resolve_path(cwd, &args[0]);
            if let Err(_) = std::fs::remove_file(&path) {
                let err = format!("rm: cannot remove '{}'\n", path);
                std::os::file_write(out_fd, err.as_bytes());
            }
        }
    } else if cmd == "mv" {
        if args.len() >= 2 {
            let src = resolve_path(cwd, &args[0]);
            let dst = resolve_path(cwd, &args[1]);
            if let Err(_) = std::fs::rename(&src, &dst) {
                let err = format!("mv: failed\n");
                std::os::file_write(out_fd, err.as_bytes());
            }
        }
    } else if cmd == "cp" {
        if args.len() >= 2 {
            let src_path = resolve_path(cwd, &args[0]);
            let dst_path = resolve_path(cwd, &args[1]);
            
            if let Ok(mut src) = std::fs::File::open(&src_path) {
                if let Ok(mut dst) = std::fs::File::create(&dst_path) {
                    let mut buf = [0u8; 1024];
                    loop {
                        match src.read(&mut buf) {
                            Ok(n) if n > 0 => { dst.write(&buf[0..n]).ok(); },
                            _ => break,
                        }
                    }
                }
            }
        }
    }
}
