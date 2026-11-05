//! Whether an AC1 author LOD shares LOD0's unwrap: per material, the
//! `geometry::uv_mismatch` of the LOD's vertices against LOD0's (both in
//! AC1 world space). The converter swaps a section whose mismatch is over
//! `body::LOD_UV_MISMATCH` for simplified LOD0.
//!
//! cargo run --release --example loduv -- <lod0.kn5> <lod_b.kn5>

use aceconv::car::geometry::{self, Geo};
use std::collections::BTreeMap;

fn by_material(path: &str) -> anyhow::Result<BTreeMap<String, Geo>> {
    let b = std::fs::read(path)?;
    let k = aceconv::ac1::kn5::parse(&b)?;
    let mut notes = Vec::new();
    let mut out: BTreeMap<String, Geo> = BTreeMap::new();
    for i in geometry::flatten(&k, &mut notes).iter().filter(|i| !i.cockpit) {
        let name = k.materials.get(i.material).map(|m| m.name.clone()).unwrap_or_default();
        geometry::append(out.entry(name).or_default(), i.mesh, &i.world, 0);
    }
    Ok(out)
}

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let (l0, ln) = (by_material(&a[0])?, by_material(&a[1])?);
    for (name, g) in &ln {
        let Some(b) = l0.get(name) else {
            println!("{name:40} not in LOD0");
            continue;
        };
        match geometry::uv_mismatch(b, g) {
            Some(d) => println!("{name:40} {d:.4}  ({} / {} tris)", g.tris(), b.tris()),
            None => println!("{name:40} too few shared places"),
        }
    }
    Ok(())
}
