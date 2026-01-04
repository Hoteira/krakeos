#![no_std]
#![no_main]

use std::{println};
extern crate alloc;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use std::os::ProcessInfo;

const STDIN_FD: usize = 0;
const STDOUT_FD: usize = 1;

struct AppState {
    processes: Vec<ProcessInfo>,
    selected_index: usize,
    scroll_offset: usize,
    screen_height: usize,
}

impl AppState {
    fn new() -> Self {
        let mut app = AppState {
            processes: Vec::new(),
            selected_index: 0,
            scroll_offset: 0,
            screen_height: 20, 
        };
        app.refresh();
        app
    }

    fn refresh(&mut self) {
        self.processes = std::os::get_process_list();
        if self.selected_index >= self.processes.len() {
            self.selected_index = self.processes.len().saturating_sub(1);
        }
    }

    fn draw(&self) {
        // Clear screen and home cursor
        std::os::file_write(STDOUT_FD, b"\x1B[2J\x1B[H");

        // Header
        let header = format!("\x1B[1;37;44m {:<6} {:<10} {:<30} {:<10} \x1B[0m\n", "PID", "State", "Name", "RAM");
        std::os::file_write(STDOUT_FD, header.as_bytes());

        // Process List
        let max_rows = self.screen_height.saturating_sub(2);
        let end_index = (self.scroll_offset + max_rows).min(self.processes.len());

        for i in self.scroll_offset..end_index {
            let p = &self.processes[i];
            
            if i == self.selected_index {
                std::os::file_write(STDOUT_FD, b"\x1B[38;2;200;160;255m"); // Pastel Purple
            } else {
                std::os::file_write(STDOUT_FD, b"\x1B[37m"); // White
            }

            let state_str = match p.state {
                0 => "Null",
                1 => "Rsrvd",
                2 => "Ready",
                3 => "Zombie",
                4 => "Sleep",
                _ => "Unk",
            };

            let name_str = String::from_utf8_lossy(&p.name);
            let clean_name = name_str.trim_matches(char::from(0));
            
            let mem_bytes = std::os::get_process_memory(p.pid);
            let mem_str = if mem_bytes >= 1024 * 1024 {
                format!("{:.1} MB", mem_bytes as f32 / 1024.0 / 1024.0)
            } else if mem_bytes >= 1024 {
                format!("{} KB", mem_bytes / 1024)
            } else {
                format!("{} B", mem_bytes)
            };

            let content = format!(" {:<6} {:<10} {:<30} {:<10}", p.pid, state_str, clean_name, mem_str);
            let padded = format!("{:<60}\n", content);
            std::os::file_write(STDOUT_FD, padded.as_bytes());
        }

        // Reset colors
        std::os::file_write(STDOUT_FD, b"\x1B[0m");

        // Status Bar
        std::os::file_write(STDOUT_FD, b"\n\x1B[90m[W/S] Move  [K] Kill  [R] Refresh  [Q] Quit\x1B[0m");
    }

    fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
            if self.selected_index < self.scroll_offset {
                self.scroll_offset = self.selected_index;
            }
        }
    }

    fn move_down(&mut self) {
        if self.selected_index + 1 < self.processes.len() {
            self.selected_index += 1;
            let max_rows = self.screen_height.saturating_sub(2);
            if self.selected_index >= self.scroll_offset + max_rows {
                self.scroll_offset += 1;
            }
        }
    }

    fn kill_selected(&mut self) {
        if self.selected_index < self.processes.len() {
            let pid = self.processes[self.selected_index].pid;
            if pid > 2 {
                 unsafe { std::os::syscall(78, pid, 0, 0) };
                 self.refresh();
            }
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn main(_argc: i32, _argv: *const *const u8) -> i32 {
    let mut app = AppState::new();
    let mut needs_redraw = true;

    // Enable alternate buffer and hide cursor
    std::os::file_write(STDOUT_FD, b"\x1B[?1049h\x1B[?25l");

    loop {
        if needs_redraw {
            app.draw();
            needs_redraw = false;
        }

        let mut buf = [0u8; 1];
        if std::os::file_read(STDIN_FD, &mut buf) > 0 {
             let c = buf[0] as char;
             match c {
                 'w' | 'W' => { app.move_up(); needs_redraw = true; },
                 's' | 'S' => { app.move_down(); needs_redraw = true; },
                 'k' | 'K' => { app.kill_selected(); needs_redraw = true; },
                 'r' | 'R' => { app.refresh(); needs_redraw = true; },
                 'q' | 'Q' => {
                     // Restore buffer and cursor
                     std::os::file_write(STDOUT_FD, b"\x1B[?1049l\x1B[?25h");
                     std::os::exit(0);
                 },
                 _ => {}
             }
        } else {
            std::os::yield_task();
        }
    }
}
