use crate::memory::paging::HHDM_OFFSET;
use crate::memory::pmm;
use crate::sync::Mutex;
use alloc::collections::BTreeMap;

pub const PAGE_CACHE_SIZE: usize = 4096;

#[derive(Debug, Clone, Copy)]
pub struct CachePage {
    pub phys_addr: u64,
}

pub struct PageCache {
    // Key: (disk_id, inode, block_index)
    pages: BTreeMap<(u8, u64, u32), CachePage>,
}

pub static GLOBAL_PAGE_CACHE: Mutex<PageCache> = Mutex::new(PageCache {
    pages: BTreeMap::new(),
});

impl PageCache {
    pub fn get_or_load(&mut self, disk_id: u8, inode: u64, block_index: u32, loader: impl FnOnce(&mut [u8])) -> u64 {
        if let Some(page) = self.pages.get(&(disk_id, inode, block_index)) {
            return page.phys_addr;
        }

        // Allocate a new frame for the cache
        let phys = pmm::allocate_frame(0).expect("OOM for Page Cache");
        let virt = phys + HHDM_OFFSET;

        let slice = unsafe { core::slice::from_raw_parts_mut(virt as *mut u8, PAGE_CACHE_SIZE) };
        loader(slice);

        self.pages.insert((disk_id, inode, block_index), CachePage { phys_addr: phys });
        phys
    }
}
