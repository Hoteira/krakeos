#![no_std]
#![no_main]

use inkui::{Window, Widget, Color, Size, Display, BackgroundStyle};
extern crate alloc;
use alloc::format;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn main(_argc: i32, _argv: *const *const u8) -> i32 {
    let width = 640;
    let height = 400;
    let mut win = Window::new("FPS Test", width, height);
    win.x = 100;
    win.y = 100;
    
    // Force opaque for fair test against optimized kernel path
    win.set_transparent(false);
    win.set_treat_as_transparent(false);

    win.show();

    std::println!("Starting FPS Test (1000 frames @ 640x400)...");

    let start_ticks = std::os::get_system_ticks();
    
    let mut root = Widget::frame(1)
        .width(Size::Relative(100))
        .height(Size::Relative(100));

    // We don't need to rebuild the widget tree every frame, just update color
    win.children.push(root);

    for i in 0..1000 {
        // Change color every frame to force visual update visibility
        let r = (i % 255) as u8;
        let g = ((i * 2) % 255) as u8;
        let b = ((i * 3) % 255) as u8;
        
        if let Some(root_widget) = win.find_widget_by_id_mut(1) {
             // We can't easily change style on the fly with current inkui API without helper,
             // so let's just clear children and re-add. 
             // Actually, win.children[0] is the root.
             // But accessing enum variants is verbose.
             // Let's just clear buffer manually to be raw and fast?
             // No, let's use the window update mechanism to test the full stack.
        }
        
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
