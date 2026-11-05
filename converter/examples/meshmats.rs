//! Every mesh of a kn5 with its material, shader, flags, triangle count and
//! world position, for finding which material a part seen in game uses.
//! An optional substring keeps only meshes whose name or material matches.
//!
//! cargo run --release --example meshmats -- <car.kn5> [substring]

use aceconv::ac1::kn5::{self, NodeKind};

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let map = unsafe { memmap2::Mmap::map(&std::fs::File::open(&a[0])?)? };
    let k = kn5::parse(&map)?;
    let filt = a.get(1).map(|s| s.to_lowercase());
    let mut all = Vec::new();
    k.root.walk(kn5::IDENTITY, &mut all);
    for (n, w) in all {
        let NodeKind::Mesh(m) = &n.kind else { continue };
        let mat = k.materials.get(m.material_id as usize);
        let mn = mat.map(|x| x.name.as_str()).unwrap_or("?");
        if filt.as_ref().is_some_and(|f| !n.name.to_lowercase().contains(f) && !mn.to_lowercase().contains(f)) {
            continue;
        }
        println!("{:40} {:28} {:28} blend {} at {} tris {:6} pos ({:.2},{:.2},{:.2})", n.name, mn, mat.map(|x| x.shader.as_str()).unwrap_or("?"),
            mat.map(|x| x.blend_mode).unwrap_or(0), mat.map(|x| x.alpha_tested as u8).unwrap_or(0), m.index_count() / 3, w[12], w[13], w[14]);
    }
    Ok(())
}
