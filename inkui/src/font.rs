// Builds the glyph atlas the shell uploads to /dev/gpu/font. Box drawing, blocks,
// braille and powerline glyphs are drawn here so they fill the cell exactly.

use std::fs::File;
use std::io::Read;

// Latin/punctuation/box-drawing plus the symbols and Nerd Font icons the
// apps use; ~1000 glyphs, so it loads fast.
pub const TERM_FONT: &str = "/fonts/CaskaydiaBasic.ttf";
pub const FULL_FONT: &str = "/fonts/CaskaydiaNerd.ttf";

// Codepoint ranges offered to apps (when the font or the procedural
// drawing has them).
const RANGES: &[(u32, u32)] = &[
    (0x20, 0x7E),
    (0xA0, 0x17F),
    (0x2000, 0x206F),
    (0x20AC, 0x20AC),
    (0x2190, 0x21FF),
    (0x2200, 0x23FF),
    (0x2500, 0x27BF),
    (0x2800, 0x28FF),
    (0xE0A0, 0xE0D7),
    (0xE700, 0xE7C5),
    (0xF000, 0xF2E0),
    (0xFFFD, 0xFFFD),
];

pub struct Atlas {
    pub cell_w: usize,
    pub cell_h: usize,
    pub codepoints: Vec<u32>,
    // Regular bitmaps for every codepoint, then bold ones.
    pub bitmaps: Vec<u8>,
}

impl Atlas {
    // The /dev/gpu/font upload format.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(20 + self.codepoints.len() * 4 + self.bitmaps.len());
        out.extend_from_slice(b"KFNT");
        for v in [self.cell_w, self.cell_h, self.codepoints.len(), 2] {
            out.extend_from_slice(&(v as u32).to_le_bytes());
        }
        for cp in &self.codepoints {
            out.extend_from_slice(&cp.to_le_bytes());
        }
        out.extend_from_slice(&self.bitmaps);
        out
    }
}

pub fn load_font_bytes() -> Option<Vec<u8>> {
    for path in [TERM_FONT, FULL_FONT] {
        let mut data = Vec::new();
        if File::open(path).and_then(|mut f| f.read_to_end(&mut data)).is_ok() {
            return Some(data);
        }
    }
    None
}

fn table(data: &[u8], tag: &[u8; 4]) -> Option<usize> {
    let n = u16::from_be_bytes([*data.get(4)?, *data.get(5)?]) as usize;
    (0..n).find_map(|i| {
        let r = 12 + i * 16;
        (data.get(r..r + 4)? == tag).then(|| u32::from_be_bytes([data[r + 8], data[r + 9], data[r + 10], data[r + 11]]) as usize)
    })
}

// (units per em, ascent, descent) from the head and hhea tables.
fn vmetrics(data: &[u8]) -> Option<(f32, f32, f32)> {
    let head = table(data, b"head")?;
    let hhea = table(data, b"hhea")?;
    let be16 = |o: usize| i16::from_be_bytes([data[o], data[o + 1]]) as f32;
    let upem = u16::from_be_bytes([*data.get(head + 18)?, *data.get(head + 19)?]) as f32;
    Some((upem, be16(hhea + 4), be16(hhea + 6)))
}

// Rasterize the atlas at px pixels per em.
pub fn build(data: &[u8], px: f32) -> Option<Atlas> {
    let (upem, asc, desc) = vmetrics(data)?;
    let mut font = titanf::TrueTypeFont::load_font(data).ok()?;
    let scale = px / upem;
    let (m, _) = font.get_char::<false>('M', px);
    let cw = m.advance_width.max(4);
    let ch = ((asc - desc) * scale).ceil() as usize;
    let baseline = (asc * scale).round() as isize;
    let n = cw * ch;

    let mut codepoints = Vec::new();
    let mut regular: Vec<u8> = Vec::new();
    for &(a, b) in RANGES {
        for cp in a..=b {
            let Some(c) = char::from_u32(cp) else { continue };
            let mut cell = vec![0u8; n];
            if !procedural(cp, cw, ch, &mut cell) {
                if font.lookup_glyph_index(c) == 0 {
                    continue;
                }
                raster(&mut font, c, px, cw, ch, baseline, &mut cell);
            }
            codepoints.push(cp);
            regular.extend_from_slice(&cell);
        }
    }
    let mut bitmaps = regular.clone();
    for (i, &cp) in codepoints.iter().enumerate() {
        let cell = &regular[i * n..(i + 1) * n];
        let mut bold = cell.to_vec();
        // Lines and blocks keep their geometry; text gets one pixel wider.
        if !(0x2500..=0x28FF).contains(&cp) && !(0xE0B0..=0xE0BF).contains(&cp) {
            for y in 0..ch {
                for x in 1..cw {
                    let v = cell[y * cw + x - 1];
                    let d = &mut bold[y * cw + x];
                    *d = (*d).max(v);
                }
            }
        }
        bitmaps.extend_from_slice(&bold);
    }
    Some(Atlas { cell_w: cw, cell_h: ch, codepoints, bitmaps })
}

fn raster(font: &mut titanf::TrueTypeFont, c: char, px: f32, cw: usize, ch: usize, baseline: isize, out: &mut [u8]) {
    let (mut m, mut bmp) = font.get_char::<false>(c, px);
    let mut ox = m.left_side_bearing;
    let mut oy = baseline + m.base_line;
    let cp = c as u32;
    // Icons are drawn for double-width cells; shrink them to fit one and
    // center them.
    let icon = cp >= 0xE000 && cp < 0xF900;
    if icon || m.width > cw + 1 {
        if m.width > cw {
            let s = px * cw as f32 / m.width as f32;
            let r = font.get_char::<false>(c, s);
            m = r.0;
            bmp = r.1;
        }
        ox = (cw as isize - m.width as isize) / 2;
        if icon {
            oy = (ch as isize - m.height as isize) / 2;
        } else {
            oy = baseline + m.base_line;
        }
    }
    for y in 0..m.height {
        let ty = oy + y as isize;
        if ty < 0 || ty >= ch as isize {
            continue;
        }
        for x in 0..m.width {
            let tx = ox + x as isize;
            if tx < 0 || tx >= cw as isize {
                continue;
            }
            let v = bmp[y * m.width + x];
            let d = &mut out[ty as usize * cw + tx as usize];
            *d = (*d).max(v);
        }
    }
}

// Procedural glyphs

// Fill a pixel rect (clamped) with alpha a.
fn rect(out: &mut [u8], cw: usize, ch: usize, x0: isize, y0: isize, x1: isize, y1: isize, a: u8) {
    let (x0, x1) = (x0.clamp(0, cw as isize) as usize, x1.clamp(0, cw as isize) as usize);
    let (y0, y1) = (y0.clamp(0, ch as isize) as usize, y1.clamp(0, ch as isize) as usize);
    for y in y0..y1 {
        for x in x0..x1 {
            out[y * cw + x] = out[y * cw + x].max(a);
        }
    }
}

// Anti-aliased shape from a coverage test, 4x4 supersampled.
fn shape(out: &mut [u8], cw: usize, ch: usize, inside: impl Fn(f32, f32) -> bool) {
    for y in 0..ch {
        for x in 0..cw {
            let mut hits = 0u32;
            for sy in 0..4 {
                for sx in 0..4 {
                    let fx = x as f32 + (sx as f32 + 0.5) / 4.0;
                    let fy = y as f32 + (sy as f32 + 0.5) / 4.0;
                    if inside(fx, fy) {
                        hits += 1;
                    }
                }
            }
            let a = (hits * 255 / 16) as u8;
            let d = &mut out[y * cw + x];
            *d = (*d).max(a);
        }
    }
}

// Arms of the light/heavy box characters U+2500..U+254B and
// U+2574..U+257F as [up, right, down, left], 0 none, 1 light, 2 heavy.
fn box_arms(cp: u32) -> Option<[u8; 4]> {
    const T: [[u8; 4]; 76] = [
        [0, 1, 0, 1], [0, 2, 0, 2], [1, 0, 1, 0], [2, 0, 2, 0], // ─━│┃
        [0, 1, 0, 1], [0, 2, 0, 2], [1, 0, 1, 0], [2, 0, 2, 0], // ┄┅┆┇
        [0, 1, 0, 1], [0, 2, 0, 2], [1, 0, 1, 0], [2, 0, 2, 0], // ┈┉┊┋
        [0, 1, 1, 0], [0, 2, 1, 0], [0, 1, 2, 0], [0, 2, 2, 0], // ┌┍┎┏
        [0, 0, 1, 1], [0, 0, 1, 2], [0, 0, 2, 1], [0, 0, 2, 2], // ┐┑┒┓
        [1, 1, 0, 0], [1, 2, 0, 0], [2, 1, 0, 0], [2, 2, 0, 0], // └┕┖┗
        [1, 0, 0, 1], [1, 0, 0, 2], [2, 0, 0, 1], [2, 0, 0, 2], // ┘┙┚┛
        [1, 1, 1, 0], [1, 2, 1, 0], [2, 1, 1, 0], [1, 1, 2, 0], // ├┝┞┟
        [2, 1, 2, 0], [2, 2, 1, 0], [1, 2, 2, 0], [2, 2, 2, 0], // ┠┡┢┣
        [1, 0, 1, 1], [1, 0, 1, 2], [2, 0, 1, 1], [1, 0, 2, 1], // ┤┥┦┧
        [2, 0, 2, 1], [2, 0, 1, 2], [1, 0, 2, 2], [2, 0, 2, 2], // ┨┩┪┫
        [0, 1, 1, 1], [0, 1, 1, 2], [0, 2, 1, 1], [0, 2, 1, 2], // ┬┭┮┯
        [0, 1, 2, 1], [0, 1, 2, 2], [0, 2, 2, 1], [0, 2, 2, 2], // ┰┱┲┳
        [1, 1, 0, 1], [1, 1, 0, 2], [1, 2, 0, 1], [1, 2, 0, 2], // ┴┵┶┷
        [2, 1, 0, 1], [2, 1, 0, 2], [2, 2, 0, 1], [2, 2, 0, 2], // ┸┹┺┻
        [1, 1, 1, 1], [1, 1, 1, 2], [1, 2, 1, 1], [1, 2, 1, 2], // ┼┽┾┿
        [2, 1, 1, 1], [1, 1, 2, 1], [2, 1, 2, 1], [2, 1, 1, 2], // ╀╁╂╃
        [2, 2, 1, 1], [1, 1, 2, 2], [1, 2, 2, 1], [2, 2, 1, 2], // ╄╅╆╇
        [1, 2, 2, 2], [2, 1, 2, 2], [2, 2, 2, 1], [2, 2, 2, 2], // ╈╉╊╋
    ];
    const H: [[u8; 4]; 12] = [
        [0, 0, 0, 1], [1, 0, 0, 0], [0, 1, 0, 0], [0, 0, 1, 0], // ╴╵╶╷
        [0, 0, 0, 2], [2, 0, 0, 0], [0, 2, 0, 0], [0, 0, 2, 0], // ╸╹╺╻
        [0, 2, 0, 1], [1, 0, 2, 0], [0, 1, 0, 2], [2, 0, 1, 0], // ╼╽╾╿
    ];
    match cp {
        0x2500..=0x254B => Some(T[(cp - 0x2500) as usize]),
        0x2574..=0x257F => Some(H[(cp - 0x2574) as usize]),
        _ => None,
    }
}

fn procedural(cp: u32, cw: usize, ch: usize, out: &mut [u8]) -> bool {
    let (w, h) = (cw as isize, ch as isize);
    let light = ((cw + 4) / 9).max(1) as isize;
    let heavy = light * 2 + 1;
    let cx = w / 2;
    let cy = h / 2;
    let thick = |k: u8| if k == 2 { heavy } else { light };

    if let Some(arms) = box_arms(cp) {
        let [u, r, d, l] = arms;
        let vmax = thick(u.max(d));
        let hmax = thick(l.max(r));
        let dashed = matches!(cp, 0x2504..=0x250B);
        if l > 0 {
            let t = thick(l);
            let end = if u.max(d) > 0 { cx + (vmax + 1) / 2 } else { cx + 1 };
            rect(out, cw, ch, 0, cy - t / 2, end, cy - t / 2 + t, 255);
        }
        if r > 0 {
            let t = thick(r);
            let start = if u.max(d) > 0 { cx - vmax / 2 } else { cx };
            rect(out, cw, ch, start, cy - t / 2, w, cy - t / 2 + t, 255);
        }
        if u > 0 {
            let t = thick(u);
            let end = if l.max(r) > 0 { cy + (hmax + 1) / 2 } else { cy + 1 };
            rect(out, cw, ch, cx - t / 2, 0, cx - t / 2 + t, end, 255);
        }
        if d > 0 {
            let t = thick(d);
            let start = if l.max(r) > 0 { cy - hmax / 2 } else { cy };
            rect(out, cw, ch, cx - t / 2, start, cx - t / 2 + t, h, 255);
        }
        if dashed {
            // Punch gaps: 3 dashes for ┄┅┆┇, 4 for ┈┉┊┋.
            let n = if cp < 0x2508 { 3 } else { 4 };
            let horizontal = l > 0;
            let len = if horizontal { w } else { h };
            for i in 0..n {
                let g0 = (i * 2 + 1) * len / (n * 2);
                let g1 = g0 + (len / (n * 4)).max(1);
                if horizontal {
                    for y in 0..h {
                        for x in g0.max(0)..g1.min(w) {
                            out[(y * w + x) as usize] = 0;
                        }
                    }
                } else {
                    for y in g0.max(0)..g1.min(h) {
                        for x in 0..w {
                            out[(y * w + x) as usize] = 0;
                        }
                    }
                }
            }
        }
        return true;
    }

    match cp {
        // ╭╮╯╰: quarter arcs joining the two half-lines.
        0x256D..=0x2570 => {
            let t = light as f32;
            let (fcx, fcy) = (cx as f32 + (light % 2) as f32 * 0.5, cy as f32 + (light % 2) as f32 * 0.5);
            let r = (w as f32 - fcx).min(h as f32 - fcy).min(fcx).min(fcy).max(2.0);
            let (sx, sy) = match cp {
                0x256D => (1.0, 1.0),   // down + right
                0x256E => (-1.0, 1.0),  // down + left
                0x256F => (-1.0, -1.0), // up + left
                _ => (1.0, -1.0),       // up + right
            };
            let (ax, ay) = (fcx + sx * r, fcy + sy * r); // arc center
            shape(out, cw, ch, |x, y| {
                let (dx, dy) = (x - ax, y - ay);
                let in_quadrant = dx * sx <= 0.0 && dy * sy <= 0.0;
                if in_quadrant {
                    let d = (dx * dx + dy * dy).sqrt();
                    (d - r).abs() <= t / 2.0
                } else {
                    // Straight parts beyond the arc.
                    let on_v = (x - fcx).abs() <= t / 2.0 && (y - ay) * sy >= 0.0;
                    let on_h = (y - fcy).abs() <= t / 2.0 && (x - ax) * sx >= 0.0;
                    on_v || on_h
                }
            });
            true
        }
        // ╱╲╳
        0x2571..=0x2573 => {
            let t = light as f32 * 0.9;
            let (fw, fh) = (w as f32, h as f32);
            let len = (fw * fw + fh * fh).sqrt();
            shape(out, cw, ch, |x, y| {
                let d1 = (x * fh - (fh - y) * fw).abs() / len; // ╱
                let d2 = (x * fh - y * fw).abs() / len; // ╲
                (cp != 0x2572 && d1 <= t / 2.0 + 0.3) || (cp != 0x2571 && d2 <= t / 2.0 + 0.3)
            });
            true
        }
        // ▀ upper half, ▁..█ lower eighths
        0x2580 => {
            rect(out, cw, ch, 0, 0, w, h / 2, 255);
            true
        }
        0x2581..=0x2588 => {
            let n = (cp - 0x2580) as isize;
            rect(out, cw, ch, 0, h - (h * n + 4) / 8, w, h, 255);
            true
        }
        // ▉..▏ left eighths
        0x2589..=0x258F => {
            let n = (0x2590 - cp) as isize;
            rect(out, cw, ch, 0, 0, (w * n + 4) / 8, h, 255);
            true
        }
        0x2590 => {
            rect(out, cw, ch, w / 2, 0, w, h, 255);
            true
        }
        0x2591..=0x2593 => {
            let a = [64u8, 128, 192][(cp - 0x2591) as usize];
            rect(out, cw, ch, 0, 0, w, h, a);
            true
        }
        0x2594 => {
            rect(out, cw, ch, 0, 0, w, (h + 4) / 8, 255);
            true
        }
        0x2595 => {
            rect(out, cw, ch, w - (w + 4) / 8, 0, w, h, 255);
            true
        }
        // Quadrants ▖▗▘▙▚▛▜▝▞▟: bits UL, UR, LL, LR.
        0x2596..=0x259F => {
            const Q: [u8; 10] = [0b0010, 0b0001, 0b1000, 0b1011, 0b1001, 0b1110, 0b1101, 0b0100, 0b0110, 0b0111];
            let q = Q[(cp - 0x2596) as usize];
            let (mx, my) = (w / 2, h / 2);
            if q & 8 != 0 {
                rect(out, cw, ch, 0, 0, mx, my, 255);
            }
            if q & 4 != 0 {
                rect(out, cw, ch, mx, 0, w, my, 255);
            }
            if q & 2 != 0 {
                rect(out, cw, ch, 0, my, mx, h, 255);
            }
            if q & 1 != 0 {
                rect(out, cw, ch, mx, my, w, h, 255);
            }
            true
        }
        // Braille: 2 x 4 dots.
        0x2800..=0x28FF => {
            let bits = cp - 0x2800;
            const POS: [(isize, isize); 8] = [(0, 0), (0, 1), (0, 2), (1, 0), (1, 1), (1, 2), (0, 3), (1, 3)];
            let r = ((w.min(h / 2) as f32) / 5.0).max(0.8);
            for (i, &(dx, dy)) in POS.iter().enumerate() {
                if bits & (1 << i) == 0 {
                    continue;
                }
                let px = (w as f32) * (0.3 + 0.4 * dx as f32);
                let py = (h as f32) * (0.125 + 0.25 * dy as f32);
                shape(out, cw, ch, |x, y| (x - px).powi(2) + (y - py).powi(2) <= r * r);
            }
            true
        }
        // Powerline separators.
        0xE0B0 | 0xE0B2 => {
            let right = cp == 0xE0B0;
            let (fw, fh) = (w as f32, h as f32);
            shape(out, cw, ch, |x, y| {
                let xx = if right { x } else { fw - x };
                xx <= fw * (1.0 - (2.0 * y / fh - 1.0).abs())
            });
            true
        }
        0xE0B1 | 0xE0B3 => {
            let right = cp == 0xE0B1;
            let (fw, fh) = (w as f32, h as f32);
            let t = light as f32;
            shape(out, cw, ch, |x, y| {
                let xx = if right { x } else { fw - x };
                let edge = fw * (1.0 - (2.0 * y / fh - 1.0).abs());
                (xx - edge).abs() <= t * 0.8
            });
            true
        }
        0xE0B4 | 0xE0B6 => {
            let right = cp == 0xE0B4;
            let (fw, fh) = (w as f32, h as f32);
            shape(out, cw, ch, |x, y| {
                let xx = if right { x } else { fw - x };
                let ny = (y - fh / 2.0) / (fh / 2.0);
                let nx = xx / fw;
                nx * nx + ny * ny <= 1.0
            });
            true
        }
        0xE0B5 | 0xE0B7 => {
            let right = cp == 0xE0B5;
            let (fw, fh) = (w as f32, h as f32);
            let t = light as f32 / fw;
            shape(out, cw, ch, |x, y| {
                let xx = if right { x } else { fw - x };
                let ny = (y - fh / 2.0) / (fh / 2.0);
                let nx = xx / fw;
                ((nx * nx + ny * ny).sqrt() - (1.0 - t)).abs() <= t
            });
            true
        }
        _ => false,
    }
}
