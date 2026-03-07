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

    // Spawn system apps
    println!("[Init] Spawning Taskbar...");
    std::thread::spawn(|| {
        std::wasm::run("@0xE0/apps/taskbar.wasm", "/", &[(0, 0), (1, 1), (2, 2)], true);
    });

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
