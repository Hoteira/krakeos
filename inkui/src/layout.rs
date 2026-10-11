// Rectangles in cell coordinates and splitting them.

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Rect {
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub h: usize,
}

// How much of a split one part takes.
#[derive(Clone, Copy, Debug)]
pub enum Constraint {
    // Exactly n cells (less if there isn't room).
    Len(usize),
    // A percentage of the whole.
    Pct(usize),
    // A share of what's left after Len and Pct parts, by weight.
    Fill(usize),
}

impl Rect {
    pub const fn new(x: usize, y: usize, w: usize, h: usize) -> Rect {
        Rect { x, y, w, h }
    }

    pub fn right(&self) -> usize {
        self.x + self.w
    }

    pub fn bottom(&self) -> usize {
        self.y + self.h
    }

    pub fn is_empty(&self) -> bool {
        self.w == 0 || self.h == 0
    }

    // Shrink by mx cells left/right and my cells top/bottom.
    pub fn inset(&self, mx: usize, my: usize) -> Rect {
        Rect {
            x: self.x + mx.min(self.w / 2),
            y: self.y + my.min(self.h / 2),
            w: self.w.saturating_sub(2 * mx),
            h: self.h.saturating_sub(2 * my),
        }
    }

    pub fn intersect(&self, o: &Rect) -> Rect {
        let x = self.x.max(o.x);
        let y = self.y.max(o.y);
        let r = self.right().min(o.right());
        let b = self.bottom().min(o.bottom());
        Rect { x, y, w: r.saturating_sub(x), h: b.saturating_sub(y) }
    }

    pub fn contains(&self, x: usize, y: usize) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }

    // A w x h rect centered inside this one (clamped to fit).
    pub fn center(&self, w: usize, h: usize) -> Rect {
        let w = w.min(self.w);
        let h = h.min(self.h);
        Rect { x: self.x + (self.w - w) / 2, y: self.y + (self.h - h) / 2, w, h }
    }

    // Take n rows off the top: (top, rest).
    pub fn take_top(&self, n: usize) -> (Rect, Rect) {
        let n = n.min(self.h);
        (Rect { h: n, ..*self }, Rect { y: self.y + n, h: self.h - n, ..*self })
    }

    // Take n rows off the bottom: (rest, bottom).
    pub fn take_bottom(&self, n: usize) -> (Rect, Rect) {
        let n = n.min(self.h);
        (Rect { h: self.h - n, ..*self }, Rect { y: self.bottom() - n, h: n, ..*self })
    }

    // Take n columns off the left: (left, rest).
    pub fn take_left(&self, n: usize) -> (Rect, Rect) {
        let n = n.min(self.w);
        (Rect { w: n, ..*self }, Rect { x: self.x + n, w: self.w - n, ..*self })
    }

    // Split top to bottom.
    pub fn split_v(&self, parts: &[Constraint]) -> Vec<Rect> {
        sizes(self.h, parts)
            .into_iter()
            .scan(self.y, |y, h| {
                let r = Rect { y: *y, h, ..*self };
                *y += h;
                Some(r)
            })
            .collect()
    }

    // Split left to right.
    pub fn split_h(&self, parts: &[Constraint]) -> Vec<Rect> {
        sizes(self.w, parts)
            .into_iter()
            .scan(self.x, |x, w| {
                let r = Rect { x: *x, w, ..*self };
                *x += w;
                Some(r)
            })
            .collect()
    }
}

fn sizes(total: usize, parts: &[Constraint]) -> Vec<usize> {
    let mut out = vec![0; parts.len()];
    let mut left = total;
    for (i, p) in parts.iter().enumerate() {
        let n = match *p {
            Constraint::Len(n) => n,
            Constraint::Pct(p) => total * p / 100,
            Constraint::Fill(_) => continue,
        }
        .min(left);
        out[i] = n;
        left -= n;
    }
    let weights: usize = parts
        .iter()
        .map(|p| if let Constraint::Fill(w) = p { *w } else { 0 })
        .sum();
    if weights > 0 {
        let pool = left;
        let mut given = 0;
        let mut last = None;
        for (i, p) in parts.iter().enumerate() {
            if let Constraint::Fill(w) = p {
                out[i] = pool * w / weights;
                given += out[i];
                last = Some(i);
            }
        }
        if let Some(i) = last {
            out[i] += pool - given; // rounding remainder
        }
    }
    out
}
