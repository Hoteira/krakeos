#[repr(align(4096))]
#[derive(Clone, Copy)]
pub struct PageTable {
    pub entries: [u64; 512],
}

const PML4_ADDR: u64 = 0x20000;
const PDPT_ADDR: u64 = 0x21000;
const PD0_ADDR: u64 = 0x22000;
const PD1_ADDR: u64 = 0x23000;
const PD2_ADDR: u64 = 0x24000;
const PD3_ADDR: u64 = 0x25000;

pub static mut PML4: PageTable = PageTable { entries: [0; 512] };

pub(crate) fn setup_paging() {
    unsafe {
        let pml4 = PML4_ADDR as *mut PageTable;
        let pdpt = PDPT_ADDR as *mut PageTable;
        let pd0 = PD0_ADDR as *mut PageTable;
        let pd1 = PD1_ADDR as *mut PageTable;
        let pd2 = PD2_ADDR as *mut PageTable;
        let pd3 = PD3_ADDR as *mut PageTable;

        core::ptr::write_bytes(pml4, 0, 1);
        core::ptr::write_bytes(pdpt, 0, 1);
        core::ptr::write_bytes(pd0, 0, 1);
        core::ptr::write_bytes(pd0, 1, 1);
        core::ptr::write_bytes(pd0, 2, 1);
        core::ptr::write_bytes(pd0, 3, 1);

        // PML4[0] -> PDPT
        (*pml4).entries[0] = PDPT_ADDR | 0b111; // Present + Writable + User

        // PDPT[0] -> PD
        (*pdpt).entries[0] = PD0_ADDR | 0b111;
        (*pdpt).entries[1] = PD1_ADDR | 0b111;
        (*pdpt).entries[2] = PD2_ADDR | 0b111;
        (*pdpt).entries[3] = PD3_ADDR | 0b111;


        // Identity map first 4GB using 2MB pages (so that it includes the framebuffer)
        for i in 0..512 {
            (*pd0).entries[i] = (i as u64 * 0x20_0000) | 0b10000111; // PS + Present + Writable + User
        }

        for i in 0..512 {
            (*pd1).entries[i] = (0x4000_0000 + i as u64 * 0x20_0000) | 0b10000111;
        }

        for i in 0..512 {
            (*pd2).entries[i] = (0x8000_0000 + i as u64 * 0x20_0000) | 0b10000111;
        }

        for i in 0..512 {
            (*pd3).entries[i] = (0xC000_0000 + i as u64 * 0x20_0000) | 0b10000111;
        }

        PML4 = *pml4;
    }
}