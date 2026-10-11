// The app loop, plus starting other apps.

use std::time::{Duration, Instant};

use crate::buffer::Buffer;
use crate::event::Event;
use crate::term::Term;

// What an app asks of the loop.
#[derive(Default)]
pub struct Control {
    quit: bool,
    redraw: bool,
}

impl Control {
    // Exit after this event; the desktop closes.
    pub fn quit(&mut self) {
        self.quit = true;
    }
    // Draw again even though no event arrived (after a tick).
    pub fn redraw(&mut self) {
        self.redraw = true;
    }
}

pub trait App {
    // Draw the whole UI. The buffer starts blank every frame.
    fn draw(&mut self, buf: &mut Buffer);
    // Handle one event. Every event is followed by a redraw.
    fn event(&mut self, ev: &Event, ctl: &mut Control);
    // Win + Q asked the app to close. Quits by default; an app with
    // unsaved work can ask first instead. A second Win + Q kills it.
    fn quit_requested(&mut self, ctl: &mut Control) {
        ctl.quit();
    }
    // Called every tick_ms when that is non-zero.
    fn tick(&mut self, _ctl: &mut Control) {}
    fn tick_ms(&self) -> u64 {
        0
    }
}

// Open the app's desktop and run until it quits.
pub fn run<A: App>(mut app: A) {
    let Ok(mut term) = Term::open() else {
        eprintln!("inkui: cannot open /dev/tty");
        return;
    };
    crate::event::set_layout(&crate::config::system().keyboard.layout);
    let mut ctl = Control::default();
    let mut dirty = true;
    let mut last_tick = Instant::now();
    loop {
        for ev in term.poll() {
            app.event(&ev, &mut ctl);
            dirty = true;
        }
        if term.quit_requested {
            term.quit_requested = false;
            app.quit_requested(&mut ctl);
            dirty = true;
        }
        if ctl.quit {
            break;
        }
        let tick = app.tick_ms();
        if tick > 0 && last_tick.elapsed() >= Duration::from_millis(tick) {
            last_tick = Instant::now();
            app.tick(&mut ctl);
        }
        if dirty || ctl.redraw {
            term.buf.clear();
            app.draw(&mut term.buf);
            term.present();
            dirty = false;
            ctl.redraw = false;
        }
        if ctl.quit {
            break;
        }
        // Key presses wake the app early.
        let wait = if tick > 0 {
            Duration::from_millis(tick).saturating_sub(last_tick.elapsed())
        } else {
            Duration::from_millis(1000)
        };
        std::thread::sleep(wait.max(Duration::from_millis(1)));
    }
}

// The app's arguments, without the program path.
pub fn args() -> Vec<String> {
    std::env::args().skip(1).collect()
}

// Start another app on its own desktop. path is the .wasm file.
pub fn spawn(path: &str, args: &[&str]) -> bool {
    let mut req = format!("/spawn:{}", if path.starts_with('/') { path.to_string() } else { format!("/{}", path) });
    for a in args {
        req.push('\t');
        req.push_str(a);
    }
    std::fs::File::open(req).is_ok()
}

// A pipe to a console app: what it prints comes out of read, and what
// is written goes to its stdin.
pub struct Pipe {
    file: std::fs::File,
}

impl Pipe {
    pub fn open() -> Option<Pipe> {
        let file = std::fs::OpenOptions::new().read(true).write(true).open("/dev/pipe").ok()?;
        Some(Pipe { file })
    }

    // Start path with its stdin, stdout and stderr on this pipe.
    pub fn spawn(&self, path: &str, args: &[&str]) -> bool {
        use std::os::fd::AsRawFd;
        let mut req = format!("/spawn:{}", if path.starts_with('/') { path.to_string() } else { format!("/{}", path) });
        for a in args {
            req.push('\t');
            req.push_str(a);
        }
        // The runner attaches the app to pipe <id> (see wasm_runner).
        req.push('\x1d');
        req.push_str(&(self.file.as_raw_fd() as u32 - 0x6000_0000).to_string());
        std::fs::File::open(req).is_ok()
    }

    // What the app printed since the last call (empty if nothing).
    pub fn read(&mut self) -> Vec<u8> {
        use std::io::Read;
        let mut out = Vec::new();
        let mut buf = [0u8; 4096];
        while let Ok(n) = self.file.read(&mut buf) {
            if n == 0 {
                break;
            }
            out.extend_from_slice(&buf[..n]);
        }
        out
    }

    // Send input to the app; an empty write is end-of-input (Ctrl + D).
    pub fn write(&mut self, data: &[u8]) {
        use std::io::Write;
        let _ = self.file.write(data);
    }

    // Whether the app is still running (or has output left to read).
    pub fn alive(&self) -> bool {
        self.file.metadata().map_or(false, |m| m.len() != 0)
    }
}

// /apps/<name>.wasm if it exists.
pub fn app_path(name: &str) -> Option<String> {
    let p = format!("/apps/{}.wasm", name);
    std::fs::File::open(&p).ok().map(|_| p)
}

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "krake")]
unsafe extern "C" {
    #[link_name = "kill"]
    fn krake_kill(tid: i32) -> i32;
}

#[cfg(not(target_arch = "wasm32"))]
unsafe fn krake_kill(_tid: i32) -> i32 {
    -1
}

// Stop an app by thread id. False if there's no such app or it can't be
// stopped (the shell and runner threads).
pub fn kill(tid: usize) -> bool {
    unsafe { krake_kill(tid as i32) == 0 }
}

// The kernel's process table: "TID STATE MEM NAME" lines, then free RAM.
pub fn processes() -> String {
    let mut s = String::new();
    if let Ok(mut f) = std::fs::File::open("/dev/system/procs") {
        let _ = std::io::Read::read_to_string(&mut f, &mut s);
    }
    s
}

// Wall-clock time as (hours, minutes, seconds), UTC.
pub fn clock() -> (u64, u64, u64) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    ((secs / 3600) % 24, (secs / 60) % 60, secs % 60)
}
