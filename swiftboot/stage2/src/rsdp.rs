#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Rsdp {
    signature: [u8; 8],
    checksum: u8,
    oem_id: [u8; 6],
    revision: u8,
    rsdt_address: u32,
}

// The RSDP is located and stored by stage 3 (see `stage3::rsdp`); stage 2 only
// needs the struct definition so the `BootInfo` layout matches across stages.