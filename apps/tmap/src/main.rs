#![no_std]
#![no_main]

extern crate alloc;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use std::fs;

const STDIN_FD: usize = 0;
const STDOUT_FD: usize = 1;

struct AppState {
    current_path: String,
    entries: Vec<fs::DirEntry>,
    selected_index: usize,
}

impl AppState {
    fn new(start_path: &str) -> Self {
        let mut app = AppState {
            current_path: String::from(start_path),
            entries: Vec::new(),
            selected_index: 0,
        };
        app.refresh();
        app
    }

    fn refresh(&mut self) {
        self.entries.clear();
        self.selected_index = 0;


        if self.current_path != "@0xE0" && self.current_path.len() > 5 {
            self.entries.push(fs::DirEntry {
                name: String::from(".."),
                file_type: fs::FileType::Directory,
            });
        }

        if let Ok(items) = fs::read_dir(&self.current_path) {
            for item in items {
                self.entries.push(item);
            }
        }
    }

    fn draw(&self) {
        std::os::file_write(STDOUT_FD, b"\x1B[2J\x1B[H");


        let header = format!("\x1B[1m\x1B[37m\x1B[44m TMAP - {}\x1B[0m\n\n", self.current_path);
        std::os::file_write(STDOUT_FD, header.as_bytes());


        for (i, entry) in self.entries.iter().enumerate() {
            if i == self.selected_index {
                std::os::file_write(STDOUT_FD, b"\x1B[7m > ");
            } else {
                std::os::file_write(STDOUT_FD, b"   ");
            }

            if entry.file_type == fs::FileType::Directory {
                std::os::file_write(STDOUT_FD, "\x1B[1m\x1B[94m\u{F07B} ".as_bytes());
            } else {
                std::os::file_write(STDOUT_FD, "\x1B[37m\u{F016} ".as_bytes());
            }

            std::os::file_write(STDOUT_FD, entry.name.as_bytes());
            std::os::file_write(STDOUT_FD, b"\x1B[0m\n");
        }


        std::os::file_write(STDOUT_FD, b"\n\x1B[90m[W/S] Move  [Enter] Open  [Q] Quit\x1B[0m");
    }

    fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    fn move_down(&mut self) {
        if self.selected_index + 1 < self.entries.len() {
            self.selected_index += 1;
        }
    }

    fn enter(&mut self) {
        if self.selected_index < self.entries.len() {
            let entry = &self.entries[self.selected_index];
            if entry.name == ".." {
                if let Some(last_slash) = self.current_path.rfind('/') {
                    self.current_path.truncate(last_slash);
                } else {
                    self.current_path = String::from("@0xE0");
                }
                self.refresh();
            } else if entry.file_type == fs::FileType::Directory {
                if !self.current_path.ends_with('/') {
                    self.current_path.push('/');
                }
                self.current_path.push_str(&entry.name);
                self.refresh();
            }
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn main() -> i32 {
    let mut app = AppState::new("@0xE0");
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
                '\n' | '\r' => {
                    app.enter();
                    needs_redraw = true;
                }
                'q' | 'Q' => {
                    std::os::file_write(STDOUT_FD, b"\x1B[?1049l\x1B[?25h");
                    std::os::exit(0);
                }
                _ => {}
            }
        } else {
            std::os::yield_task();
        }
    }
}