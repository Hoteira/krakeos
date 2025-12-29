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
    let screen_h = 45; 

    let mut win = Window::new("Taskbar", screen_w, screen_h);
    win.w_type = std::graphics::Items::Bar;
    win.x = 0;
    win.y = 0;

    
    if let Ok(mut file) = File::open("@0xE0/sys/fonts/CaskaydiaNerd.ttf") {
        let size = file.size();
        let buffer_addr = std::memory::malloc(size);
        let buffer = unsafe { core::slice::from_raw_parts_mut(buffer_addr as *mut u8, size) };
        if file.read(buffer).is_ok() {
            let static_buf = unsafe { core::slice::from_raw_parts(buffer_addr as *const u8, size) };
            win.load_font(static_buf);
        }
    }

    
    
    let mut root = Widget::frame(1)
        .width(Size::Relative(100))
        .height(Size::Relative(100))
        .background_color(Color::rgba(20, 20, 20, 200)) 
        .padding(Size::Absolute(0)) 
        .set_display(Display::None);

    let btn_y = 8;
    let btn_h = 33;
    
    
    let logo_btn = Widget::button(10, "\u{E8F0}")
        .x(Size::Absolute(12)) 
        .y(Size::Absolute(btn_y))
        .width(Size::Absolute(48))
        .height(Size::Absolute(btn_h))
        .background_color(Color::rgba(0, 0, 0, 0)) 
        .set_text_color(Color::rgb(255, 255, 255)) 
        .set_text_size(32) 
        .set_border_radius(Size::Absolute(8))
        .on_click(open_start_menu);
    root = root.add_child(logo_btn);

    
    
    let clock_w = 100;
    let clock_x = (screen_w / 2).saturating_sub(clock_w / 2);
    
    let (h, m, _s) = std::os::get_time();
    let time_str = format!("{:02}:{:02}", h, m);
    let clock_lbl = Widget::label(2, &time_str)
        .x(Size::Absolute(clock_x))
        .y(Size::Absolute(btn_y + 4))
        .width(Size::Absolute(clock_w))
        .height(Size::Absolute(btn_h))
        .set_text_size(18)
        .set_text_color(Color::rgb(255, 255, 255)) 
        .set_text_align(Align::Center)
        .background_color(Color::rgba(0, 0, 0, 0));
    root = root.add_child(clock_lbl);

    
    
    let power_w = 40;
    let power_margin_right = 12;
    let power_btn = Widget::button(12, "\u{F011}")
        .x(Size::FromRight(power_margin_right)) 
        .y(Size::Absolute(btn_y))
        .width(Size::Absolute(power_w))
        .height(Size::Absolute(btn_h))
        .background_color(Color::rgba(0, 0, 0, 0))
        .set_text_color(Color::rgb(255, 100, 100)) 
        .set_text_size(22)
        .set_border_radius(Size::Absolute(8))
        .on_click(power_off);
    root = root.add_child(power_btn);

    
    let wifi_w = 40;
    let wifi_gap = 4;
    let wifi_margin_right = power_margin_right + power_w + wifi_gap;
    let wifi_btn = Widget::button(11, "\u{F1EB}")
        .x(Size::FromRight(wifi_margin_right))
        .y(Size::Absolute(btn_y))
        .width(Size::Absolute(wifi_w))
        .height(Size::Absolute(btn_h))
        .background_color(Color::rgba(0, 0, 0, 0))
        .set_text_color(Color::rgb(255, 255, 255)) 
        .set_text_size(22)
        .set_border_radius(Size::Absolute(8))
        .on_click(wifi_status);
    root = root.add_child(wifi_btn);

    win.children.push(root);
    win.show(); 

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
                win.update();
                win.draw();
                win.show();
            }
        }
        
        std::os::yield_task();
    }
}