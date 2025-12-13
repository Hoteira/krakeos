#[derive(Debug, Copy, Clone)]
pub enum Size {
    Absolute(usize),
    Relative(usize),

    FromRight(usize),
    FromLeft(usize),

    FromUp(usize),
    FromDown(usize),
    Auto,
}

#[derive(Debug, Copy, Clone)]
pub enum Align {
    Center,
    Left,
    Right,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Color {
        Color {
            r,
            g,
            b,
            a: 255,
        }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
        Color {
            r,
            g,
            b,
            a,
        }
    }

    pub fn to_u32(&self) -> u32 {
        ((self.a as u32) << 24) | ((self.r as u32) << 16) | ((self.g as u32) << 8) | (self.b as u32)
    }

    pub fn from_u32(color: u32) -> Color {
        Color::rgba(
            ((color >> 16) & 0xFF) as u8, // Red
            ((color >> 8)  & 0xFF) as u8, // Green
            (color         & 0xFF) as u8, // Blue
            ((color >> 24) & 0xFF) as u8, // Alpha
        )
    }

    pub fn to_u24(&self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }
}