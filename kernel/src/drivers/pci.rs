use crate::drivers::port::{inl, outl};
use alloc::vec::Vec;

const PCI_CONFIG_ADDRESS: u32 = 0xCF8;
const PCI_CONFIG_DATA: u32 = 0xCFC;

#[derive(Debug, Copy, Clone)]
pub struct PciDevice {
    pub class: u32,
    pub subclass: u32,
    pub vendor_id: u32,
    pub device_id: u32,
    pub bus: u8,
    pub device: u8,
    pub function: u8,
}

#[derive(Debug, Copy, Clone)]
pub struct PciCapability {
    pub id: u8,
    pub offset: u8,
    pub next: u8,
}

impl PciDevice {
    pub fn read_u32(&self, offset: u32) -> u32 {
        let mut address: u32 = 0x80000000;
        address |= ((self.bus as u32) << 16)
            | ((self.device as u32) << 11)
            | ((self.function as u32) << 8)
            | (offset & 0xFC);
        outl(0xCF8, address);
        let value = inl(0xCFC);

        value
    }

    pub(crate) fn read_u8(&self, offset: u32) -> u8 {
        let val = unsafe { self.read_u32(offset & 0xFC) };
        ((val >> ((offset & 3) * 8)) & 0xFF) as u8
    }
}


pub const CAP_PM: u8 = 0x01;
pub const CAP_MSI: u8 = 0x05;
pub const CAP_VENDOR: u8 = 0x09;
pub const CAP_PCIE: u8 = 0x10;
pub const CAP_MSIX: u8 = 0x11;

fn pci_config_address(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    ((bus as u32) << 16)
        | ((device as u32) << 11)
        | ((function as u32) << 8)
        | ((offset as u32) & 0xFC)
        | 0x80000000
}

fn pci_read(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    let mut address: u32 = 0x80000000;
    address |= ((bus as u32) << 16)
        | ((device as u32) << 11)
        | ((function as u32) << 8)
        | ((offset as u32) & 0xFC);
    outl(0xCF8, address);
    let value = inl(0xCFC);

    value
}

pub fn find_device(v_id: u32, d_id: u32) -> Option<PciDevice> {
    for bus in 0..=255 {
        for device in 0..32 {
            for function in 0..8 {
                let vendor_id = pci_read(bus, device, function, 0) & 0xFFFF;

                if vendor_id != 0xFFFF {
                    let device_id = pci_read(bus, device, function, 2) >> 16;
                    let class_subclass = pci_read(bus, device, function, 8);
                    let class = (class_subclass >> 24) & 0xFF;
                    let subclass: u32 = (class_subclass >> 16) & 0xFF;

                    if vendor_id == v_id && device_id == d_id {
                        return Some(PciDevice {
                            class,
                            subclass,
                            vendor_id,
                            device_id,
                            bus,
                            device,
                            function,
                        });
                    }
                }
            }
        }
    }
    None
}

pub fn find_device_by_class(class_id: u32, subclass_id: u32) -> Option<PciDevice> {
    for bus in 0..=255 {
        for device in 0..32 {
            for function in 0..8 {
                let vendor_id = pci_read(bus, device, function, 0) & 0xFFFF;

                if vendor_id != 0xFFFF {
                    let device_id = pci_read(bus, device, function, 0) >> 16;
                    let class_subclass = pci_read(bus, device, function, 8);
                    let class = (class_subclass >> 24) & 0xFF;
                    let subclass = (class_subclass >> 16) & 0xFF;

                    if class == class_id && subclass == subclass_id {
                        return Some(PciDevice {
                            class,
                            subclass,
                            vendor_id,
                            device_id,
                            bus,
                            device,
                            function,
                        });
                    }
                }
            }
        }
    }
    None
}

pub fn list_devices() {
    for bus in 0..=255 {
        for device in 0..32 {
            for function in 0..8 {
                let vendor_id = pci_read(bus, device, function, 0) & 0xFFFF;

                if vendor_id != 0xFFFF {
                    let _device_id = (pci_read(bus, device, function, 0) >> 16) & 0xFFFF;
                    let class_subclass = pci_read(bus, device, function, 8);
                    let _class_code = (class_subclass >> 24) & 0xFF;
                    let _subclass_code = (class_subclass >> 16) & 0xFF;
                }
            }
        }
    }
}

impl PciDevice {
    pub fn new(vendor_id: u32, device_id: u32) -> Option<PciDevice> {
        find_device(vendor_id, device_id)
    }

    fn get_pci_irq(bus: u8, device: u8, function: u8) -> u8 {
        let value = pci_read(bus, device, function, 0x3C);
        (value & 0xFF) as u8
    }

    pub fn get_bar(&self, bar_index: u8) -> Option<u32> {
        if bar_index > 5 {
            return None;
        }

        let bar_offset = 0x10 + (bar_index as u32 * 4);
        let bar_value = self.read_config_register(bar_offset);


        if bar_value == 0 {
            return None;
        }

        let is_io = (bar_value & 0x1) == 1;

        if is_io {
            Some(bar_value & !0x3)
        } else {
            Some(bar_value & !0xF)
        }
    }

    pub fn read_bar_raw(&self, bar_index: u8) -> u32 {
        if bar_index > 5 { return 0; }
        self.read_config_register(0x10 + (bar_index as u32 * 4))
    }

    pub fn write_bar(&self, bar_index: u8, address: u32) {
        if bar_index > 5 { return; }
        let bar_offset = 0x10 + (bar_index as u32 * 4);


        let current_val = self.read_config_register(bar_offset);
        let is_64bit = (current_val & 0x4) != 0;

        let config_addr = self.generate_config_address(bar_offset as u8);
        outl(0xCF8, config_addr);
        outl(0xCFC, address);

        if is_64bit && bar_index < 5 {
            let upper_offset = bar_offset + 4;
            let upper_config_addr = self.generate_config_address(upper_offset as u8);
            outl(0xCF8, upper_config_addr);
            outl(0xCFC, 0);
        }
    }

    fn read_config_register(&self, offset: u32) -> u32 {
        let address = self.get_config_address(offset);
        let config_addr_port = 0xCF8;
        let config_data_port = 0xCFC;

        outl(config_addr_port, address);
        inl(config_data_port)
    }

    fn get_config_address(&self, offset: u32) -> u32 {
        let enable_bit = 1 << 31;
        let bus = (self.bus as u32) << 16;
        let device = (self.device as u32) << 11;
        let function = (self.function as u32) << 8;
        let aligned_offset = offset & 0xFC;

        enable_bit | bus | device | function | aligned_offset
    }

    pub fn enable_bus_mastering(&self) -> bool {
        let current_command = match self.read_command_register() {
            Some(cmd) => cmd,
            None => return false,
        };

        let new_command = current_command | 0x0004 | 0x0002 | 0x0001;
        self.write_command_register(new_command);

        match self.read_command_register() {
            Some(cmd) => (cmd & 0x0004) != 0,
            None => false,
        }
    }

    fn read_command_register(&self) -> Option<u16> {
        let config_addr = self.generate_config_address(0x04);
        outl(0xCF8, config_addr);
        let value = inl(0xCFC) & 0xFFFF;
        Some(value as u16)
    }

    fn write_command_register(&self, value: u16) {
        let config_addr = self.generate_config_address(0x04);
        outl(0xCF8, config_addr);
        let current = inl(0xCFC);
        let new_value = (current & 0xFFFF0000) | (value as u32);
        outl(0xCFC, new_value);
    }

    fn generate_config_address(&self, register: u8) -> u32 {
        let enable_bit: u32 = 1 << 31;
        let bus: u32 = (self.bus as u32) << 16;
        let device: u32 = (self.device as u32) << 11;
        let function: u32 = (self.function as u32) << 8;
        let register: u32 = (register as u32) & 0xFC;

        enable_bit | bus | device | function | register
    }


    pub fn has_capabilities(&self) -> bool {
        let status = self.read_config_register(0x04) >> 16;
        (status & 0x10) != 0
    }


    pub fn get_capabilities_pointer(&self) -> Option<u8> {
        if !self.has_capabilities() {
            return None;
        }

        let cap_ptr = self.read_config_register(0x34) & 0xFF;
        if cap_ptr == 0 {
            None
        } else {
            Some(cap_ptr as u8)
        }
    }


    pub fn read_capability(&self, offset: u8) -> Option<PciCapability> {
        if offset == 0 {
            return None;
        }

        let value = self.read_config_register(offset as u32);

        Some(PciCapability {
            id: (value & 0xFF) as u8,
            next: ((value >> 8) & 0xFF) as u8,
            offset,
        })
    }


    pub fn find_capability(&self, cap_id: u8) -> Option<PciCapability> {
        let mut offset = self.get_capabilities_pointer()?;

        for _ in 0..48 {
            let cap = self.read_capability(offset)?;

            if cap.id == cap_id {
                return Some(cap);
            }

            if cap.next == 0 {
                break;
            }

            offset = cap.next;
        }

        None
    }

    pub fn list_capabilities(&self) -> Vec<PciCapability> {
        let mut caps = Vec::new();

        if let Some(mut offset) = self.get_capabilities_pointer() {
            for _ in 0..48 {
                if let Some(cap) = self.read_capability(offset) {
                    caps.push(cap);

                    if cap.next == 0 {
                        break;
                    }

                    offset = cap.next;
                } else {
                    break;
                }
            }
        }

        caps
    }

    pub fn read_capability_data(&self, cap_offset: u8, data_offset: u8) -> u32 {
        self.read_config_register((cap_offset + data_offset) as u32)
    }
}