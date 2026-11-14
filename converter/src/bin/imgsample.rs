//! dev: mean sRGB (0..255) of rectangles of an image (PNG / JPG / DDS), to
//! read colours off a render. `imgsample <image> x0,y0,x1,y1 ...`
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let img = aceconv::io::texture::decode_any(&std::fs::read(&a[1]).unwrap()).unwrap();
    println!("{}x{}", img.w, img.h);
    // `blue` / `white` / `red`: the mean of the image's pixels of that kind
    for (name, pick) in [
        ("blue", Box::new(|c: &[u8]| c[2] as i32 > c[0] as i32 + 50 && c[2] as i32 > c[1] as i32 + 15) as Box<dyn Fn(&[u8]) -> bool>),
        ("white", Box::new(|c: &[u8]| c[0] > 200 && c[1] > 200 && c[2] > 200)),
        ("red", Box::new(|c: &[u8]| c[0] as i32 > c[1] as i32 + 90 && c[0] as i32 > c[2] as i32 + 70)),
    ] {
        if !a[2..].iter().any(|x| x == name) {
            continue;
        }
        let px: Vec<&[u8]> = img.px.chunks_exact(4).filter(|c| pick(c)).collect();
        let n = px.len().max(1) as f64;
        let m: Vec<f64> = (0..3).map(|k| px.iter().map(|c| c[k] as f64).sum::<f64>() / n).collect();
        println!("{name}: {} px, mean {:.0} {:.0} {:.0}", px.len(), m[0], m[1], m[2]);
    }
    for r in a[2..].iter().filter(|x| x.contains(',')) {
        let v: Vec<u32> = r.split(',').map(|s| s.trim().parse().unwrap()).collect();
        let (mut s, mut n) = ([0f64; 3], 0f64);
        for y in v[1]..v[3].min(img.h) {
            for x in v[0]..v[2].min(img.w) {
                let i = ((y * img.w + x) * 4) as usize;
                for k in 0..3 {
                    s[k] += img.px[i + k] as f64;
                }
                n += 1.0;
            }
        }
        println!("{r}: {:.0} {:.0} {:.0}", s[0] / n, s[1] / n, s[2] / n);
    }
}
