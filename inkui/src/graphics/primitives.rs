use crate::graphics::{draw_pixel, draw_u32};
use crate::math::{ceil_f32, sqrt_f64};
use crate::types::{Color, Size};
use alloc::vec::Vec;
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
                        draw_pixel(buffer, width0, nx as usize, ny as usize, color)
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
    border_size: usize,
    border_color: Color,
) {
    if color.a == 0 && (border_size == 0 || border_color.a == 0) {
        return;
    }
    draw_square_alpha(buffer, buffer_width, x, y, width, height, rounding, color, border_size, border_color);
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
    border_size: usize,
    border_color: Color,
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

    let is_bg_opaque = color.a == 255;
    let bg_u32 = color.to_u32();

    let is_border_opaque = border_color.a == 255;
    let border_u32 = border_color.to_u32();
    let has_border = border_size > 0 && border_color.a > 0;

    let r_sq = r * r;
    let r_ceil = ceil_f32(r) as usize;


    if r < 0.1 && border_size == 0 && is_bg_opaque {
        for row in y..end_y {
            let start = row * buffer_width + x;
            let end = start + width;
            if end <= buffer.len() {
                buffer[start..end].fill(bg_u32);
            }
        }
        return;
    }

    let inner_x_start = x + r_ceil;
    let inner_x_end = if x + width > r_ceil { x + width - r_ceil } else { x };
    let inner_y_start = y + r_ceil;
    let inner_y_end = if y + height > r_ceil { y + height - r_ceil } else { y };

    let border_sz_f = border_size as f32;

    for row in y..end_y {
        let ly = (row - y) as f32 + 0.5;
        let is_top_row = row < inner_y_start;
        let is_bottom_row = row >= inner_y_end;
        let check_corners = is_top_row || is_bottom_row;

        let is_top_border = has_border && (row < y + border_size);
        let is_bottom_border = has_border && (row >= y + height - border_size);


        let is_top_inner_edge = has_border && (row == y + border_size - 1);
        let is_bottom_inner_edge = has_border && (row == y + height - border_size);

        for col in x..end_x {
            if (!check_corners) || (col >= inner_x_start && col < inner_x_end) {
                let is_left_border = has_border && (col < x + border_size);
                let is_right_border = has_border && (col >= x + width - border_size);

                let is_border = is_top_border || is_bottom_border || is_left_border || is_right_border;

                if is_border {
                    let mut final_color = border_color;


                    let is_left_inner_edge = has_border && (col == x + border_size - 1);
                    let is_right_inner_edge = has_border && (col == x + width - border_size);


                    if is_top_inner_edge || is_bottom_inner_edge || is_left_inner_edge || is_right_inner_edge {
                        final_color.a = (final_color.a as f32 * 0.6) as u8;
                    }

                    if is_border_opaque && final_color.a == 255 {
                        draw_u32(buffer, buffer_width, col, row, border_u32);
                    } else {
                        if is_bg_opaque { draw_u32(buffer, buffer_width, col, row, bg_u32); } else { draw_pixel(buffer, buffer_width, col, row, color); }

                        draw_pixel(buffer, buffer_width, col, row, final_color);
                    }
                } else {
                    if is_bg_opaque { draw_u32(buffer, buffer_width, col, row, bg_u32); } else { draw_pixel(buffer, buffer_width, col, row, color); }
                }
                continue;
            }


            let lx = (col - x) as f32 + 0.5;
            let mut dx = 0.0;
            let mut dy = 0.0;

            if is_top_row {
                if col < inner_x_start {
                    dx = r - lx;
                    dy = r - ly;
                } else {
                    dx = lx - (width as f32 - r);
                    dy = r - ly;
                }
            } else {
                if col < inner_x_start {
                    dx = r - lx;
                    dy = ly - (height as f32 - r);
                } else {
                    dx = lx - (width as f32 - r);
                    dy = ly - (height as f32 - r);
                }
            }

            let dist_sq = dx * dx + dy * dy;


            if dist_sq > r_sq {
                let dist = sqrt_f64(dist_sq as f64) as f32;

                if dist < r + 1.0 {
                    let alpha_factor = (1.0 - (dist - r)).clamp(0.0, 1.0);

                    let base_color = if has_border { border_color } else { color };
                    let mut final_color = base_color;
                    final_color.a = (base_color.a as f32 * alpha_factor) as u8;
                    draw_pixel(buffer, buffer_width, col, row, final_color);
                }
                continue;
            }

            let dist = sqrt_f64(dist_sq as f64) as f32;


            if has_border {
                let inner_r = r - border_sz_f;

                if dist > inner_r {
                    let mut final_color = border_color;

                    if dist < inner_r + 1.0 {
                        let border_alpha_factor = (dist - inner_r).clamp(0.0, 1.0);


                        if is_bg_opaque { draw_u32(buffer, buffer_width, col, row, bg_u32); } else { draw_pixel(buffer, buffer_width, col, row, color); }


                        final_color.a = (border_color.a as f32 * border_alpha_factor) as u8;
                        draw_pixel(buffer, buffer_width, col, row, final_color);
                    } else {
                        if is_border_opaque { draw_u32(buffer, buffer_width, col, row, border_u32); } else { draw_pixel(buffer, buffer_width, col, row, border_color); }
                    }
                } else {
                    if is_bg_opaque { draw_u32(buffer, buffer_width, col, row, bg_u32); } else { draw_pixel(buffer, buffer_width, col, row, color); }
                }
            } else {
                if dist > r - 1.0 {
                    let _alpha_factor = (r - dist).clamp(0.0, 1.0);


                    let _alpha_factor = (r + 1.0 - dist).clamp(0.0, 1.0);


                    if is_bg_opaque { draw_u32(buffer, buffer_width, col, row, bg_u32); } else { draw_pixel(buffer, buffer_width, col, row, color); }
                } else {
                    if is_bg_opaque { draw_u32(buffer, buffer_width, col, row, bg_u32); } else { draw_pixel(buffer, buffer_width, col, row, color); }
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct TextSegment {
    start: usize,
    end: usize,
    color: Color,
    bg_color: Color,
    size: f32,
}

fn parse_ansi_text(text: &str, default_color: Color, default_size: f32) -> (Vec<TextSegment>, alloc::string::String) {
    let mut segments: Vec<TextSegment> = Vec::new();
    let mut clean_text = alloc::string::String::new();

    let mut current_fg = default_color;
    let mut current_bg = Color::rgba(0, 0, 0, 0);
    let current_size = default_size;
    let mut is_bold = false;
    let mut is_inverse = false;

    let mut chars = text.chars().peekable();
    let mut clean_pos = 0;

    while let Some(c) = chars.next() {
        if c == '\x1B' && chars.peek() == Some(&'[') {
            chars.next();

            let mut params_str = alloc::string::String::new();


            while let Some(&p) = chars.peek() {
                if p >= '\x20' && p <= '\x3F' {
                    chars.next();
                    params_str.push(p);
                } else {
                    break;
                }
            }


            let mut final_char = '\0';
            if let Some(&f) = chars.peek() {
                if f >= '\x40' && f <= '\x7E' {
                    chars.next();
                    final_char = f;
                }
            }


            if final_char == 'm' {
                let parts: Vec<&str> = params_str.split(';').collect();


                if parts.len() == 1 && parts[0].is_empty() {
                    current_fg = default_color;
                    current_bg = Color::rgba(0, 0, 0, 0);
                    is_bold = false;
                    is_inverse = false;
                } else {
                    let mut i = 0;
                    while i < parts.len() {
                        let part = parts[i].trim_start_matches('?');

                        if let Ok(code) = part.parse::<u8>() {
                            match code {
                                0 => {
                                    current_fg = default_color;
                                    current_bg = Color::rgba(0, 0, 0, 0);
                                    is_bold = false;
                                    is_inverse = false;
                                }
                                1 => is_bold = true,
                                7 => is_inverse = true,
                                27 => is_inverse = false,
                                39 => current_fg = default_color,
                                49 => current_bg = Color::rgba(0, 0, 0, 0),

                                30..=37 => {
                                    let idx = code - 30;
                                    let colors = [
                                        Color::rgb(0, 0, 0), Color::rgb(170, 0, 0), Color::rgb(0, 170, 0), Color::rgb(170, 85, 0),
                                        Color::rgb(0, 0, 170), Color::rgb(170, 0, 170), Color::rgb(0, 170, 170), Color::rgb(170, 170, 170)
                                    ];
                                    current_fg = colors[idx as usize];
                                }
                                90..=97 => {
                                    let idx = code - 90;
                                    let colors = [
                                        Color::rgb(85, 85, 85), Color::rgb(255, 85, 85), Color::rgb(85, 255, 85), Color::rgb(255, 255, 85),
                                        Color::rgb(85, 85, 255), Color::rgb(255, 85, 255), Color::rgb(85, 255, 255), Color::rgb(255, 255, 255)
                                    ];
                                    current_fg = colors[idx as usize];
                                }
                                40..=47 => {
                                    let idx = code - 40;
                                    let colors = [
                                        Color::rgb(0, 0, 0), Color::rgb(170, 0, 0), Color::rgb(0, 170, 0), Color::rgb(170, 85, 0),
                                        Color::rgb(0, 0, 170), Color::rgb(170, 0, 170), Color::rgb(0, 170, 170), Color::rgb(170, 170, 170)
                                    ];
                                    current_bg = colors[idx as usize];
                                }
                                100..=107 => {
                                    let idx = code - 100;
                                    let colors = [
                                        Color::rgb(85, 85, 85), Color::rgb(255, 85, 85), Color::rgb(85, 255, 85), Color::rgb(255, 255, 85),
                                        Color::rgb(85, 85, 255), Color::rgb(255, 85, 255), Color::rgb(85, 255, 255), Color::rgb(255, 255, 255)
                                    ];
                                    current_bg = colors[idx as usize];
                                }
                                38 | 48 => {
                                    let is_bg = code == 48;
                                    if i + 1 < parts.len() {
                                        if parts[i + 1] == "2" && i + 4 < parts.len() {
                                            let r = parts[i + 2].parse::<u8>().unwrap_or(0);
                                            let g = parts[i + 3].parse::<u8>().unwrap_or(0);
                                            let b = parts[i + 4].parse::<u8>().unwrap_or(0);
                                            let c = Color::rgb(r, g, b);
                                            if is_bg { current_bg = c; } else { current_fg = c; }
                                            i += 4;
                                        } else if parts[i + 1] == "5" && i + 2 < parts.len() {
                                            i += 2;
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        i += 1;
                    }
                }
            }
        } else {
            if c == '\x1B' { continue; }

            let start = clean_pos;
            clean_text.push(c);
            clean_pos += 1;

            let mut eff_fg = current_fg;
            let mut eff_bg = current_bg;

            if is_bold {
                if eff_fg.r < 128 && eff_fg.g < 128 && eff_fg.b < 128 {
                    eff_fg.r = eff_fg.r.saturating_add(85);
                    eff_fg.g = eff_fg.g.saturating_add(85);
                    eff_fg.b = eff_fg.b.saturating_add(85);
                }
            }

            if is_inverse {
                let tmp = eff_fg;


                eff_fg = if eff_bg.a == 0 { Color::rgb(0, 0, 0) } else { eff_bg };
                eff_bg = tmp;
                eff_bg.a = 255;
            }

            if let Some(last) = segments.last_mut() {
                if last.color == eff_fg && last.bg_color == eff_bg && last.size == current_size && last.end == start {
                    last.end += 1;
                    continue;
                }
            }

            segments.push(TextSegment {
                start,
                end: clean_pos,
                color: eff_fg,
                bg_color: eff_bg,
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
) -> usize {
    if buffer_width == 0 {
        return 0;
    }

    let (segments, clean_text) = parse_ansi_text(text, default_color, default_size);

    let mut current_x = x;
    let start_y_isize = y as isize - scroll_y as isize;
    let mut current_baseline_isize = start_y_isize;

    let chars: Vec<char> = clean_text.chars().collect();
    let mut i = 0;

    let limit_y = clip_y + max_height;
    let mut max_line_height_row = (default_size * 1.2) as usize;

    while i < chars.len() {
        let c = chars[i];

        let segment = segments.iter()
            .find(|s| i >= s.start && i < s.end)
            .copied()
            .unwrap_or(TextSegment {
                start: 0,
                end: clean_text.len(),
                color: default_color,
                bg_color: Color::rgba(0, 0, 0, 0),
                size: default_size,
            });

        let current_line_height = (segment.size * 1.2) as usize;
        if current_line_height > max_line_height_row {
            max_line_height_row = current_line_height;
        }

        if c == '\n' {
            current_x = x;
            current_baseline_isize += max_line_height_row as isize;
            max_line_height_row = (default_size * 1.2) as usize;
            i += 1;
            continue;
        }

        let (metrics, bitmap) = font.get_char::<true>(c, segment.size);

        let next_x_end = (current_x as isize + metrics.left_side_bearing + metrics.advance_width as isize) as usize;

        if max_width > 0 && next_x_end >= x + max_width {
            if current_x == x {
                current_x += metrics.advance_width;
                i += 1;
                continue;
            }
            current_x = x;
            current_baseline_isize += max_line_height_row as isize;
            max_line_height_row = current_line_height;
            continue;
        }

        let glyph_y_start = (current_baseline_isize + metrics.base_line as isize) as isize;
        let glyph_x = (current_x as isize + metrics.left_side_bearing) as usize;


        if segment.bg_color.a > 0 {
            let bg_x = current_x;
            let bg_w = metrics.advance_width;


            let bg_y_isize = current_baseline_isize - (segment.size * 0.8) as isize;
            let bg_h = (segment.size * 1.2) as usize;


            if bg_y_isize + (bg_h as isize) >= clip_y as isize && bg_y_isize <= limit_y as isize {
                for r in 0..bg_h {
                    let dy = bg_y_isize + r as isize;
                    if dy < clip_y as isize { continue; }
                    let udy = dy as usize;
                    if max_height > 0 && udy >= clip_y + max_height { break; }
                    if udy >= buffer.len() / buffer_width { break; }

                    for cx in 0..bg_w {
                        let dx = bg_x + cx;
                        if dx >= buffer_width { break; }
                        if max_width > 0 && dx >= x + max_width { break; }

                        draw_pixel(buffer, buffer_width, dx, udy, segment.bg_color);
                    }
                }
            }
        }

        if glyph_y_start + (metrics.height as isize) >= clip_y as isize && glyph_y_start <= limit_y as isize {
            for row in 0..metrics.height {
                let dest_y_isize = glyph_y_start + row as isize;

                if dest_y_isize < clip_y as isize { continue; }

                let dest_y = dest_y_isize as usize;

                if max_height > 0 && dest_y >= clip_y + max_height { continue; }
                if dest_y >= buffer.len() / buffer_width { continue; }

                for col in 0..metrics.width {
                    let dest_x = glyph_x + col;
                    if dest_x >= buffer_width { continue; }
                    if max_width > 0 && dest_x >= x + max_width { continue; }

                    let bitmap_alpha = bitmap[row * metrics.width + col];
                    if bitmap_alpha > 0 {
                        let mut pixel_color = segment.color;
                        pixel_color.a = ((pixel_color.a as u16 * bitmap_alpha as u16) / 255) as u8;
                        draw_pixel(buffer, buffer_width, dest_x, dest_y, pixel_color);
                    }
                }
            }
        }

        current_x += metrics.advance_width;
        i += 1;
    }


    (current_baseline_isize - start_y_isize + max_line_height_row as isize).max(0) as usize
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

use crate::types::{BackgroundStyle, GradientDirection, LinearGradient};


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
                    dist = sqrt_f64((dx * dx + dy * dy) as f64) as f32;
                    in_corner = true;
                } else if lx > w_f - r {
                    let dx = lx - (w_f - r);
                    let dy = r - ly;
                    dist = sqrt_f64((dx * dx + dy * dy) as f64) as f32;
                    in_corner = true;
                }
            } else if ly > h_f - r {
                if lx < r {
                    let dx = r - lx;
                    let dy = ly - (h_f - r);
                    dist = sqrt_f64((dx * dx + dy * dy) as f64) as f32;
                    in_corner = true;
                } else if lx > w_f - r {
                    let dx = lx - (w_f - r);
                    let dy = ly - (h_f - r);
                    dist = sqrt_f64((dx * dx + dy * dy) as f64) as f32;
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
    border_size: usize,
    border_color: Color,
) {
    match style {
        BackgroundStyle::Solid(color) => {
            draw_square(buffer, buffer_width, x, y, width, height, rounding, *color, border_size, border_color);
        }
        BackgroundStyle::Gradient(gradient) => {
            draw_square_gradient(buffer, buffer_width, x, y, width, height, rounding, gradient);


            if border_size > 0 && border_color.a > 0 {
                draw_border_only(buffer, buffer_width, x, y, width, height, rounding, border_size, border_color);
            }
        }
    }
}

fn draw_border_only(
    buffer: &mut [u32],
    buffer_width: usize,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    rounding: Size,
    border_size: usize,
    border_color: Color,
) {
    draw_square(buffer, buffer_width, x, y, width, height, rounding, Color::rgba(0, 0, 0, 0), border_size, border_color);
}
