use crate::os::krakeos::syscall;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[repr(u32)]
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
    pub back_buffer: usize,
    pub flipped: usize,
    pub pid: u64,
    pub x: isize,
    pub y: isize,
    pub z: usize,
    pub width: usize,
    pub height: usize,
    pub can_move: bool,
    pub can_resize: bool,
    pub transparent: bool,
    pub treat_as_transparent: bool,
    pub min_width: usize,
    pub min_height: usize,
    pub event_handler: usize,
    pub w_type: Items,
}

impl Window {
    pub fn new(width: usize, height: usize, buffer: usize) -> Self {
        Window {
            id: 0,
            buffer,
            back_buffer: 0,
            flipped: 0,
            pid: 0,
            x: 0,
            y: 0,
            z: 0,
            width,
            height,
            can_move: true,
            can_resize: true,
            transparent: true,
            treat_as_transparent: true,
            min_width: 0,
            min_height: 0,
            event_handler: 0,
            w_type: Items::Window,
        }
    }
}

#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }
    pub fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
    pub fn to_u32(&self) -> u32 {
        ((self.a as u32) << 24) | ((self.r as u32) << 16) | ((self.g as u32) << 8) | (self.b as u32)
    }
}

pub fn get_screen_width() -> usize {
    unsafe { syscall(106, 0, 0, 0) as usize }
}

pub fn get_screen_height() -> usize {
    unsafe { syscall(107, 0, 0, 0) as usize }
}

pub fn add_window(window: &Window) -> usize {
    unsafe { crate::os::syscall(100, window as *const Window as u64, 0, 0) as usize }
}

pub fn update_window(window: &Window) {
    unsafe { crate::os::syscall(102, window as *const Window as u64, 0, 0); }
}

pub fn update_window_area(id: usize, x: usize, y: usize, w: usize, h: usize) {
    unsafe { crate::os::syscall5(103, id as u64, x as u64, y as u64, w as u64, h as u64); }
}

pub use super::events::{Event, KeyboardEvent, MouseEvent, RedrawEvent, ResizeEvent};
