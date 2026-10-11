// Nerd Font icons and colors for files, plus path helpers.

use crate::types::{palette as p, Color};

pub const FOLDER: char = '\u{f07b}';
pub const FOLDER_OPEN: char = '\u{f07c}';
pub const FILE: char = '\u{f15b}';
pub const FILE_TEXT: char = '\u{f0f6}';
pub const FILE_CODE: char = '\u{f1c9}';
pub const FILE_IMAGE: char = '\u{f1c5}';
pub const FILE_ARCHIVE: char = '\u{f1c6}';
pub const TERMINAL: char = '\u{f120}';
pub const COG: char = '\u{f013}';
pub const CALCULATOR: char = '\u{f1ec}';
pub const PENCIL: char = '\u{f040}';
pub const EYE: char = '\u{f06e}';
pub const SEARCH: char = '\u{f002}';
pub const HOME: char = '\u{f015}';
pub const CHECK: char = '\u{f00c}';
pub const CROSS: char = '\u{f00d}';
pub const INFO: char = '\u{f05a}';
pub const WARNING: char = '\u{f071}';
pub const CLOCK: char = '\u{f017}';
pub const DISK: char = '\u{f0a0}';
pub const CHIP: char = '\u{f2db}';
pub const PICTURE: char = '\u{f03e}';
pub const SAVE: char = '\u{f0c7}';
pub const ROCKET: char = '\u{f135}';
pub const CUBE: char = '\u{f1b2}';
pub const FONT: char = '\u{f031}';
pub const HISTORY: char = '\u{f1da}';
pub const CHEVRON: char = '\u{f054}';
pub const BRANCH: char = '\u{e0a0}';

// Icon and color for a directory entry.
pub fn file_icon(name: &str, is_dir: bool) -> (char, Color) {
    if is_dir {
        return (FOLDER, p::BLUE);
    }
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "wasm" => (CUBE, p::GREEN),
        "elf" | "bin" => (CHIP, p::RED),
        "png" | "jpg" | "jpeg" | "bmp" | "gif" => (FILE_IMAGE, p::MAUVE),
        "ttf" | "otf" => (FONT, p::PEACH),
        "rs" | "c" | "h" | "py" | "js" | "toml" | "json" | "sh" => (FILE_CODE, p::PEACH),
        "txt" | "md" | "log" | "cfg" | "ini" => (FILE_TEXT, p::TEXT),
        "zip" | "tar" | "gz" | "img" => (FILE_ARCHIVE, p::YELLOW),
        _ => (FILE, p::SUBTEXT1),
    }
}

// Join path onto cwd and resolve . and .. segments. Always absolute.
pub fn resolve(cwd: &str, path: &str) -> String {
    let joined = if path.starts_with('/') { path.to_string() } else { format!("{}/{}", cwd, path) };
    let mut parts: Vec<&str> = Vec::new();
    for part in joined.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            p => parts.push(p),
        }
    }
    format!("/{}", parts.join("/"))
}

// The parent of an absolute path ("/" for top-level entries).
pub fn parent(path: &str) -> String {
    match path.trim_end_matches('/').rsplit_once('/') {
        Some((p, _)) if !p.is_empty() => p.to_string(),
        _ => "/".to_string(),
    }
}

pub fn file_name(path: &str) -> &str {
    path.trim_end_matches('/').rsplit('/').next().unwrap_or(path)
}

#[derive(Clone)]
pub struct Entry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

// List a directory: folders first, then files, each sorted by name.
pub fn list_dir(path: &str) -> std::io::Result<Vec<Entry>> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(path)? {
        let Ok(e) = e else { continue };
        let name = e.file_name().to_string_lossy().into_owned();
        let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
        let size = if is_dir {
            0
        } else {
            std::fs::File::open(resolve(path, &name)).and_then(|f| f.metadata()).map(|m| m.len()).unwrap_or(0)
        };
        out.push(Entry { name, is_dir, size });
    }
    out.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(out)
}

// Which app opens a file, by extension.
pub fn opener(name: &str) -> &'static str {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "png" => "/apps/viewer.wasm",
        _ => "/apps/editor.wasm",
    }
}
