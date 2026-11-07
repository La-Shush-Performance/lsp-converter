//! What a texture holds under each mesh of a kn5: per mesh (optionally only
//! those of one material), the texture sampled at every triangle's centre UV,
//! weighted by the triangle's area on the car (what is seen), as p10 / median
//! / p90 per channel (0..1, raw). For telling which part of a shared texture a
//! part uses (an exhaust and side panels on one maps texture).
//!
//! cargo run --release --example meshtex -- <car.kn5> <texture name> [material]

use aceconv::ac1::kn5::{self, NodeKind};

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let map = unsafe { memmap2::Mmap::map(&std::fs::File::open(&a[0])?)? };
    let k = kn5::parse(&map)?;
    // every copy of the name, as AC1 lists them (the converter reads the active one)
    for t in k.textures.iter().filter(|t| t.name.eq_ignore_ascii_case(&a[1])) {
        let img = aceconv::io::texture::decode_any(t.data)?;
        let mu = aceconv::car::materials::mean(&img);
        println!("texture {} active {} {} bytes {}x{} mean {:.3} {:.3} {:.3} {:.3}", t.name, t.active, t.data.len(), img.w, img.h, mu[0], mu[1], mu[2], mu[3]);
    }
    let t = k.textures.iter().filter(|t| t.name.eq_ignore_ascii_case(&a[1])).max_by_key(|t| t.active).ok_or_else(|| anyhow::anyhow!("{} is not in {}", a[1], a[0]))?;
    let img = aceconv::io::texture::decode_any(t.data)?;
    let mut all = Vec::new();
    k.root.walk(kn5::IDENTITY, &mut all);
    for (node, _) in &all {
        let NodeKind::Mesh(m) = &node.kind else { continue };
        let mat = k.materials.get(m.material_id as usize).map(|x| x.name.clone()).unwrap_or_default();
        if a.get(2).is_some_and(|f| !mat.eq_ignore_ascii_case(f)) {
            continue;
        }
        // (weight, rgba) per triangle
        let mut s: Vec<(f32, [f32; 4])> = Vec::new();
        for tri in 0..m.index_count() / 3 {
            let vs: Vec<usize> = (0..3).map(|j| m.index(tri * 3 + j) as usize).collect();
            let q: Vec<[f32; 2]> = vs.iter().map(|v| m.uv(*v)).collect();
            let p: Vec<[f32; 3]> = vs.iter().map(|v| m.position(*v)).collect();
            let (e1, e2) = ([p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]], [p[2][0] - p[0][0], p[2][1] - p[0][1], p[2][2] - p[0][2]]);
            let cr = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
            let area = (cr[0] * cr[0] + cr[1] * cr[1] + cr[2] * cr[2]).sqrt() * 0.5;
            let (u, v) = ((q[0][0] + q[1][0] + q[2][0]) / 3.0, (q[0][1] + q[1][1] + q[2][1]) / 3.0);
            let x = ((u.rem_euclid(1.0) * img.w as f32) as u32).min(img.w - 1);
            let y = ((v.rem_euclid(1.0) * img.h as f32) as u32).min(img.h - 1);
            let i = ((y * img.w + x) * 4) as usize;
            let px = [0, 1, 2, 3].map(|c| img.px[i + c] as f32 / 255.0);
            s.push((area.max(1e-9), px));
        }
        if s.is_empty() {
            continue;
        }
        let total: f32 = s.iter().map(|x| x.0).sum();
        let pct = |c: usize, p: f32| -> f32 {
            let mut v: Vec<(f32, f32)> = s.iter().map(|x| (x.1[c], x.0)).collect();
            v.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut acc = 0.0;
            for (val, w) in &v {
                acc += w;
                if acc >= p * total {
                    return *val;
                }
            }
            v.last().map(|x| x.0).unwrap_or(0.0)
        };
        let ch = |c: usize| format!("{:.2}/{:.2}/{:.2}", pct(c, 0.1), pct(c, 0.5), pct(c, 0.9));
        println!("{:28} {:20} tris {:6}  R {}  G {}  B {}  A {}", node.name, mat, s.len(), ch(0), ch(1), ch(2), ch(3));
    }
    Ok(())
}
