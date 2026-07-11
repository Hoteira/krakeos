//! Software rendering primitives. Everything blends properly: a Color with
//! a < 255 is srcover-composited instead of being written raw (the old apps
//! wrote `alpha<<24 | rgb` straight into the buffer, which the opaque
//! kernel compositor then displayed as-is).

pub mod primitives {
    use crate::font::Font;
    use crate::math::sqrt_f64;
    use crate::types::{BackgroundStyle, Color, GradientDirection, Size};

    #[inline]
    pub fn blend(dst: u32, color: Color) -> u32 {
        let a = color.a as u32;
        if a == 255 {
            return 0xFF000000 | ((color.r as u32) << 16) | ((color.g as u32) << 8) | color.b as u32;
        }
        if a == 0 {
            return dst;
        }
        let inv = 255 - a;
        let r = (color.r as u32 * a + ((dst >> 16) & 0xFF) * inv) / 255;
        let g = (color.g as u32 * a + ((dst >> 8) & 0xFF) * inv) / 255;
        let b = (color.b as u32 * a + (dst & 0xFF) * inv) / 255;
        0xFF000000 | (r << 16) | (g << 8) | b
    }

    /// Filled rect with alpha blending; opaque fills use fast row fills.
    pub fn fill_rect(
        fb: &mut [u32],
        stride: usize,
        x: usize,
        y: usize,
        w: usize,
        h: usize,
        color: Color,
    ) {
        if stride == 0 || color.a == 0 {
            return;
        }
        let fb_h = fb.len() / stride;
        let x1 = (x + w).min(stride);
        let y1 = (y + h).min(fb_h);
        if x >= x1 || y >= y1 {
            return;
        }
        if color.a == 255 {
            let px = color.to_u32();
            let w_px = x1 - x;
            let first = y * stride + x;
            fb[first..first + w_px].fill(px);
            for py in (y + 1)..y1 {
                let dst = py * stride + x;
                fb.copy_within(first..first + w_px, dst);
            }
        } else {
            for py in y..y1 {
                let row = py * stride;
                for px in x..x1 {
                    fb[row + px] = blend(fb[row + px], color);
                }
            }
        }
    }

    fn lerp_color(a: Color, b: Color, t: f32) -> Color {
        let l = |x: u8, y: u8| -> u8 { (x as f32 + (y as f32 - x as f32) * t) as u8 };
        Color::rgba(l(a.r, b.r), l(a.g, b.g), l(a.b, b.b), l(a.a, b.a))
    }

    /// Row inset for a rounded corner: how many pixels to skip at `dy` rows
    /// from the corner's circle center.
    fn corner_inset(radius: usize, dy: usize) -> usize {
        if dy >= radius {
            return 0;
        }
        let r = radius as f64;
        let d = (radius - dy) as f64 - 0.5;
        let x = sqrt_f64((r * r - d * d).max(0.0));
        (r - x) as usize
    }

    /// Solid or gradient background with optional rounded corners + border.
    /// Row-based so opaque solid fills stay fast.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_background_style(
        fb: &mut [u32],
        stride: usize,
        x: usize,
        y: usize,
        w: usize,
        h: usize,
        border_radius: Size,
        background: &BackgroundStyle,
        border_size: usize,
        border_color: Color,
    ) {
        if stride == 0 || w == 0 || h == 0 {
            return;
        }
        let fb_h = fb.len() / stride;
        let radius = match border_radius {
            Size::Absolute(r) => r.min(w / 2).min(h / 2),
            _ => 0,
        };

        for row in 0..h {
            let py = y + row;
            if py >= fb_h {
                break;
            }
            // Rounded-corner inset for this row
            let inset = if row < radius {
                corner_inset(radius, row)
            } else if row >= h - radius {
                corner_inset(radius, h - 1 - row)
            } else {
                0
            };
            let rx = x + inset;
            let rw = w.saturating_sub(inset * 2);
            if rw == 0 {
                continue;
            }

            let color = match background {
                BackgroundStyle::Solid(c) => *c,
                BackgroundStyle::Gradient(g) => {
                    let t = match g.direction {
                        GradientDirection::Vertical => row as f32 / h.max(1) as f32,
                        _ => 0.0, // horizontal handled per-pixel below
                    };
                    match g.direction {
                        GradientDirection::Horizontal => {
                            // per-pixel horizontal gradient row
                            let row_base = py * stride;
                            let x1 = (rx + rw).min(stride);
                            for px in rx..x1 {
                                let t = (px - x) as f32 / w.max(1) as f32;
                                let c = lerp_color(g.start_color, g.end_color, t);
                                fb[row_base + px] = blend(fb[row_base + px], c);
                            }
                            continue;
                        }
                        _ => lerp_color(g.start_color, g.end_color, t),
                    }
                }
            };
            fill_rect(fb, stride, rx, py, rw, 1, color);
        }

        // Simple border: top/bottom strips + left/right edges
        if border_size > 0 && border_color.a > 0 {
            fill_rect(fb, stride, x + radius, y, w.saturating_sub(radius * 2), border_size, border_color);
            fill_rect(fb, stride, x + radius, (y + h).saturating_sub(border_size), w.saturating_sub(radius * 2), border_size, border_color);
            fill_rect(fb, stride, x, y + radius, border_size, h.saturating_sub(radius * 2), border_color);
            fill_rect(fb, stride, (x + w).saturating_sub(border_size), y + radius, border_size, h.saturating_sub(radius * 2), border_color);
        }
    }

    /// Word-wrapped, scrollable text block. `baseline_y` is the baseline of
    /// the first line; the block is clipped to `view_height` starting at
    /// `clip_top`. Returns the full content height (for scrollbars).
    #[allow(clippy::too_many_arguments)]
    pub fn draw_text_formatted(
        fb: &mut [u32],
        stride: usize,
        x: usize,
        baseline_y: usize,
        text: &str,
        font: &mut Font,
        size: f32,
        color: Color,
        max_width: usize,
        scroll_offset_y: usize,
        view_height: usize,
        clip_top: usize,
    ) -> usize {
        if stride == 0 {
            return 0;
        }
        let line_h = font.line_height(size).max(1);
        let color_u32 =
            ((color.r as u32) << 16) | ((color.g as u32) << 8) | color.b as u32;

        let clip_bottom = clip_top + view_height;
        let mut line_idx: usize = 0;

        #[allow(clippy::too_many_arguments)]
        fn draw_line(
            fb: &mut [u32], stride: usize, x: usize, baseline_y: usize,
            font: &mut Font, size: f32, color_u32: u32, line: &str,
            line_idx: usize, line_h: usize, scroll_offset_y: usize,
            clip_top: usize, clip_bottom: usize,
        ) {
            let ly = baseline_y as i64 + (line_idx * line_h) as i64 - scroll_offset_y as i64;
            // Clip whole lines outside the view
            if ly + (line_h as i64) < clip_top as i64 || ly - (line_h as i64) > clip_bottom as i64 {
                return;
            }
            if ly < 0 {
                return;
            }
            font.draw_text(fb, stride, x, ly as usize, line, size, color_u32);
        }

        macro_rules! emit {
            ($line:expr) => {
                draw_line(fb, stride, x, baseline_y, font, size, color_u32,
                          $line, line_idx, line_h, scroll_offset_y, clip_top, clip_bottom);
                line_idx += 1;
            };
        }

        for raw_line in text.split('\n') {
            if max_width == 0 {
                emit!(raw_line);
                continue;
            }
            // Greedy wrap on words; fall back to hard char wrap
            let mut current = String::new();
            let mut current_w = 0usize;
            for word in raw_line.split_inclusive(' ') {
                let word_w: usize = word.chars().map(|c| font.char_advance(c, size)).sum();
                if current_w + word_w > max_width && !current.is_empty() {
                    emit!(&current);
                    current.clear();
                    current_w = 0;
                }
                if word_w > max_width {
                    // Hard-wrap a single overlong word
                    for c in word.chars() {
                        let cw = font.char_advance(c, size);
                        if current_w + cw > max_width && !current.is_empty() {
                            emit!(&current);
                            current.clear();
                            current_w = 0;
                        }
                        current.push(c);
                        current_w += cw;
                    }
                } else {
                    current.push_str(word);
                    current_w += word_w;
                }
            }
            emit!(&current);
        }

        line_idx * line_h
    }
}
