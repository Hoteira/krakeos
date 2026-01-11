extern crate alloc;
use crate::types::Cell;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;

pub struct TerminalBuffer {
    pub lines: Vec<Vec<Cell>>,
    pub alt_lines: Vec<Vec<Cell>>,
    pub is_alt: bool,
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub cursor_visible: bool,

    pub current_fg: u8,
    pub current_bg: u8,
    pub current_bold: bool,

    pub input_buffer: Vec<u8>,
}

impl TerminalBuffer {
    pub fn new() -> Self {
        Self {
            lines: Vec::new(),
            alt_lines: Vec::new(),
            is_alt: false,
            cursor_row: 0,
            cursor_col: 0,
            cursor_visible: true,
            current_fg: 255,
            current_bg: 255,
            current_bold: false,
            input_buffer: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        if self.is_alt {
            self.alt_lines.clear();
        } else {
            self.lines.clear();
        }
        self.cursor_row = 0;
        self.cursor_col = 0;
    }

    pub fn ensure_row(&mut self) {
        let current = if self.is_alt { &mut self.alt_lines } else { &mut self.lines };
        while current.len() <= self.cursor_row {
            current.push(Vec::new());
        }
    }

    pub fn write_char(&mut self, c: char) {
        self.ensure_row();
        let current = if self.is_alt { &mut self.alt_lines } else { &mut self.lines };
        let line = &mut current[self.cursor_row];

        while line.len() <= self.cursor_col {
            line.push(Cell::default());
        }

        let cell = Cell {
            c,
            fg: self.current_fg,
            bg: self.current_bg,
            bold: self.current_bold,
        };

        if self.cursor_col < line.len() {
            line[self.cursor_col] = cell;
        } else {
            line.push(cell);
        }
        self.cursor_col += 1;
    }

    pub fn write_str(&mut self, s: &str) {
        for c in s.chars() {
            if c == '\x1B' { continue; }
            self.write_char(c);
        }
    }

    pub fn newline(&mut self) {
        self.cursor_row += 1;
        self.cursor_col = 0;
    }

    pub fn backspace(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1;
        }
    }

    pub fn clear_line(&mut self) {
        let current = if self.is_alt { &mut self.alt_lines } else { &mut self.lines };
        if self.cursor_row < current.len() {
            current[self.cursor_row].truncate(self.cursor_col);
        }
    }

    pub fn handle_sgr(&mut self, params: &str) {
        if params.is_empty() {
            self.current_fg = 255;
            self.current_bg = 255;
            self.current_bold = false;
            return;
        }

        let parts = params.split(';');
        for part in parts {
            let n = part.parse::<u8>().unwrap_or(0);
            match n {
                0 => {
                    self.current_fg = 255;
                    self.current_bg = 255;
                    self.current_bold = false;
                }
                1 => self.current_bold = true,
                22 => self.current_bold = false,
                30..=37 => self.current_fg = n - 30,
                39 => self.current_fg = 255,
                40..=47 => self.current_bg = n - 40,
                49 => self.current_bg = 255,
                90..=97 => self.current_fg = n - 90 + 8,
                100..=107 => self.current_bg = n - 100 + 8,
                _ => {}
            }
        }
    }

    pub fn render(&self) -> String {
        let current = if self.is_alt { &self.alt_lines } else { &self.lines };
        let mut s = String::new();

        let mut last_fg = 255;
        let mut last_bg = 255;
        let mut last_bold = false;


        s.push_str("\x1B[0m");


        let max_row = current.len().max(if self.cursor_visible { self.cursor_row + 1 } else { 0 });

        for i in 0..max_row {
            if i > 0 {
                s.push('\n');
            }


            let empty_line = Vec::new();
            let line = if i < current.len() { &current[i] } else { &empty_line };


            let line_len = line.len();
            let mut max_col = line_len;
            if self.cursor_visible && i == self.cursor_row {
                max_col = max_col.max(self.cursor_col + 1);
            }

            for j in 0..max_col {
                let mut cell = if j < line_len { line[j] } else { Cell::default() };


                if self.cursor_visible && i == self.cursor_row && j == self.cursor_col {
                    cell.c = '▆';
                }


                if cell.bold != last_bold {
                    if cell.bold {
                        s.push_str("\x1B[1m");
                    } else {
                        s.push_str("\x1B[22m");
                    }
                    last_bold = cell.bold;
                }


                if cell.fg != last_fg {
                    if cell.fg == 255 {
                        s.push_str("\x1B[39m");
                    } else if cell.fg < 8 {
                        s.push_str("\x1B[");
                        s.push_str(&(30 + cell.fg).to_string());
                        s.push('m');
                    } else if cell.fg < 16 {
                        s.push_str("\x1B[");
                        s.push_str(&(90 + cell.fg - 8).to_string());
                        s.push('m');
                    }
                    last_fg = cell.fg;
                }


                if cell.bg != last_bg {
                    if cell.bg == 255 {
                        s.push_str("\x1B[49m");
                    } else if cell.bg < 8 {
                        s.push_str("\x1B[");
                        s.push_str(&(40 + cell.bg).to_string());
                        s.push('m');
                    } else if cell.bg < 16 {
                        s.push_str("\x1B[");
                        s.push_str(&(100 + cell.bg - 8).to_string());
                        s.push('m');
                    }
                    last_bg = cell.bg;
                }

                s.push(cell.c);
            }
        }
        s
    }

    pub fn switch_screen(&mut self, alt: bool) {
        if self.is_alt != alt {
            std::debugln!("[term] Switching to {} screen", if alt { "alternate" } else { "main" });
            self.is_alt = alt;
            if self.is_alt {
                self.alt_lines.clear();
                self.cursor_row = 0;
                self.cursor_col = 0;
            } else {
                self.cursor_row = self.lines.len().saturating_sub(1);
                self.cursor_col = 0;
            }
        }
    }
}
