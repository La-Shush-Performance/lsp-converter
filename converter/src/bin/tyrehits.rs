//! dev: which body nodes reach into each tyre at rest (the check-car measure,
//! with tyres.ini's size): tyrehits <AC1 car folder>
use aceconv::car::frame::{self, Frame, CORNERS};
use aceconv::car::geometry::{self, Under};
use std::collections::BTreeMap;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let car = aceconv::ac1::car::Car::open(std::path::Path::new(&a[1])).unwrap();
    let k = car.kn5().unwrap();
    let f = Frame::build(&car, &k);
    let insts = geometry::flatten(&k, &mut Vec::new());
    for c in CORNERS {
        let t = f.tyre(c);
        let (radius, hw, rim) = (t.radius, t.width / 2.0, t.rim_radius.min(t.radius - 0.03));
        let mut hits: BTreeMap<String, (usize, f64)> = BTreeMap::new();
        for i in insts.iter().filter(|i| i.under == Under::Body && !i.cockpit) {
            let m = f.evo_m(&i.world);
            for v in 0..i.mesh.vertex_count {
                let p = i.mesh.position(v);
                let q = f.to_wheel(c, frame::xform(&m, [p[0] as f64, p[1] as f64, p[2] as f64]));
                let r = (q[1] * q[1] + q[2] * q[2]).sqrt();
                if q[0].abs() <= hw - 0.004 && r > rim + 0.015 && r < radius - 0.006 {
                    let e = hits.entry(i.path.clone()).or_default();
                    e.0 += 1;
                    e.1 = e.1.max(radius - r);
                }
            }
        }
        let mut v: Vec<_> = hits.into_iter().collect();
        v.sort_by(|a, b| b.1 .0.cmp(&a.1 .0));
        println!("{c}: tyre r {:.3} w {:.3} rim {:.3}; {} nodes", radius, t.width, t.rim_radius, v.len());
        for (p, (n, d)) in v.iter().take(6) {
            println!("  {n:5} verts up to {:3.0} mm  {p}", d * 1000.0);
        }
    }
}
