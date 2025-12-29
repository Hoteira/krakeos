use core::arch::asm;
use log::debug;
use crate::{BOOT, MAX_BPP, MAX_HEIGHT, MAX_WIDTH, MIN_BPP, MIN_HEIGHT, MIN_WIDTH, VBE_MODE};
use crate::debug::debug;

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct VbeInfoBlock {
    pub signature: [u8; 4],
    pub version: u16,
    pub oem: [u16; 2],
    pub dunno: [u8; 4],
    pub video_ptr: u32,
    pub memory_size: u16,
    pub reserved: [u8; 492],
}

#[repr(C, packed)]
#[derive(Copy, Clone, Debug)]
pub struct VbeModeInfoBlock {
    attributes: u16,
    window_a: u8,
    window_b: u8,
    granularity: u16,
    window_size: u16,
    segment_a: u16,
    segment_b: u16,
    win_func_ptr: u32,
    pitch: u16,
    width: u16,
    height: u16,
    w_char: u8,
    y_char: u8,
    planes: u8,
    bpp: u8,
    banks: u8,
    memory_model: u8,
    bank_size: u8,
    image_pages: u8,
    reserved0: u8,
    red_mask_size: u8,
    red_field_position: u8,
    green_mask_size: u8,
    green_field_position: u8,
    blue_mask_size: u8,
    blue_field_position: u8,
    reserved_mask_size: u8,
    reserved_field_position: u8,
    direct_color_mode_info: u8,
    framebuffer: u32,
    reserved1: u32,
    reserved2: u16,
    lin_bytes_per_scan_line: u16,
    bnk_image_pages: u8,
    lin_image_pages: u8,
    lin_red_mask_size: u8,
    lin_red_field_position: u8,
    lin_green_mask_size: u8,
    lin_green_field_position: u8,
    lin_blue_mask_size: u8,
    lin_blue_field_position: u8,
    lin_reserved_mask_size: u8,
    lin_reserved_field_position: u8,
    max_pixel_clock: u32,
    reserved3: [u8; 189],
}

#[inline(never)]
pub fn load_vbe_mode(mode: u16) {
    let mode_info_ptr = core::ptr::addr_of!(VBE_MODE) as usize;

    unsafe {
        asm!(
        "int 0x10",
        in("ax") 0x4F01,
        in("cx") mode,
        in("edi") mode_info_ptr,
        options(nostack)
        );
    }
}

#[inline(never)]
fn save_vbe_mode(mode: u16) {
    let mode_info_ptr = unsafe { &raw mut BOOT.mode as usize };

    unsafe {
        asm!(
        "int 0x10",
        in("ax") 0x4F01,
        in("cx") mode,
        in("edi") mode_info_ptr,
        options(nostack)
        );
    }
}

#[inline(never)]
pub fn get_vbe_info() -> VbeInfoBlock {
    let mut vbe_info = unsafe { core::mem::zeroed() };

    unsafe {
        asm!(
        "int 0x10",
        in("ax") 0x4F00,
        in("edi") ((&mut vbe_info as *mut VbeInfoBlock as usize)),
        options(nostack)
        );
    }

    vbe_info
}

#[inline(never)]
pub fn find_vbe_mode() -> u16 {
    let base_mode = unsafe { BOOT.vbe.video_ptr } as *const u16;

    let mut best_mode = 0x0013;
    let mut best_width = 0;
    let mut best_height = 0;
    let mut best_bpp = 0;
    let mut i = 0;

    loop {
        let mode = unsafe { core::ptr::read_volatile(base_mode.offset(i)) };

        if mode == 0xFFFF {
            break;
        }

        load_vbe_mode(mode);

        let mode_width = unsafe { VBE_MODE.width };
        let mode_height = unsafe { VBE_MODE.height };
        let mode_bpp = unsafe { VBE_MODE.bpp };
        let mode_red = unsafe { VBE_MODE.red_field_position };
        let mode_green = unsafe { VBE_MODE.green_field_position };
        let mode_blue = unsafe { VBE_MODE.blue_field_position };
        let mode_attr = unsafe { VBE_MODE.attributes };
        let mode_fb = unsafe { VBE_MODE.framebuffer };

        if mode_red != 16 || mode_green != 8 || mode_blue != 0 || mode_fb == 0 {
            i += 1;
            continue;
        }

        if mode_width >= MIN_WIDTH
            && mode_width <= MAX_WIDTH
            && mode_height >= MIN_HEIGHT
            && mode_height <= MAX_HEIGHT
            && mode_bpp >= MIN_BPP
            && mode_bpp <= MAX_BPP
            && (mode_attr & 0x80) != 0  
            && (mode_width > best_width
            || (mode_width == best_width && mode_height > best_height)
            || (mode_width == best_width && mode_height == best_height && mode_bpp > best_bpp))
        {
            best_mode = mode;
            best_width = mode_width;
            best_height = mode_height;
            best_bpp = mode_bpp;
            save_vbe_mode(best_mode);
        }

        i += 1;
    }

    best_mode
}