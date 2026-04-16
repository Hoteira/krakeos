/// xHCI (eXtensible Host Controller Interface) driver.
///
/// Implements USB 3.x host controller init, slot allocation, device address
/// assignment, descriptor reads, and basic endpoint transfer.
///
/// Reference: Intel xHCI Specification rev 1.2 (May 2019).

use crate::drivers::pci::PciDevice;
use crate::memory::{paging, pmm};
use crate::sync::Mutex;
use core::ptr::{read_volatile, write_volatile};

// ─── MMIO register offsets (from Capability Base) ────────────────────────────

const CAP_CAPLENGTH:  usize = 0x00; // u8  – capability registers length
const CAP_HCIVERSION: usize = 0x02; // u16 – HCI version (BCD)
const CAP_HCSPARAMS1: usize = 0x04; // u32 – structural params 1
const CAP_HCSPARAMS2: usize = 0x08; // u32 – structural params 2
const CAP_HCCPARAMS1: usize = 0x10; // u32 – capability params 1

// Operational register offsets (relative to cap_base + cap_length)
const OP_USBCMD:    usize = 0x00;
const OP_USBSTS:    usize = 0x04;
const OP_PAGESIZE:  usize = 0x08;
const OP_DNCTRL:    usize = 0x14;
const OP_CRCR:      usize = 0x18; // Command Ring Control Register
const OP_DCBAAP:    usize = 0x30; // Device Context Base Address Array Pointer
const OP_CONFIG:    usize = 0x38;

// USBCMD bits
const CMD_RUN:      u32 = 1 << 0;
const CMD_HCRST:    u32 = 1 << 1; // HC Reset
const CMD_INTE:     u32 = 1 << 2; // Interrupter Enable
const CMD_HSEE:     u32 = 1 << 3; // Host System Error Enable

// USBSTS bits
const STS_HCH:      u32 = 1 << 0; // HC Halted
const STS_HSE:      u32 = 1 << 2; // Host System Error
const STS_EINT:     u32 = 1 << 3; // Event Interrupt
const STS_PCD:      u32 = 1 << 4; // Port Change Detect
const STS_CNR:      u32 = 1 << 11; // Controller Not Ready

// TRB types
const TRB_TYPE_NORMAL:           u32 = 1;
const TRB_TYPE_SETUP_STAGE:      u32 = 2;
const TRB_TYPE_DATA_STAGE:       u32 = 3;
const TRB_TYPE_STATUS_STAGE:     u32 = 4;
const TRB_TYPE_LINK:             u32 = 6;
const TRB_TYPE_ENABLE_SLOT:      u32 = 9;
const TRB_TYPE_ADDRESS_DEVICE:   u32 = 11;
const TRB_TYPE_EVALUATE_CONTEXT: u32 = 13;
const TRB_TYPE_TRANSFER_EVENT:   u32 = 32;
const TRB_TYPE_COMMAND_COMPLETE: u32 = 33;
const TRB_TYPE_PORT_STATUS_CHANGE: u32 = 34;

// USB standard request types / requests
const USB_REQ_GET_DESCRIPTOR: u8 = 0x06;
const USB_DT_DEVICE:          u8 = 0x01;
const USB_DT_CONFIG:          u8 = 0x02;
const USB_DT_STRING:          u8 = 0x03;

// Port status / control bits
const PORT_CCS:  u32 = 1 << 0;  // Current Connect Status
const PORT_PED:  u32 = 1 << 1;  // Port Enabled
const PORT_PR:   u32 = 1 << 4;  // Port Reset
const PORT_PLS_MASK: u32 = 0xF << 5;

/// A Transfer Request Block (16 bytes, always aligned to 16 bytes).
#[repr(C, align(16))]
#[derive(Copy, Clone, Default)]
struct Trb {
    param:  u64,
    status: u32,
    ctrl:   u32,  // bits [15:10] = TRB type, bit[0] = cycle bit
}

impl Trb {
    fn new(param: u64, status: u32, trb_type: u32, cycle: bool, flags: u32) -> Self {
        let ctrl = (trb_type << 10) | flags | if cycle { 1 } else { 0 };
        Trb { param, status, ctrl }
    }
    fn trb_type(&self) -> u32 { (self.ctrl >> 10) & 0x3F }
    fn cycle(&self) -> bool   { self.ctrl & 1 != 0 }
}

/// A transfer ring — 256 TRBs + 1 link TRB.
const RING_SIZE: usize = 256;

struct TransferRing {
    trbs:      &'static mut [Trb],
    phys_base: u64,
    enqueue:   usize,
    cycle:     bool,
}

impl TransferRing {
    fn new() -> Option<Self> {
        let pages = (core::mem::size_of::<Trb>() * (RING_SIZE + 1) + 4095) / 4096;
        let phys = pmm::allocate_frames(pages)?;
        let virt = phys + paging::HHDM_OFFSET;
        unsafe {
            core::ptr::write_bytes(virt as *mut u8, 0, pages * 4096);
            let trbs = core::slice::from_raw_parts_mut(virt as *mut Trb, RING_SIZE + 1);
            // Install link TRB pointing back to start with Toggle Cycle bit
            trbs[RING_SIZE] = Trb::new(phys, 0, TRB_TYPE_LINK, true, 1 << 1 /* TC */);
            Some(TransferRing { trbs, phys_base: phys, enqueue: 0, cycle: true })
        }
    }

    fn enqueue_trb(&mut self, mut trb: Trb) {
        trb.ctrl = (trb.ctrl & !1) | if self.cycle { 1 } else { 0 };
        self.trbs[self.enqueue] = trb;
        self.enqueue += 1;
        if self.enqueue >= RING_SIZE {
            // Wrap — update link TRB cycle and toggle
            let lc = &mut self.trbs[RING_SIZE];
            lc.ctrl = (lc.ctrl & !1) | if self.cycle { 1 } else { 0 };
            self.cycle = !self.cycle;
            self.enqueue = 0;
        }
    }
}

/// Per-slot state tracked by the driver.
struct SlotState {
    in_use:     bool,
    context_phys: u64,
    ep0_ring:   Option<TransferRing>,
}

impl SlotState {
    const fn empty() -> Self {
        SlotState { in_use: false, context_phys: 0, ep0_ring: None }
    }
}

const MAX_SLOTS: usize = 64;

pub struct Xhci {
    cap_base: u64,  // virtual address of Capability Registers
    op_base:  u64,  // virtual address of Operational Registers
    rt_base:  u64,  // Runtime Registers
    db_base:  u64,  // Doorbell Array

    max_slots: u8,
    max_ports: u8,

    /// Command ring
    cmd_ring: Option<TransferRing>,
    /// Event ring (Interrupter 0)
    evt_ring_phys: u64,
    evt_trbs: Option<&'static mut [Trb]>,
    evt_dequeue: usize,
    evt_cycle: bool,
    /// Event Ring Segment Table
    erst_phys: u64,

    /// DCBAA (Device Context Base Address Array)
    dcbaa_phys: u64,

    slots: [SlotState; MAX_SLOTS],
}

static XHCI: Mutex<Option<Xhci>> = Mutex::new(None);

// ─── MMIO helpers ─────────────────────────────────────────────────────────────

unsafe fn rd32(base: u64, off: usize) -> u32 {
    read_volatile((base + off as u64) as *const u32)
}
unsafe fn wr32(base: u64, off: usize, val: u32) {
    write_volatile((base + off as u64) as *mut u32, val);
}
unsafe fn rd64(base: u64, off: usize) -> u64 {
    read_volatile((base + off as u64) as *const u64)
}
unsafe fn wr64(base: u64, off: usize, val: u64) {
    write_volatile((base + off as u64) as *mut u64, val);
}

// ─── Public init ──────────────────────────────────────────────────────────────

pub fn init() {
    let Some(dev) = crate::drivers::pci::find_device_by_class(0x0C, 0x03) else {
        crate::debugln!("[xHCI] No USB 3.x host controller found.");
        return;
    };

    // Confirm prog-if = 0x30 (xHCI)
    let class_reg = dev.read_u32(0x08);
    let prog_if = ((class_reg >> 8) & 0xFF) as u8;
    if prog_if != 0x30 {
        crate::debugln!("[xHCI] USB controller prog-if={:#x} is not xHCI — skipping.", prog_if);
        return;
    }

    crate::debugln!("[xHCI] Found xHCI at {:02x}:{:02x}.{}", dev.bus, dev.device, dev.function);

    let Some(bar0) = dev.get_bar(0) else {
        crate::debugln!("[xHCI] BAR0 not readable.");
        return;
    };

    dev.enable_bus_mastering();

    let bar_phys  = (bar0 as u64) & !0xF;
    let cap_virt  = crate::memory::vmm::map_mmio(bar_phys, 0x10000);

    unsafe {
        if let Some(xhci) = init_controller(cap_virt) {
            *XHCI.lock() = Some(xhci);
            crate::drivers::registry::set_active(crate::drivers::registry::DriverKind::XhciUsb);
            crate::debugln!("[xHCI] Controller initialized.");
        }
    }
}

unsafe fn init_controller(cap_virt: u64) -> Option<Xhci> {
    let cap_length = rd32(cap_virt, CAP_CAPLENGTH) as u8 as u64;
    let op_base    = cap_virt + cap_length;
    let hcs1       = rd32(cap_virt, CAP_HCSPARAMS1);
    let hcc1       = rd32(cap_virt, CAP_HCCPARAMS1);

    let max_slots = ((hcs1 >> 0)  & 0xFF) as u8;
    let max_ports = ((hcs1 >> 24) & 0xFF) as u8;

    crate::debugln!("[xHCI] max_slots={} max_ports={} hcc1={:#x}", max_slots, max_ports, hcc1);

    // Runtime register offset from RTSOFF cap register (offset 0x18 in cap space)
    let rtsoff = rd32(cap_virt, 0x18) & !0x1F;
    let rt_base = cap_virt + rtsoff as u64;

    // Doorbell offset from DBOFF cap register (offset 0x14)
    let dboff = rd32(cap_virt, 0x14) & !0x3;
    let db_base = cap_virt + dboff as u64;

    // ── Reset controller ────────────────────────────────────────────────────
    // Stop the controller first
    let cmd = rd32(op_base, OP_USBCMD);
    wr32(op_base, OP_USBCMD, cmd & !CMD_RUN);
    // Wait for HC Halted
    let mut timeout = 100_000u32;
    while rd32(op_base, OP_USBSTS) & STS_HCH == 0 {
        timeout -= 1;
        if timeout == 0 { crate::debugln!("[xHCI] Halt timeout."); return None; }
        core::hint::spin_loop();
    }

    // Issue HC Reset
    wr32(op_base, OP_USBCMD, CMD_HCRST);
    let mut timeout = 500_000u32;
    while rd32(op_base, OP_USBCMD) & CMD_HCRST != 0 {
        timeout -= 1;
        if timeout == 0 { crate::debugln!("[xHCI] Reset timeout."); return None; }
        core::hint::spin_loop();
    }

    // Wait until CNR clears
    let mut timeout = 500_000u32;
    while rd32(op_base, OP_USBSTS) & STS_CNR != 0 {
        timeout -= 1;
        if timeout == 0 { crate::debugln!("[xHCI] CNR timeout."); return None; }
        core::hint::spin_loop();
    }

    // ── DCBAA ───────────────────────────────────────────────────────────────
    let dcbaa_phys = pmm::allocate_frame()?;
    let dcbaa_virt = dcbaa_phys + paging::HHDM_OFFSET;
    core::ptr::write_bytes(dcbaa_virt as *mut u8, 0, 4096);

    wr64(op_base, OP_DCBAAP, dcbaa_phys);
    wr32(op_base, OP_CONFIG, max_slots as u32);

    // ── Command ring ────────────────────────────────────────────────────────
    let mut cmd_ring = TransferRing::new()?;
    // Write CRCR: phys | RCS (Ring Cycle State = 1)
    wr64(op_base, OP_CRCR, cmd_ring.phys_base | 1);

    // ── Event ring (interrupter 0) ───────────────────────────────────────────
    let evt_pages = (core::mem::size_of::<Trb>() * RING_SIZE + 4095) / 4096;
    let evt_phys  = pmm::allocate_frames(evt_pages)?;
    let evt_virt  = evt_phys + paging::HHDM_OFFSET;
    core::ptr::write_bytes(evt_virt as *mut u8, 0, evt_pages * 4096);
    let evt_trbs  = core::slice::from_raw_parts_mut(evt_virt as *mut Trb, RING_SIZE);

    // Event Ring Segment Table Entry: [phys_base u64, size u32, reserved u32]
    let erst_phys = pmm::allocate_frame()?;
    let erst_virt = erst_phys + paging::HHDM_OFFSET;
    core::ptr::write_bytes(erst_virt as *mut u8, 0, 4096);
    let erst = erst_virt as *mut u64;
    *erst.add(0) = evt_phys;           // segment base address
    *erst.add(1) = RING_SIZE as u64;   // segment size (lower 32 bits)

    // Interrupter 0 is at RT_BASE + 0x20 (each interrupter = 0x20 bytes apart)
    let ir0 = rt_base + 0x20;
    wr32(ir0, 0x00, 1);                  // IMAN: Interrupt Enable
    wr32(ir0, 0x04, 0);                  // IMOD
    wr32(ir0, 0x08, 1);                  // ERSTSZ: 1 segment
    wr64(ir0, 0x10, erst_phys);          // ERSTBA
    wr64(ir0, 0x18, evt_phys | 0x8);     // ERDP (dequeue pointer, EHB=1)

    // ── Scratchpad ──────────────────────────────────────────────────────────
    let max_sp = ((rd32(cap_virt, CAP_HCSPARAMS2) >> 27) & 0x1F) as usize;
    if max_sp > 0 {
        let sp_arr_phys = pmm::allocate_frame()?;
        let sp_arr_virt = sp_arr_phys + paging::HHDM_OFFSET;
        core::ptr::write_bytes(sp_arr_virt as *mut u8, 0, 4096);
        for i in 0..max_sp.min(512) {
            if let Some(sp_phys) = pmm::allocate_frame() {
                let sp_virt = sp_phys + paging::HHDM_OFFSET;
                core::ptr::write_bytes(sp_virt as *mut u8, 0, 4096);
                *((sp_arr_virt as *mut u64).add(i)) = sp_phys;
            }
        }
        // DCBAA[0] = scratchpad buffer array
        *(dcbaa_virt as *mut u64) = sp_arr_phys;
    }

    // ── Start controller ────────────────────────────────────────────────────
    wr32(op_base, OP_USBCMD, CMD_RUN | CMD_INTE | CMD_HSEE);
    let mut timeout = 100_000u32;
    while rd32(op_base, OP_USBSTS) & STS_HCH != 0 {
        timeout -= 1;
        if timeout == 0 { crate::debugln!("[xHCI] Start timeout."); return None; }
        core::hint::spin_loop();
    }

    crate::debugln!("[xHCI] Controller running. Scanning ports...");

    let mut xhci = Xhci {
        cap_base: cap_virt,
        op_base,
        rt_base,
        db_base,
        max_slots,
        max_ports,
        cmd_ring: Some(cmd_ring),
        evt_ring_phys: evt_phys,
        evt_trbs: Some(evt_trbs),
        evt_dequeue: 0,
        evt_cycle: true,
        erst_phys,
        dcbaa_phys,
        slots: [const { SlotState::empty() }; MAX_SLOTS],
    };

    // Enumerate ports that have devices attached
    xhci.enumerate_ports();

    Some(xhci)
}

impl Xhci {
    /// Ring the command doorbell (doorbell 0).
    unsafe fn ring_cmd_doorbell(&self) {
        wr32(self.db_base, 0, 0);
    }

    /// Ring a transfer doorbell for a slot's EP0 (target = 1).
    unsafe fn ring_ep0_doorbell(&self, slot: u8) {
        wr32(self.db_base, slot as usize * 4, 1);
    }

    /// Wait for the next command completion event and return (slot_id, completion_code).
    unsafe fn wait_command_completion(&mut self) -> (u8, u8) {
        let trbs = self.evt_trbs.as_mut().unwrap();
        let mut timeout = 10_000_000u32;
        loop {
            let trb = trbs[self.evt_dequeue];
            if trb.cycle() == self.evt_cycle {
                let code    = ((trb.status >> 24) & 0xFF) as u8;
                let slot_id = ((trb.ctrl >> 24) & 0xFF) as u8;
                self.advance_event_ring();
                return (slot_id, code);
            }
            timeout -= 1;
            if timeout == 0 {
                crate::debugln!("[xHCI] wait_command_completion timeout");
                return (0, 0xFF);
            }
            core::hint::spin_loop();
        }
    }

    fn advance_event_ring(&mut self) {
        self.evt_dequeue += 1;
        if self.evt_dequeue >= RING_SIZE {
            self.evt_dequeue = 0;
            self.evt_cycle = !self.evt_cycle;
        }
        // Update ERDP to acknowledge the event
        let ir0 = self.rt_base + 0x20;
        let new_erdp = self.evt_ring_phys
            + (self.evt_dequeue * core::mem::size_of::<Trb>()) as u64
            | 0x8; // EHB
        unsafe { wr64(ir0, 0x18, new_erdp); }
    }

    /// Send an Enable Slot command; returns the allocated slot ID.
    unsafe fn enable_slot(&mut self) -> Option<u8> {
        let trb = Trb::new(0, 0, TRB_TYPE_ENABLE_SLOT, self.cmd_ring.as_ref()?.cycle, 0);
        self.cmd_ring.as_mut()?.enqueue_trb(trb);
        self.ring_cmd_doorbell();
        let (slot_id, code) = self.wait_command_completion();
        if code == 1 && slot_id > 0 { Some(slot_id) } else {
            crate::debugln!("[xHCI] enable_slot failed: code={}", code);
            None
        }
    }

    /// Scan all ports and try to enumerate any connected devices.
    pub fn enumerate_ports(&mut self) {
        for port_idx in 0..self.max_ports as usize {
            let port_base = self.op_base + 0x400 + (port_idx * 0x10) as u64;
            let portsc = unsafe { rd32(port_base, 0) };
            if portsc & PORT_CCS == 0 { continue; }  // nothing connected

            crate::debugln!("[xHCI] Port {} has device (portsc={:#x})", port_idx, portsc);

            // Reset the port to put it in the Enabled state
            unsafe {
                wr32(port_base, 0, portsc | PORT_PR);
                let mut t = 50_000u32;
                loop {
                    let s = rd32(port_base, 0);
                    if s & PORT_PR == 0 { break; }
                    t -= 1; if t == 0 { break; }
                    core::hint::spin_loop();
                }
                let s = rd32(port_base, 0);
                if s & PORT_PED == 0 {
                    crate::debugln!("[xHCI] Port {} reset but not enabled (portsc={:#x})", port_idx, s);
                    continue;
                }
            }

            unsafe { self.address_device(port_idx as u8); }
        }
    }

    /// Allocate a slot, build an Input Context, and send Address Device command.
    unsafe fn address_device(&mut self, port: u8) {
        let slot_id = match self.enable_slot() {
            Some(id) => id,
            None => return,
        };

        // Allocate input context (4KB page; 64-byte contexts)
        let ctx_phys = match pmm::allocate_frame() {
            Some(p) => p,
            None    => return,
        };
        let ctx_virt = ctx_phys + paging::HHDM_OFFSET;
        core::ptr::write_bytes(ctx_virt as *mut u8, 0, 4096);

        // Build endpoint 0 transfer ring
        let ep0_ring = match TransferRing::new() {
            Some(r) => r,
            None    => return,
        };

        // Input Control Context: A0=1, A1=1 (slot + EP0 being added)
        let icc_ptr = ctx_virt as *mut u32;
        *icc_ptr.add(0) = 0;       // Drop context flags
        *icc_ptr.add(1) = 0b11;    // Add context flags: slot + EP0

        // Slot Context (offset 0x20 in input context, or index 8 in u32)
        let slot_ctx = (ctx_virt + 0x20) as *mut u32;
        // Route string=0, speed will be filled from portsc; num endpoints = 1
        let port_base = self.op_base + 0x400 + (port as u64 * 0x10);
        let portsc = rd32(port_base, 0);
        let speed = ((portsc >> 10) & 0xF) as u32;  // Port Speed field
        *slot_ctx.add(0) = (1 << 27) | (speed << 20); // Context entries=1, speed
        *slot_ctx.add(1) = ((port as u32 + 1) << 16); // Root hub port number

        // EP0 Context (offset 0x40 in input context, index 16 in u32)
        let ep0_ctx = (ctx_virt + 0x40) as *mut u32;
        *ep0_ctx.add(0) = 0;
        *ep0_ctx.add(1) = (3 << 3) | (4 << 16) | (8 << 16); // EP type=Control(4); max packet=8
        // Dequeue pointer low + DCS
        *ep0_ctx.add(2) = (ep0_ring.phys_base as u32) | 1;
        *ep0_ctx.add(3) = (ep0_ring.phys_base >> 32) as u32;
        *ep0_ctx.add(4) = 8; // Average TRB length

        // Send Address Device command
        if let Some(ring) = self.cmd_ring.as_mut() {
            let trb = Trb::new(ctx_phys, 0, TRB_TYPE_ADDRESS_DEVICE, ring.cycle,
                               (slot_id as u32) << 24);
            ring.enqueue_trb(trb);
        }
        self.ring_cmd_doorbell();

        let (_, code) = self.wait_command_completion();
        if code != 1 {
            crate::debugln!("[xHCI] address_device failed: slot={} code={}", slot_id, code);
            return;
        }

        // Write DCBAA entry for this slot
        let dcbaa_virt = self.dcbaa_phys + paging::HHDM_OFFSET;
        // Actual output device context follows the input context — allocate separately
        let dev_ctx_phys = match pmm::allocate_frame() {
            Some(p) => p,
            None    => return,
        };
        let dev_ctx_virt = dev_ctx_phys + paging::HHDM_OFFSET;
        core::ptr::write_bytes(dev_ctx_virt as *mut u8, 0, 4096);
        *((dcbaa_virt as *mut u64).add(slot_id as usize)) = dev_ctx_phys;

        crate::debugln!("[xHCI] Slot {} addressed on port {}.", slot_id, port);

        self.slots[slot_id as usize] = SlotState {
            in_use:       true,
            context_phys: ctx_phys,
            ep0_ring:     Some(ep0_ring),
        };

        // Read the device descriptor to identify class/vendor/product
        self.read_device_descriptor(slot_id);
    }

    /// Issue a GET_DESCRIPTOR(Device) control transfer on EP0 and log result.
    unsafe fn read_device_descriptor(&mut self, slot_id: u8) {
        let mut desc = [0u8; 18];
        if self.control_transfer(slot_id, 0x80, USB_REQ_GET_DESCRIPTOR,
                                 (USB_DT_DEVICE as u16) << 8, 0, &mut desc) {
            let vendor  = u16::from_le_bytes([desc[8],  desc[9]]);
            let product = u16::from_le_bytes([desc[10], desc[11]]);
            let class   = desc[4];
            let sub     = desc[5];
            let proto   = desc[6];
            crate::debugln!("[xHCI] Slot {} device: class={:#x}/{:#x}/{:#x} vid={:#x} pid={:#x}",
                slot_id, class, sub, proto, vendor, product);

            // Route to appropriate sub-driver
            use crate::drivers::usb::{UsbClass, UsbDevice, UsbSpeed};
            let dev = UsbDevice {
                slot_id,
                speed:      UsbSpeed::High, // TODO: read from slot context
                class:      UsbClass::from_code(class),
                subclass:   sub,
                protocol:   proto,
                vendor_id:  vendor,
                product_id: product,
            };
            match dev.class {
                UsbClass::Hid         => crate::drivers::usb::hid::register(dev),
                UsbClass::MassStorage => crate::drivers::usb::mass_storage::register(dev),
                _                     => {}
            }
        }
    }

    /// Simple blocking control transfer on EP0 (Setup + [Data In] + Status).
    /// `bmRequestType`, `bRequest`, `wValue`, `wIndex` follow USB spec.
    /// Returns true if successful.
    pub unsafe fn control_transfer(&mut self, slot_id: u8,
                                   bm_req_type: u8, b_request: u8,
                                   w_value: u16, w_index: u16,
                                   buf: &mut [u8]) -> bool {
        let slot = &mut self.slots[slot_id as usize];
        if !slot.in_use { return false; }

        let ring = match slot.ep0_ring.as_mut() { Some(r) => r, None => return false };
        let w_length = buf.len() as u16;

        // Setup stage TRB (always 8 bytes)
        let setup: u64 = (bm_req_type as u64)
            | ((b_request as u64) << 8)
            | ((w_value as u64)   << 16)
            | ((w_index as u64)   << 32)
            | ((w_length as u64)  << 48);
        // TRT = 3 (In Data Stage) for GET_DESCRIPTOR
        let trt = if w_length > 0 { 3u32 } else { 0 };
        ring.enqueue_trb(Trb::new(setup, 8, TRB_TYPE_SETUP_STAGE, ring.cycle, trt << 16 | (1 << 5)/* IDT */));

        if w_length > 0 {
            // Data stage TRB
            let buf_phys = paging::virt_to_phys(buf.as_ptr() as u64);
            ring.enqueue_trb(Trb::new(buf_phys, w_length as u32, TRB_TYPE_DATA_STAGE,
                                      ring.cycle, 1 << 16 /* DIR=In */));
        }

        // Status stage TRB
        let dir = if w_length > 0 { 0 } else { 1u32 << 16 };
        ring.enqueue_trb(Trb::new(0, 0, TRB_TYPE_STATUS_STAGE, ring.cycle, dir | (1 << 5)/* IOC */));

        unsafe { self.ring_ep0_doorbell(slot_id); }

        // Wait for Transfer Event
        let trbs = self.evt_trbs.as_mut().unwrap();
        let mut timeout = 5_000_000u32;
        loop {
            let trb = trbs[self.evt_dequeue];
            if trb.cycle() == self.evt_cycle && trb.trb_type() == TRB_TYPE_TRANSFER_EVENT {
                let code = ((trb.status >> 24) & 0xFF) as u8;
                self.advance_event_ring();
                return code == 1 || code == 13; // Success or Short Packet
            }
            timeout -= 1;
            if timeout == 0 {
                crate::debugln!("[xHCI] control_transfer timeout slot={}", slot_id);
                return false;
            }
            core::hint::spin_loop();
        }
    }
}
