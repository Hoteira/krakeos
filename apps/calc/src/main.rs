// Keyboard calculator with big digits and a history.
// Supports + - * / % ^, parentheses, common functions, pi, e and ans (the last result).

use std::time::Instant;

use inkui::buffer::{Buffer, Line};
use inkui::event::{Event, Key, KeyEvent};
use inkui::icons;
use inkui::layout::{Constraint, Rect};
use inkui::types::{mix, palette as p, with_alpha, Color, Style};
use inkui::widget::{self, Block, Input, InputResult, List, ListState};
use inkui::{App, Control};

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
    ans: f64,
}

impl<'a> Parser<'a> {
    fn ws(&mut self) {
        while self.i < self.s.len() && self.s[self.i] == b' ' {
            self.i += 1;
        }
    }
    fn peek(&mut self) -> Option<u8> {
        self.ws();
        self.s.get(self.i).copied()
    }
    fn expr(&mut self) -> Result<f64, String> {
        let mut v = self.term()?;
        while let Some(c) = self.peek() {
            match c {
                b'+' => {
                    self.i += 1;
                    v += self.term()?;
                }
                b'-' => {
                    self.i += 1;
                    v -= self.term()?;
                }
                _ => break,
            }
        }
        Ok(v)
    }
    fn term(&mut self) -> Result<f64, String> {
        let mut v = self.unary()?;
        while let Some(c) = self.peek() {
            match c {
                b'*' | b'x' => {
                    self.i += 1;
                    v *= self.unary()?;
                }
                b'/' => {
                    self.i += 1;
                    let d = self.unary()?;
                    if d == 0.0 {
                        return Err("division by zero".into());
                    }
                    v /= d;
                }
                b'%' => {
                    self.i += 1;
                    let d = self.unary()?;
                    if d == 0.0 {
                        return Err("division by zero".into());
                    }
                    v %= d;
                }
                _ => break,
            }
        }
        Ok(v)
    }
    fn unary(&mut self) -> Result<f64, String> {
        match self.peek() {
            Some(b'-') => {
                self.i += 1;
                Ok(-self.unary()?)
            }
            Some(b'+') => {
                self.i += 1;
                self.unary()
            }
            _ => self.power(),
        }
    }
    fn power(&mut self) -> Result<f64, String> {
        let base = self.atom()?;
        if self.peek() == Some(b'^') {
            self.i += 1;
            let exp = self.unary()?; // right associative
            return Ok(base.powf(exp));
        }
        Ok(base)
    }
    fn atom(&mut self) -> Result<f64, String> {
        match self.peek() {
            Some(b'(') => {
                self.i += 1;
                let v = self.expr()?;
                if self.peek() != Some(b')') {
                    return Err("missing )".into());
                }
                self.i += 1;
                Ok(v)
            }
            Some(c) if c.is_ascii_digit() || c == b'.' => {
                let start = self.i;
                while self.i < self.s.len() && (self.s[self.i].is_ascii_digit() || self.s[self.i] == b'.') {
                    self.i += 1;
                }
                let t = std::str::from_utf8(&self.s[start..self.i]).unwrap();
                t.parse::<f64>().map_err(|_| format!("bad number {}", t))
            }
            Some(c) if c.is_ascii_alphabetic() => {
                let start = self.i;
                while self.i < self.s.len() && self.s[self.i].is_ascii_alphanumeric() {
                    self.i += 1;
                }
                let name = std::str::from_utf8(&self.s[start..self.i]).unwrap().to_ascii_lowercase();
                match name.as_str() {
                    "pi" => return Ok(std::f64::consts::PI),
                    "e" => return Ok(std::f64::consts::E),
                    "ans" => return Ok(self.ans),
                    _ => {}
                }
                let arg = self.atom()?;
                Ok(match name.as_str() {
                    "sqrt" => {
                        if arg < 0.0 {
                            return Err("sqrt of a negative number".into());
                        }
                        arg.sqrt()
                    }
                    "sin" => arg.sin(),
                    "cos" => arg.cos(),
                    "tan" => arg.tan(),
                    "ln" => arg.ln(),
                    "log" => arg.log10(),
                    "abs" => arg.abs(),
                    "exp" => arg.exp(),
                    "floor" => arg.floor(),
                    "ceil" => arg.ceil(),
                    "round" => arg.round(),
                    _ => return Err(format!("unknown function {}", name)),
                })
            }
            Some(c) => Err(format!("unexpected '{}'", c as char)),
            None => Err("incomplete".into()),
        }
    }
}

fn eval(s: &str, ans: f64) -> Result<f64, String> {
    let mut p = Parser { s: s.as_bytes(), i: 0, ans };
    let v = p.expr()?;
    if p.peek().is_some() {
        return Err(format!("unexpected '{}'", p.s[p.i] as char));
    }
    if !v.is_finite() {
        return Err("result is not a number".into());
    }
    Ok(v)
}

fn fmt(v: f64) -> String {
    if v == v.trunc() && v.abs() < 1e15 {
        return format!("{}", v as i64);
    }
    let s = format!("{:.10}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.').to_string();
    if s.len() > 18 { format!("{:.6e}", v) } else { s }
}

// 3x5 glyphs as rows of 3 bits.
fn glyph(c: char) -> Option<[u8; 5]> {
    Some(match c {
        '0' => [7, 5, 5, 5, 7],
        '1' => [2, 6, 2, 2, 7],
        '2' => [7, 1, 7, 4, 7],
        '3' => [7, 1, 7, 1, 7],
        '4' => [5, 5, 7, 1, 1],
        '5' => [7, 4, 7, 1, 7],
        '6' => [7, 4, 7, 5, 7],
        '7' => [7, 1, 2, 2, 2],
        '8' => [7, 5, 7, 5, 7],
        '9' => [7, 5, 7, 1, 7],
        '.' => [0, 0, 0, 0, 2],
        '-' => [0, 0, 7, 0, 0],
        _ => return None,
    })
}

// Draws text in 3-row half-block digits, right-aligned to right.
// Returns false if it doesn't fit or has a character without a glyph.
fn big_text(buf: &mut Buffer, right: usize, y: usize, max_w: usize, text: &str) -> bool {
    let glyphs: Option<Vec<[u8; 5]>> = text.chars().map(glyph).collect();
    let Some(glyphs) = glyphs else { return false };
    let w = glyphs.len() * 4;
    if w > max_w || w > right {
        return false;
    }
    let x0 = right - w;
    for (gi, g) in glyphs.iter().enumerate() {
        let t = if glyphs.len() > 1 { (gi * 255 / (glyphs.len() - 1)) as u8 } else { 0 };
        let color = mix(p::SAPPHIRE, p::MAUVE, t);
        for col in 0..3 {
            for row in 0..3 {
                let top = g.get(row * 2).map_or(false, |r| r >> (2 - col) & 1 == 1);
                let bot = g.get(row * 2 + 1).map_or(false, |r| r >> (2 - col) & 1 == 1);
                let ch = match (top, bot) {
                    (true, true) => '\u{2588}',
                    (true, false) => '\u{2580}',
                    (false, true) => '\u{2584}',
                    _ => continue,
                };
                buf.set(x0 + gi * 4 + col, y + row, ch, Style::new().fg(color));
            }
        }
    }
    true
}

const PAD: [[&str; 5]; 4] = [
    ["7", "8", "9", "/", "("],
    ["4", "5", "6", "*", ")"],
    ["1", "2", "3", "-", "^"],
    ["0", ".", "%", "+", "="],
];

struct Calc {
    input: Input,
    result: Option<f64>,
    error: Option<String>,
    history: Vec<(String, String)>,
    hist: ListState,
    browsing: bool,
    ans: f64,
    pressed: Option<(String, Instant)>,
    focused: bool,
}

impl Calc {
    fn submit(&mut self) {
        let expr = self.input.text.trim().to_string();
        if expr.is_empty() {
            return;
        }
        match eval(&expr, self.ans) {
            Ok(v) => {
                self.ans = v;
                self.result = Some(v);
                self.error = None;
                self.history.push((expr, fmt(v)));
                self.hist.selected = self.history.len() - 1;
                self.input.clear();
            }
            Err(e) => self.error = Some(e),
        }
    }

    fn key(&mut self, k: &KeyEvent, ctl: &mut Control) {
        if k.is_ctrl('q') || (k.key == Key::Esc && self.input.text.is_empty() && !self.browsing) {
            return ctl.quit();
        }
        if k.is_ctrl('c') {
            // Copy the result (or the expression, if there's none yet).
            let text = match self.result {
                Some(v) if self.input.text.is_empty() => fmt(v),
                _ => self.input.text.clone(),
            };
            inkui::clipboard::set(&text);
            self.pressed = Some(("copied".into(), Instant::now()));
            return;
        }
        if let Some(c) = k.text() {
            self.pressed = Some((c.to_string(), Instant::now()));
        } else if k.key == Key::Enter {
            self.pressed = Some(("=".into(), Instant::now()));
        }
        match k.key {
            Key::Up => {
                if !self.history.is_empty() {
                    if self.browsing {
                        self.hist.prev();
                    } else {
                        self.browsing = true;
                        self.hist.selected = self.history.len() - 1;
                    }
                    let e = self.history[self.hist.selected].0.clone();
                    self.input.set(&e);
                }
                return;
            }
            Key::Down if self.browsing => {
                if self.hist.selected + 1 >= self.history.len() {
                    self.browsing = false;
                    self.input.clear();
                } else {
                    self.hist.next(self.history.len());
                    let e = self.history[self.hist.selected].0.clone();
                    self.input.set(&e);
                }
                return;
            }
            _ => {}
        }
        // An operator first continues from the last result.
        if self.input.text.is_empty() && self.result.is_some() {
            if let Some(c) = k.text() {
                if "+-*/%^".contains(c) {
                    self.input.set("ans");
                }
            }
        }
        if k.text() == Some('=') {
            return self.submit();
        }
        match self.input.handle(k) {
            InputResult::Submit => {
                self.browsing = false;
                self.submit();
            }
            InputResult::Cancel => {
                self.browsing = false;
                self.input.clear();
                self.error = None;
            }
            InputResult::Changed => {
                self.browsing = false;
                self.error = None;
            }
            _ => {}
        }
    }
}

impl App for Calc {
    fn draw(&mut self, buf: &mut Buffer) {
        let area = buf.area;
        let (header, rest) = area.take_top(1);
        let (body, hint_row) = rest.take_bottom(1);
        widget::pill(buf, header.x + 1, header.y, &format!("{} calc", icons::CALCULATOR), p::CRUST, p::PEACH);

        let cols = body.split_h(&[Constraint::Fill(3), Constraint::Fill(2)]);
        let left = cols[0].split_v(&[Constraint::Len(9), Constraint::Fill(1)]);

        let disp = Block::new().focused(self.focused).padding(2, 0).render(buf, left[0]);
        let preview = if self.input.text.trim().is_empty() { None } else { eval(&self.input.text, self.ans).ok() };
        let shown = preview.or(self.result).map(fmt).unwrap_or_else(|| "0".into());
        let iw = disp.w.min(self.input.len() + 1);
        let irect = Rect::new(disp.right() - iw, disp.y, iw, 1);
        self.input.render(buf, irect, Style::new().fg(p::SUBTEXT1), self.focused, "");
        if self.input.text.is_empty() {
            buf.text(disp.x, disp.y, "type an expression", disp.w, Style::new().fg(p::OVERLAY0));
        }
        let big_y = disp.y + 2;
        if !big_text(buf, disp.right(), big_y, disp.w, &shown) {
            let w = shown.chars().count().min(disp.w);
            buf.text(disp.right() - w, big_y + 1, &shown, w, Style::new().fg(p::SAPPHIRE).bold());
        }
        if preview.is_some() && self.input.text.trim() != shown {
            buf.text(disp.x, big_y + 1, "= preview", disp.w, Style::new().fg(p::OVERLAY0));
        }
        if let Some(e) = &self.error {
            buf.text(disp.x, disp.bottom() - 1, &format!("{} {}", icons::WARNING, e), disp.w, Style::new().fg(p::RED));
        }

        let lit = self.pressed.as_ref().filter(|(_, t)| t.elapsed().as_millis() < 180).map(|(k, _)| k.clone());
        let pad = Block::new().focused(false).padding(1, 0).render(buf, left[1]);
        let kw = (pad.w / 5).max(3);
        let kh = (pad.h / 4).max(1);
        for (r, row) in PAD.iter().enumerate() {
            for (c, key) in row.iter().enumerate() {
                let x = pad.x + c * kw;
                let y = pad.y + r * kh;
                if y + kh > pad.bottom() {
                    continue;
                }
                let is_op = !key.chars().all(|ch| ch.is_ascii_digit() || ch == '.');
                let base: Color = if *key == "=" { p::GREEN } else if is_op { p::MAUVE } else { p::SURFACE1 };
                let on = lit.as_deref() == Some(*key);
                let bg = if on { p::YELLOW } else if is_op || *key == "=" { with_alpha(base, 0x50) } else { with_alpha(base, 0xA0) };
                let gap = if kh >= 3 { 1 } else { 0 };
                let cell = Rect::new(x, y, kw.saturating_sub(1), kh - gap);
                buf.fill(cell, ' ', Style::new().bg(bg));
                let fg = if on { p::CRUST } else if is_op || *key == "=" { base } else { p::TEXT };
                buf.text(x + cell.w / 2, y + kh / 2, key, 1, Style::new().fg(fg).bg(bg).bold());
            }
        }

        let inner = Block::new()
            .title(Line::styled(format!("{} history", icons::HISTORY), Style::new().fg(p::LAVENDER)))
            .focused(false)
            .render(buf, cols[1]);
        let items: Vec<Line> = self
            .history
            .iter()
            .map(|(e, r)| {
                let room = inner.w.saturating_sub(r.chars().count() + 4);
                let e: String = e.chars().take(room).collect();
                let pad = inner.w.saturating_sub(e.chars().count() + r.chars().count() + 4);
                Line::new()
                    .push(e, Style::new().fg(p::SUBTEXT0))
                    .push(" ".repeat(pad), Style::new())
                    .push(" = ", Style::new().fg(p::OVERLAY0))
                    .push(r.clone(), Style::new().fg(p::GREEN).bold())
            })
            .collect();
        if items.is_empty() {
            buf.text(inner.x, inner.y, "nothing yet", inner.w, Style::new().fg(p::OVERLAY0));
        }
        List::new(&items).focused(self.browsing).render(buf, inner, &mut self.hist);

        widget::hints(
            buf,
            hint_row.inset(1, 0),
            &[("Enter", "calculate"), ("\u{2191}\u{2193}", "history"), ("Esc", "clear"), ("Ctrl + C", "copy"), ("Ctrl + V", "paste"), ("Ctrl + Q", "quit")],
        );
    }

    fn event(&mut self, ev: &Event, ctl: &mut Control) {
        match ev {
            Event::Key(k) => self.key(k, ctl),
            Event::Focus(f) => self.focused = *f,
            Event::Resize(..) => {}
        }
    }

    fn tick(&mut self, ctl: &mut Control) {
        if let Some((_, t)) = &self.pressed {
            if t.elapsed().as_millis() >= 180 {
                self.pressed = None;
                ctl.redraw();
            }
        }
    }

    fn tick_ms(&self) -> u64 {
        if self.pressed.is_some() { 60 } else { 0 }
    }
}

fn main() {
    inkui::run(Calc {
        input: Input::new(),
        result: None,
        error: None,
        history: Vec::new(),
        hist: ListState::default(),
        browsing: false,
        ans: 0.0,
        pressed: None,
        focused: true,
    });
}
