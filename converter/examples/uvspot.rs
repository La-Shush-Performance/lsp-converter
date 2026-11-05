//! Find where a texture area lands on a kn5 car and what else is there: the
//! world centre of the triangles of `material` whose UVs fall within
//! `radius` of (u, v), then every mesh with triangles within `dist` metres
//! of that centre, with its material and triangle count. For telling which
//! part shows at a spot seen in game (a fuel cap, its ring). UVSPOT_TRIS=<material>
//! also prints each of that material's nearby triangles.
//!
//! cargo run --release --example uvspot -- <car.kn5> <material | @x,y,z> <u> <v> <radius> [dist]

use aceconv::ac1::kn5::{self, NodeKind};

fn xf(w: &[f32; 16], p: [f32; 3]) -> [f32; 3] {
    [p[0] * w[0] + p[1] * w[4] + p[2] * w[8] + w[12], p[0] * w[1] + p[1] * w[5] + p[2] * w[9] + w[13], p[0] * w[2] + p[1] * w[6] + p[2] * w[10] + w[14]]
}

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let map = unsafe { memmap2::Mmap::map(&std::fs::File::open(&a[0])?)? };
    let k = kn5::parse(&map)?;
    let (u0, v0, r): (f32, f32, f32) = (a[2].parse()?, a[3].parse()?, a[4].parse()?);
    let dist: f32 = a.get(5).and_then(|s| s.parse().ok()).unwrap_or(0.1);
    let mut all = Vec::new();
    k.root.walk(kn5::IDENTITY, &mut all);
    let mat = |m: &kn5::Mesh| k.materials.get(m.material_id as usize).map(|x| x.name.clone()).unwrap_or_default();
    let (mut c, mut n) = ([0f32; 3], 0f32);
    for (node, w) in &all {
        let NodeKind::Mesh(m) = &node.kind else { continue };
        if !mat(m).eq_ignore_ascii_case(&a[1]) {
            continue;
        }
        for t in 0..m.index_count() / 3 {
            let vs: Vec<usize> = (0..3).map(|j| m.index(t * 3 + j) as usize).collect();
            let uv = vs.iter().fold([0f32; 2], |s, v| { let q = m.uv(*v); [s[0] + q[0] / 3.0, s[1] + q[1] / 3.0] });
            if ((uv[0] - u0).powi(2) + (uv[1] - v0).powi(2)).sqrt() < r {
                for v in &vs {
                    let p = xf(w, m.position(*v));
                    for i in 0..3 {
                        c[i] += p[i];
                    }
                    n += 1.0;
                }
            }
        }
    }
    // `@x,y,z` in place of the material: that world point as the centre
    if let Some(p) = a[1].strip_prefix('@') {
        let v: Vec<f32> = p.split(',').filter_map(|s| s.parse().ok()).collect();
        (c, n) = ([v[0], v[1], v[2]], 1.0);
    }
    if n == 0.0 {
        anyhow::bail!("no triangle of {} near uv ({u0}, {v0})", a[1]);
    }
    let c = [c[0] / n, c[1] / n, c[2] / n];
    println!("centre ({:.3}, {:.3}, {:.3}) from {} vertices", c[0], c[1], c[2], n);
    for (node, w) in &all {
        let NodeKind::Mesh(m) = &node.kind else { continue };
        let near: Vec<usize> = (0..m.index_count() / 3).filter(|t| {
            let p = xf(w, m.position(m.index(t * 3) as usize));
            ((p[0] - c[0]).powi(2) + (p[1] - c[1]).powi(2) + (p[2] - c[2]).powi(2)).sqrt() < dist
        }).collect();
        if !near.is_empty() {
            // the UV box of those triangles, and their mean UV area per world area
            let (mut lo, mut hi, mut ua, mut wa) = ([f32::MAX; 2], [f32::MIN; 2], 0f32, 0f32);
            // UVSPOT_TRIS=<material>: also each nearby triangle of it
            let each = std::env::var("UVSPOT_TRIS").is_ok_and(|s| s.eq_ignore_ascii_case(&mat(m)));
            for t in &near {
                let v: Vec<usize> = (0..3).map(|j| m.index(t * 3 + j) as usize).collect();
                let q: Vec<[f32; 2]> = v.iter().map(|i| m.uv(*i)).collect();
                let p: Vec<[f32; 3]> = v.iter().map(|i| xf(w, m.position(*i))).collect();
                if each {
                    println!("    uv ({:.3},{:.3}) ({:.3},{:.3}) ({:.3},{:.3})  at ({:.3},{:.3},{:.3})", q[0][0], q[0][1], q[1][0], q[1][1], q[2][0], q[2][1], p[0][0], p[0][1], p[0][2]);
                }
                for u in &q {
                    for k in 0..2 {
                        lo[k] = lo[k].min(u[k]);
                        hi[k] = hi[k].max(u[k]);
                    }
                }
                ua += ((q[1][0] - q[0][0]) * (q[2][1] - q[0][1]) - (q[2][0] - q[0][0]) * (q[1][1] - q[0][1])).abs() / 2.0;
                let (e1, e2) = ([p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]], [p[2][0] - p[0][0], p[2][1] - p[0][1], p[2][2] - p[0][2]]);
                let x = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
                wa += (x[0] * x[0] + x[1] * x[1] + x[2] * x[2]).sqrt() / 2.0;
            }
            println!("{:40} {:28} {:5} tris near  uv ({:.3},{:.3})..({:.3},{:.3})  uv/m {:.3}", node.name, mat(m), near.len(), lo[0], lo[1], hi[0], hi[1], (ua / wa.max(1e-9)).sqrt());
        }
    }
    Ok(())
}
