//! Car camera checks (requirements car-cameras-exhaust-actor.md): every
//! drivable camera where the stock cars put theirs, none buried in the body.
//!
//! Ranges are the stock spread (71 cars, `src/car/cameras.rs` table) with a
//! margin; the shell test is `cameras::enclosed` (axis rays hitting the
//! exterior mesh within 3 m: stock external cameras 0..2, cockpit 2..6).

use std::collections::BTreeMap;

use crate::car::cameras::{chase_scale, enclosed, mesh_tris};
use crate::car::fx;
use crate::io::pb::Msg;

pub struct Found {
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

pub fn check(files: &BTreeMap<String, Vec<u8>>, id: &str) -> Found {
    let mut out = Found { warnings: vec![], errors: vec![] };
    let get = |p: &str| files.iter().find(|(k, _)| k.eq_ignore_ascii_case(p)).map(|(_, v)| v);
    let Some(car) = get(&format!("content\\cars\\{id}\\{id}.actor")).and_then(|b| Msg::parse(b).ok()).and_then(|a| a.sub(103)) else { return out };
    let ext = files.iter().find(|(k, _)| {
        let k = k.to_lowercase();
        k.starts_with(&format!("content\\cars\\{id}\\meshes\\").to_lowercase()) && k.ends_with("_exterior.mesh")
    });
    let Some((_, mesh)) = ext else { return out };
    let tris = mesh_tris(mesh);
    let Some(top) = Msg::parse(mesh).ok() else { return out };
    let v = |m: Option<Msg>| m.map(|m| [m.f32(1).unwrap_or(0.0) as f64, m.f32(2).unwrap_or(0.0) as f64, m.f32(3).unwrap_or(0.0) as f64]).unwrap_or([0.0; 3]);
    let (lo, hi) = (v(top.sub(6)), v(top.sub(7)));
    let cams: Vec<(String, [f64; 3], f32)> = car.subs(10).iter().filter_map(|c| {
        let name = c.sub(2)?.str(2)?;
        let (p, r) = fx::transform_of(&c.sub(1)?);
        Some((name, p.map(|x| x as f64), r[0]))
    }).collect();
    let find = |n: &str| cams.iter().find(|c| c.0.eq_ignore_ascii_case(n));
    let p3 = |p: [f64; 3]| format!("({:.2}, {:.2}, {:.2})", p[0], p[1], p[2]);
    for n in ["Cockpit", "Dash", "Bonnet", "Bumper", "NearChase", "FarChase"] {
        if find(n).is_none() {
            out.errors.push(format!("cameras: no {n} camera"));
        }
    }
    // cockpit: in the cabin, at seated head height
    if let Some((_, e, pitch)) = find("Cockpit") {
        let inside = (0..3).all(|k| e[k] > lo[k] && e[k] < hi[k]);
        let below_top = e[1] - hi[1];
        if !inside || e[0].abs() > 0.6 || e[2] < lo[2] + 0.4 || e[2] > hi[2] - 0.6 {
            out.errors.push(format!("cameras: cockpit eye {} is outside the cabin (body {} .. {})", p3(*e), p3(lo), p3(hi)));
        } else if !(-0.9..=-0.12).contains(&below_top) {
            out.warnings.push(format!("cameras: cockpit eye {:.2} m below the body top (stock -0.81..-0.19)", -below_top));
        }
        if !tris.is_empty() && enclosed(&tris, *e) < 2 {
            out.warnings.push(format!("cameras: cockpit eye {} has no body around it (stock: floor and dash at least)", p3(*e)));
        }
        if !(-6.0..=3.0).contains(pitch) {
            out.warnings.push(format!("cameras: cockpit pitch {pitch:.1} (stock -5.9..2.7)"));
        }
    }
    // external cameras: not in the body shell
    if !tris.is_empty() {
        for n in ["Bonnet", "Bumper", "NearChase", "FarChase"] {
            if let Some((_, p, _)) = find(n) {
                let h = enclosed(&tris, *p);
                if h >= 4 {
                    out.errors.push(format!("cameras: {n} {} is inside the body shell ({h} of 6 rays blocked; stock 0..2)", p3(*p)));
                }
            }
        }
    }
    // chase: behind and above, at stock distances
    for n in ["NearChase", "FarChase"] {
        if let Some((_, p, _)) = find(n) {
            match chase_finding(n, *p, lo, hi) {
                Some((true, e)) => out.errors.push(e),
                Some((false, w)) => out.warnings.push(w),
                None => {}
            }
        }
    }
    // bonnet ahead of the eye, bumper at the nose
    if let (Some((_, e, _)), Some((_, b, _)), Some((_, u, _))) = (find("Cockpit"), find("Bonnet"), find("Bumper")) {
        if b[2] < e[2] - 0.4 || b[2] > u[2] + 0.1 {
            out.warnings.push(format!("cameras: bonnet z {:.2} is not between the eye ({:.2}) and the bumper ({:.2})", b[2], e[2], u[2]));
        }
        if u[2] < hi[2] - 1.0 {
            out.warnings.push(format!("cameras: bumper z {:.2} is {:.2} m behind the nose (stock 0..0.9)", u[2], hi[2] - u[2]));
        }
    }
    // the cockpit probe in the body
    if let Some(t) = car.sub(17) {
        let p = fx::transform_of(&t).0.map(|x| x as f64);
        if !(0..3).all(|k| p[k] > lo[k] && p[k] < hi[k]) {
            out.errors.push(format!("cameras: cockpit probe {} is outside the body", p3(p)));
        }
    }
    // onboard cameras near the car
    for (i, c) in car.subs(1).iter().enumerate() {
        if let Some(t) = c.sub(1) {
            let p = fx::transform_of(&t).0.map(|x| x as f64);
            if (0..3).any(|k| p[k] < lo[k] - 3.0 || p[k] > hi[k] + 3.0) {
                out.warnings.push(format!("cameras: onboard camera {i} at {} is over 3 m from the body", p3(p)));
            }
        }
    }
    out
}

/// One chase camera (`NearChase` / `FarChase`) against the stock rule: behind
/// the tail and above the body top, else an error (true); at the stock
/// height and distance behind the tail, else a warning (false). The stock
/// ranges scale with a body bigger than any stock car's, as the converter's
/// chase rig does (`cameras::chase_scale`; 1 on every stock-sized car).
pub fn chase_finding(n: &str, p: [f64; 3], lo: [f64; 3], hi: [f64; 3]) -> Option<(bool, String)> {
    let p3 = |p: [f64; 3]| format!("({:.2}, {:.2}, {:.2})", p[0], p[1], p[2]);
    let tail = lo[2];
    let dz = p[2] - tail;
    if dz >= 0.0 || p[1] <= hi[1] {
        return Some((true, format!("cameras: {n} {} is not behind and above the car (tail z {tail:.2}, top {:.2})", p3(p), hi[1])));
    }
    let (ys, zs) = if n.eq_ignore_ascii_case("NearChase") { ((1.4, 2.2), (-3.5, -2.3)) } else { ((2.0, 2.9), (-5.0, -3.6)) };
    let s = chase_scale(lo, hi);
    let (ys, zs) = ((ys.0 * s, ys.1 * s), (zs.0 * s, zs.1 * s));
    if !(ys.0..=ys.1).contains(&p[1]) || !(zs.0..=zs.1).contains(&dz) {
        let scaled = if s > 1.0 { format!("; scaled x {s:.2} with the body") } else { String::new() };
        return Some((false, format!("cameras: {n} at y {:.2}, {:.2} m behind the tail (stock y {:.1}..{:.1}, {:.1}..{:.1} m{scaled})", p[1], -dz, ys.0, ys.1, -zs.1, -zs.0)));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::car::cameras::{bonnet_misplaced, chase};

    /// The Formula Truck Volvo FH as converted (8 Nov build): exterior box,
    /// cockpit eye and the check result of a user's conversion.
    const TRUCK_LO: [f64; 3] = [-1.25, 0.10, -2.46];
    const TRUCK_HI: [f64; 3] = [1.25, 2.59, 2.77];
    const TRUCK_EYE: [f64; 3] = [0.582, 1.901, 1.221];

    #[test]
    fn truck_chase_cameras_pass() {
        // the old fixed rule: 2.0 m, below the 2.59 m roof - the error the user got
        assert!(matches!(chase_finding("NearChase", [0.0, 2.0, -5.239], TRUCK_LO, TRUCK_HI), Some((true, _))));
        let (near, far, s) = chase(TRUCK_EYE, TRUCK_LO, TRUCK_HI);
        assert!((s - 2.59 / 1.68).abs() < 1e-9, "{s}");
        // above the roof by about the stock median (0.50), behind the tail
        assert!(near[1] - TRUCK_HI[1] > 0.4 && near[1] - TRUCK_HI[1] < 0.6, "{near:?}");
        assert!(far[1] > near[1] && far[2] < near[2] && near[2] < TRUCK_LO[2]);
        assert_eq!(chase_finding("NearChase", near, TRUCK_LO, TRUCK_HI), None);
        assert_eq!(chase_finding("FarChase", far, TRUCK_LO, TRUCK_HI), None);
    }

    #[test]
    fn stock_sized_chase_is_unchanged() {
        // mx5_na's box and eye: scale 1, the stock positions, no finding
        let (lo, hi) = ([-0.91, 0.02, -1.972], [0.91, 1.195, 1.979]);
        let (near, far, s) = chase([0.35, 0.951, -0.336], lo, hi);
        assert_eq!(s, 1.0);
        assert!((near[1] - 1.8).abs() < 1e-9 && (near[2] - (lo[2] - 2.78)).abs() < 1e-9);
        assert!((far[1] - 2.5).abs() < 1e-9 && (far[2] - (lo[2] - 4.22)).abs() < 1e-9);
        assert_eq!(chase_finding("NearChase", near, lo, hi), None);
        assert_eq!(chase_finding("FarChase", far, lo, hi), None);
        // the stock rule's warning text is kept for stock-sized cars
        let w = chase_finding("NearChase", [0.0, 2.4, lo[2] - 2.78], lo, hi).unwrap();
        assert!(!w.0 && w.1.ends_with("(stock y 1.4..2.2, 2.3..3.5 m)"), "{}", w.1);
    }

    #[test]
    fn truck_bonnet_camera_pos_is_no_bonnet_camera() {
        // the user's log: BONNET_CAMERA_POS lands at (0, 4.875, -7.285)
        assert_eq!(bonnet_misplaced([0.0, 4.875, -7.285], TRUCK_EYE, TRUCK_HI), Some("behind the driver's eye"));
        assert_eq!(bonnet_misplaced([0.0, 4.875, 2.0], TRUCK_EYE, TRUCK_HI), Some("high above the body"));
        assert_eq!(bonnet_misplaced([0.0, 2.0, 2.6], TRUCK_EYE, TRUCK_HI), Some("in or ahead of the nose"));
        // over the front of the cab; f2004's T-cam 0.36 m behind its eye
        assert_eq!(bonnet_misplaced([0.0, 2.05, 2.2], TRUCK_EYE, TRUCK_HI), None);
        assert_eq!(bonnet_misplaced([0.0, 1.08, -0.471], [0.0, 0.681, -0.111], [0.87, 1.071, 2.303]), None);
    }
}
