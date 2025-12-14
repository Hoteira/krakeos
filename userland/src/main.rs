#![no_std]
#![no_main]

use inkui::{Window, Widget, Color, Size};
use std::println;
use std::fs::File;
use alloc::vec::Vec;
use alloc::boxed::Box;
use alloc::format;

extern crate alloc;

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let heap_size = 1024 * 1024 * 10; // Increased heap for font
    let heap_ptr = std::memory::malloc(heap_size);
    std::memory::heap::init_heap(heap_ptr as *mut u8, heap_size);

    println!("Starting Rounded Window App...");

    let mut win = Window::new("Rounded Square", 400, 400);
    win.can_move = true; 
    win.can_resize = false;

    /*
    // Load font
    if let Ok(mut file) = File::open("@0xE0/sys/fonts/CaskaydiaNerd.ttf") {
        
        let file_size = file.size(); 
        
        if file_size > 0 {
            let font_ptr = std::memory::malloc(file_size) as *mut u8;
            
            if !font_ptr.is_null() {
                let font_data_slice = unsafe {
                    core::slice::from_raw_parts_mut(font_ptr, file_size)
                };

                file.read(&mut font_data_slice[..]).unwrap();

                 let actual_font_data_slice = unsafe {
                    core::slice::from_raw_parts(font_ptr, file.size())
                };

                win.load_font(actual_font_data_slice);
                println!("Font loaded! Size: {}", file.size());

            } else {
                println!("Failed to allocate memory for font");
            }
        } else {
            println!("Font file is empty");
        }
    } else {
        println!("Failed to open font file");
    }

     */

    let mut root = Widget::frame(1)
        .width(Size::Relative(100))
        .height(Size::Relative(100))
        .set_border_radius(Size::Absolute(5))
        .background_color(Color::rgb(100, 220, 100));

    // Add a label to test text
    let label = Widget::label(2, "Hello World")
        .width(Size::Relative(80))
        .height(Size::Absolute(40))
        .y(Size::Absolute(50))
        .x(Size::Absolute(0)) // Assuming Center isn't implemented, sticking to default
        .background_color(Color::rgba(255, 255, 255, 0)) // Transparent
        .set_text_size(24)
        .set_text_color(Color::rgb(0, 0, 0));

    // Add button
    let str = format!("Hello #[35pt] #[0xFFFF0000]World {}", char::from_u32(0xf015).unwrap());
    let button = Widget::button(3, str.as_str())
        .width(Size::Absolute(120))
        .height(Size::Absolute(40))
        .y(Size::Absolute(120))
        .x(Size::Absolute(140))
        .set_text_size(24)
        .background_color(Color::rgb(200, 200, 255));

    root = root.add_child(button);
    win.children.push(root);

    if win.font.is_some() {
        println!("Font loaded!");
    } else {
        println!("Font failed to load!");
    }

    win.show();

    println!("Window created!");

    loop {
        std::os::yield_task();
    }
}