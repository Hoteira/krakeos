#![no_std]
#![no_main]

use inkui::{ Window, Widget, Color, Size };
use std::fs::File;
extern crate alloc;
use alloc::vec::Vec;
use alloc::string::String;
use alloc::format;

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

static mut TERM_READ_FD: usize = 0;
static mut TERM_WRITE_FD: usize = 0;

struct TerminalBuffer {
    lines: Vec<String>,
    alt_lines: Vec<String>,
    is_alt: bool,
    cursor_row: usize,
    cursor_visible: bool,
    // cursor_col is ignored for now, we assume simple line operations or append
}

impl TerminalBuffer {
    fn new() -> Self {
        Self {
            lines: Vec::new(),
            alt_lines: Vec::new(),
            is_alt: false,
            cursor_row: 0,
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
    }

    fn ensure_row(&mut self) {
        let current = if self.is_alt { &mut self.alt_lines } else { &mut self.lines };
        while current.len() <= self.cursor_row {
            current.push(String::new());
        }
    }

    fn write_str(&mut self, s: &str) {
        self.ensure_row();
        let current = if self.is_alt { &mut self.alt_lines } else { &mut self.lines };
        current[self.cursor_row].push_str(s);
    }
    
    fn write_char(&mut self, c: char) {
        self.ensure_row();
        let current = if self.is_alt { &mut self.alt_lines } else { &mut self.lines };
        current[self.cursor_row].push(c);
    }

    fn newline(&mut self) {
        self.cursor_row += 1;
    }

    fn backspace(&mut self) {
        let current = if self.is_alt { &mut self.alt_lines } else { &mut self.lines };
        if self.cursor_row < current.len() {
            current[self.cursor_row].pop();
        }
    }

    fn clear_line(&mut self) {
        let current = if self.is_alt { &mut self.alt_lines } else { &mut self.lines };
        if self.cursor_row < current.len() {
            current[self.cursor_row].clear();
        }
    }

    fn render(&self) -> String {
        let current = if self.is_alt { &self.alt_lines } else { &self.lines };
        // Efficient join?
        let mut size = 0;
        for line in current { size += line.len() + 1; }
        let mut s = String::with_capacity(size);
        for (i, line) in current.iter().enumerate() {
            if i > 0 { s.push('\n'); }
            s.push_str(line);
        }
        // Add cursor if visible and at the end? (Simple visual hack for now)
        if self.cursor_visible {
             s.push('\u{2586}'); // Lower 3/4 Block cursor char
        }
        s
    }
    
    fn switch_screen(&mut self, alt: bool) {
        if self.is_alt != alt {
            self.is_alt = alt;
            if self.is_alt {
                // Entering alt screen: Save cursor row of main screen?
                // For now, alt screen starts fresh.
                self.alt_lines.clear();
                // We keep cursor_row as a shared index or reset?
                // Usually alt screen has its own cursor pos.
                // Let's reset cursor for alt screen.
                self.cursor_row = 0;
            } else {
                // Leaving alt screen: Restore main screen logic
                // Ideally we should have saved the main screen cursor row.
                // For this simple implementation, we just set it to the end of the main lines.
                self.cursor_row = self.lines.len().saturating_sub(1);
            }
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let heap_size = 1024 * 1024 * 10;
    let heap_ptr = std::memory::malloc(heap_size);
    std::memory::heap::init_heap(heap_ptr as *mut u8, heap_size);

    let width = 600;
    let height = 400;
    
    let screen_w = std::graphics::get_screen_width();
    let screen_h = std::graphics::get_screen_height();
    let x = (screen_w / 2).saturating_sub(width / 2);
    let y = (screen_h / 2).saturating_sub(height / 2);

    let mut win = Window::new("krakeOS Term", width, height);
    win.x = x;
    win.y = y;
    
    {
        if let Ok(mut file) = File::open("@0xE0/sys/fonts/CaskaydiaMonoNerdFontPropo-Regular.ttf") {
            let size = file.size();
            let buffer_addr = std::memory::malloc(size);
            let buffer = unsafe { core::slice::from_raw_parts_mut(buffer_addr as *mut u8, size) };
            if file.read(buffer).is_ok() {
                let static_buf = unsafe { core::slice::from_raw_parts(buffer_addr as *const u8, size) };
                win.load_font(static_buf);
            }
        }
    }

    // --- PIPES SETUP ---
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
    
    std::os::spawn_with_fds("@0xE0/sys/bin/shell.elf", &fds_map);
    
    // Close the ends we don't use
    std::os::file_close(fds_in[0] as usize);
    std::os::file_close(fds_out[1] as usize);

    // --- GUI SETUP ---
    let mut root = Widget::frame(1)
        .width(Size::Relative(100))
        .height(Size::Relative(100))
        .background_color(Color::rgba(0, 0, 0, 164))
        .set_border_color(Color::rgb(0, 34, 255))
        .set_border_size(Size::Absolute(1))
        .set_border_radius(Size::Absolute(5));
    
    let term_display = Widget::label(2, "")
        .width(Size::Relative(92))
        .height(Size::Relative(90)) 
        .x(Size::Relative(4))
        .y(Size::Relative(5))
        .padding(Size::Absolute(10))
        .set_text_color(Color::rgb(255, 255, 255)) 
        .set_text_size(14)
        .background_color(Color::rgba(0, 0, 0, 0));

    root = root.add_child(term_display);
    win.children.push(root);
    win.show();
    
    let mut term_buffer = TerminalBuffer::new();
    let mut pipe_buf = [0u8; 4096];

    loop {
        // 1. Handle Window Events
        use inkui::Event;
        let mut events: [Event; 16] = [Event::None; 16];
        unsafe {
            std::os::syscall(52, win.id as u64, events.as_mut_ptr() as u64, 16);
        }
        
        for event in events.iter() {
            match event {
                Event::Keyboard(e) => {
                    if let Some(c) = core::char::from_u32(e.key) {
                        for _ in 0..e.repeat {
                            let mut buf = [0u8; 4];
                            let s = c.encode_utf8(&mut buf);
                            std::os::file_write(unsafe { TERM_WRITE_FD }, s.as_bytes());
                        }
                    }
                },
                Event::Mouse(e) => {
                    if e.scroll != 0 {
                        // Forward scroll to label (Widget ID 2)
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

        // 2. Handle Pipe Input
        let n = std::os::file_read(unsafe { TERM_READ_FD }, &mut pipe_buf);
        if n > 0 && n != usize::MAX {
            let mut i = 0;
            while i < n {
                let b = pipe_buf[i];
                if b == 0x08 { // Backspace
                    term_buffer.backspace();
                    i += 1;
                } else if b == b'\n' || b == b'\r' {
                    term_buffer.newline();
                    i += 1;
                } else if b == 0x1B { // Escape
                    if i + 1 < n && pipe_buf[i+1] == b'[' {
                        // Scan for end of sequence
                        let mut j = i + 2;
                        let mut end_found = false;
                        while j < n && j < i + 12 { // Limit max ESC len
                            let c = pipe_buf[j];
                            if c >= 0x40 && c <= 0x7E {
                                end_found = true;
                                break;
                            }
                            j += 1;
                        }

                        if end_found {
                            let cmd = pipe_buf[j];
                            let seq = &pipe_buf[i+2..j]; // Between [ and cmd
                            
                            match cmd {
                                b'J' => {
                                    // 2J
                                    if seq == b"2" {
                                        term_buffer.clear();
                                    }
                                },
                                b'H' => {
                                    // H or row;colH
                                    if seq.is_empty() {
                                        term_buffer.cursor_row = 0;
                                    } else {
                                        // Parse row;col
                                        let s = unsafe { core::str::from_utf8_unchecked(seq) };
                                        let parts: Vec<&str> = s.split(';').collect();
                                        if !parts.is_empty() {
                                            if let Ok(r) = parts[0].parse::<usize>() {
                                                term_buffer.cursor_row = r.saturating_sub(1);
                                            }
                                        }
                                        // Ignore col for now, as we only support line addressing in this simple buffer
                                    }
                                },
                                b'K' => {
                                    term_buffer.clear_line();
                                },
                                b'm' => {
                                    let seq_full = unsafe { core::str::from_utf8_unchecked(&pipe_buf[i..j+1]) };
                                    term_buffer.write_str(seq_full);
                                },
                                b'h' => {
                                    // Set Mode
                                    // ?25h Show Cursor
                                    // ?1049h Alt Screen
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
                                    // Reset Mode
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
                                    // Window Manipulation
                                    // 18t -> Report size in chars (rows;cols)
                                    if seq == b"18" {
                                        if let Some(widget) = win.find_widget_by_id(2) {
                                            if let inkui::widget::Widget::Label { text, geometry, .. } = widget {
                                                 let padding = 10; // Match widget setup
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
                    
                    // Improved Auto-scroll logic
                    // 1. Get dimensions
                    let padding = 10; // Must match widget definition
                    let width = geometry.width.saturating_sub(padding * 2);
                    let height = geometry.height.saturating_sub(padding * 2);
                    
                    if width > 0 {
                        // 2. Estimate visual lines (handling wrapping)
                        let char_width = (text.size as f32 * 0.6) as usize; // Approx width for monospace
                        if char_width > 0 {
                            let chars_per_line = width / char_width;
                            let mut visual_lines = 0;
                            
                            for line in &term_buffer.lines {
                                let len = line.chars().count(); // UTF-8 aware count
                                if len == 0 {
                                    visual_lines += 1;
                                } else {
                                    visual_lines += (len + chars_per_line - 1) / chars_per_line;
                                }
                            }
                            // Add extra "phantom" line if the last char is a newline (not stored in Vec<String> usually but check logic)
                            // Our logic: term_buffer stores lines. If prompt is "> ", it's the last line.
                            
                            let line_height = (text.size as f32 * 1.2) as usize;
                            let content_height = visual_lines * line_height;
                            
                            // 3. Scroll if needed (keep bottom visible + extra margin)
                            // We want content_bottom to be at visible_bottom - margin
                            let extra_margin = line_height; 
                            
                            if content_height + extra_margin > height {
                                geometry.scroll_offset_y = (content_height + extra_margin) - height;
                            } else {
                                geometry.scroll_offset_y = 0;
                            }
                        }
                    }
                }
            }
            win.draw();
            win.update();
        }

        std::os::yield_task();
    }
}
