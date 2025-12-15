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
