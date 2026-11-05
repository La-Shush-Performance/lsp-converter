//! dev: every mesh node with its parent path, material, tris, active flags
//! kn5mesh <file.kn5> [bounds]: with `bounds`, also each mesh's world box (AC1 axes)
use aceconv::ac1::kn5::{self, Node, NodeKind};

fn walk(n: &Node, path: &str, world: [f32; 16], bounds: bool, k: &kn5::Kn5, out: &mut Vec<String>) {
    let p = if path.is_empty() { n.name.clone() } else { format!("{path}/{}", n.name) };
    let world = match &n.kind {
        NodeKind::Transform(m) => kn5::mul(m, &world),
        _ => world,
    };
    if let NodeKind::Mesh(m) = &n.kind {
        let mat = k.materials.get(m.material_id as usize).map(|x| x.name.as_str()).unwrap_or("?");
        let mut line = format!("{:6} tris  act={} rend={} lod={}/{}  mat={:24} {}", m.index_count() / 3, n.active as u8,
            m.renderable as u8, m.lod_in, m.lod_out, mat, p);
        if bounds && m.vertex_count > 0 {
            let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
            for v in 0..m.vertex_count {
                let q = kn5::xform(&world, m.position(v));
                for i in 0..3 {
                    lo[i] = lo[i].min(q[i]);
                    hi[i] = hi[i].max(q[i]);
                }
            }
            line += &format!("  box ({:.3}, {:.3}, {:.3})..({:.3}, {:.3}, {:.3})", lo[0], lo[1], lo[2], hi[0], hi[1], hi[2]);
        }
        out.push(line);
    }
    for c in &n.children {
        walk(c, &p, world, bounds, k, out);
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let f = std::fs::File::open(&a[1]).unwrap();
    let map = unsafe { memmap2::Mmap::map(&f).unwrap() };
    let k = kn5::parse(&map).unwrap();
    let bounds = a.get(2).is_some_and(|s| s == "bounds");
    let mut out = vec![];
    walk(&k.root, "", kn5::IDENTITY, bounds, &k, &mut out);
    for l in out {
        println!("{l}");
    }
}
