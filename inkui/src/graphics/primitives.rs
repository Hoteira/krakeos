use crate::graphics::{draw_pixel, draw_u32};
use crate::types::{Color, Size};
use crate::math::sqrt_f64;
use titanf::TrueTypeFont;

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

// Add to inkui/src/graphics/primitives.rs
use alloc::vec::Vec;

#[derive(Debug, Clone, Copy)]
struct TextSegment {
    start: usize,
    end: usize,
    color: Color,
    size: f32,
}

fn parse_format_tags(text: &str, default_color: Color, default_size: f32) -> (Vec<TextSegment>, alloc::string::String) {
    let mut segments = Vec::new();
    let mut clean_text = alloc::string::String::new();
    let mut current_color = default_color;
    let mut current_size = default_size;
    let mut chars = text.chars().peekable();
    let mut clean_pos = 0;

    while let Some(c) = chars.next() {
        if c == '#' && chars.peek() == Some(&'[') {
            chars.next(); // consume '['

            // Parse tag content
            let mut tag = alloc::string::String::new();
            let mut valid_tag = false;

            while let Some(tc) = chars.next() {
                if tc == ']' {
                    valid_tag = true;
                    break;
                }
                tag.push(tc);
            }

            if valid_tag {
                // Parse color: #[0xAARRGGBB], #[0xRRGGBB], or #[0xRRGGBB,AA]
                if tag.starts_with("0x") {
                    // Check for separate alpha: 0xRRGGBB,AA
                    if let Some(comma_pos) = tag.find(',') {
                        let color_part = &tag[2..comma_pos];
                        let alpha_part = &tag[comma_pos+1..];

                        if let (Ok(rgb), Ok(alpha)) = (
                            usize::from_str_radix(color_part, 16),
                            alpha_part.parse::<u8>()
                        ) {
                            current_color = Color::rgba(
                                ((rgb >> 16) & 0xFF) as u8,
                                ((rgb >> 8) & 0xFF) as u8,
                                (rgb & 0xFF) as u8,
                                alpha
                            );
                        }
                    } else if let Ok(hex_val) = usize::from_str_radix(&tag[2..], 16) {
                        if tag.len() == 10 { // 0xAARRGGBB
                            current_color = Color::from_u32(hex_val as u32);
                        } else if tag.len() == 8 { // 0xRRGGBB
                            current_color = Color::from_u32((hex_val as u32) | 0xFF000000);
                        }
                    }
                }
                // Parse size: #[13pt] or #[13]
                else if tag.ends_with("pt") {
                    if let Ok(size) = tag[..tag.len()-2].parse::<f32>() {
                        current_size = size;
                    }
                } else if let Ok(size) = tag.parse::<f32>() {
                    current_size = size;
                }
            } else {
                // Invalid tag, treat as literal
                clean_text.push('#');
                clean_text.push('[');
                clean_text.push_str(&tag);
            }
        } else {
            // Regular character
            let segment_start = clean_pos;
            clean_text.push(c);
            clean_pos += 1;

            segments.push(TextSegment {
                start: segment_start,
                end: clean_pos,
                color: current_color,
                size: current_size,
            });
        }
    }

    (segments, clean_text)
}

pub fn draw_text_formatted(
    buffer: &mut [u32],
    buffer_width: usize,
    x: usize,
    y: usize,
    text: &str,
    font: &mut TrueTypeFont,
    default_size: f32,
    default_color: Color,
) {
    if buffer_width == 0 {
        return;
    }

    let (segments, clean_text) = parse_format_tags(text, default_color, default_size);

    let mut current_x = x;
    let baseline_y = y;

    let chars: Vec<char> = clean_text.chars().collect();

    for (idx, &c) in chars.iter().enumerate() {
        // Find segment for this character
        let segment = segments.iter()
            .find(|s| idx >= s.start && idx < s.end)
            .copied()
            .unwrap_or(TextSegment {
                start: 0,
                end: clean_text.len(),
                color: default_color,
                size: default_size,
            });

        let (metrics, bitmap) = font.get_char::<true>(c, segment.size);

        let glyph_x = (current_x as isize + metrics.left_side_bearing) as usize;
        let glyph_y = (baseline_y as isize + metrics.base_line) as usize;

        for row in 0..metrics.height {
            let dest_y = glyph_y + row;
            if dest_y >= buffer.len() / buffer_width { continue; }

            for col in 0..metrics.width {
                let dest_x = glyph_x + col;
                if dest_x >= buffer_width { continue; }

                let bitmap_alpha = bitmap[row * metrics.width + col];
                if bitmap_alpha > 0 {
                    let mut pixel_color = segment.color;
                    // Multiply color alpha with bitmap alpha
                    pixel_color.a = ((pixel_color.a as u16 * bitmap_alpha as u16) / 255) as u8;
                    draw_pixel(buffer, buffer_width, dest_x, dest_y, pixel_color);
                }
            }
        }

        current_x += metrics.advance_width;
    }
}

// Keep the old draw_text but make it call the formatted version
pub fn draw_text(
    buffer: &mut [u32],
    buffer_width: usize,
    x: usize,
    y: usize,
    text: &str,
    font: &mut TrueTypeFont,
    size: f32,
    color: Color,
) {
    draw_text_formatted(buffer, buffer_width, x, y, text, font, size, color);
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t) as u8
}

fn lerp_color(start: Color, end: Color, t: f32) -> Color {
    let t = if t < 0.0 { 0.0 } else if t > 1.0 { 1.0 } else { t };
    Color::rgba(
        lerp_u8(start.r, end.r, t),
        lerp_u8(start.g, end.g, t),
        lerp_u8(start.b, end.b, t),
        lerp_u8(start.a, end.a, t),
    )
}

use crate::types::{LinearGradient, GradientDirection, BackgroundStyle};


pub fn draw_square_gradient(
    buffer: &mut [u32],
    buffer_width: usize,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    rounding: Size,
    gradient: &LinearGradient,
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

    let w_f = width as f32;
    let h_f = height as f32;

    for row in y..end_y {
        let ly = (row - y) as f32 + 0.5;

        for col in x..end_x {
            let lx = (col - x) as f32 + 0.5;

            // Check rounded corners
            let mut dist = 0.0;
            let mut in_corner = false;

            if ly < r {
                if lx < r {
                    let dx = r - lx;
                    let dy = r - ly;
                    dist = sqrt_f64((dx*dx + dy*dy) as f64) as f32;
                    in_corner = true;
                } else if lx > w_f - r {
                    let dx = lx - (w_f - r);
                    let dy = r - ly;
                    dist = sqrt_f64((dx*dx + dy*dy) as f64) as f32;
                    in_corner = true;
                }
            } else if ly > h_f - r {
                if lx < r {
                    let dx = r - lx;
                    let dy = ly - (h_f - r);
                    dist = sqrt_f64((dx*dx + dy*dy) as f64) as f32;
                    in_corner = true;
                } else if lx > w_f - r {
                    let dx = lx - (w_f - r);
                    let dy = ly - (h_f - r);
                    dist = sqrt_f64((dx*dx + dy*dy) as f64) as f32;
                    in_corner = true;
                }
            }

            if in_corner && dist > r {
                continue;
            }

            // Calculate gradient position
            let t = match gradient.direction {
                GradientDirection::Horizontal => lx / w_f,
                GradientDirection::Vertical => ly / h_f,
                GradientDirection::Diagonal => (lx + ly) / (w_f + h_f),
                GradientDirection::DiagonalAlt => ((w_f - lx) + ly) / (w_f + h_f),
                GradientDirection::Custom { angle } => {
                    let norm_angle = ((angle % 360.0) + 360.0) % 360.0;

                    if norm_angle < 45.0 || norm_angle >= 315.0 {
                        lx / w_f
                    } else if norm_angle < 135.0 {
                        ly / h_f
                    } else if norm_angle < 225.0 {
                        1.0 - (lx / w_f)
                    } else {
                        1.0 - (ly / h_f)
                    }
                }
            };

            let mut color = lerp_color(gradient.start_color, gradient.end_color, t);

            // Apply corner antialiasing
            if in_corner && dist > r - 1.0 {
                let alpha_factor = (r - dist).max(0.0).min(1.0);
                color.a = (color.a as f32 * alpha_factor) as u8;
            }

            if color.a > 0 {
                draw_pixel(buffer, buffer_width, col, row, color);
            }
        }
    }
}


pub fn draw_background_style(
    buffer: &mut [u32],
    buffer_width: usize,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    rounding: Size,
    style: &BackgroundStyle,
) {
    match style {
        BackgroundStyle::Solid(color) => {
            draw_square(buffer, buffer_width, x, y, width, height, rounding, *color);
        },
        BackgroundStyle::Gradient(gradient) => {
            draw_square_gradient(buffer, buffer_width, x, y, width, height, rounding, gradient);
        }
    }
}