//! Runtime font rasterization via titanf (same engine ref/inkui's stack

//! tools/fontsubset — which parses in milliseconds. The full Nerd font
//! works too, it just takes ~16s to parse under the interpreter.

use std::fs::File;
use std::io::Read;

pub struct Font {
    inner: titanf::TrueTypeFont,
}

impl Font {
    pub fn load(path: &str) -> Option<Font> {
        let mut f = File::options().read(true).open(path).ok()?;
        let mut data = Vec::new();
        f.read_to_end(&mut data).ok()?;
        titanf::TrueTypeFont::load_font(&data)
            .ok()
            .map(|inner| Font { inner })
    }

    /// The fast ASCII subset first, full font as fallback.
    pub fn load_default() -> Option<Font> {
        Self::load("/fonts/CaskaydiaNerd.ttf")
    }

    pub fn line_height(&self, size: f32) -> usize {
        (size * 1.4) as usize
    }

    pub fn char_advance(&mut self, c: char, size: f32) -> usize {
        let (metrics, _) = self.inner.get_char::<true>(c, size);
        metrics.advance_width
    }

    pub fn measure(&mut self, text: &str, size: f32) -> usize {
        text.chars().map(|c| self.char_advance(c, size)).sum()
    }

    /// Single-line draw; `y` is the baseline. Alpha-blended.
    pub fn draw_text(
        &mut self,
        fb: &mut [u32],
        stride: usize,
        x: usize,
        y: usize,
        text: &str,
        size: f32,
        color: u32,
    ) {
        if stride == 0 {
            return;
        }
        let fb_h = fb.len() / stride;
        let fg_r = (color >> 16) & 0xFF;
        let fg_g = (color >> 8) & 0xFF;
        let fg_b = color & 0xFF;

        let mut pen_x = x as i64;
        for c in text.chars() {
            let (metrics, bmp) = self.inner.get_char::<true>(c, size);
            let gw = metrics.width;
            let gh = metrics.height;
            for by in 0..gh {
                let sy = y as i64 + metrics.base_line as i64 + by as i64;
                if sy < 0 || sy as usize >= fb_h {
                    continue;
                }
                let row = sy as usize * stride;
                for bx in 0..gw {
                    let a = bmp[by * gw + bx] as u32;
                    if a == 0 {
                        continue;
                    }
                    let sx = pen_x + metrics.left_side_bearing as i64 + bx as i64;
                    if sx < 0 || sx as usize >= stride {
                        continue;
                    }
                    let idx = row + sx as usize;
                    if a == 255 {
                        fb[idx] = 0xFF000000 | (fg_r << 16) | (fg_g << 8) | fg_b;
                    } else {
                        let bg = fb[idx];
                        let inv = 255 - a;
                        let r = (fg_r * a + ((bg >> 16) & 0xFF) * inv) / 255;
                        let g = (fg_g * a + ((bg >> 8) & 0xFF) * inv) / 255;
                        let b = (fg_b * a + (bg & 0xFF) * inv) / 255;
                        fb[idx] = 0xFF000000 | (r << 16) | (g << 8) | b;
                    }
                }
            }
            pen_x += metrics.advance_width as i64;
        }
    }
}
