//! VirtIO network driver, built on the `virtio-drivers` crate's `VirtIONet` over a
//! PCI transport. The public API (`init`, `send_packet`, `recv_packet`, `poll_rx`,
//! `read_isr`) is preserved so the net stack and syscalls are unchanged.

use crate::arch::x86_64::io::{inl, outl};
use crate::drivers::virtio_hal::KrakenHal;
use crate::sync::Mutex;
use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::vec::Vec;

use virtio_drivers::device::net::{TxBuffer, VirtIONet};
use virtio_drivers::transport::pci::bus::{ConfigurationAccess, DeviceFunction, PciRoot};
use virtio_drivers::transport::pci::PciTransport;
use virtio_drivers::transport::Transport;

/// Virtqueue depth handed to the crate (power of two, <= device maximum).
const NET_QUEUE_SIZE: usize = 16;
/// Per-buffer length for RX/TX (Ethernet MTU + virtio-net header headroom).
const NET_BUF_LEN: usize = 2048;

/// PCI configuration-space access via legacy `0xCF8/0xCFC` port I/O.
#[derive(Clone)]
struct PortCam;

impl PortCam {
    #[inline]
    fn address(df: DeviceFunction, offset: u8) -> u32 {
        0x8000_0000
            | ((df.bus as u32) << 16)
            | ((df.device as u32) << 11)
            | ((df.function as u32) << 8)
            | ((offset as u32) & 0xFC)
    }
}

impl ConfigurationAccess for PortCam {
    fn read_word(&self, df: DeviceFunction, register_offset: u8) -> u32 {
        let addr = Self::address(df, register_offset);
        unsafe {
            outl(0xCF8, addr);
            inl(0xCFC)
        }
    }

    fn write_word(&mut self, df: DeviceFunction, register_offset: u8, data: u32) {
        let addr = Self::address(df, register_offset);
        unsafe {
            outl(0xCF8, addr);
            outl(0xCFC, data);
        }
    }

    unsafe fn unsafe_clone(&self) -> Self {
        PortCam
    }
}

type NetDev = VirtIONet<KrakenHal, PciTransport, NET_QUEUE_SIZE>;

pub struct VirtioNetDevice {
    dev: NetDev,
    #[allow(dead_code)]
    mac: [u8; 6],
    /// Raw packets delivered to userspace via `recv_packet` (the kernel net stack is
    /// fed separately through `crate::net::on_receive` in `poll_rx`).
    rx_packet_queue: VecDeque<Vec<u8>>,
}

pub static NET_DEVICE: Mutex<Option<VirtioNetDevice>> = Mutex::new(None);

/// Program any memory BARs that firmware left unset (swiftboot does no PCI BAR
/// assignment), so the crate's transport can read valid BAR addresses.
fn program_bars(pci: &crate::drivers::pci::PciDevice) {
    let mut bar = 0u8;
    while bar < 6 {
        let raw = pci.read_bar_raw(bar);
        let is_io = raw & 1 == 1;
        let is_64 = (raw & 0x6) == 0x4;
        if !is_io && raw != 0 {
            let cur = pci.get_bar(bar).unwrap_or(0);
            if cur < 0xC000_0000 {
                let addr = crate::drivers::pci::allocate_bar_address(0x100_0000); // 16 MiB
                pci.write_bar(bar, addr);
                crate::debugln!("VirtIO Net: remapped BAR {} -> {:#x}", bar, addr);
            }
        }
        // A 64-bit BAR consumes the next index for its high half.
        bar += if is_64 { 2 } else { 1 };
    }
}

pub fn init() -> Result<(), String> {
    let pci = crate::drivers::pci::find_device(0x1AF4, 0x1041) // modern virtio-net
        .or_else(|| crate::drivers::pci::find_device(0x1AF4, 0x1000)); // transitional
    let pci = match pci {
        Some(d) => d,
        None => return Err(String::from("VirtIO Net: device not found")),
    };
    crate::debugln!(
        "VirtIO Net: found at {}:{}.{}",
        pci.bus,
        pci.device,
        pci.function
    );

    pci.enable_bus_mastering();
    program_bars(&pci);

    let df = DeviceFunction {
        bus: pci.bus,
        device: pci.device,
        function: pci.function,
    };
    let mut root = PciRoot::new(PortCam);
    let transport = PciTransport::new::<KrakenHal, PortCam>(&mut root, df)
        .map_err(|e| {
            crate::debugln!("VirtIO Net: transport init failed: {:?}", e);
            String::from("VirtIO Net: transport init failed")
        })?;

    let dev = NetDev::new(transport, NET_BUF_LEN).map_err(|e| {
        crate::debugln!("VirtIO Net: device init failed: {:?}", e);
        String::from("VirtIO Net: device init failed")
    })?;

    let mac = dev.mac_address();
    crate::println!(
        "VirtIO Net: MAC {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        mac[0],
        mac[1],
        mac[2],
        mac[3],
        mac[4],
        mac[5]
    );

    *NET_DEVICE.lock() = Some(VirtioNetDevice {
        dev,
        mac,
        rx_packet_queue: VecDeque::new(),
    });
    crate::debugln!("VirtIO Net: initialized.");
    Ok(())
}

/// Drain the RX virtqueue: feed each packet to the kernel net stack and queue it for
/// `recv_packet`, then service the loopback queue. Returns 0 (kept for ABI parity).
pub fn poll_rx() {
    if let Some(nd) = NET_DEVICE.lock().as_mut() {
        while nd.dev.can_recv() {
            match nd.dev.receive() {
                Ok(rx) => {
                    let pkt = rx.packet().to_vec();
                    crate::net::on_receive(&pkt);
                    let _ = nd.dev.recycle_rx_buffer(rx);
                    nd.rx_packet_queue.push_back(pkt);
                }
                Err(_) => break,
            }
        }
    }
    crate::net::poll_loopback();
}

pub fn recv_packet() -> Option<Vec<u8>> {
    poll_rx();
    let mut guard = NET_DEVICE.lock();
    guard.as_mut().and_then(|nd| nd.rx_packet_queue.pop_front())
}

/// Transmit a raw Ethernet frame. Returns 0 on success, nonzero error code otherwise.
pub fn send_packet(data: &[u8]) -> usize {
    let mut guard = NET_DEVICE.lock();
    let nd = match guard.as_mut() {
        Some(d) => d,
        None => return 1,
    };
    let tx = TxBuffer::from(data);
    match nd.dev.send(tx) {
        Ok(()) => 0,
        Err(_) => 2,
    }
}

/// Acknowledge a device interrupt. Returns 1 if a queue interrupt is pending (so the
/// handler should `poll_rx`), 0 otherwise. Mirrors the old ISR-status read.
pub fn read_isr() -> u8 {
    let mut guard = NET_DEVICE.lock();
    match guard.as_mut() {
        // ack_interrupt() takes &mut self, so it can't go in a match guard.
        Some(nd) => {
            if nd.dev.ack_interrupt() {
                1
            } else {
                0
            }
        }
        None => 0,
    }
}
