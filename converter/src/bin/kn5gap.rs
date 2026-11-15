//! dev: how far a mesh sits off another surface - for each vertex of node A
//! (in kn5 A) the signed distance to the nearest triangle of node B (in kn5
//! B), along that triangle's face normal (positive = on its front side):
//! min / p10 / median / max in mm, over the vertices within 5 cm of B.
//! `kn5gap <a.kn5> <nodeA> <b.kn5> <nodeB>`
use aceconv::ac1::kn5::{self, NodeKind};

type V = [f32; 3];

fn sub(a: V, b: V) -> V { [a[0] - b[0], a[1] - b[1], a[2] - b[2]] }
fn dot(a: V, b: V) -> f32 { a[0] * b[0] + a[1] * b[1] + a[2] * b[2] }
fn cross(a: V, b: V) -> V { [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]] }

/// (vertices, triangles) of the named mesh nodes, in world space
fn mesh(path: &str, node: &str) -> (Vec<V>, Vec<[V; 3]>) {
    let b = std::fs::read(path).unwrap();
    let k = kn5::parse(&b).unwrap();
    let id = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
    let mut all = Vec::new();
    k.root.walk(id, &mut all);
    let (mut vs, mut ts) = (Vec::new(), Vec::new());
    for (n, w) in all {
        let NodeKind::Mesh(m) = &n.kind else { continue };
        if !n.name.eq_ignore_ascii_case(node) {
            continue;
        }
        let p: Vec<V> = (0..m.vertex_count).map(|v| kn5::xform(&w, m.position(v))).collect();
        for t in 0..m.index_count() / 3 {
            ts.push([p[m.index(t * 3) as usize], p[m.index(t * 3 + 1) as usize], p[m.index(t * 3 + 2) as usize]]);
        }
        vs.extend(p);
    }
    (vs, ts)
}

/// closest point on triangle t to p (Ericson, Real-Time Collision Detection 5.1.5)
fn closest(p: V, t: &[V; 3]) -> V {
    let (a, b, c) = (t[0], t[1], t[2]);
    let (ab, ac, ap) = (sub(b, a), sub(c, a), sub(p, a));
    let (d1, d2) = (dot(ab, ap), dot(ac, ap));
    if d1 <= 0.0 && d2 <= 0.0 { return a; }
    let bp = sub(p, b);
    let (d3, d4) = (dot(ab, bp), dot(ac, bp));
    if d3 >= 0.0 && d4 <= d3 { return b; }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 { let v = d1 / (d1 - d3); return [a[0] + ab[0] * v, a[1] + ab[1] * v, a[2] + ab[2] * v]; }
    let cp = sub(p, c);
    let (d5, d6) = (dot(ab, cp), dot(ac, cp));
    if d6 >= 0.0 && d5 <= d6 { return c; }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 { let w = d2 / (d2 - d6); return [a[0] + ac[0] * w, a[1] + ac[1] * w, a[2] + ac[2] * w]; }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        let bc = sub(c, b);
        return [b[0] + bc[0] * w, b[1] + bc[1] * w, b[2] + bc[2] * w];
    }
    let den = 1.0 / (va + vb + vc);
    let (v, w) = (vb * den, vc * den);
    [a[0] + ab[0] * v + ac[0] * w, a[1] + ab[1] * v + ac[1] * w, a[2] + ab[2] * v + ac[2] * w]
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (va, _) = mesh(&a[1], &a[2]);
    let (_, tb) = mesh(&a[3], &a[4]);
    println!("{} verts in {}, {} tris in {}", va.len(), a[2], tb.len(), a[4]);
    let mut d: Vec<f32> = va.iter().filter_map(|&p| {
        let (t, q) = tb.iter().map(|t| (t, closest(p, t))).min_by(|x, y| {
            let l = |q: V| { let s = sub(p, q); dot(s, s) };
            l(x.1).total_cmp(&l(y.1))
        })?;
        let s = sub(p, q);
        if dot(s, s).sqrt() > 0.05 {
            return None;
        }
        let n = cross(sub(t[1], t[0]), sub(t[2], t[0]));
        let len = dot(n, n).sqrt().max(1e-12);
        Some(dot(s, n) / len * 1000.0)
    }).collect();
    d.sort_by(f32::total_cmp);
    if d.is_empty() {
        println!("no vertex within 5 cm");
        return;
    }
    let q = |f: f32| d[((d.len() - 1) as f32 * f) as usize];
    println!("{} near verts: min {:.2} mm, p10 {:.2} mm, median {:.2} mm, max {:.2} mm", d.len(), q(0.0), q(0.1), q(0.5), q(1.0));
}
