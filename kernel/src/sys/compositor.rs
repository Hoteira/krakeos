use alloc::collections::VecDeque;
use alloc::vec::Vec;

pub const MAX_WINDOWS: usize = 16;
pub const COMPOSITOR_FD_BASE: usize = 0x4000_0000;

pub struct Window {
    pub active: bool,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub z_order: usize,
    pub buffer: Vec<u32>,
    /// Mouse events routed to this window (content-local coords), read by the
    /// owning app through its window fd. Each entry is 8 bytes:
    /// [x:u16][y:u16][button:u8][pressed:u8][reserved:u16].
    pub mouse_events: VecDeque<[u8; 8]>,
}

pub struct Compositor {
    pub windows: [Window; MAX_WINDOWS],
}

const INIT_WINDOW: Window = Window {
    active: false,
    x: 0,
    y: 0,
    width: 0,
    height: 0,
    z_order: 0,
    buffer: Vec::new(),
    mouse_events: VecDeque::new(),
};

pub static mut COMPOSITOR: Compositor = Compositor {
    windows: [INIT_WINDOW, INIT_WINDOW, INIT_WINDOW, INIT_WINDOW,
              INIT_WINDOW, INIT_WINDOW, INIT_WINDOW, INIT_WINDOW,
              INIT_WINDOW, INIT_WINDOW, INIT_WINDOW, INIT_WINDOW,
              INIT_WINDOW, INIT_WINDOW, INIT_WINDOW, INIT_WINDOW],
};

// Bumped on every visible change (window create/close/move/draw). The kernel
// compositor recomposes when it changes; the shell can also read it via the
// window-state fd.
pub static mut CONTENT_GEN: u32 = 0;

pub fn bump_gen() {
    unsafe {
        let gen = &mut *core::ptr::addr_of_mut!(CONTENT_GEN);
        *gen = gen.wrapping_add(1);
    }
}

// ---- Kernel-side compositing ----
//
// The shell only renders the base layer (wallpaper, icons, taskbar) and
// window *management*; windows + decorations are composed here in native
// code. Compositing in the shell meant every window blit ran under the
// wasmi interpreter (~30-100ms/frame); here the same work is ~1ms.

const FBW: usize = crate::drivers::virtio_gpu::FB_WIDTH as usize;
const FBH: usize = crate::drivers::virtio_gpu::FB_HEIGHT as usize;
const TITLE_H: usize = 24;
const TASKBAR_H: usize = 40;
const COL_TITLE: u32 = 0xFF444444;
const COL_CLOSE: u32 = 0xFFFF3333;

// Base layer written by the shell via /dev/gpu/fb
pub static mut BASE: [u32; FBW * FBH] = [0xFF000000; FBW * FBH];
pub static mut BASE_DIRTY: bool = false;

static mut LAST_COMPOSED_GEN: u32 = u32::MAX;
static mut LAST_COMPOSE_TIME: u64 = 0;

// Accumulated damage rectangle [x0, y0, x1, y1) (exclusive high). compose()
// only rebuilds + flushes this region, so a window drag costs the union of
// its old and new footprints instead of the whole screen. `None` = clean.
static mut DAMAGE: Option<(usize, usize, usize, usize)> = None;

/// Expand the damage rectangle to include the given pixel rect (clamped to
/// the framebuffer). Call on any change to on-screen content.
pub fn expand_damage(x: usize, y: usize, w: usize, h: usize) {
    if w == 0 || h == 0 {
        return;
    }
    let x0 = x.min(FBW);
    let y0 = y.min(FBH);
    let x1 = (x + w).min(FBW);
    let y1 = (y + h).min(FBH);
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    unsafe {
        let d = &mut *core::ptr::addr_of_mut!(DAMAGE);
        *d = Some(match *d {
            None => (x0, y0, x1, y1),
            Some((ax0, ay0, ax1, ay1)) => {
                (ax0.min(x0), ay0.min(y0), ax1.max(x1), ay1.max(y1))
            }
        });
    }
}

pub fn damage_all() {
    expand_damage(0, 0, FBW, FBH);
}

/// Called from the timer tick: recompose at most every ~16ms and only when
/// something actually changed.
pub fn maybe_compose() {
    unsafe {
        if !crate::drivers::virtio_gpu::is_ready() { return; }
        let gen = *core::ptr::addr_of!(CONTENT_GEN);
        let dirty = *core::ptr::addr_of!(BASE_DIRTY);
        if !dirty && gen == *core::ptr::addr_of!(LAST_COMPOSED_GEN) { return; }

        let now = crate::csr::read_time();
        if now.wrapping_sub(*core::ptr::addr_of!(LAST_COMPOSE_TIME)) < 160_000 { return; }

        *core::ptr::addr_of_mut!(LAST_COMPOSE_TIME) = now;
        *core::ptr::addr_of_mut!(LAST_COMPOSED_GEN) = gen;
        *core::ptr::addr_of_mut!(BASE_DIRTY) = false;
        compose();
    }
}

fn compose() {
    unsafe {
        // The damage rectangle bounds everything we rebuild + flush. If a
        // recompose was triggered without explicit damage, be safe.
        let (dx0, dy0, dx1, dy1) = match *core::ptr::addr_of!(DAMAGE) {
            Some(r) => r,
            None => (0, 0, FBW, FBH),
        };
        *core::ptr::addr_of_mut!(DAMAGE) = None;
        if dx0 >= dx1 || dy0 >= dy1 {
            return;
        }

        let fb = core::ptr::addr_of_mut!(crate::drivers::virtio_gpu::FB_MEM) as *mut u32;
        let base = core::ptr::addr_of!(BASE) as *const u32;
        let dw = dx1 - dx0;

        // 1. Base layer, damage rows only.
        for y in dy0..dy1 {
            let off = y * FBW + dx0;
            core::ptr::copy_nonoverlapping(base.add(off), fb.add(off), dw);
        }

        // 2. Windows, back to front (insertion sort by z_order — max 16).
        let comp = &*core::ptr::addr_of!(COMPOSITOR);
        let mut order = [0usize; MAX_WINDOWS];
        let mut n = 0;
        for i in 1..MAX_WINDOWS {
            let w = &comp.windows[i];
            if w.active && w.width > 0 && w.height > 0 {
                order[n] = i;
                n += 1;
            }
        }
        let mut k = 1;
        while k < n {
            let mut j = k;
            while j > 0 && comp.windows[order[j - 1]].z_order > comp.windows[order[j]].z_order {
                order.swap(j - 1, j);
                j -= 1;
            }
            k += 1;
        }
        for k in 0..n {
            draw_window(fb, &comp.windows[order[k]], dx0, dy0, dx1, dy1);
        }

        // 3. Taskbar strip always on top (only if the damage reaches it).
        let bar_y = FBH - TASKBAR_H;
        if dy1 > bar_y {
            let y0 = dy0.max(bar_y);
            for y in y0..dy1 {
                let off = y * FBW + dx0;
                core::ptr::copy_nonoverlapping(base.add(off), fb.add(off), dw);
            }
        }

        crate::drivers::virtio_gpu::flush_rect(
            dx0 as u32,
            dy0 as u32,
            dw as u32,
            (dy1 - dy0) as u32,
        );
    }
}

/// Draw a window, clipped to the damage rect [cx0,cy0,cx1,cy1).
unsafe fn draw_window(
    fb: *mut u32,
    win: &Window,
    cx0: usize,
    cy0: usize,
    cx1: usize,
    cy1: usize,
) {
    let x = win.x as usize;
    let y = win.y as usize;
    let w = win.width as usize;
    if x >= FBW || w == 0 {
        return;
    }
    let vis_w = w.min(FBW - x);
    // Horizontal clip to damage.
    let px0 = x.max(cx0);
    let px1 = (x + vis_w).min(cx1);
    if px0 >= px1 {
        return;
    }
    let close_start = if vis_w > TITLE_H { vis_w - TITLE_H } else { 0 };

    // Title bar + close button.
    for row in 0..TITLE_H {
        let sy = y + row;
        if sy >= FBH || sy < cy0 || sy >= cy1 {
            continue;
        }
        for sx in px0..px1 {
            let local = sx - x;
            *fb.add(sy * FBW + sx) = if local >= close_start { COL_CLOSE } else { COL_TITLE };
        }
    }

    // Content.
    let rows = (win.height as usize).min(win.buffer.len() / w.max(1));
    let src = win.buffer.as_ptr();
    for row in 0..rows {
        let sy = y + TITLE_H + row;
        if sy >= FBH || sy < cy0 || sy >= cy1 {
            continue;
        }
        let src_row = src.add(row * w + (px0 - x));
        core::ptr::copy_nonoverlapping(src_row, fb.add(sy * FBW + px0), px1 - px0);
    }
}

pub fn create_window() -> Option<usize> {
    unsafe {
        let comp = &mut *core::ptr::addr_of_mut!(COMPOSITOR);
        let mut max_z = 0;
        for i in 0..16 {
            if comp.windows[i].active && comp.windows[i].z_order > max_z {
                max_z = comp.windows[i].z_order;
            }
        }
        
        // Slot 0 is reserved: the shell treats window 0 as its own (no title
        // bar, clicks ignored), so apps must never be handed it.
        for i in 1..16 {
            if !comp.windows[i].active {
                comp.windows[i].active = true;
                comp.windows[i].x = 100 + (i as u32 * 20);
                comp.windows[i].y = 100 + (i as u32 * 20);
                comp.windows[i].width = 400;
                comp.windows[i].height = 500;
                comp.windows[i].z_order = max_z + 1;
                comp.windows[i].buffer.resize((400 * 500) as usize, 0xFF000000);
                comp.windows[i].mouse_events.clear();
                let (wx, wy) = (comp.windows[i].x as usize, comp.windows[i].y as usize);
                expand_damage(wx, wy, 400, 500 + TITLE_H);
                bump_gen();
                return Some(i);
            }
        }
        None
    }
}

pub fn write_window(win_id: usize, data: &[u8]) -> usize {
    if win_id >= MAX_WINDOWS { return 0; }
    unsafe {
        let win = &mut (*core::ptr::addr_of_mut!(COMPOSITOR)).windows[win_id];
        if !win.active || data.len() < 16 { return 0; }
        
        let mut buf = [0u8; 4];
        buf.copy_from_slice(&data[0..4]);
        let x = u32::from_le_bytes(buf);
        buf.copy_from_slice(&data[4..8]);
        let y = u32::from_le_bytes(buf);
        buf.copy_from_slice(&data[8..12]);
        let w = u32::from_le_bytes(buf);
        buf.copy_from_slice(&data[12..16]);
        let h = u32::from_le_bytes(buf);

        // x == 0xFFFF_FFFF is a resize request: (w, h) become the new
        // window dimensions (inkui apps use this to pick their size).
        if x == 0xFFFF_FFFF {
            let w = w.min(crate::drivers::virtio_gpu::FB_WIDTH);
            let h = h.min(crate::drivers::virtio_gpu::FB_HEIGHT);
            if w > 0 && h > 0 && (w != win.width || h != win.height) {
                let (wx, wy) = (win.x as usize, win.y as usize);
                let old_w = win.width as usize;
                let old_h = win.height as usize;
                win.width = w;
                win.height = h;
                win.buffer.clear();
                win.buffer.resize(w as usize * h as usize, 0xFF000000);
                // Damage both the old and new footprints.
                expand_damage(wx, wy, old_w.max(w as usize), old_h.max(h as usize) + TITLE_H);
                bump_gen();
            }
            return data.len();
        }

        let pixels_data = &data[16..];

        // Clip the destination rect against the window so a bad rect can
        // never index past the window buffer.
        if x >= win.width || y >= win.height || w == 0 || h == 0 {
            return data.len();
        }
        let copy_w = w.min(win.width - x) as usize;
        let copy_h = h.min(win.height - y) as usize;
        let src_stride = w as usize * 4;

        for row in 0..copy_h {
            let src_off = row * src_stride;
            if src_off + copy_w * 4 > pixels_data.len() { break; }
            let dst_idx = (y as usize + row) * win.width as usize + x as usize;
            core::ptr::copy_nonoverlapping(
                pixels_data.as_ptr().add(src_off),
                win.buffer.as_mut_ptr().add(dst_idx) as *mut u8,
                copy_w * 4,
            );
        }
        // Damage the on-screen region this write touched (offset by title bar).
        let sx = win.x as usize + x as usize;
        let sy = win.y as usize + TITLE_H + y as usize;
        expand_damage(sx, sy, copy_w, copy_h);
        bump_gen();
        data.len()
    }
}

/// Route a mouse button event to the top-most window whose content area
/// (below the title bar) contains the cursor. Coordinates are screen pixels;
/// the queued event uses window-content-local coordinates. The shell handles
/// title bars and desktop chrome separately from its own mouse reads.
pub fn route_mouse(cursor_x: u32, cursor_y: u32, button: u8, pressed: u8) {
    unsafe {
        let comp = &mut *core::ptr::addr_of_mut!(COMPOSITOR);
        let mut best: Option<usize> = None;
        let mut best_z = 0usize;
        for i in 1..MAX_WINDOWS {
            let w = &comp.windows[i];
            if !w.active || w.width == 0 || w.height == 0 {
                continue;
            }
            let cx0 = w.x;
            let cy0 = w.y + TITLE_H as u32;
            let cx1 = w.x + w.width;
            let cy1 = cy0 + w.height;
            if cursor_x >= cx0 && cursor_x < cx1 && cursor_y >= cy0 && cursor_y < cy1 {
                if best.is_none() || w.z_order > best_z {
                    best = Some(i);
                    best_z = w.z_order;
                }
            }
        }
        if let Some(i) = best {
            let w = &mut comp.windows[i];
            let lx = (cursor_x - w.x) as u16;
            let ly = (cursor_y - (w.y + TITLE_H as u32)) as u16;
            let mut ev = [0u8; 8];
            ev[0..2].copy_from_slice(&lx.to_le_bytes());
            ev[2..4].copy_from_slice(&ly.to_le_bytes());
            ev[4] = button;
            ev[5] = pressed;
            if w.mouse_events.len() < 64 {
                w.mouse_events.push_back(ev);
            }
        }
    }
}

/// Pop one routed mouse event for a window (read through its window fd).
/// Returns 8 on success, 0 if the queue is empty.
pub fn read_window_mouse(win_id: usize, buf: &mut [u8]) -> usize {
    if win_id >= MAX_WINDOWS || buf.len() < 8 {
        return 0;
    }
    unsafe {
        let w = &mut (*core::ptr::addr_of_mut!(COMPOSITOR)).windows[win_id];
        if let Some(ev) = w.mouse_events.pop_front() {
            buf[0..8].copy_from_slice(&ev);
            8
        } else {
            0
        }
    }
}
