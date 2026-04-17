#![no_std]

extern crate alloc;
use alloc::string::{String, ToString};
use inkui::{Color, Size, Widget, Window};
use std::fs::File;
use std::os::{sleep, Items};
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

    if let Ok(mut file) = File::open("/sys/img/wallpaper2.png") {
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

    if let Ok(mut file) = File::open("/sys/img/wallpaper2.png") {}

    std::thread::spawn(|| {
        std::wasm::run("/apps/container_test.wasm", "/", &[(0, 0), (1, 1), (2, 2)], true);
    });

    sleep(1000);

    /*std::thread::spawn(|| {
        std::wasm::run("/apps/aot_test.wasm", "/", &[(0, 0), (1, 1), (2, 2)], true);
    });

    sleep(500);

    std::thread::spawn(|| {
        std::wasm::run("/apps/taskbar.wasm", "/", &[(0, 0), (1, 1), (2, 2)], true);
    });

    sleep(500);*/

    std::thread::spawn(|| {
        std::wasm::run("/apps/term.wasm", "/", &[(0, 0), (1, 1), (2, 2)], true);
    });


    loop {
        std::os::yield_task();
    }
}

