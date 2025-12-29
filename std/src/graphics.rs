
use crate::os::syscall;

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
    
    pub fn to_u32(&self) -> u32 {
        ((self.a as u32) << 24) | ((self.r as u32) << 16) | ((self.g as u32) << 8) | (self.b as u32)
    }
}

pub fn add_window(window: &Window) -> usize {
    unsafe {
        syscall(22, window as *const _ as u64, 0, 0) as usize
    }
}

pub fn update_window(window: &Window) {
    unsafe {
        syscall(51, window as *const _ as u64, 0, 0);
    }
}

pub fn get_screen_width() -> usize {
    unsafe { syscall(44, 0, 0, 0) as usize }
}

pub fn get_screen_height() -> usize {
    unsafe { syscall(45, 0, 0, 0) as usize }
}

pub fn get_mouse_pos() -> (usize, usize) {
    let res = unsafe { syscall(53, 0, 0, 0) };
    let x = (res >> 32) as usize;
    let y = (res & 0xFFFFFFFF) as usize;
    (x, y)
}

pub fn get_events(window_id: usize, events_ptr: usize, max_events: usize) {
    unsafe { syscall(52, window_id as u64, events_ptr as u64, max_events as u64); }
}
