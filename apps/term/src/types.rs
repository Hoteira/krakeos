use alloc::string::String;

pub enum TermAction {
    Backspace,
    CarriageReturn,
    Newline,
    Csi(u8, String),
    Text(String),
}

#[derive(Clone, Copy, PartialEq)]
pub struct Cell {
    pub c: char,
    pub fg: u8,
    pub bg: u8,
    pub bold: bool,
}

impl Cell {
    pub fn default() -> Self {
        Self { c: ' ', fg: 255, bg: 255, bold: false }
    }
}
