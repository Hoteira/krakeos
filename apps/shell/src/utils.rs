use alloc::string::String;
use alloc::vec::Vec;

pub fn resolve_path(cwd: &str, path: &str) -> String {
    let trimmed_path = path.trim();
    if trimmed_path.is_empty() { return String::from(cwd); }

    let mut full_path = if trimmed_path.starts_with('/') {
        String::from(trimmed_path)
    } else {
        let mut base = String::from(cwd);
        if !base.ends_with('/') {
            base.push('/');
        }
        base.push_str(trimmed_path);
        base
    };

    let mut parts: Vec<&str> = Vec::new();
    for part in full_path.split('/') {
        if part.is_empty() || part == "." {
            continue;
        } else if part == ".." {
            if !parts.is_empty() {
                parts.pop();
            }
        } else {
            parts.push(part);
        }
    }

    let mut res = String::from("/");
    for (i, p) in parts.iter().enumerate() {
        if i > 0 { res.push('/'); }
        res.push_str(p);
    }
    res
}
