//! Find the game and its Saved Games folder (ACE, with the mods folder) -
//! ported from ACECM's detect.py. Ask the systems that already know
//! (Steam's registry key, libraryfolders.vdf, appmanifest_<id>.acf, the
//! Windows known-folder API) instead of guessing or scanning disks.
//!
//! Results are cached in <data>/paths.json and re-validated on every use, so
//! a moved install heals.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde_json::{json, Map, Value};

use crate::{config, paths};

pub const APPID_GAME: &str = "3058630";

fn norm(p: &Path) -> String {
    p.to_string_lossy().replace('/', "\\")
}

// ------------------------------------------------------------------ steam --
#[cfg(windows)]
pub fn steam_root() -> String {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use winreg::RegKey;
    for (hive, key, name) in [
        (HKEY_CURRENT_USER, r"Software\Valve\Steam", "SteamPath"),
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Valve\Steam", "InstallPath"),
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\Valve\Steam", "InstallPath"),
    ] {
        if let Ok(k) = RegKey::predef(hive).open_subkey(key) {
            if let Ok(p) = k.get_value::<String, _>(name) {
                if !p.is_empty() && Path::new(&p).is_dir() {
                    return norm(Path::new(&p));
                }
            }
        }
    }
    String::new()
}

#[cfg(not(windows))]
pub fn steam_root() -> String {
    String::new() // Linux: proton.steam_root - ported with the Linux phase
}

pub fn steam_libraries() -> Vec<String> {
    let root = steam_root();
    if root.is_empty() {
        return vec![];
    }
    let mut libs: Vec<String> = vec![];
    let mut add = |p: &str| {
        let n = norm(Path::new(p));
        if Path::new(&n).is_dir() && !libs.iter().any(|x| x.eq_ignore_ascii_case(&n)) {
            libs.push(n);
        }
    };
    add(&root);
    let vdf = Path::new(&root).join("steamapps").join("libraryfolders.vdf");
    if let Ok(text) = std::fs::read(&vdf) {
        let text = String::from_utf8_lossy(&text);
        // Valve KeyValues: every library has a "path" entry, that is all we need
        for v in kv_values(&text, "path") {
            add(&v.replace("\\\\", "\\"));
        }
    }
    libs
}

/// Every value of `"key" "value"` pairs in a Valve KeyValues text.
fn kv_values(text: &str, key: &str) -> Vec<String> {
    let needle = format!("\"{key}\"");
    let mut out = vec![];
    let mut rest = text;
    while let Some(i) = rest.find(&needle) {
        rest = &rest[i + needle.len()..];
        let t = rest.trim_start();
        if let Some(s) = t.strip_prefix('"') {
            if let Some(end) = s.find('"') {
                out.push(s[..end].to_string());
            }
        }
    }
    out
}

pub fn steam_app_dir(appid: &str) -> String {
    for lib in steam_libraries() {
        let acf = Path::new(&lib).join("steamapps").join(format!("appmanifest_{appid}.acf"));
        let Ok(text) = std::fs::read(&acf) else { continue };
        let text = String::from_utf8_lossy(&text);
        let Some(dir) = kv_values(&text, "installdir").into_iter().next() else { continue };
        let p = Path::new(&lib).join("steamapps").join("common").join(dir);
        if p.is_dir() {
            return norm(&p);
        }
    }
    String::new()
}

// --------------------------------------------------------- known folders --
#[cfg(windows)]
fn known_folder(id: &windows_sys::core::GUID, fallback: PathBuf) -> String {
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::SHGetKnownFolderPath;
    unsafe {
        let mut out: windows_sys::core::PWSTR = std::ptr::null_mut();
        if SHGetKnownFolderPath(id, 0, std::ptr::null_mut(), &mut out) == 0 && !out.is_null() {
            let mut n = 0;
            while *out.add(n) != 0 {
                n += 1;
            }
            let s = String::from_utf16_lossy(std::slice::from_raw_parts(out, n));
            CoTaskMemFree(out as _);
            if !s.is_empty() && Path::new(&s).is_dir() {
                return s;
            }
        } else if !out.is_null() {
            CoTaskMemFree(out as _);
        }
    }
    norm(&fallback)
}

fn home() -> PathBuf {
    std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME"))
        .map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("."))
}

/// The real "Saved Games" - people move it, OneDrive redirects it. The
/// known folder, else %USERPROFILE%\Saved Games when the shell names none.
/// ⚠ the known folder ignores a changed USERPROFILE
pub fn saved_games() -> String {
    #[cfg(windows)]
    {
        known_folder(&windows_sys::Win32::UI::Shell::FOLDERID_SavedGames, home().join("Saved Games"))
    }
    #[cfg(not(windows))]
    {
        norm(&home().join("Saved Games"))
    }
}

// ------------------------------------------------------------- detectors --
fn first_dir(cands: &[PathBuf], contains: Option<&dyn Fn(&str) -> bool>) -> String {
    for c in cands {
        if !c.is_dir() {
            continue;
        }
        match contains {
            None => return norm(c),
            Some(f) => {
                let Ok(rd) = std::fs::read_dir(c) else { continue };
                if rd.flatten().any(|e| f(&e.file_name().to_string_lossy())) {
                    return norm(c);
                }
            }
        }
    }
    String::new()
}

fn game_dir() -> String {
    // a game exe set by hand in Settings names its folder
    let exe = config::get_str(&config::load(), "game_exe");
    if !exe.is_empty() {
        if let Some(d) = Path::new(&exe).parent().filter(|d| d.join("content.kspkg").is_file()) {
            return norm(d);
        }
    }
    let d = steam_app_dir(APPID_GAME);
    if !d.is_empty() {
        return d;
    }
    let mut cands: Vec<PathBuf> = steam_libraries().iter()
        .map(|l| Path::new(l).join("steamapps").join("common").join("Assetto Corsa EVO"))
        .collect();
    cands.push(PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Assetto Corsa EVO"));
    cands.push(PathBuf::from(r"C:\Program Files\Steam\steamapps\common\Assetto Corsa EVO"));
    first_dir(&cands, Some(&|n: &str| n.eq_ignore_ascii_case("assettocorsaevo.exe")))
}

fn game_exe() -> String {
    let d = find("game_dir", false);
    if d.is_empty() {
        return String::new();
    }
    let p = Path::new(&d).join("AssettoCorsaEVO.exe");
    if p.is_file() { norm(&p) } else { String::new() }
}

fn ace_dir() -> String {
    first_dir(&[Path::new(&saved_games()).join("ACE"), home().join("Saved Games").join("ACE")], None)
}

fn client_mods() -> String {
    let d = find("ace_dir", false);
    if d.is_empty() { d } else { norm(&Path::new(&d).join("mods")) }
}

/// Made by the game on first use, so missing is not yet wrong.
fn expected(what: &str) -> bool {
    what == "client_mods"
}

fn finder(what: &str) -> String {
    match what {
        "game_dir" => game_dir(),
        "game_exe" => game_exe(),
        "ace_dir" => ace_dir(),
        "client_mods" => client_mods(),
        _ => String::new(),
    }
}

// ----------------------------------------------------------------- cache --
fn cache_path() -> PathBuf {
    paths::data().join("paths.json")
}

fn load_cache() -> Map<String, Value> {
    crate::jsoncache::read(&cache_path())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
}

fn save_cache(what: &str, entry: Value) {
    let mut c = load_cache();
    // ⚠ only touch the disk when the path actually changed (misses are common)
    let before = c.get(what).and_then(|e| e.get("path")).cloned();
    if before.as_ref() == entry.get("path") {
        return;
    }
    c.insert(what.to_string(), entry);
    let p = cache_path();
    let tmp = p.with_extension("json.part");
    if std::fs::write(&tmp, serde_json::to_string_pretty(&Value::Object(c)).unwrap_or_default()).is_ok() {
        let _ = std::fs::rename(&tmp, &p);
    }
    crate::jsoncache::forget(&p);
}

static PROBED: Mutex<Option<HashMap<String, Instant>>> = Mutex::new(None);
const REPROBE_SECS: u64 = 15;

/// A detected path, cached. Never returns something that is not there.
pub fn find(what: &str, refresh: bool) -> String {
    let cfg = config::load();
    let ov = config::get_str(&cfg, what);
    if !ov.is_empty() && Path::new(&ov).exists() {
        return ov;
    }
    let hit = load_cache().get(what).and_then(|e| e.get("path"))
        .and_then(|p| p.as_str()).unwrap_or("").to_string();
    if !hit.is_empty() && !refresh && Path::new(&hit).exists() {
        return hit;
    }
    // ⚠ a legitimately absent path would otherwise be re-searched (and
    // re-logged) on every call; re-probe on a timer instead
    {
        let mut g = PROBED.lock().unwrap();
        let m = g.get_or_insert_with(HashMap::new);
        if !refresh {
            if let Some(t) = m.get(what) {
                if t.elapsed().as_secs() < REPROBE_SECS {
                    return hit;
                }
            }
        }
        m.insert(what.to_string(), Instant::now());
    }
    let t0 = Instant::now();
    let found = finder(what);
    let ms = (t0.elapsed().as_secs_f64() * 10000.0).round() / 10.0;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    save_cache(what, json!({"path": found, "found_at": now, "ms": ms}));
    found
}

/// The game's content.kspkg, or None. ⚠ ACECM_CLIENT_KSPKG (dev and tests)
/// points every read and write of the game's archive at a copy.
pub fn game_kspkg() -> Option<String> {
    if let Ok(p) = std::env::var("ACECM_CLIENT_KSPKG") {
        return Path::new(&p).is_file().then_some(p);
    }
    let g = steam_app_dir(APPID_GAME);
    let g = if g.is_empty() { find("game_dir", true) } else { g };
    if g.is_empty() {
        return None;
    }
    let p = Path::new(&g).join("content.kspkg");
    p.is_file().then(|| p.to_string_lossy().into_owned())
}

/// EVO's Saved Games folder: the setting, else detected, else where it
/// will be (Saved Games\ACE).
pub fn saved_ace_dir() -> String {
    let cfg = config::load();
    let a = config::get_str(&cfg, "ace_dir");
    if !a.is_empty() {
        return a;
    }
    let d = find("ace_dir", false);
    if !d.is_empty() {
        return d;
    }
    Path::new(&saved_games()).join("ACE").to_string_lossy().into_owned()
}

/// The folders the converter uses, for the Settings page (GET /api/detect).
pub fn all_paths(refresh: bool) -> Value {
    let t0 = Instant::now();
    let cfg = config::load();
    let mut out = Map::new();
    for what in ["game_dir", "ace_dir", "client_mods"] {
        let p = find(what, refresh);
        let exists = !p.is_empty() && Path::new(&p).exists();
        out.insert(what.to_string(), json!({
            "path": p,
            "exists": exists,
            "pending": !p.is_empty() && !exists && expected(what),
            "source": if config::get_str(&cfg, what).is_empty() { "detected" } else { "setting" },
        }));
    }
    json!({
        "paths": out,
        "steam": steam_root(),
        "libraries": steam_libraries(),
        "saved_games": saved_games(),
        "took_ms": (t0.elapsed().as_secs_f64() * 10000.0).round() / 10.0,
    })
}
