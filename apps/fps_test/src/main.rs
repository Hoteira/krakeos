#![no_std]
#![no_main]

extern crate alloc;
use inkui::{Color, Size, Widget, Window};

#[unsafe(no_mangle)]
pub extern "C" fn main() -> i32 {
    let width = 640;
    let height = 400;
    let mut win = Window::new("FPS Test", width, height);
    win.x = 100;
    win.y = 100;


    win.set_transparent(false);
    win.set_treat_as_transparent(false);

    win.show();

    std::println!("Starting FPS Test (1000 frames @ 640x400)...");

    let start_ticks = std::os::get_system_ticks();

    let mut root = Widget::frame(1)
        .width(Size::Relative(100))
        .height(Size::Relative(100));


    win.children.push(root);

    for i in 0..1000 {
        let r = (i % 255) as u8;
        let g = ((i * 2) % 255) as u8;
        let b = ((i * 3) % 255) as u8;

        if let Some(root_widget) = win.find_widget_by_id_mut(1) {}

        win.children.clear();
        let bg = Widget::frame(1)
            .width(Size::Relative(100))
            .height(Size::Relative(100))
            .background_color(Color::rgb(r, g, b));
        win.children.push(bg);

        win.draw();
        win.update();
    }

    let end_ticks = std::os::get_system_ticks();
    let duration_ms = end_ticks - start_ticks;

    let fps = if duration_ms > 0 {
        (1000.0 / duration_ms as f64) * 1000.0
    } else {
        9999.0
    };

    std::println!("Test Complete.");
    std::println!("Time: {} ms", duration_ms);
    std::println!("Average FPS: {:.2}", fps);

    loop {
        std::os::sleep(1000);
    }
}
