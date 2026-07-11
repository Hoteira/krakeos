//! File explorer, rebuilt on inkui: a scrollable list of everything the
//! kernel reports via dev/system/fs. Instant startup (atlas font, no TTF
//! parse) and properly blended text.

use inkui::{Color, Size, Widget, Window};
use std::fs::File;
use std::io::Read;

fn main() {
    let Some(mut win) = Window::new("File Explorer", 420, 500) else { return };
    win.background = Color::rgb(255, 255, 255);

    let mut listing = String::new();
    if let Ok(mut fs_info) = File::options().read(true).open("/dev/system/fs") {
        let _ = fs_info.read_to_string(&mut listing);
    }
    if listing.is_empty() {
        listing.push_str("(no filesystem info)");
    }

    let title = Widget::label(1, "File Explorer")
        .background_color(Color::rgba(0, 0, 0, 0))
        .set_text_color(Color::rgb(20, 20, 30))
        .set_text_size(20.0)
        .x(Size::Absolute(0))
        .y(Size::Absolute(0))
        .width(Size::Auto)
        .height(Size::Absolute(36))
        .padding(Size::Absolute(8));

    let list = Widget::label(2, &listing)
        .background_color(Color::rgb(245, 245, 250))
        .set_text_color(Color::rgb(40, 40, 55))
        .set_text_size(14.0)
        .x(Size::Absolute(0))
        .y(Size::FromUp(40))
        .width(Size::Auto)
        .height(Size::Auto)
        .padding(Size::Absolute(8));

    win.add_child(title);
    win.add_child(list);
    win.show();

    loop {
        // j/k scroll the listing
        for event in win.poll_events() {
            if let inkui::Event::Keyboard(e) = event {
                if e.pressed {
                    let delta: i8 = match e.key {
                        106 => 1,  // j: down
                        107 => -1, // k: up
                        _ => 0,
                    };
                    if delta != 0 {
                        if let Some(w) = win.find_widget_by_id_mut(2) {
                            w.handle_scroll(delta);
                        }
                        win.mark_dirty();
                        win.update();
                    }
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(60));
    }
}
