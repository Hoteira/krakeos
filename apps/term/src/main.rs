#![no_std]
#![no_main]

mod types;
mod buffer;

extern crate alloc;
use alloc::format;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use alloc::boxed::Box;
use std::io::Read;
use std::println;

use inkui::{Color, Size, Widget, Window};
use std::fs::File;

use crate::types::{TermAction, Cell};
use crate::buffer::TerminalBuffer;

static mut TERM_READ_FD: usize = 0;
static mut TERM_WRITE_FD: usize = 0;

fn update_term_size(win: &Window) {
    if let Some(widget) = win.find_widget_by_id(2) {
        if let inkui::widget::Widget::Label { text, geometry, .. } = widget {
            let width = geometry.width.saturating_sub(geometry.width * 4 / 100);
            let height = geometry.height.saturating_sub(geometry.height * 4 / 100);

            let char_width = (text.size as f32 * 0.8) as usize;
            let line_height = (text.size as f32 * 1.5) as usize;

            if char_width > 0 && line_height > 0 {
                let cols = (width / char_width) as u16;
                let rows = ((height / line_height) as u16).saturating_sub(2);
                
                let ws = std::os::WinSize {
                    ws_row: rows,
                    ws_col: cols,
                    ws_xpixel: 0,
                    ws_ypixel: 0,
                };
                
                std::os::ioctl(0, std::os::TIOCSWINSZ, &ws as *const _ as u64);
            }
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn main() -> i32 {
    let width = 800;
    let height = 400;

    
    let font_size = 14.0f32;
    let char_w = (font_size * 0.7) as usize;
    let line_h = (font_size * 1.3) as usize;

    
    let avail_w = (width - width * 4 / 100) as f32;
    let avail_h = (height - height * 5 / 100) as f32;

    if char_w > 0 && line_h > 0 {
        let cols = (avail_w / char_w as f32) as u16;
        let rows = (avail_h / line_h as f32) as u16;
        let rows = rows.saturating_sub(2); 

        let ws = std::os::WinSize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        std::os::ioctl(0, std::os::TIOCSWINSZ, &ws as *const _ as u64);
    }

    let screen_w = std::graphics::get_screen_width();
    let screen_h = std::graphics::get_screen_height();
    let x = (screen_w / 2).saturating_sub(width / 2);
    let y = (screen_h / 2).saturating_sub(height / 2);

    let mut win = Window::new("krakeOS Term", width, height);
    win.x = x as isize;
    win.y = y as isize;

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

    std::os::spawn_with_fds("@0xE0/sys/bin/shell.elf", &[], &fds_map);


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
    update_term_size(&win);
    win.show();
    win.draw();
    win.update();

    let mut term_buffer = TerminalBuffer::new();
    let mut pipe_buf = [0u8; 4096];

    loop {
        let mut did_work = false;
        use inkui::Event;
        let mut events: [Event; 16] = [Event::None; 16];
        unsafe {
            std::os::syscall(104, win.id as u64, events.as_mut_ptr() as u64, 16);
        }

        if events[0] != Event::None {
            did_work = true;
        }

        let mut needs_redraw = false;

        for event in events.iter() {
            match event {
                Event::Keyboard(e) => {
                    if e.pressed {
                        if let Some(c) = core::char::from_u32(e.key) {
                            for _ in 0..e.repeat {
                                let mut buf = [0u8; 4];
                                let s = c.encode_utf8(&mut buf);
                                std::os::file_write(unsafe { TERM_WRITE_FD }, s.as_bytes());
                            }
                        } else {
                            
                            let seq = match e.key {
                                0x110003 => Some("\x1B[A"), 
                                0x110004 => Some("\x1B[B"), 
                                0x110002 => Some("\x1B[C"), 
                                0x110001 => Some("\x1B[D"), 
                                0x110007 => None, 
                                0x110005 => None, 
                                0x110006 => None, 
                                _ => None,
                            };
                            if let Some(s) = seq {
                                for _ in 0..e.repeat {
                                    std::os::file_write(unsafe { TERM_WRITE_FD }, s.as_bytes());
                                }
                            }
                        }
                    }
                }
                Event::Mouse(e) => {
                    if e.scroll != 0 && !term_buffer.is_alt { 
                        if let Some(widget) = win.find_widget_by_id_mut(2) {
                            widget.handle_scroll(e.scroll);
                            needs_redraw = true;
                        }
                    }
                }
                Event::Resize(e) => {
                    win.resize(e.width, e.height, true);
                    update_term_size(&win);
                    needs_redraw = true;
                }
                _ => {}
            }
        }

        let n = std::os::file_read(unsafe { TERM_READ_FD }, &mut pipe_buf);
        if n > 0 && n != usize::MAX {
            did_work = true;
            term_buffer.input_buffer.extend_from_slice(&pipe_buf[..n]);

            let mut consumed = 0;
            loop {
                let (action, bytes_to_consume) = {
                    let bytes = &term_buffer.input_buffer[consumed..];
                    if bytes.is_empty() {
                        (None, 0)
                    } else {
                        let b = bytes[0];
                        if b == 0x08 {
                            (Some(TermAction::Backspace), 1)
                        } else if b == b'\r' {
                            (Some(TermAction::CarriageReturn), 1)
                        } else if b == b'\n' {
                            (Some(TermAction::Newline), 1)
                        } else if b == 0x1B {
                            if bytes.len() < 2 {
                                (None, 0)
                            } else if bytes[1] == b'[' {
                                let mut j = 2;
                                let mut end_found = false;
                                while j < bytes.len() {
                                    let c = bytes[j];
                                    if c >= 0x40 && c <= 0x7E {
                                        end_found = true;
                                        break;
                                    }
                                    j += 1;
                                }
                                if end_found {
                                    let cmd = bytes[j];
                                    let seq = &bytes[2..j];
                                    let seq_str = unsafe { core::str::from_utf8_unchecked(seq) }.to_string();
                                    (Some(TermAction::Csi(cmd, seq_str)), j + 1)
                                } else if bytes.len() > 64 {
                                    (None, 1) 
                                } else {
                                    (None, 0) 
                                }
                            } else {
                                (None, 1) 
                            }
                        } else {
                            let mut len = 1;
                            if (b & 0xE0) == 0xC0 { len = 2; } else if (b & 0xF0) == 0xE0 { len = 3; } else if (b & 0xF8) == 0xF0 { len = 4; }
                            if bytes.len() >= len {
                                if let Ok(s) = core::str::from_utf8(&bytes[..len]) {
                                    (Some(TermAction::Text(s.to_string())), len)
                                } else {
                                    (None, 1)
                                }
                            } else {
                                (None, 0)
                            }
                        }
                    }
                };

                if bytes_to_consume == 0 { break; }

                match action {
                    Some(TermAction::Backspace) => term_buffer.backspace(),
                    Some(TermAction::CarriageReturn) => term_buffer.cursor_col = 0,
                    Some(TermAction::Newline) => term_buffer.newline(),
                    Some(TermAction::Csi(cmd, seq)) => {
                        match cmd {
                            b'A' => { 
                                let n = if seq.is_empty() { 1 } else { seq.parse::<usize>().unwrap_or(1) };
                                term_buffer.cursor_row = term_buffer.cursor_row.saturating_sub(n);
                            }
                            b'B' => { 
                                let n = if seq.is_empty() { 1 } else { seq.parse::<usize>().unwrap_or(1) };
                                term_buffer.cursor_row += n;
                            }
                            b'C' => { 
                                let n = if seq.is_empty() { 1 } else { seq.parse::<usize>().unwrap_or(1) };
                                term_buffer.cursor_col += n;
                            }
                            b'D' => { 
                                let n = if seq.is_empty() { 1 } else { seq.parse::<usize>().unwrap_or(1) };
                                term_buffer.cursor_col = term_buffer.cursor_col.saturating_sub(n);
                            }
                            b'G' => { 
                                let n = if seq.is_empty() { 1 } else { seq.parse::<usize>().unwrap_or(1) };
                                term_buffer.cursor_col = n.saturating_sub(1);
                            }
                            b'J' => {
                                if seq == "2" {
                                    term_buffer.clear();
                                }
                            }
                            b'H' => {
                                if seq.is_empty() {
                                    term_buffer.cursor_row = 0;
                                    term_buffer.cursor_col = 0;
                                } else {
                                    let parts: Vec<&str> = seq.split(';').collect();
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
                                        term_buffer.cursor_col = 0;
                                    }
                                }
                            }
                            b'd' => { 
                                let n = if seq.is_empty() { 1 } else { seq.parse::<usize>().unwrap_or(1) };
                                term_buffer.cursor_row = n.saturating_sub(1);
                            }
                            b'K' => {
                                if seq == "1" { 
                                    let current = if term_buffer.is_alt { &mut term_buffer.alt_lines } else { &mut term_buffer.lines };
                                    if term_buffer.cursor_row < current.len() {
                                        for i in 0..core::cmp::min(term_buffer.cursor_col + 1, current[term_buffer.cursor_row].len()) {
                                            current[term_buffer.cursor_row][i] = Cell::default();
                                        }
                                    }
                                } else if seq == "2" { 
                                    let current = if term_buffer.is_alt { &mut term_buffer.alt_lines } else { &mut term_buffer.lines };
                                    if term_buffer.cursor_row < current.len() {
                                        current[term_buffer.cursor_row].clear();
                                    }
                                } else { 
                                    term_buffer.clear_line();
                                }
                            }
                            b'm' => {
                                term_buffer.handle_sgr(&seq);
                            }
                            b'h' => {
                                if seq.starts_with('?') {
                                    let param = &seq[1..];
                                    if param == "25" {
                                        term_buffer.cursor_visible = true;
                                    } else if param == "1049" {
                                        term_buffer.switch_screen(true);
                                    }
                                }
                            }
                            b'l' => {
                                if seq.starts_with('?') {
                                    let param = &seq[1..];
                                    if param == "25" {
                                        term_buffer.cursor_visible = false;
                                    } else if param == "1049" {
                                        term_buffer.switch_screen(false);
                                    }
                                }
                            }
                            b't' => {
                                // Disabled to prevent blocking writes if shell is not reading
                            }
                            _ => {}
                        }
                    }
                    Some(TermAction::Text(s)) => {
                        term_buffer.write_str(&s);
                    }
                    None => {}
                }
                consumed += bytes_to_consume;
            }
            term_buffer.input_buffer.drain(..consumed);

            if let Some(widget) = win.find_widget_by_id_mut(2) {
                if let inkui::widget::Widget::Label { text, geometry, .. } = widget {
                    text.text = term_buffer.render();

                    if term_buffer.is_alt {
                        geometry.scroll_offset_y = 0;
                    } else {
                        let padding = 10;
                        let width = geometry.width.saturating_sub(padding * 2);
                        let height = geometry.height.saturating_sub(padding * 2);

                        if width > 0 {
                                                            let char_width = (text.size as f32 * 0.8) as usize;                            if char_width > 0 {
                                let chars_per_line = width / char_width;
                                let mut visual_lines = 0;

                                let current_lines = &term_buffer.lines;
                                for line in current_lines {
                                    let len = line.len();
                                    if len == 0 {
                                        visual_lines += 1;
                                    } else {
                                        visual_lines += (len + chars_per_line - 1) / chars_per_line;
                                    }
                                }

                                let line_height = (text.size as f32 * 1.2) as usize;
                                let content_height = visual_lines * line_height;

                                if content_height > height {
                                    geometry.scroll_offset_y = content_height.saturating_sub(height).saturating_add(20);
                                } else {
                                    geometry.scroll_offset_y = 0;
                                }
                            }
                        }
                    }
                }
            }
            needs_redraw = true;
        }

        if needs_redraw {
            win.draw();
            win.update();
        }

        if !did_work {
            std::os::yield_task();
        }
    }
}
