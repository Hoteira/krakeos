use alloc::collections::VecDeque;
use alloc::vec::Vec;
use crate::display::{Color, DisplayServer, Mouse, State};
use core::sync::atomic::{AtomicU16, Ordering};
use std::os::print;
use crate::{debugln, println};

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[repr(C)]
pub struct MouseEvent {
    pub wid: u32,
    pub x: usize,
    pub y: usize,
    pub buttons: [bool; 3],
    pub scroll: i8,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[repr(C)]
pub struct KeyboardEvent {
    pub wid: u32,
    pub char: char,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[repr(C)]
pub struct ResizeEvent {
    pub wid: u32,
    pub width: usize,
    pub height: usize,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[repr(C)]
pub struct RedrawEvent {
    pub wid: u32,
    pub to_fb: bool,
    pub to_db: bool,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[repr(C)]
pub enum Event {
    Mouse(MouseEvent),
    Keyboard(KeyboardEvent),
    Resize(ResizeEvent),
    Redraw(RedrawEvent),
    None
}

impl Event {
    pub fn get_window_id(&self) -> u32 {
        match self {
            Event::Mouse(event) => event.wid,
            Event::Keyboard(event) => event.wid,
            Event::Resize(event) => event.wid,
            Event::Redraw(event) => event.wid,

            Event::None => 0,
        }
    }
}

pub static mut DISPLAY_SERVER: DisplayServer = DisplayServer {
    width: 0,
    height: 0,
    pitch: 0,
    depth: 8,

    framebuffer: 0,
    double_buffer: 0,
};

pub static mut MOUSE: Mouse = Mouse {
    x: 0,
    y: 0,

    left: false,
    center: false,
    right: false,

    state: State::Point,
};

pub static mut GLOBAL_EVENT_QUEUE: EventQueue = EventQueue { queue: Vec::new() };

pub struct EventQueue {
    pub queue: Vec<Event>,
}

impl EventQueue {
    pub fn get_and_remove_events(&mut self, window_id: u32, max_events: usize) -> Vec<Event> {
        let mut result = Vec::with_capacity(max_events.min(self.queue.len()));
        let mut write_idx = 0;
        let mut read_idx = 0;

        while read_idx < self.queue.len() && result.len() < max_events {
            if self.queue[read_idx].get_window_id() == window_id {
                result.push(self.queue[read_idx]);
                read_idx += 1;
            } else {
                if write_idx != read_idx {
                    self.queue[write_idx] = self.queue[read_idx];
                }
                write_idx += 1;
                read_idx += 1;
            }
        }

        while read_idx < self.queue.len() {
            if write_idx != read_idx {
                self.queue[write_idx] = self.queue[read_idx];
            }
            write_idx += 1;
            read_idx += 1;
        }

        self.queue.truncate(write_idx);

        result
    }

    pub fn add_event(&mut self, event: Event) {
        if self.queue.len() >= 1000 {
            self.reset_queue();
        }
        self.queue.push(event);
    }

    pub fn reset_queue(&mut self) {
        self.queue.clear();
    }
}

impl DisplayServer {
    pub fn init(&mut self) {
        let boot_info = unsafe { crate::boot::BOOT_INFO };
        let vbe = boot_info.mode;
        self.width = vbe.width as u64;
        self.pitch = vbe.pitch as u64;
        self.height = vbe.height as u64;
        self.depth = vbe.bpp as usize;

        unsafe {
            crate::display::DEPTH = vbe.bpp;
        }

        // Remap Framebuffer to Write-Combining (WC)
        // We use a high virtual address to avoid collisions
        let fb_phys = vbe.framebuffer as u64;
        let fb_size = (self.pitch * self.height) as usize;
        let fb_pages = (fb_size + 4095) / 4096;
        let fb_virt_base = 0xFFFF_8000_FD00_0000; // Arbitrary high address

        for i in 0..fb_pages {
            let offset = (i * 4096) as u64;
            let phys = fb_phys + offset;
            let virt = fb_virt_base + offset;
            
            // Use PAGE_PAT bit to select Index 4 (WC)
            // Note: PAGE_HUGE is also bit 7, but map_page handles 4KB pages where Bit 7 is PAT.
            // Ensure map_page doesn't set PAGE_HUGE.
            let flags = crate::memory::paging::PAGE_PRESENT | 
                        crate::memory::paging::PAGE_WRITABLE | 
                        crate::memory::paging::PAGE_PAT; // WC
            
            crate::memory::vmm::map_page(virt, phys, flags, None);
        }

        self.framebuffer = fb_virt_base;

        let size_bytes = self.pitch as usize * self.height as usize;
        let pages = (size_bytes + 4095) / 4096;

        unsafe {
            if let Some(buffer) = crate::memory::pmm::allocate_frames(pages, 0) {
                self.double_buffer = buffer;
            } else {
                panic!("[DisplayServer] Failed to allocate double buffer!");
            }
        }
    }

    pub fn copy(&self) {
        let buffer_size = self.pitch as u64 * self.height as u64;
        unsafe {
            core::ptr::copy(
                self.double_buffer as *const u8,
                self.framebuffer as *mut u8,
                buffer_size as usize,
            );
        }
    }

    pub fn copy_to_fb(&self, x: u32, y: u32, width: u32, height: u32) {
        let bytes_per_pixel = match self.depth {
            32 => 4,
            24 => 3,
            _  => return,
        };

        let max_x = (self.width as u32).min(x + width);
        let max_y = (self.height as u32).min(y + height);

        if x >= self.width as u32 || y >= self.height as u32 {
            return;
        }

        let copy_width = (max_x - x) as usize;
        let copy_height = (max_y - y) as usize;

        let src = self.double_buffer as *const u8;
        let dst = self.framebuffer   as *mut u8;
        let pitch = self.pitch as u32;

        unsafe {
            for row in 0..copy_height {
                let line_start = (y + row as u32) * pitch;
                let offset = line_start + x * bytes_per_pixel;

                core::ptr::copy(
                    src.add(offset as usize),
                    dst.add(offset as usize),
                    copy_width * bytes_per_pixel as usize,
                );
            }
        }
    }

    pub fn copy_to_db(&self, width: u32, height: u32, buffer: usize, x: u32, y: u32) {
        let dst_bpp = self.depth;
        if dst_bpp != 32 && dst_bpp != 24 {
            return;
        }

        // Source is always 32-bit RGBA/BGRA
        const SRC_BYTES_PER_PIXEL: usize = 4;
        let dst_bytes_per_pixel = if dst_bpp == 32 { 4 } else { 3 };

        let dst_pitch = self.pitch as usize;
        let src_pitch = (width as usize) * SRC_BYTES_PER_PIXEL;

        let max_x = (self.width as u32).min(x + width);
        let max_y = (self.height as u32).min(y + height);

        if x >= self.width as u32 || y >= self.height as u32 {
            return;
        }

        let copy_width = (max_x - x) as usize;
        let copy_height = (max_y - y) as usize;

        unsafe {
            let src_ptr = buffer as *const u8;
            let dst_ptr = self.double_buffer as *mut u8;

            if dst_bpp == 32 {
                for row in 0..copy_height {
                    for col in 0..copy_width {
                        let src_offset = row * src_pitch + col * SRC_BYTES_PER_PIXEL;
                        let dst_offset = (y as usize + row) * dst_pitch + (x as usize + col) * 4;

                        let src_a = *src_ptr.add(src_offset + 3);

                        if src_a == 0 {
                            continue; // Transparent
                        } else if src_a == 255 {
                            // Opaque
                            core::ptr::copy(
                                src_ptr.add(src_offset),
                                dst_ptr.add(dst_offset),
                                4,
                            );
                        } else {
                            // Blend
                            let alpha = src_a as u32;
                            let inv_alpha = 255 - alpha;

                            let src_r = *src_ptr.add(src_offset) as u32;
                            let src_g = *src_ptr.add(src_offset + 1) as u32;
                            let src_b = *src_ptr.add(src_offset + 2) as u32;

                            let dst_r = *dst_ptr.add(dst_offset) as u32;
                            let dst_g = *dst_ptr.add(dst_offset + 1) as u32;
                            let dst_b = *dst_ptr.add(dst_offset + 2) as u32;

                            *dst_ptr.add(dst_offset) = ((src_r * alpha + dst_r * inv_alpha) / 255) as u8;
                            *dst_ptr.add(dst_offset + 1) = ((src_g * alpha + dst_g * inv_alpha) / 255) as u8;
                            *dst_ptr.add(dst_offset + 2) = ((src_b * alpha + dst_b * inv_alpha) / 255) as u8;
                            *dst_ptr.add(dst_offset + 3) = 255;
                        }
                    }
                }
            } else {
                for row in 0..copy_height {
                    for col in 0..copy_width {
                        let src_offset = row * src_pitch + col * SRC_BYTES_PER_PIXEL;
                        let dst_offset = (y as usize + row) * dst_pitch + (x as usize + col) * 3;

                        let src_a = *src_ptr.add(src_offset + 3);

                        if src_a == 0 {
                            continue;
                        } else if src_a == 255 {
                            *dst_ptr.add(dst_offset) = *src_ptr.add(src_offset);
                            *dst_ptr.add(dst_offset + 1) = *src_ptr.add(src_offset + 1);
                            *dst_ptr.add(dst_offset + 2) = *src_ptr.add(src_offset + 2);
                        } else {
                            // Blend
                            let alpha = src_a as u32;
                            let inv_alpha = 255 - alpha;

                            let src_r = *src_ptr.add(src_offset) as u32;
                            let src_g = *src_ptr.add(src_offset + 1) as u32;
                            let src_b = *src_ptr.add(src_offset + 2) as u32;

                            let dst_r = *dst_ptr.add(dst_offset) as u32;
                            let dst_g = *dst_ptr.add(dst_offset + 1) as u32;
                            let dst_b = *dst_ptr.add(dst_offset + 2) as u32;

                            *dst_ptr.add(dst_offset) = ((src_r * alpha + dst_r * inv_alpha) / 255) as u8;
                            *dst_ptr.add(dst_offset + 1) = ((src_g * alpha + dst_g * inv_alpha) / 255) as u8;
                            *dst_ptr.add(dst_offset + 2) = ((src_b * alpha + dst_b * inv_alpha) / 255) as u8;
                        }
                    }
                }
            }
        }
    }


    pub fn copy_to_fb_a(&self, width: u32, height: u32, buffer: usize, x: u32, y: u32) {
        let dst_bpp = self.depth;
        if dst_bpp != 32 && dst_bpp != 24 {
            return;
        }

        const SRC_BYTES_PER_PIXEL: usize = 4;
        let dst_bytes_per_pixel = if dst_bpp == 32 { 4 } else { 3 };

        let dst_pitch = self.pitch as usize;
        let src_pitch = (width as usize) * SRC_BYTES_PER_PIXEL;

        let max_x = (self.width as u32).min(x + width);
        let max_y = (self.height as u32).min(y + height);

        if x >= self.width as u32 || y >= self.height as u32 {
            return;
        }

        let copy_width = (max_x - x) as usize;
        let copy_height = (max_y - y) as usize;

        unsafe {
            let src_ptr = buffer as *const u8;
            let dst_ptr = self.framebuffer as *mut u8;

            for row in 0..copy_height {
                for col in 0..copy_width {
                    let src_offset = row * src_pitch + col * SRC_BYTES_PER_PIXEL;
                    let dst_offset = (y as usize + row) * dst_pitch + (x as usize + col) * dst_bytes_per_pixel;

                    let src_r = *src_ptr.add(src_offset);
                    let src_g = *src_ptr.add(src_offset + 1);
                    let src_b = *src_ptr.add(src_offset + 2);
                    let src_a = *src_ptr.add(src_offset + 3);

                    if dst_bpp == 32 {
                        // 32-bit destination with alpha blending
                        if src_a == 255 {
                            // Fully opaque: direct copy (optimization)
                            *dst_ptr.add(dst_offset) = src_r;
                            *dst_ptr.add(dst_offset + 1) = src_g;
                            *dst_ptr.add(dst_offset + 2) = src_b;
                            *dst_ptr.add(dst_offset + 3) = src_a;
                        } else if src_a == 0 {
                            // Fully transparent: skip (optimization)
                            continue;
                        } else {
                            // Alpha blending required
                            let alpha = src_a as u32;
                            let inv_alpha = 255 - alpha;

                            // Read destination pixel
                            let dst_r = *dst_ptr.add(dst_offset) as u32;
                            let dst_g = *dst_ptr.add(dst_offset + 1) as u32;
                            let dst_b = *dst_ptr.add(dst_offset + 2) as u32;

                            // Blend and write back
                            *dst_ptr.add(dst_offset) = ((src_r as u32 * alpha + dst_r * inv_alpha) / 255) as u8;
                            *dst_ptr.add(dst_offset + 1) = ((src_g as u32 * alpha + dst_g * inv_alpha) / 255) as u8;
                            *dst_ptr.add(dst_offset + 2) = ((src_b as u32 * alpha + dst_b * inv_alpha) / 255) as u8;
                            // Keep destination alpha or set to opaque
                            *dst_ptr.add(dst_offset + 3) = 255;
                        }
                    } else {
                        // 24-bit destination: convert RGBA to RGB
                        if src_a == 0 {
                            // Fully transparent: skip
                            continue;
                        } else if src_a == 255 {
                            // Fully opaque: direct copy RGB channels
                            *dst_ptr.add(dst_offset) = src_r;
                            *dst_ptr.add(dst_offset + 1) = src_g;
                            *dst_ptr.add(dst_offset + 2) = src_b;
                        } else {
                            // Alpha blending with 24-bit destination
                            let alpha = src_a as u32;
                            let inv_alpha = 255 - alpha;

                            // Read destination RGB
                            let dst_r = *dst_ptr.add(dst_offset) as u32;
                            let dst_g = *dst_ptr.add(dst_offset + 1) as u32;
                            let dst_b = *dst_ptr.add(dst_offset + 2) as u32;

                            // Blend and write back RGB only
                            *dst_ptr.add(dst_offset) = ((src_r as u32 * alpha + dst_r * inv_alpha) / 255) as u8;
                            *dst_ptr.add(dst_offset + 1) = ((src_g as u32 * alpha + dst_g * inv_alpha) / 255) as u8;
                            *dst_ptr.add(dst_offset + 2) = ((src_b as u32 * alpha + dst_b * inv_alpha) / 255) as u8;
                        }
                    }
                }
            }
        }
    }

    pub fn write_pixel(&self, row: u32, col: u32, color: Color) {
        if col < self.width as u32 && row < self.height as u32 {
            unsafe {
                let offset = (row as u64 * self.pitch + col as u64 * (self.depth as u64 / 8)) as usize;
                match self.depth {
                    16 => {
                        *((self.framebuffer as *mut u8).add(offset) as *mut u16) = color.to_u16();
                    },

                    24 => {
                        let color = color.to_u24();
                        *((self.framebuffer as *mut u8).add(offset)) = color[0];
                        *((self.framebuffer as *mut u8).add(offset + 1)) = color[1];
                        *((self.framebuffer as *mut u8).add(offset + 2)) = color[2];
                    }

                    32 => {
                        *((self.framebuffer as *mut u8).add(offset) as *mut u32) = color.to_u32();
                    }

                    _ => {}
                }
            }
        }
    }

    pub fn draw_mouse(&self, x: u16, y: u16) {
        use crate::drivers::periferics::mouse::{CURSOR_BUFFER, CURSOR_WIDTH, CURSOR_HEIGHT};

        // Fallback for non-32bpp or weird pitch
        if self.depth != 32 {
            unsafe {
                for i in 0..CURSOR_HEIGHT {
                    for j in 0..CURSOR_WIDTH {
                        let color = CURSOR_BUFFER[i * CURSOR_WIDTH + j];
                        if color != 0 {
                            self.write_pixel(y.wrapping_add(i as u16) as u32, x.wrapping_add(j as u16) as u32, Color::from_u32(color));
                        }
                    }
                }
            }
            return;
        }

        // Optimized 32bpp path
        let mut temp_buf: [u32; 1024] = [0; 1024];
        let pitch_bytes = self.pitch as usize;
        // let db_ptr = self.double_buffer as *const u8; // Unused, we read from FB
        let fb_ptr = self.framebuffer as *mut u8;
        let screen_w = self.width as usize;
        let screen_h = self.height as usize;
        let mx = x as usize;
        let my = y as usize;

        unsafe {
            for row in 0..CURSOR_HEIGHT {
                let screen_y = my + row;
                if screen_y >= screen_h { break; }

                let row_byte_offset = screen_y * pitch_bytes;

                for col in 0..CURSOR_WIDTH {
                    let screen_x = mx + col;
                    if screen_x >= screen_w { break; }

                    let pixel_offset = row_byte_offset + screen_x * 4;

                    // Read background from Framebuffer (to capture dragged windows not in DB)
                    let bg_color = *(fb_ptr.add(pixel_offset) as *const u32);

                    let cursor_color = CURSOR_BUFFER[row * CURSOR_WIDTH + col];

                    if cursor_color != 0 {
                        temp_buf[row * CURSOR_WIDTH + col] = cursor_color;
                    } else {
                        temp_buf[row * CURSOR_WIDTH + col] = bg_color;
                    }
                }
            }

            for row in 0..CURSOR_HEIGHT {
                let screen_y = my + row;
                if screen_y >= screen_h { break; }

                let fb_offset = screen_y * pitch_bytes + mx * 4;

                let copy_w = if mx + CURSOR_WIDTH > screen_w {
                    screen_w - mx
                } else {
                    CURSOR_WIDTH
                };

                core::ptr::copy_nonoverlapping(
                    temp_buf.as_ptr().add(row * CURSOR_WIDTH) as *const u8,
                    fb_ptr.add(fb_offset),
                    copy_w * 4
                );
            }
        }
    }
}

pub static mut LAST_INPUT: u8 = 0;
pub static mut DRAGS: u8 = 0;
pub static mut DRAG: bool = false;
pub static mut DRAGGING_WINDOW: AtomicU16 = AtomicU16::new(0);
pub static mut RESIZING_WINDOW: AtomicU16 = AtomicU16::new(0);
pub static mut W_WIDTH: usize = 0;
pub static mut W_HEIGHT: usize = 0;

impl Mouse {
        pub fn cursor(&mut self, data: [u8; 4]) {
            unsafe { (*(&raw mut DISPLAY_SERVER)).copy_to_fb(self.x as u32, self.y as u32, 32, 32) };
    
            let mut x_rel = data[1] as i16;
            let mut y_rel = data[2] as i16;

            if (data[0] & 0x10) != 0 {
                x_rel |= 0xFF00u16 as i16; 
            }

            if (data[0] & 0x20) != 0 {
                y_rel |= 0xFF00u16 as i16;
            }
    
            self.x = self.clamp_mx(x_rel);
            self.y = self.clamp_my(-y_rel);
    
            let prev_left = self.left;
            self.left = (data[0] & 0b00000001) != 0;
            self.right = (data[0] & 0b00000010) != 0;
            self.center = (data[0] & 0b00000100) != 0;
    
            unsafe {
                LAST_INPUT = data[0];
            }
    
            let scroll_val = data[3] as i8;
            let x_vec = x_rel;
            let y_vec = y_rel;
    
            unsafe {
                if self.left {

                    DRAGS = DRAGS.wrapping_add(1);
                    if DRAGS > 2 {
                        DRAG = true;
                    }
                } else {
                    DRAGS = 0;
                    DRAG = false;
    
                    if (*(&raw mut RESIZING_WINDOW)).load(Ordering::Relaxed) != 0 {
                        let w = (*(&raw mut COMPOSER))
                            .find_window_id((*(&raw mut RESIZING_WINDOW)).load(Ordering::Relaxed) as usize)
                            .unwrap();
    
                        (*(&raw mut DRAGGING_WINDOW)).store(0, Ordering::Relaxed);
                        (*(&raw mut RESIZING_WINDOW)).store(0, Ordering::Relaxed);
    
                        if w.event_handler != 0 {
                            (*(&raw mut GLOBAL_EVENT_QUEUE)).add_event(Event::Resize(ResizeEvent {
                                wid: w.id as u32,
                                width: W_WIDTH,
                                height: W_HEIGHT,
                            }));
                        }
                        W_WIDTH = 0;
                        W_HEIGHT = 0;
    
                    } else if (*(&raw mut DRAGGING_WINDOW)).load(Ordering::Relaxed) != 0 {
                         let wid = (*(&raw mut DRAGGING_WINDOW)).load(Ordering::Relaxed) as usize;
                         (*(&raw mut COMPOSER)).copy_window(wid);
                         (*(&raw mut COMPOSER)).copy_window_fb(wid);
    
                         (*(&raw mut DRAGGING_WINDOW)).store(0, Ordering::Relaxed);
                         (*(&raw mut RESIZING_WINDOW)).store(0, Ordering::Relaxed);
                         W_WIDTH = 0;
                         W_HEIGHT = 0;
                    }
                }
            }

            let w = unsafe { (*(&raw mut COMPOSER)).find_window(self.x as usize, self.y as usize) };
    
            if unsafe { (*(&raw mut RESIZING_WINDOW)).load(Ordering::Relaxed) != 0 } {
                 let dx = x_vec;
                 let dy = y_vec * -1; // Invert Y
                
                 let w = unsafe {
                    (*(&raw mut COMPOSER))
                        .find_window_id((*(&raw mut RESIZING_WINDOW)).load(Ordering::Relaxed) as usize)
                        .unwrap()
                };
                
                let final_width = self.rem_sign(unsafe { W_WIDTH } as i16 + dx) as usize;
                let final_height = self.rem_sign(unsafe { W_HEIGHT } as i16 + dy) as usize;
                
                unsafe {
                    W_WIDTH = cap(final_width, ((*(&raw mut DISPLAY_SERVER)).width as usize).saturating_sub(w.x));
                    W_HEIGHT = cap(final_height, ((*(&raw mut DISPLAY_SERVER)).height as usize).saturating_sub(w.y));
                }

                self.draw_square_outline(w.y as u16, w.x as u16, unsafe { W_HEIGHT as u16 }, unsafe { W_WIDTH as u16 }, Color::rgb(245, 245, 247));
    
                // Draw mouse before returning (Resize path)
                unsafe { (*(&raw mut DISPLAY_SERVER)).draw_mouse(self.x, self.y) };
                return;
    
            } else if unsafe { (*(&raw mut DRAGGING_WINDOW)).load(Ordering::Relaxed) != 0 } {
                let composer = unsafe { &mut *(&raw mut COMPOSER) };
                let display_server = unsafe { &mut *(&raw mut DISPLAY_SERVER) };
                let wid = unsafe { (*(&raw mut DRAGGING_WINDOW)).load(Ordering::Relaxed) as usize };
    
                let (id, width, height, old_x, old_y) = {
                    let window_opt = composer.find_window_id(wid);
                    let w = match window_opt {
                        Some(w) => w,
                        None => return,
                    };
    
                    let old_x = w.x;
                    let old_y = w.y;
                    let width = w.width;
                    let height = w.height;
    
                    let mut new_x = w.x;
                    let mut new_y = w.y;
    
                    if x_vec < 0 {
                        new_x = new_x.saturating_sub((-x_vec) as usize);
                    } else {
                        new_x = new_x.saturating_add(x_vec as usize);
                    }
    
                    if -y_vec < 0 {
                        new_y = new_y.saturating_sub((-(-y_vec)) as usize);
                    } else {
                        new_y = new_y.saturating_add((-y_vec) as usize);
                    }
    
                    let mut final_x = new_x;
                    let mut final_y = new_y;
                    let screen_width = display_server.width as usize;
                    let screen_height = display_server.height as usize;
    
                    // Relaxed clamping: Keep 10px visible
                    if final_x > screen_width.saturating_sub(10) { final_x = screen_width.saturating_sub(10); }
                    if final_y > screen_height.saturating_sub(10) { final_y = screen_height.saturating_sub(10); }
    
                    w.x = final_x;
                    w.y = final_y;
    
                    (w.id, width, height, old_x, old_y)
                };

                display_server.copy_to_fb(
                    old_x as u32,
                    old_y as u32,
                    width as u32,
                    height as u32,
                );

                composer.copy_window_fb(id);

                display_server.draw_mouse(self.x, self.y);
                return;
            }
    
            if let Some(ws) = w {
                if ws.w_type == Items::Window && ws.z != 0 && self.left && !prev_left {
                     // Click to front
                     let x = ws.x;
                     let y = ws.y;
                     let width = ws.width;
                     let height = ws.height;
                     let id = ws.id;
    
                     unsafe {
                        for i in (*(&raw mut COMPOSER)).windows.iter_mut() {
                            if i.id != id {
                                i.z = i.z.wrapping_add(1);
                            } else {
                                i.z = 0;
                            }
                        }
                        (*(&raw mut COMPOSER)).windows.sort_by_key(|w| w.z);
                        (*(&raw mut COMPOSER)).copy_window(id);
                        
                        (*(&raw mut DISPLAY_SERVER)).copy_to_fb(
                            x as u32,
                            y as u32,
                            width as u32,
                            height as u32,
                        );
                     }
                }

                let mut handled_drag = false;
                if self.left {
                    if ws.can_move && self.y as usize >= ws.y && self.y as usize <= ws.y + 25 {
                        if unsafe { DRAG == true } {
                            if unsafe { (*(&raw mut DRAGGING_WINDOW)).load(Ordering::Relaxed) == 0 } {

                                unsafe {
                                    (*(&raw mut DRAGGING_WINDOW)).store(ws.id as u16, Ordering::Relaxed);
                                    (*(&raw mut COMPOSER)).recompose_except(ws.id);
                                }
                            }
                            handled_drag = true;
                        }
                    } else if ws.can_move && self.is_bottom_right(ws.x as u16, ws.y as u16, ws.width as u16, ws.height as u16, self.x, self.y) {
                         if unsafe { DRAG == true } {
                             if unsafe { (*(&raw mut RESIZING_WINDOW)).load(Ordering::Relaxed) == 0 } {
                                 unsafe {
                                     W_WIDTH = ws.width;
                                     W_HEIGHT = ws.height;
                                     (*(&raw mut RESIZING_WINDOW)).store(ws.id as u16, Ordering::Relaxed);
                                 }
                             }
                             handled_drag = true;
                         }
                    }
                }
    
                if handled_drag {
                    unsafe { (*(&raw mut DISPLAY_SERVER)).draw_mouse(self.x, self.y) };
                    return;
                }
    
                // Normal Click/Scroll/Move Event
                if ws.event_handler != 0 && unsafe { DRAG == false } {
                     // Only send event if something changed (click or scroll) to avoid flooding? 
                     // Or send every move? User asked for "mouse's ... click + scroll".
                     // If we send every move, it's fine for now.
                     let xc = ws.x;
                     let yc = ws.y;
                     let id = ws.id;
                     unsafe {
                        (*(&raw mut GLOBAL_EVENT_QUEUE)).add_event(Event::Mouse(MouseEvent {
                            wid: id as u32,
                            x: self.x as usize - xc,
                            y: self.y as usize - yc,
                            buttons: [self.left, self.center, self.right],
                            scroll: scroll_val,
                        }));
                     }
                }
            }
            
            // Final draw_mouse at the very end to guarantee it's on top.
            unsafe { (*(&raw mut DISPLAY_SERVER)).draw_mouse(self.x, self.y) };
        }
    fn rem_sign(&self, n: i16) -> u16 {
        if n < 0 { (n * -1) as u16 } else { n as u16 }
    }

    pub fn union_rect(
        &self,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        x2: u32,
        y2: u32,
    ) -> (u32, u32, u32, u32) {
        let min_x = x.min(x2);
        let max_x = (x + width).max(x2 + width);
        let min_y = y.min(y2);
        let max_y = (y + height).max(y2 + height);

        let width = max_x - min_x;
        let height = max_y - min_y;

        (min_x, min_y, width, height)
    }

    fn is_bottom_right(
        &self,
        w_x: u16,
        w_y: u16,
        w_width: u16,
        w_height: u16,
        mouse_x: u16,
        mouse_y: u16,
    ) -> bool {
        let x_min = w_x.wrapping_add(w_width.wrapping_sub(8));
        let x_max = w_x.wrapping_add(w_width.wrapping_sub(0));
        let y_min = w_y.wrapping_add(w_height.wrapping_sub(8));
        let y_max = w_y.wrapping_add(w_height.wrapping_sub(0));

        (mouse_x >= x_min && mouse_x <= x_max) && (mouse_y >= y_min && mouse_y <= y_max)
    }

    pub fn draw_square_outline(&self, x: u16, y: u16, width: u16, height: u16, color: Color) {
        let max_x = x + width - 1;
        let max_y = y + height - 1;
        unsafe {
            for i in x..=max_x {
                (*(&raw mut DISPLAY_SERVER)).write_pixel(i as u32, y as u32, color);
                (*(&raw mut DISPLAY_SERVER)).write_pixel(i as u32, max_y as u32, color);
            }
            for i in y..=max_y {
                (*(&raw mut DISPLAY_SERVER)).write_pixel(x as u32, i as u32, color);
                (*(&raw mut DISPLAY_SERVER)).write_pixel(max_x as u32, i as u32, color);
            }
        }
    }

    fn clamp_mx(&self, n: i16) -> u16 {
        let mx_0 = self.x as i16;
        let sx = unsafe { (*(&raw mut DISPLAY_SERVER)).width } as u16;

        if n + mx_0 >= (sx as i16 - 2) {
            sx.wrapping_sub(2)
        } else if n + mx_0 <= 0 {
            0
        } else {
            (n + mx_0) as u16
        }
    }

    fn clamp_my(&self, n: i16) -> u16 {
        let my_0 = self.y as i16;
        let sy = unsafe { (*(&raw mut DISPLAY_SERVER)).height } as u16;

        if n + my_0 >= (sy as i16 - 2) {
            sy.wrapping_sub(2)
        } else if n + my_0 <= 0 {
            return 0;
        } else {
            (n + my_0) as u16
        }
    }
}

fn cap(n: usize, value: usize) -> usize {
    if n > value { value } else { n }
}

pub fn add_delta(n: u16, m: i16) -> u16 {
    if (n as i16 + m) < 0 {
        0
    } else {
        (n as i16 + m) as u16
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[repr(C)]
pub enum Items {
    Wallpaper,
    Bar,
    Popup,
    Window,
    Null,
}

#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub struct Window {
    pub id: usize,
    pub buffer: usize,

    pub x: usize,
    pub y: usize,
    pub z: usize,
    pub width: usize,
    pub height: usize,

    pub can_move: bool,
    pub can_resize: bool,
    pub min_width: usize,
    pub min_height: usize,

    pub event_handler: usize,
    pub w_type: Items,
}

pub static NULL_WINDOW: Window = Window {
    id: 0,
    buffer: 0,

    x: 0,
    y: 0,
    z: 0,
    width: 0,
    height: 0,

    can_move: false,
    can_resize: false,
    min_width: 0,
    min_height: 0,

    event_handler: 0,
    w_type: Items::Null,
};

#[derive(Debug, Clone)]
pub struct Composer {
    pub windows: [Window; 16],
}

pub static mut COMPOSER: Composer = Composer {
    windows: [NULL_WINDOW; 16],
};

impl Composer {
    pub fn copy_window(&mut self, id: usize) {
        for i in 0..self.windows.len() {
            if id == self.windows[i].id {
                match self.windows[i].w_type {
                    Items::Null => {}
                    _ => unsafe {
                        (*(&raw mut crate::composer::DISPLAY_SERVER)).copy_to_db(
                            self.windows[i].width as u32,
                            self.windows[i].height as u32,
                            self.windows[i].buffer,
                            self.windows[i].x as u32,
                            self.windows[i].y as u32,
                        )
                    },
                }
            }
        }
    }

    pub fn copy_window_fb(&mut self, id: usize) {
        for i in 0..self.windows.len() {
            if id == self.windows[i].id {
                match self.windows[i].w_type {
                    Items::Null => {}
                    _ => unsafe {
                        (*(&raw mut DISPLAY_SERVER)).copy_to_fb_a(
                            self.windows[i].width as u32,
                            self.windows[i].height as u32,
                            self.windows[i].buffer,
                            self.windows[i].x as u32,
                            self.windows[i].y as u32,
                        )
                    },
                }
            }
        }
    }

    pub fn find_window(&mut self, x: usize, y: usize) -> Option<&mut Window> {
        for i in 0..self.windows.len() {
            if x >= self.windows[i].x
                && x <= (self.windows[i].x + self.windows[i].width)
                && y >= self.windows[i].y
                && y <= (self.windows[i].y + self.windows[i].height)
            {
                match self.windows[i].w_type {
                    Items::Null => {}
                    _ => return Some(&mut self.windows[i]),
                }
            }
        }

        None
    }

    pub fn find_window_id(&mut self, id: usize) -> Option<&mut Window> {
        for i in 0..self.windows.len() {
            if self.windows[i].id == id {
                let h = self.windows[i].w_type;
                if h != Items::Null {
                    return Some(&mut self.windows[i]);
                }
            }
        }

        None
    }

    pub fn check_id(&self, _rng_seed: u64) -> usize {
        // Simple incrementing ID for now to avoid pulling in RNG dependency if not needed
        static mut NEXT_ID: usize = 1;
        unsafe {
            let id = NEXT_ID;
            NEXT_ID += 1;
            id
        }
    }

    pub fn add_window(&mut self, mut w: Window) -> usize {
        let wtype = w.w_type;
        if wtype == Items::Wallpaper {
            w.z = 255;
        } else if wtype == Items::Bar {
            w.z = 0;
        } else if wtype == Items::Popup {
            w.z = 0;
        }

        w.id = self.check_id(w.buffer as u64);

        for i in 0..self.windows.len() {
            match self.windows[i].w_type {
                Items::Null => {
                    self.windows[i] = w;
                    break;
                }
                _ => {}
            }
        }

        for i in 0..self.windows.len() {
            if self.windows[i].id != w.id {
                self.windows[i].z = self.windows[i].z.saturating_add(1);
            }
        }

        self.windows.sort_by_key(|w| w.z);

        w.id
    }

    pub fn resize_window(&mut self, w: Window) {;
        for i in 0..self.windows.len() {
            if w.id == self.windows[i].id {
                self.windows[i].width = w.width;
                self.windows[i].height = w.height;
                self.windows[i].buffer = w.buffer;
                self.windows[i].can_move = w.can_move;
            }
        }
    }

    pub fn recompose_except(&mut self, except_id: usize) {
        unsafe {
            let display_server = &mut *(&raw mut DISPLAY_SERVER);
            // Clear double buffer
            if display_server.double_buffer != 0 {
                core::ptr::write_bytes(
                    display_server.double_buffer as *mut u8,
                    0,
                    (display_server.pitch * display_server.height) as usize,
                );
            }

            // Iterate Reverse (Back to Front)
            for i in (0..self.windows.len()).rev() {
                if self.windows[i].id != except_id {
                    match self.windows[i].w_type {
                        Items::Null => {}
                        _ => {
                            display_server.copy_to_db(
                                self.windows[i].width as u32,
                                self.windows[i].height as u32,
                                self.windows[i].buffer,
                                self.windows[i].x as u32,
                                self.windows[i].y as u32,
                            );
                        }
                    }
                }
            }
        }
    }

    pub fn remove_window(&mut self, wid: usize) {
        for i in 0..self.windows.len() {
            if self.windows[i].id == wid {
                self.windows[i].w_type = Items::Null;
                self.windows[i].z = 255;
            }
        }

        self.windows.sort_by_key(|w| w.z);

        unsafe {
            let display_server = &mut *(&raw mut DISPLAY_SERVER);
            if display_server.double_buffer != 0 {
                core::ptr::write_bytes(
                    display_server.double_buffer as *mut u8,
                    0,
                    (display_server.pitch * display_server.height) as usize,
                );
            }

            for j in (0..self.windows.len()).rev() {
                match self.windows[j].w_type {
                    Items::Null => {}
                    _ => {
                        (*(&raw mut DISPLAY_SERVER)).copy_to_db(
                            self.windows[j].width as u32,
                            self.windows[j].height as u32,
                            self.windows[j].buffer,
                            self.windows[j].x as u32,
                            self.windows[j].y as u32,
                        );
                    }
                }
            }

            (*(&raw mut DISPLAY_SERVER)).copy();
        }
    }
}
