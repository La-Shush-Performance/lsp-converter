//! Files dragged onto the Convert page: each drop is streamed into its own
//! folder under data\drop (api.rs drop_part), then unpacked by
//! convert::import_dropped. Moved here from ACECM's install.rs, whose mod
//! installer the converter never used.

use std::path::PathBuf;

use serde_json::{json, Value};

use crate::paths;

fn drop_root() -> PathBuf {
    let d = paths::data().join("drop");
    let _ = std::fs::create_dir_all(&d);
    d
}

pub fn drop_staging(did: &str, create: bool) -> Option<PathBuf> {
    let did = did.trim();
    if !(8..=40).contains(&did.len()) || !did.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        return None;
    }
    let d = drop_root().join(did);
    if create {
        let _ = std::fs::create_dir_all(&d);
    }
    if d.is_dir() || create { Some(d) } else { None }
}

/// A dropped file's own name, or "" when it could step outside its folder.
fn safe_base(name: &str) -> String {
    let n = name.replace('\\', "/").rsplit('/').next().unwrap_or("").trim().to_string();
    if n.is_empty() || n == "." || n == ".." || n.chars().any(|c| "/:*?\"<>|\0".contains(c)) {
        return String::new();
    }
    n
}

/// Open the file one dropped file streams into. ⚠ The caller reads EXACTLY
/// Content-Length (an HTTP/1.1 browser never sends EOF).
pub fn drop_part_open(did: &str, filename: &str) -> Result<(PathBuf, std::fs::File), Value> {
    let name = safe_base(filename);
    if name.is_empty() {
        return Err(json!({"ok": false, "error": "bad filename"}));
    }
    let Some(dir) = drop_staging(did, true) else { return Err(json!({"ok": false, "error": "bad drop id"})) };
    let dest = dir.join(&name);
    let f = std::fs::File::create(&dest).map_err(|e| json!({"ok": false, "error": e.to_string()}))?;
    Ok((dest, f))
}

pub const DROP_LIMIT: u64 = 8 * 1024 * 1024 * 1024;

pub fn drop_cleanup(did: &str) {
    if let Some(d) = drop_staging(did, false) {
        let _ = std::fs::remove_dir_all(d);
    }
}
