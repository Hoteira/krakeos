pub static mut BOOT_INFO: BootInfo = unsafe { core::mem::zeroed() };
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct BootInfo {
    pub mmap: MemoryMap,
    rsdp: Rsdp,
    pub tss: u16,
    vbe: VbeInfoBlock,
    pub mode: VbeModeInfoBlock,
    pub pml4: u64,
    pub kernel_stack: u64,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct MemoryMapEntry {
    pub base: u64,
    pub length: u64,
    pub memory_type: u32,
    pub reserved_acpi: u32,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct MemoryMap {
    pub entries: [MemoryMapEntry; 32],
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Rsdp {
    signature: [u8; 8],
    checksum: u8,
    oem_id: [u8; 6],
    revision: u8,
    rsdt_address: u32,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct TaskStateSegment {
    pub reserved1: u32,
    pub rsp0: u64,
    pub rsp1: u64,
    pub rsp2: u64,
    pub reserved2: u64,
    pub ist1: u64,
    pub ist2: u64,
    pub ist3: u64,
    pub ist4: u64,
    pub ist5: u64,
    pub ist6: u64,
    pub ist7: u64,
    pub reserved3: u64,
    pub reserved4: u16,
    pub iopb_offset: u16,
}

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
    pub segment_b: u16,
    pub win_func_ptr: u32,
    pub pitch: u16,
    pub width: u16,
    pub height: u16,
    pub w_char: u8,
    pub y_char: u8,
    pub planes: u8,
    pub bpp: u8,
    pub banks: u8,
    pub memory_model: u8,
    pub bank_size: u8,
    pub image_pages: u8,
    pub reserved0: u8,
    pub red_mask_size: u8,
    pub red_field_position: u8,
    pub green_mask_size: u8,
    pub green_field_position: u8,
    pub blue_mask_size: u8,
    pub blue_field_position: u8,
    pub reserved_mask_size: u8,
    pub reserved_field_position: u8,
    pub direct_color_mode_info: u8,
    pub framebuffer: u32,
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