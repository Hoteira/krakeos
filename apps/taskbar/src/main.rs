#![no_std]
#![no_main]

use inkui::{Window, Widget, Color, Size, Display, Align};
use std::fs::File;
extern crate alloc;
use alloc::format;

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

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
pub extern "C" fn _start() -> ! {
    let heap_size = 1024 * 1024 * 10;
    let heap_ptr = std::memory::malloc(heap_size);
    std::memory::heap::init_heap(heap_ptr as *mut u8, heap_size);

    let screen_w = std::graphics::get_screen_width();
    let screen_total_h = std::graphics::get_screen_height();
    let screen_h = (screen_total_h * 3) / 100;

    let mut win = Window::new("Taskbar", screen_w, screen_h);
    win.w_type = std::graphics::Items::Bar;
    win.x = 0;
    win.y = 0;

    
    let font_data = if let Ok(mut file) = File::open("@0xE0/sys/fonts/CaskaydiaNerd.ttf") {
        let size = file.size();
        let buffer_addr = std::memory::malloc(size);
        let buffer = unsafe { core::slice::from_raw_parts_mut(buffer_addr as *mut u8, size) };
        if file.read(buffer).is_ok() {
            let static_buf = unsafe { core::slice::from_raw_parts(buffer_addr as *const u8, size) };
            win.load_font(static_buf);
            Some(static_buf)
        } else { None }
    } else { None };

    
    let mut font = font_data.and_then(|data| titanf::TrueTypeFont::load_font(data).ok());

    
    
    let get_center_y = |f: &mut Option<titanf::TrueTypeFont>, size: usize, char_code: char| -> usize {
        if let Some(font) = f {
            let (metrics, _) = font.get_char::<false>(char_code, size as f32);
            let h = metrics.height as f32;
            let s = size as f32;
            
            
            let offset = (h / 2.0) - (s / 3.0);
            if offset > 0.0 { offset as usize } else { 0 }
        } else {
            0
        }
    };

    
    let mut root = Widget::frame(1)
        .width(Size::Relative(100))
        .height(Size::Relative(100))
        .background_color(Color::rgba(20, 20, 20, 200)) 
        .padding(Size::Absolute(0)) 
        .set_display(Display::None);

    let base_size = 16;
    let logo_size = base_size + 2;
    
    
    let start_btn_w = 32;
    let start_btn_x = 12;
    let start_logo_y = get_center_y(&mut font, logo_size, '\u{E8F0}');
    let start_btn = Widget::button(10, "\u{E8F0}")
        .x(Size::Absolute(start_btn_x)) 
        .y(Size::Absolute(start_logo_y))
        .width(Size::Absolute(start_btn_w))
        .height(Size::Relative(100))
        .background_color(Color::rgba(0, 0, 0, 0)) 
        .set_text_color(Color::rgb(255, 255, 255)) 
        .set_text_size(logo_size) 
        .on_click(open_start_menu);
    root = root.add_child(start_btn);

    
    let user_logo_w = 24;
    let user_logo_x = start_btn_x + start_btn_w + 12;
    let user_logo_y = get_center_y(&mut font, logo_size, '\u{E8F0}');
    let user_logo = Widget::label(21, "\u{E8F0}")
        .x(Size::Absolute(user_logo_x))
        .y(Size::Absolute(user_logo_y))
        .width(Size::Absolute(user_logo_w))
        .height(Size::Relative(100))
        .set_text_size(logo_size)
        .set_text_color(Color::rgb(255, 255, 255))
        .background_color(Color::rgba(0, 0, 0, 0));
    root = root.add_child(user_logo);

    
    let guest_text = " Guest |";
    let guest_w = (guest_text.len() * base_size * 6) / 10; 
    let guest_x = user_logo_x + user_logo_w;
    let guest_y = get_center_y(&mut font, base_size, 'G');
    let guest_lbl = Widget::label(20, guest_text)
        .x(Size::Absolute(guest_x))
        .y(Size::Absolute(guest_y))
        .width(Size::Absolute(guest_w))
        .height(Size::Relative(100))
        .set_text_size(base_size)
        .set_text_color(Color::rgb(200, 200, 200))
        .background_color(Color::rgba(0, 0, 0, 0));
    root = root.add_child(guest_lbl);

    
    
    let power_w = 32;
    let power_margin_right = 12;
    let power_y = get_center_y(&mut font, base_size, '\u{F011}');
    let power_btn = Widget::button(12, "\u{F011}")
        .x(Size::FromRight(power_margin_right)) 
        .y(Size::Absolute(power_y))
        .width(Size::Absolute(power_w))
        .height(Size::Relative(100))
        .background_color(Color::rgba(0, 0, 0, 0))
        .set_text_color(Color::rgb(255, 100, 100)) 
        .set_text_size(base_size)
        .on_click(power_off);
    root = root.add_child(power_btn);

    
    let wifi_w = 32;
    let wifi_gap = 4;
    let wifi_margin_right = power_margin_right + power_w + wifi_gap;
    let wifi_y = get_center_y(&mut font, base_size, '\u{F1EB}');
    let wifi_btn = Widget::button(11, "\u{F1EB}")
        .x(Size::FromRight(wifi_margin_right))
        .y(Size::Absolute(wifi_y))
        .width(Size::Absolute(wifi_w))
        .height(Size::Relative(100))
        .background_color(Color::rgba(0, 0, 0, 0))
        .set_text_color(Color::rgb(255, 255, 255)) 
        .set_text_size(base_size)
        .on_click(wifi_status);
    root = root.add_child(wifi_btn);

    
    let clock_w = 80;
    let clock_x = (screen_w / 2).saturating_sub(clock_w / 2);
    let clock_y = get_center_y(&mut font, base_size, '0');
    
    let (h, m, _s) = std::os::get_time();
    let time_str = format!("{:02}:{:02}", h, m);
    let clock_lbl = Widget::label(2, &time_str)
        .x(Size::Absolute(clock_x))
        .y(Size::Absolute(clock_y))
        .width(Size::Absolute(clock_w))
        .height(Size::Relative(100))
        .set_text_size(base_size)
        .set_text_color(Color::rgb(255, 255, 255)) 
        .set_text_align(Align::Center)
        .background_color(Color::rgba(0, 0, 0, 0));
    root = root.add_child(clock_lbl);

    win.children.push(root);
    win.show(); 
    win.draw();
    win.update();

    let mut last_m = m;
    let mut ticks = 0;

    loop {
        ticks += 1;
        win.event_loop(); 

        if ticks % 50 == 0 {
            let (h, m, _s) = std::os::get_time();
            if m != last_m {
                if let Some(w) = win.find_widget_by_id_mut(2) {
                    if let Widget::Label { text, .. } = w {
                        text.text = format!("{:02}:{:02}", h, m);
                    }
                }
                last_m = m;
                win.draw();
                win.update();
            }
        }
        std::os::yield_task();
    }
}