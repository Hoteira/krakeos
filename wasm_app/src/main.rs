#![no_std]
#![no_main]

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[link(wasm_import_module = "wasi_snapshot_preview1")]
unsafe extern "C" {
    fn fd_write(fd: i32, iovs_ptr: *const WasiCiovec, iovs_len: usize, nwritten_ptr: *mut usize) -> i32;
    fn fd_read(fd: i32, iovs_ptr: *const WasiCiovec, iovs_len: usize, nread_ptr: *mut usize) -> i32;
    fn fd_close(fd: i32) -> i32;
    fn path_open(
        dirfd: i32, dirflags: i32, path_ptr: *const u8, path_len: usize,
        oflags: i32, rights_base: i64, rights_inheriting: i64, fdflags: i32,
        fd_out_ptr: *mut i32
    ) -> i32;
    fn fd_seek(fd: i32, offset: i64, whence: i32, newoffset_ptr: *mut i64) -> i32;
}

#[link(wasm_import_module = "krakeos")]
unsafe extern "C" {
    fn fb_flush(ptr: *const u32, len: usize) -> u32;
    fn sleep(ms: u32);
}

#[repr(C)]
struct WasiCiovec {
    buf: *const u8,
    buf_len: usize,
}

unsafe fn print(msg: &str) {
    let iov = WasiCiovec { buf: msg.as_ptr(), buf_len: msg.len() };
    let mut nwritten = 0;
    fd_write(1, &iov, 1, &mut nwritten);
}

#[unsafe(no_mangle)]
pub extern "C" fn __main_void() -> i32 {
    unsafe {
        print("\n=== KrakeOS GUI Test (SVG) ===\n");
        
        // Test Persistent FS Write
        let p_path = "boot_log.txt";
        let mut p_fd = 0;
        if path_open(3, 0, p_path.as_ptr(), p_path.len(), 1, 0, 0, 0, &mut p_fd) == 0 {
            let mut newoffset = 0;
            // Seek to end (whence = 2) to test sys_fstat
            fd_seek(p_fd, 0, 2, &mut newoffset);
            
            let log_msg = "wasm_app booted!\n";
            let iov = WasiCiovec { buf: log_msg.as_ptr(), buf_len: log_msg.len() };
            let mut nwritten = 0;
            fd_write(p_fd, &iov, 1, &mut nwritten);
            print("Wrote to persistent FatSquid file!\n");
        }
        
        let path = "screen.gpu.ram";
        let mut fd = 0;
        
        // Open with O_CREAT | O_TRUNC (1 | 8 = 9)
        if path_open(3, 0, path.as_ptr(), path.len(), 9, 0, 0, 0, &mut fd) != 0 {
            print("Failed to open file.\n");
            return 1;
        }
        
        // Animate a bouncing circle
        let mut cx = 100;
        let mut dx = 50;
        
        loop {
            cx += dx;
            if cx > 700 || cx < 100 {
                dx = -dx;
            }
            
            let header = b"<svg width=\"800\" height=\"600\" xmlns=\"http://www.w3.org/2000/svg\">\n<rect width=\"800\" height=\"600\" fill=\"#1a1b26\"/>\n<circle cx=\"";
            let mut cx_str = [0u8; 3];
            let mut temp = cx;
            cx_str[2] = b'0' + (temp % 10) as u8; temp /= 10;
            cx_str[1] = b'0' + (temp % 10) as u8; temp /= 10;
            cx_str[0] = b'0' + (temp % 10) as u8;
            
            let footer = b"\" cy=\"300\" r=\"80\" fill=\"#7aa2f7\" />\n</svg>";
            
            let mut svg_buf = [b' '; 256];
            let mut len = 0;
            for &b in header.iter() { svg_buf[len] = b; len += 1; }
            for &b in cx_str.iter() { svg_buf[len] = b; len += 1; }
            for &b in footer.iter() { svg_buf[len] = b; len += 1; }
            
            let mut newoffset = 0;
            fd_seek(fd, 0, 0, &mut newoffset);
            
            let iov = WasiCiovec { buf: svg_buf.as_ptr(), buf_len: 256 };
            let mut nwritten = 0;
            fd_write(fd, &iov, 1, &mut nwritten);
            
            // Sleep for 16ms to yield CPU to Window Manager
            sleep(16);
        }
    }
    0
}
