//! Score every `.texture` under an unpacked package folder for colourful
//! noise (mean saturation x mean neighbour hue change, at most 256 a side),
//! highest first, for finding which texture shows as rainbow speckle.
//!
//! cargo run --release --example texnoise -- <unpacked dir> [top]

use aceconv::stats::albedo::texture_mip;

fn walk(d: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    for e in std::fs::read_dir(d).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().is_some_and(|x| x == "texture") {
            out.push(p);
        }
    }
}

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let top: usize = a.get(1).and_then(|s| s.parse().ok()).unwrap_or(25);
    let mut files = Vec::new();
    walk(std::path::Path::new(&a[0]), &mut files);
    let mut scores = Vec::new();
    for f in files {
        let (Ok(h), Ok(mips)) = (std::fs::read(&f), std::fs::read(format!("{}mips", f.display()))) else { continue };
        let page = |i: usize| mips.get(i * 65536..((i + 1) * 65536).min(mips.len())).map(|s| s.to_vec());
        let Some((w, ht, px, _)) = texture_mip(&h, &page, 256) else { continue };
        let sat = |c: [u8; 4]| { let (mx, mn) = (c[0].max(c[1]).max(c[2]) as f32, c[0].min(c[1]).min(c[2]) as f32); if mx > 0.0 { (mx - mn) / mx } else { 0.0 } };
        let (mut s, mut d, mut n) = (0f32, 0f32, 0f32);
        for y in 0..ht.saturating_sub(1) {
            for x in 0..w.saturating_sub(1) {
                let c = px[(y * w + x) as usize];
                let r = px[(y * w + x + 1) as usize];
                s += sat(c);
                // chroma change to the right neighbour (rgb minus its grey)
                let g = |c: [u8; 4]| { let m = (c[0] as f32 + c[1] as f32 + c[2] as f32) / 3.0; [c[0] as f32 - m, c[1] as f32 - m, c[2] as f32 - m] };
                let (gc, gr) = (g(c), g(r));
                d += ((gc[0] - gr[0]).abs() + (gc[1] - gr[1]).abs() + (gc[2] - gr[2]).abs()) / 255.0;
                n += 1.0;
            }
        }
        if n > 0.0 {
            scores.push((s / n * d / n, s / n, d / n, f.display().to_string()));
        }
    }
    scores.sort_by(|x, y| y.0.total_cmp(&x.0));
    for (sc, s, d, f) in scores.into_iter().take(top) {
        println!("{sc:.4}  sat {s:.2}  chroma-step {d:.3}  {f}");
    }
    Ok(())
}
