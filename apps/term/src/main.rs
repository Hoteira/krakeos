#![no_std]
#![no_main]

use inkui::{ Window, Widget, Color, Size };
use std::fs::File;
extern crate alloc;
use alloc::vec::Vec;
use alloc::string::String;
use alloc::format;

static mut TERM_READ_FD: usize = 0;
static mut TERM_WRITE_FD: usize = 0;
static mut CHILD_PID: usize = 0;

struct TerminalBuffer {
    lines: Vec<String>,
    alt_lines: Vec<String>,
    is_alt: bool,
    cursor_row: usize,
    cursor_col: usize,
    cursor_visible: bool,
    
}

impl TerminalBuffer {
    fn new() -> Self {
        Self {
            lines: Vec::new(),
            alt_lines: Vec::new(),
            is_alt: false,
            cursor_row: 0,
            cursor_col: 0,
            cursor_visible: true,
        }
    }

    fn clear(&mut self) {
        if self.is_alt {
            self.alt_lines.clear();
        } else {
            self.lines.clear();
        }
        self.cursor_row = 0;
        self.cursor_col = 0;
    }

    fn ensure_row(&mut self) {
        let current = if self.is_alt { &mut self.alt_lines } else { &mut self.lines };
        while current.len() <= self.cursor_row {
            current.push(String::new());
        }
    }

    fn write_str(&mut self, s: &str) {
        if s.starts_with("\x1B[") {
             // Pass-through escape sequences for inkui
             self.ensure_row();
             let current = if self.is_alt { &mut self.alt_lines } else { &mut self.lines };
             current[self.cursor_row].push_str(s);
             return;
        }

        for c in s.chars() {
            self.write_char(c);
        }
    }
    
    fn write_char(&mut self, c: char) {
        self.ensure_row();
        let current = if self.is_alt { &mut self.alt_lines } else { &mut self.lines };
        let line = &mut current[self.cursor_row];
        
        let char_count = line.chars().count();
        if self.cursor_col < char_count {
            // Replace character
            let mut new_line = String::with_capacity(line.len());
            for (i, existing_c) in line.chars().enumerate() {
                if i == self.cursor_col {
                    new_line.push(c);
                } else {
                    new_line.push(existing_c);
                }
            }
            *line = new_line;
        } else {
            // Pad with spaces if needed
            while line.chars().count() < self.cursor_col {
                line.push(' ');
            }
            line.push(c);
        }
        self.cursor_col += 1;
    }

    fn newline(&mut self) {
        self.cursor_row += 1;
        self.cursor_col = 0;
        self.ensure_row();
    }

    fn insert_line(&mut self) {
        let current = if self.is_alt { &mut self.alt_lines } else { &mut self.lines };
        if self.cursor_row < current.len() {
            current.insert(self.cursor_row, String::new());
        } else {
            self.ensure_row();
        }
    }

    fn delete_line(&mut self) {
        let current = if self.is_alt { &mut self.alt_lines } else { &mut self.lines };
        if self.cursor_row < current.len() {
            current.remove(self.cursor_row);
        }
    }

    fn backspace(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1;
            let current = if self.is_alt { &mut self.alt_lines } else { &mut self.lines };
            if self.cursor_row < current.len() {
                let line = &mut current[self.cursor_row];
                let char_count = line.chars().count();
                if self.cursor_col < char_count {
                    let mut new_line = String::with_capacity(line.len());
                    for (i, c) in line.chars().enumerate() {
                        if i != self.cursor_col {
                            new_line.push(c);
                        }
                    }
                    *line = new_line;
                }
            }
        }
    }

    fn clear_line(&mut self) {
        let current = if self.is_alt { &mut self.alt_lines } else { &mut self.lines };
        if self.cursor_row < current.len() {
            current[self.cursor_row].clear();
            self.cursor_col = 0;
        }
    }

    fn render(&self) -> String {
        let current = if self.is_alt { &self.alt_lines } else { &self.lines };
        
        let mut size = 0;
        for line in current { size += line.len() + 1; }
        let mut s = String::with_capacity(size + 10);
        
        for (r, line) in current.iter().enumerate() {
            if r > 0 { s.push('\n'); }
            
            if self.cursor_visible && r == self.cursor_row {
                let mut added = false;
                let chars: Vec<char> = line.chars().collect();
                for (c, &ch) in chars.iter().enumerate() {
                    if c == self.cursor_col {
                        s.push('\u{2586}'); 
                        added = true;
                    }
                    s.push(ch);
                }
                if !added {
                    for _ in chars.len()..self.cursor_col {
                        s.push(' ');
                    }
                    s.push('\u{2586}');
                }
            } else {
                s.push_str(line);
            }
        }
        
        s
    }
    
    fn switch_screen(&mut self, alt: bool) {
        if self.is_alt != alt {
            self.is_alt = alt;
            if self.is_alt {
                
                
                self.alt_lines.clear();
                
                
                
                self.cursor_row = 0;
            } else {
                
                
                
                self.cursor_row = self.lines.len().saturating_sub(1);
            }
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn main(_argc: i32, _argv: *const *const u8) -> i32 {
    let width = 500;
    let height = 400;
    
    let screen_w = std::graphics::get_screen_width();
    let screen_h = std::graphics::get_screen_height();
    let x = (screen_w / 2).saturating_sub(width / 2);
    let y = (screen_h / 2).saturating_sub(height / 2);

    let mut win = Window::new("krakeOS Term", width, height);
    win.x = x as isize;
    win.y = y as isize;
    
    let mut ctrl_pressed = false;
    const KEY_CTRL: u32 = 0x110005;
    
    {
        if let Ok(mut file) = File::open("@0xE0/sys/fonts/CaskaydiaNerd.ttf") {
            let size = file.size();
            let buffer_addr = std::memory::malloc(size);
            let buffer = unsafe { core::slice::from_raw_parts_mut(buffer_addr as *mut u8, size) };
            if file.read(buffer).is_ok() {
                let static_buf = unsafe { core::slice::from_raw_parts(buffer_addr as *const u8, size) };
                win.load_font(static_buf);
            }
        }
    }

    
    let mut fds_out = [0i32; 2];
    std::os::pipe(&mut fds_out);
    unsafe { TERM_READ_FD = fds_out[0] as usize; } 

    let mut fds_in = [0i32; 2];
    std::os::pipe(&mut fds_in);
    unsafe { TERM_WRITE_FD = fds_in[1] as usize; } 

    let fds_map = [
        (0, fds_in[0] as u8),
        (1, fds_out[1] as u8),
        (2, fds_out[1] as u8),
    ];
    
    let args = std::args();
    let cmd_to_run = if args.len() > 1 {
        &args[1]
    } else {
        "@0xE0/sys/bin/shell.elf"
    };

    unsafe {
        let env = ["TERM=xterm"];
        CHILD_PID = std::os::spawn_with_fds(cmd_to_run, &fds_map, Some(&env));
    }
    
    
    std::os::file_close(fds_in[0] as usize);
    std::os::file_close(fds_out[1] as usize);

    
    let mut root = Widget::frame(1)
        .width(Size::Relative(100))
        .height(Size::Relative(100))
        .background_color(Color::rgba(0, 0, 0, 164));
    
    let term_display = Widget::label(2, "")
        .width(Size::Relative(92))
        .height(Size::Relative(90)) 
        .x(Size::Relative(4))
        .y(Size::Relative(5))
        .padding(Size::Absolute(10))
        .set_text_color(Color::rgb(255, 255, 255)) 
        .set_text_size(14.0)
        .background_color(Color::rgba(0, 0, 0, 0));

    root = root.add_child(term_display);
    win.children.push(root);
    win.show();
    win.draw();
    win.update();
    
    let mut term_buffer = TerminalBuffer::new();
    let mut pipe_buf = [0u8; 4096];

    loop {
        
        use inkui::Event;
        let mut events: [Event; 16] = [Event::None; 16];
        unsafe {
            std::os::syscall(52, win.id as u64, events.as_mut_ptr() as u64, 16);
        }
        
        for event in events.iter() {
            match event {
                Event::Keyboard(e) => {
                    if e.key == KEY_CTRL {
                        ctrl_pressed = e.pressed;
                    } else if e.pressed {
                        if let Some(c) = core::char::from_u32(e.key) {
                            if ctrl_pressed && (c == 'c' || c == 'C') {
                                std::os::file_write(unsafe { TERM_WRITE_FD }, &[0x03]); // Send ETX (Ctrl+C)
                                win.draw();
                                win.update();
                                continue;
                            }

                            for _ in 0..e.repeat {
                                let mut buf = [0u8; 4];
                                let bytes_to_write = if ctrl_pressed {
                                    if c >= 'a' && c <= 'z' {
                                        let code = (c as u8) - b'a' + 1;
                                        buf[0] = code;
                                        &buf[0..1]
                                    } else if c >= 'A' && c <= 'Z' {
                                        let code = (c as u8) - b'A' + 1;
                                        buf[0] = code;
                                        &buf[0..1]
                                    } else {
                                        c.encode_utf8(&mut buf).as_bytes()
                                    }
                                } else {
                                    c.encode_utf8(&mut buf).as_bytes()
                                };
                                std::os::file_write(unsafe { TERM_WRITE_FD }, bytes_to_write);
                            }
                            win.draw();
                            win.update();
                        }
                    }
                },
                Event::Mouse(e) => {
                    if e.scroll != 0 {
                        
                        if let Some(widget) = win.find_widget_by_id_mut(2) {
                            widget.handle_scroll(e.scroll);
                            win.draw();
                            win.update();
                        }
                    }
                },
                Event::Resize(e) => {
                    win.resize(e.width, e.height, true);
                    win.draw();
                    win.update();
                },
                _ => {} 
            }
        }

        let old_cursor_row = term_buffer.cursor_row;
        let mut old_scroll_y = 0;
        if let Some(w) = win.find_widget_by_id(2) {
            old_scroll_y = w.geometry().scroll_offset_y;
        }

        let n = std::os::file_read(unsafe { TERM_READ_FD }, &mut pipe_buf);
        if n > 0 && n != usize::MAX {
            let mut i = 0;
            let mut has_newline = false;
            while i < n {
                let b = pipe_buf[i];
                if b == 0x08 { 
                    term_buffer.backspace();
                    i += 1;
                } else if b == b'\n' || b == b'\r' {
                    term_buffer.newline();
                    has_newline = true;
                    i += 1;
                } else if b == 0x1B { 
                    if i + 1 < n && pipe_buf[i+1] == b'[' {
                        
                        let mut j = i + 2;
                        let mut end_found = false;
                        while j < n && j < i + 32 {
                            let c = pipe_buf[j];
                            if c >= 0x40 && c <= 0x7E {
                                end_found = true;
                                break;
                            }
                            j += 1;
                        }

                        if end_found {
                            let cmd = pipe_buf[j];
                            let seq = &pipe_buf[i+2..j]; 
                            
                            match cmd {
                                b'A' => { // Cursor Up
                                    let n = if seq.is_empty() { 1 } else { 
                                        unsafe { core::str::from_utf8_unchecked(seq) }.parse::<usize>().unwrap_or(1)
                                    };
                                    term_buffer.cursor_row = term_buffer.cursor_row.saturating_sub(n);
                                },
                                b'B' => { // Cursor Down
                                    let n = if seq.is_empty() { 1 } else { 
                                        unsafe { core::str::from_utf8_unchecked(seq) }.parse::<usize>().unwrap_or(1)
                                    };
                                    term_buffer.cursor_row += n;
                                },
                                b'C' => { // Cursor Forward
                                    let n = if seq.is_empty() { 1 } else { 
                                        unsafe { core::str::from_utf8_unchecked(seq) }.parse::<usize>().unwrap_or(1)
                                    };
                                    term_buffer.cursor_col += n;
                                },
                                b'D' => { // Cursor Backward
                                    let n = if seq.is_empty() { 1 } else { 
                                        unsafe { core::str::from_utf8_unchecked(seq) }.parse::<usize>().unwrap_or(1)
                                    };
                                    term_buffer.cursor_col = term_buffer.cursor_col.saturating_sub(n);
                                },
                                b'J' => {
                                    
                                    if seq == b"2" {
                                        term_buffer.clear();
                                    }
                                },
                                b'H' | b'f' => { // CUP or HVP
                                    
                                    if seq.is_empty() {
                                        term_buffer.cursor_row = 0;
                                        term_buffer.cursor_col = 0;
                                    } else {
                                        let s = unsafe { core::str::from_utf8_unchecked(seq) };
                                        let parts: Vec<&str> = s.split(';').collect();
                                        if parts.len() >= 2 {
                                            if let Ok(r) = parts[0].parse::<usize>() {
                                                term_buffer.cursor_row = r.saturating_sub(1);
                                            }
                                            if let Ok(c) = parts[1].parse::<usize>() {
                                                term_buffer.cursor_col = c.saturating_sub(1);
                                            }
                                        } else if !parts.is_empty() {
                                            if let Ok(r) = parts[0].parse::<usize>() {
                                                term_buffer.cursor_row = r.saturating_sub(1);
                                            }
                                        }
                                    }
                                },
                                b'K' => {
                                    term_buffer.clear_line();
                                },
                                b'L' => { // Insert Line
                                    term_buffer.insert_line();
                                },
                                b'M' => { // Delete Line
                                    term_buffer.delete_line();
                                },
                                b'G' => { // CHA - Horizontal Absolute
                                    let n = if seq.is_empty() { 1 } else { 
                                        unsafe { core::str::from_utf8_unchecked(seq) }.parse::<usize>().unwrap_or(1)
                                    };
                                    term_buffer.cursor_col = n.saturating_sub(1);
                                },
                                b'd' => { // VPA - Vertical Absolute
                                    let n = if seq.is_empty() { 1 } else { 
                                        unsafe { core::str::from_utf8_unchecked(seq) }.parse::<usize>().unwrap_or(1)
                                    };
                                    term_buffer.cursor_row = n.saturating_sub(1);
                                },
                                b'm' => {
                                    let seq_full = unsafe { core::str::from_utf8_unchecked(&pipe_buf[i..j+1]) };
                                    term_buffer.write_str(seq_full);
                                },
                                b'h' => {
                                    
                                    
                                    
                                    if seq.len() > 1 && seq[0] == b'?' {
                                        let param = unsafe { core::str::from_utf8_unchecked(&seq[1..]) };
                                        if param == "25" {
                                            term_buffer.cursor_visible = true;
                                        } else if param == "1049" {
                                            term_buffer.switch_screen(true);
                                        }
                                    }
                                },
                                b'l' => {
                                    
                                    if seq.len() > 1 && seq[0] == b'?' {
                                        let param = unsafe { core::str::from_utf8_unchecked(&seq[1..]) };
                                        if param == "25" {
                                            term_buffer.cursor_visible = false;
                                        } else if param == "1049" {
                                            term_buffer.switch_screen(false);
                                        }
                                    }
                                },
                                b't' => {
                                    
                                    
                                    if seq == b"18" {
                                        if let Some(widget) = win.find_widget_by_id(2) {
                                            if let inkui::widget::Widget::Label { text, geometry, .. } = widget {
                                                 let padding = 10; 
                                                 let width = geometry.width.saturating_sub(padding * 2);
                                                 let height = geometry.height.saturating_sub(padding * 2);
                                                 
                                                 let char_width = (text.size as f32 * 0.6) as usize;
                                                 let line_height = (text.size as f32 * 1.2) as usize;
                                                 
                                                 if char_width > 0 && line_height > 0 {
                                                     let cols = width / char_width;
                                                     let rows = height / line_height;
                                                     let resp = format!("\x1B[8;{};{}t", rows, cols);
                                                     std::os::file_write(unsafe { TERM_WRITE_FD }, resp.as_bytes());
                                                 }
                                            }
                                        }
                                    }
                                },
                                _ => {} 
                            }
                            i = j + 1;
                        } else {
                            i += 1;
                        }
                    } else {
                         i += 1;
                    }
                } else {
                    let mut len = 1;
                    if (b & 0xE0) == 0xC0 { len = 2; }
                    else if (b & 0xF0) == 0xE0 { len = 3; }
                    else if (b & 0xF8) == 0xF0 { len = 4; }
                    
                    if i + len <= n {
                        let s = unsafe { core::str::from_utf8_unchecked(&pipe_buf[i..i+len]) };
                        term_buffer.write_str(s);
                        i += len;
                    } else {
                        i += 1; 
                    }
                }
            }

            if let Some(widget) = win.find_widget_by_id_mut(2) {
                if let inkui::widget::Widget::Label { text, geometry, .. } = widget {
                    text.text = term_buffer.render();
                    
                    
                    
                    let padding = 10; 
                    let width = geometry.width.saturating_sub(padding * 2);
                    
                    if width > 0 {
                        
                        let char_width = (text.size as f32 * 0.6) as usize; 
                        if char_width > 0 {
                            let chars_per_line = width / char_width;
                            let mut visual_lines = 0;
                            
                            let current_lines = if term_buffer.is_alt { &term_buffer.alt_lines } else { &term_buffer.lines };
                            for line in current_lines {
                                let len = line.chars().count(); 
                                if len == 0 {
                                    visual_lines += 1;
                                }
                                 else {
                                    visual_lines += (len + chars_per_line - 1) / chars_per_line;
                                }
                            }
                            
                            
                            
                            let line_height = (text.size as f32 * 1.2) as usize;
                            let content_height = visual_lines * line_height;
                            
                            
                            
                            let extra_margin = line_height * 2; 
                            
                            let widget_height = geometry.height.saturating_sub(padding * 2);
                            if content_height + extra_margin > widget_height {
                                geometry.scroll_offset_y = (content_height + extra_margin) - widget_height;
                            } else {
                                geometry.scroll_offset_y = 0;
                            }
                        }
                    }
                }
            }
            win.draw();
            
            
            let mut partial_update = false;
            let new_scroll_y = if let Some(w) = win.find_widget_by_id(2) { w.geometry().scroll_offset_y } else { 0 };

            if !has_newline && old_scroll_y == new_scroll_y && old_cursor_row == term_buffer.cursor_row {
                if let Some(widget) = win.find_widget_by_id(2) {
                    if let inkui::widget::Widget::Label { text, geometry, .. } = widget {
                        let line_height = (text.size as f32 * 1.2) as usize;
                        let scroll_y = geometry.scroll_offset_y;

                        let padding = 10;
                        let width = geometry.width.saturating_sub(padding * 2);
                        let char_width = (text.size as f32 * 0.6) as usize;
                        
                        let current_lines = if term_buffer.is_alt { &term_buffer.alt_lines } else { &term_buffer.lines };
                        let mut prev_visual_lines = 0;
                        
                        if width > 0 && char_width > 0 {
                            let chars_per_line = width / char_width;
                            for (i, line) in current_lines.iter().enumerate() {
                                if i == term_buffer.cursor_row {
                                    break;
                                }
                                let len = line.chars().count();
                                if len == 0 { prev_visual_lines += 1; } else { prev_visual_lines += (len + chars_per_line - 1) / chars_per_line; }
                            }
                            
                            let current_line = &current_lines[term_buffer.cursor_row];
                            let len = current_line.chars().count();
                            let current_row_visual_lines = if len == 0 { 1 } else { (len + chars_per_line - 1) / chars_per_line };
                            
                            let row_y_start = prev_visual_lines * line_height;
                            let update_height = (current_row_visual_lines + 1) * line_height;

                            if row_y_start + update_height >= scroll_y {
                                let relative_y = if row_y_start >= scroll_y {
                                    row_y_start - scroll_y
                                } else {
                                    0
                                };
                                let screen_y = geometry.y + geometry.padding + relative_y;

                                if screen_y < win.height {
                                     win.update_area(0, screen_y, win.width, update_height + 5);
                                     partial_update = true;
                                }
                            }
                        }
                    }
                }
            }

            if !partial_update {
                win.update();
            }
        }

        std::os::yield_task();
    }
}
