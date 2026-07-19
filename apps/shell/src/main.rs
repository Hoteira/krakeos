use std::format;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::string::String;

const FB_WIDTH: u32 = 1024;
const FB_HEIGHT: u32 = 576;

#[repr(C)]
#[derive(Clone, Copy)]
struct WindowState {
    active: u32,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    z_order: u32,
}

fn draw_rect(fb: &mut [u32], x: u32, y: u32, w: u32, h: u32, color: u32) {
    // Clip once up front; the old per-pixel bounds checks and branches were
    // a large interpreted-instruction cost per frame.
    if x >= FB_WIDTH || y >= FB_HEIGHT {
        return;
    }
    let x1 = (x + w).min(FB_WIDTH);
    let y1 = (y + h).min(FB_HEIGHT);
    let a = (color >> 24) & 0xFF;
    if a == 0 || x1 <= x {
        return;
    }

    if a == 255 {
        // Opaque fast path: fill the first row, then replicate it with
        // copy_within — that lowers to wasm memory.copy, which the
        // interpreter executes as a native memcpy. A plain u32 fill loop
        // runs store-by-store under interpretation (~30ms for the taskbar).
        let w_px = (x1 - x) as usize;
        let first_row = (y * FB_WIDTH + x) as usize;
        fb[first_row..first_row + w_px].fill(color);
        for py in (y + 1)..y1 {
            let dst = (py * FB_WIDTH + x) as usize;
            fb.copy_within(first_row..first_row + w_px, dst);
        }
    } else {
        let inv_a = 255 - a;
        let fg_r = ((color >> 16) & 0xFF) * a;
        let fg_g = ((color >> 8) & 0xFF) * a;
        let fg_b = (color & 0xFF) * a;
        for py in y..y1 {
            let row_start = (py * FB_WIDTH) as usize;
            for px in x..x1 {
                let idx = row_start + px as usize;
                let cur = fb[idx];
                let out_r = (fg_r + ((cur >> 16) & 0xFF) * inv_a) / 255;
                let out_g = (fg_g + ((cur >> 8) & 0xFF) * inv_a) / 255;
                let out_b = (fg_b + (cur & 0xFF) * inv_a) / 255;
                fb[idx] = 0xFF000000 | (out_r << 16) | (out_g << 8) | out_b;
            }
        }
    }
}

fn draw_text(
    fb: &mut [u32],
    font: &mut inkui::Font,
    text: &str,
    start_x: u32,
    start_y: u32,
    scale: f32,
    fg_color: u32,
) {
    // Rasterized on demand by titanf (cached), blended in inkui.
    font.draw_text(
        fb,
        FB_WIDTH as usize,
        start_x as usize,
        start_y as usize,
        text,
        scale,
        fg_color & 0xFFFFFF,
    );
}

fn load_png(path: &str) -> Option<(u32, u32, Vec<u32>)> {
    let mut file = File::options().read(true).open(path).ok()?;
    let mut data = Vec::new();
    file.read_to_end(&mut data).ok()?;

    let decoder = png::Decoder::new(data.as_slice());
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;

    let width = info.width;
    let height = info.height;
    let mut pixels = vec![0u32; (width * height) as usize];

    for i in 0..(width * height) as usize {
        let r = buf[i * 4];
        let g = buf[i * 4 + 1];
        let b = buf[i * 4 + 2];
        let a = buf[i * 4 + 3];
        pixels[i] = ((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
    }

    Some((width, height, pixels))
}

fn draw_image(fb: &mut [u32], img: &(u32, u32, Vec<u32>), x: u32, y: u32) {
    let (iw, ih, ref pixels) = img;
    for py in 0..*ih {
        for px in 0..*iw {
            let sx = x + px;
            let sy = y + py;
            if sx < FB_WIDTH && sy < FB_HEIGHT {
                let color = pixels[(py * iw + px) as usize];
                let a = (color >> 24) & 0xFF;
                if a == 255 {
                    fb[(sy * FB_WIDTH + sx) as usize] = color;
                } else if a > 0 {
                    let current = fb[(sy * FB_WIDTH + sx) as usize];
                    let inv_a = 255 - a;
                    let bg_r = (current >> 16) & 0xFF;
                    let bg_g = (current >> 8) & 0xFF;
                    let bg_b = current & 0xFF;
                    let fg_r = (color >> 16) & 0xFF;
                    let fg_g = (color >> 8) & 0xFF;
                    let fg_b = color & 0xFF;
                    let out_r = (fg_r * a + bg_r * inv_a) / 255;
                    let out_g = (fg_g * a + bg_g * inv_a) / 255;
                    let out_b = (fg_b * a + bg_b * inv_a) / 255;
                    fb[(sy * FB_WIDTH + sx) as usize] =
                        0xFF000000 | (out_r << 16) | (out_g << 8) | out_b;
                }
            }
        }
    }
}

fn main() {
    println!("shell: starting Compositor/WM in userspace!");

    let mut fb_file = match File::options().write(true).open("/dev/gpu/fb") {
        Ok(f) => f,
        Err(e) => {
            println!("shell: failed to open /dev/gpu/fb: {:?}", e);
            return;
        }
    };

    let mut windows_meta = match File::options().read(true).open("/dev/system/windows") {
        Ok(f) => f,
        Err(e) => {
            println!("shell: failed to open /dev/system/windows: {:?}", e);
            return;
        }
    };

    let mut mouse = match File::options().read(true).open("/dev/input/mouse") {
        Ok(f) => f,
        Err(e) => {
            println!("shell: failed to open /dev/input/mouse: {:?}", e);
            return;
        }
    };

    let mut time_file = match File::options().read(true).open("/dev/system/time") {
        Ok(f) => f,
        Err(e) => {
            println!("shell: failed to open /dev/system/time: {:?}", e);
            return;
        }
    };

    let mut font_opt = inkui::Font::load_default();
    println!("shell: font loaded: {}", font_opt.is_some());

    let cursor = load_png("/img/cursor1.png");

    // Hardware cursor: upload the image once and let the kernel move the
    // virtio-gpu cursor from its input path. The pointer then tracks the
    // mouse at tick rate instead of at our (interpreted) frame rate.
    let mut hw_cursor = false;
    if let Some((cw, ch, ref px)) = cursor {
        if let Ok(mut cur_dev) = File::options().write(true).open("/dev/gpu/cursor") {
            let mut img = vec![0u32; 64 * 64];
            for y in 0..ch.min(64) {
                for x in 0..cw.min(64) {
                    img[(y * 64 + x) as usize] = px[(y * cw + x) as usize];
                }
            }
            let bytes: &[u8] =
                unsafe { std::slice::from_raw_parts(img.as_ptr() as *const u8, img.len() * 4) };
            hw_cursor = cur_dev.write_all(bytes).is_ok();
        }
    }
    println!("shell: hardware cursor: {}", hw_cursor);

    // Pre-render the static background ONCE (wallpaper scaled to screen + desktop
    // icons + labels). wasmi interprets every pixel op, so rebuilding this each
    // frame would take seconds; per frame we just memcpy it into local_fb.
    let mut bg_fb = vec![0xFF003366u32; (FB_WIDTH * FB_HEIGHT) as usize];

    // Slow path: decode + stretch the PNG (freed right after)
    if let Some((iw, ih, pixels)) = load_png("/img/wallpaper.png") {
        for y in 0..FB_HEIGHT {
            let sy = (y as u64 * ih as u64) / FB_HEIGHT as u64;
            let row = (sy * iw as u64) as usize;
            for x in 0..FB_WIDTH {
                let sx = ((x as u64 * iw as u64) / FB_WIDTH as u64) as usize;
                bg_fb[(y * FB_WIDTH + x) as usize] = 0xFF000000 | (pixels[row + sx] & 0xFFFFFF);
            }
        }
    }

    // Desktop Icons
    draw_rect(&mut bg_fb, 10, 10, 50, 50, 0xFF00AA00); // Calc
    draw_rect(&mut bg_fb, 10, 80, 50, 50, 0xFF555555); // Terminal
    draw_rect(&mut bg_fb, 10, 150, 50, 50, 0xFFD4AA00); // Explorer
    draw_rect(&mut bg_fb, 10, 220, 50, 50, 0xFF2299FF); // Viewer
    draw_rect(&mut bg_fb, 10, 290, 50, 50, 0xFFFF5555); // Editor

    if let Some(ref mut font) = font_opt {
        draw_text(&mut bg_fb, font, "Calc", 15, 65, 12.0, 0xFFFFFF);
        draw_text(&mut bg_fb, font, "Term", 15, 135, 12.0, 0xFFFFFF);
        draw_text(&mut bg_fb, font, "Expl", 15, 205, 12.0, 0xFFFFFF);
        draw_text(&mut bg_fb, font, "View", 15, 275, 12.0, 0xFFFFFF);
        draw_text(&mut bg_fb, font, "Edit", 15, 345, 12.0, 0xFFFFFF);
    }

    let mut local_fb = vec![0u32; (FB_WIDTH * FB_HEIGHT) as usize];

    let mut event_buf = [0u8; 8];
    let mut cursor_x = FB_WIDTH / 2;
    let mut cursor_y = FB_HEIGHT / 2;
    let mut mouse_btn_down = false;
    let mut dragged_win: Option<usize> = None;
    let mut drag_offset_x: i32 = 0;
    let mut drag_offset_y: i32 = 0;

    let taskbar_y = FB_HEIGHT - 40;

    // The kernel composites windows natively; the shell only renders the
    // base layer (wallpaper/icons/taskbar/clock), so it re-renders rarely.
    let mut last_time_str = String::new();
    let mut frame_ms: u32 = 0;

    loop {
        // 1. Read Window States
        let mut meta_buf = [0u8; 16 * 24];
        let mut states: [WindowState; 16] = [WindowState {
            active: 0,
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            z_order: 0,
        }; 16];
        let _ = windows_meta.seek(SeekFrom::Start(0));
        if let Ok(bytes) = windows_meta.read(&mut meta_buf) {
            if bytes >= 16 * 24 {
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        meta_buf.as_ptr(),
                        states.as_mut_ptr() as *mut u8,
                        16 * 24,
                    );
                }
            }
        }

        // z values only grow; the kernel sorts by z when compositing, so no
        // normalization is needed (the old "z = 15" cap made freshly created
        // windows invisible and z collisions swallowed clicks).
        let bring_to_front = |states: &mut [WindowState; 16], win_id: usize| {
            let mut max_z = 0;
            for i in 0..16 {
                if states[i].active == 1 && states[i].z_order > max_z {
                    max_z = states[i].z_order;
                }
            }
            states[win_id].z_order = max_z + 1;
        };

        let mut states_changed = false;
        let mut had_mouse_event = false;

        // 2. Read mouse events
        while let Ok(bytes) = mouse.read(&mut event_buf) {
            if bytes == 8 {
                had_mouse_event = true;
                let type_ = u16::from_le_bytes([event_buf[0], event_buf[1]]);
                let code = u16::from_le_bytes([event_buf[2], event_buf[3]]);
                let value =
                    u32::from_le_bytes([event_buf[4], event_buf[5], event_buf[6], event_buf[7]]);

                if type_ == 3 {
                    // EV_ABS
                    if code == 0 {
                        cursor_x = (value * FB_WIDTH) / 32767;
                        if cursor_x >= FB_WIDTH {
                            cursor_x = FB_WIDTH - 1;
                        }
                    } else if code == 1 {
                        cursor_y = (value * FB_HEIGHT) / 32767;
                        if cursor_y >= FB_HEIGHT {
                            cursor_y = FB_HEIGHT - 1;
                        }
                    }
                } else if type_ == 1 && code == 0x110 {
                    mouse_btn_down = value == 1;

                    if mouse_btn_down {
                        let mut consumed = false;

                        // Check windows top-to-bottom: sort active ids by
                        // z descending (z values are unbounded now)
                        let mut hit_order: Vec<usize> =
                            (1..16).filter(|&i| states[i].active == 1).collect();
                        hit_order.sort_by(|&a, &b| states[b].z_order.cmp(&states[a].z_order));

                        for win_id in hit_order {
                            {
                                let win = &mut states[win_id];

                                let title_h = 24;
                                if cursor_x >= win.x && cursor_x < win.x + win.width {
                                    if cursor_y >= win.y && cursor_y < win.y + title_h {
                                        // Check close button
                                        if cursor_x >= win.x + win.width - 24 {
                                            win.active = 0;
                                            states_changed = true;
                                        } else {
                                            // Drag title bar
                                            dragged_win = Some(win_id);
                                            drag_offset_x = (cursor_x as i32) - (win.x as i32);
                                            drag_offset_y = (cursor_y as i32) - (win.y as i32);
                                            bring_to_front(&mut states, win_id);
                                            states_changed = true;
                                        }
                                        consumed = true;
                                        break;
                                    } else if cursor_y >= win.y + title_h
                                        && cursor_y < win.y + title_h + win.height
                                    {
                                        // Click inside window content
                                        bring_to_front(&mut states, win_id);
                                        states_changed = true;
                                        consumed = true;
                                        break;
                                    }
                                }
                            }
                        }

                        if !consumed {
                            // Check Desktop Icons
                            let calc_x = 10;
                            let calc_y = 10;
                            let term_x = 10;
                            let term_y = 80;
                            let expl_x = 10;
                            let expl_y = 150;
                            let view_x = 10;
                            let view_y = 220;
                            let edit_x = 10;
                            let edit_y = 290;
                            let icon_s = 50;

                            if cursor_x >= calc_x
                                && cursor_x <= calc_x + icon_s
                                && cursor_y >= calc_y
                                && cursor_y <= calc_y + icon_s
                            {
                                println!("shell: Clicked calc icon!");
                                let _ = File::open("/spawn:/apps/calc.wasm");
                            } else if cursor_x >= term_x
                                && cursor_x <= term_x + icon_s
                                && cursor_y >= term_y
                                && cursor_y <= term_y + icon_s
                            {
                                println!("shell: Clicked terminal icon!");
                                let _ = File::open("/spawn:/apps/terminal.wasm");
                            } else if cursor_x >= expl_x
                                && cursor_x <= expl_x + icon_s
                                && cursor_y >= expl_y
                                && cursor_y <= expl_y + icon_s
                            {
                                println!("shell: Clicked explorer icon!");
                                let _ = File::open("/spawn:/apps/explorer.wasm");
                            } else if cursor_x >= view_x
                                && cursor_x <= view_x + icon_s
                                && cursor_y >= view_y
                                && cursor_y <= view_y + icon_s
                            {
                                println!("shell: Clicked viewer icon!");
                                let _ = File::open("/spawn:/apps/viewer.wasm");
                            } else if cursor_x >= edit_x
                                && cursor_x <= edit_x + icon_s
                                && cursor_y >= edit_y
                                && cursor_y <= edit_y + icon_s
                            {
                                println!("shell: Clicked editor icon!");
                                let _ = File::open("/spawn:/apps/editor.wasm");
                            }

                            // Check Taskbar Start Button
                            if cursor_x <= 60 && cursor_y >= taskbar_y && cursor_y <= FB_HEIGHT {
                                println!("shell: Start Menu Clicked!");
                            }
                        }
                    } else {
                        dragged_win = None;
                    }
                }
            } else {
                break;
            }
        }

        if let Some(win_id) = dragged_win {
            if mouse_btn_down {
                states[win_id].x = (cursor_x as i32 - drag_offset_x).max(0) as u32;
                states[win_id].y = (cursor_y as i32 - drag_offset_y).max(0) as u32;
                states_changed = true;
            }
        }

        if states_changed {
            let mut write_buf = [0u8; 16 * 24];
            unsafe {
                std::ptr::copy_nonoverlapping(
                    states.as_ptr() as *const u8,
                    write_buf.as_mut_ptr(),
                    16 * 24,
                );
            }
            let _ = windows_meta.seek(SeekFrom::Start(0));
            let _ = windows_meta.write_all(&write_buf);
        }

        // Read the clock (cheap; a change forces one re-render per minute)
        let mut time_str = String::from("00:00");
        let _ = time_file.seek(SeekFrom::Start(0));
        let mut time_buf = [0u8; 8];
        if let Ok(8) = time_file.read(&mut time_buf) {
            let ns = u64::from_le_bytes(time_buf);
            let s = ns / 1_000_000_000;
            let h = (s / 3600) % 24;
            let m = (s % 3600) / 60;
            time_str = format!("{:02}:{:02} UTC", h, m);
        }

        // The kernel composites windows on top of our base layer, so the
        // shell only re-renders when its own content (clock) changes.
        // Without a hardware cursor we also redraw on mouse motion.
        let need_render = time_str != last_time_str || (!hw_cursor && had_mouse_event);

        if !need_render {
            sleep_ms(10);
            continue;
        }
        last_time_str = time_str.clone();
        let render_start = std::time::Instant::now();

        // 3. Draw Background (pre-rendered: wallpaper + icons + labels)
        local_fb.copy_from_slice(&bg_fb);

        // 4. Draw Taskbar
        draw_rect(&mut local_fb, 0, taskbar_y, FB_WIDTH, 40, 0xFF222222);

        // Start Button
        draw_rect(&mut local_fb, 0, taskbar_y, 60, 40, 0xFF444444);

        if let Some(ref mut font) = font_opt {
            draw_text(
                &mut local_fb,
                font,
                "Krake",
                10,
                taskbar_y + 25,
                16.0,
                0xFFFFFF,
            );
            draw_text(
                &mut local_fb,
                font,
                &time_str,
                FB_WIDTH - 80,
                taskbar_y + 25,
                16.0,
                0xFFFFFF,
            );
            let ms_str = format!("{} ms", frame_ms);
            draw_text(
                &mut local_fb,
                font,
                &ms_str,
                FB_WIDTH - 150,
                taskbar_y + 25,
                16.0,
                0xFF88FF88,
            );
        }

        // 6. Draw Cursor (software fallback only; normally the kernel moves
        // the virtio-gpu hardware cursor)
        if !hw_cursor {
            if let Some(ref cur) = cursor {
                draw_image(&mut local_fb, cur, cursor_x, cursor_y);
            } else {
                draw_rect(&mut local_fb, cursor_x, cursor_y, 8, 8, 0xFFFF0000);
            }
        }

        // 7. Blit to FB
        let local_fb_bytes: &[u8] = unsafe {
            std::slice::from_raw_parts(local_fb.as_ptr() as *const u8, local_fb.len() * 4)
        };
        let _ = fb_file.seek(SeekFrom::Start(0));
        let _ = fb_file.write_all(local_fb_bytes);

        frame_ms = render_start.elapsed().as_millis() as u32;

        sleep_ms(10);
    }
}

fn sleep_ms(ms: u64) {
    unsafe {
        let clock = __wasi_subscription_u_clock_t {
            identifier: 0,
            id: 0, // CLOCKID_REALTIME
            timeout: ms * 1_000_000,
            precision: 0,
            flags: 0,
        };
        let sub = __wasi_subscription_t {
            userdata: 0,
            u: __wasi_subscription_u_t { clock },
            tag: 0, // EVENTTYPE_CLOCK
        };
        let mut event: __wasi_event_t = std::mem::zeroed();
        let mut nevents: usize = 0;
        poll_oneoff(&sub, &mut event, 1, &mut nevents);
    }
}

#[repr(C)]
struct __wasi_subscription_u_clock_t {
    identifier: u64,
    id: u32,
    timeout: u64,
    precision: u64,
    flags: u16,
}
#[repr(C)]
struct __wasi_subscription_u_t {
    clock: __wasi_subscription_u_clock_t,
}
#[repr(C)]
struct __wasi_subscription_t {
    userdata: u64,
    u: __wasi_subscription_u_t,
    tag: u8,
}
#[repr(C)]
struct __wasi_event_t {
    userdata: u64,
    error: u16,
    type_: u8,
    fd_readwrite: [u64; 2],
}

#[link(wasm_import_module = "wasi_snapshot_preview1")]
extern "C" {
    fn poll_oneoff(
        in_: *const __wasi_subscription_t,
        out: *mut __wasi_event_t,
        nsubscriptions: usize,
        nevents: *mut usize,
    ) -> u16;
}
