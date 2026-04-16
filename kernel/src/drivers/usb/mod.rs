pub mod xhci;
pub mod hid;
pub mod mass_storage;

/// USB device speed
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum UsbSpeed {
    Low,        // 1.5 Mbps  (USB 1.0)
    Full,       // 12 Mbps   (USB 1.1)
    High,       // 480 Mbps  (USB 2.0)
    Super,      // 5 Gbps    (USB 3.0)
    SuperPlus,  // 10 Gbps   (USB 3.1)
}

/// USB device class codes
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum UsbClass {
    Hid,          // 0x03
    MassStorage,  // 0x08
    Hub,          // 0x09
    Other(u8),
}

impl UsbClass {
    pub fn from_code(code: u8) -> Self {
        match code {
            0x03 => UsbClass::Hid,
            0x08 => UsbClass::MassStorage,
            0x09 => UsbClass::Hub,
            c    => UsbClass::Other(c),
        }
    }
}

/// Minimal descriptor for an enumerated USB device slot.
#[derive(Debug, Copy, Clone)]
pub struct UsbDevice {
    pub slot_id:    u8,
    pub speed:      UsbSpeed,
    pub class:      UsbClass,
    pub subclass:   u8,
    pub protocol:   u8,
    pub vendor_id:  u16,
    pub product_id: u16,
}

/// Abstract input device — implemented by HID mouse, keyboard, tablet, and PS/2.
pub trait InputDevice: Send + Sync {
    fn kind(&self) -> InputKind;
    /// Poll any pending events; returns number written into `out`.
    fn poll(&mut self, out: &mut [InputEvent]) -> usize;
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum InputKind {
    Keyboard,
    Mouse,
    Tablet,    // absolute-coordinate digitizer
}

#[derive(Debug, Copy, Clone)]
pub enum InputEvent {
    Key { scancode: u8, pressed: bool },
    MouseRel { dx: i16, dy: i16, dz: i16, buttons: u8 },
    TabletAbs { x: u16, y: u16, pressure: u16, buttons: u8 },
}
