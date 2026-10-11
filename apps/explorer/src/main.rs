// Ranger-style file explorer: parent, current and preview columns.

use std::fs::File;
use std::io::Read;

use inkui::buffer::{Buffer, Line};
use inkui::event::{Event, Key, KeyEvent};
use inkui::icons::{self, Entry};
use inkui::layout::{Constraint, Rect};
use inkui::types::{palette as p, with_alpha, Style};
use inkui::widget::{self, Block, Input, InputResult, List, ListState};
use inkui::{files, App, Control};

const PREVIEW_BYTES: u64 = 8 * 1024;

#[derive(Clone, PartialEq)]
enum Ask {
    NewFile,
    NewFolder,
    Rename(String),
    Delete(String),
}

enum Preview {
    Empty,
    Dir(Vec<Entry>),
    Text(Vec<String>),
    Info(Vec<Line>),
}

struct Explorer {
    cwd: String,
    entries: Vec<Entry>,
    state: ListState,
    parent: Vec<Entry>,
    parent_state: ListState,
    preview: Preview,
    preview_for: String,
    // The question on the status line, if any, and its answer so far.
    asking: Option<(Ask, Input)>,
    // A path cut with x: pasting moves it while the clipboard still holds it.
    cut: Option<String>,
    msg: Option<(String, bool)>,
    focused: bool,
}

impl Explorer {
    fn new(dir: &str) -> Explorer {
        let mut e = Explorer {
            cwd: String::new(),
            entries: Vec::new(),
            state: ListState::default(),
            parent: Vec::new(),
            parent_state: ListState::default(),
            preview: Preview::Empty,
            preview_for: String::new(),
            asking: None,
            cut: None,
            msg: None,
            focused: true,
        };
        if !e.cd(dir, None) {
            e.cd("/", None);
        }
        e
    }

    // Enters dir and selects select if given. False if dir can't be read.
    fn cd(&mut self, dir: &str, select: Option<&str>) -> bool {
        let Ok(entries) = icons::list_dir(dir) else { return false };
        self.cwd = dir.to_string();
        self.entries = entries;
        self.state = ListState::default();
        if let Some(name) = select {
            if let Some(i) = self.entries.iter().position(|e| e.name == name) {
                self.state.selected = i;
            }
        }
        if dir == "/" {
            self.parent.clear();
        } else {
            let up = icons::parent(dir);
            self.parent = icons::list_dir(&up).unwrap_or_default();
            let me = icons::file_name(dir);
            self.parent_state = ListState::default();
            self.parent_state.selected = self.parent.iter().position(|e| e.name == me).unwrap_or(0);
        }
        self.preview_for.clear();
        true
    }

    fn selected(&self) -> Option<&Entry> {
        self.entries.get(self.state.selected)
    }

    fn selected_path(&self) -> Option<String> {
        self.selected().map(|e| icons::resolve(&self.cwd, &e.name))
    }

    fn update_preview(&mut self) {
        let Some(path) = self.selected_path() else {
            self.preview = Preview::Empty;
            return;
        };
        if path == self.preview_for {
            return;
        }
        self.preview_for = path.clone();
        let e = self.selected().unwrap().clone();
        if e.is_dir {
            self.preview = Preview::Dir(icons::list_dir(&path).unwrap_or_default());
            return;
        }
        let mut data = Vec::new();
        if let Ok(f) = File::open(&path) {
            let _ = f.take(PREVIEW_BYTES).read_to_end(&mut data);
        }
        let label = |k: &str, v: String| Line::new().push(format!("{:<8}", k), Style::new().fg(p::BLUE).bold()).push(v, Style::new());
        if data.starts_with(b"\x89PNG") && data.len() >= 24 {
            let w = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
            let h = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
            self.preview = Preview::Info(vec![
                Line::styled(format!("{} PNG image", icons::PICTURE), Style::new().fg(p::MAUVE).bold()),
                Line::new(),
                label("size", format!("{} x {}", w, h)),
                label("file", widget::human_size(e.size)),
                Line::new(),
                Line::styled("Enter opens the viewer", Style::new().fg(p::OVERLAY1)),
            ]);
        } else if data.starts_with(b"\0asm") {
            self.preview = Preview::Info(vec![
                Line::styled(format!("{} wasm app", icons::CUBE), Style::new().fg(p::GREEN).bold()),
                Line::new(),
                label("file", widget::human_size(e.size)),
                Line::new(),
                Line::styled("Enter runs it on a new desktop", Style::new().fg(p::OVERLAY1)),
            ]);
        } else if data.iter().take(512).any(|&b| b == 0) {
            self.preview = Preview::Info(vec![
                Line::styled(format!("{} binary file", icons::FILE), Style::new().fg(p::SUBTEXT1).bold()),
                Line::new(),
                label("file", widget::human_size(e.size)),
            ]);
        } else {
            let text = String::from_utf8_lossy(&data);
            self.preview = Preview::Text(text.lines().take(200).map(|l| l.replace('\t', "    ")).collect());
        }
    }

    fn open(&mut self) {
        let Some(e) = self.selected().cloned() else { return };
        let path = icons::resolve(&self.cwd, &e.name);
        if e.is_dir {
            self.cd(&path, None);
        } else if path.ends_with(".wasm") {
            self.launch(&path, &[]);
        } else {
            self.launch(icons::opener(&path), &[&path]);
        }
    }

    fn launch(&mut self, app: &str, args: &[&str]) {
        if inkui::spawn(app, args) {
            self.msg = Some((format!("{} opened on a new desktop (Win + \u{2190} comes back)", icons::ROCKET), false));
        } else {
            self.msg = Some((format!("could not start {}", app), true));
        }
    }

    fn refresh(&mut self, select: Option<&str>) {
        let cwd = self.cwd.clone();
        let keep = self.state.selected;
        self.cd(&cwd, select);
        if select.is_none() {
            self.state.selected = keep.min(self.entries.len().saturating_sub(1));
        }
    }

    fn report(&mut self, r: std::io::Result<()>, done: String, what: &str) -> bool {
        match r {
            Ok(()) => {
                self.msg = Some((format!("{} {}", icons::CHECK, done), false));
                true
            }
            Err(e) => {
                self.msg = Some((format!("cannot {}: {}", what, files::why(&e)), true));
                false
            }
        }
    }

    fn answer(&mut self, ask: Ask, name: &str) {
        let path = icons::resolve(&self.cwd, name);
        let ok = match &ask {
            Ask::NewFile => {
                let r = if files::exists(&path) {
                    Err(std::io::Error::from_raw_os_error(20))
                } else {
                    File::create(&path).map(|_| ())
                };
                self.report(r, format!("created {}", name), &format!("create {}", name))
            }
            Ask::NewFolder => {
                let r = std::fs::create_dir(&path);
                self.report(r, format!("created folder {}", name), &format!("create {}", name))
            }
            Ask::Rename(old) => {
                let from = icons::resolve(&self.cwd, old);
                let r = if files::exists(&path) && from != path {
                    Err(std::io::Error::from_raw_os_error(20))
                } else {
                    std::fs::rename(&from, &path)
                };
                self.report(r, format!("renamed to {}", name), &format!("rename {}", old))
            }
            Ask::Delete(_) => false,
        };
        if ok {
            self.refresh(Some(icons::file_name(&path)));
        }
    }

    fn delete(&mut self, name: &str) {
        let path = icons::resolve(&self.cwd, name);
        let r = files::remove_all(&path);
        if self.report(r, format!("deleted {}", name), &format!("delete {}", name)) {
            self.refresh(None);
        }
    }

    // Puts the selected path on the clipboard; cut makes the next paste move it.
    fn copy(&mut self, cut: bool) {
        let Some(path) = self.selected_path() else { return };
        inkui::clipboard::set(&path);
        let name = icons::file_name(&path).to_string();
        self.cut = if cut { Some(path) } else { None };
        let verb = if cut { "cut" } else { "copied" };
        self.msg = Some((format!("{} {} {} (p pastes it)", icons::CHECK, verb, name), false));
    }

    // Pastes the path on the clipboard into this folder.
    fn paste(&mut self) {
        let src = inkui::clipboard::get().trim().to_string();
        if !src.starts_with('/') || !files::exists(&src) {
            self.msg = Some(("the clipboard has no file to paste (y copies one)".into(), true));
            return;
        }
        let name = icons::file_name(&src).to_string();
        let moving = self.cut.as_deref() == Some(src.as_str());
        let (r, dst) = if moving {
            let dst = icons::resolve(&self.cwd, &name);
            if dst == src {
                self.cut = None;
                return;
            }
            if files::exists(&dst) {
                (Err(std::io::Error::from_raw_os_error(20)), dst)
            } else {
                (std::fs::rename(&src, &dst), dst)
            }
        } else {
            let dst = files::free_name(&self.cwd, &name);
            (files::copy(&src, &dst), dst)
        };
        let verb = if moving { "moved" } else { "pasted" };
        if self.report(r, format!("{} {}", verb, icons::file_name(&dst)), &format!("paste {}", name)) {
            if moving {
                self.cut = None;
                inkui::clipboard::set(&dst);
            }
            self.refresh(Some(icons::file_name(&dst)));
        }
    }

    fn up(&mut self) {
        if self.cwd != "/" {
            let me = icons::file_name(&self.cwd).to_string();
            let up = icons::parent(&self.cwd);
            self.cd(&up, Some(&me));
        }
    }

    fn key(&mut self, k: &KeyEvent, ctl: &mut Control) {
        if let Some((ask, input)) = &mut self.asking {
            let ask = ask.clone();
            if let Ask::Delete(name) = &ask {
                match k.text().map(|c| c.to_ascii_lowercase()) {
                    Some('y') => {
                        self.asking = None;
                        self.delete(name);
                    }
                    Some('n') => self.asking = None,
                    _ if k.key == Key::Esc => self.asking = None,
                    _ => {}
                }
                return;
            }
            match input.handle(k) {
                InputResult::Submit => {
                    let name = input.text.trim().to_string();
                    self.asking = None;
                    if !name.is_empty() {
                        self.answer(ask, &name);
                    }
                }
                InputResult::Cancel => self.asking = None,
                _ => {}
            }
            return;
        }
        self.msg = None;
        let len = self.entries.len();
        match k.key {
            Key::Down | Key::Char('j') => self.state.next(len),
            Key::Up | Key::Char('k') => self.state.prev(),
            Key::PageDown => self.state.page_down(len, 10),
            Key::PageUp => self.state.page_up(10),
            Key::Home | Key::Char('g') => self.state.selected = 0,
            Key::End | Key::Char('G') => self.state.selected = len.saturating_sub(1),
            Key::Right | Key::Enter | Key::Char('l') => self.open(),
            Key::Left | Key::Backspace | Key::Char('h') => self.up(),
            Key::Char('e') => {
                if let Some(path) = self.selected_path().filter(|_| !self.selected().unwrap().is_dir) {
                    self.launch("/apps/editor.wasm", &[&path]);
                }
            }
            Key::Char('t') => {
                let cwd = self.cwd.clone();
                self.launch("/apps/terminal.wasm", &[&cwd]);
            }
            Key::Char('n') => self.asking = Some((Ask::NewFile, Input::new())),
            Key::Char('N') => self.asking = Some((Ask::NewFolder, Input::new())),
            Key::Char('R') | Key::F(2) => {
                if let Some(e) = self.selected() {
                    let mut input = Input::new();
                    input.set(&e.name);
                    self.asking = Some((Ask::Rename(e.name.clone()), input));
                }
            }
            Key::Char('d') | Key::Delete => {
                if let Some(e) = self.selected() {
                    self.asking = Some((Ask::Delete(e.name.clone()), Input::new()));
                }
            }
            Key::Char('y') => self.copy(false),
            Key::Char('c') if k.ctrl() => self.copy(false),
            Key::Char('x') => self.copy(true),
            Key::Char('p') => self.paste(),
            Key::Char('v') if k.ctrl() => self.paste(),
            Key::Char('r') => {
                let (cwd, sel) = (self.cwd.clone(), self.selected().map(|e| e.name.clone()));
                self.cd(&cwd, sel.as_deref());
            }
            Key::Char('q') | Key::Esc => ctl.quit(),
            _ => {}
        }
    }
}

fn entry_line(e: &Entry, with_size: bool, width: usize) -> Line {
    let (icon, color) = icons::file_icon(&e.name, e.is_dir);
    let name_style = if e.is_dir { Style::new().fg(color).bold() } else { Style::new().fg(p::TEXT) };
    let mut l = Line::new().push(format!("{} ", icon), Style::new().fg(color)).push(e.name.clone(), name_style);
    if with_size {
        let size = if e.is_dir { String::new() } else { widget::human_size(e.size) };
        let used = e.name.chars().count() + 2;
        let pad = width.saturating_sub(used + size.len() + 1);
        l.add(" ".repeat(pad), Style::new());
        l.add(size, Style::new().fg(p::OVERLAY1));
    }
    l
}

impl App for Explorer {
    fn draw(&mut self, buf: &mut Buffer) {
        self.update_preview();
        let area = buf.area;
        let (header, rest) = area.take_top(1);
        let (body, footer) = rest.take_bottom(2);

        let mut x = widget::pill(buf, header.x + 1, header.y, &format!("{} files", icons::FOLDER_OPEN), p::CRUST, p::BLUE) + 1;
        let crumbs: Vec<&str> = self.cwd.split('/').filter(|s| !s.is_empty()).collect();
        x = buf.text(x, header.y, " / ", 3, Style::new().fg(p::OVERLAY1));
        for (i, c) in crumbs.iter().enumerate() {
            let last = i + 1 == crumbs.len();
            let st = if last { Style::new().fg(p::BLUE).bold() } else { Style::new().fg(p::SUBTEXT0) };
            x = buf.text(x, header.y, c, header.right().saturating_sub(x), st);
            if !last {
                x = buf.text(x, header.y, " \u{f054} ", 3, Style::new().fg(p::SURFACE2));
            }
        }

        let cols = body.split_h(&[Constraint::Pct(20), Constraint::Pct(40), Constraint::Fill(1)]);

        let inner = Block::new().focused(false).padding(0, 0).render(buf, cols[0]);
        let items: Vec<Line> = self.parent.iter().map(|e| entry_line(e, false, inner.w)).collect();
        List::new(&items).focused(false).symbol(" ").render(buf, inner, &mut self.parent_state);

        let title = Line::styled(format!(" {}", if self.cwd == "/" { "/" } else { icons::file_name(&self.cwd) }), Style::new().fg(p::BLUE).bold());
        let count = Line::styled(format!("{} items", self.entries.len()), Style::new().fg(p::OVERLAY1));
        let inner = Block::new().title(title).footer(count).focused(self.focused).padding(0, 0).render(buf, cols[1]);
        let items: Vec<Line> = self.entries.iter().map(|e| entry_line(e, true, inner.w.saturating_sub(2))).collect();
        if items.is_empty() {
            buf.text(inner.x + 2, inner.y, "empty", inner.w, Style::new().fg(p::OVERLAY0));
        }
        List::new(&items).focused(self.focused).render(buf, inner, &mut self.state);

        let ptitle = self.selected().map(|e| e.name.clone()).unwrap_or_default();
        let inner = Block::new()
            .title(Line::styled(ptitle, Style::new().fg(p::LAVENDER)))
            .color(p::SURFACE1)
            .render(buf, cols[2]);
        match &self.preview {
            Preview::Empty => {}
            Preview::Dir(entries) => {
                if entries.is_empty() {
                    buf.text(inner.x, inner.y, "empty folder", inner.w, Style::new().fg(p::OVERLAY0));
                }
                for (i, e) in entries.iter().take(inner.h).enumerate() {
                    buf.line(inner.x, inner.y + i, &entry_line(e, true, inner.w), inner.w);
                }
            }
            Preview::Text(lines) => {
                for (i, l) in lines.iter().take(inner.h).enumerate() {
                    buf.text(inner.x, inner.y + i, l, inner.w, Style::new().fg(p::SUBTEXT1));
                }
            }
            Preview::Info(lines) => {
                for (i, l) in lines.iter().take(inner.h).enumerate() {
                    buf.line(inner.x, inner.y + 1 + i, l, inner.w);
                }
            }
        }

        let (status, hint_row) = footer.take_top(1);
        let status = status.inset(1, 0);
        if let Some((ask, input)) = &mut self.asking {
            let bg = with_alpha(p::SURFACE0, 0xE0);
            buf.fill(status, ' ', Style::new().bg(bg));
            let label = match ask {
                Ask::NewFile => "New file: ".to_string(),
                Ask::NewFolder => "New folder: ".to_string(),
                Ask::Rename(old) => format!("Rename {} to: ", old),
                Ask::Delete(name) => format!("Delete {}? (y / n) ", name),
            };
            let x = buf.text(status.x, status.y, &label, status.w, Style::new().fg(p::YELLOW).bg(bg).bold());
            if !matches!(ask, Ask::Delete(_)) {
                input.render(buf, Rect::new(x, status.y, status.right().saturating_sub(x), 1), Style::new().bg(bg), true, "name");
            }
        } else if let Some((m, is_err)) = &self.msg {
            buf.text(status.x, status.y, m, status.w, Style::new().fg(if *is_err { p::RED } else { p::GREEN }));
        } else if let Some(e) = self.selected() {
            let what = if e.is_dir { "folder".to_string() } else { widget::human_size(e.size) };
            let l = Line::new()
                .push(format!("{} ", self.state.selected + 1), Style::new().fg(p::LAVENDER))
                .push(format!("of {}  ", self.entries.len()), Style::new().fg(p::OVERLAY1))
                .push(what, Style::new().fg(p::SUBTEXT0));
            buf.line(status.x, status.y, &l, status.w);
        }
        widget::hints(
            buf,
            hint_row.inset(1, 0),
            &[("\u{2192}", "open"), ("e", "edit"), ("n / N", "new file / folder"), ("R", "rename"), ("d", "delete"), ("y / x / p", "copy / cut / paste"), ("t", "terminal"), ("q", "quit")],
        );
    }

    fn event(&mut self, ev: &Event, ctl: &mut Control) {
        match ev {
            Event::Key(k) => self.key(k, ctl),
            Event::Focus(f) => {
                self.focused = *f;
                if *f {
                    self.msg = None;
                    // Files may have changed while we were away.
                    let (cwd, sel) = (self.cwd.clone(), self.selected().map(|e| e.name.clone()));
                    self.cd(&cwd, sel.as_deref());
                }
            }
            Event::Resize(..) => {}
        }
    }
}

fn main() {
    let dir = inkui::args().into_iter().next().unwrap_or_else(|| "/".to_string());
    inkui::run(Explorer::new(&dir));
}
