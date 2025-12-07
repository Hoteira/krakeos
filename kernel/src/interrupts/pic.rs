use crate::drivers::port::{inb, outb};

pub static mut PICS: Pics = Pics {
    master: Pic {
        offset: 32,
        port: 0x20,
        data: 0x21,
    },
    slave: Pic {
        offset: 40,
        port: 0xa0,
        data: 0xa1,
    },
};

pub struct Pic {
    offset: u8,
    port: u8,
    data: u8,
}

pub struct Pics {
    pub master: Pic,
    pub slave: Pic,
}

impl Pic {
    pub fn read_data(&self) -> u8 {
        let data: u8;

        data = inb(self.data as u16);

        data
    }

    pub fn write_data(&self, data: u8) {
        outb(self.data as u16, data);
    }

    pub fn send_command(&self, command: u8) {
        outb(self.port as u16, command);
    }

    pub fn end_interrupt(&self) {
        outb(self.port as u16, 0x20);
    }

    pub fn handles_interrupt(&self, interrupt: u8) -> bool {
        self.offset <= interrupt && interrupt < self.offset.wrapping_add(8)
    }

    pub fn unmask_irq(&self, irq: u8) {
        let mask = self.read_data() & !(1 << irq);
        self.write_data(mask);
    }

    #[allow(dead_code)]
    pub fn mask_irq(&self, irq: u8) {
        let mask = self.read_data() | (1 << irq);
        self.write_data(mask);
    }
}

impl Pics {
    pub fn init(&self) {
        // Start initialization sequence (ICW1)
        self.master.send_command(0x11);
        wait();
        self.slave.send_command(0x11);
        wait();

        // ICW2: Remap offsets
        self.master.write_data(self.master.offset);
        wait();
        self.slave.write_data(self.slave.offset);
        wait();

        // ICW3: Cascade setup
        self.master.write_data(4); // Master has slave at IRQ2
        wait();
        self.slave.write_data(2);  // Slave identity
        wait();

        // ICW4: Environment (8086 mode)
        self.master.write_data(0x01);
        wait();
        self.slave.write_data(0x01);
        wait();

        // OCW1: Mask all interrupts to prevent artifacts/residue
        self.master.write_data(0xFF);
        self.slave.write_data(0xFF);

        // Unmask specific interrupts
        self.master.unmask_irq(0); // Timer
        self.master.unmask_irq(1); // Keyboard
        self.master.unmask_irq(2); // Cascade (Required for Slave PIC)
        
        self.slave.unmask_irq(3);  // Legacy/User IRQ (IRQ 11)
        self.slave.unmask_irq(4);  // Mouse (IRQ 12)
    }

    pub fn handles_interrupt(&self, interrupt: u8) -> bool {
        self.master.handles_interrupt(interrupt) || self.slave.handles_interrupt(interrupt)
    }

    pub fn end_interrupt(&self, interrupt: u8) {
        if self.handles_interrupt(interrupt) {
            if self.slave.handles_interrupt(interrupt) {
                self.slave.end_interrupt();
            }
            self.master.end_interrupt();
        }
    }
}

#[allow(dead_code)]
pub fn reset_ps2_controller() {
    outb(0x64, 0xAD);
    outb(0x64, 0xA7);

    while (inb(0x64) & 1) != 0 {
        let _ = inb(0x60);
    }

    outb(0x64, 0x20);
    let config = inb(0x60) & 0xBC;
    outb(0x64, 0x60);
    outb(0x60, config);
}

pub fn wait() {
    outb(0x80, 0);
}
