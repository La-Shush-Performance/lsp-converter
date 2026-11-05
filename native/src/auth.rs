//! Who may call an API route - ported from ACECM's auth.py.
//!
//! ⚠ Our app answers this PC only (server.rs), and this PC needs no token.
//! But a WEB PAGE in the user's browser can reach loopback too, so writes
//! must come from our own origin and requests must use our own host names
//! (DNS rebinding).

use std::net::IpAddr;

use axum::http::HeaderMap;
use serde_json::{json, Value};

pub fn is_loopback(ip: &IpAddr) -> bool {
    ip.to_canonical().is_loopback()
}

/// Refuse requests a web PAGE made without the user meaning to.
pub fn browser_ok(write: bool, peer: &IpAddr, h: &HeaderMap) -> bool {
    let hv = |n: &str| h.get(n).and_then(|v| v.to_str().ok()).unwrap_or("").trim().to_ascii_lowercase();
    let host = hv("host");
    if write {
        if hv("sec-fetch-site") == "cross-site" {
            return false;
        }
        let origin = hv("origin");
        if !origin.is_empty() {
            if origin == "null" {
                return false;
            }
            // netloc of the origin must be exactly our Host
            let netloc = origin.split("://").nth(1).unwrap_or("").split('/').next().unwrap_or("");
            if netloc != host {
                return false;
            }
        }
    }
    if is_loopback(peer) && !host.is_empty() {
        let name = if host.starts_with('[') {
            format!("{}]", host.split(']').next().unwrap_or(""))
        } else {
            host.rsplit_once(':').map(|(n, _)| n.to_string()).unwrap_or(host.clone())
        };
        if !["127.0.0.1", "localhost", "[::1]"].contains(&name.as_str()) {
            return false;
        }
    }
    true
}

pub fn deny_browser() -> (u16, Value) {
    (403, json!({"ok": false,
                 "error": "refused: that request did not come from LSP Converter's own window"}))
}
