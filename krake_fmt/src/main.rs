// Host tool for the KrakeOS disk image: one FAT32 partition behind an MBR.
//   krake_fmt <image> format
//   krake_fmt <image> pack <host_dir>
//   krake_fmt <image> extract <image_dir> <host_dir>

mod legacy;

use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;

use fatfs::{DefaultTimeProvider, FatType, FileSystem, FormatVolumeOptions, FsOptions, LossyOemCpConverter, StdIoWrapper};
use fscommon::{BufStream, StreamSlice};

const IMAGE_SIZE: u64 = 1024 * 1024 * 1024;
const PART_LBA: u64 = 2048;
// 8 KB is the largest cluster that keeps a 1 GB volume FAT32 (65525 clusters minimum).
const CLUSTER: u32 = 8 * 1024;

type Io = StdIoWrapper<BufStream<StreamSlice<fs::File>>>;
type Fs = FileSystem<Io, DefaultTimeProvider, LossyOemCpConverter>;
type Dir<'a> = fatfs::Dir<'a, Io, DefaultTimeProvider, LossyOemCpConverter>;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let usage = "Usage: krake_fmt <image> format | pack <host_dir> | extract <image_dir> <host_dir>";
    if args.len() < 3 {
        println!("{usage}");
        return;
    }
    let image = &args[1];
    let result = match (args[2].as_str(), args.get(3), args.get(4)) {
        ("format", _, _) => format(image),
        ("pack", Some(dir), _) => pack(image, Path::new(dir)),
        ("extract", Some(from), Some(to)) => extract(image, from, Path::new(to)),
        _ => {
            println!("{usage}");
            return;
        }
    };
    if let Err(e) = result {
        println!("krake_fmt {}: {e}", args[2]);
        std::process::exit(1);
    }
}

fn part_sectors() -> u64 {
    IMAGE_SIZE / 512 - PART_LBA
}

fn format(image: &str) -> io::Result<()> {
    let mut file = OpenOptions::new().read(true).write(true).create(true).truncate(true).open(image)?;
    file.set_len(IMAGE_SIZE)?;

    // One FAT32 LBA partition (type 0x0C) covering the rest of the disk.
    let mut mbr = [0u8; 512];
    mbr[446 + 4] = 0x0C;
    mbr[446 + 8..446 + 12].copy_from_slice(&(PART_LBA as u32).to_le_bytes());
    mbr[446 + 12..446 + 16].copy_from_slice(&(part_sectors() as u32).to_le_bytes());
    mbr[510] = 0x55;
    mbr[511] = 0xAA;
    file.write_all(&mbr)?;

    let part = StreamSlice::new(file, PART_LBA * 512, IMAGE_SIZE)?;
    let opts = FormatVolumeOptions::new()
        .fat_type(FatType::Fat32)
        .bytes_per_cluster(CLUSTER)
        .volume_label(*b"KRAKEOS    ");
    fatfs::format_volume(&mut StdIoWrapper::from(BufStream::new(part)), opts).map_err(to_io)?;
    println!("Formatted {image} as FAT32.");
    Ok(())
}

fn open_fs(image: &str) -> io::Result<Fs> {
    let file = OpenOptions::new().read(true).write(true).open(image)?;
    let part = StreamSlice::new(file, PART_LBA * 512, IMAGE_SIZE)?;
    FileSystem::new(BufStream::new(part), FsOptions::new()).map_err(to_io)
}

fn pack(image: &str, host_dir: &Path) -> io::Result<()> {
    let fs = open_fs(image)?;
    let mut files = 0;
    pack_dir(&fs.root_dir(), host_dir, &mut files)?;
    // Unmounting clears the dirty flag, so the host sees a clean volume.
    fs.unmount().map_err(to_io)?;
    println!("Packed {files} files from {} into {image}.", host_dir.display());
    Ok(())
}

fn pack_dir(dir: &Dir<'_>, host: &Path, files: &mut usize) -> io::Result<()> {
    for entry in fs::read_dir(host)? {
        let entry = entry?;
        let name = entry.file_name().into_string().map_err(|_| io::Error::other("file name is not UTF-8"))?;
        let path = entry.path();
        if path.is_dir() {
            let sub = match dir.open_dir(&name) {
                Ok(d) => d,
                Err(_) => dir.create_dir(&name).map_err(to_io)?,
            };
            pack_dir(&sub, &path, files)?;
        } else {
            let mut out = dir.create_file(&name).map_err(to_io)?;
            out.truncate().map_err(to_io)?;
            out.write_all(&fs::read(&path)?).map_err(to_io)?;
            out.flush().map_err(to_io)?;
            *files += 1;
        }
    }
    Ok(())
}

// Copies a folder out of the image (settings and files the OS saved) so the
// next pack puts it back. The host folder is replaced only if this succeeds.
fn extract(image: &str, from: &str, to: &Path) -> io::Result<()> {
    if !Path::new(image).exists() {
        println!("No image to extract from.");
        return Ok(());
    }
    let tmp = to.with_extension("extracting");
    let _ = fs::remove_dir_all(&tmp);
    let found = if legacy::is_fatsquid(image)? {
        legacy::extract(image, from, &tmp)?
    } else {
        let fs = open_fs(image)?;
        let root = fs.root_dir();
        let from = from.trim_matches(['/', '\\']);
        let dir = if from.is_empty() { Some(root) } else { root.open_dir(from).ok() };
        match dir {
            Some(d) => {
                extract_dir(&d, &tmp)?;
                true
            }
            None => false,
        }
    };
    if !found {
        println!("{from} is not in the image; nothing to extract.");
        return Ok(());
    }
    let _ = fs::remove_dir_all(to);
    fs::rename(&tmp, to)?;
    println!("Extracted {from} to {}", to.display());
    Ok(())
}

fn extract_dir(dir: &Dir<'_>, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in dir.iter() {
        let entry = entry.map_err(to_io)?;
        let name = entry.file_name();
        if name == "." || name == ".." {
            continue;
        }
        if entry.is_dir() {
            extract_dir(&entry.to_dir(), &to.join(&name))?;
        } else {
            let mut data = Vec::new();
            entry.to_file().read_to_end(&mut data).map_err(to_io)?;
            fs::write(to.join(&name), data)?;
        }
    }
    Ok(())
}

fn to_io<E: std::fmt::Debug>(e: E) -> io::Error {
    io::Error::other(format!("{e:?}"))
}

