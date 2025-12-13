#![no_std]
#![no_main]

use inkui::{Window, Widget, Color, Size};
use std::println;

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    unsafe { core::arch::asm!("and rsp, -16"); } // Align stack to 16 bytes for SSE

    let heap_size = 1024 * 1024;
    let heap_ptr = std::graphics::malloc(heap_size);
    std::memory::heap::init_heap(heap_ptr as *mut u8, heap_size);

    let x = 0.2311111111111111;
    let y = 33423.34243243;

    println!("Starting Movable Window App... {}", x * y);

    /*let mut win = Window::new("Movable Window", 400, 300);
    win.can_move = true; 
    win.can_resize = false;

    let mut root = Widget::frame(1)
        .width(Size::Relative(100))
        .height(Size::Relative(100))
        .background_color(Color::rgb(220, 220, 220));

    let title_bar = Widget::frame(2)
        .width(Size::Relative(100))
        .height(Size::Absolute(25))
        .background_color(Color::rgb(50, 50, 150));

    let square = Widget::button(3, "")
        .x(Size::Absolute(100))
        .y(Size::Absolute(100))
        .width(Size::Absolute(50))
        .height(Size::Absolute(50))
        .background_color(Color::rgb(200, 50, 50))
        .set_border_radius(Size::Relative(50));

    root = root.add_child(title_bar).add_child(square);
    win.children.push(root);

    win.show();*/

    println!("Window created!");

    loop {
        std::os::yield_task();
    }
}