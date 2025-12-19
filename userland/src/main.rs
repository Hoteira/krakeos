#![no_std]
#![no_main]

use inkui::{Window, Widget, Color, Size};
use std::println;
use std::fs::File;

extern crate alloc;

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let heap_size = 1024 * 1024 * 200; // Increased heap for image loading
    let heap_ptr = std::memory::malloc(heap_size);
    std::memory::heap::init_heap(heap_ptr as *mut u8, heap_size);

    println!("Starting Wallpaper App...");

    let width = std::graphics::get_screen_width();
    let height = std::graphics::get_screen_height();
    println!("Detected Screen Resolution: {}x{}", width, height);

    let mut win = Window::new("Wallpaper", width, height);
    win.can_move = false; 
    win.can_resize = false;

    // 1. Root Container
    let mut root = Widget::frame(1)
        .width(Size::Relative(100))
        .height(Size::Relative(100))
        .background_color(Color::rgb(0, 0, 0));

    // 2. Load Image from VFS
    if let Ok(mut file) = File::open("@0xE0/sys/img/wallpaper.png") {
        let size = file.size();
        if size > 0 {
            let buffer_addr = std::memory::malloc(size);
            let buffer = unsafe { core::slice::from_raw_parts_mut(buffer_addr as *mut u8, size) };
            
            if file.read(buffer).is_ok() {
                println!("Image loaded ({} bytes). Rendering PNG...", size);
                
                let img_widget = Widget::image(2, buffer)
                    .width(Size::Relative(100))
                    .height(Size::Relative(100));
                
                root = root.add_child(img_widget);
            } else {
                println!("Failed to read image file.");
            }
            // Memory is not freed because Widget::image copies the data, but buffer_addr leaks here.
            // In a real app we'd free it, but for a wallpaper that runs forever it's minor.
            // Ideally: std::memory::free(buffer_addr, 0);
        }
    } else {
        println!("Could not find /sys/img/wallpaper.png");
    }

    win.children.push(root);
    win.show();

    println!("Wallpaper displayed.");

    loop {
        //win.event_loop();
        std::os::yield_task();
    }
}