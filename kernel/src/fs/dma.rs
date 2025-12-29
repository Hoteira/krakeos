use crate::drivers::pci::*;
use crate::drivers::port::*;


fn test_pci_detection() {
    unsafe {
        if let Some(dev) = find_device(0x8086, 0x7010) {


            BM_BASE = dev.get_bar(4).unwrap() as u16;
            BMR_COMMAND = BM_BASE + 0;
            BMR_STATUS = BM_BASE + 2;
            BMR_PRDT = BM_BASE + 4;

            dev.enable_bus_mastering();
        } else {
            panic!("PIIX4 controller not found!");
        }
    }
}

#[allow(dead_code)]
struct PrdtEntry {
    buffer_phys: u32,
    transfer_size: u16,
    flags: u16,
}

#[allow(dead_code)]
static mut PRDT: PrdtEntry = PrdtEntry {
    buffer_phys: 0,
    transfer_size: 0,
    flags: 0,
};

static mut BM_BASE: u16 = 0;
static mut BMR_COMMAND: u16 = unsafe { BM_BASE + 0 };
static mut BMR_STATUS: u16 = unsafe { BM_BASE + 2 };
static mut BMR_PRDT: u16 = unsafe { BM_BASE + 4 };

#[allow(dead_code)]
const ATA: u16 = 0x1F0;
#[allow(dead_code)]
const ATA_DISK: u16 = ATA + 6;
#[allow(dead_code)]
const ATA_SECTOR: u16 = ATA + 2;
#[allow(dead_code)]
const ATA_LBA_LOW: u16 = ATA + 3;
#[allow(dead_code)]
const ATA_LBA_MID: u16 = ATA + 4;
#[allow(dead_code)]
const ATA_LBA_HIG: u16 = ATA + 5;
#[allow(dead_code)]
const ATA_COMMAND: u16 = ATA + 7;

#[allow(dead_code)]
const ATA_READ_DMA: u8 = 0xC8;
#[allow(dead_code)]
const ATA_WRITE_DMA: u8 = 0xCA;

#[allow(dead_code)]
pub fn read(lba: u64, disk: u8, target: &mut [u8]) {
    let sectors = (target.len() / 512) as u8;
    unsafe {
        PRDT.buffer_phys = target.as_mut_ptr() as u32;
        PRDT.transfer_size = 512 * sectors as u16;
        PRDT.flags = 0x8000;

        outl(BMR_PRDT, core::ptr::addr_of!(PRDT) as u32);

        
        let drive_select = 0xE0 | ((disk & 1) << 4);
        outb(ATA_DISK, (drive_select as u64 | ((lba >> 24) & 0x0F)) as u8);

        outb(ATA_SECTOR, sectors as u8);
        outb(ATA_LBA_LOW, lba as u8);
        outb(ATA_LBA_MID, (lba >> 8) as u8);
        outb(ATA_LBA_HIG, (lba >> 16) as u8);

        outb(ATA_COMMAND, ATA_READ_DMA);

        outb(BMR_COMMAND, 0x8 | 0x1);

        loop {
            let status = inb(BMR_STATUS);

            if status & 0x02 != 0 {
                panic!("DMA error occurred");
            }

            if (status & 0x04) == 0 {
                break;
            }
        }

        let mut command = inb(BMR_COMMAND);
        command &= !0x01;

        outb(BMR_COMMAND, command);

        outb(BMR_STATUS, 0x04 | 0x02);
    }
}

#[allow(dead_code)]
pub fn write(lba: u64, disk: u8, buffer: &[u8]) {
    let sectors = (buffer.len() / 512) as u8;
    unsafe {

        outb(BMR_COMMAND, 0);

        PRDT.buffer_phys = buffer.as_ptr() as u32;
        PRDT.transfer_size = 512 * sectors as u16;
        PRDT.flags = 0x8000;

        outl(BMR_PRDT, core::ptr::addr_of!(PRDT) as u32);

        let drive_select = 0xE0 | ((disk & 1) << 4);
        outb(ATA_DISK, (drive_select as u64 | ((lba >> 24) & 0x0F)) as u8);

        outb(ATA_SECTOR, sectors as u8);
        outb(ATA_LBA_LOW, lba as u8);
        outb(ATA_LBA_MID, (lba >> 8) as u8);
        outb(ATA_LBA_HIG, (lba >> 16) as u8);

        outb(ATA_COMMAND, ATA_WRITE_DMA);

        outb(BMR_COMMAND, 0x1);

        loop {
            let status = inb(BMR_STATUS);

            if status & 0x02 != 0 {
                panic!("DMA error occurred");
            }

            if (status & 0x04) == 0 {
                break;
            }
        }

        let mut command = inb(BMR_COMMAND);
        command &= !0x01;


        outb(BMR_COMMAND, command);
    }
}

#[allow(dead_code)]
pub fn init() {
    test_pci_detection();
}

#[allow(dead_code)]
pub fn is_active() -> bool {
    unsafe { BM_BASE != 0 }
}