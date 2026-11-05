//! Survey the top-level fields of stock `.material` files (everything but the
//! shader name f1, the properties f4 and the texture slots f5) against
//! whether the material has an opacity map and its blendMode, for finding
//! what those fields switch. Prints one count per combination per shader.
//!
//! cargo run --release --example matflags -- <content.kspkg> [path filter]

use aceconv::io::kspkg::Pkg;
use aceconv::io::material::{get_prop, get_slot};
use aceconv::io::pb::{Msg, Val};
use std::collections::BTreeMap;

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let pkg = Pkg::open(std::path::Path::new(&a[1]))?;
    let filt = a.get(2).map(|s| s.to_lowercase()).unwrap_or_default();
    let mut counts: BTreeMap<String, (usize, String)> = BTreeMap::new();
    for e in pkg.entries.iter().filter(|e| e.path.to_lowercase().ends_with(".material") && e.path.to_lowercase().contains(&filt)) {
        let Some(m) = pkg.read(e).ok().and_then(|b| Msg::parse(&b).ok()) else { continue };
        let top: Vec<String> = m.fields.iter().filter(|f| !matches!(f.num, 1 | 4 | 5)).map(|f| match &f.val {
            Val::Varint(v) => format!("f{}={v}", f.num),
            Val::F32(b) => format!("f{}={}", f.num, f32::from_le_bytes(*b)),
            _ => format!("f{}=?", f.num),
        }).collect();
        let opac = [("Base", "Base_HasOpacityMap"), ("Red", "Red_HasOpacityMap"), ("Green", "Green_HasOpacityMap"), ("Blue", "Blue_HasOpacityMap")]
            .iter().filter(|(l, p)| get_prop(&m, p, 1).is_some_and(|v| v[0] > 0.5) && get_slot(&m, &format!("{l}_OpacityMap")).is_some_and(|s| !s.is_empty()))
            .map(|x| x.0).collect::<Vec<_>>().join("+");
        let blend = get_prop(&m, "blendMode", 1).map(|v| v[0]).unwrap_or(-1.0);
        let key = format!("{:24} opac[{:10}] blendMode {:4} | {}", m.str(1).unwrap_or_default(), opac, blend, top.join(" "));
        let c = counts.entry(key).or_insert((0, e.path.clone()));
        c.0 += 1;
    }
    for (k, (n, ex)) in counts {
        println!("{n:5}  {k}   e.g. {ex}");
    }
    Ok(())
}
