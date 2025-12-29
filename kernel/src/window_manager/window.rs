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

pub static NULL_WINDOW: Window = Window {
    id: 0,
    buffer: 0,
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
};
