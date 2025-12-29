#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Rsdp {
    signature: [u8; 8],
    checksum: u8,
    oem_id: [u8; 6],
    revision: u8,
    rsdt_address: u32,
}

#[inline(never)]
pub fn get_rsdp() -> Rsdp {
    let mut addr = 0xE0000 as *const u8;
    let end = 0xFFFFF as *const u8;

    unsafe {
        while addr <= end {
            let sig = core::slice::from_raw_parts(addr, 8);
            if sig == b"RSD PTR " {
                let rsdp = (addr as *const Rsdp).read();
                return rsdp;
            }
            addr = addr.add(16);
        }
    }

    Rsdp {
        signature: [0; 8],
        checksum: 0,
        oem_id: [0; 6],
        revision: 0,
        rsdt_address: 0,
    }
}