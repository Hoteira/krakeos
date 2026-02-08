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
#[repr(C)]
#[derive(Copy)]
pub struct Window {
    pub id: usize,
    pub buffer: usize,
    pub back_buffer: usize,
    pub flipped: usize, // Pointer to AtomicBool
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
    // Previous state for maximize toggle
    pub prev_x: isize,
    pub prev_y: isize,
    pub prev_width: usize,
    pub prev_height: usize,
}

impl Window {
    pub fn get_active_buffer(&self) -> usize {
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
    min_width: 0,
    min_height: 0,
    event_handler: 0,
    w_type: Items::Null,
    prev_x: 0,
    prev_y: 0,
    prev_width: 0,
    prev_height: 0,
};
