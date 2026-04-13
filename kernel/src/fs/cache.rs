use crate::memory::paging::HHDM_OFFSET;
use crate::memory::pmm;
use crate::sync::YieldMutex;
use alloc::collections::BTreeMap;

pub const PAGE_CACHE_SIZE: usize = 4096;

#[derive(Debug, Clone, Copy)]
pub struct CachePage {
    pub phys_addr: u64,
}

pub struct PageCache {
    pages: BTreeMap<(u8, u64, u32), CachePage>,
}

pub static GLOBAL_PAGE_CACHE: YieldMutex<PageCache> = YieldMutex::new(PageCache {
    pages: BTreeMap::new(),
});

impl PageCache {
    pub fn get(&self, disk_id: u8, inode: u64, block_index: u32) -> Option<u64> {
        self.pages.get(&(disk_id, inode, block_index)).map(|p| p.phys_addr)
    }

    pub fn insert(&mut self, disk_id: u8, inode: u64, block_index: u32, phys_addr: u64) {
        self.pages.insert((disk_id, inode, block_index), CachePage { phys_addr });
    }

    pub fn invalidate(&mut self, disk_id: u8, inode: u64, block_index: u32) {
        if let Some(page) = self.pages.remove(&(disk_id, inode, block_index)) {
            pmm::free_frame(page.phys_addr);
        }
    }
}
