#![no_std]
#![no_main]

use std::{print, println};
use std::graphics::{Window, Color, add_window, update_window, get_screen_width, get_screen_height, malloc};

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    println!("Initializing Graphics (Userland)...");

    let screen_width = get_screen_width();
    let screen_height = get_screen_height();
    
    println!("Screen Resolution: {}x{}", screen_width, screen_height);

    let width = 200;
    let height = 200;
    let buffer_size = width * height * 4; // 4 bytes per pixel (RGBA)

    // Allocate buffer for the window (using malloc syscall)
    // This returns a pointer to an identity-mapped frame accessible by user
    let buffer_ptr = malloc(buffer_size);
    
    if buffer_ptr == 0 {
        println!("Failed to allocate window buffer!");
        loop { std::os::yield_task(); }
    }
    
    // Create local buffer slice to draw into
    let buffer = unsafe { core::slice::from_raw_parts_mut(buffer_ptr as *mut u32, width * height) };

    // Fill buffer with blue color
    let blue = Color::rgb(0, 0, 255).to_u32();
    for pixel in buffer.iter_mut() {
        *pixel = blue;
    }

    // Draw a red square in the middle
    let red = Color::rgb(255, 0, 0).to_u32();
    let square_size = 50;
    let start_x = (width - square_size) / 2;
    let start_y = (height - square_size) / 2;

    for y in start_y..(start_y + square_size) {
        for x in start_x..(start_x + square_size) {
            buffer[y * width + x] = red;
        }
    }

    // Create and register the window
    // Note: We use the malloc'd pointer 'buffer_ptr'
    let mut window = Window::new(width, height, buffer_ptr);
    window.x = (screen_width - width) / 2;
    window.y = (screen_height - height) / 2;
    
    let window_id = add_window(&window);
    window.id = window_id;
    
    println!("Window created with ID: {}", window_id);

    // Initial draw (pushes buffer to window backbuffer in kernel)
    update_window(&window);

    println!("Entering event loop...");
    loop {
        // In a real app, we would poll events here. 
        // For now, just yield to let the OS compositor render.
        std::os::yield_task();
    }
}