pub mod pmm;
pub mod vmm;
pub mod paging;
pub mod address;
pub mod mapper;
pub mod mmio;
pub mod allocator;
pub mod address_space;
pub mod shm;
pub mod vma;

pub fn init() {
    pmm::init();
    vmm::init();
}
