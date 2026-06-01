use crate::types::Color;

pub mod primitives;

pub fn draw_pixel(buffer: &mut [u32], width: usize, x: usize, y: usize, color: Color) {
    if width == 0 || color.a == 0 { return; }

    let idx = y * width + x;
    if idx >= buffer.len() { return; }

    if color.a == 255 {
        buffer[idx] = color.to_u32();
    } else {
        let prev = Color::from_u32(buffer[idx]);
        if prev.a == 0 {
            buffer[idx] = color.to_u32();
        } else {
            let alpha = color.a as u32;
            let inv_alpha = 255 - alpha;

            let r_mul = (color.r as u32 * alpha) + (prev.r as u32 * inv_alpha);
            let g_mul = (color.g as u32 * alpha) + (prev.g as u32 * inv_alpha);
            let b_mul = (color.b as u32 * alpha) + (prev.b as u32 * inv_alpha);

            let r = (r_mul + 1 + (r_mul >> 8)) >> 8;
            let g = (g_mul + 1 + (g_mul >> 8)) >> 8;
            let b = (b_mul + 1 + (b_mul >> 8)) >> 8;


            let a = (alpha + prev.a as u32).min(255);


            buffer[idx] = (a << 24) | (r << 16) | (g << 8) | b;
        }
    }
}

pub fn draw_u32(buffer: &mut [u32], width: usize, x: usize, y: usize, color: u32) {
    let idx = y * width + x;
    if idx < buffer.len() {
        buffer[idx] = color;
    }
}

/// Temporary build de-risk: forces `tiny-skia` and `fontdue` to compile under the
/// WASM `-Z build-std` constraint. Remove once the real renderer migration lands.
#[doc(hidden)]
pub fn _derisk_graphics_libs() -> usize {
    let w = tiny_skia::Pixmap::new(2, 2).map(|p| p.width() as usize).unwrap_or(0);
    w + core::mem::size_of::<fontdue::Font>()
}
