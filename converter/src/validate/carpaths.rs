//! CarData paths (check-car): every file EVO's physics loads by path is named
//! and resolves.
//!
//! When a path it loads is empty, EVO's physics thread throws "Trying to load
//! a message with an empty path" and the game crashes as the car loads. The
//! Formula Truck Volvo FH (11 Nov 2026) had a front wing without a drag curve:
//! its aero.ini named a table that is not there (`car::physics::aero`).
//! `check_refs` resolves the paths a file names, but not one that was left
//! out, nor the physics file types (.tyre, .coilover, .wing, ...); this check
//! reads CarData field by field (the CarData schema embedded in
//! AssettoCorsaEVO.exe). Required = filled on every stock car:
//! `aceconv check-car-stock` runs this check on them.

use crate::io::pb::{Msg, Val};

/// CarData's top-level path fields: (field, what, required on every stock car).
const TOP: [(u32, &str, bool); 13] = [
    (16, "setup limits", true),
    (19, "stock setup", true),
    (20, "engine", true),
    (21, "collider mesh", true),
    (22, "drivetrain", true),
    (24, "gearbox", true),
    (26, "clutch", true),
    (27, "brakes", true),
    (29, "AI car data", true),
    (77, "electronics", false),
    (190, "AI setup", false),
    (191, "wet setup", false),
    (201, "general", false),
];

/// SuspensionsData (CarData f2) path fields, all required.
const SUSPENSION: [(u32, &str); 4] = [(20, "front coilover"), (21, "rear coilover"), (22, "front suspension"), (23, "rear suspension")];

/// EngineData curves: (field, what, required). The throttle response curve is
/// optional: ks_bmw_m8_competition, ks_mazda_rx7_fd and ks_toyota_supra_mkiv
/// have none.
const ENGINE: [(u32, &str, bool); 4] = [(2, "power curve", true), (3, "coast curve", true), (14, "throttle response curve", false), (21, "gas / coast offset curve", false)];

/// WingData curves: (field, what, required). Every stock wing and fin has
/// both angle-of-attack curves; the ground-height pair is optional.
const WING: [(u32, &str, bool); 4] = [(7, "angle-of-attack lift curve", true), (8, "angle-of-attack drag curve", true), (9, "ground-height lift curve", false), (10, "ground-height drag curve", false)];

/// Every string of a (repeated) field.
fn strs(m: &Msg, num: u32) -> Vec<String> {
    m.all(num).filter_map(|v| match v {
        Val::Bytes(b) => Some(String::from_utf8_lossy(b).into_owned()),
        _ => None,
    }).collect()
}

/// The path problems of a car's CarData (`cardata`: its path), read with
/// `read`; `has` says whether a path resolves (the package or the game).
pub fn check(cardata: &str, read: &dyn Fn(&str) -> Option<Vec<u8>>, has: &dyn Fn(&str) -> bool) -> Vec<String> {
    let mut errs = Vec::new();
    let Some(cd) = read(cardata).and_then(|b| Msg::parse(&b).ok()) else {
        errs.push(format!("physics: {cardata} is missing or unreadable"));
        return errs;
    };
    let path = |what: &str, v: Option<String>, required: bool, errs: &mut Vec<String>| -> Option<String> {
        match v.filter(|s| !s.trim().is_empty()) {
            None => {
                if required {
                    errs.push(format!("physics: no {what} path - EVO's physics throws \"Trying to load a message with an empty path\" and the game crashes as the car loads"));
                }
                None
            }
            Some(p) if !has(&p) => {
                errs.push(format!("physics: the {what} {p} is missing"));
                None
            }
            Some(p) => Some(p),
        }
    };
    let mut named = std::collections::BTreeMap::new();
    for (num, what, required) in TOP {
        if let Some(p) = path(what, cd.str(num), required, &mut errs) {
            named.insert(num, p);
        }
    }
    for (num, what) in [(10u32, "front tyre compound"), (11, "rear tyre compound")] {
        let all = strs(&cd, num);
        if all.is_empty() {
            path(what, None, true, &mut errs);
        }
        for p in all {
            path(what, Some(p), true, &mut errs);
        }
    }
    let sp = cd.sub(2).unwrap_or_default();
    for (num, what) in SUSPENSION {
        path(what, sp.str(num), true, &mut errs);
    }
    // the engine: its own file when CarData names one, else the inline copy
    let engine = named.get(&20).and_then(|p| read(p)).and_then(|b| Msg::parse(&b).ok()).or_else(|| cd.sub(4)).unwrap_or_default();
    for (num, what, required) in ENGINE {
        path(what, engine.str(num), required, &mut errs);
    }
    for p in strs(&engine, 29) {
        path("turbo", Some(p), true, &mut errs);
    }
    // wings and fins, inline and in the .wing files CarData lists
    let aero = cd.sub(12).unwrap_or_default();
    let mut wings: Vec<(String, Msg)> = aero.subs(3).into_iter().chain(aero.subs(4)).map(|w| ("CarData".to_string(), w)).collect();
    for p in strs(&aero, 10) {
        if let Some(p) = path("wing file", Some(p), true, &mut errs) {
            if let Some(w) = read(&p).and_then(|b| Msg::parse(&b).ok()) {
                wings.push((p.rsplit('\\').next().unwrap_or(&p).to_string(), w));
            }
        }
    }
    for (src, w) in &wings {
        let name = w.str(2).unwrap_or_default();
        for (num, what, required) in WING {
            path(&format!("{what} of wing \"{name}\" ({src})"), w.str(num), required, &mut errs);
        }
    }
    // brake compounds named by the brakes file
    if let Some(b) = named.get(&27).and_then(|p| read(p)).and_then(|b| Msg::parse(&b).ok()) {
        for num in [12u32, 13] {
            for p in strs(&b, num) {
                path("brake compound", Some(p), true, &mut errs);
            }
        }
    }
    errs.sort();
    errs.dedup();
    errs
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn car(front_cd: Option<&str>) -> BTreeMap<String, Vec<u8>> {
        let base = "content\\cars\\ks_x\\data\\";
        let mut files = BTreeMap::new();
        let mut cd = Msg::default();
        for (num, _, required) in TOP {
            if required {
                cd.set_str(num, &format!("{base}f{num}.bin"));
            }
        }
        cd.push(10, Val::Bytes(format!("{base}front.tyre").into_bytes()));
        cd.push(11, Val::Bytes(format!("{base}rear.tyre").into_bytes()));
        let mut sp = Msg::default();
        for (num, _) in SUSPENSION {
            sp.set_str(num, &format!("{base}s{num}.bin"));
        }
        cd.set(2, Val::Msg(sp));
        let mut eng = Msg::default();
        for (num, _, required) in ENGINE {
            if required {
                eng.set_str(num, &format!("{base}e{num}.curve"));
            }
        }
        let wing = |name: &str, cd_curve: Option<&str>| {
            let mut w = Msg::default();
            w.set_str(2, name);
            w.set_str(7, &format!("{base}{name}_cl.curve"));
            if let Some(c) = cd_curve {
                w.set_str(8, &format!("{base}{c}"));
            }
            w
        };
        let mut aero = Msg::default();
        aero.push(3, Val::Msg(wing("BODY", Some("body_cd.curve"))));
        aero.push(3, Val::Msg(wing("FRONT WING", front_cd)));
        cd.set(12, Val::Msg(aero));
        files.insert(format!("{base}cardata.car"), cd.encode());
        files.insert(format!("{base}f20.bin"), eng.encode());
        files
    }

    fn run(files: &BTreeMap<String, Vec<u8>>) -> Vec<String> {
        let read = |p: &str| files.get(p).cloned();
        // everything named resolves except what the test leaves out
        let has = |p: &str| !p.ends_with("absent.curve");
        check("content\\cars\\ks_x\\data\\cardata.car", &read, &has)
    }

    #[test]
    fn a_wing_without_its_drag_curve_is_an_error() {
        // the Formula Truck Volvo FH's front wing (aero.ini named a table that is not there)
        let e = run(&car(None));
        assert_eq!(e.len(), 1, "{e:?}");
        assert!(e[0].contains("drag curve of wing \"FRONT WING\"") && e[0].contains("empty path"), "{e:?}");
        assert!(run(&car(Some("front_cd.curve"))).is_empty());
        // named but not in the package or the game
        let e = run(&car(Some("absent.curve")));
        assert!(e.len() == 1 && e[0].contains("is missing"), "{e:?}");
    }
}
