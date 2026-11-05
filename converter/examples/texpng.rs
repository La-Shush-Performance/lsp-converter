//! Save an image file (as the converter decodes it) as a PNG scaled down to
//! fit `max` pixels, for looking at a whole livery; `alpha` saves the alpha
//! channel as grey instead of the colour. Also prints its size and mean.
//!
//! `<image>` may name a texture inside a kn5: `model.kn5::Body.dds`.
//!
//! cargo run --release --example texpng -- <image> <out.png> [max] [alpha]

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
    let max: u32 = a.get(2).map(|s| s.parse()).transpose()?.unwrap_or(1024);
    let alpha = a.get(3).map(|s| s == "alpha").unwrap_or(false);
    let m = aceconv::car::materials::mean(&img);
    println!("{}x{} mean rgba {:.3} {:.3} {:.3} {:.3}", img.w, img.h, m[0], m[1], m[2], m[3]);
    let img = aceconv::io::texture::resize_to(&img, max, true);
    let px: Vec<u8> = img.px.chunks_exact(4).flat_map(|p| if alpha { [p[3], p[3], p[3], 255] } else { [p[0], p[1], p[2], 255] }).collect();
    image::RgbaImage::from_raw(img.w, img.h, px).ok_or_else(|| anyhow::anyhow!("bad size"))?.save(&a[1])?;
    Ok(())
}
