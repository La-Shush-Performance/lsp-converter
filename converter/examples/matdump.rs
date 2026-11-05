//! Print an EVO `.material`: its shader, every property with its value
//! (zero ones left out unless `--all`) and every used texture slot. For
//! checking what a converted material really carries.
//!
//! cargo run --release --example matdump -- <file.material>... [--all] [--grep <substring>]

use aceconv::io::pb::{Msg, Val};

fn sub(v: &Val) -> Option<Msg> {
    match v {
        Val::Bytes(b) => Msg::parse(b).ok(),
        Val::Msg(m) => Some(m.clone()),
        _ => None,
    }
}

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let all = a.iter().any(|x| x == "--all");
    let grep = a.iter().position(|x| x == "--grep").and_then(|i| a.get(i + 1)).map(|s| s.to_lowercase());
    let files: Vec<&String> = a.iter().enumerate().filter(|(i, x)| !x.starts_with("--") && (*i == 0 || a[i - 1] != "--grep")).map(|(_, x)| x).collect();
    for f in files {
        let m = Msg::parse(&std::fs::read(f)?)?;
        println!("== {f}: {}", m.str(1).unwrap_or_default());
        for fl in &m.fields {
            let Some(p) = sub(&fl.val) else { continue };
            let name = p.str(1).unwrap_or_default();
            if grep.as_ref().map(|g| !name.to_lowercase().contains(g)).unwrap_or(false) {
                continue;
            }
            if fl.num == 4 {
                let v = p.sub(2).unwrap_or_default();
                let vals: Vec<f32> = match (1..=4u32).find(|n| v.sub(*n).is_some()) {
                    Some(n) if n >= 2 => { let c = v.sub(n).unwrap(); (1..=n).map(|k| c.f32(k).unwrap_or(0.0)).collect() }
                    _ => vec![v.f32(1).unwrap_or(0.0)],
                };
                if all || vals.iter().any(|x| *x != 0.0) {
                    println!("  {name} = {vals:?}");
                }
            } else if fl.num == 5 {
                let path = p.sub(2).and_then(|s| s.str(2)).unwrap_or_default();
                if all || !path.is_empty() {
                    println!("  [slot] {name} = {path}");
                }
            }
        }
    }
    Ok(())
}
