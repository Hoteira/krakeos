use crate::types::Color;

pub mod primitives;

pub fn draw_pixel(buffer: &mut [u32], width: usize, x: usize, y: usize, mut color: Color) {
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
            let alpha_src = color.a as f32 / 255.0;
            // Linear interpolation of RGB
            color.r = (color.r as f32 * alpha_src + prev.r as f32 * (1.0 - alpha_src)) as u8;
            color.g = (color.g as f32 * alpha_src + prev.g as f32 * (1.0 - alpha_src)) as u8;
            color.b = (color.b as f32 * alpha_src + prev.b as f32 * (1.0 - alpha_src)) as u8;
            
            // Alpha is sum of both, capped at 255
            color.a = (color.a as u16 + prev.a as u16).min(255) as u8;
            
            buffer[idx] = color.to_u32();
        }
    }
}

pub fn draw_u32(buffer: &mut [u32], width: usize, x: usize, y: usize, color: u32) {
    let idx = y * width + x;
    if idx < buffer.len() {
        buffer[idx] = color;
    }
}
