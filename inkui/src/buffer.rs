// The cell grid apps draw into, and styled text.

use crate::layout::Rect;
use crate::types::{Color, Style, GLASS};

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Cell {
    pub ch: char,
    pub fg: Color,
    pub bg: Color,
    pub attrs: u32,
}

impl Cell {
    pub const BLANK: Cell = Cell { ch: ' ', fg: crate::palette::TEXT, bg: GLASS, attrs: 0 };

    pub fn style(&self) -> Style {
        Style { fg: self.fg, bg: self.bg, attrs: self.attrs }
    }
}

// A styled run of text.
#[derive(Clone, Debug, Default)]
pub struct Span {
    pub text: String,
    pub style: Style,
}

impl Span {
    pub fn new(text: impl Into<String>, style: Style) -> Span {
        Span { text: text.into(), style }
    }
    pub fn raw(text: impl Into<String>) -> Span {
        Span { text: text.into(), style: Style::new() }
    }
    pub fn width(&self) -> usize {
        self.text.chars().count()
    }
}

#[derive(Clone, Debug, Default)]
pub struct Line {
    pub spans: Vec<Span>,
}

impl Line {
    pub fn new() -> Line {
        Line { spans: Vec::new() }
    }
    pub fn styled(text: impl Into<String>, style: Style) -> Line {
        Line { spans: vec![Span::new(text, style)] }
    }
    pub fn push(mut self, text: impl Into<String>, style: Style) -> Line {
        self.spans.push(Span::new(text, style));
        self
    }
    pub fn add(&mut self, text: impl Into<String>, style: Style) {
        self.spans.push(Span::new(text, style));
    }
    pub fn width(&self) -> usize {
        self.spans.iter().map(|s| s.width()).sum()
    }
    pub fn plain(&self) -> String {
        self.spans.iter().map(|s| s.text.as_str()).collect()
    }
}

impl From<&str> for Line {
    fn from(s: &str) -> Line {
        Line { spans: vec![Span::raw(s)] }
    }
}

impl From<String> for Line {
    fn from(s: String) -> Line {
        Line { spans: vec![Span::raw(s)] }
    }
}

impl From<Span> for Line {
    fn from(s: Span) -> Line {
        Line { spans: vec![s] }
    }
}

pub struct Buffer {
    pub area: Rect,
    pub cells: Vec<Cell>,
}

impl Buffer {
    pub fn new(w: usize, h: usize) -> Buffer {
        Buffer { area: Rect::new(0, 0, w, h), cells: vec![Cell::BLANK; w * h] }
    }

    pub fn resize(&mut self, w: usize, h: usize) {
        self.area = Rect::new(0, 0, w, h);
        self.cells = vec![Cell::BLANK; w * h];
    }

    pub fn clear(&mut self) {
        self.cells.fill(Cell::BLANK);
    }

    pub fn get(&self, x: usize, y: usize) -> Option<&Cell> {
        if x < self.area.w && y < self.area.h {
            self.cells.get(y * self.area.w + x)
        } else {
            None
        }
    }

    pub fn get_mut(&mut self, x: usize, y: usize) -> Option<&mut Cell> {
        if x < self.area.w && y < self.area.h {
            self.cells.get_mut(y * self.area.w + x)
        } else {
            None
        }
    }

    pub fn set(&mut self, x: usize, y: usize, ch: char, style: Style) {
        if let Some(c) = self.get_mut(x, y) {
            *c = Cell { ch, fg: style.fg, bg: style.bg, attrs: style.attrs };
        }
    }

    // Change a cell's colors but keep its character.
    pub fn paint(&mut self, x: usize, y: usize, style: Style) {
        if let Some(c) = self.get_mut(x, y) {
            c.fg = style.fg;
            c.bg = style.bg;
            c.attrs = style.attrs;
        }
    }

    // Write s at (x, y), at most max_w cells. Returns the x after it.
    pub fn text(&mut self, x: usize, y: usize, s: &str, max_w: usize, style: Style) -> usize {
        let mut cx = x;
        for ch in s.chars() {
            if cx >= x.saturating_add(max_w) || cx >= self.area.w {
                break;
            }
            let ch = if ch == '\t' || ch.is_control() { ' ' } else { ch };
            self.set(cx, y, ch, style);
            cx += 1;
        }
        cx
    }

    // Write a line of spans, clipped to max_w. Returns the x after it.
    pub fn line(&mut self, x: usize, y: usize, line: &Line, max_w: usize) -> usize {
        let mut cx = x;
        for span in &line.spans {
            let left = x.saturating_add(max_w).saturating_sub(cx);
            if left == 0 {
                break;
            }
            cx = self.text(cx, y, &span.text, left, span.style);
        }
        cx
    }

    // Fill a rect with one character and style.
    pub fn fill(&mut self, r: Rect, ch: char, style: Style) {
        let r = r.intersect(&self.area);
        for y in r.y..r.bottom() {
            for x in r.x..r.right() {
                self.set(x, y, ch, style);
            }
        }
    }

    // Set the background of a rect, keeping characters and foregrounds.
    pub fn tint(&mut self, r: Rect, bg: Color) {
        let r = r.intersect(&self.area);
        for y in r.y..r.bottom() {
            for x in r.x..r.right() {
                if let Some(c) = self.get_mut(x, y) {
                    c.bg = bg;
                }
            }
        }
    }
}
