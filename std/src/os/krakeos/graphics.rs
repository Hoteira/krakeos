#[cfg(not(target_arch = "wasm32"))]
use crate::os::krakeos::syscall;
#[cfg(not(target_arch = "wasm32"))]
use crate::os::krakeos::syscall5;

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
    pub prev_x: isize,
    pub prev_y: isize,
    pub prev_width: usize,
    pub prev_height: usize,
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
            prev_x: 0,
            prev_y: 0,
            prev_width: 0,
            prev_height: 0,
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
    unsafe { crate::wasi::krakeos::get_screen_width() as usize }
}

pub fn get_screen_height() -> usize {
    unsafe { crate::wasi::krakeos::get_screen_height() as usize }
}

pub fn add_window(window: &Window) -> usize {
    #[cfg(not(target_arch = "wasm32"))]
    unsafe { syscall(100, window as *const Window as u64, 0, 0) as usize }
    #[cfg(target_arch = "wasm32")]
    unsafe { crate::wasi::krakeos::window_create(window as *const Window as *const u8) as usize }
}

pub fn update_window(window: &Window) {
    #[cfg(not(target_arch = "wasm32"))]
    unsafe { syscall(102, window as *const Window as u64, 0, 0); }
    #[cfg(target_arch = "wasm32")]
    unsafe { crate::wasi::krakeos::window_update(0, window as *const Window as *const u8); }
}

pub fn update_window_area(id: usize, x: usize, y: usize, w: usize, h: usize) {
    #[cfg(not(target_arch = "wasm32"))]
    unsafe { syscall5(103, id as u64, x as u64, y as u64, w as u64, h as u64); }
}

pub fn get_events(wid: usize, events: &mut [Event]) -> usize {
    #[cfg(not(target_arch = "wasm32"))]
    unsafe {
        crate::os::syscall(104, wid as u64, events.as_mut_ptr() as u64, events.len() as u64) as usize
    }
    #[cfg(target_arch = "wasm32")]
    unsafe {
        // We cast Event array to u8 buffer. Ensure layout matches!
        // Event is repr(C).
        let ptr = events.as_mut_ptr() as *mut u8;
        crate::wasi::krakeos::window_get_events(wid as u64, ptr, events.len() as u32) as usize
    }
}

pub use super::events::{Event, KeyboardEvent, MouseEvent, RedrawEvent, ResizeEvent};
