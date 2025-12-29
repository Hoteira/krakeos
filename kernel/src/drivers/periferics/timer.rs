

use crate::drivers::port::outb;

const PIT_CHANNEL_0: u16 = 0x40;
const PIT_COMMAND: u16 = 0x43;

pub fn init_pit(frequency: u32) {
    let divisor = 1193182 / frequency;

    unsafe {
        
        
        
        
        
        outb(PIT_COMMAND, 0x36);

        
        outb(PIT_CHANNEL_0, (divisor & 0xFF) as u8);
        outb(PIT_CHANNEL_0, ((divisor >> 8) & 0xFF) as u8);
    }
}
