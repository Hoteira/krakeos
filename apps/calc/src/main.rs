use std::fs::File;
use std::io::Write;

fn write_rect(window: &mut File, x: u32, y: u32, w: u32, h: u32, color: u32) {
    let mut data = Vec::with_capacity(16 + (w * h * 4) as usize);
    data.extend_from_slice(&x.to_le_bytes());
    data.extend_from_slice(&y.to_le_bytes());
    data.extend_from_slice(&w.to_le_bytes());
    data.extend_from_slice(&h.to_le_bytes());
    
    let color_bytes = color.to_le_bytes();
    for _ in 0..(w * h) {
        data.extend_from_slice(&color_bytes);
    }
    
    let _ = window.write_all(&data);
}

fn main() {
    let mut window = match File::options().read(true).write(true).open("/dev/gpu/window") {
        Ok(f) => f,
        Err(_) => return, // Failed to open window
    };
    
    let calc_x = 0;
    let calc_y = 0;
    let calc_w = 400;
    let calc_h = 500;
    
    // Draw Calc Background (Dark gray)
    write_rect(&mut window, calc_x, calc_y, calc_w, calc_h, 0xFF313244);
    
    // Draw Calc Display (Lighter gray)
    write_rect(&mut window, calc_x + 20, calc_y + 20, calc_w - 40, 80, 0xFF45475A);
    
    // Draw Some Buttons
    for row in 0..4 {
        for col in 0..3 {
            let bx = calc_x + 20 + col * 120;
            let by = calc_y + 120 + row * 90;
            write_rect(&mut window, bx, by, 100, 70, 0xFF585B70);
        }
    }
    
    // We just draw it and exit. Or we can loop and sleep so it stays alive.
    loop {
        unsafe {
            let clock = __wasi_subscription_u_clock_t {
                identifier: 0,
                id: 0, // CLOCKID_REALTIME
                timeout: 100_000_000,
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
}

// WASI bindings
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
    fn poll_oneoff(in_: *const __wasi_subscription_t, out: *mut __wasi_event_t, nsubscriptions: usize, nevents: *mut usize) -> u16;
}
