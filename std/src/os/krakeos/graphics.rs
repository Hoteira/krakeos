// --- Window/screen method_export! bindings ---

method_export!("krakeos:system/window@0.2.0", "create",
    pub fn window_create(attributes_ptr: *const u8) -> u64 {
        crate::sys::syscall(100, attributes_ptr as u64, 0, 0)
    }
);

method_export!("krakeos:system/window@0.2.0", "update",
    pub fn window_update(_handle: u64, attributes_ptr: *const u8) {
        crate::sys::syscall(102, attributes_ptr as u64, 0, 0);
    }
);

method_export!("krakeos:system/window@0.2.0", "update-area",
    pub fn window_update_area(id: u64, x: u64, y: u64, w: u64, h: u64) {
        crate::sys::syscall5(103, id, x, y, w, h);
    }
);

method_export!("krakeos:system/window@0.2.0", "get-events",
    pub fn window_get_events(_handle: u64, buf_ptr: *mut u8, max: u32) -> i32 {
        crate::sys::syscall(104, 0, buf_ptr as u64, max as u64) as i32
    }
);

method_export!("krakeos:graphics/screen@0.2.0", "get-width",
    pub fn screen_get_width() -> u32 {
        crate::sys::syscall(106, 0, 0, 0) as u32
    }
);

method_export!("krakeos:graphics/screen@0.2.0", "get-height",
    pub fn screen_get_height() -> u32 {
        crate::sys::syscall(107, 0, 0, 0) as u32
    }
);

// --- Types ---

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

// --- Simplified public API ---

pub fn get_screen_width() -> usize {
    screen_get_width() as usize
}

pub fn get_screen_height() -> usize {
    screen_get_height() as usize
}

pub fn add_window(window: &Window) -> usize {
    let res = window_create(window as *const Window as *const u8) as usize;
    res
}

pub fn update_window(window: &Window) {
    window_update(0, window as *const Window as *const u8);
}

pub fn update_window_area(id: usize, x: usize, y: usize, w: usize, h: usize) {
    window_update_area(id as u64, x as u64, y as u64, w as u64, h as u64);
}

pub fn get_events(wid: usize, events: &mut [Event]) -> usize {
    let ptr = events.as_mut_ptr() as *mut u8;
    window_get_events(wid as u64, ptr, events.len() as u32) as usize
}

pub use super::events::{Event, KeyboardEvent, MouseEvent, RedrawEvent, ResizeEvent};
