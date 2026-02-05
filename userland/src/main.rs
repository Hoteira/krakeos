#![no_std]

/*
   1. Integer Arithmetic Edge Cases (Done)
   2. Floating Point Precision & Rounding (Done)
   3. Complex Control Flow
   4. Memory & Bulk Operations
   5. Tables & Globals
   6. Multi-Value & ABI Stress

*/

extern crate alloc;
use alloc::string::{String, ToString};
use inkui::{Color, Size, Widget, Window};
use std::fs::File;
use std::os::Items;
use std::io::Read;
use std::{debugln, println};
use std::math::FloatMath;

pub fn main() {
    println!("Starting Userland Shell...");

    let width = std::os::graphics::get_screen_width();
    let height = std::os::graphics::get_screen_height();
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

    println!("Starting WASM Apps...");

    run_wasm("@0xE0/apps/aot_test.wasm", false);


    /*std::thread::spawn(|| {
        run_wasm("@0xE0/apps/taskbar.wasm", true);
    });*/

    std::os::spawn("@0xE0/sys/bin/term.elf");

    loop {
        std::os::yield_task();
    }
}

fn run_wasm(path: &str, enable_aot: bool) {
    use alloc::vec;
    use alloc::vec::Vec;
    use std::wasm::{validate, Linker, Store};

    debugln!("WASM: Starting WASI App: {}...", path);

    if let Ok(mut file) = File::open(path) {
        let size = file.size();
        let mut buffer = vec![0u8; size];
        if file.read(&mut buffer).is_ok() {
            match validate(&buffer) {
                Ok(validation_info) => {
                    debugln!("WASM: Module {} parsed and validated successfully.", path);

                    let mut store = Store::new(());
                    store.aot_enabled = enable_aot;
                    let mut linker = Linker::new();

                    std::wasm::wasi::create_wasi_imports(&mut linker, &mut store);
                    std::wasm::wasi::create_wasi_p2_imports(&mut linker, &mut store);

                    store.wasi_ctx = Some(std::wasm::wasi::WasiCtx::new(alloc::vec![path.to_string()], String::from("@0xE0"), &[(0, 0), (1, 1), (2, 2)]));

                    if let Some(component) = &validation_info.component {
                        debugln!("WASI: [COMPONENT] Starting {}...", path);
                        match std::wasm::interpreter::component_executor::instantiate_component(
                            &mut store, &linker, component, &buffer,
                        ) {
                            Ok(_) => {
                                debugln!("WASI: [COMPONENT] Finished {}.", path);
                            }
                            Err(e) => debugln!("WASM: Component Execution error: {:?}", e),
                        }
                    } else {
                        // 1. Run using Interpreter or AOT
                        if enable_aot {
                            debugln!("WASI: [AOT] Starting {}...", path);
                        } else {
                            debugln!("WASI: [INTERPRETER] Starting {}...", path);
                        }
                        match linker.module_instantiate_unchecked(&mut store, &validation_info, None) {
                            Ok(instance) => {

                                // Try "run" first (WASI Preview 2 convention), then "_start" as fallback
                                let entry_point = store
                                    .instance_export_unchecked(instance.module_addr, "run")
                                    .ok()
                                    .and_then(|e| e.as_func())
                                    .or_else(|| {
                                        store
                                            .instance_export_unchecked(instance.module_addr, "_start")
                                            .ok()
                                            .and_then(|e| e.as_func())
                                    });

                                if let Some(func_addr) = entry_point {
                                    let _ = store.invoke_unchecked(func_addr, Vec::new(), None);
                                    if enable_aot {
                                        debugln!("WASI: [AOT] Finished {}.", path);
                                    } else {
                                        debugln!("WASI: [INTERPRETER] Finished {}.", path);
                                    }
                                }
                            }
                            Err(e) => debugln!("WASM: Instantiation error: {:?}", e),
                        }
                    }
                }
                Err(e) => debugln!("WASM: Validation error: {:?}", e),
            }
        }
    } else {
        debugln!("WASM: WASM file not found at {}", path);
    }
}
