/// USB HID driver — handles Keyboard (protocol 1), Mouse (protocol 2),
/// and Tablet/Digitizer (subclass 0, protocol 0 with absolute coords).
///
/// On registration the device is classified and added to the input device
/// list.  The WM input module prefers USB HID devices and falls back to PS/2
/// only when no HID device is present.

use super::{UsbDevice, InputDevice, InputEvent, InputKind};
use crate::sync::Mutex;
use alloc::vec::Vec;

// HID subclass / protocol codes
const HID_SUBCLASS_BOOT: u8 = 1;
const HID_PROTOCOL_KEYBOARD: u8 = 1;
const HID_PROTOCOL_MOUSE:    u8 = 2;

// ─── Concrete HID devices ────────────────────────────────────────────────────

pub struct HidKeyboard {
    pub slot_id: u8,
    pending: [InputEvent; 16],
    pending_len: usize,
}

impl HidKeyboard {
    fn new(dev: UsbDevice) -> Self {
        HidKeyboard { slot_id: dev.slot_id, pending: [InputEvent::Key { scancode: 0, pressed: false }; 16], pending_len: 0 }
    }
}

impl InputDevice for HidKeyboard {
    fn kind(&self) -> InputKind { InputKind::Keyboard }
    fn poll(&mut self, out: &mut [InputEvent]) -> usize {
        // TODO: read from interrupt endpoint ring set up during HID init.
        // For now returns 0 — the full implementation requires the EP1 IN
        // interrupt ring, which is set up by configure_hid_ep().
        let _ = out;
        0
    }
}

pub struct HidMouse {
    pub slot_id: u8,
}

impl InputDevice for HidMouse {
    fn kind(&self) -> InputKind { InputKind::Mouse }
    fn poll(&mut self, out: &mut [InputEvent]) -> usize {
        let _ = out;
        0 // TODO: read from interrupt EP ring
    }
}

pub struct HidTablet {
    pub slot_id: u8,
}

impl InputDevice for HidTablet {
    fn kind(&self) -> InputKind { InputKind::Tablet }
    fn poll(&mut self, out: &mut [InputEvent]) -> usize {
        let _ = out;
        0 // TODO: read from interrupt EP ring
    }
}

// ─── Device list ──────────────────────────────────────────────────────────────

struct HidList {
    keyboards: Vec<HidKeyboard>,
    mice:      Vec<HidMouse>,
    tablets:   Vec<HidTablet>,
}

impl HidList {
    const fn new() -> Self {
        HidList { keyboards: Vec::new(), mice: Vec::new(), tablets: Vec::new() }
    }
}

static HID_DEVICES: Mutex<HidList> = Mutex::new(HidList::new());

/// Called by xHCI when a new HID device is enumerated.
pub fn register(dev: UsbDevice) {
    let proto = dev.protocol;
    let sub   = dev.subclass;

    crate::debugln!("[HID] Registering slot={} proto={} sub={}", dev.slot_id, proto, sub);

    let mut list = HID_DEVICES.lock();

    if sub == HID_SUBCLASS_BOOT {
        match proto {
            HID_PROTOCOL_KEYBOARD => list.keyboards.push(HidKeyboard::new(dev)),
            HID_PROTOCOL_MOUSE    => list.mice.push(HidMouse { slot_id: dev.slot_id }),
            _ => {}
        }
    } else {
        // Treat unknown protocol as tablet (absolute coordinates) when sub=0
        list.tablets.push(HidTablet { slot_id: dev.slot_id });
    }
}

/// True if at least one USB keyboard is registered.
pub fn has_keyboard() -> bool {
    !HID_DEVICES.lock().keyboards.is_empty()
}

/// True if at least one USB mouse or tablet is registered.
pub fn has_pointer() -> bool {
    let l = HID_DEVICES.lock();
    !l.mice.is_empty() || !l.tablets.is_empty()
}

/// Poll all HID keyboards; writes events into `out`, returns count.
pub fn poll_keyboards(out: &mut [InputEvent]) -> usize {
    let mut total = 0;
    let mut list = HID_DEVICES.lock();
    for kb in &mut list.keyboards {
        total += kb.poll(&mut out[total..]);
        if total >= out.len() { break; }
    }
    total
}

/// Poll all HID mice; writes events into `out`, returns count.
pub fn poll_mice(out: &mut [InputEvent]) -> usize {
    let mut total = 0;
    let mut list = HID_DEVICES.lock();
    for m in &mut list.mice {
        total += m.poll(&mut out[total..]);
        if total >= out.len() { break; }
    }
    total
}
