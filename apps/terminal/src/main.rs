//! Terminal, rebuilt on inkui. Text rendering comes from the pre-rasterized
//! atlas (instant startup -- no TTF parsing) and is properly alpha-blended.

use inkui::{Color, Event, Size, Widget, Window};

const OUTPUT_ID: usize = 1;

fn main() {
    let Some(mut win) = Window::new("Terminal", 400, 500) else { return };
    win.background = Color::rgb(30, 30, 46);

    let output = Widget::label(OUTPUT_ID, "> ")
        .background_color(Color::rgba(0, 0, 0, 0)) // transparent: window bg shows
        .set_text_color(Color::rgb(205, 214, 244))
        .set_text_size(16.0)
        .x(Size::Absolute(0))
        .y(Size::Absolute(0))
        .width(Size::Auto)
        .height(Size::Auto)
        .padding(Size::Absolute(10));
    win.add_child(output);
    win.show();

    loop {
        let mut changed = false;
        for event in win.poll_events() {
            if let Event::Keyboard(e) = event {
                if !e.pressed || e.key == 0 {
                    continue;
                }
                let c = char::from_u32(e.key).unwrap_or('\0');
                if let Some(label) = win.find_widget_by_id_mut(OUTPUT_ID) {
                    match c {
                        '\x08' => {
                            let t = label.get_text();
                            if t.len() > 2 {
                                label.set_text(&t[..t.len() - 1]);
                            }
                        }
                        '\n' => {
                            label.append_text("\n> ");
                        }
                        '\0' => {}
                        _ => {
                            let mut s = [0u8; 4];
                            label.append_text(c.encode_utf8(&mut s));
                        }
                    }
                    changed = true;
                }
            }
        }

        if changed {
            // Autoscroll: keep the newest line visible
            let (content_h, view_h) = win
                .find_widget_by_id(OUTPUT_ID)
                .map(|w| (w.geometry().content_height, w.geometry().height))
                .unwrap_or((0, 0));
            if content_h > view_h && view_h > 0 {
                if let Some(w) = win.find_widget_by_id_mut(OUTPUT_ID) {
                    w.geometry_mut().scroll_offset_y = content_h - view_h + 24;
                }
            }
            win.mark_dirty();
            win.update();
        }

        std::thread::sleep(std::time::Duration::from_millis(30));
    }
}
