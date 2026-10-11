// Colors (0xAARRGGBB) and styles. A background with alpha 0 (GLASS) shows the
// desktop's translucent default; partial alpha blends over it.

pub type Color = u32;

// The desktop's translucent default background.
pub const GLASS: Color = 0;

pub const fn rgb(r: u8, g: u8, b: u8) -> Color {
    0xFF00_0000 | (r as u32) << 16 | (g as u32) << 8 | b as u32
}

pub const fn with_alpha(c: Color, a: u8) -> Color {
    (c & 0x00FF_FFFF) | (a as u32) << 24
}

// Blend b over a by t (0..=255), keeping full alpha.
pub fn mix(a: Color, b: Color, t: u8) -> Color {
    let t = t as u32;
    let ch = |s: u32| {
        let (x, y) = ((a >> s) & 0xFF, (b >> s) & 0xFF);
        ((x * (255 - t) + y * t) / 255) << s
    };
    0xFF00_0000 | ch(16) | ch(8) | ch(0)
}

// Catppuccin Mocha.
pub mod palette {
    use super::Color;
    pub const ROSEWATER: Color = 0xFFF5_E0DC;
    pub const FLAMINGO: Color = 0xFFF2_CDCD;
    pub const PINK: Color = 0xFFF5_C2E7;
    pub const MAUVE: Color = 0xFFCB_A6F7;
    pub const RED: Color = 0xFFF3_8BA8;
    pub const MAROON: Color = 0xFFEB_A0AC;
    pub const PEACH: Color = 0xFFFA_B387;
    pub const YELLOW: Color = 0xFFF9_E2AF;
    pub const GREEN: Color = 0xFFA6_E3A1;
    pub const TEAL: Color = 0xFF94_E2D5;
    pub const SKY: Color = 0xFF89_DCEB;
    pub const SAPPHIRE: Color = 0xFF74_C7EC;
    pub const BLUE: Color = 0xFF89_B4FA;
    pub const LAVENDER: Color = 0xFFB4_BEFE;
    pub const TEXT: Color = 0xFFCD_D6F4;
    pub const SUBTEXT1: Color = 0xFFBA_C2DE;
    pub const SUBTEXT0: Color = 0xFFA6_ADC8;
    pub const OVERLAY2: Color = 0xFF93_99B2;
    pub const OVERLAY1: Color = 0xFF7F_849C;
    pub const OVERLAY0: Color = 0xFF6C_7086;
    pub const SURFACE2: Color = 0xFF58_5B70;
    pub const SURFACE1: Color = 0xFF45_475A;
    pub const SURFACE0: Color = 0xFF31_3244;
    pub const BASE: Color = 0xFF1E_1E2E;
    pub const MANTLE: Color = 0xFF18_1825;
    pub const CRUST: Color = 0xFF11_111B;

    // Accent pair used for focused borders and highlights.
    pub const ACCENT: Color = BLUE;
    pub const ACCENT2: Color = MAUVE;
}

pub const BOLD: u32 = 1 << 24;
pub const UNDERLINE: u32 = 1 << 25;

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Style {
    pub fg: Color,
    pub bg: Color,
    pub attrs: u32,
}

impl Default for Style {
    fn default() -> Self {
        Style::new()
    }
}

impl Style {
    pub const fn new() -> Style {
        Style { fg: palette::TEXT, bg: GLASS, attrs: 0 }
    }
    pub const fn fg(mut self, c: Color) -> Style {
        self.fg = c;
        self
    }
    pub const fn bg(mut self, c: Color) -> Style {
        self.bg = c;
        self
    }
    pub const fn bold(mut self) -> Style {
        self.attrs |= BOLD;
        self
    }
    pub const fn underline(mut self) -> Style {
        self.attrs |= UNDERLINE;
        self
    }
    // Swap foreground and background (an opaque one if the bg is glass).
    pub const fn reversed(self) -> Style {
        let bg = if self.bg >> 24 == 0 { palette::BASE } else { self.bg };
        Style { fg: bg, bg: self.fg, attrs: self.attrs }
    }
}
