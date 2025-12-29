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
            ((color >> 16) & 0xFF) as u8, 
            ((color >> 8)  & 0xFF) as u8, 
            (color         & 0xFF) as u8, 
            ((color >> 24) & 0xFF) as u8, 
        )
    }

    pub fn to_u24(&self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }
}


#[derive(Debug, Copy, Clone)]
pub enum GradientDirection {
    Horizontal,
    Vertical,
    Diagonal,
    DiagonalAlt,
    Custom { angle: f32 },
}

#[derive(Debug, Clone, Copy)]
pub struct LinearGradient {
    pub start_color: Color,
    pub end_color: Color,
    pub direction: GradientDirection,
}

impl LinearGradient {
    pub const fn new(start: Color, end: Color, direction: GradientDirection) -> Self {
        Self {
            start_color: start,
            end_color: end,
            direction,
        }
    }

    pub const fn horizontal(start: Color, end: Color) -> Self {
        Self::new(start, end, GradientDirection::Horizontal)
    }

    pub const fn vertical(start: Color, end: Color) -> Self {
        Self::new(start, end, GradientDirection::Vertical)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum BackgroundStyle {
    Solid(Color),
    Gradient(LinearGradient),
}

impl BackgroundStyle {
    pub const fn solid(color: Color) -> Self {
        BackgroundStyle::Solid(color)
    }

    pub const fn gradient(gradient: LinearGradient) -> Self {
        BackgroundStyle::Gradient(gradient)
    }
}