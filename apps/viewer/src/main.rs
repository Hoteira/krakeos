// Image viewer: PNGs drawn with half-block characters, two pixels per cell
// (top pixel as the character color, bottom one as the background).

use std::fs::File;
use std::io::Read;

use inkui::buffer::{Buffer, Line};
use inkui::event::{Event, Key, KeyEvent};
use inkui::icons;
use inkui::layout::Rect;
use inkui::types::{palette as p, Style};
use inkui::widget::{self, Block, List, ListState};
use inkui::{App, Control};

struct Image {
    w: usize,
    h: usize,
    // RGBA.
    px: Vec<[u8; 4]>,
}

fn load_png(path: &str) -> Result<Image, String> {
    let mut data = Vec::new();
    File::open(path).and_then(|mut f| f.read_to_end(&mut data)).map_err(|_| "cannot read the file".to_string())?;
    let mut dec = png::Decoder::new(data.as_slice());
    dec.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = dec.read_info().map_err(|e| format!("not a PNG ({})", e))?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(|e| format!("bad PNG ({})", e))?;
    let (w, h) = (info.width as usize, info.height as usize);
    let ch = match info.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        png::ColorType::GrayscaleAlpha => 2,
        _ => 1,
    };
    let px = (0..w * h)
        .map(|i| {
            let s = &buf[i * ch..i * ch + ch];
            match ch {
                4 => [s[0], s[1], s[2], s[3]],
                3 => [s[0], s[1], s[2], 255],
                2 => [s[0], s[0], s[0], s[1]],
                _ => [s[0], s[0], s[0], 255],
            }
        })
        .collect();
    Ok(Image { w, h, px })
}

// Height of half a cell relative to its width, from the kernel's cell size.
fn half_cell_aspect() -> f32 {
    let mut b = [0u8; 28];
    if let Ok(mut f) = File::open("/dev/gpu/info") {
        if f.read(&mut b).unwrap_or(0) == 28 {
            let cw = u32::from_le_bytes([b[12], b[13], b[14], b[15]]) as f32;
            let ch = u32::from_le_bytes([b[16], b[17], b[18], b[19]]) as f32;
            if cw > 0.0 {
                return ch / 2.0 / cw;
            }
        }
    }
    1.0
}

struct View {
    path: String,
    img: Image,
    // Image pixels per cell column; smaller is more zoomed in.
    scale: f32,
    fit: bool,
    // Image coordinate at the view's center.
    cx: f32,
    cy: f32,
    aspect: f32,
    // Rendered cells (top, bottom colors) for the last area and view.
    cache: Vec<(u32, u32)>,
    cache_key: (Rect, u32, u32, u32),
}

impl View {
    fn fit_scale(&self, area: Rect) -> f32 {
        let sx = self.img.w as f32 / area.w.max(1) as f32;
        let sy = self.img.h as f32 / (area.h.max(1) as f32 * 2.0) / self.aspect;
        sx.max(sy).max(1e-3)
    }

    // Average the image over a source rect, blended over a dark backdrop.
    fn sample(&self, x0: f32, y0: f32, x1: f32, y1: f32) -> Option<u32> {
        let img = &self.img;
        if x1 <= 0.0 || y1 <= 0.0 || x0 >= img.w as f32 || y0 >= img.h as f32 {
            return None;
        }
        let (ix0, iy0) = (x0.max(0.0) as usize, y0.max(0.0) as usize);
        let ix1 = (x1.ceil() as usize).clamp(ix0 + 1, img.w);
        let iy1 = (y1.ceil() as usize).clamp(iy0 + 1, img.h);
        // Cap the work per cell when zoomed far out.
        let step_x = ((ix1 - ix0) / 6).max(1);
        let step_y = ((iy1 - iy0) / 6).max(1);
        let (mut r, mut g, mut b, mut n) = (0u32, 0u32, 0u32, 0u32);
        let mut y = iy0;
        while y < iy1 {
            let mut x = ix0;
            while x < ix1 {
                let [pr, pg, pb, pa] = img.px[y * img.w + x];
                let a = pa as u32;
                // Over Catppuccin crust.
                r += (pr as u32 * a + 0x11 * (255 - a)) / 255;
                g += (pg as u32 * a + 0x11 * (255 - a)) / 255;
                b += (pb as u32 * a + 0x1B * (255 - a)) / 255;
                n += 1;
                x += step_x;
            }
            y += step_y;
        }
        Some(0xFF00_0000 | (r / n) << 16 | (g / n) << 8 | (b / n))
    }

    fn render(&mut self, buf: &mut Buffer, area: Rect) {
        if self.fit {
            self.scale = self.fit_scale(area);
            self.cx = self.img.w as f32 / 2.0;
            self.cy = self.img.h as f32 / 2.0;
        }
        let key = (area, self.scale.to_bits(), self.cx.to_bits(), self.cy.to_bits());
        if key != self.cache_key {
            self.cache_key = key;
            self.cache.clear();
            let sx = self.scale;
            let sy = self.scale * self.aspect;
            let left = self.cx - area.w as f32 * sx / 2.0;
            let top = self.cy - area.h as f32 * sy;
            for row in 0..area.h {
                for col in 0..area.w {
                    let x0 = left + col as f32 * sx;
                    let y0 = top + row as f32 * 2.0 * sy;
                    let t = self.sample(x0, y0, x0 + sx, y0 + sy).unwrap_or(0);
                    let b = self.sample(x0, y0 + sy, x0 + sx, y0 + 2.0 * sy).unwrap_or(0);
                    self.cache.push((t, b));
                }
            }
        }
        for row in 0..area.h {
            for col in 0..area.w {
                let (ch, st) = match self.cache[row * area.w + col] {
                    (0, 0) => continue,
                    (t, 0) => ('\u{2580}', Style::new().fg(t)),
                    (0, b) => ('\u{2584}', Style::new().fg(b)),
                    (t, b) => ('\u{2580}', Style::new().fg(t).bg(b)),
                };
                buf.set(area.x + col, area.y + row, ch, st);
            }
        }
    }

    fn zoom(&mut self, factor: f32, area: Rect) {
        if self.fit {
            self.scale = self.fit_scale(area);
            self.fit = false;
        }
        self.scale = (self.scale * factor).clamp(0.05, 64.0);
    }

    fn pan(&mut self, dx: f32, dy: f32, area: Rect) {
        if self.fit {
            return;
        }
        self.cx = (self.cx + dx * area.w as f32 * self.scale).clamp(0.0, self.img.w as f32);
        self.cy = (self.cy + dy * area.h as f32 * 2.0 * self.scale * self.aspect).clamp(0.0, self.img.h as f32);
    }
}

enum Mode {
    Pick { files: Vec<String>, state: ListState, error: Option<String> },
    View(View),
}

struct Viewer {
    mode: Mode,
    last_area: Rect,
    focused: bool,
}

fn find_pngs(dir: &str, out: &mut Vec<String>, depth: usize) {
    let Ok(entries) = icons::list_dir(dir) else { return };
    for e in entries {
        let path = icons::resolve(dir, &e.name);
        if e.is_dir && depth < 4 {
            find_pngs(&path, out, depth + 1);
        } else if e.name.to_ascii_lowercase().ends_with(".png") {
            out.push(path);
        }
    }
}

impl Viewer {
    fn open(&mut self, path: &str) {
        match load_png(path) {
            Ok(img) => {
                self.mode = Mode::View(View {
                    path: path.to_string(),
                    cx: img.w as f32 / 2.0,
                    cy: img.h as f32 / 2.0,
                    img,
                    scale: 1.0,
                    fit: true,
                    aspect: half_cell_aspect(),
                    cache: Vec::new(),
                    cache_key: (Rect::default(), 0, 0, 0),
                })
            }
            Err(e) => {
                let mut files = Vec::new();
                find_pngs("/", &mut files, 0);
                self.mode = Mode::Pick { files, state: ListState::default(), error: Some(format!("{}: {}", icons::file_name(path), e)) };
            }
        }
    }

    fn key(&mut self, k: &KeyEvent, ctl: &mut Control) {
        let area = self.last_area;
        match &mut self.mode {
            Mode::Pick { files, state, .. } => match k.key {
                Key::Up | Key::Char('k') => state.prev(),
                Key::Down | Key::Char('j') => state.next(files.len()),
                Key::Enter => {
                    if let Some(f) = files.get(state.selected).cloned() {
                        self.open(&f);
                    }
                }
                Key::Char('q') | Key::Esc => ctl.quit(),
                _ => {}
            },
            Mode::View(v) => match k.key {
                Key::Char('+') | Key::Char('=') => v.zoom(0.8, area),
                Key::Char('-') => v.zoom(1.25, area),
                Key::Char('f') | Key::Char('0') => v.fit = true,
                Key::Char('1') => {
                    v.fit = false;
                    v.scale = 1.0;
                }
                Key::Left | Key::Char('h') => v.pan(-0.2, 0.0, area),
                Key::Right | Key::Char('l') => v.pan(0.2, 0.0, area),
                Key::Up | Key::Char('k') => v.pan(0.0, -0.2, area),
                Key::Down | Key::Char('j') => v.pan(0.0, 0.2, area),
                Key::Char('q') | Key::Esc => ctl.quit(),
                _ => {}
            },
        }
    }
}

impl App for Viewer {
    fn draw(&mut self, buf: &mut Buffer) {
        let area = buf.area;
        let (header, rest) = area.take_top(1);
        let (body, hint_row) = rest.take_bottom(1);
        widget::pill(buf, header.x + 1, header.y, &format!("{} viewer", icons::PICTURE), p::CRUST, p::MAUVE);
        match &mut self.mode {
            Mode::Pick { files, state, error } => {
                let inner = Block::new()
                    .title(Line::styled("choose an image", Style::new().fg(p::MAUVE).bold()))
                    .focused(self.focused)
                    .render(buf, body.center(body.w.min(70), body.h.min(files.len() + 4).max(6)));
                let mut list_area = inner;
                if let Some(e) = error {
                    buf.text(inner.x, inner.y, &format!("{} {}", icons::WARNING, e), inner.w, Style::new().fg(p::RED));
                    list_area = inner.take_top(2).1;
                }
                let items: Vec<Line> = files
                    .iter()
                    .map(|f| Line::new().push(format!("{} ", icons::FILE_IMAGE), Style::new().fg(p::MAUVE)).push(f.clone(), Style::new()))
                    .collect();
                if items.is_empty() {
                    buf.text(list_area.x, list_area.y, "no .png files found", list_area.w, Style::new().fg(p::OVERLAY0));
                }
                List::new(&items).focused(self.focused).render(buf, list_area, state);
                widget::hints(buf, hint_row.inset(1, 0), &[("\u{2191}\u{2193}", "choose"), ("Enter", "open"), ("q", "quit")]);
            }
            Mode::View(v) => {
                let name = icons::file_name(&v.path).to_string();
                let zoom = if v.fit { "fit".to_string() } else { format!("{:.0}%", 100.0 / v.scale) };
                let inner = Block::new()
                    .title(Line::styled(name, Style::new().fg(p::TEXT).bold()))
                    .title_right(Line::styled(format!("{} x {}  {}", v.img.w, v.img.h, zoom), Style::new().fg(p::LAVENDER)))
                    .focused(self.focused)
                    .padding(0, 0)
                    .render(buf, body);
                self.last_area = inner;
                v.render(buf, inner);
                widget::hints(
                    buf,
                    hint_row.inset(1, 0),
                    &[("+ -", "zoom"), ("\u{2190}\u{2191}\u{2192}\u{2193}", "pan"), ("f", "fit"), ("1", "100%"), ("q", "quit")],
                );
            }
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

fn main() {
    let mut v = Viewer {
        mode: Mode::Pick { files: Vec::new(), state: ListState::default(), error: None },
        last_area: Rect::default(),
        focused: true,
    };
    match inkui::args().first() {
        Some(path) => v.open(path),
        None => {
            let mut files = Vec::new();
            find_pngs("/", &mut files, 0);
            v.mode = Mode::Pick { files, state: ListState::default(), error: None };
        }
    }
    inkui::run(v);
}
