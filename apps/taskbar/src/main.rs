#![no_std]
#![no_main]

extern crate alloc;
use alloc::format;
use std::io::Read;
use std::println;
use inkui::{Color, Display, Size, Widget, Window};
use std::fs::File;

fn open_start_menu(_win: &mut Window, _id: usize) {
    std::os::print("Start Menu Clicked\n");
}

fn power_off(_win: &mut Window, _id: usize) {
    std::os::print("Power Off Clicked\n");
}

fn wifi_status(_win: &mut Window, _id: usize) {
    std::os::print("Wifi Clicked\n");
}

#[unsafe(no_mangle)]
pub extern "C" fn main() -> i32 {
    let screen_w = std::graphics::get_screen_width();
    let screen_total_h = std::graphics::get_screen_height();
    let screen_h = (screen_total_h * 4) / 100;

    let mut win = Window::new("Taskbar", screen_w, screen_h);
    win.w_type = std::graphics::Items::Bar;
    win.x = 0;
    win.y = 0;

    {
        if let Ok(mut file) = File::open("@0xE0/sys/fonts/CaskaydiaNerd.ttf") {
            let size = file.size();
            let buffer_addr = std::memory::malloc(size);
            let buffer = unsafe { core::slice::from_raw_parts_mut(buffer_addr as *mut u8, size) };
            if file.read(buffer).is_ok() {
                let static_buf = unsafe { core::slice::from_raw_parts(buffer_addr as *const u8, size) };
                win.load_font(static_buf);
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

    let l = Widget::label(2, " \u{E8F0}  Guest | ")
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