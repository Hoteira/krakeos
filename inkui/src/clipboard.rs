// The clipboard every app shares, kept by the kernel at /dev/clipboard.

use std::fs::File;
use std::io::{Read, Write};

const PATH: &str = "/dev/clipboard";

// The clipboard text (empty if there is none).
pub fn get() -> String {
    let mut data = Vec::new();
    if let Ok(mut f) = File::open(PATH) {
        let _ = f.read_to_end(&mut data);
    }
    String::from_utf8_lossy(&data).into_owned()
}

// Replace the clipboard text. Returns false if the kernel refused it.
pub fn set(text: &str) -> bool {
    // Opening with truncate clears it first.
    match File::create(PATH) {
        Ok(mut f) => f.write_all(text.as_bytes()).is_ok(),
        Err(_) => false,
    }
}
