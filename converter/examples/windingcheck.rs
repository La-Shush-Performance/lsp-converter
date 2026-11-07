//! Per kn5 material: how many triangles have stored normals agreeing with
//! their winding normal (b - a) x (c - a), and the mean stored normal (kn5
//! axes, mesh-local). For telling which side a mod's glass is drawn on.
//!
//! cargo run --release --example windingcheck -- <car.kn5> [material substring]

use aceconv::ac1::kn5::{self, Node, NodeKind};
use std::collections::BTreeMap;

fn walk<'a>(n: &'a Node<'a>, out: &mut Vec<&'a kn5::Mesh<'a>>) {
    if let NodeKind::Mesh(m) = &n.kind {
        out.push(m);
    }
    for c in &n.children {
        walk(c, out);
    }
}

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let map = unsafe { memmap2::Mmap::map(&std::fs::File::open(&a[0])?)? };
    let k = kn5::parse(&map)?;
    let filt = a.get(1).map(|s| s.to_lowercase()).unwrap_or_default();
    let mut meshes = Vec::new();
    walk(&k.root, &mut meshes);
    let mut stat: BTreeMap<String, (usize, usize, [f64; 3])> = BTreeMap::new();
    for m in meshes {
        let name = k.materials.get(m.material_id as usize).map(|x| x.name.clone()).unwrap_or_default();
        if !name.to_lowercase().contains(&filt) {
            continue;
        }
        let e = stat.entry(name).or_insert((0, 0, [0.0; 3]));
        for t in 0..m.index_count() / 3 {
            let v: Vec<usize> = (0..3).map(|i| m.index(t * 3 + i) as usize).collect();
            let (p0, p1, p2) = (m.position(v[0]), m.position(v[1]), m.position(v[2]));
            let u = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
            let w = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
            let f = [u[1] * w[2] - u[2] * w[1], u[2] * w[0] - u[0] * w[2], u[0] * w[1] - u[1] * w[0]];
            let mut sn = [0f32; 3];
            for &i in &v {
                let n = m.normal(i);
                for c in 0..3 {
                    sn[c] += n[c];
                }
            }
            e.0 += 1;
            if f[0] * sn[0] + f[1] * sn[1] + f[2] * sn[2] > 0.0 {
                e.1 += 1;
            }
            for c in 0..3 {
                e.2[c] += sn[c] as f64;
            }
        }
    }
    for (n, (t, agree, s)) in stat {
        let l = (s[0] * s[0] + s[1] * s[1] + s[2] * s[2]).sqrt().max(1e-9);
        println!("{n:32} tris {t:6}  normals agree with winding {:5.1}%  mean normal ({:.2}, {:.2}, {:.2})", 100.0 * agree as f64 / t.max(1) as f64, s[0] / l, s[1] / l, s[2] / l);
    }
    Ok(())
}
