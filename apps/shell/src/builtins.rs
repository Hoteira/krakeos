use crate::utils::resolve_path;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

fn list_dir_safe(path: &str, out_fd: usize) -> i32 {
    match std::fs::read_dir(path) {
        Ok(entries) => {
            for entry in &entries {
                let mut line = String::from("  ");
                if entry.file_type == std::fs::FileType::Directory {
                    let icon = core::char::from_u32(0xF07B).unwrap_or('/');
                    line.push_str("\x1B[1m\x1B[94m");
                    line.push(icon);
                    line.push(' ');
                    line.push_str(&entry.name);
                    line.push_str("/\x1B[0m\n");
                } else {
                    let icon = core::char::from_u32(0xF016).unwrap_or('-');
                    line.push_str("\x1B[37m");
                    line.push(icon);
                    line.push(' ');
                    line.push_str(&entry.name);
                    line.push_str("\x1B[0m\n");
                }
                std::os::file_write(out_fd, line.as_bytes());
            }
            0
        }
        Err(_) => -1,
    }
}

pub fn execute_builtin(
    cmd: &str,
    args: &[String],
    cwd: &mut String,
    path_env: &mut String,
    in_fd: usize,
    out_fd: usize,
) -> i32 {
    if cmd == "help" {
        let help_text = "
KrakeOS Shell Builtin Commands:
--------------------------------------------------
File System:
  ls [path]          List directory contents
  cd [path]          Change current working directory
  pwd                Print current working directory
  touch <file>       Create an empty file
  mkdir <dir>        Create a new directory
  rm <path>          Remove a file or directory
  mv <src> <dst>     Move/Rename a file or directory
  cp <src> <dst>     Copy a file
  cat [file]         Concatenate and print files

System:
  help               Show this help message
  clear              Clear the screen
  sleep <ms>         Suspend execution for a period
  osfetch            Show system information
  echo [text]        Display a line of text
  export <VAR=VAL>   Set an environment variable
  exit               Exit the shell

WASM:
  wasm [args]        Run a WASM module
--------------------------------------------------
";
        std::os::file_write(out_fd, help_text.as_bytes());
        return 0;
    } else if cmd == "wasm" {
        let mut actual_args = args.to_vec();
        let mut i = 0;
        let mut prog_idx = None;

        while i < actual_args.len() {
            if actual_args[i] == "--dir" {
                if i + 1 < actual_args.len() {
                    actual_args[i + 1] = resolve_path(cwd, &actual_args[i + 1]);
                    i += 2;
                } else {
                    i += 1;
                }
            } else if actual_args[i].starts_with("--") {
                i += 1;
            } else {
                if prog_idx.is_none() {
                    prog_idx = Some(i);
                }
                i += 1;
            }
        }

        if let Some(idx) = prog_idx {
            let prog_name = actual_args[idx].clone();
            let mut prog_path = resolve_path(cwd, &prog_name);

            if std::fs::File::open(&prog_path).is_err() && !prog_name.starts_with('@') && !prog_name.contains('/') {
                let mut found = false;
                for path_dir in path_env.split(';') {
                    let p = format!("{}/{}", path_dir, prog_name);
                    let p_wasm = if p.ends_with(".wasm") { p.clone() } else { format!("{}.wasm", p) };
                    if std::fs::File::open(&p_wasm).is_ok() {
                        prog_path = p_wasm;
                        found = true;
                        break;
                    }
                }
                if !found {
                    let err = format!("wasm: file not found: {}\n", prog_name);
                    std::os::file_write(out_fd, err.as_bytes());
                    return 1;
                }
            }

            // Prepare arguments: we pass ALL of them, including flags.
            // But we need to make sure the wasm runner gets them in a sensible way.
            // Actually, the wasm runner in std/src/wasm/runner.rs parses flags from the beginning.
            let args_refs: Vec<&str> = actual_args.iter().map(|s| s.as_str()).collect();
            let pid = std::os::spawn_with_fds(&prog_path, &args_refs, &[(0, in_fd as u8), (1, out_fd as u8), (2, 2)]);

            if pid != usize::MAX {
                return std::os::waitpid(pid as u64) as i32;
            } else {
                let err = format!("wasm: failed to spawn {}\n", prog_path);
                std::os::file_write(out_fd, err.as_bytes());
                return 1;
            }
        }
        return 1;
    }
 else if cmd == "export" {
        if !args.is_empty() {
            let arg = &args[0];
            if arg.starts_with("PATH=") {
                *path_env = String::from(&arg[5..]);
            }
        }
        return 0;
    } else if cmd == "echo" {
        for (i, arg) in args.iter().enumerate() {
            if i > 0 { std::os::file_write(out_fd, b" "); }
            std::os::file_write(out_fd, arg.as_bytes());
        }
        std::os::file_write(out_fd, b"\n");
        return 0;
    } else if cmd == "osfetch" {
        // (osfetch logic remains same, but using file_write to out_fd)
        let screen_w = std::os::graphics::get_screen_width();
        let screen_h = std::os::graphics::get_screen_height();
        let ticks = std::os::get_system_ticks();
        let s = (ticks / 1000) % 60;
        let m = (ticks / 60000) % 60;
        let h = (ticks / 3600000);

        let msg = format!("  OS: KrakeOS\n  Uptime: {}:{}:{:02}\n  Resolution: {}x{}\n", h, m, s, screen_w, screen_h);
        std::os::file_write(out_fd, msg.as_bytes());
        return 0;
    } else if cmd == "sleep" {
        if !args.is_empty() {
            if let Ok(ms) = args[0].parse::<u64>() { std::os::sleep(ms); }
        }
        return 0;
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
                let path_c = format!("{}\0", path);
                let fd = std::os::native_file_open(path_c.as_ptr(), path_c.len() as u64 - 1, 0);
                if fd >= 0 {
                    let mut buf = [0u8; 1024];
                    loop {
                        let n = std::os::file_read(fd as usize, &mut buf);
                        if n == 0 || n == usize::MAX { break; }
                        std::os::file_write(out_fd, &buf[0..n]);
                    }
                    std::os::file_close(fd as usize);
                } else {
                    let err = format!("cat: {}: No such file\n", path);
                    std::os::file_write(out_fd, err.as_bytes());
                    return 1;
                }
            }
        }
        return 0;
    } else if cmd == "clear" {
        std::os::file_write(out_fd, b"\x1B[2J\x1B[H");
        return 0;
    } else if cmd == "pwd" {
        std::os::file_write(out_fd, cwd.as_bytes());
        std::os::file_write(out_fd, b"\n");
        return 0;
    } else if cmd == "ls" {
        let target = if args.is_empty() { "." } else { &args[0] };
        let full_path = resolve_path(cwd, target);
        if list_dir_safe(&full_path, out_fd) != 0 {
            let err = format!("ls: cannot access \"{}\": No such file or directory\n", full_path);
            std::os::file_write(out_fd, err.as_bytes());
            return 1;
        }
        return 0;
    } else if cmd == "cd" {
        let target = if args.is_empty() { "@0xE0" } else { &args[0] };
        let new_path = resolve_path(cwd, target);
        
        if std::os::chdir(&new_path) == 0 {
            *cwd = new_path;
            return 0;
        } else {
            let err = format!("cd: {}: No such file or directory\n", new_path);
            std::os::file_write(out_fd, err.as_bytes());
            return 1;
        }
    }
    0
}
