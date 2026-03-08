#![no_std]

extern crate alloc;

use alloc::vec;
use inkui::Window;
use std::os::{sleep, Items};
use std::io::Read;
use std::println;

pub fn main() {
    std::allocator::debug_allocator();
    println!("[Init] Starting WASM Userland...");

    let sw = std::os::graphics::get_screen_width();
    let sh = std::os::graphics::get_screen_height();
    println!("[Init] Detected Screen Size: {}x{}", sw, sh);

    // Setup wallpaper
    println!("[Init] Calling Window::new...");
    let mut win = Window::new("Wallpaper", sw, sh);
    println!("[Init] Window::new returned.");
    win.w_type = Items::Wallpaper;
    win.x = 0;
    win.y = 0;

    println!("[Init] Loading wallpaper...");
    match std::fs::read("@0xE0/sys/img/wallpaper2.raw") {
        Ok(bytes) => {
            println!("[Init] Wallpaper loaded ({} bytes).", bytes.len());
            let img = inkui::Widget::raw_image(1, &bytes, 1024, 576)
                .width(inkui::Size::Relative(100))
                .height(inkui::Size::Relative(100));
            win.children.push(img);
            win.show();
        }
        Err(_) => {
            println!("[Init] Failed to load wallpaper.");
        }
    }

    std::os::user::set_current_user("racap");

    // Spawn system apps
    println!("[Init] Spawning Taskbar...");
    
    match std::os::spawn_with_fds("@0xE0/apps/taskbar.wasm", &[], &[(0, 0), (1, 1), (2, 2)]) {
        pid if pid != usize::MAX => println!("[Init] Taskbar spawned into its own slot with PID {}", pid),
        _ => println!("[Init] Failed to spawn taskbar"),
    }

    /*// Per Step 29 decision: spawn as container
    let offset = 0x40000000; // 1GB offset
    let size = 0x4000000;   // 64MB size
    
    #[cfg(target_arch = "wasm32")]
    core::arch::wasm32::memory_grow(0, 32768); // Grow to 2GB to accommodate child at 1GB offset

    match std::os::container_plant_from_path("@0xE0/apps/taskbar.wasm", offset, size, Some(&[(0, 0), (1, 1), (2, 2)])) {
        Ok(id) => println!("[Init] Taskbar planted as container ID {}", id),
        Err(e) => println!("[Init] Failed to plant taskbar: {}", e),
    }

    std::thread::spawn(|| {
        std::wasm::run("@0xE0/apps/taskbar.wasm", "/", &[(0, 0), (1, 1), (2, 2)], true);
    });*/

    sleep(500);

    /*println!("[Init] Spawning AOT Test...");
    std::thread::spawn(|| {
        std::wasm::run("@0xE0/apps/aot_test.wasm", "/", &[(0, 0), (1, 1), (2, 2)], true);
    });

    sleep(500);

    println!("[Init] Spawning Terminal...");
    std::thread::spawn(|| {
        std::wasm::run("@0xE0/apps/term.wasm", "/", &[(0, 0), (1, 1), (2, 2)], true);
    });*/

    println!("[Init] System ready.");

    loop {
        std::os::yield_task();
    }
}
