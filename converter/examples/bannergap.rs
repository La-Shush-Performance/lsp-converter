//! How far a car's EXT_BANNER* sits off its EXT_WINDOWS glass: per banner
//! vertex near the glass (within 5 cm of a glass vertex), the offset along
//! that glass vertex's normal (+ outside the glass). Stock cars from
//! content.kspkg (path filter), or a converted car package.
//!
//! With a third argument (`int`), also each banner vertex's offset from the
//! interior mesh's INT_WINDSHIELD (+ banner outside it).
//!
//! cargo run --release --example bannergap -- <pkg.kspkg> [path filter] [int]

use aceconv::io::kspkg::Pkg;
use aceconv::io::pb::{varints, Msg};

fn f32v(b: Option<&[u8]>) -> Vec<f32> {
    b.map(|b| b.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect()).unwrap_or_default()
}

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let pkg = Pkg::open(std::path::Path::new(&a[0]))?;
    let filt = a.get(1).map(|s| s.to_lowercase()).unwrap_or_default();
    for e in pkg.entries.iter().filter(|e| e.path.to_lowercase().ends_with("_exterior.mesh") && e.path.to_lowercase().contains(&filt)) {
        let Ok(top) = pkg.read(e).and_then(|b| Msg::parse(&b)) else { continue };
        let Some(lod) = top.subs(5).into_iter().next() else { continue };
        let (pos, nrm) = (f32v(lod.bytes(5)), f32v(lod.bytes(6)));
        let idx = lod.bytes(11).map(varints).unwrap_or_default();
        let verts_of = |pre: &str| -> Vec<usize> {
            let mut v: Vec<usize> = lod.subs(4).iter().filter(|s| s.str(1).is_some_and(|n| n.starts_with(pre))).flat_map(|s| {
                let (st, n) = (s.varint(2).unwrap_or(0) as usize, s.varint(3).unwrap_or(0) as usize);
                idx.get(st..st + n).unwrap_or(&[]).iter().map(|&x| x as usize).collect::<Vec<_>>()
            }).collect();
            v.sort_unstable();
            v.dedup();
            v
        };
        let (ban, gl) = (verts_of("EXT_BANNER"), verts_of("EXT_WINDOWS"));
        if ban.is_empty() || gl.is_empty() {
            continue;
        }
        let p = |v: usize| [pos[v * 3], pos[v * 3 + 1], pos[v * 3 + 2]];
        let mut gaps = Vec::new();
        for &b in &ban {
            let pb = p(b);
            let mut best = (f32::MAX, 0usize);
            for &g in &gl {
                let q = p(g);
                let d = (pb[0] - q[0]).powi(2) + (pb[1] - q[1]).powi(2) + (pb[2] - q[2]).powi(2);
                if d < best.0 {
                    best = (d, g);
                }
            }
            if best.0.sqrt() < 0.05 {
                let (q, g) = (p(best.1), best.1);
                let n = [nrm[g * 3], nrm[g * 3 + 1], nrm[g * 3 + 2]];
                gaps.push((pb[0] - q[0]) * n[0] + (pb[1] - q[1]) * n[1] + (pb[2] - q[2]) * n[2]);
            }
        }
        if gaps.is_empty() {
            println!("{}: banner not on the glass ({} banner verts)", e.path, ban.len());
            continue;
        }
        gaps.sort_by(|x, y| x.partial_cmp(y).unwrap());
        let q = |f: f64| gaps[((gaps.len() - 1) as f64 * f) as usize] * 1000.0;
        println!("{}: {} of {} banner verts on the glass, offset mm p10 {:.2} p50 {:.2} p90 {:.2}", e.path, gaps.len(), ban.len(), q(0.1), q(0.5), q(0.9));
        if a.get(2).map(|s| s.as_str()) == Some("int") {
            let ip = e.path.replace("_exterior.mesh", "_interior.mesh");
            let Some(ie) = pkg.entries.iter().find(|x| x.path == ip) else { continue };
            let Ok(it) = pkg.read(ie).and_then(|b| Msg::parse(&b)) else { continue };
            let Some(il) = it.subs(5).into_iter().next() else { continue };
            let (ipos, inrm) = (f32v(il.bytes(5)), f32v(il.bytes(6)));
            let iidx = il.bytes(11).map(varints).unwrap_or_default();
            for sec in il.subs(4).iter().filter(|s| s.str(1).is_some_and(|n| n.starts_with("INT_WIND"))) {
                let (st, n) = (sec.varint(2).unwrap_or(0) as usize, sec.varint(3).unwrap_or(0) as usize);
                let mut iv: Vec<usize> = iidx.get(st..st + n).unwrap_or(&[]).iter().map(|&x| x as usize).collect();
                iv.sort_unstable();
                iv.dedup();
                let mut g2 = Vec::new();
                let mut dots = 0f64;
                for &b in &ban {
                    let pb = p(b);
                    let mut best = (f32::MAX, 0usize);
                    for &g in &iv {
                        let q = [ipos[g * 3], ipos[g * 3 + 1], ipos[g * 3 + 2]];
                        let d = (pb[0] - q[0]).powi(2) + (pb[1] - q[1]).powi(2) + (pb[2] - q[2]).powi(2);
                        if d < best.0 { best = (d, g); }
                    }
                    if best.0.sqrt() < 0.05 {
                        let g = best.1;
                        let q = [ipos[g * 3], ipos[g * 3 + 1], ipos[g * 3 + 2]];
                        let nn = [inrm[g * 3], inrm[g * 3 + 1], inrm[g * 3 + 2]];
                        // + : banner on the side the interior glass's normal points to
                        g2.push((pb[0] - q[0]) * nn[0] + (pb[1] - q[1]) * nn[1] + (pb[2] - q[2]) * nn[2]);
                        dots += (nrm[b * 3] * nn[0] + nrm[b * 3 + 1] * nn[1] + nrm[b * 3 + 2] * nn[2]) as f64;
                    }
                }
                if g2.is_empty() { println!("  {}: not near the banner", sec.str(1).unwrap_or_default()); continue; }
                g2.sort_by(|x, y| x.partial_cmp(y).unwrap());
                let q2 = |f: f64| g2[((g2.len() - 1) as f64 * f) as usize] * 1000.0;
                println!("  {}: {} banner verts near it, offset along its normal mm p10 {:.2} p50 {:.2} p90 {:.2}, mean normal dot banner {:.2}", sec.str(1).unwrap_or_default(), g2.len(), q2(0.1), q2(0.5), q2(0.9), dots / g2.len() as f64);
            }
        }
    }
    Ok(())
}
