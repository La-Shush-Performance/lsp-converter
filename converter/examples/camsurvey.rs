//! Survey the stock drivable cameras of content.kspkg against each car's
//! exterior mesh bounds (the box `validate::camcheck` uses): per car the
//! body top, tail, nose and length, the cockpit eye, and every external
//! camera relative to the body (chase height over the top and distance
//! behind the tail, bonnet / bumper against the eye and the nose).
//! Calibrates the chase and bonnet rules in src/car/cameras.rs.
//!
//! cargo run --release --example camsurvey -- <content.kspkg> [car id filter]

use aceconv::car::fx;
use aceconv::io::kspkg::Pkg;
use aceconv::io::pb::Msg;
use std::collections::HashMap;

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let pkg = Pkg::open(std::path::Path::new(&a[1]))?;
    let filt = a.get(2).cloned().unwrap_or_default();
    let idx: HashMap<String, usize> = pkg.entries.iter().enumerate().map(|(i, e)| (e.path.to_lowercase(), i)).collect();
    let get = |p: &str| idx.get(&p.to_lowercase()).and_then(|&i| pkg.read(&pkg.entries[i]).ok());
    let mut ids: Vec<String> = pkg.entries.iter().filter_map(|e| {
        let l = e.path.to_lowercase();
        let r = l.strip_prefix("content\\cars\\")?;
        let (id, file) = r.split_once('\\')?;
        (file == format!("{id}.actor")).then(|| id.to_string())
    }).collect();
    ids.sort();
    ids.dedup();
    println!("car\ttop\tbottom\ttail\tnose\tlength\twidth\teye_y\teye_z\tnear_y\tnear_over_top\tnear_behind_tail\tnear_pitch\tfar_y\tfar_over_top\tfar_behind_tail\tfar_pitch\tbonnet_y\tbonnet_z\tbonnet_pitch\tbumper_y\tbumper_z");
    for id in ids.iter().filter(|i| i.contains(&filt)) {
        let Some(ab) = get(&format!("content\\cars\\{id}\\{id}.actor")) else { continue };
        let Ok(act) = Msg::parse(&ab) else { continue };
        let Some(car) = act.sub(103) else { continue };
        // the exterior mesh, as camcheck finds it
        let ext = idx.keys().find(|k| k.starts_with(&format!("content\\cars\\{id}\\meshes\\")) && k.ends_with("_exterior.mesh")).cloned();
        let Some(mesh) = ext.and_then(|p| get(&p)) else { continue };
        let Ok(top) = Msg::parse(&mesh) else { continue };
        let v = |m: Option<Msg>| m.map(|m| [m.f32(1).unwrap_or(0.0) as f64, m.f32(2).unwrap_or(0.0) as f64, m.f32(3).unwrap_or(0.0) as f64]).unwrap_or([0.0; 3]);
        let (lo, hi) = (v(top.sub(6)), v(top.sub(7)));
        let cams: Vec<(String, [f64; 3], f32)> = car.subs(10).iter().filter_map(|c| {
            let name = c.sub(2)?.str(2)?;
            let (p, r) = fx::transform_of(&c.sub(1)?);
            Some((name, p.map(|x| x as f64), r[0]))
        }).collect();
        let find = |n: &str| cams.iter().find(|c| c.0.eq_ignore_ascii_case(n));
        let (Some(e), Some(nc), Some(fc), Some(bo), Some(bu)) = (find("Cockpit"), find("NearChase"), find("FarChase"), find("Bonnet"), find("Bumper")) else { continue };
        println!("{id}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.1}\t{:.3}\t{:.3}\t{:.3}\t{:.1}\t{:.3}\t{:.3}\t{:.1}\t{:.3}\t{:.3}",
            hi[1], lo[1], lo[2], hi[2], hi[2] - lo[2], hi[0] - lo[0], e.1[1], e.1[2],
            nc.1[1], nc.1[1] - hi[1], lo[2] - nc.1[2], nc.2,
            fc.1[1], fc.1[1] - hi[1], lo[2] - fc.1[2], fc.2,
            bo.1[1], bo.1[2], bo.2, bu.1[1], bu.1[2]);
    }
    Ok(())
}
