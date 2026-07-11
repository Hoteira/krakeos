#[derive(Debug, Copy, Clone)]
pub struct KeyboardEvent {
    /// Unicode character for the key, 0 if none. Backspace is '\x08',
    /// enter is '\n'.
    pub key: u32,
    /// Raw evdev scancode.
    pub code: u16,
    pub pressed: bool,
    pub repeat: u32,
}

#[derive(Debug, Copy, Clone)]
pub struct MouseEvent {
    pub x: i32,
    pub y: i32,
    pub buttons: [bool; 3],
    pub scroll: i8,
}

#[derive(Debug, Copy, Clone)]
pub struct ResizeEvent {
    pub width: i32,
    pub height: i32,
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Copy, Clone)]
pub enum Event {
    None,
    Keyboard(KeyboardEvent),
    /// Reserved: the kernel does not yet route mouse events per-window.
    Mouse(MouseEvent),
    /// Reserved: window resize is shell-driven for now.
    Resize(ResizeEvent),
}

/// Translate an evdev scancode to a character (US layout, unshifted —
/// mirrors what the old terminal app supported).
pub fn scancode_to_char(code: u16) -> Option<char> {
    Some(match code {
        2..=10 => (b'1' + (code as u8) - 2) as char,
        11 => '0',
        16 => 'q', 17 => 'w', 18 => 'e', 19 => 'r', 20 => 't',
        21 => 'y', 22 => 'u', 23 => 'i', 24 => 'o', 25 => 'p',
        30 => 'a', 31 => 's', 32 => 'd', 33 => 'f', 34 => 'g',
        35 => 'h', 36 => 'j', 37 => 'k', 38 => 'l',
        44 => 'z', 45 => 'x', 46 => 'c', 47 => 'v', 48 => 'b',
        49 => 'n', 50 => 'm',
        57 => ' ',
        28 => '\n',
        14 => '\x08', // backspace
        15 => '\t',   // tab
        12 => '-',
        13 => '=',
        51 => ',',
        52 => '.',
        53 => '/',
        39 => ';',
        40 => '\'',
        _ => return None,
    })
}
