// The FAT partition as a byte stream for fatfs, on top of the VirtIO block device.
// Reads are cached (see cached), writes go straight through to the disk.

use alloc::vec;
use alloc::vec::Vec;

use crate::virtio;

const SECTOR: usize = 512;
// Bounce buffer for DMA, since fatfs passes user buffers that aren't identity-mapped.
const BOUNCE_SECTORS: usize = 128;
const SMALL_SLOTS: u64 = 512;

static mut BOUNCE: [u8; BOUNCE_SECTORS * SECTOR] = [0; BOUNCE_SECTORS * SECTOR];

#[derive(Debug)]
pub struct DiskError;

impl fatfs::IoError for DiskError {
    fn is_interrupted(&self) -> bool {
        false
    }
    fn new_unexpected_eof_error() -> Self {
        DiskError
    }
    fn new_write_zero_error() -> Self {
        DiskError
    }
}

pub struct Disk {
    // Partition start and length, in sectors.
    start: u64,
    sectors: u64,
    pos: u64,
    // Sectors below this are cached; set once the boot sector is known.
    meta_end: u64,
    meta: Vec<Option<[u8; SECTOR]>>,
    // Direct-mapped by sector number; u64::MAX marks an empty slot.
    small: Vec<(u64, [u8; SECTOR])>,
}

impl Disk {
    pub fn new(start: u64, sectors: u64) -> Self {
        let mut d = Disk { start, sectors, pos: 0, meta_end: 0, meta: Vec::new(), small: vec![(u64::MAX, [0; SECTOR]); SMALL_SLOTS as usize] };
        let mut boot = [0u8; SECTOR];
        d.raw_read(0, 1, &mut boot);
        // BPB: reserved sectors, FAT count, and FAT size (FAT32 field, else FAT16 one).
        let reserved = u16::from_le_bytes([boot[14], boot[15]]) as u64;
        let fats = boot[16] as u64;
        let mut fat_size = u16::from_le_bytes([boot[22], boot[23]]) as u64;
        if fat_size == 0 {
            fat_size = u32::from_le_bytes([boot[36], boot[37], boot[38], boot[39]]) as u64;
        }
        d.meta_end = (reserved + fats * fat_size).min(sectors);
        d.meta = vec![None; d.meta_end as usize];
        d
    }

    fn raw_read(&self, sector: u64, count: usize, out: &mut [u8]) {
        unsafe {
            let bounce = &mut *core::ptr::addr_of_mut!(BOUNCE);
            virtio::read_sector(self.start + sector, bounce.as_mut_ptr(), count);
            out[..count * SECTOR].copy_from_slice(&bounce[..count * SECTOR]);
        }
    }

    fn raw_write(&self, sector: u64, count: usize, data: &[u8]) {
        unsafe {
            let bounce = &mut *core::ptr::addr_of_mut!(BOUNCE);
            bounce[..count * SECTOR].copy_from_slice(&data[..count * SECTOR]);
            virtio::write_sector(self.start + sector, bounce.as_ptr(), count);
        }
    }

    // FAT sectors stay cached for good; other partly accessed sectors (mostly
    // directory entries, which fatfs reads 32 bytes at a time) share a small cache.
    fn cached(&mut self, sector: u64) -> &mut [u8; SECTOR] {
        if sector < self.meta_end {
            if self.meta[sector as usize].is_none() {
                let mut s = [0u8; SECTOR];
                self.raw_read(sector, 1, &mut s);
                self.meta[sector as usize] = Some(s);
            }
            return self.meta[sector as usize].as_mut().unwrap();
        }
        let i = (sector % SMALL_SLOTS) as usize;
        if self.small[i].0 != sector {
            let mut s = [0u8; SECTOR];
            self.raw_read(sector, 1, &mut s);
            self.small[i] = (sector, s);
        }
        &mut self.small[i].1
    }

    fn len(&self) -> u64 {
        self.sectors * SECTOR as u64
    }
}

impl fatfs::IoBase for Disk {
    type Error = DiskError;
}

impl fatfs::Read for Disk {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, DiskError> {
        let n = (buf.len() as u64).min(self.len().saturating_sub(self.pos)) as usize;
        let mut done = 0;
        while done < n {
            let sector = (self.pos + done as u64) / SECTOR as u64;
            let skip = ((self.pos + done as u64) % SECTOR as u64) as usize;
            let left = n - done;
            if skip == 0 && left >= SECTOR && sector >= self.meta_end {
                // Whole sectors of file data go straight to the device.
                let count = (left / SECTOR).min(BOUNCE_SECTORS);
                self.raw_read(sector, count, &mut buf[done..done + count * SECTOR]);
                done += count * SECTOR;
                continue;
            }
            let take = (SECTOR - skip).min(left);
            let s = self.cached(sector);
            buf[done..done + take].copy_from_slice(&s[skip..skip + take]);
            done += take;
        }
        self.pos += n as u64;
        Ok(n)
    }
}

impl fatfs::Write for Disk {
    fn write(&mut self, buf: &[u8]) -> Result<usize, DiskError> {
        let n = (buf.len() as u64).min(self.len().saturating_sub(self.pos)) as usize;
        let mut done = 0;
        while done < n {
            let sector = (self.pos + done as u64) / SECTOR as u64;
            let skip = ((self.pos + done as u64) % SECTOR as u64) as usize;
            let left = n - done;
            if skip == 0 && left >= SECTOR && sector >= self.meta_end {
                let count = (left / SECTOR).min(BOUNCE_SECTORS);
                self.raw_write(sector, count, &buf[done..done + count * SECTOR]);
                // Keep any cached copies of these sectors current.
                for i in 0..count as u64 {
                    let slot = &mut self.small[((sector + i) % SMALL_SLOTS) as usize];
                    if slot.0 == sector + i {
                        let at = done + i as usize * SECTOR;
                        slot.1.copy_from_slice(&buf[at..at + SECTOR]);
                    }
                }
                done += count * SECTOR;
                continue;
            }
            let take = (SECTOR - skip).min(left);
            let mut s = *self.cached(sector);
            s[skip..skip + take].copy_from_slice(&buf[done..done + take]);
            self.raw_write(sector, 1, &s);
            *self.cached(sector) = s;
            done += take;
        }
        self.pos += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> Result<(), DiskError> {
        Ok(())
    }
}

impl fatfs::Seek for Disk {
    fn seek(&mut self, pos: fatfs::SeekFrom) -> Result<u64, DiskError> {
        let new = match pos {
            fatfs::SeekFrom::Start(p) => p as i64,
            fatfs::SeekFrom::Current(d) => self.pos as i64 + d,
            fatfs::SeekFrom::End(d) => self.len() as i64 + d,
        };
        if new < 0 || new as u64 > self.len() {
            return Err(DiskError);
        }
        self.pos = new as u64;
        Ok(self.pos)
    }
}
