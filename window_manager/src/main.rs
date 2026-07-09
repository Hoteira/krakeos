#[repr(C)]
struct WasiCiovec {
    buf: *const u8,
    buf_len: usize,
}

#[link(wasm_import_module = "wasi_snapshot_preview1")]
unsafe extern "C" {
    fn path_open(
        dirfd: i32, dirflags: i32, path_ptr: *const u8, path_len: usize,
        oflags: i32, rights_base: i64, rights_inheriting: i64, fdflags: i32,
        fd_out_ptr: *mut i32
    ) -> i32;
    fn fd_read(fd: i32, iovs_ptr: *const WasiCiovec, iovs_len: usize, nread_ptr: *mut usize) -> i32;
    fn fd_write(fd: i32, iovs_ptr: *const WasiCiovec, iovs_len: usize, nwritten_ptr: *mut usize) -> i32;
    fn fd_close(fd: i32) -> i32;
}

#[link(wasm_import_module = "krakeos")]
unsafe extern "C" {
    fn fb_flush(ptr: *const u32, len: usize) -> u32;
    fn wait_fs_event(fd: i32) -> u32;
}

const WIDTH: u32 = 800;
const HEIGHT: u32 = 600;

fn main() {
    println!("Window Manager (WASM) started!");

    let path = b"screen.gpu.ram";
    let mut fd = 0;
    
    unsafe {
        if path_open(3, 0, path.as_ptr(), path.len(), 1, 0, 0, 0, &mut fd) != 0 {
            println!("WM: Failed to open file!");
            return;
        }
    }


    let mut pixmap = resvg::tiny_skia::Pixmap::new(WIDTH, HEIGHT).unwrap();
    let opt = usvg::Options::default();
    
    loop {
        // Read file contents
        let mut svg_data = vec![0u8; 1024 * 1024]; // 1MB buffer
        let mut total_read = 0;
        unsafe {
            // Because we don't have fd_seek, we just close and reopen!
            fd_close(fd);
            path_open(3, 0, path.as_ptr(), path.len(), 1, 0, 0, 0, &mut fd);
            
            let iov = WasiCiovec { buf: svg_data.as_mut_ptr(), buf_len: svg_data.len() };
            let mut nr = 0;
            fd_read(fd, &iov, 1, &mut nr);
            total_read = nr;
        }
        
        svg_data.truncate(total_read);
        
        if total_read > 0 {
            let tree_res = usvg::Tree::from_data(&svg_data, &opt);
            if let Ok(tree) = tree_res {
                pixmap.fill(resvg::tiny_skia::Color::BLACK);
                resvg::render(&tree, usvg::Transform::default(), &mut pixmap.as_mut());
                
                let mut fb_buffer = vec![0u32; (WIDTH * HEIGHT) as usize];
                let pixels = pixmap.pixels();
                for i in 0..(WIDTH * HEIGHT) as usize {
                    let p = pixels[i];
                    fb_buffer[i] = 0xFF000000 | ((p.red() as u32) << 16) | ((p.green() as u32) << 8) | (p.blue() as u32);
                }
                
                unsafe { fb_flush(fb_buffer.as_ptr(), (WIDTH * HEIGHT) as usize); }
            } else {
                let err = tree_res.err().unwrap();
                println!("WM: Failed to parse SVG ({} bytes read). Error: {:?}", total_read, err);
                if let Ok(s) = core::str::from_utf8(&svg_data) {
                    println!("WM: SVG Content: {}", s);
                } else {
                    println!("WM: SVG Content is not valid UTF-8!");
                }
            }
        }
        
        println!("WM: Waiting for next frame...");
        unsafe { wait_fs_event(fd); }
    }
}
