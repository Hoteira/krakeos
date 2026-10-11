// Widgets: blocks, paragraphs, lists, line input, text editor, gauges, key hints.

use crate::buffer::{Buffer, Line};
use crate::event::{Key, KeyEvent};
use crate::layout::Rect;
use crate::types::{mix, palette as p, with_alpha, Color, Style};

#[derive(Clone, Copy, PartialEq)]
pub enum Border {
    None,
    Plain,
    Rounded,
    Thick,
    Double,
}

impl Border {
    // tl, tr, bl, br, horizontal, vertical
    fn chars(self) -> [char; 6] {
        match self {
            Border::None => [' '; 6],
            Border::Plain => ['┌', '┐', '└', '┘', '─', '│'],
            Border::Rounded => ['╭', '╮', '╰', '╯', '─', '│'],
            Border::Thick => ['┏', '┓', '┗', '┛', '━', '┃'],
            Border::Double => ['╔', '╗', '╚', '╝', '═', '║'],
        }
    }
}

pub struct Block {
    title: Option<Line>,
    title_right: Option<Line>,
    footer: Option<Line>,
    border: Border,
    colors: (Color, Color),
    bg: Option<Color>,
    pad: (usize, usize),
}

impl Default for Block {
    fn default() -> Self {
        Block::new()
    }
}

impl Block {
    pub fn new() -> Block {
        Block {
            title: None,
            title_right: None,
            footer: None,
            border: Border::Rounded,
            colors: (p::SURFACE2, p::SURFACE2),
            bg: None,
            pad: (1, 0),
        }
    }
    pub fn title(mut self, t: impl Into<Line>) -> Block {
        self.title = Some(t.into());
        self
    }
    pub fn title_right(mut self, t: impl Into<Line>) -> Block {
        self.title_right = Some(t.into());
        self
    }
    pub fn footer(mut self, t: impl Into<Line>) -> Block {
        self.footer = Some(t.into());
        self
    }
    pub fn border(mut self, b: Border) -> Block {
        self.border = b;
        self
    }
    pub fn color(mut self, c: Color) -> Block {
        self.colors = (c, c);
        self
    }
    // A border that fades from a (top left) to b (bottom right).
    pub fn gradient(mut self, a: Color, b: Color) -> Block {
        self.colors = (a, b);
        self
    }
    pub fn focused(self, on: bool) -> Block {
        if on {
            self.gradient(p::ACCENT, p::ACCENT2)
        } else {
            self.color(p::SURFACE1)
        }
    }
    pub fn bg(mut self, c: Color) -> Block {
        self.bg = Some(c);
        self
    }
    // Inner padding (columns, rows).
    pub fn padding(mut self, x: usize, y: usize) -> Block {
        self.pad = (x, y);
        self
    }

    pub fn inner(&self, area: Rect) -> Rect {
        let b = if self.border == Border::None { 0 } else { 1 };
        area.inset(b + self.pad.0, b + self.pad.1)
    }

    fn color_at(&self, area: Rect, x: usize, y: usize) -> Color {
        if self.colors.0 == self.colors.1 {
            return self.colors.0;
        }
        let span = (area.w + area.h * 2).max(1);
        let t = ((x - area.x) + (y - area.y) * 2) * 255 / span;
        mix(self.colors.0, self.colors.1, t.min(255) as u8)
    }

    pub fn render(&self, buf: &mut Buffer, area: Rect) -> Rect {
        if area.w < 2 || area.h < 2 {
            return Rect::default();
        }
        if let Some(bg) = self.bg {
            buf.fill(area, ' ', Style::new().bg(bg));
        }
        if self.border != Border::None {
            let [tl, tr, bl, br, hz, vt] = self.border.chars();
            let (r, b) = (area.right() - 1, area.bottom() - 1);
            let bg = self.bg.unwrap_or(0);
            let put = |buf: &mut Buffer, x: usize, y: usize, ch: char| {
                let c = self.color_at(area, x, y);
                buf.set(x, y, ch, Style::new().fg(c).bg(bg));
            };
            put(buf, area.x, area.y, tl);
            put(buf, r, area.y, tr);
            put(buf, area.x, b, bl);
            put(buf, r, b, br);
            for x in area.x + 1..r {
                put(buf, x, area.y, hz);
                put(buf, x, b, hz);
            }
            for y in area.y + 1..b {
                put(buf, area.x, y, vt);
                put(buf, r, y, vt);
            }
            let room = area.w.saturating_sub(4);
            if let Some(t) = &self.title {
                let x = buf.text(area.x + 1, area.y, " ", 1, Style::new().bg(bg));
                let x = buf.line(x, area.y, t, room.saturating_sub(2));
                buf.text(x, area.y, " ", 1, Style::new().bg(bg));
            }
            if let Some(t) = &self.title_right {
                let w = t.width().min(room);
                let x = r.saturating_sub(w + 2);
                buf.text(x, area.y, " ", 1, Style::new().bg(bg));
                let x = buf.line(x + 1, area.y, t, w);
                buf.text(x, area.y, " ", 1, Style::new().bg(bg));
            }
            if let Some(t) = &self.footer {
                let w = t.width().min(room);
                let x = r.saturating_sub(w + 2);
                buf.text(x, b, " ", 1, Style::new().bg(bg));
                let x = buf.line(x + 1, b, t, w);
                buf.text(x, b, " ", 1, Style::new().bg(bg));
            }
        }
        self.inner(area)
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Align {
    Left,
    Center,
    Right,
}

pub struct Paragraph<'a> {
    lines: &'a [Line],
    wrap: bool,
    scroll: usize,
    align: Align,
}

impl<'a> Paragraph<'a> {
    pub fn new(lines: &'a [Line]) -> Paragraph<'a> {
        Paragraph { lines, wrap: false, scroll: 0, align: Align::Left }
    }
    pub fn wrap(mut self, on: bool) -> Self {
        self.wrap = on;
        self
    }
    pub fn scroll(mut self, rows: usize) -> Self {
        self.scroll = rows;
        self
    }
    pub fn align(mut self, a: Align) -> Self {
        self.align = a;
        self
    }

    pub fn render(&self, buf: &mut Buffer, area: Rect) {
        let mut rows: Vec<Line> = Vec::new();
        for l in self.lines {
            if self.wrap {
                rows.extend(wrap_line(l, area.w));
            } else {
                rows.push(l.clone());
            }
            if rows.len() > self.scroll + area.h {
                break;
            }
        }
        for (i, l) in rows.iter().skip(self.scroll).take(area.h).enumerate() {
            let w = l.width().min(area.w);
            let x = match self.align {
                Align::Left => area.x,
                Align::Center => area.x + (area.w - w) / 2,
                Align::Right => area.right() - w,
            };
            buf.line(x, area.y + i, l, area.w);
        }
    }
}

// Break a line into rows of at most width cells, at spaces when possible.
pub fn wrap_line(line: &Line, width: usize) -> Vec<Line> {
    if width == 0 {
        return vec![];
    }
    let cells: Vec<(char, Style)> = line
        .spans
        .iter()
        .flat_map(|s| s.text.chars().map(move |c| (c, s.style)))
        .collect();
    if cells.len() <= width {
        return vec![line.clone()];
    }
    let mut out = Vec::new();
    let mut start = 0;
    while start < cells.len() {
        let mut end = (start + width).min(cells.len());
        if end < cells.len() {
            if let Some(sp) = (start + 1..end).rev().find(|&i| cells[i].0 == ' ') {
                end = sp + 1;
            }
        }
        let mut l = Line::new();
        for &(c, st) in &cells[start..end] {
            match l.spans.last_mut() {
                Some(s) if s.style == st => s.text.push(c),
                _ => l.add(c.to_string(), st),
            }
        }
        out.push(l);
        start = end;
    }
    out
}

#[derive(Default, Clone, Copy)]
pub struct ListState {
    pub selected: usize,
    pub offset: usize,
}

impl ListState {
    pub fn next(&mut self, len: usize) {
        if len > 0 {
            self.selected = (self.selected + 1).min(len - 1);
        }
    }
    pub fn prev(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }
    pub fn page_down(&mut self, len: usize, page: usize) {
        if len > 0 {
            self.selected = (self.selected + page.max(1)).min(len - 1);
        }
    }
    pub fn page_up(&mut self, page: usize) {
        self.selected = self.selected.saturating_sub(page.max(1));
    }
    pub fn clamp(&mut self, len: usize) {
        if len == 0 {
            self.selected = 0;
        } else if self.selected >= len {
            self.selected = len - 1;
        }
    }
    // Scroll so the selection is visible in height rows.
    pub fn follow(&mut self, height: usize) {
        if height == 0 {
            return;
        }
        if self.selected < self.offset {
            self.offset = self.selected;
        } else if self.selected >= self.offset + height {
            self.offset = self.selected + 1 - height;
        }
    }
}

pub struct List<'a> {
    items: &'a [Line],
    highlight: Color,
    symbol: &'a str,
    focused: bool,
}

impl<'a> List<'a> {
    pub fn new(items: &'a [Line]) -> List<'a> {
        List { items, highlight: with_alpha(p::SURFACE1, 0xD0), symbol: "▌", focused: true }
    }
    pub fn highlight(mut self, bg: Color) -> Self {
        self.highlight = bg;
        self
    }
    pub fn symbol(mut self, s: &'a str) -> Self {
        self.symbol = s;
        self
    }
    pub fn focused(mut self, on: bool) -> Self {
        self.focused = on;
        self
    }

    pub fn render(&self, buf: &mut Buffer, area: Rect, state: &mut ListState) {
        state.clamp(self.items.len());
        state.follow(area.h);
        let sym_w = self.symbol.chars().count();
        for (i, item) in self.items.iter().enumerate().skip(state.offset).take(area.h) {
            let y = area.y + i - state.offset;
            let selected = i == state.selected;
            if selected {
                let bg = if self.focused { self.highlight } else { with_alpha(p::SURFACE0, 0xA0) };
                buf.fill(Rect::new(area.x, y, area.w, 1), ' ', Style::new().bg(bg));
                if self.focused {
                    buf.text(area.x, y, self.symbol, sym_w, Style::new().fg(p::ACCENT).bg(bg));
                }
                let x = buf.line(area.x + sym_w, y, item, area.w.saturating_sub(sym_w));
                buf.tint(Rect::new(area.x + sym_w, y, x - area.x - sym_w, 1), bg);
            } else {
                buf.line(area.x + sym_w, y, item, area.w.saturating_sub(sym_w));
            }
        }
        // Scroll bar.
        if self.items.len() > area.h && area.w > 2 {
            let x = area.right() - 1;
            let total = self.items.len();
            let thumb = (area.h * area.h / total).max(1);
            let top = state.offset * (area.h - thumb) / (total - area.h).max(1);
            for y in 0..area.h {
                let on = y >= top && y < top + thumb;
                let c = if on { p::OVERLAY0 } else { p::SURFACE0 };
                buf.set(x, area.y + y, if on { '┃' } else { '│' }, Style::new().fg(c));
            }
        }
    }
}

#[derive(PartialEq, Debug)]
pub enum InputResult {
    Ignored,
    Changed,
    Moved,
    Submit,
    Cancel,
}

// A single-line text field.
#[derive(Default, Clone)]
pub struct Input {
    pub text: String,
    // Cursor, in characters.
    pub cursor: usize,
    offset: usize,
}

impl Input {
    pub fn new() -> Input {
        Input::default()
    }

    pub fn set(&mut self, s: &str) {
        self.text = s.to_string();
        self.cursor = self.len();
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
        self.offset = 0;
    }

    pub fn len(&self) -> usize {
        self.text.chars().count()
    }

    fn byte(&self, ch: usize) -> usize {
        self.text.char_indices().nth(ch).map(|(b, _)| b).unwrap_or(self.text.len())
    }

    pub fn insert(&mut self, c: char) {
        let b = self.byte(self.cursor);
        self.text.insert(b, c);
        self.cursor += 1;
    }

    pub fn insert_str(&mut self, s: &str) {
        for c in s.chars() {
            self.insert(c);
        }
    }

    pub fn handle(&mut self, k: &KeyEvent) -> InputResult {
        if let Some(c) = k.text() {
            self.insert(c);
            return InputResult::Changed;
        }
        match k.key {
            Key::Enter => InputResult::Submit,
            Key::Esc => InputResult::Cancel,
            Key::Backspace if k.ctrl() || k.alt() => {
                // Delete the word before the cursor.
                let chars: Vec<char> = self.text.chars().collect();
                let mut i = self.cursor;
                while i > 0 && chars[i - 1] == ' ' {
                    i -= 1;
                }
                while i > 0 && chars[i - 1] != ' ' && chars[i - 1] != '/' {
                    i -= 1;
                }
                let (a, b) = (self.byte(i), self.byte(self.cursor));
                self.text.replace_range(a..b, "");
                self.cursor = i;
                InputResult::Changed
            }
            Key::Backspace if self.cursor > 0 => {
                self.cursor -= 1;
                let b = self.byte(self.cursor);
                self.text.remove(b);
                InputResult::Changed
            }
            Key::Delete if self.cursor < self.len() => {
                let b = self.byte(self.cursor);
                self.text.remove(b);
                InputResult::Changed
            }
            Key::Char('u') if k.ctrl() => {
                let b = self.byte(self.cursor);
                self.text.replace_range(..b, "");
                self.cursor = 0;
                InputResult::Changed
            }
            Key::Char('v') if k.ctrl() => {
                // Paste the clipboard's first line.
                let clip = crate::clipboard::get();
                let line = clip.lines().next().unwrap_or("").replace('\t', " ");
                for c in line.chars() {
                    self.insert(c);
                }
                InputResult::Changed
            }
            Key::Char('a') if k.ctrl() => {
                self.cursor = 0;
                InputResult::Moved
            }
            Key::Char('e') if k.ctrl() => {
                self.cursor = self.len();
                InputResult::Moved
            }
            Key::Left => {
                self.cursor = self.cursor.saturating_sub(1);
                InputResult::Moved
            }
            Key::Right => {
                self.cursor = (self.cursor + 1).min(self.len());
                InputResult::Moved
            }
            Key::Home => {
                self.cursor = 0;
                InputResult::Moved
            }
            Key::End => {
                self.cursor = self.len();
                InputResult::Moved
            }
            _ => InputResult::Ignored,
        }
    }

    // Draw the text (or a dim placeholder) with a block cursor if focused.
    pub fn render(&mut self, buf: &mut Buffer, area: Rect, style: Style, focused: bool, placeholder: &str) {
        if area.w == 0 {
            return;
        }
        if self.cursor < self.offset {
            self.offset = self.cursor;
        } else if self.cursor >= self.offset + area.w {
            self.offset = self.cursor + 1 - area.w;
        }
        if self.text.is_empty() && !placeholder.is_empty() {
            buf.text(area.x, area.y, placeholder, area.w, Style::new().fg(p::OVERLAY0).bg(style.bg));
        } else {
            let visible: String = self.text.chars().skip(self.offset).take(area.w).collect();
            buf.text(area.x, area.y, &visible, area.w, style);
        }
        if focused {
            let x = area.x + self.cursor - self.offset;
            let ch = buf.get(x, area.y).map(|c| c.ch).unwrap_or(' ');
            buf.set(x, area.y, ch, Style::new().fg(p::BASE).bg(p::ROSEWATER));
        }
    }
}

// A multi-line editor: cursor movement, typing, line cut/paste.
pub struct TextArea {
    pub lines: Vec<String>,
    pub row: usize,
    pub col: usize,
    // Column the cursor wants when moving vertically through short lines.
    want_col: usize,
    pub top: usize,
    pub left: usize,
    pub modified: bool,
    // Lines collected by consecutive Ctrl+K cuts.
    cut_lines: Vec<String>,
    last_was_cut: bool,
    // Spaces per Tab stop.
    pub tab_width: usize,
    // Visible rows, from the last render (for paging).
    page: usize,
}

impl TextArea {
    pub fn new(text: &str) -> TextArea {
        let mut lines: Vec<String> = text.split('\n').map(|l| l.trim_end_matches('\r').replace('\t', "    ")).collect();
        if lines.is_empty() {
            lines.push(String::new());
        }
        TextArea {
            lines,
            row: 0,
            col: 0,
            want_col: 0,
            top: 0,
            left: 0,
            modified: false,
            cut_lines: Vec::new(),
            last_was_cut: false,
            tab_width: 4,
            page: 20,
        }
    }

    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    fn line_len(&self, r: usize) -> usize {
        self.lines[r].chars().count()
    }

    fn byte(&self, r: usize, c: usize) -> usize {
        let l = &self.lines[r];
        l.char_indices().nth(c).map(|(b, _)| b).unwrap_or(l.len())
    }

    pub fn goto(&mut self, row: usize, col: usize) {
        self.row = row.min(self.lines.len() - 1);
        self.col = col.min(self.line_len(self.row));
        self.want_col = self.col;
    }

    fn vmove(&mut self, row: usize) {
        self.row = row.min(self.lines.len() - 1);
        self.col = self.want_col.min(self.line_len(self.row));
    }

    // Find needle after the cursor (wrapping). Returns true if found.
    pub fn find(&mut self, needle: &str) -> bool {
        if needle.is_empty() {
            return false;
        }
        let n = self.lines.len();
        for i in 0..=n {
            let r = (self.row + i) % n;
            let line = &self.lines[r];
            let from = if i == 0 { self.byte(r, self.col + 1).min(line.len()) } else { 0 };
            if let Some(b) = line[from..].find(needle) {
                let c = line[..from + b].chars().count();
                self.goto(r, c);
                return true;
            }
        }
        false
    }

    // Insert text: whole lines (ending in a newline, as cut and copy
    // make them) go above the cursor line; anything else at the cursor.
    pub fn paste(&mut self, text: &str) {
        let text = text.replace("\r\n", "\n").replace('\t', &" ".repeat(self.tab_width.max(1)));
        if text.is_empty() {
            return;
        }
        if let Some(body) = text.strip_suffix('\n') {
            let lines: Vec<String> = body.split('\n').map(String::from).collect();
            let n = lines.len();
            for (i, l) in lines.into_iter().enumerate() {
                self.lines.insert(self.row + i, l);
            }
            self.row = (self.row + n).min(self.lines.len() - 1);
            self.col = 0;
        } else {
            let b = self.byte(self.row, self.col);
            let rest = self.lines[self.row].split_off(b);
            let mut parts = text.split('\n');
            self.lines[self.row].push_str(parts.next().unwrap_or(""));
            for part in parts {
                self.row += 1;
                self.lines.insert(self.row, part.to_string());
            }
            self.col = self.line_len(self.row);
            self.lines[self.row].push_str(&rest);
        }
        self.want_col = self.col;
        self.modified = true;
    }

    // Handle an editing key. Returns true if it was used.
    pub fn handle(&mut self, k: &KeyEvent) -> bool {
        let cut = k.is_ctrl('k') || k.is_ctrl('x');
        if !cut {
            self.last_was_cut = false;
        }
        if let Some(c) = k.text() {
            let b = self.byte(self.row, self.col);
            self.lines[self.row].insert(b, c);
            self.col += 1;
            self.want_col = self.col;
            self.modified = true;
            return true;
        }
        match k.key {
            Key::Enter => {
                let b = self.byte(self.row, self.col);
                let rest = self.lines[self.row].split_off(b);
                let indent: String = self.lines[self.row].chars().take_while(|c| *c == ' ').collect();
                self.row += 1;
                self.col = indent.chars().count();
                self.lines.insert(self.row, indent + &rest);
                self.want_col = self.col;
                self.modified = true;
            }
            Key::Tab => {
                let tw = self.tab_width.max(1);
                let n = tw - self.col % tw;
                let b = self.byte(self.row, self.col);
                self.lines[self.row].insert_str(b, &" ".repeat(n));
                self.col += n;
                self.want_col = self.col;
                self.modified = true;
            }
            Key::Backspace => {
                if self.col > 0 {
                    self.col -= 1;
                    let b = self.byte(self.row, self.col);
                    self.lines[self.row].remove(b);
                } else if self.row > 0 {
                    let cur = self.lines.remove(self.row);
                    self.row -= 1;
                    self.col = self.line_len(self.row);
                    self.lines[self.row].push_str(&cur);
                } else {
                    return true;
                }
                self.want_col = self.col;
                self.modified = true;
            }
            Key::Delete => {
                if self.col < self.line_len(self.row) {
                    let b = self.byte(self.row, self.col);
                    self.lines[self.row].remove(b);
                } else if self.row + 1 < self.lines.len() {
                    let next = self.lines.remove(self.row + 1);
                    self.lines[self.row].push_str(&next);
                } else {
                    return true;
                }
                self.modified = true;
            }
            Key::Char('k') | Key::Char('x') if k.ctrl() => {
                // nano: cut the line; consecutive cuts collect lines. They
                // go to the shared clipboard as whole lines.
                if !self.last_was_cut {
                    self.cut_lines.clear();
                }
                self.last_was_cut = true;
                if self.lines.len() > 1 {
                    self.cut_lines.push(self.lines.remove(self.row));
                    if self.row >= self.lines.len() {
                        self.row = self.lines.len() - 1;
                    }
                } else {
                    self.cut_lines.push(std::mem::take(&mut self.lines[0]));
                }
                crate::clipboard::set(&(self.cut_lines.join("\n") + "\n"));
                self.col = 0;
                self.want_col = 0;
                self.modified = true;
            }
            Key::Char('c') if k.ctrl() => {
                crate::clipboard::set(&(self.lines[self.row].clone() + "\n"));
            }
            Key::Char('u') | Key::Char('v') if k.ctrl() => self.paste(&crate::clipboard::get()),
            Key::Left => {
                if self.col > 0 {
                    self.col -= 1;
                } else if self.row > 0 {
                    self.row -= 1;
                    self.col = self.line_len(self.row);
                }
                self.want_col = self.col;
            }
            Key::Right => {
                if self.col < self.line_len(self.row) {
                    self.col += 1;
                } else if self.row + 1 < self.lines.len() {
                    self.row += 1;
                    self.col = 0;
                }
                self.want_col = self.col;
            }
            Key::Up => self.vmove(self.row.saturating_sub(1)),
            Key::Down => self.vmove(self.row + 1),
            Key::PageUp => self.vmove(self.row.saturating_sub(self.page)),
            Key::PageDown => self.vmove(self.row + self.page),
            Key::Home if k.ctrl() => self.goto(0, 0),
            Key::End if k.ctrl() => self.goto(usize::MAX, usize::MAX),
            Key::Home => {
                // Toggle between the first non-blank and column 0.
                let ind = self.lines[self.row].chars().take_while(|c| *c == ' ').count();
                self.col = if self.col == ind { 0 } else { ind };
                self.want_col = self.col;
            }
            Key::End => {
                self.col = self.line_len(self.row);
                self.want_col = self.col;
            }
            _ => return false,
        }
        true
    }

    // Draw with a line-number gutter. color picks a style per character
    // (for syntax highlighting); None uses plain text.
    pub fn render(
        &mut self,
        buf: &mut Buffer,
        area: Rect,
        focused: bool,
        color: Option<&dyn Fn(&str) -> Vec<Style>>,
    ) {
        let digits = self.lines.len().to_string().len().max(3);
        let gutter = digits + 2;
        if area.w <= gutter || area.h == 0 {
            return;
        }
        let tw = area.w - gutter;
        self.page = area.h.saturating_sub(1).max(1);
        if self.row < self.top {
            self.top = self.row;
        } else if self.row >= self.top + area.h {
            self.top = self.row + 1 - area.h;
        }
        if self.col < self.left {
            self.left = self.col;
        } else if self.col >= self.left + tw {
            self.left = self.col + 1 - tw;
        }
        for i in 0..area.h {
            let r = self.top + i;
            let y = area.y + i;
            if r >= self.lines.len() {
                buf.text(area.x + digits - 1, y, "~", 1, Style::new().fg(p::SURFACE1));
                continue;
            }
            let current = r == self.row;
            if current && focused {
                buf.fill(Rect::new(area.x, y, area.w, 1), ' ', Style::new().bg(with_alpha(p::SURFACE0, 0x90)));
            }
            let num = format!("{:>w$} ", r + 1, w = digits);
            let nst = if current { Style::new().fg(p::LAVENDER).bold() } else { Style::new().fg(p::SURFACE2) };
            let bg = buf.get(area.x, y).map(|c| c.bg).unwrap_or(0);
            buf.text(area.x, y, &num, gutter, nst.bg(bg));
            buf.set(area.x + digits + 1, y, '│', Style::new().fg(p::SURFACE1).bg(bg));
            let line = &self.lines[r];
            let styles = color.map(|f| f(line));
            for (j, ch) in line.chars().enumerate().skip(self.left).take(tw) {
                let mut st = styles.as_ref().and_then(|s| s.get(j).copied()).unwrap_or_default();
                st.bg = bg;
                buf.set(area.x + gutter + j - self.left, y, ch, st);
            }
        }
        if focused {
            let (x, y) = (area.x + gutter + self.col - self.left, area.y + self.row - self.top);
            let ch = buf.get(x, y).map(|c| c.ch).unwrap_or(' ');
            buf.set(x, y, ch, Style::new().fg(p::BASE).bg(p::ROSEWATER));
        }
    }
}

// A horizontal bar filled to ratio with eighth-cell precision.
pub fn gauge(buf: &mut Buffer, area: Rect, ratio: f32, fill: Color, label: &str) {
    if area.w == 0 || area.h == 0 {
        return;
    }
    let track = with_alpha(p::SURFACE0, 0xC0);
    let eighths = (ratio.clamp(0.0, 1.0) * (area.w * 8) as f32).round() as usize;
    const PART: [char; 8] = [' ', '▏', '▎', '▍', '▌', '▋', '▊', '▉'];
    for x in 0..area.w {
        let n = eighths.saturating_sub(x * 8).min(8);
        let (ch, st) = match n {
            8 => (' ', Style::new().bg(fill)),
            0 => (' ', Style::new().bg(track)),
            n => (PART[n], Style::new().fg(fill).bg(track)),
        };
        for y in area.y..area.bottom() {
            buf.set(area.x + x, y, ch, st);
        }
    }
    let lw = label.chars().count();
    if lw > 0 && lw <= area.w {
        let lx = area.x + (area.w - lw) / 2;
        let ly = area.y + area.h / 2;
        for (i, ch) in label.chars().enumerate() {
            let x = lx + i;
            let filled = (x - area.x) * 8 < eighths;
            let st = if filled {
                Style::new().fg(p::CRUST).bg(fill).bold()
            } else {
                Style::new().fg(p::TEXT).bg(track).bold()
            };
            buf.set(x, ly, ch, st);
        }
    }
}

// A row of key hints: each key drawn as a colored cap, then its label.
pub fn hints(buf: &mut Buffer, area: Rect, items: &[(&str, &str)]) {
    let mut x = area.x;
    for (key, label) in items {
        let need = key.chars().count() + label.chars().count() + 4;
        if x + need > area.right() {
            break;
        }
        let cap = Style::new().fg(p::CRUST).bg(p::LAVENDER).bold();
        x = buf.text(x, area.y, "\u{e0b6}", 1, Style::new().fg(p::LAVENDER));
        x = buf.text(x, area.y, key, area.right() - x, cap);
        x = buf.text(x, area.y, "\u{e0b4}", 1, Style::new().fg(p::LAVENDER));
        x = buf.text(x + 1, area.y, label, area.right() - x, Style::new().fg(p::SUBTEXT0));
        x += 2;
    }
}

// A powerline-style segment: text on a colored pill. Returns the x after.
pub fn pill(buf: &mut Buffer, x: usize, y: usize, text: &str, fg: Color, bg: Color) -> usize {
    let x = buf.text(x, y, "\u{e0b6}", 1, Style::new().fg(bg));
    let x = buf.text(x, y, text, usize::MAX, Style::new().fg(fg).bg(bg).bold());
    buf.text(x, y, "\u{e0b4}", 1, Style::new().fg(bg))
}

pub fn human_size(n: u64) -> String {
    const U: [&str; 5] = ["B", "K", "M", "G", "T"];
    let mut v = n as f64;
    let mut u = 0;
    while v >= 1024.0 && u < U.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{}{}", n, U[0])
    } else {
        format!("{:.1}{}", v, U[u])
    }
}

// A centered popup box: clears the area under it and draws a block.
pub fn popup(buf: &mut Buffer, outer: Rect, w: usize, h: usize, title: &str) -> Rect {
    let r = outer.center(w, h);
    buf.fill(r, ' ', Style::new().bg(with_alpha(p::MANTLE, 0xF0)));
    Block::new()
        .title(Line::styled(title, Style::new().fg(p::ACCENT).bold()))
        .gradient(p::ACCENT, p::ACCENT2)
        .bg(with_alpha(p::MANTLE, 0xF0))
        .render(buf, r)
}
