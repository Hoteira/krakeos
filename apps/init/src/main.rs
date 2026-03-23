#![no_std]

extern crate alloc;

use alloc::vec;
use inkui::Window;
use std::debugln;
use std::io::Read;
use std::os::{sleep, Items};

pub fn main() {
    std::allocator::debug_allocator();
    debugln!("[Init] Starting WASM Userland...");

    let sw = std::os::graphics::get_screen_width();
    let sh = std::os::graphics::get_screen_height();
    debugln!("[Init] Detected Screen Size: {}x{}", sw, sh);

    // Setup wallpaper
    debugln!("[Init] Calling Window::new...");
    let mut win = Window::new("Wallpaper", sw, sh);
    debugln!("[Init] Window::new returned.");
    win.w_type = Items::Wallpaper;
    win.x = 0;
    win.y = 0;

    debugln!("[Init] Loading wallpaper...");
    match std::fs::read("@0xE0/sys/img/wallpaper2.png") {
        Ok(bytes) => {
            debugln!("[Init] Wallpaper loaded ({} bytes).", bytes.len());
            let img = inkui::Widget::image(1, &bytes)
                .width(inkui::Size::Relative(100))
                .height(inkui::Size::Relative(100));
            win.children.push(img);
            win.show();
        }
        Err(_) => {
            debugln!("[Init] Failed to load wallpaper.");
        }
    }

    std::os::user::set_current_user("racap");

    // Spawn system apps
    /*debugln!("[Init] Spawning Taskbar...");

    match std::os::spawn_with_fds("@0xE0/apps/taskbar.wasm", &[], &[(0, 0), (1, 1), (2, 2)]) {
        pid if pid != usize::MAX => debugln!("[Init] Taskbar spawned into its own slot with PID {}", pid),
        _ => debugln!("[Init] Failed to spawn taskbar"),
    }*/

    sleep(500);

    match std::os::spawn_with_fds("@0xE0/apps/aot_test.wasm", &[], &[(0, 0), (1, 1), (2, 2)]) {
        pid if pid != usize::MAX => debugln!("[Init] TAOT test into its own slot with PID {}", pid),
        _ => debugln!("[Init] Failed to spawn tests"),
    }

    sleep(500);
    debugln!("[Init] About to spawn dummy.wasm (WASIP1)...");

    match std::os::spawn_with_fds("@0xE0/apps/taskbar.wasm", &[], &[(0, 0), (1, 1), (2, 2)]) {
        pid if pid != usize::MAX => debugln!("[Init] Taskbar app spawned with PID {}", pid),
        _ => debugln!("[Init] Failed to spawn tsk app"),
    }

    /*
    match std::os::spawn_with_fds("@0xE0/apps/term.wasm", &[], &[(0, 0), (1, 1), (2, 2)]) {
        pid if pid != usize::MAX => debugln!("[Init] Term app spawned with PID {}", pid),
        _ => debugln!("[Init] Failed to spawn term app"),
    }*/

    debugln!("[Init] System ready.");

    loop {
        std::os::yield_task();
    }
}
