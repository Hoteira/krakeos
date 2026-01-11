#![no_std]
#![no_main]

extern crate alloc;
use inkui::{Color, Size, Widget, Window};
use std::fs::File;
use std::io::Read;
use std::graphics::Items;
use std::{debugln, println};

#[unsafe(no_mangle)]
pub extern "C" fn main() -> i32 {
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
        .background_color(Color::rgb(255, 0, 0)); 


    if let Ok(mut file) = File::open("@0xE0/sys/img/wallpaper2.png") {
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

    test_wasm();

    loop {
        std::os::yield_task();
    }

    0
}

fn test_wasm() {
    use std::wasm::parser::Parser;
    use std::wasm::interpreter::Interpreter;
    use std::wasm::runtime::{Store, Stack, ExternalVal};
    use std::wasm::wasi::preview2::create_wasi_module;
    use alloc::vec;
    use alloc::rc::Rc;

    debugln!("WASM: Starting WASI Test App...");
    
    if let Ok(mut file) = File::open("@0xE0/wasm_test.wasm") {
        let size = file.size();
        let mut buffer = vec![0u8; size];
        if file.read(&mut buffer).is_ok() {
            let mut parser = Parser::new(&buffer);
            match parser.parse_module() {
                Ok(module) => {
                    debugln!("WASM: Module parsed successfully.");
                    
                    debugln!("--- WASM IMPORTS ---");
                    for import in &module.imports {
                        debugln!("Import: {}.{}", import.module, import.name);
                    }
                    debugln!("--------------------");
                    
                    let mut store = Store::new();
                    let mut stack = Stack::new();
                    
                    // Instantiate WASI
                    let wasi = create_wasi_module(&mut store);
                    
                    // Instantiate Module
                    let module_rc = Rc::new(module);
                    match store.instantiate(module_rc, &[wasi]) {
                        Ok(instance) => {
                            debugln!("WASM: Module instantiated.");
                            
                             // Find _start
                            let start_func_addr = instance.exports.iter().find(|e| e.name == "_start")
                                .and_then(|e| match e.value {
                                    ExternalVal::Func(addr) => Some(addr),
                                    _ => None
                                });

                            if let Some(addr) = start_func_addr {
                                let mut interpreter = Interpreter::new(&mut store, &mut stack);
                                match interpreter.invoke(addr, vec![]) {
                                    Ok(_) => debugln!("WASM: Execution finished successfully."),
                                    Err(e) => debugln!("WASM: Execution error: {}", e),
                                }
                            } else {
                                debugln!("WASM: _start function not found.");
                            }
                        },
                        Err(e) => debugln!("WASM: Instantiation error: {}", e),
                    }
                }
                Err(e) => debugln!("WASM: Parse error: {:?}", e),
            }
        }
    } else {
        debugln!("WASM: wasm_test.wasm not found at @0xE0/wasm_test.wasm");
    }
}