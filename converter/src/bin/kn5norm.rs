//! dev: mean world position and normal of mesh nodes by name, to see which
//! way a surface faces and how far apart two coincident ones sit.
//! `kn5norm <file.kn5> <node> [node...]`
use aceconv::ac1::kn5::{self, NodeKind};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let b = std::fs::read(&a[1]).unwrap();
    let k = kn5::parse(&b).unwrap();
    let want: Vec<String> = a[2..].iter().map(|s| s.to_lowercase()).collect();
    let id = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
    let mut all = Vec::new();
    k.root.walk(id, &mut all);
    for (n, w) in all {
        let NodeKind::Mesh(m) = &n.kind else { continue };
        if !want.contains(&n.name.to_lowercase()) {
            continue;
        }
        let (mut p, mut nr) = ([0f64; 3], [0f64; 3]);
        for v in 0..m.vertex_count {
            let q = kn5::xform(&w, m.position(v));
            let nn = m.normal(v);
            for i in 0..3 {
                p[i] += q[i] as f64;
                nr[i] += nn[i] as f64;
            }
        }
        let c = m.vertex_count.max(1) as f64;
        println!("{}: {} tris, mean pos ({:.4}, {:.4}, {:.4}), mean local normal ({:.3}, {:.3}, {:.3})",
            n.name, m.index_count() / 3, p[0] / c, p[1] / c, p[2] / c, nr[0] / c, nr[1] / c, nr[2] / c);
    }
}
