use std::fs::File;
use std::io::{Read, Write, Seek, SeekFrom};

const FB_WIDTH: u32 = 640;
const FB_HEIGHT: u32 = 480;

fn draw_image(fb: &mut [u32], pixels: &[u32], img_w: u32, img_h: u32, x: u32, y: u32) {
    for py in 0..img_h {
        for px in 0..img_w {
            let sx = x + px;
            let sy = y + py;
            if sx < FB_WIDTH && sy < FB_HEIGHT {
                let color = pixels[(py * img_w + px) as usize];
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
                    fb[(sy * FB_WIDTH + sx) as usize] = 0xFF000000 | (out_r << 16) | (out_g << 8) | out_b;
                }
            }
        }
    }
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

fn main() {
    let mut win_file = match File::options().write(true).open("/dev/gpu/window") {
        Ok(f) => f,
        Err(_) => return,
    };
    
    let mut fb = vec![0xFF222222u32; (FB_WIDTH * FB_HEIGHT) as usize];
    
    if let Some((w, h, pixels)) = load_png("/img/wallpaper.png") {
        draw_image(&mut fb, &pixels, w, h, 0, 0);
    } else {
        println!("viewer: failed to load png");
    }
    
    let fb_bytes: &[u8] = unsafe {
        std::slice::from_raw_parts(fb.as_ptr() as *const u8, fb.len() * 4)
    };
    let _ = win_file.seek(SeekFrom::Start(0));
    let _ = win_file.write_all(fb_bytes);
    
    // Just sleep forever
    loop {
        unsafe {
            let clock = __wasi_subscription_u_clock_t {
                identifier: 0,
                id: 0,
                timeout: 100_000_000,
                precision: 0,
                flags: 0,
            };
            let sub = __wasi_subscription_t {
                userdata: 0,
                u: __wasi_subscription_u_t { clock },
                tag: 0,
            };
            let mut event: __wasi_event_t = std::mem::zeroed();
            let mut nevents: usize = 0;
            poll_oneoff(&sub, &mut event, 1, &mut nevents);
        }
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
unsafe extern "C" {
    fn poll_oneoff(
        in_: *const __wasi_subscription_t,
        out: *mut __wasi_event_t,
        nsubscriptions: usize,
        nevents: *mut usize,
    ) -> u16;
}
