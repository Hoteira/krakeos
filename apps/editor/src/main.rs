use std::fs::File;
use std::io::{Read, Write, Seek, SeekFrom};
use std::string::String;

const FB_WIDTH: u32 = 640;
const FB_HEIGHT: u32 = 480;

fn draw_rect(fb: &mut [u32], x: u32, y: u32, w: u32, h: u32, color: u32) {
    for py in y..(y + h) {
        for px in x..(x + w) {
            if px < FB_WIDTH && py < FB_HEIGHT {
                fb[(py * FB_WIDTH + px) as usize] = color;
            }
        }
    }
}

fn draw_text(fb: &mut [u32], font: &mut titanf::TrueTypeFont, text: &str, start_x: u32, start_y: u32, scale: f32, fg_color: u32) {
    let mut pen_x = start_x as f32;
    let pen_y = start_y as f32;
    let mut prev_c = '\0';
    let kerning_scale = scale * (font.dpi / 72.0) / 2048.0; // Assuming 2048 units per EM
    
    for c in text.chars() {
        if prev_c != '\0' {
            if let Some(kern) = font.get_kerning(prev_c, c) {
                pen_x += kern as f32 * kerning_scale;
            }
        }
        
        let (metrics, bmp) = font.get_char::<true>(c, scale);
        let bmp_w = metrics.width;
        let bmp_h = metrics.height;
        for by in 0..bmp_h {
            for bx in 0..bmp_w {
                let alpha = bmp[(by * bmp_w + bx) as usize];
                if alpha > 0 {
                    let sx = (pen_x + metrics.left_side_bearing as f32 + bx as f32) as u32;
                    let sy = (pen_y + metrics.base_line as f32 + by as f32) as u32;
                    if sx < FB_WIDTH && sy < FB_HEIGHT {
                        let color = (alpha as u32) << 24 | (fg_color & 0xFFFFFF);
                        draw_rect(fb, sx, sy, 1, 1, color);
                    }
                }
            }
        }
        // ADVANCE!
        pen_x += metrics.advance_width as f32;
        prev_c = c;
    }
}

fn main() {
    let mut win_file = match File::options().write(true).open("/dev/gpu/window") {
        Ok(f) => f,
        Err(_) => return,
    };
    
    let mut kbd_file = match File::options().read(true).open("/dev/input/keyboard") {
        Ok(f) => f,
        Err(_) => return,
    };
    
    let mut font_file = match File::options().read(true).open("/fonts/CaskaydiaNerd.ttf") {
        Ok(f) => f,
        Err(_) => return,
    };
    
    let mut font_data = std::vec::Vec::new();
    font_file.read_to_end(&mut font_data).unwrap();
    let mut font = titanf::TrueTypeFont::load_font(&font_data).unwrap();
    
    let mut fb = vec![0xFF222222u32; (FB_WIDTH * FB_HEIGHT) as usize];
    let mut text_buf = String::new();
    let mut cursor_visible = true;
    let mut frame_count = 0;
    
    let mut event_buf = [0u8; 8];
    
    loop {
        while let Ok(bytes) = kbd_file.read(&mut event_buf) {
            if bytes == 8 {
                let type_ = u16::from_le_bytes([event_buf[0], event_buf[1]]);
                let code = u16::from_le_bytes([event_buf[2], event_buf[3]]);
                let value = u32::from_le_bytes([event_buf[4], event_buf[5], event_buf[6], event_buf[7]]);
                
                if type_ == 1 && value == 1 { // Key press
                    if code == 14 { // Backspace
                        text_buf.pop();
                    } else if code == 28 { // Enter
                        text_buf.push('\n');
                    } else if code == 1 { // Esc
                        // Save and exit
                        if let Ok(mut out) = File::options().write(true).open("/test.txt") {
                            let _ = out.write_all(text_buf.as_bytes());
                        }
                        return;
                    } else if code < 128 {
                        // Very naive scancode mapping for testing
                        let chars = b"\0\x1B1234567890-=\x08\tqwertyuiop[]\r asdfghjkl;'`\0\\zxcvbnm,./\0*\0 ";
                        if (code as usize) < chars.len() {
                            let c = chars[code as usize] as char;
                            if c != '\0' && c != '\x08' && c != '\r' {
                                text_buf.push(c);
                            }
                        }
                    }
                }
            } else {
                break;
            }
        }
        
        draw_rect(&mut fb, 0, 0, FB_WIDTH, FB_HEIGHT, 0xFF222222);
        
        let mut y = 20;
        for line in text_buf.split('\n') {
            draw_text(&mut fb, &mut font, line, 10, y, 16.0, 0xFFFFFFFF);
            y += 20;
        }
        
        if cursor_visible {
            let last_line = text_buf.split('\n').last().unwrap_or("");
            let cursor_x = 10 + (last_line.len() * 10) as u32; // rough estimate
            draw_rect(&mut fb, cursor_x, y - 20, 2, 16, 0xFFFFFFFF);
        }
        
        frame_count += 1;
        if frame_count > 30 {
            cursor_visible = !cursor_visible;
            frame_count = 0;
        }
        
        let fb_bytes: &[u8] = unsafe {
            std::slice::from_raw_parts(fb.as_ptr() as *const u8, fb.len() * 4)
        };
        let _ = win_file.seek(SeekFrom::Start(0));
        let _ = win_file.write_all(fb_bytes);
        
        // Sleep ~16ms (60 FPS)
        unsafe {
            let clock = __wasi_subscription_u_clock_t {
                identifier: 0,
                id: 0,
                timeout: 16_000_000,
                precision: 0,
                flags: 0,
            };
            let sub = __wasi_subscription_t {
                userdata: 0,
                u: __wasi_subscription_u_t { clock },
                tag: 0,
            };
            let mut event: __wasi_event_t = std::mem::zeroed();
            let mut nevents: usize = 0;
            poll_oneoff(&sub, &mut event, 1, &mut nevents);
        }
    }
}

#[repr(C)]
struct __wasi_subscription_u_clock_t {
    identifier: u64,
    id: u32,
    timeout: u64,
    precision: u64,
    flags: u16,
}
#[repr(C)]
struct __wasi_subscription_u_t {
    clock: __wasi_subscription_u_clock_t,
}
#[repr(C)]
struct __wasi_subscription_t {
    userdata: u64,
    u: __wasi_subscription_u_t,
    tag: u8,
}
#[repr(C)]
struct __wasi_event_t {
    userdata: u64,
    error: u16,
    type_: u8,
    fd_readwrite: [u64; 2],
}

#[link(wasm_import_module = "wasi_snapshot_preview1")]
unsafe extern "C" {
    fn poll_oneoff(
        in_: *const __wasi_subscription_t,
        out: *mut __wasi_event_t,
        nsubscriptions: usize,
        nevents: *mut usize,
    ) -> u16;
}
