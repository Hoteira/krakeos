#![no_std]
#![no_main]

use inkui::{Window, Widget, Color, Size};
use std::println;
use std::fs::File;
use std::graphics::Items; 

extern crate alloc;

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let heap_size = 1024 * 1024 * 50;
    let heap_ptr = std::memory::malloc(heap_size);
    std::memory::heap::init_heap(heap_ptr as *mut u8, heap_size);

    println!("Starting Userland Shell...");

    let width = std::graphics::get_screen_width();
    let height = std::graphics::get_screen_height();
    println!("Detected Screen Resolution: {}x{}", width, height);

    
    let mut win_wallpaper = Window::new("Wallpaper", width, height);
    win_wallpaper.w_type = Items::Wallpaper; 
    win_wallpaper.can_move = false; 
    win_wallpaper.can_resize = false;

    let mut root_wallpaper = Widget::frame(1)
        .width(Size::Relative(100))
        .height(Size::Relative(100))
        .background_color(Color::rgb(0, 0, 0));

    
    if let Ok(mut file) = File::open("@0xE0/sys/img/wallpaper.png") {
        let size = file.size();
        if size > 0 {
            let buffer_addr = std::memory::malloc(size);
            let buffer = unsafe { core::slice::from_raw_parts_mut(buffer_addr as *mut u8, size) };
            
            if file.read(buffer).is_ok() {
                println!("Wallpaper loaded.");
                let img_widget = Widget::image(2, buffer)
                    .width(Size::Relative(100))
                    .height(Size::Relative(100));
                root_wallpaper = root_wallpaper.add_child(img_widget);
            }
            
        }
    }

    win_wallpaper.children.push(root_wallpaper);
    win_wallpaper.show();

    println!("Desktop Environment Initialized.");
    
    
    std::os::exec("@0xE0/sys/bin/taskbar.elf");
    
    
    std::os::exec("@0xE0/sys/bin/term.elf");

    loop {
        std::os::yield_task();
    }
}