use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::arch::asm;

pub static mut PID: u64 = 0;

#[inline(never)]
pub unsafe fn syscall(num: u64, arg1: u64, arg2: u64, arg3: u64) -> u64 {
    let res;
    asm!(
        "syscall",
        in("rax") num,
        in("rdi") arg1,
        in("rsi") arg2,
        in("rdx") arg3,
        lateout("rax") res,
        out("rcx") _,
        out("r11") _,
        options(nostack, preserves_flags)
    );
    res
}

pub fn malloc(size: u64) -> u64 { unsafe { syscall(5, size, 0, 0) } }

pub fn sys_free(base: u64, pid: u64) {
    let main_pid = (pid >> 32);
    let child_pid = pid & 0xFFFFFFFF;
    unsafe {
        syscall(6, base, main_pid, child_pid);
    }
}

pub fn expand(base: u64, size: u64) -> u64 { unsafe { syscall(10, base, size, 0) } }

pub fn get_dub_buffer() -> u64 {
    unsafe { syscall(7, 0, 0, 0) }
}

pub fn write_to_screen(buffer: u64, c: Coordiates) {
    unsafe { syscall(8, buffer, &c as *const _ as u64, 0); }
}

pub fn write_wid_to_screen(wid: u64) {
    unsafe { syscall(9, wid, 0, 0); }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[repr(C)]
pub enum Items {
    Wallpaper,
    Bar,
    Popup,
    Window,
    Null,
}

#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub struct Window {
    pub id: usize,
    pub buffer: usize,

    pub x: usize,
    pub y: usize,
    pub z: usize,
    pub width: usize,
    pub height: usize,

    pub can_move: bool,
    pub can_resize: bool,
    pub min_width: usize,
    pub min_height: usize,

    pub event_handler: usize,
    pub w_type: Items,
}

pub fn add_window(w: &Window) -> u64 {
    unsafe { syscall(22, w as *const _ as u64, 0, 0) }
}

pub fn update_window(w: &Window) {
    unsafe { syscall(51, w as *const _ as u64, 0, 0); }
}

pub fn remove_window(id: u64) {
    unsafe { syscall(23, id, 0, 0); }
}

pub fn get_events(window_id: u64, events_ptr: u64, max_events: u64) {
    unsafe { syscall(52, window_id, events_ptr, max_events); }
}

/*pub fn change_window(w: Window) {

    syscall(24, 0, 0, 0);
}*/

pub fn add_task(base: u64, args: Option<&[u64]>) {
    let mut args_ptr = 0;
    if args.is_some() {
        args_ptr = args.unwrap().as_ptr() as u64;
    }
    unsafe { syscall(25, base, 0, args_ptr); }
}

pub fn exit() -> ! {
    unsafe { syscall(60, 0, 0, 0); }

    loop {}
}

pub fn poll_event(window_id: u64, event_ptr: u64) {
    unsafe { syscall(50, event_ptr, window_id, 0); }
}

pub fn thread_yield() {
    unsafe {
        asm!("int 0x20");
    }
}

pub fn move_window(id: u64, x: u64, y: u64) {
    unsafe { syscall(43, id, x ,y); }
}

pub fn get_screen_width() -> u64 {
    unsafe { syscall(44, 0, 0, 0) }
}

pub fn get_screen_height() -> u64 {
    unsafe { syscall(45, 0, 0, 0) }
}

pub fn get_mouse_pos() -> (u16, u16) {
    let res = unsafe { syscall(53, 0, 0, 0) };
    let x = (res >> 32) as u16;
    let y = res as u16;
    (x, y)
}

#[repr(C, packed)]
#[derive(Copy, Clone, Debug)]
pub struct Process {
    id: u16,
    draw: u64,
    mouse: u64,
}

#[repr(C, packed)]
#[derive(Copy, Clone, Debug)]
pub struct Coordiates {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}
