//! List the EVO-space bounds (after the datum) of every mesh whose node or
//! material name contains one of the filters, with the kind the converter
//! files it under and whether it sits in the cockpit. For checking where a
//! part lands after conversion (a wing off the boot, a part in the cabin).
//!
//! cargo run --release --example nodebounds -- <ac1 car dir> <filter>...

use aceconv::ac1::car::Car;
use aceconv::car::frame::{self, Frame};
use aceconv::car::geometry;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let car = Car::open(std::path::Path::new(&args[0]))?;
    let k = car.kn5()?;
    let f = Frame::build(&car, &k);
    let filters: Vec<String> = args[1..].iter().map(|s| s.to_lowercase()).collect();
    let mut notes = Vec::new();
    let insts = geometry::flatten(&k, &mut notes);
    println!("datum ({:.3}, {:.3}, {:.3})", f.datum[0], f.datum[1], f.datum[2]);
    for i in &insts {
        let mat = k.materials.get(i.material).map(|m| m.name.as_str()).unwrap_or("?");
        let hay = format!("{} {}", i.node, mat).to_lowercase();
        if !filters.is_empty() && !filters.iter().any(|x| hay.contains(x)) {
            continue;
        }
        let w = f.evo_m(&i.world);
        let (mut lo, mut hi) = ([f64::MAX; 3], [f64::MIN; 3]);
        for v in 0..i.mesh.vertex_count {
            let p = i.mesh.position(v);
            let q = frame::xform(&w, [p[0] as f64, p[1] as f64, p[2] as f64]);
            for a in 0..3 {
                lo[a] = lo[a].min(q[a]);
                hi[a] = hi[a].max(q[a]);
            }
        }
        println!("{:28} {:24} under={:?} cockpit={} x {:+.3}..{:+.3} y {:+.3}..{:+.3} z {:+.3}..{:+.3}", i.node, mat, i.under, i.cockpit as u8, lo[0], hi[0], lo[1], hi[1], lo[2], hi[2]);
    }
    Ok(())
}
