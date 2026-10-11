// Nano-style text editor with light syntax highlighting.
// Settings are in /config/editor.toml.

use std::fs::File;
use std::io::{Read, Write};

use inkui::buffer::{Buffer, Line};
use inkui::event::{Event, Key, KeyEvent};
use inkui::icons;
use inkui::types::{palette as p, with_alpha, Style};
use inkui::widget::{self, Block, Input, InputResult, TextArea};
use inkui::{App, Control};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(default)]
struct Config {
    tab_width: usize,
    syntax_highlighting: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config { tab_width: 4, syntax_highlighting: true }
    }
}

#[derive(PartialEq, Clone, Copy)]
enum Prompt {
    None,
    SaveAs,
    Find,
    Goto,
    ConfirmQuit,
}

struct Editor {
    path: Option<String>,
    area: TextArea,
    prompt: Prompt,
    input: Input,
    last_find: String,
    // Status line message and whether it is an error.
    msg: Option<(String, bool)>,
    lang: Lang,
    focused: bool,
    cfg: Config,
}

#[derive(Clone, Copy, PartialEq)]
enum Lang {
    Plain,
    // Comment prefix and keyword list.
    Code(&'static str, &'static [&'static str]),
}

const RUST_KW: &[&str] = &[
    "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn", "for", "if", "impl", "in",
    "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super",
    "trait", "true", "type", "unsafe", "use", "where", "while", "async", "await", "dyn",
];
const C_KW: &[&str] = &[
    "if", "else", "for", "while", "do", "return", "switch", "case", "break", "continue", "struct", "typedef", "static",
    "const", "void", "int", "char", "unsigned", "long", "float", "double", "sizeof", "enum", "function", "var", "let",
    "true", "false", "null", "class", "new", "import", "export", "from",
];
const PY_KW: &[&str] = &[
    "def", "class", "if", "elif", "else", "for", "while", "return", "import", "from", "as", "in", "not", "and", "or",
    "True", "False", "None", "with", "try", "except", "lambda", "pass", "break", "continue", "yield",
];

fn lang_for(path: &str, cfg: &Config) -> Lang {
    if !cfg.syntax_highlighting {
        return Lang::Plain;
    }
    match path.rsplit_once('.').map(|(_, e)| e) {
        Some("rs") => Lang::Code("//", RUST_KW),
        Some("c" | "h" | "js" | "ts" | "cpp" | "java") => Lang::Code("//", C_KW),
        Some("py" | "sh" | "toml" | "cfg" | "ini") => Lang::Code("#", PY_KW),
        _ => Lang::Plain,
    }
}

// Per-character styles for one line of code.
fn highlight(line: &str, comment: &str, keywords: &[&str]) -> Vec<Style> {
    let chars: Vec<char> = line.chars().collect();
    let mut out = vec![Style::new(); chars.len()];
    let cm: Vec<char> = comment.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if chars[i..].starts_with(&cm) {
            for s in &mut out[i..] {
                *s = Style::new().fg(p::OVERLAY1);
            }
            break;
        }
        if c == '"' || c == '\'' {
            let start = i;
            i += 1;
            while i < chars.len() && chars[i] != c {
                if chars[i] == '\\' {
                    i += 1;
                }
                i += 1;
            }
            let end = (i + 1).min(chars.len());
            for s in &mut out[start..end] {
                *s = Style::new().fg(p::GREEN);
            }
            i = end;
            continue;
        }
        if c.is_ascii_digit() && (i == 0 || !chars[i - 1].is_alphanumeric() && chars[i - 1] != '_') {
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '.' || chars[i] == '_') {
                out[i] = Style::new().fg(p::PEACH);
                i += 1;
            }
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            let st = if keywords.contains(&word.as_str()) {
                Style::new().fg(p::MAUVE).bold()
            } else if i < chars.len() && chars[i] == '(' {
                Style::new().fg(p::BLUE)
            } else if i < chars.len() && chars[i] == '!' {
                Style::new().fg(p::SKY)
            } else if word.chars().next().map_or(false, |c| c.is_uppercase()) {
                Style::new().fg(p::YELLOW)
            } else {
                Style::new()
            };
            for s in &mut out[start..i] {
                *s = st;
            }
            continue;
        }
        if "{}[]()<>=+-*/&|!;:,.".contains(c) {
            out[i] = Style::new().fg(p::SKY);
        }
        i += 1;
    }
    out
}

impl Editor {
    fn open(path: Option<String>) -> Editor {
        let mut text = String::new();
        let mut msg = None;
        if let Some(p) = &path {
            match File::open(p) {
                Ok(mut f) => {
                    let mut data = Vec::new();
                    let _ = f.read_to_end(&mut data);
                    text = String::from_utf8_lossy(&data).into_owned();
                    let n = text.lines().count();
                    msg = Some((format!("read {} lines", n), false));
                }
                Err(_) => msg = Some(("new file".to_string(), false)),
            }
        }
        let cfg: Config = inkui::config::load("editor");
        let lang = path.as_deref().map(|p| lang_for(p, &cfg)).unwrap_or(Lang::Plain);
        let mut area = TextArea::new(&text);
        area.tab_width = cfg.tab_width.clamp(1, 16);
        Editor {
            path,
            area,
            prompt: Prompt::None,
            input: Input::new(),
            last_find: String::new(),
            msg,
            lang,
            focused: true,
            cfg,
        }
    }

    fn save(&mut self) {
        let Some(path) = self.path.clone() else {
            self.ask(Prompt::SaveAs, "");
            return;
        };
        let text = self.area.text();
        let res = File::options()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)
            .and_then(|mut f| f.write_all(text.as_bytes()));
        match res {
            Ok(()) => {
                self.area.modified = false;
                if path.starts_with("/config/") && path.ends_with(".toml") {
                    // Settings files: say so if they don't parse.
                    let name = icons::file_name(&path).trim_end_matches(".toml").to_string();
                    let checked = match name.as_str() {
                        "system" => inkui::config::check::<inkui::config::System>(&name),
                        "editor" => inkui::config::check::<Config>(&name),
                        _ => inkui::config::check::<toml_any::Any>(&name),
                    };
                    if let Err(e) = checked {
                        self.msg = Some((format!("saved, but the settings have an error: {}", e.trim()), true));
                        return;
                    }
                }
                self.msg = Some((format!("{} saved {} lines to {}", icons::CHECK, self.area.lines.len(), path), false));
            }
            Err(_) => self.msg = Some((format!("cannot write {}", path), true)),
        }
    }

    fn ask(&mut self, p: Prompt, initial: &str) {
        self.prompt = p;
        self.input.set(initial);
    }

    fn prompt_key(&mut self, k: &KeyEvent, ctl: &mut Control) {
        if self.prompt == Prompt::ConfirmQuit {
            match k.text().map(|c| c.to_ascii_lowercase()) {
                Some('y') => {
                    self.prompt = Prompt::None;
                    self.save();
                    if !self.area.modified {
                        ctl.quit();
                    }
                }
                Some('n') => ctl.quit(),
                _ if k.key == Key::Esc => self.prompt = Prompt::None,
                _ => {}
            }
            return;
        }
        match self.input.handle(k) {
            InputResult::Cancel => self.prompt = Prompt::None,
            InputResult::Submit => {
                let text = self.input.text.trim().to_string();
                let kind = self.prompt;
                self.prompt = Prompt::None;
                match kind {
                    Prompt::SaveAs if !text.is_empty() => {
                        let path = icons::resolve("/", &text);
                        self.lang = lang_for(&path, &self.cfg);
                        self.path = Some(path);
                        self.save();
                    }
                    Prompt::Find => {
                        self.last_find = text.clone();
                        self.find();
                    }
                    Prompt::Goto => match text.parse::<usize>() {
                        Ok(n) if n > 0 => self.area.goto(n - 1, 0),
                        _ => self.msg = Some(("not a line number".into(), true)),
                    },
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn find(&mut self) {
        let needle = self.last_find.clone();
        if !self.area.find(&needle) {
            self.msg = Some((format!("\"{}\" not found", needle), true));
        }
    }

    fn key(&mut self, k: &KeyEvent, ctl: &mut Control) {
        if self.prompt != Prompt::None {
            return self.prompt_key(k, ctl);
        }
        self.msg = None;
        match k.key {
            Key::Char('s') if k.ctrl() => self.save(),
            Key::Char('q') if k.ctrl() => {
                if self.area.modified {
                    self.prompt = Prompt::ConfirmQuit;
                } else {
                    ctl.quit();
                }
            }
            Key::Char('f') | Key::Char('w') if k.ctrl() => {
                let last = self.last_find.clone();
                self.ask(Prompt::Find, &last);
            }
            Key::F(3) => self.find(),
            Key::Char('g') if k.ctrl() => self.ask(Prompt::Goto, ""),
            _ => {
                self.area.handle(k);
            }
        }
    }
}

impl App for Editor {
    fn draw(&mut self, buf: &mut Buffer) {
        let area = buf.area;
        let (main, bottom) = area.take_bottom(2);
        let name = self.path.as_deref().map(icons::file_name).unwrap_or("untitled").to_string();
        let (icon, color) = icons::file_icon(&name, false);

        let mut title = Line::new()
            .push(format!("{} ", icon), Style::new().fg(color))
            .push(name.clone(), Style::new().fg(p::TEXT).bold());
        if self.area.modified {
            title.add(" \u{25cf}", Style::new().fg(p::PEACH));
        }
        let pos = Line::new()
            .push(format!("Ln {}", self.area.row + 1), Style::new().fg(p::LAVENDER))
            .push(format!(", Col {}", self.area.col + 1), Style::new().fg(p::SUBTEXT0));
        let footer = Line::styled(
            self.path.clone().unwrap_or_else(|| "not saved yet".into()),
            Style::new().fg(p::OVERLAY0),
        );
        let inner = Block::new()
            .title(title)
            .title_right(pos)
            .footer(footer)
            .focused(self.focused)
            .padding(0, 0)
            .render(buf, main);
        let editing = self.prompt == Prompt::None && self.focused;
        match self.lang {
            Lang::Code(cm, kw) => {
                let f = move |l: &str| highlight(l, cm, kw);
                self.area.render(buf, inner, editing, Some(&f));
            }
            Lang::Plain => self.area.render(buf, inner, editing, None),
        }

        let (status, hint_row) = bottom.take_top(1);
        let status = status.inset(1, 0);
        let label = match self.prompt {
            Prompt::SaveAs => Some("Save as: "),
            Prompt::Find => Some("Find: "),
            Prompt::Goto => Some("Go to line: "),
            Prompt::ConfirmQuit => Some("Save changes before closing? (y / n / Esc) "),
            Prompt::None => None,
        };
        if let Some(label) = label {
            buf.fill(status, ' ', Style::new().bg(with_alpha(p::SURFACE0, 0xE0)));
            let x = buf.text(status.x, status.y, label, status.w, Style::new().fg(p::YELLOW).bg(with_alpha(p::SURFACE0, 0xE0)).bold());
            if self.prompt != Prompt::ConfirmQuit {
                let r = inkui::Rect::new(x, status.y, status.right().saturating_sub(x), 1);
                self.input.render(buf, r, Style::new().bg(with_alpha(p::SURFACE0, 0xE0)), true, "");
            }
        } else if let Some((m, is_err)) = &self.msg {
            let st = if *is_err { Style::new().fg(p::RED) } else { Style::new().fg(p::GREEN) };
            buf.text(status.x, status.y, m, status.w, st);
        }
        widget::hints(
            buf,
            hint_row.inset(1, 0),
            &[("Ctrl + S", "save"), ("Ctrl + Q", "quit"), ("Ctrl + F", "find"), ("Ctrl + G", "go to line"), ("Ctrl + X", "cut line"), ("Ctrl + C", "copy line"), ("Ctrl + V", "paste")],
        );
    }

    fn quit_requested(&mut self, ctl: &mut Control) {
        if self.area.modified {
            self.prompt = Prompt::ConfirmQuit;
        } else {
            ctl.quit();
        }
    }

    fn event(&mut self, ev: &Event, ctl: &mut Control) {
        match ev {
            Event::Key(k) => self.key(k, ctl),
            Event::Focus(f) => self.focused = *f,
            Event::Resize(..) => {}
        }
    }
}

// Accepts any valid TOML, to check a settings file's syntax.
mod toml_any {
    #[derive(serde::Deserialize)]
    pub struct Any {}
}

fn main() {
    let path = inkui::args().into_iter().next();
    inkui::run(Editor::open(path));
}
