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

pub(crate) fn setup_paging(fb_addr: u64, fb_size: u64) {
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
        core::ptr::write_bytes(pd1, 0, 1);
        core::ptr::write_bytes(pd2, 0, 1);
        core::ptr::write_bytes(pd3, 0, 1);

        
        (*pml4).entries[0] = PDPT_ADDR | 0b111; 

        
        (*pdpt).entries[0] = PD0_ADDR | 0b111;
        (*pdpt).entries[1] = PD1_ADDR | 0b111;
        (*pdpt).entries[2] = PD2_ADDR | 0b111;
        (*pdpt).entries[3] = PD3_ADDR | 0b111;

        
        let fill_pd = |pd: *mut PageTable, start_phys: u64| {
            for i in 0..512 {
                let phys = start_phys + (i as u64 * 0x200000);
                let page_end = phys + 0x200000;
                let fb_end = fb_addr + fb_size;
                
                let mut flags = 0b10000111; 
                
                
                
                (*pd).entries[i] = phys | flags;
            }
        };

        fill_pd(pd0, 0x0000_0000);
        fill_pd(pd1, 0x4000_0000);
        fill_pd(pd2, 0x8000_0000);
        fill_pd(pd3, 0xC000_0000);

        PML4 = *pml4;
    }
}