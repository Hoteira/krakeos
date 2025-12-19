use crate::graphics::{draw_pixel, draw_u32};
use crate::types::{Color, Size};
use crate::math::{sqrt_f64, ceil_f32};
use titanf::TrueTypeFont;
use alloc::vec::Vec;

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

    let is_opaque = color.a == 255;
    let color_u32 = color.to_u32();

    let r_sq = r * r;
    let r_ceil = ceil_f32(r) as usize;

    let inner_x_start = x + r_ceil;
    let inner_x_end = if x + width > r_ceil { x + width - r_ceil } else { x };
    let inner_y_start = y + r_ceil;
    let inner_y_end = if y + height > r_ceil { y + height - r_ceil } else { y };

    for row in y..end_y {
        let ly = (row - y) as f32 + 0.5;
        let is_top_row = row < inner_y_start;
        let is_bottom_row = row >= inner_y_end;
        let check_corners = is_top_row || is_bottom_row;

        for col in x..end_x {
            if !check_corners && col >= inner_x_start && col < inner_x_end {
                if is_opaque {
                    draw_u32(buffer, buffer_width, col, row, color_u32);
                } else {
                    draw_pixel(buffer, buffer_width, col, row, color);
                }
                continue;
            }

            let lx = (col - x) as f32 + 0.5;
            let mut in_corner = false;
            let mut dx = 0.0;
            let mut dy = 0.0;

            if is_top_row {
                if col < inner_x_start {
                    dx = r - lx;
                    dy = r - ly;
                    in_corner = true;
                } else if col >= inner_x_end {
                    dx = lx - (width as f32 - r);
                    dy = r - ly;
                    in_corner = true;
                }
            } else if is_bottom_row {
                if col < inner_x_start {
                    dx = r - lx;
                    dy = ly - (height as f32 - r);
                    in_corner = true;
                } else if col >= inner_x_end {
                    dx = lx - (width as f32 - r);
                    dy = ly - (height as f32 - r);
                    in_corner = true;
                }
            }

            if in_corner {
                let dist_sq = dx*dx + dy*dy;
                if dist_sq > r_sq {
                     continue;
                }
                
                let r_inner = r - 1.0;
                if r_inner > 0.0 && dist_sq > r_inner * r_inner {
                    let dist = sqrt_f64(dist_sq as f64) as f32;
                    let alpha_factor = (r - dist).clamp(0.0, 1.0);
                    let mut final_color = color;
                    final_color.a = (color.a as f32 * alpha_factor) as u8;
                    draw_pixel(buffer, buffer_width, col, row, final_color);
                    continue;
                }
            }

            if is_opaque {
                draw_u32(buffer, buffer_width, col, row, color_u32);
            } else {
                draw_pixel(buffer, buffer_width, col, row, color);
            }
        }
    }
}

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
            chars.next();

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
                if tag.starts_with("0x") {
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
                } else if tag.ends_with("pt") {
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
    max_width: usize,
    scroll_y: usize,
    max_height: usize,
    clip_y: usize,
) {
    if buffer_width == 0 {
        return;
    }
    
    let (segments, clean_text) = parse_format_tags(text, default_color, default_size);

    let mut current_x = x;
    let start_y_isize = y as isize - scroll_y as isize;
    let mut current_baseline_isize = start_y_isize;

    let chars: Vec<char> = clean_text.chars().collect();
    let mut i = 0;
    
    let limit_y = clip_y + max_height;

    while i < chars.len() {
        let c = chars[i];
        
        let segment = segments.iter()
            .find(|s| i >= s.start && i < s.end)
            .copied()
            .unwrap_or(TextSegment {
                start: 0,
                end: clean_text.len(),
                color: default_color,
                size: default_size,
            });

        if c == '\n' {
            current_x = x;
            let line_height = (segment.size * 1.2) as usize;
            current_baseline_isize += line_height as isize;
            i += 1;
            continue;
        }

        let (metrics, bitmap) = font.get_char::<true>(c, segment.size);
        
        let next_x_end = (current_x as isize + metrics.left_side_bearing + metrics.advance_width as isize) as usize;
        let line_height = (segment.size * 1.2) as usize;

        if max_width > 0 && next_x_end >= x + max_width {
             current_x = x;
             current_baseline_isize += line_height as isize;
        }

        let glyph_y_start = (current_baseline_isize + metrics.base_line as isize) as isize;
        
        if glyph_y_start + (metrics.height as isize) < clip_y as isize {
             current_x += metrics.advance_width;
             i += 1;
             continue;
        }
        
        if glyph_y_start > limit_y as isize {
             break; 
        }

        let glyph_x = (current_x as isize + metrics.left_side_bearing) as usize;

        for row in 0..metrics.height {
            let dest_y_isize = glyph_y_start + row as isize;
            
            if dest_y_isize < clip_y as isize { continue; }
            
            let dest_y = dest_y_isize as usize;
            
            if max_height > 0 && dest_y >= clip_y + max_height { continue; }
            if dest_y >= buffer.len() / buffer_width { continue; }

            for col in 0..metrics.width {
                let dest_x = glyph_x + col;
                if dest_x >= buffer_width { continue; }
                if max_width > 0 && dest_x >= x + max_width { continue; } // Pixel Clip

                let bitmap_alpha = bitmap[row * metrics.width + col];
                if bitmap_alpha > 0 {
                    let mut pixel_color = segment.color;
                    pixel_color.a = ((pixel_color.a as u16 * bitmap_alpha as u16) / 255) as u8;
                    draw_pixel(buffer, buffer_width, dest_x, dest_y, pixel_color);
                }
            }
        }

        current_x += metrics.advance_width;
        i += 1;
    }
}

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
    draw_text_formatted(buffer, buffer_width, x, y, text, font, size, color, 0, 0, 9999, y);
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
