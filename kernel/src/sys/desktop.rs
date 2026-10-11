// Desktops: one full-screen character grid per app that opens /dev/tty; position 0 is home.
// Runs under the big kernel lock, so no extra locking. Integer math only: the FPU is lazily switched.

use alloc::collections::VecDeque;
use alloc::vec;
use alloc::vec::Vec;

use crate::drivers::virtio_gpu as gpu;

pub const TTY_FD_BASE: usize = 0x5000_0000;
const MAX_DESKTOPS: usize = 12;

// ch: codepoint in the low 21 bits plus attribute bits. Colors are 0xAARRGGBB;
// bg alpha 0 means the translucent default (glass), other alphas blend over it.
#[repr(C)]
#[derive(Clone, Copy, PartialEq)]
pub struct Cell {
    pub ch: u32,
    pub fg: u32,
    pub bg: u32,
}

pub const CELL_BYTES: usize = 12;
pub const ATTR_BOLD: u32 = 1 << 24;
pub const ATTR_UNDERLINE: u32 = 1 << 25;
const CP_MASK: u32 = 0x1F_FFFF;
const BLANK: Cell = Cell { ch: b' ' as u32, fg: 0xFFCD_D6F4, bg: 0 };

// Events read from a tty fd: 16 bytes, [kind u32][a i32][b i32][c u32].
pub const EV_RESIZE: u32 = 1; // a = cols, b = rows
pub const EV_KEY: u32 = 5; // a = key code, b = 0 up / 1 down / 2 repeat, c = mods
pub const EV_FOCUS: u32 = 6; // a = 1 gained, 0 lost
pub const EV_QUIT: u32 = 7; // Win+Q: please quit (the app may ask first)

const GAP: usize = 10;
const BORDER: usize = 2;
const RADIUS: usize = 12;
const PAD: usize = 8;
const GLASS_FALLBACK: u32 = 0xFF1E_1E2E;
const BORDER_A: u32 = 0xFF89_B4FA; // gradient top left
const BORDER_B: u32 = 0xFFCB_A6F7; // gradient bottom right
const DOT_ON: u32 = 0xFF89_B4FA;
const DOT_OFF: u32 = 0xFF6C_7086;

const SLIDE_TICKS: u64 = 2_200_000; // 220 ms at the 10 MHz timebase
const FRAME_TICKS: u64 = 120_000;
const DAMAGE_TICKS: u64 = 60_000;

struct Desktop {
    id: usize,
    owner: usize,
    cols: usize,
    rows: usize,
    cells: Vec<Cell>,
    // Panel image (panel_w * panel_h): frame, glass and drawn cells.
    pix: Vec<u32>,
    events: VecDeque<[u32; 4]>,
    // Win+Q sent and no key since; a second Win+Q kills the app.
    quit_sent: bool,
}

struct Atlas {
    cell_w: usize,
    cell_h: usize,
    codepoints: Vec<u32>,
    styles: usize,
    bitmaps: Vec<u8>,
    ascii: [u16; 128],
    fallback: u16,
}

impl Atlas {
    fn index(&self, cp: u32) -> Option<usize> {
        if cp < 128 {
            let i = self.ascii[cp as usize];
            return if i == u16::MAX { None } else { Some(i as usize) };
        }
        self.codepoints.binary_search(&cp).ok()
    }

    fn glyph(&self, cp: u32, bold: bool) -> Option<&[u8]> {
        let i = match self.index(cp) {
            Some(i) => i,
            None if self.fallback != u16::MAX => self.fallback as usize,
            None => return None,
        };
        let style = if bold && self.styles > 1 { 1 } else { 0 };
        let n = self.cell_w * self.cell_h;
        let off = (style * self.codepoints.len() + i) * n;
        self.bitmaps.get(off..off + n)
    }
}

struct Manager {
    desktops: Vec<Desktop>,
    next_id: usize,
    // Visible position: 0 = home, p = desktops[p - 1].
    current: usize,
    // Camera x along the strip of desktops (position p sits at p * width).
    cam: i64,
    anim_from: i64,
    anim_start: u64,
    animating: bool,
    w: usize,
    h: usize,
    wallpaper: Vec<u32>,
    glass: Vec<u32>,
    home: Vec<u32>,
    atlas: Option<Atlas>,
    // Background or atlas changed: redraw every panel before composing.
    restyle: bool,
    damage: Option<(usize, usize, usize, usize)>,
    last_frame: u64,
}

static mut DM: Manager = Manager {
    desktops: Vec::new(),
    next_id: 1,
    current: 0,
    cam: 0,
    anim_from: 0,
    anim_start: 0,
    animating: false,
    w: 0,
    h: 0,
    wallpaper: Vec::new(),
    glass: Vec::new(),
    home: Vec::new(),
    atlas: None,
    restyle: false,
    damage: None,
    last_frame: 0,
};

fn dm() -> &'static mut Manager {
    unsafe { &mut *core::ptr::addr_of_mut!(DM) }
}

#[inline]
fn div255(v: u32) -> u32 {
    (v + 1 + (v >> 8)) >> 8
}

// Mix b over a with weight t (0..=255).
#[inline]
fn mix(a: u32, b: u32, t: u32) -> u32 {
    if t == 0 {
        return a;
    }
    if t >= 255 {
        return b | 0xFF00_0000;
    }
    let it = 255 - t;
    let r = div255(((a >> 16) & 0xFF) * it + ((b >> 16) & 0xFF) * t);
    let g = div255(((a >> 8) & 0xFF) * it + ((b >> 8) & 0xFF) * t);
    let bl = div255((a & 0xFF) * it + (b & 0xFF) * t);
    0xFF00_0000 | (r << 16) | (g << 8) | bl
}

fn isqrt(n: u64) -> u64 {
    if n < 2 {
        return n;
    }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

// Coverage (0..=256) of pixel (px, py) by a w x h rounded rect, sampled at the pixel center.
fn rr_coverage(px: usize, py: usize, w: usize, h: usize, r: usize) -> u32 {
    if px >= w || py >= h {
        return 0;
    }
    let (r16, cx, cy) = ((r * 16) as i64, (px * 16 + 8) as i64, (py * 16 + 8) as i64);
    let (w16, h16) = ((w * 16) as i64, (h * 16) as i64);
    let dx = if cx < r16 { r16 - cx } else if cx > w16 - r16 { cx - (w16 - r16) } else { 0 };
    let dy = if cy < r16 { r16 - cy } else if cy > h16 - r16 { cy - (h16 - r16) } else { 0 };
    if dx == 0 || dy == 0 {
        return 256;
    }
    let d = isqrt((dx * dx + dy * dy) as u64) as i64;
    ((r16 - d + 8).clamp(0, 16) * 16) as u32
}

impl Manager {
    fn panel(&self) -> (usize, usize) {
        (self.w.saturating_sub(2 * GAP), self.h.saturating_sub(2 * GAP))
    }

    fn cell_size(&self) -> (usize, usize) {
        match &self.atlas {
            Some(a) => (a.cell_w, a.cell_h),
            None => (9, 19),
        }
    }

    fn grid(&self) -> (usize, usize) {
        let (pw, ph) = self.panel();
        let (cw, ch) = self.cell_size();
        let inset = 2 * (BORDER + PAD);
        ((pw.saturating_sub(inset) / cw).max(1), (ph.saturating_sub(inset) / ch).max(1))
    }

    fn grid_origin(&self, cols: usize, rows: usize) -> (usize, usize) {
        let (pw, ph) = self.panel();
        let (cw, ch) = self.cell_size();
        ((pw.saturating_sub(cols * cw)) / 2, (ph.saturating_sub(rows * ch)) / 2)
    }

    fn glass_at(&self, x: usize, y: usize) -> u32 {
        self.glass.get(y * self.w + x).copied().unwrap_or(GLASS_FALLBACK)
    }

    fn wall_at(&self, x: usize, y: usize) -> u32 {
        self.wallpaper.get(y * self.w + x).copied().unwrap_or(0xFF11_111B)
    }

    fn find(&self, id: usize) -> Option<usize> {
        self.desktops.iter().position(|d| d.id == id)
    }
}

// Rounded corners showing the wallpaper, a gradient border and glass inside.
fn draw_frame(m: &Manager, pix: &mut [u32]) {
    let (pw, ph) = m.panel();
    if pix.len() < pw * ph {
        return;
    }
    let span = (pw + ph).max(1) as u32;
    for y in 0..ph {
        let edge_y = y < RADIUS + 1 || y + RADIUS + 1 >= ph;
        for x in 0..pw {
            let near_edge = edge_y || x < RADIUS + 1 || x + RADIUS + 1 >= pw;
            let glass = m.glass_at(GAP + x, GAP + y);
            if !near_edge {
                pix[y * pw + x] = glass;
                continue;
            }
            let outer = rr_coverage(x, y, pw, ph, RADIUS);
            let inner = if x >= BORDER && y >= BORDER {
                rr_coverage(x - BORDER, y - BORDER, pw - 2 * BORDER, ph - 2 * BORDER, RADIUS - BORDER)
            } else {
                0
            };
            let t = ((x + y) as u32 * 255) / span;
            let border = mix(BORDER_A, BORDER_B, t);
            let ring = mix(border, glass, (inner * 255) >> 8);
            pix[y * pw + x] = mix(m.wall_at(GAP + x, GAP + y), ring, (outer * 255) >> 8);
        }
    }
}

fn draw_cell(m: &Manager, d: &mut Desktop, col: usize, row: usize) {
    let (pw, _) = m.panel();
    let (cw, chh) = m.cell_size();
    let (ox, oy) = m.grid_origin(d.cols, d.rows);
    let c = d.cells[row * d.cols + col];
    let px0 = ox + col * cw;
    let py0 = oy + row * chh;
    let cp = c.ch & CP_MASK;
    let glyph = if cp == b' ' as u32 || cp == 0 {
        None
    } else {
        m.atlas.as_ref().and_then(|a| a.glyph(cp, c.ch & ATTR_BOLD != 0))
    };
    let bg_a = c.bg >> 24;
    let underline = c.ch & ATTR_UNDERLINE != 0;
    for y in 0..chh {
        let row_off = (py0 + y) * pw + px0;
        let Some(dst) = d.pix.get_mut(row_off..row_off + cw) else { return };
        for x in 0..cw {
            let base = if bg_a == 255 {
                c.bg
            } else {
                let g = m.glass_at(GAP + px0 + x, GAP + py0 + y);
                mix(g, c.bg, bg_a)
            };
            let mut a = match glyph {
                Some(gl) => gl[y * cw + x] as u32,
                None => 0,
            };
            if underline && y == chh - 2 {
                a = 255;
            }
            dst[x] = mix(base, c.fg, a);
        }
    }
}

fn redraw_desktop(m: &Manager, d: &mut Desktop) {
    let (pw, ph) = m.panel();
    if d.pix.len() != pw * ph {
        d.pix = vec![0; pw * ph];
    }
    draw_frame(m, &mut d.pix);
    for row in 0..d.rows {
        for col in 0..d.cols {
            draw_cell(m, d, col, row);
        }
    }
}

fn draw_text(m: &Manager, buf: &mut [u32], x0: usize, y0: usize, text: &str, fg: u32, bold: bool) {
    let Some(atlas) = &m.atlas else { return };
    let (cw, chh) = (atlas.cell_w, atlas.cell_h);
    for (i, ch) in text.chars().enumerate() {
        if ch == ' ' {
            continue;
        }
        let Some(gl) = atlas.glyph(ch as u32, bold) else { continue };
        for y in 0..chh {
            for x in 0..cw {
                let (sx, sy) = (x0 + i * cw + x, y0 + y);
                if sx < m.w && sy < m.h {
                    let p = &mut buf[sy * m.w + sx];
                    *p = mix(*p, fg, gl[y * cw + x] as u32);
                }
            }
        }
    }
}

// Home: the wallpaper plus a hint pill near the bottom.
fn redraw_home(m: &mut Manager) {
    let mut home = core::mem::take(&mut m.home);
    home.clear();
    home.extend_from_slice(&m.wallpaper);
    home.resize(m.w * m.h, 0xFF11_111B);
    if m.atlas.is_some() {
        let (cw, chh) = m.cell_size();
        let key = " Win ";
        let rest = "  open a terminal";
        let tw = (key.len() + rest.len()) * cw;
        let (bw, bh) = (tw + 4 * cw, chh * 2);
        let bx = m.w.saturating_sub(bw) / 2;
        let by = m.h.saturating_sub(bh + m.h / 8);
        for y in 0..bh {
            for x in 0..bw {
                let cov = rr_coverage(x, y, bw, bh, bh / 2);
                if cov == 0 {
                    continue;
                }
                let p = &mut home[(by + y) * m.w + bx + x];
                *p = mix(*p, 0xFF1E_1E2E, (cov * 200) >> 8);
            }
        }
        let tx = bx + 2 * cw;
        let ty = by + (bh - chh) / 2;
        for y in 0..chh {
            for x in 0..key.len() * cw {
                let cov = rr_coverage(x, y, key.len() * cw, chh, 5);
                let p = &mut home[(ty + y) * m.w + tx + x];
                *p = mix(*p, BORDER_A, (cov * 255) >> 8);
            }
        }
        draw_text(m, &mut home, tx, ty, key, 0xFF1E_1E2E, true);
        draw_text(m, &mut home, tx + key.len() * cw, ty, rest, 0xFFCD_D6F4, false);
    }
    m.home = home;
}

fn restyle_all(m: &mut Manager) {
    let mut desks = core::mem::take(&mut m.desktops);
    let (cols, rows) = m.grid();
    for d in desks.iter_mut() {
        if d.cols != cols || d.rows != rows {
            let mut cells = vec![BLANK; cols * rows];
            for r in 0..rows.min(d.rows) {
                for c in 0..cols.min(d.cols) {
                    cells[r * cols + c] = d.cells[r * d.cols + c];
                }
            }
            d.cells = cells;
            d.cols = cols;
            d.rows = rows;
            push_event(d, [EV_RESIZE, cols as u32, rows as u32, 0]);
        }
        redraw_desktop(m, d);
    }
    m.desktops = desks;
    redraw_home(m);
    m.restyle = false;
    damage_all(m);
}

fn expand_damage(m: &mut Manager, x: usize, y: usize, w: usize, h: usize) {
    let (x0, y0) = (x.min(m.w), y.min(m.h));
    let (x1, y1) = ((x + w).min(m.w), (y + h).min(m.h));
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    m.damage = Some(match m.damage {
        None => (x0, y0, x1, y1),
        Some((a, b, c, d)) => (a.min(x0), b.min(y0), c.max(x1), d.max(y1)),
    });
}

fn damage_all(m: &mut Manager) {
    m.damage = Some((0, 0, m.w, m.h));
}

// Positions past the last desktop show the wallpaper.
unsafe fn copy_row(m: &Manager, p: usize, y: usize, x: usize, len: usize, dst: *mut u32) {
    let w = m.w;
    let wall = m.wallpaper.as_ptr().add(y * w);
    if p == 0 {
        let src = if m.home.len() == w * m.h { m.home.as_ptr().add(y * w) } else { wall };
        core::ptr::copy_nonoverlapping(src.add(x), dst, len);
        return;
    }
    let (pw, ph) = m.panel();
    let d = match m.desktops.get(p - 1) {
        Some(d) if y >= GAP && y < GAP + ph && d.pix.len() == pw * ph => d,
        _ => {
            core::ptr::copy_nonoverlapping(wall.add(x), dst, len);
            return;
        }
    };
    let prow = d.pix.as_ptr().add((y - GAP) * pw);
    let end = x + len;
    let mut cx = x;
    while cx < end {
        let (seg_end, src) = if cx < GAP {
            (GAP.min(end), wall.add(cx))
        } else if cx < GAP + pw {
            ((GAP + pw).min(end), prow.add(cx - GAP))
        } else {
            (end, wall.add(cx))
        };
        core::ptr::copy_nonoverlapping(src, dst.add(cx - x), seg_end - cx);
        cx = seg_end;
    }
}

// Position dots in the bottom gap, one per desktop including home.
fn dots_rect(m: &Manager) -> (usize, usize, usize, usize) {
    let n = m.desktops.len() + 1;
    let w = n * 12 + 8;
    let h = 6;
    (m.w.saturating_sub(w) / 2, m.h.saturating_sub(GAP / 2 + h / 2 + 0), w, h)
}

unsafe fn draw_dots(m: &Manager, fb: *mut u32) {
    if m.desktops.is_empty() {
        return;
    }
    let (x0, y0, _, h) = dots_rect(m);
    let mut x = x0;
    for p in 0..=m.desktops.len() {
        let (dw, col) = if p == m.current { (14, DOT_ON) } else { (6, DOT_OFF) };
        for y in 0..h {
            for i in 0..dw {
                let cov = rr_coverage(i, y, dw, h, h / 2);
                if cov > 0 && x + i < m.w && y0 + y < m.h {
                    let px = fb.add((y0 + y) * m.w + x + i);
                    *px = mix(*px, col, (cov * 255) >> 8);
                }
            }
        }
        x += dw + 6;
    }
}

pub fn tick() {
    if !gpu::is_ready() {
        return;
    }
    if gpu::poll_resize() {
        on_mode_change();
    }
    let m = dm();
    if m.w == 0 {
        on_mode_change();
    }
    if m.restyle {
        restyle_all(m);
    }
    let now = crate::csr::read_time();
    if m.animating {
        if now.wrapping_sub(m.last_frame) < FRAME_TICKS {
            return;
        }
        let target = (m.current * m.w) as i64;
        let t = (now.wrapping_sub(m.anim_start) * 1024 / SLIDE_TICKS).min(1024) as i64;
        // Ease out cubic: 1 - (1 - t)^3.
        let inv = 1024 - t;
        let e = 1024 - inv * inv / 1024 * inv / 1024;
        m.cam = m.anim_from + (target - m.anim_from) * e / 1024;
        if t >= 1024 {
            m.cam = target;
            m.animating = false;
        }
        m.damage = Some((0, 0, m.w, m.h));
    } else if m.damage.is_none() || now.wrapping_sub(m.last_frame) < DAMAGE_TICKS {
        return;
    }
    m.last_frame = now;
    compose(m);
}

fn compose(m: &mut Manager) {
    let Some((mut x0, mut y0, mut x1, mut y1)) = m.damage.take() else { return };
    let (w, h) = (gpu::width() as usize, gpu::height() as usize);
    if w != m.w || h != m.h || m.wallpaper.len() != w * h {
        return;
    }
    // Dots are drawn over what is below them, so damage touching them must cover them whole.
    let (dx, dy, dw, dh) = dots_rect(m);
    if dx < x1 && dx + dw > x0 && dy < y1 && dy + dh > y0 {
        x0 = x0.min(dx);
        y0 = y0.min(dy);
        x1 = x1.max(dx + dw).min(w);
        y1 = y1.max(dy + dh).min(h);
    }
    let fb = gpu::fb_ptr();
    let cam = m.cam.max(0) as usize;
    unsafe {
        for y in y0..y1 {
            let mut x = x0;
            while x < x1 {
                let sx = cam + x;
                let (p, lx) = (sx / w, sx % w);
                let n = (w - lx).min(x1 - x);
                copy_row(m, p, y, lx, n, fb.add(y * w + x));
                x += n;
            }
        }
        draw_dots(m, fb);
    }
    gpu::flush_rect(x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32);
}

// Keeps the old layer up until the shell uploads one at the new size.
fn rescale(src: &[u32], sw: usize, sh: usize, dw: usize, dh: usize) -> Vec<u32> {
    if src.len() != sw * sh || sw == 0 || sh == 0 {
        return Vec::new();
    }
    let mut out = vec![0u32; dw * dh];
    for y in 0..dh {
        let sy = y * sh / dh;
        for x in 0..dw {
            out[y * dw + x] = src[sy * sw + x * sw / dw];
        }
    }
    out
}

fn on_mode_change() {
    let m = dm();
    let (w, h) = (gpu::width() as usize, gpu::height() as usize);
    if w == m.w && h == m.h {
        return;
    }
    m.wallpaper = rescale(&m.wallpaper, m.w, m.h, w, h);
    if m.wallpaper.len() != w * h {
        m.wallpaper = vec![0xFF11_111B; w * h];
    }
    m.glass = rescale(&m.glass, m.w, m.h, w, h);
    m.w = w;
    m.h = h;
    m.cam = (m.current * w) as i64;
    m.animating = false;
    m.restyle = true;
}

fn slide_to(m: &mut Manager, p: usize) {
    let p = p.min(m.desktops.len());
    if p == m.current && !m.animating {
        return;
    }
    if m.current != p {
        if m.current >= 1 {
            if let Some(d) = m.desktops.get_mut(m.current - 1) {
                push_event(d, [EV_FOCUS, 0, 0, 0]);
            }
        }
        if p >= 1 {
            push_event(&mut m.desktops[p - 1], [EV_FOCUS, 1, 0, 0]);
        }
    }
    m.current = p;
    m.anim_from = m.cam;
    m.anim_start = crate::csr::read_time();
    m.animating = true;
}

pub fn switch_left() {
    let m = dm();
    if m.current > 0 {
        slide_to(m, m.current - 1);
    }
}

pub fn switch_right() {
    let m = dm();
    slide_to(m, m.current + 1);
}

fn remove_at(m: &mut Manager, i: usize) {
    let p = i + 1;
    m.desktops.remove(i);
    if p < m.current {
        m.current -= 1;
        m.cam -= m.w as i64;
        if m.animating {
            m.anim_from -= m.w as i64;
        }
        damage_all(m);
    } else if p == m.current {
        // Glide over to the left neighbour.
        m.current = p - 1;
        if m.current >= 1 {
            push_event(&mut m.desktops[m.current - 1], [EV_FOCUS, 1, 0, 0]);
        }
        m.anim_from = m.cam;
        m.anim_start = crate::csr::read_time();
        m.animating = true;
    } else {
        damage_all(m); // the dots changed
    }
}

// Win+Q: close the visible app's desktop and stop the app.
pub fn close_current() {
    let m = dm();
    if m.current == 0 {
        return;
    }
    let d = &mut m.desktops[m.current - 1];
    if !d.quit_sent {
        // Ask nicely first; the app closes its desktop when it exits.
        d.quit_sent = true;
        push_event(d, [EV_QUIT, 0, 0, 0]);
        return;
    }
    let owner = d.owner;
    remove_at(m, m.current - 1);
    crate::sys::scheduler::Scheduler::kill(owner);
}

fn push_event(d: &mut Desktop, ev: [u32; 4]) {
    if d.events.len() < 256 {
        d.events.push_back(ev);
    }
    unsafe {
        let t = &mut crate::sys::scheduler::SCHEDULER.threads[d.owner];
        if let crate::sys::scheduler::ThreadState::Sleeping(_) = t.state {
            t.state = crate::sys::scheduler::ThreadState::Ready;
        }
    }
}

pub fn key(code: u16, value: u32, mods: u32) {
    let m = dm();
    if m.current >= 1 {
        if let Some(d) = m.desktops.get_mut(m.current - 1) {
            d.quit_sent = false;
            push_event(d, [EV_KEY, code as u32, value, mods]);
        }
    }
}

pub fn open_tty() -> Option<usize> {
    let m = dm();
    if m.desktops.len() >= MAX_DESKTOPS {
        return None;
    }
    let (cols, rows) = m.grid();
    let id = m.next_id;
    m.next_id += 1;
    let mut d = Desktop {
        id,
        owner: crate::sys::scheduler::current_tid(),
        cols,
        rows,
        cells: vec![BLANK; cols * rows],
        pix: Vec::new(),
        events: VecDeque::new(),
        quit_sent: false,
    };
    d.events.push_back([EV_RESIZE, cols as u32, rows as u32, 0]);
    redraw_desktop(m, &mut d);
    m.desktops.push(d);
    let p = m.desktops.len();
    slide_to(m, p);
    Some(TTY_FD_BASE + id)
}

fn owned(fd: usize) -> Option<usize> {
    let m = dm();
    let i = m.find(fd - TTY_FD_BASE)?;
    if m.desktops[i].owner != crate::sys::scheduler::current_tid() {
        return None;
    }
    Some(i)
}

// Records: [x, y, w, h] (u32, in cells) then w*h 12-byte cells, repeated.
pub fn write_tty(fd: usize, data: &[u8]) -> usize {
    let Some(i) = owned(fd) else { return 0 };
    let m = dm();
    let mut d = core::mem::replace(&mut m.desktops[i], Desktop {
        id: 0,
        owner: 0,
        cols: 0,
        rows: 0,
        cells: Vec::new(),
        pix: Vec::new(),
        events: VecDeque::new(),
        quit_sent: false,
    });
    let visible = !m.animating && m.current == i + 1;
    let (cw, chh) = m.cell_size();
    let (ox, oy) = m.grid_origin(d.cols, d.rows);
    let rd = |o: usize| u32::from_le_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]]) as usize;
    let mut off = 0;
    while off + 16 <= data.len() {
        let (x, y, w, h) = (rd(off), rd(off + 4), rd(off + 8), rd(off + 12));
        off += 16;
        let Some(bytes) = w.checked_mul(h).and_then(|n| n.checked_mul(CELL_BYTES)) else { break };
        if off + bytes > data.len() {
            break;
        }
        let (mut c0, mut r0, mut c1, mut r1) = (usize::MAX, usize::MAX, 0, 0);
        for r in 0..h {
            for c in 0..w {
                let (col, row) = (x + c, y + r);
                if col >= d.cols || row >= d.rows {
                    continue;
                }
                let o = off + (r * w + c) * CELL_BYTES;
                let cell = Cell { ch: rd(o) as u32, fg: rd(o + 4) as u32, bg: rd(o + 8) as u32 };
                let slot = &mut d.cells[row * d.cols + col];
                if *slot == cell {
                    continue;
                }
                *slot = cell;
                draw_cell(m, &mut d, col, row);
                c0 = c0.min(col);
                r0 = r0.min(row);
                c1 = c1.max(col + 1);
                r1 = r1.max(row + 1);
            }
        }
        if visible && c0 < c1 {
            expand_damage(m, GAP + ox + c0 * cw, GAP + oy + r0 * chh, (c1 - c0) * cw, (r1 - r0) * chh);
        }
        off += bytes;
    }
    m.desktops[i] = d;
    data.len()
}

pub fn read_tty(fd: usize, buf: &mut [u8]) -> usize {
    let Some(i) = owned(fd) else { return 0 };
    let d = &mut dm().desktops[i];
    let mut n = 0;
    while n + 16 <= buf.len() {
        let Some(ev) = d.events.pop_front() else { break };
        for k in 0..4 {
            buf[n + k * 4..n + k * 4 + 4].copy_from_slice(&ev[k].to_le_bytes());
        }
        n += 16;
    }
    n
}

pub fn close_tty(fd: usize) -> bool {
    let Some(i) = owned(fd) else { return false };
    remove_at(dm(), i);
    true
}

pub fn destroy_of(tid: usize) {
    let m = dm();
    while let Some(i) = m.desktops.iter().position(|d| d.owner == tid) {
        remove_at(m, i);
    }
}

// /dev/gpu/info: [width, height, mode gen, cell w, cell h, cols, rows].
pub fn info() -> [u32; 7] {
    let m = dm();
    let (cw, ch) = m.cell_size();
    let (cols, rows) = m.grid();
    [gpu::width(), gpu::height(), gpu::mode_gen(), cw as u32, ch as u32, cols as u32, rows as u32]
}

// Screen-sized BGRA at byte offset.
pub fn write_layer(glass: bool, offset: usize, data: &[u8]) -> usize {
    let m = dm();
    if m.w == 0 {
        on_mode_change();
    }
    let n = m.w * m.h;
    let layer = if glass { &mut m.glass } else { &mut m.wallpaper };
    if layer.len() != n {
        *layer = vec![if glass { GLASS_FALLBACK } else { 0xFF11_111B }; n];
    }
    let bytes = n * 4;
    if offset >= bytes {
        return 0;
    }
    let len = data.len().min(bytes - offset);
    unsafe {
        core::ptr::copy_nonoverlapping(data.as_ptr(), (layer.as_mut_ptr() as *mut u8).add(offset), len);
    }
    m.restyle = true;
    len
}

// "KFNT", cell_w, cell_h, count, styles (u32 each), count sorted codepoints,
// then styles * count alpha bitmaps of cell_w * cell_h bytes.
pub fn write_font(data: &[u8]) -> usize {
    if data.len() < 20 || &data[0..4] != b"KFNT" {
        return 0;
    }
    let rd = |o: usize| u32::from_le_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]]) as usize;
    let (cw, chh, count, styles) = (rd(4), rd(8), rd(12), rd(16));
    if cw == 0 || chh == 0 || cw > 64 || chh > 128 || styles == 0 || styles > 2 {
        return 0;
    }
    let cps_end = 20 + count * 4;
    let total = cps_end + styles * count * cw * chh;
    if data.len() < total {
        return 0;
    }
    let codepoints: Vec<u32> = (0..count).map(|i| rd(20 + i * 4) as u32).collect();
    let mut ascii = [u16::MAX; 128];
    for (i, &cp) in codepoints.iter().enumerate() {
        if cp < 128 {
            ascii[cp as usize] = i as u16;
        }
    }
    let fallback = codepoints
        .binary_search(&0xFFFD)
        .ok()
        .map(|i| i as u16)
        .unwrap_or(ascii[b'?' as usize]);
    let m = dm();
    m.atlas = Some(Atlas {
        cell_w: cw,
        cell_h: chh,
        codepoints,
        styles,
        bitmaps: data[cps_end..total].to_vec(),
        ascii,
        fallback,
    });
    m.restyle = true;
    data.len()
}
