use core::arch::asm;

#[allow(dead_code)]
pub struct Port {
    port: u16
}

impl Port {

    #[allow(dead_code)]
    pub fn new(port: u16) -> Self {
        Port { port }
    }

    #[allow(dead_code)]
    pub fn outb(&self, value: u8) {
        outb(self.port, value);
    }

    #[allow(dead_code)]
    pub fn inb(&self) -> u8 {
        inb(self.port)
    }

    #[allow(dead_code)]
    pub fn outw(&self, value: u16) {
        outw(self.port, value);
    }

    #[allow(dead_code)]
    pub fn inw(&self) -> u16 {
        inw(self.port)
    }

    #[allow(dead_code)]
    pub fn outl(&self, value: u32) {
        outl(self.port, value);
    }

    #[allow(dead_code)]
    pub fn inl(&self) -> u32 {
        inl(self.port)
    }
}

#[allow(dead_code)]
#[inline(always)]
pub fn inb(port: u16) -> u8 {
    let value: u8;
    unsafe {
        asm!(
        "in al, dx",
        out("al") value,
        in("dx") port);
    }
    value
}

#[allow(dead_code)]
#[inline(always)]
pub fn outb(port: u16, value: u8) {
    unsafe {
        asm!(
        "out dx, al",
        in("dx") port,
        in("al") value);
    }
}

#[allow(dead_code)]
#[inline(always)]
pub fn outw(port: u16, value: u16) {
    unsafe {
        asm!(
        "out dx, ax",
        in("dx") port,
        in("ax") value);
    }
}

#[allow(dead_code)]
#[inline(always)]
pub fn inw(port: u16) -> u16 {
    let value: u16;
    unsafe {
        asm!(
        "in ax, dx",
        out("ax") value,
        in("dx") port);
    }
    value
}

#[allow(dead_code)]
#[inline(always)]
pub fn outl(port: u16, value: u32) {
    unsafe {
        asm!(
        "out dx, eax",
        in("dx") port,
        in("eax") value);
    }
}

#[allow(dead_code)]
#[inline(always)]
pub fn inl(port: u16) -> u32 {
    unsafe {
        let value: u32;
        asm!(
        "in eax, dx",
        in("dx") port,
        out("eax") value);

        value
    }
}
