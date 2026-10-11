// The app's desktop, /dev/tty. Writes are [x, y, w, h] u32 headers (in cells) then
// w*h cells of [ch | attrs, fg, bg] u32; reads are 16-byte events [kind, a, b, c].

use std::fs::File;
use std::io::{Read, Write};

use crate::buffer::{Buffer, Cell};
use crate::event::{decode, Event};

const EV_RESIZE: u32 = 1;
const EV_KEY: u32 = 5;
const EV_FOCUS: u32 = 6;
const EV_QUIT: u32 = 7;

pub struct Term {
    file: File,
    // What the desktop currently shows.
    front: Vec<Cell>,
    // The frame being drawn.
    pub buf: Buffer,
    out: Vec<u8>,
    // Win+Q asked the app to quit (cleared by whoever handles it).
    pub quit_requested: bool,
}

impl Term {
    // Create this app's desktop. The kernel slides the view to it.
    pub fn open() -> std::io::Result<Term> {
        let file = File::options().read(true).write(true).open("/dev/tty")?;
        let mut t = Term { file, front: Vec::new(), buf: Buffer::new(0, 0), out: Vec::new(), quit_requested: false };
        // The first event is always the grid size.
        for _ in 0..50 {
            for ev in t.poll() {
                if let Event::Resize(..) = ev {
                    return Ok(t);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        Ok(t)
    }

    pub fn cols(&self) -> usize {
        self.buf.area.w
    }

    pub fn rows(&self) -> usize {
        self.buf.area.h
    }

    fn resize(&mut self, cols: usize, rows: usize) {
        self.buf.resize(cols, rows);
        // Unknown contents: send everything next frame.
        self.front = vec![Cell { ch: '\u{0}', ..Cell::BLANK }; cols * rows];
    }

    // Pending events, without waiting.
    pub fn poll(&mut self) -> Vec<Event> {
        let mut out = Vec::new();
        let mut raw = [0u8; 16 * 64];
        loop {
            let n = self.file.read(&mut raw).unwrap_or(0);
            for ev in raw[..n].chunks_exact(16) {
                let w = |i: usize| u32::from_le_bytes([ev[i * 4], ev[i * 4 + 1], ev[i * 4 + 2], ev[i * 4 + 3]]);
                match w(0) {
                    EV_RESIZE => {
                        let (c, r) = (w(1) as usize, w(2) as usize);
                        self.resize(c, r);
                        out.push(Event::Resize(c, r));
                    }
                    EV_KEY if w(2) != 0 => {
                        if let Some(k) = decode(w(1) as u16, w(3)) {
                            out.push(Event::Key(k));
                        }
                    }
                    EV_FOCUS => out.push(Event::Focus(w(1) != 0)),
                    EV_QUIT => self.quit_requested = true,
                    _ => {}
                }
            }
            if n < raw.len() {
                break;
            }
        }
        out
    }

    // Send the cells that changed since the last present.
    pub fn present(&mut self) {
        let w = self.buf.area.w;
        self.out.clear();
        for y in 0..self.buf.area.h {
            let row = y * w;
            let mut x = 0;
            while x < w {
                if self.buf.cells[row + x] == self.front[row + x] {
                    x += 1;
                    continue;
                }
                // Extend the span over short unchanged gaps; a record
                // header costs as much as a cell and a third.
                let start = x;
                let mut end = x + 1;
                let mut gap = 0;
                let mut i = x + 1;
                while i < w && gap <= 2 {
                    if self.buf.cells[row + i] != self.front[row + i] {
                        end = i + 1;
                        gap = 0;
                    } else {
                        gap += 1;
                    }
                    i += 1;
                }
                for v in [start as u32, y as u32, (end - start) as u32, 1] {
                    self.out.extend_from_slice(&v.to_le_bytes());
                }
                for c in &self.buf.cells[row + start..row + end] {
                    let ch = c.ch as u32 | c.attrs;
                    for v in [ch, c.fg, c.bg] {
                        self.out.extend_from_slice(&v.to_le_bytes());
                    }
                }
                self.front[row + start..row + end].copy_from_slice(&self.buf.cells[row + start..row + end]);
                x = end;
            }
        }
        if !self.out.is_empty() {
            let _ = self.file.write_all(&self.out);
        }
    }
}
