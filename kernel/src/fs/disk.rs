use core::arch::asm;
use crate::drivers::port::*;
use crate::debugln;

#[allow(dead_code)]
pub fn read(lba: u64, disk: u8, buffer: &mut [u8]) {
    debugln!("Disk: Read LBA {} (DMA Active: {})", lba, crate::fs::dma::is_active());
    if crate::fs::dma::is_active() {
        crate::fs::dma::read(lba, disk, buffer);
        return;
    }

    // PIO Fallback
    while is_busy() {}

    let total_bytes = buffer.len();
    let sector_count = (total_bytes + 511) / 512;
    
    outb(0x3f6, 0b00000010);

    outb(0x1F1, 0x00);
    outb(0x1F2, if sector_count > 255 { 0 } else { sector_count as u8 }); // 0 means 256 sectors, but PIO loop handles it manually
    // Actually PIO usually does one sector at a time or block? 
    // The previous code did loop over sectors. Let's do sector by sector to be safe and handle large reads.

    let mut current_lba = lba;
    let mut bytes_read = 0;
    
    while bytes_read < total_bytes {
        let bytes_remaining = total_bytes - bytes_read;
        
        while is_busy() {}

        outb(0x1F2, 1); // Read 1 sector
        outb(0x1F3, current_lba as u8);
        outb(0x1F4, (current_lba >> 8) as u8);
        outb(0x1F5, (current_lba >> 16) as u8);
            outb(0x1F6, disk | ((current_lba >> 24) & 0x0F) as u8);
        while is_busy() {}
        while !is_ready() {}

        for i in 0..256 {
            let word = inw(0x1F0);
            if bytes_remaining > 0 {
                 let current_offset = bytes_read + i * 2;
                 // Write first byte
                 if current_offset < total_bytes {
                     buffer[current_offset] = (word & 0xFF) as u8;
                 }
                 // Write second byte
                 if current_offset + 1 < total_bytes {
                     buffer[current_offset + 1] = (word >> 8) as u8;
                 }
            }
        }
        
        bytes_read += 512; // We consumed a sector from the disk, even if we didn't store all bytes
        current_lba += 1;
    }

    reset();
}

#[allow(dead_code)]
pub fn write(lba: u64, disk: u8, buffer: &[u8]) {
    if crate::fs::dma::is_active() {
        crate::fs::dma::write(lba, disk, buffer);
        return;
    }

    // PIO Fallback
    let total_bytes = buffer.len();
    let _sector_count = (total_bytes + 511) / 512;
    
    let mut current_lba = lba;
    let mut bytes_written = 0;

    while bytes_written < total_bytes {
        while is_busy() {}

        outb(0x3f6, 0b00000010);
        outb(0x1F1, 0x00);
        outb(0x1F2, 1); // Write 1 sector
        outb(0x1F3, current_lba as u8);
        outb(0x1F4, (current_lba >> 8) as u8);
        outb(0x1F5, (current_lba >> 16) as u8);
        outb(0x1F6, disk | ((current_lba >> 24) & 0x0F) as u8);
        outb(0x1F7, 0x30); // Write command

        while is_busy() {}
        while !is_ready() {}

        for i in 0..256 {
             let current_offset = bytes_written + i * 2;
             let mut word: u16 = 0;

             if current_offset < total_bytes {
                 word |= buffer[current_offset] as u16;
             }
             if current_offset + 1 < total_bytes {
                 word |= (buffer[current_offset + 1] as u16) << 8;
             }
             // If beyond bounds, word remains 0 (zero padding)
             
             outw(0x1F0, word);
        }
        
        bytes_written += 512;
        current_lba += 1;
    }

    reset();
    outb(0x1F7, 0xE7); // Cache flush?
}

#[allow(dead_code)]
pub fn reset() {
    outb(0x3f6, 0b00000110);
    outb(0x3f6, 0b00000010);
}

#[allow(dead_code)]
pub fn is_ready() -> bool {
    let status: u8 = inb(0x1F7);

    (status & 0b01000000) != 0
}

pub fn is_busy() -> bool {
    let status: u8 = inb(0x1F7);

    (status & 0b10000000) != 0
}

fn delay() {
    for _ in 0..10000 {
        unsafe { asm!("nop") };
    }
}

pub fn check_disk() -> [bool; 2] {
    let mut master = false;
    let mut slave = false;

    outb(0x1F6, 0xF0);
    outb(0x1F7, 0xEC);

    delay();

    let status = inb(0x1F7);
    if status != 0 {
        slave = true;
    }

    delay();

    outb(0x1F6, 0xE0);
    outb(0x1F7, 0xEC);

    delay();

    let status = inb(0x1F7);
    if status != 0 {
        master = true;
    }

    [master, slave]
}

