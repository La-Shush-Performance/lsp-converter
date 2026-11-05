//! The texcoord range of every car `.mesh` (LOD 0) in a package: per mesh
//! the min / max u and v, and the largest |u| or |v|. For checking whether
//! stock meshes ever carry UVs far outside 0..1 (AC1 mods do, e.g. v in
//! -11..-10) and what the converter writes.
//!
//! cargo run --release --example meshuv -- <package.kspkg> [path substring]

use aceconv::io::kspkg::Pkg;
use aceconv::io::pb::Msg;

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let pkg = Pkg::open(std::path::Path::new(&a[0]))?;
    let filt = a.get(1).map(|s| s.to_lowercase()).unwrap_or_else(|| "content\\cars\\".into());
    let mut worst = 0f32;
    for e in pkg.entries.iter().filter(|e| e.path.to_lowercase().ends_with(".mesh") && e.path.to_lowercase().contains(&filt)) {
        let Some(top) = pkg.read(e).ok().and_then(|b| Msg::parse(&b).ok()) else { continue };
        let Some(body) = top.subs(5).into_iter().next() else { continue };
        let uv: Vec<f32> = body.bytes(7).map(|b| b.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect()).unwrap_or_default();
        if uv.is_empty() {
            continue;
        }
        let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
        for c in uv.chunks_exact(2) {
            for k in 0..2 {
                lo[k] = lo[k].min(c[k]);
                hi[k] = hi[k].max(c[k]);
            }
        }
        let m = lo[0].abs().max(lo[1].abs()).max(hi[0].abs()).max(hi[1].abs());
        worst = worst.max(m);
        println!("{m:8.2}  u {:.2}..{:.2}  v {:.2}..{:.2}  {}", lo[0], hi[0], lo[1], hi[1], e.path);
    }
    println!("largest |uv| {worst:.2}");
    Ok(())
}
