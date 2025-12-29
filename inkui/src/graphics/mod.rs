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

            let r = ((color.r as u32 * alpha) + (prev.r as u32 * inv_alpha)) / 255;
            let g = ((color.g as u32 * alpha) + (prev.g as u32 * inv_alpha)) / 255;
            let b = ((color.b as u32 * alpha) + (prev.b as u32 * inv_alpha)) / 255;
            
            
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
