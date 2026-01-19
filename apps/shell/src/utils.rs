use alloc::string::String;
use alloc::vec::Vec;

pub fn resolve_path(cwd: &str, path: &str) -> String {
    let trimmed_path = path.trim();
    if trimmed_path.is_empty() { return String::from(cwd); }

    let mut parts = Vec::new();

    if !trimmed_path.starts_with('@') {
        if trimmed_path.starts_with('/') {
            if let Some(idx) = cwd.find('/') {
                parts.push(&cwd[..idx]);
            } else {
                parts.push(cwd);
            }
        } else {
            for part in cwd.split('/') {
                if !part.is_empty() {
                    parts.push(part);
                }
            }
        }
    }

    for part in trimmed_path.split('/') {
        if part.is_empty() || part == "." {
            continue;
        } else if part == ".." {
            if parts.len() > 1 {
                parts.pop();
            }
        } else {
            parts.push(part);
        }
    }

    if parts.is_empty() {
        return String::from("@0xE0");
    }

    let mut res = String::new();
    for (i, p) in parts.iter().enumerate() {
        if i > 0 { res.push('/'); }
        res.push_str(p);
    }
    res
}
