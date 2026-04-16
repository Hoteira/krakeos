/// Device Registry — built once at boot by scanning PCI.
///
/// Every driver init path registers its device here.  Syscall handlers use
/// `get_category()` to find the active driver for a category without
/// hard-coding vendor/device IDs in every call site.
use crate::drivers::pci::PciDevice;
use crate::sync::Mutex;

// ─── Categories ──────────────────────────────────────────────────────────────

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum DeviceCategory {
    BlockStorage,
    UsbController,
    NetworkAdapter,
    Display,
    InputKeyboard,
    InputMouse,
    Other,
}

// ─── Driver kinds ─────────────────────────────────────────────────────────────

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum DriverKind {
    // Block storage
    VirtioBlock,
    Ahci,
    Nvme,
    LegacyIde,
    // USB
    XhciUsb,
    EhciUsb,
    OhciUsb,
    UhciUsb,
    // Network
    VirtioNet,
    // Display
    VirtioGpu,
    VgaDisplay,
    // Input (legacy)
    PS2Keyboard,
    PS2Mouse,
    // Fallback
    Unknown,
}

// ─── Entry ────────────────────────────────────────────────────────────────────

#[derive(Copy, Clone)]
pub struct DeviceEntry {
    pub category: DeviceCategory,
    pub driver:   DriverKind,
    pub pci:      Option<PciDevice>,
    pub name:     &'static str,
    /// Set to true by the driver's init() once hardware is confirmed active.
    pub active:   bool,
}

// ─── Table ────────────────────────────────────────────────────────────────────

const MAX_DEVICES: usize = 64;

struct DeviceTable {
    entries: [DeviceEntry; MAX_DEVICES],
    count:   usize,
}

impl DeviceTable {
    const fn empty_entry() -> DeviceEntry {
        DeviceEntry {
            category: DeviceCategory::Other,
            driver:   DriverKind::Unknown,
            pci:      None,
            name:     "",
            active:   false,
        }
    }

    const fn new() -> Self {
        DeviceTable {
            entries: [Self::empty_entry(); MAX_DEVICES],
            count: 0,
        }
    }

    fn push(&mut self, entry: DeviceEntry) {
        if self.count < MAX_DEVICES {
            self.entries[self.count] = entry;
            self.count += 1;
        }
    }

    fn as_slice(&self) -> &[DeviceEntry] {
        &self.entries[..self.count]
    }
}

static DEVICE_TABLE: Mutex<DeviceTable> = Mutex::new(DeviceTable::new());

// ─── Classify ────────────────────────────────────────────────────────────────

fn classify(dev: PciDevice, prog_if: u8) -> (DeviceCategory, DriverKind, &'static str) {
    // VirtIO devices — identified by vendor ID first
    if dev.vendor_id == 0x1AF4 {
        match dev.device_id {
            0x1001 | 0x1042 => return (DeviceCategory::BlockStorage, DriverKind::VirtioBlock, "VirtIO Block"),
            0x1000 | 0x1041 => return (DeviceCategory::NetworkAdapter, DriverKind::VirtioNet,  "VirtIO Network"),
            0x1050 | 0x1052 => return (DeviceCategory::Display,        DriverKind::VirtioGpu,  "VirtIO GPU"),
            _ => {}
        }
    }

    match (dev.class as u8, dev.subclass as u8, prog_if) {
        // Block storage
        (0x01, 0x06, _)    => (DeviceCategory::BlockStorage, DriverKind::Ahci,      "AHCI Controller"),
        (0x01, 0x08, 0x02) => (DeviceCategory::BlockStorage, DriverKind::Nvme,      "NVMe Controller"),
        (0x01, 0x01, _)    => (DeviceCategory::BlockStorage, DriverKind::LegacyIde, "IDE Controller"),
        (0x01, 0x80, _)    => (DeviceCategory::BlockStorage, DriverKind::VirtioBlock,"Mass Storage (other)"),

        // USB controllers
        (0x0C, 0x03, 0x30) => (DeviceCategory::UsbController, DriverKind::XhciUsb, "xHCI USB 3.x"),
        (0x0C, 0x03, 0x20) => (DeviceCategory::UsbController, DriverKind::EhciUsb, "EHCI USB 2.0"),
        (0x0C, 0x03, 0x10) => (DeviceCategory::UsbController, DriverKind::OhciUsb, "OHCI USB 1.1"),
        (0x0C, 0x03, 0x00) => (DeviceCategory::UsbController, DriverKind::UhciUsb, "UHCI USB 1.1"),

        // Network
        (0x02, 0x00, _)    => (DeviceCategory::NetworkAdapter, DriverKind::VirtioNet, "Ethernet Controller"),

        // Display
        (0x03, 0x00, _)    => (DeviceCategory::Display,    DriverKind::VgaDisplay,   "VGA Display"),
        (0x03, 0x80, _)    => (DeviceCategory::Display,    DriverKind::VirtioGpu,    "Other Display"),

        // Input (PS/2 via legacy ISA — no PCI device, handled separately)
        (0x09, 0x00, _)    => (DeviceCategory::InputKeyboard, DriverKind::PS2Keyboard, "Keyboard Controller"),
        (0x09, 0x02, _)    => (DeviceCategory::InputMouse,    DriverKind::PS2Mouse,    "Mouse Controller"),

        _ => (DeviceCategory::Other, DriverKind::Unknown, "Unknown Device"),
    }
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Scan the PCI bus and populate the device table.  Called once during boot
/// after PCI is accessible but before individual driver `init()` calls.
pub fn enumerate() {
    use crate::arch::x86_64::io::{inl, outl};

    fn pci_read(bus: u8, dev: u8, func: u8, off: u8) -> u32 {
        let addr: u32 = 0x80000000
            | ((bus as u32) << 16)
            | ((dev as u32) << 11)
            | ((func as u32) << 8)
            | (off as u32 & 0xFC);
        outl(0xCF8, addr);
        inl(0xCFC)
    }

    let mut table = DEVICE_TABLE.lock();

    // Add synthetic PS/2 entries — always present on x86.
    table.push(DeviceEntry {
        category: DeviceCategory::InputKeyboard,
        driver:   DriverKind::PS2Keyboard,
        pci:      None,
        name:     "PS/2 Keyboard",
        active:   true,
    });
    table.push(DeviceEntry {
        category: DeviceCategory::InputMouse,
        driver:   DriverKind::PS2Mouse,
        pci:      None,
        name:     "PS/2 Mouse",
        active:   true,
    });

    for bus in 0u8..=255 {
        for dev in 0u8..32 {
            for func in 0u8..8 {
                let id_reg   = pci_read(bus, dev, func, 0x00);
                let vendor   = (id_reg & 0xFFFF) as u32;
                if vendor == 0xFFFF { continue; }

                let device_id = (id_reg >> 16) as u32;
                let class_reg = pci_read(bus, dev, func, 0x08);
                let class     = ((class_reg >> 24) & 0xFF) as u32;
                let subclass  = ((class_reg >> 16) & 0xFF) as u32;
                let prog_if   = ((class_reg >> 8)  & 0xFF) as u8;

                let pci_dev = PciDevice { class, subclass, vendor_id: vendor, device_id, bus, device: dev, function: func };
                let (cat, drv, name) = classify(pci_dev, prog_if);

                crate::debugln!("[Registry] {:02x}:{:02x}.{} {:04x}:{:04x} class={:02x}/{:02x} prog={:02x} → {:?} / {:?}",
                    bus, dev, func, vendor, device_id, class, subclass, prog_if, cat, drv);

                table.push(DeviceEntry { category: cat, driver: drv, pci: Some(pci_dev), name, active: false });

                // Stop scanning functions if not a multi-function device
                if func == 0 {
                    let hdr = (pci_read(bus, dev, func, 0x0C) >> 16) as u8;
                    if (hdr & 0x80) == 0 { break; }
                }
            }
        }
    }
}

/// Mark a device as active (called by the driver's init after hardware confirm).
pub fn set_active(driver: DriverKind) {
    let mut table = DEVICE_TABLE.lock();
    let count = table.count;
    for e in table.entries[..count].iter_mut() {
        if e.driver == driver { e.active = true; }
    }
}

/// Returns the first *active* entry for a category, or None.
pub fn get_active(category: DeviceCategory) -> Option<DeviceEntry> {
    let table = DEVICE_TABLE.lock();
    table.as_slice().iter().find(|e| e.category == category && e.active).copied()
}

/// Returns the first entry (active or not) for a category — useful to check
/// if hardware was detected even if the driver hasn't started yet.
pub fn get_detected(category: DeviceCategory) -> Option<DeviceEntry> {
    let table = DEVICE_TABLE.lock();
    table.as_slice().iter().find(|e| e.category == category).copied()
}

/// Iterate all detected devices (for diagnostics / syscall enumeration).
pub fn for_each<F: FnMut(&DeviceEntry)>(mut f: F) {
    let table = DEVICE_TABLE.lock();
    for e in table.as_slice() { f(e); }
}
