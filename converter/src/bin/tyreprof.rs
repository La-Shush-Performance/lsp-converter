//! dev: the profile of an AC1 tyre node - its vertices in the node's own
//! frame (axle = local x), as (radius band -> widest axial offset from the
//! tread's middle), plus the width, bead and outer radius.
//! `tyreprof <car.kn5> <node> [bands]`
use aceconv::ac1::kn5::{self, NodeKind};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let bands: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(16);
    let b = std::fs::read(&a[1]).unwrap();
    let k = kn5::parse(&b).unwrap();
    let id = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
    let mut all = Vec::new();
    k.root.walk(id, &mut all);
    // the node's own world frame: the parent transform of its mesh(es)
    let mut pts: Vec<(f64, f64)> = Vec::new();
    for (n, w) in &all {
        let NodeKind::Mesh(m) = &n.kind else { continue };
        if !n.name.eq_ignore_ascii_case(&a[2]) {
            continue;
        }
        // world -> local: w is the mesh's world matrix (row vectors); its
        // translation is the wheel centre, its first row the axle
        let (ax, ay, az) = (w[0] as f64, w[1] as f64, w[2] as f64);
        let al = (ax * ax + ay * ay + az * az).sqrt();
        let (ax, ay, az) = (ax / al, ay / al, az / al);
        let (ox, oy, oz) = (w[12] as f64, w[13] as f64, w[14] as f64);
        for v in 0..m.vertex_count {
            let p = kn5::xform(w, m.position(v));
            let (dx, dy, dz) = (p[0] as f64 - ox, p[1] as f64 - oy, p[2] as f64 - oz);
            let axial = dx * ax + dy * ay + dz * az;
            let (rx, ry, rz) = (dx - axial * ax, dy - axial * ay, dz - axial * az);
            pts.push((axial, (rx * rx + ry * ry + rz * rz).sqrt()));
        }
    }
    if pts.is_empty() {
        println!("no mesh node {}", a[2]);
        return;
    }
    let rmax = pts.iter().map(|p| p.1).fold(0.0, f64::max);
    let tread: Vec<f64> = pts.iter().filter(|p| p.1 >= rmax * 0.97).map(|p| p.0).collect();
    let mid = (tread.iter().copied().fold(f64::MAX, f64::min) + tread.iter().copied().fold(f64::MIN, f64::max)) / 2.0;
    let rmin = pts.iter().map(|p| p.1).fold(f64::MAX, f64::min);
    let w = pts.iter().map(|p| p.0).fold(f64::MIN, f64::max) - pts.iter().map(|p| p.0).fold(f64::MAX, f64::min);
    println!("{} verts: width {:.4}, bead r {:.4}, outer r {:.4}, tread mid {:+.4}", pts.len(), w, rmin, rmax, mid);
    for i in 0..=bands {
        let lo = rmin + (rmax - rmin) * i as f64 / (bands as f64 + 1.0);
        let hi = rmin + (rmax - rmin) * (i + 1) as f64 / (bands as f64 + 1.0);
        let hw = pts.iter().filter(|p| p.1 >= lo && p.1 <= hi).map(|p| (p.0 - mid).abs()).fold(0.0, f64::max);
        println!("  r {:.3}  half-width {:.4}", (lo + hi) / 2.0, hw);
    }
}
