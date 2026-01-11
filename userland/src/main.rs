#![no_std]
#![no_main]

extern crate alloc;
use inkui::{Color, Size, Widget, Window};
use std::fs::File;
use std::graphics::Items;
use std::io::Read;
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
    use std::wasm::{validate, Linker, Store};
    use std::wasm::checked::StoredRunState;
    use alloc::vec;
    use alloc::vec::Vec;

    debugln!("WASM: Starting WASI Test App...");

    if let Ok(mut file) = File::open("@0xE0/wasm_test.wasm") {
        let size = file.size();
        let mut buffer = vec![0u8; size];
        if file.read(&mut buffer).is_ok() {
            match validate(&buffer) {
                Ok(validation_info) => {
                    debugln!("WASM: Module parsed and validated successfully.");

                    debugln!("--- WASM IMPORTS ---");
                    for import in &validation_info.imports {
                        debugln!("Import: {}.{}", import.module_name, import.name);
                    }
                    debugln!("--------------------");

                    let mut store = Store::new(());
                    let mut linker = Linker::new();

                    std::wasm::wasi::create_wasi_imports(&mut linker, &mut store);

                    // Instantiate Module
                    match linker.module_instantiate(&mut store, &validation_info, None) {
                        Ok(instance) => {
                            debugln!("WASM: Module instantiated.");

                            // Find _start
                            match store.instance_export(instance.module_addr, "_start") {
                                Ok(export) => {
                                    if let Some(func_addr) = export.as_func() {
                                        debugln!("WASM: Found _start. Invoking...");
                                        match store.invoke(func_addr, Vec::new(), None) {
                                            Ok(run_state) => {
                                                match run_state {
                                                    StoredRunState::Finished { values, .. } => {
                                                        debugln!("WASM: Execution Finished. Returns: {:?}", values);
                                                    }
                                                    StoredRunState::Resumable { .. } => {
                                                        debugln!("WASM: Execution Suspended.");
                                                    }
                                                }
                                            }
                                            Err(e) => debugln!("WASM: Execution error: {:?}", e),
                                        }
                                    } else {
                                        debugln!("WASM: _start is not a function.");
                                    }
                                }
                                Err(_) => debugln!("WASM: _start export not found."),
                            }
                        }
                        Err(e) => {
                            debugln!("WASM: Instantiation error: {:?}", e);
                            if !validation_info.imports.is_empty() {
                                debugln!("Note: This module requires imports. WASI/Host functions are not yet provided in this test.");
                            }
                        }
                    }
                }
                Err(e) => debugln!("WASM: Validation error: {:?}", e),
            }
        }
    } else {
        debugln!("WASM: wasm_test.wasm not found at @0xE0/wasm_test.wasm");
    }
}