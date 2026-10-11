// The shell: paints the wallpaper and builds the text font.
// Uploads /dev/gpu/font, fb and glass on boot, on resize and when /config/system.toml changes.

use std::fs::File;
use std::io::{Read, Write};
use std::time::Duration;

// Glass tint: Catppuccin base over the blurred wallpaper.
const TINT: [f32; 3] = [30.0, 30.0, 46.0];

fn read_info() -> Option<[u32; 7]> {
    let mut f = File::open("/dev/gpu/info").ok()?;
    let mut b = [0u8; 28];
    let n = f.read(&mut b).ok()?;
    if n < 28 {
        return None;
    }
    let mut out = [0u32; 7];
    for i in 0..7 {
        out[i] = u32::from_le_bytes([b[i * 4], b[i * 4 + 1], b[i * 4 + 2], b[i * 4 + 3]]);
    }
    Some(out)
}

fn upload(path: &str, data: &[u8]) {
    // A fresh fd each time so the write starts at offset 0.
    if let Ok(mut f) = File::options().write(true).open(path) {
        let _ = f.write_all(data);
    }
}

fn pixels_bytes(px: &[u32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(px.len() * 4);
    for p in px {
        out.extend_from_slice(&p.to_le_bytes());
    }
    out
}

// RGB float image.
struct Img {
    w: usize,
    h: usize,
    px: Vec<[f32; 3]>,
}

fn load_png(path: &str) -> Option<Img> {
    let mut data = Vec::new();
    File::open(path).ok()?.read_to_end(&mut data).ok()?;
    let mut reader = png::Decoder::new(data.as_slice()).read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    let (w, h) = (info.width as usize, info.height as usize);
    let ch = match info.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Grayscale => 1,
        _ => return None,
    };
    if info.bit_depth != png::BitDepth::Eight {
        return None;
    }
    let px = (0..w * h)
        .map(|i| {
            let s = &buf[i * ch..];
            if ch >= 3 {
                [s[0] as f32, s[1] as f32, s[2] as f32]
            } else {
                [s[0] as f32; 3]
            }
        })
        .collect();
    Some(Img { w, h, px })
}

// Bilinear scale to cover w x h, cropping the overflow.
fn cover(img: &Img, w: usize, h: usize) -> Img {
    let s = (w as f32 / img.w as f32).max(h as f32 / img.h as f32);
    let ox = (img.w as f32 * s - w as f32) / 2.0;
    let oy = (img.h as f32 * s - h as f32) / 2.0;
    let mut px = Vec::with_capacity(w * h);
    for y in 0..h {
        let fy = ((y as f32 + 0.5 + oy) / s - 0.5).clamp(0.0, (img.h - 1) as f32);
        let y0 = fy as usize;
        let y1 = (y0 + 1).min(img.h - 1);
        let ty = fy - y0 as f32;
        for x in 0..w {
            let fx = ((x as f32 + 0.5 + ox) / s - 0.5).clamp(0.0, (img.w - 1) as f32);
            let x0 = fx as usize;
            let x1 = (x0 + 1).min(img.w - 1);
            let tx = fx - x0 as f32;
            let (a, b) = (img.px[y0 * img.w + x0], img.px[y0 * img.w + x1]);
            let (c, d) = (img.px[y1 * img.w + x0], img.px[y1 * img.w + x1]);
            let mut o = [0.0; 3];
            for k in 0..3 {
                let top = a[k] + (b[k] - a[k]) * tx;
                let bot = c[k] + (d[k] - c[k]) * tx;
                o[k] = top + (bot - top) * ty;
            }
            px.push(o);
        }
    }
    Img { w, h, px }
}

// Separable box blur, r pixels each way.
fn box_blur(img: &mut Img, r: usize) {
    let (w, h) = (img.w, img.h);
    let mut tmp = vec![[0.0f32; 3]; w * h];
    let norm = 1.0 / (2 * r + 1) as f32;
    for y in 0..h {
        let row = &img.px[y * w..(y + 1) * w];
        let mut acc = [0.0f32; 3];
        for i in 0..=2 * r {
            let p = row[(i as isize - r as isize).clamp(0, w as isize - 1) as usize];
            for k in 0..3 {
                acc[k] += p[k];
            }
        }
        for x in 0..w {
            for k in 0..3 {
                tmp[y * w + x][k] = acc[k] * norm;
            }
            let out = row[(x as isize - r as isize).clamp(0, w as isize - 1) as usize];
            let inp = row[(x + r + 1).min(w - 1)];
            for k in 0..3 {
                acc[k] += inp[k] - out[k];
            }
        }
    }
    for x in 0..w {
        let mut acc = [0.0f32; 3];
        for i in 0..=2 * r {
            let p = tmp[(i as isize - r as isize).clamp(0, h as isize - 1) as usize * w + x];
            for k in 0..3 {
                acc[k] += p[k];
            }
        }
        for y in 0..h {
            for k in 0..3 {
                img.px[y * w + x][k] = acc[k] * norm;
            }
            let out = tmp[(y as isize - r as isize).clamp(0, h as isize - 1) as usize * w + x];
            let inp = tmp[(y + r + 1).min(h - 1) * w + x];
            for k in 0..3 {
                acc[k] += inp[k] - out[k];
            }
        }
    }
}

fn to_bgra(img: &Img) -> Vec<u32> {
    img.px
        .iter()
        .map(|p| {
            let c = |v: f32| v.clamp(0.0, 255.0) as u32;
            0xFF00_0000 | c(p[0]) << 16 | c(p[1]) << 8 | c(p[2])
        })
        .collect()
}

// Glass layer: blur a quarter-size copy (cheap and very soft), scale it back up and tint it.
fn glass(wall: &Img, opacity: f32) -> Vec<u32> {
    let (sw, sh) = ((wall.w / 4).max(1), (wall.h / 4).max(1));
    let mut small = cover(wall, sw, sh);
    for _ in 0..3 {
        box_blur(&mut small, 3);
    }
    let mut big = cover(&small, wall.w, wall.h);
    for p in big.px.iter_mut() {
        for k in 0..3 {
            p[k] = p[k] * (1.0 - opacity) + TINT[k] * opacity;
        }
    }
    to_bgra(&big)
}

// A gradient for when there's no wallpaper file.
fn fallback_wallpaper(w: usize, h: usize) -> Img {
    let mut px = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            let t = (x + y) as f32 / (w + h) as f32;
            px.push([30.0 + 40.0 * t, 30.0 + 10.0 * t, 46.0 + 50.0 * (1.0 - t)]);
        }
    }
    Img { w, h, px }
}

fn font_px(screen_h: u32, configured: f32) -> f32 {
    if configured > 0.0 {
        return configured.clamp(8.0, 64.0).round();
    }
    (screen_h as f32 / 50.0).clamp(14.0, 24.0).round()
}

fn main() {
    let font = inkui::font::load_font_bytes();
    let mut shown = (0u32, 0u32);
    let mut font_size = 0.0;
    let mut cfg = inkui::config::Display::default();
    let mut source_path = String::new();
    let mut source = None;
    loop {
        let new_cfg = inkui::config::system().display;
        let changed = new_cfg != cfg || source_path.is_empty();
        if new_cfg.wallpaper != source_path {
            source_path = new_cfg.wallpaper.clone();
            source = load_png(&source_path);
        }
        cfg = new_cfg;
        if let Some(info) = read_info() {
            let (w, h) = (info[0], info[1]);
            if ((w, h) != shown || changed) && w > 0 && h > 0 {
                shown = (w, h);
                let px = font_px(h, cfg.font_size);
                if px != font_size {
                    if let Some(atlas) = font.as_deref().and_then(|f| inkui::font::build(f, px)) {
                        upload("/dev/gpu/font", &atlas.encode());
                        font_size = px;
                    }
                }
                let (w, h) = (w as usize, h as usize);
                let wall = match &source {
                    Some(img) => cover(img, w, h),
                    None => fallback_wallpaper(w, h),
                };
                upload("/dev/gpu/fb", &pixels_bytes(&to_bgra(&wall)));
                upload("/dev/gpu/glass", &pixels_bytes(&glass(&wall, cfg.glass_opacity.clamp(0.0, 1.0))));
            }
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}
