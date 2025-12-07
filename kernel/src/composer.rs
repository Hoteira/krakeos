use alloc::collections::VecDeque;
use alloc::vec::Vec;
use crate::display::{Color, DisplayServer, Mouse, State};
use core::sync::atomic::{AtomicU16, Ordering};
use crate::debugln; // Use debugln! instead of libk::print

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

const O: u32 = 0x0000_0000;
const B: u32 = 0x0000_00FF;
const T: u32 = 0xFFFF_FFFF;

const MOUSE_CURSOR: [u32; 96] = [
    B, O, O, O, O, O, O, O,
    B, B, O, O, O, O, O, O,
    B, T, B, O, O, O, O, O,
    B, T, T, B, O, O, O, O,
    B, T, T, T, B, O, O, O,
    B, T, T, T, T, B, O, O,
    B, T, T, T, T, T, B, O,
    B, T, T, T, T, T, T, B,
    B, T, T, T, B, B, B, B,
    B, T, B, B, T, B, O, O,
    B, B, O, O, B, T, B, O,
    B, O, O, O, O, B, B, O,
];

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

        self.framebuffer = vbe.framebuffer as u64;
        
        let size_bytes = self.pitch as usize * self.height as usize;
        // Allocate pages for double buffer
        // PAGE_SIZE is 4096
        let pages = (size_bytes + 4095) / 4096;
        
        unsafe {
            if let Some(buffer) = crate::memory::pmm::allocate_frames(pages) {
                self.double_buffer = buffer;
                debugln!("[DisplayServer] Double buffer allocated at {:#x} ({} pages)", buffer, pages);
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

        let src = self.double_buffer as *const u8;
        let dst = self.framebuffer   as *mut u8;
        let pitch = self.pitch as u32;

        unsafe {
            for row in 0..height {
                let line_start = (y + row) * pitch;
                let offset = line_start + x * bytes_per_pixel;

                core::ptr::copy(
                    src.add(offset as usize),
                    dst.add(offset as usize),
                    (width * bytes_per_pixel) as usize,
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
                            continue;
                        }

                        core::ptr::copy(
                            src_ptr.add(src_offset),
                            dst_ptr.add(dst_offset),
                            4,
                        );
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
                        }

                        *dst_ptr.add(dst_offset) = *src_ptr.add(src_offset);
                        *dst_ptr.add(dst_offset + 1) = *src_ptr.add(src_offset + 1);
                        *dst_ptr.add(dst_offset + 2) = *src_ptr.add(src_offset + 2);
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
                        if src_a == 255 {
                            *dst_ptr.add(dst_offset) = src_r;
                            *dst_ptr.add(dst_offset + 1) = src_g;
                            *dst_ptr.add(dst_offset + 2) = src_b;
                            *dst_ptr.add(dst_offset + 3) = src_a;
                        } else if src_a == 0 {
                            continue;
                        } else {
                            let alpha = src_a as u16;
                            let inv_alpha = 255 - alpha;

                            let dst_r = *dst_ptr.add(dst_offset) as u16;
                            let dst_g = *dst_ptr.add(dst_offset + 1) as u16;
                            let dst_b = *dst_ptr.add(dst_offset + 2) as u16;

                            *dst_ptr.add(dst_offset) = ((src_r as u16 * alpha + dst_r * inv_alpha) / 255) as u8;
                            *dst_ptr.add(dst_offset + 1) = ((src_g as u16 * alpha + dst_g * inv_alpha) / 255) as u8;
                            *dst_ptr.add(dst_offset + 2) = ((src_b as u16 * alpha + dst_b * inv_alpha) / 255) as u8;
                            *dst_ptr.add(dst_offset + 3) = 255;
                        }
                    } else {
                        if src_a == 0 {
                            continue;
                        } else if src_a == 255 {
                            *dst_ptr.add(dst_offset) = src_r;
                            *dst_ptr.add(dst_offset + 1) = src_g;
                            *dst_ptr.add(dst_offset + 2) = src_b;
                        } else {
                            let alpha = src_a as u16;
                            let inv_alpha = 255 - alpha;

                            let dst_r = *dst_ptr.add(dst_offset) as u16;
                            let dst_g = *dst_ptr.add(dst_offset + 1) as u16;
                            let dst_b = *dst_ptr.add(dst_offset + 2) as u16;

                            *dst_ptr.add(dst_offset) = ((src_r as u16 * alpha + dst_r * inv_alpha) / 255) as u8;
                            *dst_ptr.add(dst_offset + 1) = ((src_g as u16 * alpha + dst_g * inv_alpha) / 255) as u8;
                            *dst_ptr.add(dst_offset + 2) = ((src_b as u16 * alpha + dst_b * inv_alpha) / 255) as u8;
                        }
                    }
                }
            }
        }
    }

    pub fn write_pixel(&self, row: u32, col: u32, color: Color) {
        if col < self.width as u32 && row < self.height as u32 {
            unsafe {
                match self.depth {
                    16 => {
                        *((self.framebuffer as *mut u16).add((row * self.width as u32 + col) as usize)) = color.to_u16();
                    },

                    24 => {
                        let color = color.to_u24();
                        *((self.framebuffer as *mut u8).add(((row * self.width as u32 + col) * 3 + 0) as usize)) = color[0];
                        *((self.framebuffer as *mut u8).add(((row * self.width as u32 + col) * 3 + 1) as usize)) = color[1];
                        *((self.framebuffer as *mut u8).add(((row * self.width as u32 + col) * 3 + 2) as usize)) = color[2];
                    }

                    32 => {
                        *((self.framebuffer as *mut u32).add((row * self.width as u32 + col) as usize)) = color.to_u32();
                    }

                    _ => {}
                }
            }
        }
    }

    pub fn draw_mouse(&self, x: u16, y: u16) {

        for i in 0..12 {
            for j in 0..8 {
                let color = MOUSE_CURSOR[(i * 8 + j) as usize];
                if color != O {
                    self.write_pixel(
                        y.wrapping_add(i) as u32,
                        x.wrapping_add(j) as u32,
                        Color::from_u32(color),
                    );
                }
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
        unsafe { (*(&raw mut DISPLAY_SERVER)).copy_to_fb(self.x as u32, self.y as u32, 8, 12) };

        let x_vec = (data[1] as i8) as i16;
        let y_vec = (data[2] as i8) as i16;

        self.x = self.clamp_mx(x_vec);
        self.y = self.clamp_my(-y_vec);

        self.left = (data[0] & 0b00000001) != 0;
        self.right = (data[0] & 0b00000010) != 0;
        self.center = (data[0] & 0b00000100) != 0;

        unsafe {
            LAST_INPUT = data[0];
        }

        unsafe { (*(&raw mut DISPLAY_SERVER)).draw_mouse(self.x, self.y) };
    }

    fn clamp_mx(&self, n: i16) -> u16 {
        let mx_0 = self.x as i16;
        let sx = unsafe { (*(&raw mut DISPLAY_SERVER)).width } as u16;

        if n + mx_0 >= (sx as i16 - 8) {
            sx.wrapping_sub(8)
        } else if n + mx_0 <= 0 {
            0
        } else {
            (n + mx_0) as u16
        }
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

    pub fn clamp_my(&self, n: i16) -> u16 {
        let my_0 = self.y as i16;
        let sy = unsafe { (*(&raw mut DISPLAY_SERVER)).height } as u16;

        if n + my_0 >= (sy as i16 - 12) {
            sy.wrapping_sub(12)
        } else if n + my_0 <= 0 {
            return 0;
        } else {
            (n + my_0) as u16
        }
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
        unsafe {
            (*(&raw mut DRAGGING_WINDOW)).store(0, Ordering::Relaxed);
            (*(&raw mut RESIZING_WINDOW)).store(0, Ordering::Relaxed);
            W_WIDTH = 0;
            W_HEIGHT = 0;
        }

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
