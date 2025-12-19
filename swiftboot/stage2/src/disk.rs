use core::arch::asm;
use core::mem::size_of;

#[repr(C, packed)]
struct Dap {
    size: u8,
    zero: u8,
    sectors: u16,
    offset: u16,
    segment: u16,
    lba: u64,
}

pub fn read(segment: u16, offset: u16, lba: u64, sectors: u16) {
    let disk_setup = Dap {
        size: size_of::<Dap>() as u8,
        zero: 0,
        sectors,
        offset,
        segment,
        lba,
    };

    unsafe {
        asm!(
        "mov {1:x}, si",
        "mov si, {0:x}",
        "int 0x13",

        "jc fail",
        "mov si, {1:x}",

        in(reg) &disk_setup as *const Dap as u16,
        out(reg) _,
        in("ax") 0x4200_u16,
        in("dx") 0x80_u16,
        );
    }
}
