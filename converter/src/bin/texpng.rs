//! dev: an EVO `.texture` + `.texturemips` (BC7 / BC1 / BC4) -> PNG of one
//! mip, to see what the game is given. `texpng <x.texture> <out.png> [mip]`
use aceconv::io::pb::{self, Msg};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let raw = std::fs::read(&a[1]).unwrap();
    // an AC1 source image (DDS / PNG / ...): its top mip, as the converter reads it
    if raw.starts_with(b"DDS ") || !a[1].to_lowercase().ends_with(".texture") {
        let img = aceconv::io::texture::decode_any(&raw).unwrap();
        image::RgbaImage::from_raw(img.w, img.h, img.px).unwrap().save(&a[2]).unwrap();
        println!("source -> {}", a[2]);
        return;
    }
    let hdr = Msg::parse(&raw).unwrap();
    let mips = std::fs::read(format!("{}mips", a[1])).unwrap();
    let want: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
    let (w0, h0) = (hdr.varint(1).unwrap() as u32, hdr.varint(2).unwrap() as u32);
    let fmt = hdr.varint(4).unwrap();
    let pt = hdr.sub(12).unwrap();
    let (tw, th) = (pt.varint(1).unwrap() as u32, pt.varint(2).unwrap() as u32);
    let first = pb::varints(pt.bytes(4).unwrap());
    let (bpb, bc) = match fmt {
        33 | 34 | 26 => (16usize, "bc7"),
        12 | 13 => (8, "bc1"),
        27 => (8, "bc4"),
        x => panic!("format {x} not handled"),
    };
    let (w, h) = ((w0 >> want).max(1), (h0 >> want).max(1));
    let (bw, bh) = (w.div_ceil(4) as usize, h.div_ceil(4) as usize);
    let (tbw, tbh) = ((tw / 4) as usize, (th / 4) as usize);
    let cols = w.div_ceil(tw).max(1) as usize;
    let page0 = first[want] as usize;
    // de-tile into contiguous rows of blocks
    let mut blocks = vec![0u8; bw * bh * bpb];
    for by in 0..bh {
        for bx in 0..bw {
            let (tx, ty) = (bx / tbw, by / tbh);
            let page = page0 + ty * cols + tx;
            let src = page * 65536 + ((by % tbh) * tbw + (bx % tbw)) * bpb;
            let dst = (by * bw + bx) * bpb;
            blocks[dst..dst + bpb].copy_from_slice(&mips[src..src + bpb]);
        }
    }
    let (pw, ph) = (bw * 4, bh * 4);
    let mut words = vec![0u32; pw * ph];
    match bc {
        "bc7" => texture2ddecoder::decode_bc7(&blocks, pw, ph, &mut words).unwrap(),
        "bc1" => texture2ddecoder::decode_bc1(&blocks, pw, ph, &mut words).unwrap(),
        _ => texture2ddecoder::decode_bc4(&blocks, pw, ph, &mut words).unwrap(),
    }
    let mut img = image::RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let v = words[y as usize * pw + x as usize];
            img.put_pixel(x, y, image::Rgba([(v >> 16) as u8, (v >> 8) as u8, v as u8, 255]));
        }
    }
    img.save(&a[2]).unwrap();
    println!("{w0}x{h0} fmt {fmt}, mip {want}: {w}x{h} -> {}", a[2]);
}
