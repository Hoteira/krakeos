#![no_std]

extern crate alloc;
use alloc::string::{String, ToString};
use inkui::{Color, Size, Widget, Window};
use std::fs::File;
use std::os::Items;
use std::io::Read;
use std::{debugln, println};
use std::math::FloatMath;

pub fn main() {
    println!("Starting Userland Shell...");

    let width = std::os::graphics::get_screen_width();
    let height = std::os::graphics::get_screen_height();
    println!("Detected Screen Resolution: {}x{}", width, height);

    let mut win_wallpaper = Window::new("Wallpaper", width, height);
    win_wallpaper.w_type = Items::Wallpaper;
    win_wallpaper.can_move = false;
    win_wallpaper.can_resize = false;

    let mut root_wallpaper = Widget::frame(1)
        .width(Size::Relative(100))
        .height(Size::Relative(100))
        .background_color(Color::rgb(255, 0, 0));

    if let Ok(mut file) = File::open("@0xE0/sys/img/wallpaper2.png") {
        let size = file.size();
        if size > 0 {
            let buffer_addr = std::memory::malloc(size);
            let buffer = unsafe { core::slice::from_raw_parts_mut(buffer_addr as *mut u8, size) };

            if file.read(buffer).is_ok() {
                println!("Wallpaper loaded.");

                let img_widget = Widget::image(2, buffer)
                    .width(Size::Relative(100))
                    .height(Size::Relative(100));
                root_wallpaper = root_wallpaper.add_child(img_widget);
            }
        }
    }

    win_wallpaper.children.push(root_wallpaper);
    win_wallpaper.show();

    println!("Desktop Environment Initialized.");

    println!("Starting userland WASM Apps...");

    std::thread::spawn(|| {
        std::wasm::run("@0xE0/apps/container_test.wasm", "/", &[(0, 0), (1, 1), (2, 2)], true);
    });

    /*std::thread::spawn(|| {
        std::wasm::run("@0xE0/apps/aot_test.wasm", "/", &[(0, 0), (1, 1), (2, 2)], true);
    });

    std::thread::spawn(|| {
        std::wasm::run("@0xE0/apps/net_test.wasm", "/", &[(0, 0), (1, 1), (2, 2)], true);
    });*/

    std::thread::spawn(|| {
        std::wasm::run("@0xE0/apps/taskbar.wasm", "/", &[(0, 0), (1, 1), (2, 2)], true);
    });

    //std::os::spawn("@0xE0/sys/bin/term.elf");

    loop {
        std::os::yield_task();
    }
}

