use super::address::{PhysAddr, VirtAddr};
use super::paging::{phys_to_virt, PageTable, PageTableFlags};
use super::pmm;

pub struct Mapper {
    pml4: &'static mut PageTable,
}

impl Mapper {
    pub unsafe fn new(pml4_phys: PhysAddr) -> Self {
        let virt = phys_to_virt(pml4_phys);
        let pml4 = &mut *(virt.as_mut_ptr() as *mut PageTable);
        Mapper { pml4 }
    }

    pub fn map(&mut self, virt: VirtAddr, phys: PhysAddr, flags: PageTableFlags) -> Result<(), &'static str> {
        let p4_idx = ((virt.as_u64() >> 39) & 0x1FF) as usize;
        let p3_idx = ((virt.as_u64() >> 30) & 0x1FF) as usize;
        let p2_idx = ((virt.as_u64() >> 21) & 0x1FF) as usize;
        let p1_idx = ((virt.as_u64() >> 12) & 0x1FF) as usize;

        let p3 = self.get_next_table(p4_idx)?;
        let p2 = Self::get_next_table_from(p3, p3_idx)?;
        let p1 = Self::get_next_table_from(p2, p2_idx)?;

        let entry = &mut p1[p1_idx];
        if !entry.is_unused() {
            return Err("Page already mapped");
        }

        entry.set_addr(phys, flags | PageTableFlags::PRESENT);
        Ok(())
    }

    fn get_next_table(&mut self, index: usize) -> Result<&'static mut PageTable, &'static str> {
        Self::get_next_table_from(self.pml4, index)
    }

    fn get_next_table_from(table: &mut PageTable, index: usize) -> Result<&'static mut PageTable, &'static str> {
        let entry = &mut table[index];

        if entry.flags().contains(PageTableFlags::HUGE_PAGE) {
            return Err("Huge page encountered while walking");
        }

        if entry.is_unused() {
            let frame = pmm::allocate_frame(0).ok_or("OOM: Failed to allocate page table")?;


            let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE;

            entry.set_addr(PhysAddr::new(frame), flags);

            let virt = phys_to_virt(PhysAddr::new(frame));
            let table = unsafe { &mut *(virt.as_mut_ptr() as *mut PageTable) };
            table.zero();
            Ok(table)
        } else {
            let phys = entry.addr();
            let virt = phys_to_virt(phys);
            Ok(unsafe { &mut *(virt.as_mut_ptr() as *mut PageTable) })
        }
    }
}
