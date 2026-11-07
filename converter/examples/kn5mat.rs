//! A kn5's materials with their shader, every property and every texture
//! slot, as AC1 reads them. An optional substring keeps only materials whose
//! name or shader matches. For checking what finish a mod gave a part.
//!
//! cargo run --release --example kn5mat -- <car.kn5> [substring]

use aceconv::ac1::kn5;

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let map = unsafe { memmap2::Mmap::map(&std::fs::File::open(&a[0])?)? };
    let k = kn5::parse(&map)?;
    let filt = a.get(1).map(|s| s.to_lowercase());
    for m in &k.materials {
        if filt.as_ref().is_some_and(|f| !m.name.to_lowercase().contains(f) && !m.shader.to_lowercase().contains(f)) {
            continue;
        }
        println!("{}  [{}] blend {} alpha-tested {}", m.name, m.shader, m.blend_mode, m.alpha_tested);
        for (n, p) in &m.props {
            println!("    {n} = {p:?}");
        }
        for (s, t) in &m.textures {
            println!("    [{s}] {t}");
        }
    }
    Ok(())
}
