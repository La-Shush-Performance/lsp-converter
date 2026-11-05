//! Settings - ported from ACECM's config.py (DEFAULTS, load, save,
//! public_cfg), keeping only what the converter uses.

use serde_json::{json, Map, Value};
use std::path::PathBuf;

use crate::paths;

pub fn defaults() -> Map<String, Value> {
    let v = json!({
        "ui_theme": "",
        // the game's exe, when it isn't where Steam says (detect.rs)
        "game_exe": "",
        // the EVO install folder (content.kspkg); empty = detected
        "game_dir": "",
        "ace_dir": "",
        "client_mods_dir": "",
        "ui_port": crate::edition::PORT,
        // the terms version last agreed ("terms 1", TERMS.md's Version
        // line); only a new terms version asks again (16-paint.js)
        "terms_agreed": "",
        // Convert page settings (convert.rs): ac1_root, out_dir, threads, memory_gb
        "convert": {}
    });
    match v {
        Value::Object(m) => m,
        _ => unreachable!(),
    }
}

pub fn path() -> PathBuf {
    // ⚠ follows ACECM_DATA when that is pinned, like config.config_path
    if std::env::var("ACECM_DATA").is_ok() {
        return paths::data().join("config.json");
    }
    paths::root().join("config.json")
}

/// DEFAULTS, overlaid with the file (unknown keys in the file are kept).
pub fn load() -> Map<String, Value> {
    let mut cfg = defaults();
    if let Some(Value::Object(file)) = crate::jsoncache::read(&path()) {
        for (k, v) in file {
            cfg.insert(k, v);
        }
    }
    cfg
}

pub fn get_str(cfg: &Map<String, Value>, key: &str) -> String {
    cfg.get(key).and_then(|v| v.as_str()).unwrap_or("").trim().to_string()
}

/// Settings for the UI.
pub fn public_cfg(cfg: &Map<String, Value>) -> Value {
    Value::Object(cfg.clone())
}

/// Persist a settings change - only keys we know about.
pub fn save(patch: &Value) -> std::io::Result<Value> {
    let mut cfg = load();
    let known = defaults();
    if let Value::Object(p) = patch {
        for (k, v) in p {
            if known.contains_key(k) {
                cfg.insert(k.clone(), v.clone());
            }
        }
    }
    write(&cfg)?;
    Ok(public_cfg(&cfg))
}

fn write(cfg: &Map<String, Value>) -> std::io::Result<()> {
    let p = path();
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    // write-then-rename: a reader never sees half a file
    let tmp = p.with_extension("json.part");
    std::fs::write(&tmp, serde_json::to_string_pretty(&Value::Object(cfg.clone()))?)?;
    let r = std::fs::rename(&tmp, &p);
    crate::jsoncache::forget(&p);
    r
}
