use alloc::string::{String, ToString};
use alloc::vec::Vec;
use std::os::{file_read, file_write};

const STDIN: usize = 0;
const STDOUT: usize = 1;

pub struct LineReader {
    history: Vec<String>,
    history_index: usize,
}

impl LineReader {
    pub fn new() -> Self {
        Self {
            history: Vec::new(),
            history_index: 0,
        }
    }

    pub fn add_history(&mut self, line: String) {
        if !line.trim().is_empty() {
            if self.history.last() != Some(&line) {
                self.history.push(line);
            }
        }
        self.history_index = self.history.len();
    }

    fn read_byte(&self) -> Option<u8> {
        let mut b = [0u8; 1];
        loop {
            let n = file_read(STDIN, &mut b);
            if n == 0 || n == usize::MAX {
                return None;
            }
            if n == usize::MAX - 1 {
                std::os::yield_task();
                continue;
            }
            return Some(b[0]);
        }
    }

    pub fn read_line(&mut self, prompt: &str) -> Option<String> {
        file_write(STDOUT, prompt.as_bytes());
        
        let mut buffer: Vec<char> = Vec::new();
        let mut cursor = 0;
        self.history_index = self.history.len();

        loop {
            let byte = match self.read_byte() {
                Some(b) => b,
                None => return None,
            };

            match byte {
                b'\r' | b'\n' => {
                    file_write(STDOUT, b"\n");
                    return Some(buffer.into_iter().collect());
                }
                0x03 => { // Ctrl+C
                    file_write(STDOUT, b"^C\n");
                    return Some(String::new());
                }
                0x04 => { // Ctrl+D
                    if buffer.is_empty() {
                        file_write(STDOUT, b"exit\n");
                        return None; 
                    }
                }
                0x0C => { // Ctrl+L
                    file_write(STDOUT, b"\x1B[2J\x1B[H");
                    file_write(STDOUT, prompt.as_bytes());
                    let s: String = buffer.iter().collect();
                    file_write(STDOUT, s.as_bytes());
                    // Restore cursor
                    let diff = buffer.len() - cursor;
                    for _ in 0..diff {
                        file_write(STDOUT, b"\x1B[D");
                    }
                }
                0x7F | 0x08 => { // Backspace
                    if cursor > 0 {
                        buffer.remove(cursor - 1);
                        cursor -= 1;
                        self.redraw_line(prompt, &buffer, cursor);
                    }
                }
                0x1B => { // Escape sequence
                    if let Some(b1) = self.read_byte() {
                        if b1 == b'[' {
                            if let Some(b2) = self.read_byte() {
                                match b2 {
                                    b'A' => { // Up
                                        if self.history_index > 0 {
                                            self.history_index -= 1;
                                            buffer = self.history[self.history_index].chars().collect();
                                            cursor = buffer.len();
                                            self.redraw_line(prompt, &buffer, cursor);
                                        }
                                    }
                                    b'B' => { // Down
                                        if self.history_index + 1 < self.history.len() {
                                            self.history_index += 1;
                                            buffer = self.history[self.history_index].chars().collect();
                                        } else {
                                            self.history_index = self.history.len();
                                            buffer.clear();
                                        }
                                        cursor = buffer.len();
                                        self.redraw_line(prompt, &buffer, cursor);
                                    }
                                    b'D' => { // Left
                                        if cursor > 0 {
                                            cursor -= 1;
                                            file_write(STDOUT, b"\x1B[D");
                                        }
                                    }
                                    b'C' => { // Right
                                        if cursor < buffer.len() {
                                            cursor += 1;
                                            file_write(STDOUT, b"\x1B[C");
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                c if c >= 0x80 => {
                    // UTF-8 Handling
                    let mut utf8_buf = Vec::new();
                    utf8_buf.push(c);
                    
                    let expected_len = if (c & 0xE0) == 0xC0 { 2 }
                    else if (c & 0xF0) == 0xE0 { 3 }
                    else if (c & 0xF8) == 0xF0 { 4 }
                    else { 0 };

                    if expected_len > 1 {
                        for _ in 1..expected_len {
                            if let Some(next_b) = self.read_byte() {
                                utf8_buf.push(next_b);
                            }
                        }
                    }

                    if let Ok(s) = core::str::from_utf8(&utf8_buf) {
                        if let Some(ch) = s.chars().next() {
                            buffer.insert(cursor, ch);
                            cursor += 1;

                            // End-of-line typing is echoed locally by the
                            // terminal; only a mid-line insert needs a reprint.
                            if cursor < buffer.len() {
                                self.redraw_line(prompt, &buffer, cursor);
                            }
                        }
                    }
                }
                c if c >= 0x20 => {
                    buffer.insert(cursor, c as char);
                    cursor += 1;
                    // End-of-line typing is echoed locally by the terminal (no
                    // round-trip). Only a mid-line insert needs the shell to
                    // reprint the line authoritatively.
                    if cursor < buffer.len() {
                        self.redraw_line(prompt, &buffer, cursor);
                    }
                }
                _ => {}
            }
        }
    }

    fn redraw_line(&self, prompt: &str, buffer: &[char], cursor: usize) {
        // Build a single output string: CR + prompt + buffer + clear-to-EOL + cursor moves
        let total_len = buffer.len();
        let diff = if cursor < total_len { total_len - cursor } else { 0 };

        let mut out = String::with_capacity(1 + prompt.len() + total_len + 4 + diff * 3);
        out.push('\r');
        out.push_str(prompt);
        for &ch in buffer {
            out.push(ch);
        }
        // Clear to end of line
        out.push_str("\x1B[K");
        // Restore cursor position
        for _ in 0..diff {
            out.push_str("\x1B[D");
        }
        file_write(STDOUT, out.as_bytes());
    }
}