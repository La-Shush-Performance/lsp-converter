//! The stock fleet's handling facts behind the converter's profile rules
//! (src/car/physics/profile.rs): per stock car, the tyre templates it
//! drives on (class, leaf-field count, the template's own pressure), the
//! stock setup's alignment (pressure, camber, toe per axle), front bias and
//! whether the setup lets the player change it, the steering (lock, ratio,
//! road-wheel lock) and the TC levels' activation slip angles.
//!
//! cargo run --release --example handlingsurvey -- <content.kspkg> > tools/handling_stock.tsv

use aceconv::car::physics::pbx::{rf, rf_at};
use aceconv::io::kspkg::Pkg;
use aceconv::io::pb::{Msg, Val};
use std::collections::BTreeMap;

/// Leaf fields of a message, nested messages expanded (strings count once).
fn leaves(m: &Msg) -> usize {
    m.fields.iter().map(|f| match &f.val {
        Val::Msg(s) => leaves(s),
        Val::Bytes(b) if !b.iter().all(|c| (0x20..0x7f).contains(c)) => Msg::parse(b).map(|s| leaves(&s)).unwrap_or(1),
        _ => 1,
    }).sum()
}

fn class(p: &str) -> String {
    let l = p.to_lowercase();
    l.split("\\tyres\\").nth(1).and_then(|r| r.split('\\').next()).unwrap_or("?").to_string()
}

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let pkg = Pkg::open(std::path::Path::new(&a[1]))?;
    let read = |p: &str| pkg.find(p).and_then(|e| pkg.read(e).ok()).and_then(|b| Msg::parse(&b).ok());
    let mut cars: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for e in &pkg.entries {
        let l = e.path.to_lowercase();
        if let Some(r) = l.strip_prefix("content\\cars\\") {
            let id = r.split('\\').next().unwrap_or("");
            if id.starts_with("ks_") {
                cars.entry(id.to_string()).or_default().push(e.path.clone());
            }
        }
    }
    println!("car\tclass_f\tclass_r\ttyre_f\ttyre_r\tleaves_f\tleaves_r\ttmpl_psi_f\ttmpl_psi_r\tpsi_f\tpsi_r\tcamber_f\tcamber_r\ttoe_f\ttoe_r\tbias\tbias_adj\tbias_lo\tbias_hi\tlock\tratio\tsetup_ratio\twheel_lock\ttc_activation");
    for (id, paths) in &cars {
        let Some(cdp) = aceconv::car::physics::stats::cardata_of(paths.iter().map(|s| s.as_str())) else { continue };
        let Some(cd) = read(&cdp) else { continue };
        let tf = cd.str(10).unwrap_or_default();
        let tr = cd.str(11).unwrap_or_default();
        let (mf, mr) = (read(&tf), read(&tr));
        let lf = mf.as_ref().map(leaves).unwrap_or(0);
        let lr = mr.as_ref().map(leaves).unwrap_or(0);
        let pf = mf.as_ref().and_then(|m| rf(m, 11)).unwrap_or(f64::NAN);
        let pr = mr.as_ref().and_then(|m| rf(m, 11)).unwrap_or(f64::NAN);
        let set = cd.str(19).and_then(|p| read(&p)).unwrap_or_default();
        let lim = cd.str(16).and_then(|p| read(&p)).unwrap_or_default();
        let al = set.subs(4);
        let g = |k: usize, n: u32| al.get(k).and_then(|m| rf(m, n)).unwrap_or(f64::NAN);
        let bias = rf_at(&set, &[1, 3, 1]).unwrap_or(f64::NAN);
        let bl = lim.sub(1).and_then(|m| m.sub(3)).and_then(|m| m.sub(1)).unwrap_or_default();
        let ctl = cd.sub(8).unwrap_or_default();
        let lock = rf(&ctl, 2).unwrap_or(f64::NAN);
        let ratio = rf(&ctl, 3).unwrap_or(f64::NAN);
        let sratio = rf_at(&set, &[1, 2]).unwrap_or(f64::NAN);
        let tc: Vec<String> = cd.sub(7).and_then(|e| e.sub(1)).map(|t| t.subs(7).iter().skip(1).map(|s| format!("{}", rf(s, 7).unwrap_or(0.0))).collect()).unwrap_or_default();
        let r = if sratio.is_finite() && sratio.abs() > 1.0 { sratio } else { ratio };
        println!(
            "{id}\t{}\t{}\t{}\t{}\t{lf}\t{lr}\t{pf}\t{pr}\t{}\t{}\t{}\t{}\t{}\t{}\t{bias}\t{}\t{}\t{}\t{lock}\t{ratio}\t{sratio}\t{:.2}\t{}",
            class(&tf), class(&tr), tf.rsplit('\\').next().unwrap_or(""), tr.rsplit('\\').next().unwrap_or(""),
            g(0, 1), g(2, 1), g(0, 2), g(2, 2), g(0, 3), g(2, 3),
            bl.varint(4).unwrap_or(0), rf(&bl, 2).unwrap_or(f64::NAN), rf(&bl, 3).unwrap_or(f64::NAN),
            lock / r.abs(), tc.join(",")
        );
    }
    Ok(())
}
