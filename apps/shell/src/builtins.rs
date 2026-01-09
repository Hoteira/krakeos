use alloc::format;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use std::io::{Read, Write};
use crate::utils::resolve_path;

pub fn execute_builtin(cmd: &str, args: &[String], cwd: &mut String, path_env: &mut String, in_fd: usize, out_fd: usize) {
    if cmd == "help" {
        std::os::file_write(out_fd, b"Available commands: help, clear, ls, cd, pwd, touch, mkdir, rm, mv, cp, cat, sleep, osfetch, echo, export\n");
    } else if cmd == "export" {
        if !args.is_empty() {
            let arg = &args[0];
            if arg.starts_with("PATH=") {
                *path_env = String::from(&arg[5..]);
            }
        }
    } else if cmd == "echo" {
        for (i, arg) in args.iter().enumerate() {
            if i > 0 {
                std::os::file_write(out_fd, b" ");
            }
            std::os::file_write(out_fd, arg.as_bytes());
        }
        std::os::file_write(out_fd, b"\n");
    } else if cmd == "osfetch" {
        let white = "\x1B[97m";
        let blue = "\x1B[94m";
        let gray = "\x1B[90m";
        let reset = "\x1B[0m";

        let p_pink = "\x1B[38;2;255;182;193m";
        let p_green = "\x1B[38;2;152;251;152m";
        let p_blue = "\x1B[38;2;173;216;230m";
        let p_yellow = "\x1B[38;2;255;255;186m";
        let p_purple = "\x1B[38;2;221;160;221m";
        let p_cyan = "\x1B[38;2;175;238;238m";

        let ascii = [
            "              @@@             ",
            "         @@@@@@@@@@@@@        ",
            "       @@@@@@@@@@@@@@@@@      ",
            "      @@@@@@@@@@@@@@@@@@@     ",
            "     &@@@@@@@@@@@@@@@@@@@     ",
            "     &@@@@@@@@@@@@@@@@@@@     ",
            r#"     &@/   \@@@@@@@/   \@     "#,
            r#"     &@\   /@@@@@@@\   /@     "#,
            "     &@@@@@@@@@@@@@@@@@@@     ",
            "     &@@@  @@@   @@@  @@@     ",
            " @@@ &@@   @@@   @@@  @@@  @@@",
            "@@@@ &@@   @@@   @@@  @@@  &@@",
            "  @@@@@    @@@   @@@   @@@@@@ ",
            "",
        ];

        let screen_w = std::graphics::get_screen_width();
        let screen_h = std::graphics::get_screen_height();

        let ticks = std::os::get_system_ticks();
        let total_seconds = ticks / 1000;
        let h = total_seconds / 3600;
        let m = (total_seconds % 3600) / 60;
        let s = total_seconds % 60;

        let mut info: Vec<String> = Vec::new();
        info.push(format!("{}{}@{}krakeos{}", p_pink, white, p_blue, reset));
        info.push(format!("{}-----------------{}", gray, reset));
        
        let mut os_line = String::from(p_cyan);
        os_line.push(' ');
        os_line.push(core::char::from_u32(0xF300).unwrap_or('?')); 
        os_line.push_str(" OS: ");
        os_line.push_str(white);
        os_line.push_str("KrakeOS");
        os_line.push_str(reset);
        info.push(os_line);

        let mut kernel_line = String::from(p_green);
        kernel_line.push(' ');
        kernel_line.push(core::char::from_u32(0xE8F1).unwrap_or('?'));
        kernel_line.push_str(" Kernel: ");
        kernel_line.push_str(white);
        kernel_line.push_str("KrakeOS Kernel 0.1.0");
        kernel_line.push_str(reset);
        info.push(kernel_line);

        let mut uptime_line = String::from(p_yellow);
        uptime_line.push(' ');
        uptime_line.push(core::char::from_u32(0xF017).unwrap_or('?'));
        uptime_line.push_str(" Uptime: ");
        uptime_line.push_str(white);
        uptime_line.push_str(&format!("{}:{}:{:02}", h, m, s));
        uptime_line.push_str(reset);
        info.push(uptime_line);

        let mut res_line = String::from(p_purple);
        res_line.push(' ');
        res_line.push(core::char::from_u32(0xF26C).unwrap_or('?'));
        res_line.push_str(" Resolution: ");
        res_line.push_str(white);
        res_line.push_str(&format!("{}x{}", screen_w, screen_h));
        res_line.push_str(reset);
        info.push(res_line);

        let mut shell_line = String::from(p_pink);
        shell_line.push(' ');
        shell_line.push(core::char::from_u32(0xE795).unwrap_or('?'));
        shell_line.push_str(" Shell: ");
        shell_line.push_str(white);
        shell_line.push_str("shell");
        shell_line.push_str(reset);
        info.push(shell_line);

        let mut pgu_line = String::from(p_cyan);
        pgu_line.push(' ');
        pgu_line.push(core::char::from_u32(0xF2DB).unwrap_or('?'));
        pgu_line.push_str(" PGU: ");
        pgu_line.push_str(white);
        pgu_line.push_str("virtIO");
        pgu_line.push_str(reset);
        info.push(pgu_line);

        let mut font_line = String::from(p_green);
        font_line.push(' ');
        font_line.push(core::char::from_u32(0xF031).unwrap_or('?'));
        font_line.push_str(" Font: ");
        font_line.push_str(white);
        font_line.push_str("Caskaydia Nerd Font");
        font_line.push_str(reset);
        info.push(font_line);

        info.push(String::new());
        
        let mut palette1 = String::new();
        for i in 0..8 {
            palette1.push_str(&format!("\x1B[{}m  ", 40 + i));
        }
        palette1.push_str(reset);
        info.push(palette1);

        let mut palette2 = String::new();
        for i in 0..8 {
            palette2.push_str(&format!("\x1B[{}m  ", 100 + i));
        }
        palette2.push_str(reset);
        info.push(palette2);

        let ascii_width = 40;

        for i in 0..14 {
            let a_line = if i < ascii.len() { ascii[i] } else { "" };
            let i_line = if i < info.len() { &info[i] } else { "" };

            let mut a_string = String::from(a_line);
            if a_string.chars().count() > ascii_width {
                let mut new_s = String::new();
                for (idx, c) in a_string.chars().enumerate() {
                    if idx >= ascii_width { break; }
                    new_s.push(c);
                }
                a_string = new_s;
            }

            while a_string.chars().count() < ascii_width {
                a_string.push(' ');
            }

            let msg = format!("{}{}{}  {} \n", blue, a_string, reset, i_line);
            std::os::file_write(out_fd, msg.as_bytes());
        }
        std::os::file_write(out_fd, b"\n");
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
                            }
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
                        line.push_str("\x1B[1m\x1B[94m");
                        line.push(core::char::from_u32(0xF07B).unwrap());
                        line.push_str(" ");
                        line.push_str(&entry.name);
                        line.push_str("/");
                        line.push_str("\x1B[0m\n");
                    } else {
                        line.push_str("\x1B[37m");
                        line.push(core::char::from_u32(0xF016).unwrap());
                        line.push_str(" ");
                        line.push_str(&entry.name);
                        line.push_str("\x1B[0m\n");
                    }
                    std::os::file_write(out_fd, line.as_bytes());
                }
            }
            Err(_) => {
                let err = format!("ls: cannot access \"{}\": No such file\n", full_path);
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
                let err = format!("touch: cannot create \"{}\"\n", path);
                std::os::file_write(out_fd, err.as_bytes());
            }
        }
    } else if cmd == "mkdir" {
        if !args.is_empty() {
            let path = resolve_path(cwd, &args[0]);
            if let Err(_) = std::fs::create_dir(&path) {
                let err = format!("mkdir: cannot create \"{}\"\n", path);
                std::os::file_write(out_fd, err.as_bytes());
            }
        }
    } else if cmd == "rm" {
        if !args.is_empty() {
            let path = resolve_path(cwd, &args[0]);
            if let Err(_) = std::fs::remove_file(&path) {
                let err = format!("rm: cannot remove \"{}\"\n", path);
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
                            Ok(n) if n > 0 => { let _ = dst.write(&buf[0..n]); }
                            _ => break,
                        }
                    }
                }
            }
        }
    }
}
