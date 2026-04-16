/// USB Mass Storage driver skeleton.
///
/// Registers USB mass storage devices in the device registry so the
/// filesystem layer can find them.  Full BOT (Bulk-Only Transport) / SCSI
/// command implementation is a future TODO.

use super::UsbDevice;

/// Called by xHCI when a USB mass storage device is enumerated.
pub fn register(dev: UsbDevice) {
    crate::debugln!("[USB MSC] Registered slot={} vid={:#x} pid={:#x}",
        dev.slot_id, dev.vendor_id, dev.product_id);
    // TODO: configure bulk IN/OUT endpoints, perform INQUIRY, READ CAPACITY,
    // and register a block device entry in the device registry.
    crate::drivers::registry::set_active(crate::drivers::registry::DriverKind::Unknown);
}
