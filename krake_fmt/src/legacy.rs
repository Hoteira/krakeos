// Read-only support for images from before the switch to FAT32, so the first
// rebuild still carries settings and home files over.

use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

const BLOCK: u64 = 1024 * 1024;
const DESCRIPTORS: usize = 1024;
const DESC_SIZE: usize = 4096;

struct Desc {
    name: String,
    size: u32,
    is_dir: bool,
    blocks: Vec<u16>,
}

fn part2_offset(file: &mut File) -> io::Result<Option<u64>> {
    let mut mbr = [0u8; 512];
    file.seek(SeekFrom::Start(0))?;
    if file.read_exact(&mut mbr).is_err() || mbr[462 + 4] != 0x83 {
        return Ok(None);
    }
    let lba = u32::from_le_bytes(mbr[470..474].try_into().unwrap()) as u64;
    let mut boot = [0u8; 12];
    file.seek(SeekFrom::Start(lba * 512))?;
    file.read_exact(&mut boot)?;
    Ok((&boot[4..12] == b"FTSQUID1").then_some(lba * 512))
}

pub fn is_fatsquid(image: &str) -> io::Result<bool> {
    Ok(part2_offset(&mut File::open(image)?)?.is_some())
}

fn read_table(file: &mut File, base: u64) -> io::Result<Vec<Desc>> {
    let mut table = vec![0u8; DESCRIPTORS * DESC_SIZE];
    file.seek(SeekFrom::Start(base + BLOCK))?;
    file.read_exact(&mut table)?;
    Ok(table
        .chunks(DESC_SIZE)
        .map(|d| {
            let len = d[..64].iter().position(|&b| b == 0).unwrap_or(64);
            Desc {
                name: String::from_utf8_lossy(&d[..len]).into_owned(),
                size: u32::from_le_bytes(d[64..68].try_into().unwrap()),
                is_dir: d[68] == 1,
                blocks: d[72..72 + 2048].chunks(2).map(|b| u16::from_le_bytes([b[0], b[1]])).collect(),
            }
        })
        .collect())
}

// A directory's blocks hold its children's descriptor indices.
fn children(t: &[Desc], idx: usize) -> Vec<usize> {
    t[idx].blocks.iter().map(|&b| b as usize).filter(|&c| c != 0 && c < DESCRIPTORS && !t[c].name.is_empty()).collect()
}

pub fn extract(image: &str, from: &str, to: &Path) -> io::Result<bool> {
    let mut file = File::open(image)?;
    let Some(base) = part2_offset(&mut file)? else { return Ok(false) };
    let t = read_table(&mut file, base)?;
    let mut idx = 0;
    for part in from.split(['/', '\\']).filter(|p| !p.is_empty()) {
        match children(&t, idx).into_iter().find(|&c| t[c].name == part) {
            Some(c) if t[c].is_dir => idx = c,
            _ => return Ok(false),
        }
    }
    copy_out(&t, idx, to, &mut file, base)?;
    Ok(true)
}

fn copy_out(t: &[Desc], idx: usize, to: &Path, file: &mut File, base: u64) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for c in children(t, idx) {
        let d = &t[c];
        let path = to.join(&d.name);
        if d.is_dir {
            copy_out(t, c, &path, file, base)?;
            continue;
        }
        let mut data = Vec::with_capacity(d.size as usize);
        let mut left = d.size as u64;
        for &b in &d.blocks {
            if left == 0 || b == 0 {
                break;
            }
            let n = left.min(BLOCK);
            let mut chunk = vec![0u8; n as usize];
            file.seek(SeekFrom::Start(base + b as u64 * BLOCK))?;
            file.read_exact(&mut chunk)?;
            data.extend_from_slice(&chunk);
            left -= n;
        }
        fs::write(&path, &data)?;
    }
    Ok(())
}
