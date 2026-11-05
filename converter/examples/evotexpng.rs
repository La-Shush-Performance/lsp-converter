//! Decode an unpacked EVO `.texture` (with its `.texturemips` beside it) to a
//! PNG: the largest mip with both sides at most `max` (default 1024). For
//! checking what a converted texture really holds; `alpha` saves the alpha
//! channel as grey instead of the colour.
//!
//! cargo run --release --example evotexpng -- <file.texture> <out.png> [max] [alpha]

use aceconv::stats::albedo::texture_mip;

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let header = std::fs::read(&a[0])?;
    let mips = std::fs::read(format!("{}mips", a[0]))?;
    let max: u32 = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(1024);
    let alpha = a.iter().any(|x| x == "alpha");
    let page = |i: usize| mips.get(i * 65536..((i + 1) * 65536).min(mips.len())).map(|s| s.to_vec());
    let (w, h, px, srgb) = texture_mip(&header, &page, max).ok_or_else(|| anyhow::anyhow!("cannot decode {}", a[0]))?;
    let raw: Vec<u8> = px.iter().flat_map(|c| if alpha { [c[3], c[3], c[3], 255] } else { [c[0], c[1], c[2], 255] }).collect();
    image::save_buffer(&a[1], &raw, w, h, image::ExtendedColorType::Rgba8)?;
    println!("{w}x{h} srgb {srgb}");
    Ok(())
}
