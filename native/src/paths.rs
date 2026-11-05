//! Where LSP Converter keeps things, and the few settings the shell itself
//! needs.
//!
//! Everything lives under %LOCALAPPDATA%\LSP Converter (config.json beside
//! data\, the window's browser data in webview\), never beside the exe.
//! (ACECM used %LOCALAPPDATA%\ACECM; the ACECM_* variables below keep
//! ACECM's names.)

use std::path::PathBuf;

pub fn root() -> PathBuf {
    if let Ok(r) = std::env::var("ACECM_ROOT") {
        return PathBuf::from(r);
    }
    let base = std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    base.join(crate::edition::NAME)
}

pub fn data() -> PathBuf {
    std::env::var("ACECM_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| root().join("data"))
}

/// The repository this shell was built from: a debug build serves the UI
/// and the legal files from it (bundle.rs).
pub fn repo() -> PathBuf {
    if let Ok(r) = std::env::var("ACECM_REPO") {
        return PathBuf::from(r);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// config.json as JSON, or an empty object.
pub fn config() -> serde_json::Value {
    let path = root().join("config.json");
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}))
}

pub struct Listen {
    pub host: String,
    pub port: u16,
}

/// The UI's port, on this PC only (server.rs refuses anyone else too).
pub fn listen() -> Listen {
    let port = config().get("ui_port").and_then(|v| v.as_u64()).unwrap_or(8092) as u16;
    Listen { host: "127.0.0.1".into(), port }
}
