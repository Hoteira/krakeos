// Events from the desktop and key decoding (Italian or US layout, from
// /config/system.toml).

use std::sync::atomic::{AtomicBool, Ordering};

pub const MOD_SHIFT: u32 = 1;
pub const MOD_CTRL: u32 = 2;
pub const MOD_ALT: u32 = 4;
pub const MOD_SUPER: u32 = 8;
pub const MOD_CAPS: u32 = 16;
// Right Alt (AltGr), for the third-level symbols: @ # [ ] { } ~ €.
pub const MOD_ALTGR: u32 = 32;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Event {
    Key(KeyEvent),
    // The grid changed size: (cols, rows).
    Resize(usize, usize),
    // The app's desktop became visible (true) or was switched away from.
    Focus(bool),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Key {
    Char(char),
    Enter,
    Backspace,
    Tab,
    BackTab,
    Esc,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    Delete,
    F(u8),
    Unknown(u16),
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct KeyEvent {
    pub key: Key,
    pub mods: u32,
}

impl KeyEvent {
    pub fn ctrl(&self) -> bool {
        self.mods & MOD_CTRL != 0
    }
    pub fn alt(&self) -> bool {
        self.mods & MOD_ALT != 0
    }
    pub fn altgr(&self) -> bool {
        self.mods & MOD_ALTGR != 0
    }
    // Win (or its Right Ctrl alias). Only Up/Down reach apps with it held.
    pub fn super_(&self) -> bool {
        self.mods & MOD_SUPER != 0
    }
    pub fn shift(&self) -> bool {
        self.mods & MOD_SHIFT != 0
    }
    // Ctrl plus the letter c (given lowercase).
    pub fn is_ctrl(&self, c: char) -> bool {
        self.ctrl() && self.key == Key::Char(c)
    }
    // A printable character typed without Ctrl/Alt. AltGr counts even with Ctrl
    // down, since Windows hosts send AltGr as Ctrl+Right Alt.
    pub fn text(&self) -> Option<char> {
        match self.key {
            Key::Char(c) if (!self.ctrl() && !self.alt()) || self.altgr() => Some(c),
            _ => None,
        }
    }
}

static US_LAYOUT: AtomicBool = AtomicBool::new(false);

// Pick the keyboard layout by name ("it" or "us"); anything else keeps Italian.
pub fn set_layout(name: &str) {
    US_LAYOUT.store(name.trim().eq_ignore_ascii_case("us"), Ordering::Relaxed);
}

fn layout(code: u16) -> Option<[char; 4]> {
    if US_LAYOUT.load(Ordering::Relaxed) {
        layout_us(code)
    } else {
        layout_it(code)
    }
}

// US layout: same shape as layout_it, without an AltGr level.
fn layout_us(code: u16) -> Option<[char; 4]> {
    const ROWS: [(u16, &str, &str); 5] = [
        (2, "1234567890-=", "!@#$%^&*()_+"),
        (16, "qwertyuiop[]", "QWERTYUIOP{}"),
        (30, "asdfghjkl;'`", "ASDFGHJKL:\"~"),
        (43, "\\zxcvbnm,./", "|ZXCVBNM<>?"),
        (86, "\\", "|"),
    ];
    for (start, plain, shifted) in ROWS {
        let i = code.checked_sub(start)? as usize;
        if let (Some(a), Some(b)) = (plain.chars().nth(i), shifted.chars().nth(i)) {
            return Some([a, b, '\0', '\0']);
        }
        if code < start + plain.len() as u16 {
            return None;
        }
    }
    None
}

// Italian layout: (plain, shift, altgr, altgr+shift) per key code, with
// '\0' where a level has nothing.
fn layout_it(code: u16) -> Option<[char; 4]> {
    const N: char = '\0';
    Some(match code {
        2 => ['1', '!', N, N],
        3 => ['2', '"', N, N],
        4 => ['3', '£', N, N],
        5 => ['4', '$', N, N],
        6 => ['5', '%', '€', N],
        7 => ['6', '&', N, N],
        8 => ['7', '/', '{', N],
        9 => ['8', '(', '[', N],
        10 => ['9', ')', ']', N],
        11 => ['0', '=', '}', N],
        12 => ['\'', '?', '`', N],
        13 => ['ì', '^', '~', N],
        16 => ['q', 'Q', N, N],
        17 => ['w', 'W', N, N],
        18 => ['e', 'E', '€', N],
        19 => ['r', 'R', N, N],
        20 => ['t', 'T', N, N],
        21 => ['y', 'Y', N, N],
        22 => ['u', 'U', N, N],
        23 => ['i', 'I', N, N],
        24 => ['o', 'O', N, N],
        25 => ['p', 'P', N, N],
        26 => ['è', 'é', '[', '{'],
        27 => ['+', '*', ']', '}'],
        30 => ['a', 'A', N, N],
        31 => ['s', 'S', N, N],
        32 => ['d', 'D', N, N],
        33 => ['f', 'F', N, N],
        34 => ['g', 'G', N, N],
        35 => ['h', 'H', N, N],
        36 => ['j', 'J', N, N],
        37 => ['k', 'K', N, N],
        38 => ['l', 'L', N, N],
        39 => ['ò', 'ç', '@', N],
        40 => ['à', '°', '#', N],
        41 => ['\\', '|', N, N],
        43 => ['ù', '§', '`', N],
        44 => ['z', 'Z', N, N],
        45 => ['x', 'X', N, N],
        46 => ['c', 'C', N, N],
        47 => ['v', 'V', N, N],
        48 => ['b', 'B', N, N],
        49 => ['n', 'N', N, N],
        50 => ['m', 'M', N, N],
        51 => [',', ';', N, N],
        52 => ['.', ':', N, N],
        53 => ['-', '_', N, N],
        86 => ['<', '>', N, N],
        _ => return None,
    })
}

// Key code (Linux input numbering) plus modifiers to a key; None for bare
// modifiers. With Ctrl held, letters come back lowercase regardless of Shift.
pub fn decode(code: u16, mods: u32) -> Option<KeyEvent> {
    let shift = mods & MOD_SHIFT != 0;
    let caps = mods & MOD_CAPS != 0;
    if let Some(levels) = layout(code) {
        let [plain, shifted, alt, alt_shifted] = levels;
        let c = if mods & MOD_ALTGR != 0 && alt != '\0' {
            if shift && alt_shifted != '\0' { alt_shifted } else { alt }
        } else if mods & MOD_CTRL != 0 {
            plain
        } else {
            let upper = if plain.is_ascii_alphabetic() { shift ^ caps } else { shift };
            if upper { shifted } else { plain }
        };
        return Some(KeyEvent { key: Key::Char(c), mods });
    }
    let key = match code {
        1 => Key::Esc,
        14 => Key::Backspace,
        15 => if shift { Key::BackTab } else { Key::Tab },
        28 | 96 => Key::Enter,
        55 => Key::Char('*'),
        57 => Key::Char(' '),
        59..=68 => Key::F((code - 58) as u8),
        87 => Key::F(11),
        88 => Key::F(12),
        // Keypad (no Num Lock tracking: digits always).
        71 => Key::Char('7'),
        72 => Key::Char('8'),
        73 => Key::Char('9'),
        74 => Key::Char('-'),
        75 => Key::Char('4'),
        76 => Key::Char('5'),
        77 => Key::Char('6'),
        78 => Key::Char('+'),
        79 => Key::Char('1'),
        80 => Key::Char('2'),
        81 => Key::Char('3'),
        82 => Key::Char('0'),
        83 => Key::Char('.'),
        98 => Key::Char('/'),
        102 => Key::Home,
        103 => Key::Up,
        104 => Key::PageUp,
        105 => Key::Left,
        106 => Key::Right,
        107 => Key::End,
        108 => Key::Down,
        109 => Key::PageDown,
        110 => Key::Insert,
        111 => Key::Delete,
        29 | 97 | 42 | 54 | 56 | 100 | 58 | 125 | 126 | 69 | 70 => return None,
        _ => Key::Unknown(code),
    };
    Some(KeyEvent { key, mods })
}
