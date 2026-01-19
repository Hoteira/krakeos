#![no_std]
#![no_main]

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
        std::os::file_write(STDOUT_FD, b"\x1B[2J\x1B[H");
        std::os::file_write(STDOUT_FD, b"\x1B[1;37;42m SYSMON - System Monitor \x1B[0m\n\n");
        std::os::file_write(STDOUT_FD, b"\x1B[1m  PID   STATE   NAME\x1B[0m\n");

        for (i, proc) in self.processes.iter().enumerate().skip(self.scroll_offset).take(self.screen_height) {
            if i == self.selected_index {
                std::os::file_write(STDOUT_FD, b"\x1B[7m");
            }

            let state_str = match proc.state {
                0 => "RUN  ",
                1 => "SLEEP",
                2 => "ZOMB ",
                _ => "UNKN ",
            };

            let name = String::from_utf8_lossy(&proc.name);
            let name_trimmed = name.trim_matches('\0');

            let line = format!("  {:<5} {:<7} {}\n", proc.pid, state_str, name_trimmed);
            std::os::file_write(STDOUT_FD, line.as_bytes());

            if i == self.selected_index {
                std::os::file_write(STDOUT_FD, b"\x1B[0m");
            }
        }

        let footer = format!("\n\x1B[90mTotal Processes: {}  [W/S] Move  [K] Kill  [Q] Quit\x1B[0m", self.processes.len());
        std::os::file_write(STDOUT_FD, footer.as_bytes());
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
            if self.selected_index >= self.scroll_offset + self.screen_height {
                self.scroll_offset = self.selected_index - self.screen_height + 1;
            }
        }
    }

    fn kill_selected(&mut self) {
        if self.selected_index < self.processes.len() {
            let pid = self.processes[self.selected_index].pid;
            unsafe {
                std::os::syscall(62, pid, 9, 0);
            }
            self.refresh();
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn main() -> i32 {
    let mut app = AppState::new();
    let mut needs_redraw = true;


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
                'w' | 'W' => {
                    app.move_up();
                    needs_redraw = true;
                }
                's' | 'S' => {
                    app.move_down();
                    needs_redraw = true;
                }
                'k' | 'K' => {
                    app.kill_selected();
                    needs_redraw = true;
                }
                'r' | 'R' => {
                    app.refresh();
                    needs_redraw = true;
                }
                'q' | 'Q' => {
                    std::os::file_write(STDOUT_FD, b"\x1B[?1049l\x1B[?25h");
                    return 0;
                }
                _ => {}
            }
        } else {
            std::os::yield_task();
        }
    }
}