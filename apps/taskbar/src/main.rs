#![no_std]

extern crate alloc;

use alloc::boxed::Box;
use alloc::format;
use inkui::{Color, Display, Size, Widget, Window};
use std::os::{get_screen_height, get_screen_width, Items};
use std::println;

pub fn main() {
    let screen_w = get_screen_width();
    let screen_total_h = get_screen_height();
    let screen_h = (screen_total_h * 4) / 100;

    let mut win = Window::new("Taskbar", screen_w, screen_h);
    win.w_type = Items::Bar;
    win.x = 0;
    win.y = 0;

    println!("Loading font...");
    {
        match std::fs::read("@0xE0/sys/fonts/CaskaydiaNerd.ttf") {
            Ok(bytes) => {
                println!("Font loaded ({} bytes).", bytes.len());
                // Leak the memory to create a 'static slice for the window font loader
                let static_buf = Box::leak(bytes.into_boxed_slice());
                win.load_font(static_buf);
            }
            Err(_) => {
                println!("Failed to load font.");
            }
        }
    }

    let mut root = Widget::frame(1)
        .width(Size::Relative(100))
        .height(Size::Relative(100))
        .background_color(Color::rgba(20, 20, 20, 255))
        .set_display(Display::None);


    let unit = screen_h as f32 / 8.0;
    let font_size = unit * 4.0;

    let user_name = std::os::user::get_current_user();
    let l = Widget::label(2, &format!(" \u{E8F0}  {} | ", user_name.trim()))
        .y(Size::Absolute((unit) as usize))
        .set_text_color(Color::rgb(255, 255, 255))
        .background_color(Color::rgba(0, 0, 0, 0))
        .set_text_size(font_size);

    root = root.add_child(l);

    let clock = Widget::label(3, "00:00")
        .y(Size::Absolute((unit * 2.0) as usize))
        .x(Size::Relative(48))
        .set_text_color(Color::rgb(250, 250, 250))
        .background_color(Color::rgba(0, 0, 0, 0))
        .set_text_size(font_size);

    root = root.add_child(clock);
    win.children.push(root);
    win.show();
    println!("Taskbar initialized.");

    let mut last_minute = 99;

    loop {
        let (h, m, _) = std::os::get_time();

        if m != last_minute {
            last_minute = m;
            let time_str = format!("{:02}:{:02}", h, m);

            if let Some(widget) = win.find_widget_by_id_mut(3) {
                if let Widget::Label { text, .. } = widget {
                    text.text = time_str;
                }
            }

            win.draw();
            win.update();
        }

        std::os::sleep(1000);
    }
}
