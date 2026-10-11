// Terminal: the place apps are started from.
// Settings live in /config/terminal.toml, history in /config/history.txt.

use std::fs::File;
use std::io::{Read, Write};

use inkui::buffer::{Buffer, Line};
use inkui::event::{Event, Key, KeyEvent};
use inkui::icons::{self, resolve};
use inkui::layout::Rect;
use inkui::types::{palette as p, with_alpha, Style};
use inkui::widget::{self, Input, InputResult};
use inkui::{files, App, Control};
use serde::Deserialize;

const MAX_LINES: usize = 2000;
const MAX_CAT_BYTES: usize = 64 * 1024;

const COMMANDS: &[(&str, &str)] = &[
    ("help", "this list"),
    ("ls [-l] [dir]", "list a directory"),
    ("cd [dir]", "change directory"),
    ("pwd", "print the current directory"),
    ("cat <file>", "print a file"),
    ("echo <text> [> file]", "print text, or write it to a file"),
    ("touch <file>", "create an empty file"),
    ("mkdir [-p] <dir>", "create a folder"),
    ("rm [-r] <path>", "delete a file (-r: a folder too)"),
    ("rmdir <dir>", "delete an empty folder"),
    ("mv <from> <to>", "move or rename"),
    ("cp [-r] <from> <to>", "copy a file (-r: a folder too)"),
    ("clip [file]", "show the clipboard, or copy a file into it"),
    ("config [name]", "edit settings (system, terminal, editor)"),
    ("open <file>", "open a file in the editor or viewer"),
    ("edit [file]", "text editor"),
    ("view <image>", "image viewer"),
    ("files [dir]", "file explorer"),
    ("calc", "calculator"),
    ("python [file]", "Python 3 prompt, or run a script"),
    ("ps", "running apps"),
    ("kill <tid>", "stop an app"),
    ("fetch", "system summary"),
    ("colors", "the palette"),
    ("history", "previous commands"),
    ("clear", "clear the screen (Ctrl + L)"),
    ("exit", "close this terminal"),
];

// Short names for apps.
const ALIASES: &[(&str, &str)] = &[
    ("edit", "editor"),
    ("nano", "editor"),
    ("view", "viewer"),
    ("files", "explorer"),
    ("ranger", "explorer"),
];

const HISTORY_FILE: &str = "/config/history.txt";

// Apps that print and read text instead of opening a desktop; they run here.
const CONSOLE_APPS: &[&str] = &["python"];

// A console app running in this terminal.
struct Child {
    pipe: inkui::app::Pipe,
    // Output after the last newline (for Python, its >>> prompt).
    partial: String,
    // Bytes of a UTF-8 character split across reads.
    pending: Vec<u8>,
    // Inside an ANSI escape sequence, which is dropped.
    escape: bool,
    // Name shown while it starts up (Python takes a few seconds).
    name: String,
    started: bool,
}

#[derive(Deserialize)]
#[serde(default)]
struct Config {
    // Text in the first prompt pill.
    prompt_name: String,
    start_dir: String,
    save_history: bool,
    history_size: usize,
}

impl Default for Config {
    fn default() -> Self {
        Config { prompt_name: "krake".into(), start_dir: "/".into(), save_history: true, history_size: 500 }
    }
}

const CONFIG_DEFAULT: &str = r#"# Terminal settings, read when a terminal starts or comes back into view.

# text in the first prompt pill
prompt_name = "krake"
# where new terminals start
start_dir = "/"
# keep command history in /config/history.txt across reboots
save_history = true
history_size = 500
"#;

const EDITOR_DEFAULT: &str = r#"# Editor settings, read when the editor starts.

# spaces per Tab stop
tab_width = 4
# colors for .rs, .c, .js, .py, .toml and similar files
syntax_highlighting = true
"#;

// Config files the config command knows, with their default text.
const CONFIGS: &[(&str, &str)] = &[
    ("system", inkui::config::SYSTEM_DEFAULT),
    ("terminal", CONFIG_DEFAULT),
    ("editor", EDITOR_DEFAULT),
];

fn ok() -> Style {
    Style::new().fg(p::GREEN)
}
fn err() -> Style {
    Style::new().fg(p::RED)
}
fn dim() -> Style {
    Style::new().fg(p::OVERLAY1)
}

struct Terminal {
    out: Vec<Line>,
    input: Input,
    history: Vec<String>,
    hist_pos: Option<usize>,
    draft: String,
    cwd: String,
    // Rows scrolled up from the bottom.
    scroll: usize,
    // Rows the output area had at the last draw (for paging).
    page: usize,
    // Text columns at the last draw (for ls columns and wrapping).
    page_cols: usize,
    last_ok: bool,
    focused: bool,
    exit: bool,
    cfg: Config,
    child: Option<Child>,
}

impl Terminal {
    fn new() -> Terminal {
        let mut t = Terminal {
            out: Vec::new(),
            input: Input::new(),
            history: Vec::new(),
            hist_pos: None,
            draft: String::new(),
            cwd: "/".to_string(),
            scroll: 0,
            page: 20,
            page_cols: 80,
            last_ok: true,
            focused: true,
            exit: false,
            cfg: inkui::config::load("terminal"),
            child: None,
        };
        t.load_history();
        t.banner();
        let start = t.cfg.start_dir.clone();
        t.cd(&start);
        t
    }

    fn load_history(&mut self) {
        if !self.cfg.save_history {
            return;
        }
        if let Ok(text) = std::fs::read_to_string(HISTORY_FILE) {
            self.history = text.lines().filter(|l| !l.trim().is_empty()).map(String::from).collect();
            let keep = self.cfg.history_size.max(1);
            if self.history.len() > keep {
                self.history.drain(..self.history.len() - keep);
            }
        }
    }

    fn save_history(&self) {
        if !self.cfg.save_history {
            return;
        }
        let _ = std::fs::create_dir(inkui::config::DIR);
        let keep = self.cfg.history_size.max(1);
        let from = self.history.len().saturating_sub(keep);
        let mut text = self.history[from..].join("\n");
        text.push('\n');
        let _ = std::fs::write(HISTORY_FILE, text);
    }

    fn print(&mut self, l: impl Into<Line>) {
        self.out.push(l.into());
        if self.out.len() > MAX_LINES {
            self.out.drain(..self.out.len() - MAX_LINES);
        }
    }

    // A blank row so the next prompt doesn't sit right under the last one.
    fn gap(&mut self) {
        if self.out.last().map_or(false, |l| l.width() > 0) {
            self.print(Line::new());
        }
    }

    fn fail(&mut self, msg: impl Into<String>) {
        self.print(Line::new().push(format!("{} ", icons::CROSS), err()).push(msg.into(), err()));
        self.last_ok = false;
    }

    fn banner(&mut self) {
        self.print(
            Line::new()
                .push(" krake", Style::new().fg(p::MAUVE).bold())
                .push("OS", Style::new().fg(p::BLUE).bold())
                .push("  terminal", dim()),
        );
        self.print(Line::new().push("  type ", dim()).push("help", Style::new().fg(p::YELLOW)).push(" for commands, or an app name to start it", dim()));
        self.print(Line::new());
    }

    fn prompt(&self) -> Line {
        if let Some(c) = &self.child {
            if !c.started {
                return Line::new().push(format!("starting {}\u{2026} ", c.name), dim());
            }
            return Line::new().push(c.partial.clone(), Style::new().fg(p::SUBTEXT0));
        }
        let home = if self.cwd == "/" { "/".to_string() } else { self.cwd.clone() };
        let arrow = if self.last_ok { p::GREEN } else { p::RED };
        Line::new()
            .push("\u{e0b6}", Style::new().fg(p::MAUVE))
            .push(format!("{} {} ", icons::TERMINAL, self.cfg.prompt_name), Style::new().fg(p::CRUST).bg(p::MAUVE).bold())
            .push("\u{e0b0}", Style::new().fg(p::MAUVE).bg(p::BLUE))
            .push(format!(" {} {} ", icons::FOLDER_OPEN, home), Style::new().fg(p::CRUST).bg(p::BLUE).bold())
            .push("\u{e0b0}", Style::new().fg(p::BLUE))
            .push(" \u{276f} ", Style::new().fg(arrow).bold())
    }

    fn run(&mut self, line: &str) {
        let line = line.trim();
        self.last_ok = true;
        if line.is_empty() {
            return;
        }
        let (cmd, rest) = line.split_once(' ').map(|(a, b)| (a, b.trim())).unwrap_or((line, ""));
        let args: Vec<&str> = rest.split_whitespace().collect();
        match cmd {
            "help" => self.help(),
            "ls" | "dir" | "ll" => self.ls(&args, cmd == "ll"),
            "cd" => self.cd(rest),
            "pwd" => self.print(self.cwd.clone()),
            "cat" => self.cat(rest),
            "echo" => self.echo(rest),
            "touch" => self.touch(rest),
            "mkdir" => self.mkdir(&args),
            "rm" => self.rm(&args),
            "rmdir" => self.rmdir(&args),
            "mv" => self.mv(&args),
            "cp" => self.cp(&args),
            "clip" => self.clip(rest),
            "config" => self.config(rest),
            "open" => self.open(rest),
            "clear" | "cls" => self.out.clear(),
            "ps" => self.ps(),
            "kill" => self.kill(rest),
            "fetch" | "neofetch" => self.fetch(),
            "colors" => self.colors(),
            "history" => {
                for (i, h) in self.history.clone().iter().enumerate() {
                    self.print(Line::new().push(format!("{:>4}  ", i + 1), dim()).push(h.clone(), Style::new()));
                }
            }
            "run" => {
                if let Some((app, more)) = args.split_first() {
                    self.start(app, more);
                } else {
                    self.fail("usage: run <app> [args]");
                }
            }
            _ => self.start(cmd, &args),
        }
    }

    fn help(&mut self) {
        self.print(Line::styled("commands", Style::new().fg(p::MAUVE).bold()));
        for (c, d) in COMMANDS {
            self.print(Line::new().push(format!("  {:<22}", c), Style::new().fg(p::YELLOW)).push(*d, Style::new().fg(p::SUBTEXT0)));
        }
        self.print(Line::styled("apps", Style::new().fg(p::MAUVE).bold()));
        let apps = self.apps().join("  ");
        self.print(Line::new().push("  ", Style::new()).push(apps, Style::new().fg(p::GREEN)));
        self.print(Line::styled("desktops", Style::new().fg(p::MAUVE).bold()));
        for (k, d) in [("Win", "new terminal"), ("Win + \u{2190}/\u{2192}", "switch desktop"), ("Win + \u{2191}/\u{2193}", "scroll"), ("Win + Q", "close the app on screen")] {
            self.print(Line::new().push(format!("  {:<22}", k), Style::new().fg(p::SKY)).push(d, Style::new().fg(p::SUBTEXT0)));
        }
    }

    fn apps(&self) -> Vec<String> {
        icons::list_dir("/apps")
            .map(|v| {
                v.into_iter()
                    .filter_map(|e| e.name.strip_suffix(".wasm").map(|s| s.to_string()))
                    .filter(|n| n != "shell")
                    .collect()
            })
            .unwrap_or_default()
    }

    fn ls(&mut self, args: &[&str], long_default: bool) {
        let long = long_default || args.contains(&"-l");
        let target = args.iter().find(|a| !a.starts_with('-')).copied().unwrap_or(".");
        let path = resolve(&self.cwd, target);
        let entries = match icons::list_dir(&path) {
            Ok(e) => e,
            Err(_) => return self.fail(format!("ls: {}: no such directory", target)),
        };
        if entries.is_empty() {
            self.print(Line::styled("  (empty)", dim()));
            return;
        }
        let item = |e: &icons::Entry| {
            let (icon, color) = icons::file_icon(&e.name, e.is_dir);
            let st = if e.is_dir { Style::new().fg(color).bold() } else { Style::new().fg(color) };
            (format!("{} {}", icon, e.name), st)
        };
        if long {
            for e in &entries {
                let (text, st) = item(e);
                let size = if e.is_dir { "-".to_string() } else { widget::human_size(e.size) };
                self.print(Line::new().push(format!("{:>8}  ", size), Style::new().fg(p::SUBTEXT0)).push(text, st));
            }
            return;
        }
        // Columns, filled down first like ls.
        let width = self.cols();
        let colw = entries.iter().map(|e| e.name.chars().count() + 4).max().unwrap_or(8).min(width.max(8));
        let ncols = (width / colw).max(1);
        let nrows = (entries.len() + ncols - 1) / ncols;
        for r in 0..nrows {
            let mut l = Line::new();
            for c in 0..ncols {
                if let Some(e) = entries.get(c * nrows + r) {
                    let (text, st) = item(e);
                    let pad = colw.saturating_sub(text.chars().count());
                    l.add(text, st);
                    l.add(" ".repeat(pad), Style::new());
                }
            }
            self.print(l);
        }
    }

    fn cols(&self) -> usize {
        self.page_cols
    }

    fn cd(&mut self, arg: &str) {
        let target = if arg.is_empty() { "/" } else { arg };
        let path = resolve(&self.cwd, target);
        if std::fs::read_dir(&path).is_ok() {
            self.cwd = path;
        } else {
            self.fail(format!("cd: {}: no such directory", target));
        }
    }

    fn cat(&mut self, arg: &str) {
        if arg.is_empty() {
            return self.fail("usage: cat <file>");
        }
        let path = resolve(&self.cwd, arg);
        let mut data = Vec::new();
        match File::open(&path) {
            Ok(f) => {
                let _ = f.take(MAX_CAT_BYTES as u64).read_to_end(&mut data);
            }
            Err(_) => return self.fail(format!("cat: {}: no such file", arg)),
        }
        if data.iter().take(512).any(|&b| b == 0) {
            return self.fail(format!("cat: {}: binary file ({})", arg, widget::human_size(data.len() as u64)));
        }
        for l in String::from_utf8_lossy(&data).lines() {
            self.print(l.replace('\t', "    "));
        }
    }

    fn echo(&mut self, rest: &str) {
        if let Some((text, file)) = rest.split_once('>') {
            let (append, file) = match file.strip_prefix('>') {
                Some(f) => (true, f.trim()),
                None => (false, file.trim()),
            };
            if file.is_empty() {
                return self.fail("echo: missing file name after >");
            }
            let path = resolve(&self.cwd, file);
            let mut f = match File::options().write(true).create(true).append(append).truncate(!append).open(&path) {
                Ok(f) => f,
                Err(_) => return self.fail(format!("echo: cannot write {}", file)),
            };
            let _ = f.write_all(format!("{}\n", text.trim()).as_bytes());
        } else {
            self.print(rest.to_string());
        }
    }

    fn touch(&mut self, arg: &str) {
        if arg.is_empty() {
            return self.fail("usage: touch <file>");
        }
        let path = resolve(&self.cwd, arg);
        if File::options().write(true).create(true).open(&path).is_err() {
            self.fail(format!("touch: cannot create {}", arg));
        }
    }

    // Splits args into flags (like -r, -rf) and paths.
    fn flags<'a>(args: &[&'a str]) -> (String, Vec<&'a str>) {
        let mut flags = String::new();
        let mut paths = Vec::new();
        for a in args {
            match a.strip_prefix('-') {
                Some(f) if !f.is_empty() => flags.push_str(f),
                _ => paths.push(*a),
            }
        }
        (flags, paths)
    }

    fn mkdir(&mut self, args: &[&str]) {
        let (flags, paths) = Self::flags(args);
        if paths.is_empty() {
            return self.fail("usage: mkdir [-p] <dir>...");
        }
        for name in paths {
            let path = resolve(&self.cwd, name);
            let r = if flags.contains('p') { std::fs::create_dir_all(&path) } else { std::fs::create_dir(&path) };
            if let Err(e) = r {
                self.fail(format!("mkdir: {}: {}", name, files::why(&e)));
            }
        }
    }

    fn rm(&mut self, args: &[&str]) {
        let (flags, paths) = Self::flags(args);
        if paths.is_empty() {
            return self.fail("usage: rm [-r] <path>...");
        }
        for name in paths {
            let path = resolve(&self.cwd, name);
            if path == "/" {
                self.fail("rm: refusing to delete /");
                continue;
            }
            let r = if files::is_dir(&path) {
                if !flags.contains('r') {
                    self.fail(format!("rm: {}: is a folder (use rm -r)", name));
                    continue;
                }
                files::remove_all(&path)
            } else {
                std::fs::remove_file(&path)
            };
            match r {
                Ok(()) if self.cwd == path || self.cwd.starts_with(&format!("{}/", path)) => self.cwd = icons::parent(&path),
                Ok(()) => {}
                Err(e) => self.fail(format!("rm: {}: {}", name, files::why(&e))),
            }
        }
    }

    fn rmdir(&mut self, args: &[&str]) {
        if args.is_empty() {
            return self.fail("usage: rmdir <dir>...");
        }
        for name in args {
            if let Err(e) = std::fs::remove_dir(resolve(&self.cwd, name)) {
                self.fail(format!("rmdir: {}: {}", name, files::why(&e)));
            }
        }
    }

    // Where from lands for mv/cp: inside to if that's a folder.
    fn target(&self, from: &str, to: &str) -> String {
        let to = resolve(&self.cwd, to);
        if files::is_dir(&to) {
            resolve(&to, icons::file_name(from))
        } else {
            to
        }
    }

    fn mv(&mut self, args: &[&str]) {
        let (_, paths) = Self::flags(args);
        let [from, to] = paths[..] else { return self.fail("usage: mv <from> <to>") };
        let src = resolve(&self.cwd, from);
        let dst = self.target(&src, to);
        if let Err(e) = std::fs::rename(&src, &dst) {
            self.fail(format!("mv: {}: {}", from, files::why(&e)));
        }
    }

    fn cp(&mut self, args: &[&str]) {
        let (flags, paths) = Self::flags(args);
        let [from, to] = paths[..] else { return self.fail("usage: cp [-r] <from> <to>") };
        let src = resolve(&self.cwd, from);
        if files::is_dir(&src) && !flags.contains('r') {
            return self.fail(format!("cp: {}: is a folder (use cp -r)", from));
        }
        let dst = self.target(&src, to);
        if dst == src {
            return self.fail(format!("cp: {}: source and destination are the same", from));
        }
        if let Err(e) = files::copy(&src, &dst) {
            self.fail(format!("cp: {}: {}", from, files::why(&e)));
        }
    }

    fn clip(&mut self, arg: &str) {
        if arg.is_empty() {
            let text = inkui::clipboard::get();
            if text.is_empty() {
                self.print(Line::styled("  (the clipboard is empty)", dim()));
            }
            for l in text.lines() {
                self.print(l.replace('\t', "    "));
            }
            return;
        }
        match std::fs::read(resolve(&self.cwd, arg)) {
            Ok(data) if data.len() <= 1 << 20 => {
                inkui::clipboard::set(&String::from_utf8_lossy(&data));
                self.print(Line::styled(format!("{} copied {} to the clipboard", icons::CHECK, widget::human_size(data.len() as u64)), ok()));
            }
            Ok(_) => self.fail(format!("clip: {}: too big (1 MB max)", arg)),
            Err(e) => self.fail(format!("clip: {}: {}", arg, files::why(&e))),
        }
    }

    fn config(&mut self, arg: &str) {
        let name = if arg.is_empty() { "system" } else { arg.trim_end_matches(".toml") };
        let Some((_, default)) = CONFIGS.iter().find(|(n, _)| *n == name) else {
            let names: Vec<&str> = CONFIGS.iter().map(|(n, _)| *n).collect();
            return self.fail(format!("config: unknown settings '{}' (try {})", name, names.join(", ")));
        };
        let path = inkui::config::ensure(name, default);
        self.launch("/apps/editor.wasm", &[&path]);
    }

    fn open(&mut self, arg: &str) {
        if arg.is_empty() {
            return self.fail("usage: open <file>");
        }
        let path = resolve(&self.cwd, arg);
        if File::open(&path).is_err() {
            return self.fail(format!("open: {}: no such file", arg));
        }
        if path.ends_with(".wasm") {
            return self.launch(&path, &[]);
        }
        let app = icons::opener(&path);
        self.launch(app, &[&path]);
    }

    // Starts an app name, alias or .wasm path; file arguments resolve against the cwd.
    fn start(&mut self, name: &str, args: &[&str]) {
        let app = ALIASES.iter().find(|(a, _)| *a == name).map(|(_, b)| *b).unwrap_or(name);
        let path = if app.ends_with(".wasm") {
            let p = resolve(&self.cwd, app);
            File::open(&p).ok().map(|_| p)
        } else {
            inkui::app::app_path(app)
        };
        let Some(path) = path else {
            return self.fail(format!("{}: command not found (try help)", name));
        };
        let resolved: Vec<String> = args
            .iter()
            .map(|a| if a.starts_with('-') { a.to_string() } else { resolve(&self.cwd, a) })
            .collect();
        let refs: Vec<&str> = resolved.iter().map(|s| s.as_str()).collect();
        let stem = icons::file_name(&path).trim_end_matches(".wasm").to_string();
        if CONSOLE_APPS.contains(&stem.as_str()) {
            return self.run_console(&path, &refs);
        }
        self.launch(&path, &refs);
    }

    // Runs a console app here: output to the scrollback, input line to its stdin.
    fn run_console(&mut self, path: &str, args: &[&str]) {
        let Some(pipe) = inkui::app::Pipe::open() else {
            return self.fail("could not open a pipe for the app");
        };
        if !pipe.spawn(path, args) {
            return self.fail(format!("could not start {}", path));
        }
        let name = icons::file_name(path).trim_end_matches(".wasm").to_string();
        self.child = Some(Child { pipe, partial: String::new(), pending: Vec::new(), escape: false, name, started: false });
    }

    // Moves the console app's new output into the scrollback; true if anything changed.
    fn pump_child(&mut self) -> bool {
        let Some(c) = &mut self.child else { return false };
        let mut bytes = std::mem::take(&mut c.pending);
        bytes.extend(c.pipe.read());
        if bytes.is_empty() {
            return false;
        }
        c.started = true;
        let valid = match std::str::from_utf8(&bytes) {
            Ok(s) => s.len(),
            Err(e) if e.error_len().is_none() => e.valid_up_to(),
            Err(_) => bytes.len(),
        };
        c.pending = bytes[valid..].to_vec();
        let text = String::from_utf8_lossy(&bytes[..valid]).into_owned();
        let mut lines = Vec::new();
        for ch in text.chars() {
            if c.escape {
                c.escape = !ch.is_ascii_alphabetic();
                continue;
            }
            match ch {
                '\x1b' => c.escape = true,
                '\n' => lines.push(std::mem::take(&mut c.partial)),
                '\r' => {}
                '\x08' => {
                    c.partial.pop();
                }
                '\t' => c.partial.push_str("    "),
                ch if ch.is_control() => {}
                ch => c.partial.push(ch),
            }
        }
        for l in lines {
            self.print(Line::styled(l, Style::new().fg(p::TEXT)));
        }
        true
    }

    // The console app exited (or was stopped): back to the shell prompt.
    fn end_child(&mut self) {
        if let Some(c) = self.child.take() {
            if !c.partial.is_empty() {
                self.print(Line::styled(c.partial, Style::new().fg(p::TEXT)));
            }
        }
        self.gap();
    }

    fn launch(&mut self, path: &str, args: &[&str]) {
        if inkui::spawn(path, args) {
            self.print(
                Line::new()
                    .push(format!("{} ", icons::ROCKET), Style::new().fg(p::PEACH))
                    .push("started ", dim())
                    .push(icons::file_name(path).trim_end_matches(".wasm").to_string(), Style::new().fg(p::GREEN).bold())
                    .push(" on a new desktop  ", dim())
                    .push("Win + \u{2190}", Style::new().fg(p::SKY))
                    .push(" comes back here", dim()),
            );
        } else {
            self.fail(format!("could not start {}", path));
        }
    }

    fn ps(&mut self) {
        let text = inkui::app::processes();
        for (i, l) in text.lines().enumerate() {
            let st = if i == 0 {
                Style::new().fg(p::MAUVE).bold()
            } else if l.starts_with("free") || l.starts_with("cpus") {
                Style::new().fg(p::TEAL)
            } else if l.contains("running") {
                Style::new().fg(p::GREEN)
            } else {
                Style::new().fg(p::SUBTEXT1)
            };
            self.print(Line::styled(l.to_string(), st));
        }
    }

    fn kill(&mut self, arg: &str) {
        let Ok(tid) = arg.trim().parse::<usize>() else {
            return self.fail("usage: kill <tid>  (see ps)");
        };
        if inkui::app::kill(tid) {
            self.print(Line::new().push(format!("{} ", icons::CHECK), ok()).push(format!("stopped {}", tid), Style::new()));
        } else {
            self.fail(format!("kill: {}: no such app (system threads can't be stopped)", tid));
        }
    }

    fn fetch(&mut self) {
        let procs = inkui::app::processes();
        let apps = procs.lines().filter(|l| l.contains(".wasm")).count();
        let free = procs.lines().find(|l| l.starts_with("free")).unwrap_or("").replace("free RAM: ", "");
        let cpus = procs.lines().find(|l| l.starts_with("cpus")).unwrap_or("cpus: 1").replace("cpus: ", "");
        let cores = if cpus == "1" { "1 core".to_string() } else { format!("{} cores", cpus) };
        let (h, m, _) = inkui::app::clock();
        let logo = [
            "  \u{2584}\u{2588}\u{2588}\u{2588}\u{2584}  ",
            " \u{2588} \u{2588}\u{2588}\u{2588} \u{2588} ",
            " \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588} ",
            " \u{2588}\u{2580}\u{2588}\u{2580}\u{2588}\u{2580}\u{2588} ",
            " \u{2580} \u{2580} \u{2580} \u{2580} ",
        ];
        let info: [(&str, String); 5] = [
            ("os", format!("KrakeOS (RISC-V, {})", cores)),
            ("apps", format!("{} running", apps)),
            ("memory", format!("{} free", free)),
            ("time", format!("{:02}:{:02} UTC", h, m)),
            ("shell", "inkui terminal".into()),
        ];
        let colors = [p::MAUVE, p::PINK, p::RED, p::PEACH, p::YELLOW];
        for i in 0..logo.len() {
            let mut l = Line::new().push(logo[i], Style::new().fg(colors[i]).bold()).push("  ", Style::new());
            let (k, v) = &info[i];
            l.add(format!("{:<8}", k), Style::new().fg(p::BLUE).bold());
            l.add(v.clone(), Style::new());
            self.print(l);
        }
        self.colors();
    }

    fn colors(&mut self) {
        let mut l = Line::new().push("  ", Style::new());
        for c in [p::RED, p::PEACH, p::YELLOW, p::GREEN, p::TEAL, p::SKY, p::BLUE, p::MAUVE, p::PINK, p::FLAMINGO] {
            l.add("\u{2588}\u{2588}\u{2588}", Style::new().fg(c));
        }
        self.print(l);
    }

    fn complete(&mut self) {
        let text = self.input.text.clone();
        let word_start = text.rfind(' ').map(|i| i + 1).unwrap_or(0);
        let word = &text[word_start..];
        let candidates: Vec<String> = if word_start == 0 {
            let mut c: Vec<String> = COMMANDS.iter().map(|(c, _)| c.split(' ').next().unwrap().to_string()).collect();
            c.extend(self.apps());
            c.extend(ALIASES.iter().map(|(a, _)| a.to_string()));
            c.sort();
            c.dedup();
            c.into_iter().filter(|c| c.starts_with(word)).collect()
        } else {
            let (dir, prefix) = match word.rfind('/') {
                Some(i) => (&word[..=i], &word[i + 1..]),
                None => ("", word),
            };
            let path = resolve(&self.cwd, if dir.is_empty() { "." } else { dir });
            icons::list_dir(&path)
                .unwrap_or_default()
                .into_iter()
                .filter(|e| e.name.starts_with(prefix))
                .map(|e| format!("{}{}{}", dir, e.name, if e.is_dir { "/" } else { "" }))
                .collect()
        };
        if candidates.is_empty() {
            return;
        }
        let common = candidates.iter().skip(1).fold(candidates[0].clone(), |acc, c| {
            acc.chars().zip(c.chars()).take_while(|(a, b)| a == b).map(|(a, _)| a).collect()
        });
        let mut new = text[..word_start].to_string() + &common;
        if candidates.len() == 1 && !common.ends_with('/') {
            new.push(' ');
        }
        if candidates.len() > 1 && common.len() == word.len() {
            // Nothing more to add: show the options.
            let mut l = Line::new();
            for c in &candidates {
                let name = c.rsplit('/').find(|s| !s.is_empty()).unwrap_or(c);
                let st = if c.ends_with('/') { Style::new().fg(p::BLUE).bold() } else { Style::new().fg(p::SUBTEXT1) };
                l.add(format!("{}  ", name), st);
            }
            self.print(self.prompt().push(text.clone(), Style::new()));
            self.out.extend(widget::wrap_line(&l, self.page_cols.max(10)));
            self.gap();
        }
        self.input.set(&new);
    }

    fn submit(&mut self) {
        let line = std::mem::take(&mut self.input.text);
        self.input.clear();
        if let Some(c) = &mut self.child {
            let echo = Line::new().push(std::mem::take(&mut c.partial), Style::new().fg(p::SUBTEXT0)).push(line.clone(), Style::new());
            c.pipe.write(format!("{}\n", line).as_bytes());
            self.print(echo);
            if !line.trim().is_empty() && self.history.last().map(|h| h != &line).unwrap_or(true) {
                self.history.push(line);
            }
            self.hist_pos = None;
            self.scroll = 0;
            return;
        }
        let echo = self.prompt().push(line.clone(), Style::new());
        self.print(echo);
        if !line.trim().is_empty() && self.history.last().map(|h| h != &line).unwrap_or(true) {
            self.history.push(line.clone());
            self.save_history();
        }
        self.hist_pos = None;
        self.scroll = 0;
        if line.trim() == "exit" {
            self.exit = true;
            return;
        }
        self.run(&line);
        if self.child.is_none() {
            self.gap();
        }
    }

    fn key(&mut self, k: &KeyEvent, ctl: &mut Control) {
        match k.key {
            Key::Up if k.super_() => {
                self.scroll += 3;
                return;
            }
            Key::Down if k.super_() => {
                self.scroll = self.scroll.saturating_sub(3);
                return;
            }
            Key::Char('l') if k.ctrl() => {
                self.out.clear();
                return;
            }
            Key::Char('c') if k.ctrl() && self.child.is_some() => {
                // Closing the pipe stops the app.
                let echo = self.prompt().push(self.input.text.clone(), Style::new()).push("^C", dim());
                if let Some(c) = &mut self.child {
                    c.partial.clear();
                }
                self.print(echo);
                self.input.clear();
                self.end_child();
                return;
            }
            Key::Char('d') if k.ctrl() && self.input.text.is_empty() && self.child.is_some() => {
                if let Some(c) = &mut self.child {
                    c.pipe.write(&[]);
                }
                return;
            }
            Key::Tab if self.child.is_some() => {
                // Python indentation
                for _ in 0..4 {
                    self.input.handle(&KeyEvent { key: Key::Char(' '), mods: 0 });
                }
                return;
            }
            Key::Char('c') if k.ctrl() => {
                let echo = self.prompt().push(self.input.text.clone(), Style::new()).push("^C", dim());
                self.print(echo);
                self.gap();
                self.input.clear();
                return;
            }
            Key::Char('d') if k.ctrl() && self.input.text.is_empty() => {
                ctl.quit();
                return;
            }
            Key::Tab => {
                self.complete();
                return;
            }
            Key::Up => {
                if self.history.is_empty() {
                    return;
                }
                let pos = match self.hist_pos {
                    None => {
                        self.draft = self.input.text.clone();
                        self.history.len() - 1
                    }
                    Some(p) => p.saturating_sub(1),
                };
                self.hist_pos = Some(pos);
                let h = self.history[pos].clone();
                self.input.set(&h);
                return;
            }
            Key::Down => {
                match self.hist_pos {
                    Some(p) if p + 1 < self.history.len() => {
                        self.hist_pos = Some(p + 1);
                        let h = self.history[p + 1].clone();
                        self.input.set(&h);
                    }
                    Some(_) => {
                        self.hist_pos = None;
                        let d = self.draft.clone();
                        self.input.set(&d);
                    }
                    None => {}
                }
                return;
            }
            _ => {}
        }
        match self.input.handle(k) {
            InputResult::Submit => {
                self.submit();
                if self.exit {
                    ctl.quit();
                }
            }
            InputResult::Cancel => self.input.clear(),
            InputResult::Changed => self.scroll = 0,
            _ => {}
        }
    }
}

impl App for Terminal {
    fn draw(&mut self, buf: &mut Buffer) {
        let area = buf.area;
        self.page_cols = area.w.saturating_sub(2);
        let (body, footer) = area.take_bottom(1);
        let (header, body) = body.take_top(1);
        let body = body.inset(1, 0);

        widget::pill(buf, header.x + 1, header.y, &format!("{} terminal", icons::TERMINAL), p::CRUST, if self.focused { p::MAUVE } else { p::SURFACE2 });
        buf.text(header.x + 15, header.y, &self.cwd, header.w.saturating_sub(30), Style::new().fg(p::OVERLAY1));
        let (h, m, _) = inkui::app::clock();
        let clock = format!("{} {:02}:{:02}", icons::CLOCK, h, m);
        widget::pill(buf, header.right().saturating_sub(10), header.y, &clock, p::CRUST, p::BLUE);

        let mut rows: Vec<Line> = Vec::new();
        for l in &self.out {
            rows.extend(widget::wrap_line(l, body.w));
        }
        let prompt = self.prompt();
        let pw = prompt.width();
        let total = rows.len() + 1;
        self.page = body.h.max(1);
        let max_scroll = total.saturating_sub(body.h);
        self.scroll = self.scroll.min(max_scroll);
        let first = total.saturating_sub(body.h + self.scroll);
        for (i, l) in rows.iter().enumerate().skip(first).take(body.h) {
            buf.line(body.x, body.y + i - first, l, body.w);
        }
        let prompt_row = total - 1;
        if prompt_row >= first && prompt_row < first + body.h {
            let y = body.y + prompt_row - first;
            buf.line(body.x, y, &prompt, body.w);
            let input_area = Rect::new(body.x + pw, y, body.w.saturating_sub(pw), 1);
            self.input.render(buf, input_area, Style::new(), self.focused, "");
        }
        if self.scroll > 0 {
            let tag = format!(" \u{2191} {} more ", self.scroll);
            let w = tag.chars().count();
            buf.text(body.right().saturating_sub(w), body.y, &tag, w, Style::new().fg(p::CRUST).bg(p::YELLOW));
        }

        buf.fill(footer, ' ', Style::new().bg(with_alpha(p::CRUST, 0x60)));
        let hints: &[(&str, &str)] = if self.child.is_some() {
            &[("Enter", "send line"), ("Ctrl + C", "stop"), ("Ctrl + D", "end input"), ("Win + \u{2191}\u{2193}", "scroll")]
        } else {
            &[("Win", "terminal"), ("Win + \u{2190}\u{2192}", "desktops"), ("Win + Q", "close"), ("Tab", "complete"), ("Win + \u{2191}\u{2193}", "scroll")]
        };
        widget::hints(buf, footer.inset(1, 0), hints);
    }

    fn event(&mut self, ev: &Event, ctl: &mut Control) {
        match ev {
            Event::Key(k) => self.key(k, ctl),
            Event::Focus(f) => {
                self.focused = *f;
                if *f {
                    // Pick up edits made with config terminal.
                    self.cfg = inkui::config::load("terminal");
                }
            }
            Event::Resize(..) => {}
        }
    }

    fn tick(&mut self, ctl: &mut Control) {
        if self.child.is_some() {
            if self.pump_child() {
                self.scroll = 0;
                ctl.redraw();
            }
            let done = self.child.as_ref().map_or(false, |c| !c.pipe.alive());
            if done {
                self.pump_child();
                self.end_child();
                ctl.redraw();
            }
            return;
        }
        ctl.redraw(); // the clock
    }

    fn tick_ms(&self) -> u64 {
        if self.child.is_some() { 30 } else { 15_000 }
    }
}

fn main() {
    let mut t = Terminal::new();
    // terminal <dir> starts there (the explorer's "terminal here").
    if let Some(dir) = inkui::args().first() {
        t.cd(dir);
    }
    inkui::run(t);
}
