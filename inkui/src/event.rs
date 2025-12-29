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
    pub key: u32,
    pub pressed: bool,
    pub repeat: u16,
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
