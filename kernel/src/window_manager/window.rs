#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[repr(C)]
pub enum Items {
    Wallpaper,
    Bar,
    Popup,
    Window,
    Null,
}

#[derive(Debug, Clone)]
#[repr(C, align(8))]
#[derive(Copy)]
pub struct Window {
    pub id: u64,
    pub buffer: u64,
    pub back_buffer: u64,
    pub flipped: u64, // Pointer to AtomicBool
    pub pid: u64,
    pub x: i64,
    pub y: i64,
    pub z: u64,
    pub width: u64,
    pub height: u64,
    pub can_move: bool,
    pub can_resize: bool,
    pub transparent: bool,
    pub treat_as_transparent: bool,
    pub is_maximized: bool,
    pub _pad0: [u8; 3], // Align to 8 bytes
    pub min_width: u64,
    pub min_height: u64,
    pub event_handler: u64,
    pub w_type: Items,
    pub _pad1: [u8; 4], // Align to 8 bytes
    // Previous state for maximize toggle
    pub prev_x: i64,
    pub prev_y: i64,
    pub prev_width: u64,
    pub prev_height: u64,
    // Tiling target state
    pub tiled_x: i64,
    pub tiled_y: i64,
    pub tiled_width: u64,
    pub tiled_height: u64,
}

impl Window {
    pub fn get_active_buffer(&self) -> u64 {
        if self.flipped == 0 {
            return self.buffer;
        }

        let flipped = unsafe { &*(self.flipped as *const core::sync::atomic::AtomicBool) };
        if flipped.load(core::sync::atomic::Ordering::Acquire) {
            self.back_buffer
        } else {
            self.buffer
        }
    }
}

pub static NULL_WINDOW: Window = Window {
    id: 0,
    buffer: 0,
    back_buffer: 0,
    flipped: 0,
    pid: 0,
    x: 0,
    y: 0,
    z: 0,
    width: 0,
    height: 0,
    can_move: false,
    can_resize: false,
    transparent: true,
    treat_as_transparent: true,
    is_maximized: false,
    _pad0: [0; 3],
    min_width: 0,
    min_height: 0,
    event_handler: 0,
    w_type: Items::Null,
    _pad1: [0; 4],
    prev_x: 0,
    prev_y: 0,
    prev_width: 0,
    prev_height: 0,
    tiled_x: 0,
    tiled_y: 0,
    tiled_width: 0,
    tiled_height: 0,
};
