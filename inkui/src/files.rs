// File operations shared by the terminal and the explorer: recursive
// copy and delete, and short error messages.

use std::fs;
use std::io;

use crate::icons::{self, resolve};

// A short reason for a failed file operation.
pub fn why(e: &io::Error) -> &'static str {
    // WASI errno values, which is what the kernel returns.
    match e.raw_os_error() {
        Some(20) => "already exists",
        Some(28) => "invalid name",
        Some(31) => "is a folder",
        Some(44) => "no such file or folder",
        Some(51) => "disk or folder is full",
        Some(54) => "not a folder",
        Some(55) => "folder is not empty",
        Some(58) => "not supported here",
        _ => match e.kind() {
            io::ErrorKind::NotFound => "no such file or folder",
            io::ErrorKind::AlreadyExists => "already exists",
            _ => "failed",
        },
    }
}

pub fn is_dir(path: &str) -> bool {
    fs::metadata(path).map(|m| m.is_dir()).unwrap_or(false)
}

pub fn exists(path: &str) -> bool {
    fs::metadata(path).is_ok()
}

// Copy a file, or a folder with everything in it.
pub fn copy(from: &str, to: &str) -> io::Result<()> {
    if is_dir(from) {
        if to == from || to.starts_with(&format!("{}/", from.trim_end_matches('/'))) {
            return Err(io::Error::from_raw_os_error(28));
        }
        fs::create_dir(to)?;
        for e in icons::list_dir(from)? {
            copy(&resolve(from, &e.name), &resolve(to, &e.name))?;
        }
        Ok(())
    } else {
        let data = fs::read(from)?;
        fs::write(to, data)
    }
}

// Delete a file, or a folder with everything in it.
pub fn remove_all(path: &str) -> io::Result<()> {
    if is_dir(path) {
        for e in icons::list_dir(path)? {
            remove_all(&resolve(path, &e.name))?;
        }
        fs::remove_dir(path)
    } else {
        fs::remove_file(path)
    }
}

// dir/name, or dir/name copy, dir/name copy 2... if that's taken
// (the number goes before the extension).
pub fn free_name(dir: &str, name: &str) -> String {
    let first = resolve(dir, name);
    if !exists(&first) {
        return first;
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s, format!(".{}", e)),
        _ => (name, String::new()),
    };
    for n in 1.. {
        let suffix = if n == 1 { " copy".to_string() } else { format!(" copy {}", n) };
        let p = resolve(dir, &format!("{}{}{}", stem, suffix, ext));
        if !exists(&p) {
            return p;
        }
    }
    unreachable!()
}
