use core::ffi::{c_int, c_void};
use crate::stdlib::malloc;

#[unsafe(no_mangle)] pub unsafe extern "C" fn krake_syscall(n: u64, a1: u64, a2: u64, a3: u64, a4: u64) -> u64 { std::os::syscall4(n, a1, a2, a3, a4) }

#[unsafe(no_mangle)] pub unsafe extern "C" fn krake_sleep(ms: usize) {
    std::os::sleep(ms as u64);
}

#[unsafe(no_mangle)] pub unsafe extern "C" fn krake_get_time_ms() -> usize { std::os::syscall(55, 0, 0, 0) as usize }

#[unsafe(no_mangle)]
pub unsafe extern "C" fn krake_get_event(wid: usize, out_event: *mut u32) -> c_int {
    let mut buf = [0u64; 8];
    
    if krake_syscall(52, wid as u64, buf.as_mut_ptr() as u64, 1, 0) == 1 {
        let tag = buf[0] as u32; 
        *out_event.add(0) = tag; 
        
        let data_ptr = buf.as_ptr().add(1) as *const u32; 
        
        match tag {
            0 => { 
                let x = *(buf.as_ptr().add(2)) as u32;
                let y = *(buf.as_ptr().add(3)) as u32;
                let buttons_scroll = *(buf.as_ptr().add(4)) as u32;
                
                *out_event.add(1) = x; 
                *out_event.add(2) = y; 
                *out_event.add(3) = buttons_scroll & 0xFFFFFF; 
                *out_event.add(4) = (buttons_scroll >> 24) & 0xFF; 
            },
            1 => { 
                let wid_key = *(buf.as_ptr().add(1));
                let key = (wid_key >> 32) as u32;
                let flags = *(buf.as_ptr().add(2));
                let pressed = (flags & 0xFF) as u32;
                let repeat = ((flags >> 16) & 0xFFFF) as u32;
                
                if pressed == 1 && key == 32 { std::println!("LIBC: Space Key (32) Received"); }
                
                *out_event.add(1) = key;
                *out_event.add(2) = repeat;  
                *out_event.add(3) = pressed; 
            },
            2 => { 
                let w = *(buf.as_ptr().add(2)) as u32;
                let h = *(buf.as_ptr().add(3)) as u32;
                *out_event.add(3) = w; 
                *out_event.add(4) = h; 
            },
            _ => {}
        }
        return 1;
    }
    0
}

#[repr(C)]
struct RawWindow {
    id: usize, buffer: usize, pid: u64, x: isize, y: isize, z: usize, width: usize, height: usize,
    can_move: bool, can_resize: bool, transparent: bool, treat_as_transparent: bool, min_width: usize, min_height: usize,
    event_handler: usize, w_type: u32,
}

static mut DOOM_WINDOW_BUFFER: usize = 0;
static mut DOOM_WINDOW_WIDTH: usize = 0;
static mut DOOM_WINDOW_HEIGHT: usize = 0;
static mut DOOM_WINDOW_TRANSPARENT: bool = true;
static mut DOOM_WINDOW_TREAT_AS_TRANSPARENT: bool = true;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn krake_window_create(width: usize, height: usize, _transparent: bool, _treat_as_transparent: bool) -> usize {
    let buffer_size = width * height * 4;
    let buffer = malloc(buffer_size) as usize;
    DOOM_WINDOW_BUFFER = buffer;
    DOOM_WINDOW_WIDTH = width;
    DOOM_WINDOW_HEIGHT = height;
    
    DOOM_WINDOW_TRANSPARENT = false;
    DOOM_WINDOW_TREAT_AS_TRANSPARENT = false;
    
    let w = RawWindow {
        id: 0, buffer, pid: 0, x: 100, y: 100, z: 0, width, height,
        can_move: true, can_resize: false, 
        transparent: false, 
        treat_as_transparent: false, 
        min_width: 0, min_height: 0,
        event_handler: 1, w_type: 3,
    };
    krake_syscall(22, &w as *const _ as u64, 0, 0, 0) as usize
}

#[unsafe(no_mangle)] pub unsafe extern "C" fn krake_window_get_buffer(_wid: usize) -> *mut c_void { DOOM_WINDOW_BUFFER as *mut c_void }

#[unsafe(no_mangle)]
pub unsafe extern "C" fn krake_window_draw(wid: usize) {
    let w = RawWindow {
        id: wid, buffer: DOOM_WINDOW_BUFFER, pid: 0, x: 0, y: 0, z: 0, 
        width: DOOM_WINDOW_WIDTH, height: DOOM_WINDOW_HEIGHT,
        can_move: false, can_resize: false, transparent: DOOM_WINDOW_TRANSPARENT, treat_as_transparent: DOOM_WINDOW_TREAT_AS_TRANSPARENT, min_width: 0, min_height: 0,
        event_handler: 0, w_type: 3,
    };
    krake_syscall(51, &w as *const _ as u64, 0, 0, 0);
}

// --- SIGNAL STUBS ---
#[unsafe(no_mangle)] pub unsafe extern "C" fn kill(pid: c_int, sig: c_int) -> c_int {
    // Syscall 78 is kill(pid) in kernel, but here we have sig.
    // If sig == 0, it's a check. If sig == 9 (SIGKILL), we call syscall 78.
    // For now, only support SIGKILL or ignore.
    if sig == 9 {
        krake_syscall(78, pid as u64, 0, 0, 0) as c_int
    } else {
        0
    }
}

#[unsafe(no_mangle)] pub unsafe extern "C" fn raise(sig: c_int) -> c_int {
    kill(crate::unistd::getpid(), sig)
}

#[unsafe(no_mangle)] pub unsafe extern "C" fn sigemptyset(set: *mut u32) -> c_int { *set = 0; 0 }
#[unsafe(no_mangle)] pub unsafe extern "C" fn sigfillset(set: *mut u32) -> c_int { *set = 0xFFFFFFFF; 0 }
#[unsafe(no_mangle)] pub unsafe extern "C" fn sigaddset(set: *mut u32, signum: c_int) -> c_int { *set |= 1 << (signum - 1); 0 }
#[unsafe(no_mangle)] pub unsafe extern "C" fn sigdelset(set: *mut u32, signum: c_int) -> c_int { *set &= !(1 << (signum - 1)); 0 }
#[unsafe(no_mangle)] pub unsafe extern "C" fn sigismember(set: *const u32, signum: c_int) -> c_int { if (*set & (1 << (signum - 1))) != 0 { 1 } else { 0 } }
#[unsafe(no_mangle)] pub unsafe extern "C" fn sigprocmask(_how: c_int, _set: *const u32, _oldset: *mut u32) -> c_int { 0 }
