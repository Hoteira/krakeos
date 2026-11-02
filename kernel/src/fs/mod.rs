// The disk file system: FAT32 through fatfs, on the first FAT partition.
// Kernel file handles are slots in a node table. Opening a path that is already
// open returns its handle, since fatfs can't keep two writers on one file in sync.

use alloc::string::String;
use alloc::vec::Vec;

use fatfs::{Read, Seek, SeekFrom, Write};

pub mod disk;

// Handles are 1..1024; device fds sit at 100..=127 and above 1024.
pub const MAX_DESCRIPTORS: usize = 1024;

// WASI errno values, returned as-is by the path operations below.
pub const E_EXIST: usize = 20;
pub const E_INVAL: usize = 28;
pub const E_IO: usize = 29;
pub const E_ISDIR: usize = 31;
pub const E_NOENT: usize = 44;
pub const E_NOSPC: usize = 51;
pub const E_NOTDIR: usize = 54;
pub const E_NOTEMPTY: usize = 55;
pub const E_NOTSUP: usize = 58;

type Fs = fatfs::FileSystem<disk::Disk, Clock, fatfs::LossyOemCpConverter>;
type File = fatfs::File<'static, disk::Disk, Clock, fatfs::LossyOemCpConverter>;
type Dir = fatfs::Dir<'static, disk::Disk, Clock, fatfs::LossyOemCpConverter>;
type Entry = fatfs::DirEntry<'static, disk::Disk, Clock, fatfs::LossyOemCpConverter>;

// File times from the RTC (UTC).
#[derive(Debug)]
pub struct Clock;

impl fatfs::TimeProvider for Clock {
    fn get_current_date(&self) -> fatfs::Date {
        self.get_current_date_time().date
    }

    fn get_current_date_time(&self) -> fatfs::DateTime {
        let secs = crate::rtc::get_time_ns() / 1_000_000_000;
        let (days, rem) = ((secs / 86400) as i64, secs % 86400);
        // Civil date from days since 1970 (Howard Hinnant's algorithm).
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = (doy - (153 * mp + 2) / 5 + 1) as u16;
        let month = if mp < 10 { mp + 3 } else { mp - 9 } as u16;
        let year = (yoe + era * 400 + (month <= 2) as i64).clamp(1980, 2107) as u16;
        let time = fatfs::Time::new((rem / 3600) as u16, (rem / 60 % 60) as u16, (rem % 60) as u16, 0);
        fatfs::DateTime::new(fatfs::Date::new(year, month, day), time)
    }
}

struct Node {
    // Relative to the root, without leading or trailing '/'. Empty is the root.
    path: String,
    is_dir: bool,
    // Kept here because asking fatfs walks the whole cluster chain.
    size: usize,
    // Opened on first use and dropped (which flushes it) before a rename or remove.
    file: Option<File>,
    // Threads holding the handle open, once per open.
    owners: Vec<usize>,
}

static mut FS: Option<&'static Fs> = None;
static mut NODES: Vec<Option<Node>> = Vec::new();

fn fs() -> Option<&'static Fs> {
    unsafe { *core::ptr::addr_of!(FS) }
}

fn nodes() -> &'static mut Vec<Option<Node>> {
    unsafe { &mut *core::ptr::addr_of_mut!(NODES) }
}

fn root() -> Option<Dir> {
    Some(fs()?.root_dir())
}

pub fn mount() -> bool {
    crate::println!("Mounting FAT32...");
    let mut mbr = [0u8; 512];
    crate::virtio::read_sector(0, mbr.as_mut_ptr(), 1);
    if mbr[510] != 0x55 || mbr[511] != 0xAA {
        crate::println!("fs: no MBR on the disk.");
        return false;
    }
    // First FAT32 partition (type 0x0B or 0x0C).
    let Some(entry) = (0..4).map(|i| &mbr[446 + i * 16..462 + i * 16]).find(|e| e[4] == 0x0B || e[4] == 0x0C) else {
        crate::println!("fs: no FAT32 partition. Rebuild fs.img with krake_fmt.");
        return false;
    };
    let start = u32::from_le_bytes([entry[8], entry[9], entry[10], entry[11]]) as u64;
    let sectors = u32::from_le_bytes([entry[12], entry[13], entry[14], entry[15]]) as u64;
    let opts = fatfs::FsOptions::new().time_provider(Clock).update_accessed_date(false);
    match fatfs::FileSystem::new(disk::Disk::new(start, sectors), opts) {
        Ok(fs) => unsafe {
            FS = Some(alloc::boxed::Box::leak(alloc::boxed::Box::new(fs)));
            let nodes = nodes();
            nodes.resize_with(MAX_DESCRIPTORS, || None);
            nodes[0] = Some(Node { path: String::new(), is_dir: true, size: 0, file: None, owners: Vec::new() });
        },
        Err(e) => {
            crate::println!("fs: mount failed: {:?}", e);
            return false;
        }
    }
    crate::println!("FAT32 mounted from sector {}.", start);
    true
}

fn errno<E>(e: fatfs::Error<E>) -> usize {
    match e {
        fatfs::Error::NotFound => E_NOENT,
        fatfs::Error::AlreadyExists => E_EXIST,
        fatfs::Error::DirectoryIsNotEmpty => E_NOTEMPTY,
        fatfs::Error::NotEnoughSpace => E_NOSPC,
        fatfs::Error::InvalidInput | fatfs::Error::InvalidFileNameLength | fatfs::Error::UnsupportedFileNameCharacter => E_INVAL,
        _ => E_IO,
    }
}

fn trim_path(name: &str) -> &str {
    name.trim_start_matches('/').trim_end_matches('/')
}

fn same_path(a: &str, b: &str) -> bool {
    // FAT names are case-insensitive.
    a.eq_ignore_ascii_case(b)
}

fn under(path: &str, dir: &str) -> bool {
    path.len() > dir.len() && same_path(&path[..dir.len()], dir) && path.as_bytes()[dir.len()] == b'/'
}

fn split(path: &str) -> (&str, &str) {
    match path.rfind('/') {
        Some(i) => (&path[..i], &path[i + 1..]),
        None => ("", path),
    }
}

fn entry(path: &str) -> Option<Entry> {
    let (parent, base) = split(path);
    let root = root()?;
    let dir = if parent.is_empty() { root } else { root.open_dir(parent).ok()? };
    dir.iter().filter_map(|e| e.ok()).find(|e| e.file_name().eq_ignore_ascii_case(base))
}

// Checks a new entry's name and that its parent is a directory.
fn check_new(path: &str) -> Result<(), usize> {
    let (parent, base) = split(path);
    if base.is_empty() || base.len() > 255 || base == "." || base == ".." {
        return Err(E_INVAL);
    }
    if !parent.is_empty() {
        match entry(parent) {
            Some(e) if e.is_dir() => {}
            Some(_) => return Err(E_NOTDIR),
            None => return Err(E_NOENT),
        }
    }
    Ok(())
}

fn alloc_node(path: &str, is_dir: bool, size: usize, file: Option<File>) -> Option<usize> {
    let nodes = nodes();
    let free = (1..MAX_DESCRIPTORS).filter(|&i| !reserved_fd(i));
    let slot = free.clone().find(|&i| nodes[i].is_none())
        // Table full: reuse a node nobody holds open (e.g. one the kernel read itself).
        .or_else(|| free.clone().find(|&i| nodes[i].as_ref().is_some_and(|n| n.owners.is_empty())))?;
    nodes[slot] = Some(Node { path: String::from(path), is_dir, size, file, owners: Vec::new() });
    Some(slot)
}

fn node(idx: usize) -> Option<&'static mut Node> {
    nodes().get_mut(idx)?.as_mut()
}

// The node's open file, reopened by path if it was dropped.
fn node_file(idx: usize) -> Option<&'static mut File> {
    let n = node(idx)?;
    if n.is_dir {
        return None;
    }
    if n.file.is_none() {
        n.file = Some(root()?.open_file(&n.path).ok()?);
    }
    n.file.as_mut()
}

// Handles skip stdio, the WASI preopen and the device fds (100..=127).
pub fn reserved_fd(i: usize) -> bool {
    i <= 3 || (100..=127).contains(&i)
}

pub fn find_file(name: &str) -> Option<usize> {
    let path = trim_path(name);
    if path.is_empty() {
        return Some(0);
    }
    if let Some(i) = nodes().iter().position(|n| n.as_ref().is_some_and(|n| same_path(&n.path, path))) {
        return Some(i);
    }
    let e = entry(path)?;
    let file = if e.is_dir() { None } else { Some(e.to_file()) };
    alloc_node(path, e.is_dir(), e.len() as usize, file)
}

// The parent must exist.
pub fn create_file(name: &str) -> Option<usize> {
    let path = trim_path(name);
    if entry(path).is_some() || check_new(path).is_err() {
        return None;
    }
    let file = root()?.create_file(path).ok()?;
    alloc_node(path, false, 0, Some(file))
}

// Called for each successful open by a thread, and undone by close or its exit.
pub fn retain(idx: usize, tid: usize) {
    if let Some(n) = node(idx) {
        n.owners.push(tid);
    }
}

pub fn release(idx: usize, tid: usize) {
    if idx == 0 {
        return;
    }
    let Some(n) = node(idx) else { return };
    if let Some(i) = n.owners.iter().position(|&t| t == tid) {
        n.owners.swap_remove(i);
    }
    if n.owners.is_empty() {
        nodes()[idx] = None;
    }
}

pub fn thread_exited(tid: usize) {
    for idx in 1..MAX_DESCRIPTORS {
        while node(idx).is_some_and(|n| n.owners.contains(&tid)) {
            release(idx, tid);
        }
    }
}

pub fn get_file_size(idx: usize) -> usize {
    node(idx).map_or(0, |n| if n.is_dir { 0 } else { n.size })
}

pub fn truncate_file(idx: usize, length: usize) {
    let size = get_file_size(idx);
    let Some(f) = node_file(idx) else { return };
    if length < size && f.seek(SeekFrom::Start(length as u64)).is_ok() && f.truncate().is_ok() {
        let _ = f.flush();
        node(idx).unwrap().size = length;
    }
}

pub fn read_file(idx: usize, offset: usize, buf: &mut [u8]) -> usize {
    let size = get_file_size(idx);
    let Some(f) = node_file(idx) else { return 0 };
    // Sequential reads skip the seek, which would walk the cluster chain from the start.
    if offset >= size || f.seek(SeekFrom::Start(offset as u64)).is_err() {
        return 0;
    }
    // fatfs stops at cluster ends, so keep going until buf is full or the file ends.
    let mut done = 0;
    while done < buf.len() {
        match f.read(&mut buf[done..]) {
            Ok(0) | Err(_) => break,
            Ok(n) => done += n,
        }
    }
    done
}

pub fn write_file(idx: usize, offset: usize, buf: &[u8]) -> usize {
    let mut size = get_file_size(idx);
    let Some(f) = node_file(idx) else { return 0 };
    // Writing past the end fills the gap with zeros.
    if size < offset && f.seek(SeekFrom::Start(size as u64)).is_err() {
        return 0;
    }
    while size < offset {
        let zeros = [0u8; 512];
        match f.write(&zeros[..(offset - size).min(512)]) {
            Ok(0) | Err(_) => return 0,
            Ok(n) => size += n,
        }
    }
    if f.seek(SeekFrom::Start(offset as u64)).is_err() {
        return 0;
    }
    let mut done = 0;
    while done < buf.len() {
        match f.write(&buf[done..]) {
            Ok(0) | Err(_) => break,
            Ok(n) => done += n,
        }
    }
    // Writes the new size to the directory entry.
    let _ = f.flush();
    let n = node(idx).unwrap();
    n.size = n.size.max(offset + done);
    done
}

// For WASI path_filestat_get.
pub fn stat(name: &str) -> Option<(usize, bool)> {
    let path = trim_path(name);
    if path.is_empty() {
        return Some((0, true));
    }
    // An open file's size may not be in its directory entry yet.
    if let Some(i) = nodes().iter().position(|n| n.as_ref().is_some_and(|n| !n.is_dir && same_path(&n.path, path))) {
        return Some((get_file_size(i), false));
    }
    let e = entry(path)?;
    Some((if e.is_dir() { 0 } else { e.len() as usize }, e.is_dir()))
}

pub fn make_dir(name: &str) -> Result<(), usize> {
    let path = trim_path(name);
    check_new(path)?;
    if entry(path).is_some() {
        return Err(E_EXIST);
    }
    root().ok_or(E_IO)?.create_dir(path).map(|_| ()).map_err(errno)
}

// Drops open files at or under path, flushing them, so fatfs can move or delete the entries.
fn close_under(path: &str) {
    for n in nodes().iter_mut().flatten() {
        if same_path(&n.path, path) || under(&n.path, path) {
            n.file = None;
        }
    }
}

pub fn remove(name: &str, dir: bool) -> Result<(), usize> {
    let path = trim_path(name);
    if path.is_empty() {
        return Err(E_INVAL);
    }
    let e = entry(path).ok_or(E_NOENT)?;
    if e.is_dir() != dir {
        return Err(if dir { E_NOTDIR } else { E_ISDIR });
    }
    close_under(path);
    root().ok_or(E_IO)?.remove(path).map_err(errno)?;
    // Handles still open on it now read as empty.
    for n in nodes().iter_mut().flatten() {
        if same_path(&n.path, path) {
            n.path = String::from("\0removed");
        }
    }
    Ok(())
}

// An existing file at the target is replaced; an existing directory is not.
pub fn rename(from: &str, to: &str) -> Result<(), usize> {
    let (a, b) = (trim_path(from), trim_path(to));
    if a.is_empty() {
        return Err(E_INVAL);
    }
    entry(a).ok_or(E_NOENT)?;
    check_new(b)?;
    if a == b {
        return Ok(());
    }
    // A directory can't move inside itself.
    if under(b, a) {
        return Err(E_INVAL);
    }
    // A case-only rename is the same entry, so there is nothing to replace.
    if !same_path(a, b) {
        if let Some(existing) = entry(b) {
            if existing.is_dir() {
                return Err(E_EXIST);
            }
            remove(b, false)?;
        }
    }
    close_under(a);
    let root = root().ok_or(E_IO)?;
    root.rename(a, &root, b).map_err(errno)?;
    for n in nodes().iter_mut().flatten() {
        if same_path(&n.path, a) {
            n.path = String::from(b);
        } else if under(&n.path, a) {
            n.path = alloc::format!("{}{}", b, &n.path[a.len()..]);
        }
    }
    Ok(())
}

// A stable nonzero inode number for readdir, so listing doesn't use up handles.
fn ino(dir: &str, name: &str) -> u32 {
    let mut h: u32 = 2_166_136_261;
    for b in dir.bytes().chain(core::iter::once(b'/')).chain(name.bytes()) {
        h = (h ^ b.to_ascii_lowercase() as u32).wrapping_mul(16_777_619);
    }
    h.max(1)
}

// Writes [next_cookie: u32][size: u32][ino: u32][is_dir: u8][name_len: u8][name...].
// Returns its length, 0 at the end, or None if not a directory or buf is too small.
pub fn read_dir_entry(idx: usize, cookie: usize, buf: &mut [u8]) -> Option<usize> {
    let n = node(idx)?;
    if !n.is_dir {
        return None;
    }
    let root = root()?;
    let dir = if n.path.is_empty() { root } else { root.open_dir(&n.path).ok()? };
    let entries = dir.iter().filter_map(|e| e.ok()).filter(|e| {
        let name = e.file_name();
        name != "." && name != ".."
    });
    let Some(e) = entries.skip(cookie).next() else { return Some(0) };
    let name = e.file_name();
    let name_len = name.len().min(255);
    let total = 14 + name_len;
    if buf.len() < total {
        return None;
    }
    let size = if e.is_dir() { 0 } else { e.len() as u32 };
    buf[0..4].copy_from_slice(&((cookie + 1) as u32).to_le_bytes());
    buf[4..8].copy_from_slice(&size.to_le_bytes());
    buf[8..12].copy_from_slice(&ino(&n.path, &name).to_le_bytes());
    buf[12] = e.is_dir() as u8;
    buf[13] = name_len as u8;
    buf[14..total].copy_from_slice(&name.as_bytes()[..name_len]);
    Some(total)
}

