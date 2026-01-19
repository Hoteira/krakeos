#![no_std]
#![no_main]

extern crate alloc;
mod utils;
mod parser;
mod builtins;

use alloc::format;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;

use crate::builtins::execute_builtin;
use crate::parser::parse_segment;
use crate::utils::resolve_path;

const STDIN_FD: usize = 0;
const STDOUT_FD: usize = 1;

#[unsafe(no_mangle)]
pub extern "C" fn main() -> i32 {
    let welcome_icon = core::char::from_u32(0xE8F0).unwrap_or(' ');
    let welcome_msg = format!("\nWelcome to KrakeOS Shell {} \n> ", welcome_icon);
    std::os::file_write(STDOUT_FD, welcome_msg.as_bytes());

    let mut cwd = String::from("@0xE0");
    let mut path_env = String::from("/sys/bin;/apps");
    let mut cmd_buffer = String::new();

    loop {
        let mut buf = [0u8; 1];
        let n = std::os::file_read(STDIN_FD, &mut buf);
        if n > 0 && n != usize::MAX {
            let b = buf[0];
            let c = b as char;

            if b == b'\r' || b == b'\n' {
                std::os::file_write(STDOUT_FD, b"\n");
                let line = cmd_buffer.trim();

                if !line.is_empty() {
                    let logical_blocks: Vec<&str> = line.split("&&").collect();

                    for block in logical_blocks {
                        let segments: Vec<&str> = block.split('|').collect();
                        let mut prev_pipe_read: Option<i32> = None;
                        let mut children_pids = Vec::new();
                        let mut last_exit_code = 0;

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
                                    last_exit_code = 1;
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
                                    }
                                    Err(_) => {
                                        let err = format!("Failed to open output: {}\n", path);
                                        std::os::file_write(STDOUT_FD, err.as_bytes());
                                        last_exit_code = 1;
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
                                    last_exit_code = 1;
                                    break;
                                }
                            }

                            let is_builtin = match parsed.cmd.as_str() {
                                "cd" | "ls" | "pwd" | "help" | "clear" | "touch" | "mkdir" | "rm" | "mv" | "cp" | "sleep" | "osfetch" | "echo" | "cat" | "export" => true,
                                _ => false
                            };

                            if is_builtin {
                                last_exit_code = execute_builtin(&parsed.cmd, &parsed.args, &mut cwd, &mut path_env, stdin_fd, stdout_fd) as usize;
                            } else {
                                let mut prog_path = String::new();
                                let mut found = false;

                                if parsed.cmd.starts_with('@') || parsed.cmd.contains('/') {
                                    prog_path = resolve_path(&cwd, &parsed.cmd);

                                    if let Ok(_) = std::fs::File::open(&prog_path) {
                                        found = true;
                                    }
                                } else {
                                    for path_dir in path_env.split(';') {
                                        // Try exact match first
                                        let p_exact = format!("{}/{}", path_dir, parsed.cmd);
                                        if let Ok(_) = std::fs::File::open(&p_exact) {
                                            prog_path = p_exact;
                                            found = true;
                                            break;
                                        }

                                        if !parsed.cmd.ends_with(".elf") && !parsed.cmd.ends_with(".wasm") {
                                            let p_elf = format!("{}/{}.elf", path_dir, parsed.cmd);
                                            if let Ok(_) = std::fs::File::open(&p_elf) {
                                                prog_path = p_elf;
                                                found = true;
                                                break;
                                            }

                                            let p_wasm = format!("{}/{}.wasm", path_dir, parsed.cmd);
                                            if let Ok(_) = std::fs::File::open(&p_wasm) {
                                                prog_path = p_wasm;
                                                found = true;
                                                break;
                                            }
                                        }

                                        if !found && (path_dir.ends_with("/apps") || path_dir == "@0xE0/apps") {
                                            let apps_dir = format!("{}/{}", path_dir, parsed.cmd);
                                            if let Ok(entries) = std::fs::read_dir(&apps_dir) {
                                                for entry in entries {
                                                    if entry.file_type == std::fs::FileType::File && entry.name.ends_with(".elf") {
                                                        prog_path = format!("{}/{}", apps_dir, entry.name);
                                                        found = true;
                                                        break;
                                                    }
                                                    if entry.file_type == std::fs::FileType::File && entry.name.ends_with(".wasm") {
                                                        prog_path = format!("{}/{}", apps_dir, entry.name);
                                                        found = true;
                                                        break;
                                                    }
                                                }
                                            }
                                        }
                                        if found { break; }
                                    }
                                }

                                if found {
                                    let map = [
                                        (0, stdin_fd as u8),
                                        (1, stdout_fd as u8),
                                        (2, 2)
                                    ];

                                    let args_refs: Vec<&str> = parsed.args.iter().map(|s| s.as_str()).collect();

                                    let pid;
                                    if prog_path.ends_with(".wasm") {
                                        let runner = "@0xE0/sys/bin/wasm_runner.elf";
                                        let mut runner_args = Vec::new();
                                        runner_args.push(prog_path.as_str());
                                        runner_args.extend_from_slice(&args_refs);
                                        pid = std::os::spawn_with_fds(runner, &runner_args, &map);
                                    } else {
                                        pid = std::os::spawn_with_fds(&prog_path, &args_refs, &map);
                                    }

                                    if pid != usize::MAX {
                                        children_pids.push(pid);
                                    } else {
                                        let err = format!("Failed to spawn: {}\n", prog_path);
                                        std::os::file_write(STDOUT_FD, err.as_bytes());
                                        last_exit_code = 1;
                                    }
                                } else {
                                    let err = format!("Command not found: {}\n", parsed.cmd);
                                    std::os::file_write(STDOUT_FD, err.as_bytes());
                                    last_exit_code = 127;
                                }
                            }

                            if close_stdin && stdin_fd > 2 { std::os::file_close(stdin_fd); }
                            if close_stdout && stdout_fd > 2 { std::os::file_close(stdout_fd); }

                            prev_pipe_read = next_pipe_read;
                        }

                        for pid in children_pids {
                            last_exit_code = std::os::waitpid(pid);
                        }

                        if last_exit_code != 0 {
                            break;
                        }
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