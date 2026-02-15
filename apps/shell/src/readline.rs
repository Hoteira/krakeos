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

    pub fn read_line(&mut self, prompt: &str) -> Option<String> {
        file_write(STDOUT, prompt.as_bytes());
        
        let mut buffer: Vec<char> = Vec::new();
        let mut cursor = 0;
        self.history_index = self.history.len();

        loop {
            let mut b = [0u8; 1];
            if file_read(STDIN, &mut b) == 0 {
                std::os::yield_task();
                continue;
            }
            let byte = b[0];

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
                    if file_read(STDIN, &mut b) > 0 && b[0] == b'[' {
                        if file_read(STDIN, &mut b) > 0 {
                            match b[0] {
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
                            if file_read(STDIN, &mut b) > 0 {
                                utf8_buf.push(b[0]);
                            }
                        }
                    }

                    if let Ok(s) = core::str::from_utf8(&utf8_buf) {
                        if let Some(ch) = s.chars().next() {
                            buffer.insert(cursor, ch);
                            cursor += 1;
                            
                            // Echo the UTF-8 bytes back
                            file_write(STDOUT, &utf8_buf);
                            
                            if cursor < buffer.len() {
                                self.redraw_line(prompt, &buffer, cursor);
                            }
                        }
                    }
                }
                c if c >= 0x20 => {
                    buffer.insert(cursor, c as char);
                    cursor += 1;
                    if cursor == buffer.len() {
                        file_write(STDOUT, &[c]);
                    } else {
                        self.redraw_line(prompt, &buffer, cursor);
                    }
                }
                _ => {}
            }
        }
    }

    fn redraw_line(&self, prompt: &str, buffer: &[char], cursor: usize) {
        // Move to start of line
        file_write(STDOUT, b"\r");
        file_write(STDOUT, prompt.as_bytes());
        let s: String = buffer.iter().collect();
        file_write(STDOUT, s.as_bytes());
        // Clear to end of line
        file_write(STDOUT, b"\x1B[K");
        
        // Restore cursor
        let total_len = buffer.len();
        if cursor < total_len {
            let diff = total_len - cursor;
            let mut moves = String::new();
            for _ in 0..diff {
                moves.push_str("\x1B[D");
            }
            file_write(STDOUT, moves.as_bytes());
        }
    }
}