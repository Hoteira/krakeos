use crate::graphics::{draw_pixel, draw_u32};
use crate::types::{Color, Size};
use crate::math::sqrt_f64;

pub fn draw_line(buffer: &mut [u32], width0: usize, x0: usize, y0: usize, x1: usize, y1: usize, color: Color, width: usize) {
    let dx = (x1 as isize - x0 as isize).abs();
    let dy = -(y1 as isize - y0 as isize).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let mut x = x0 as isize;
    let mut y = y0 as isize;
    let half_thickness = (width as isize) / 2;

    loop {
        for tx in -half_thickness..=half_thickness {
            for ty in -half_thickness..=half_thickness {
                let nx = x + tx;
                let ny = y + ty;
                if nx >= 0 && nx < width0 as isize && ny >= 0 && ny < core::cmp::max(y0, y1) as isize {
                    let idx = (ny as usize) * width0 + (nx as usize);
                    if idx < buffer.len() {
                        draw_pixel(buffer, width0, nx as usize, ny as usize, color )
                    }
                }
            }
        }

        if x == x1 as isize && y == y1 as isize {
            break;
        }

        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

pub fn draw_square(
    buffer: &mut [u32],
    buffer_width: usize,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    rounding: Size,
    color: Color,
) {
    if color.a == 0 {
        return;
    }
    draw_square_alpha(buffer, buffer_width, x, y, width, height, rounding, color);
}

pub fn draw_square_alpha(
    buffer: &mut [u32],
    buffer_width: usize,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    rounding: Size,
    color: Color,
) {
    if buffer_width == 0 || width == 0 || height == 0 {
        return;
    }

    let r_val = match rounding {
        Size::Absolute(v) => v as f32,
        Size::Relative(pct) => (width.min(height) as f32 * pct as f32) / 100.0,
        _ => 0.0,
    };
    let r = r_val.min(width as f32 / 2.0).min(height as f32 / 2.0);
    
    let end_y = (y + height).min(buffer.len() / buffer_width);
    let end_x = (x + width).min(buffer_width);

    // Optimizations
    let is_opaque = color.a == 255;
    let color_u32 = color.to_u32();

    for row in y..end_y {
        let ly = (row - y) as f32 + 0.5; // Center of pixel y
        
        for col in x..end_x {
            let lx = (col - x) as f32 + 0.5; // Center of pixel x
            
            // Check corners
            let mut dist = 0.0;
            let mut in_corner = false;

            if ly < r {
                // Top row
                if lx < r {
                    // Top-Left
                    let dx = r - lx;
                    let dy = r - ly;
                    dist = sqrt_f64((dx*dx + dy*dy) as f64) as f32;
                    in_corner = true;
                } else if lx > (width as f32 - r) {
                    // Top-Right
                    let dx = lx - (width as f32 - r);
                    let dy = r - ly;
                    dist = sqrt_f64((dx*dx + dy*dy) as f64) as f32;
                    in_corner = true;
                }
            } else if ly > (height as f32 - r) {
                // Bottom row
                if lx < r {
                    // Bottom-Left
                    let dx = r - lx;
                    let dy = ly - (height as f32 - r);
                    dist = sqrt_f64((dx*dx + dy*dy) as f64) as f32;
                    in_corner = true;
                } else if lx > (width as f32 - r) {
                    // Bottom-Right
                    let dx = lx - (width as f32 - r);
                    let dy = ly - (height as f32 - r);
                    dist = sqrt_f64((dx*dx + dy*dy) as f64) as f32;
                    in_corner = true;
                }
            }

            if in_corner {
                if dist > r {
                    continue; // Skip
                }
                if dist > r - 1.0 {
                    // AA
                    let alpha_factor = (r - dist).clamp(0.0, 1.0);
                    let mut final_color = color;
                    final_color.a = (color.a as f32 * alpha_factor) as u8;
                    draw_pixel(buffer, buffer_width, col, row, final_color);
                    continue;
                }
            }

            // Solid draw
            if is_opaque {
                draw_u32(buffer, buffer_width, col, row, color_u32);
            } else {
                draw_pixel(buffer, buffer_width, col, row, color);
            }
        }
    }
}