//! Save a square crop of an image file (as the converter decodes it) as a
//! 24-bit BMP, for looking at a texture's fine detail; `alpha` saves the
//! alpha channel as grey instead of the colour.
//!
//! `<image>` may name a texture inside a kn5: `model.kn5::Diffuser.dds`.
//!
//! cargo run --release --example texcrop -- <image> <out.bmp> <x> <y> <size> [alpha]

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let img = match a[0].split_once("::") {
        Some((kn5, name)) => {
            let b = std::fs::read(kn5)?;
            let k = aceconv::ac1::kn5::parse(&b)?;
            let t = k.textures.iter().find(|t| t.name.eq_ignore_ascii_case(name)).ok_or_else(|| anyhow::anyhow!("{name} is not in {kn5}"))?;
            aceconv::io::texture::decode_any(t.data)?
        }
        None => aceconv::io::texture::decode_any(&std::fs::read(&a[0])?)?,
    };
    let (x0, y0, n): (u32, u32, u32) = (a[2].parse()?, a[3].parse()?, a[4].parse()?);
    let alpha = a.get(5).map(|s| s == "alpha").unwrap_or(false);
    let n = n.min(img.w.saturating_sub(x0)).min(img.h.saturating_sub(y0));
    println!("{}x{}, crop {n} at ({x0}, {y0})", img.w, img.h);
    let row = (n * 3).div_ceil(4) * 4;
    let mut out = Vec::with_capacity(54 + (row * n) as usize);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&(54 + row * n).to_le_bytes());
    out.extend_from_slice(&[0, 0, 0, 0]);
    out.extend_from_slice(&54u32.to_le_bytes());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&n.to_le_bytes());
    out.extend_from_slice(&n.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&24u16.to_le_bytes());
    out.extend_from_slice(&[0u8; 24]);
    // BMP rows run bottom-up, BGR
    for y in (0..n).rev() {
        let start = out.len();
        for x in 0..n {
            let i = (((y0 + y) * img.w + x0 + x) * 4) as usize;
            let p = &img.px[i..i + 4];
            if alpha {
                out.extend_from_slice(&[p[3], p[3], p[3]]);
            } else {
                out.extend_from_slice(&[p[2], p[1], p[0]]);
            }
        }
        out.resize(start + row as usize, 0);
    }
    std::fs::write(&a[1], out)?;
    Ok(())
}
