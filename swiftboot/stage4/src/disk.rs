use core::arch::asm;

pub fn read<T>(lba: u64, sectors: u16, target: *mut T) {
    let mut current_lba = lba;
    let mut current_target = target as *mut u8;

    for _ in 0..sectors {
        while is_busy() {}

        outb(0x3f6, 0b00000010);

        outb(0x1F1, 0x00);
        outb(0x1F2, 1);
        outb(0x1F3, current_lba as u8);
        outb(0x1F4, (current_lba >> 8) as u8);
        outb(0x1F5, (current_lba >> 16) as u8);
        outb(0x1F6, (0xE0 | ((current_lba >> 24) & 0x0F)) as u8);

        outb(0x1F7, 0x20);

        while is_busy() {}
        while !is_ready() {}

        for _ in 0..256 {
            let bytes_16 = inw(0x1F0);

            unsafe {
                core::ptr::write_unaligned(current_target, (bytes_16 & 0xFF) as u8);
                current_target = current_target.add(1);
                core::ptr::write_unaligned(
                    current_target,
                    ((bytes_16 >> 8) & 0xFF) as u8,
                );
                current_target = current_target.add(1);
            }
        }
        current_lba += 1;
    }

    reset();
}

pub fn reset() {
    outb(0x3f6, 0b00000110);
    outb(0x3f6, 0b00000010);
}

pub fn is_ready() -> bool {
    let status: u8 = inb(0x1F7);

    (status & 0b01000000) != 0
}

pub fn is_busy() -> bool {
    let status: u8 = inb(0x1F7);

    (status & 0b10000000) != 0
}

pub fn inb(port: u16) -> u8 {
    let value: u8;
    unsafe {
        asm!(
            "in al, dx",
            out("al") value,
            in("dx") port,
            options(nomem, nostack, preserves_flags));
    }
    value
}

pub fn outb(port: u16, value: u8) {
    unsafe {
        asm!(
            "out dx, al",
            in("dx") port,
            in("al") value,
            options(nomem, nostack, preserves_flags));
    }
}

pub fn inw(port: u16) -> u16 {
    let value: u16;
    unsafe {
        asm!(
            "in ax, dx",
            out("ax") value,
            in("dx") port,
            options(nomem, nostack, preserves_flags)
        );
    }
    value
}
