//! Where converted cars and tracks go: the game's mods folder (from ACECM's
//! install.py mods_dir).

use std::path::Path;

use crate::{config, detect};

/// The setting, else detected, else where it will be (Saved Games\ACE\mods).
pub fn client_mods_dir() -> String {
    let cfg = config::load();
    let s = config::get_str(&cfg, "client_mods_dir");
    if !s.is_empty() {
        return s;
    }
    let d = detect::find("client_mods", false);
    if !d.is_empty() {
        return d;
    }
    Path::new(&detect::saved_games()).join("ACE").join("mods").to_string_lossy().into_owned()
}
