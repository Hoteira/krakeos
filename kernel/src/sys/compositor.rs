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
}

pub struct Compositor {
    pub windows: [Window; MAX_WINDOWS],
}

const INIT_WINDOW: Window = Window { active: false, x: 0, y: 0, width: 0, height: 0, z_order: 0, buffer: Vec::new() };

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
        let fb = core::ptr::addr_of_mut!(crate::drivers::virtio_gpu::FB_MEM) as *mut u32;
        let base = core::ptr::addr_of!(BASE) as *const u32;

        // 1. Base layer
        core::ptr::copy_nonoverlapping(base, fb, FBW * FBH);

        // 2. Windows, back to front (insertion sort by z_order — max 16)
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
            draw_window(fb, &comp.windows[order[k]]);
        }

        // 3. Taskbar strip always on top
        let strip = (FBH - TASKBAR_H) * FBW;
        core::ptr::copy_nonoverlapping(base.add(strip), fb.add(strip), TASKBAR_H * FBW);

        crate::drivers::virtio_gpu::flush_rect(0, 0, FBW as u32, FBH as u32);
    }
}

unsafe fn draw_window(fb: *mut u32, win: &Window) {
    let x = win.x as usize;
    let y = win.y as usize;
    let w = win.width as usize;
    if x >= FBW { return; }
    let vis_w = w.min(FBW - x);
    let close_start = if vis_w > TITLE_H { vis_w - TITLE_H } else { 0 };

    // Title bar + close button
    for row in 0..TITLE_H {
        let sy = y + row;
        if sy >= FBH { return; }
        let dst = fb.add(sy * FBW + x);
        for px in 0..close_start { *dst.add(px) = COL_TITLE; }
        for px in close_start..vis_w { *dst.add(px) = COL_CLOSE; }
    }

    // Content
    let rows = (win.height as usize).min(win.buffer.len() / w.max(1));
    let src = win.buffer.as_ptr();
    for row in 0..rows {
        let sy = y + TITLE_H + row;
        if sy >= FBH { return; }
        core::ptr::copy_nonoverlapping(src.add(row * w), fb.add(sy * FBW + x), vis_w);
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
                win.width = w;
                win.height = h;
                win.buffer.clear();
                win.buffer.resize(w as usize * h as usize, 0xFF000000);
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
        bump_gen();
        data.len()
    }
}
