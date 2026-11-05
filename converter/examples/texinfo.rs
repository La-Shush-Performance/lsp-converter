//! A `.texture` header's size, mips, format and page table (first page and
//! page count per mip) beside its `.texturemips` length, for checking how a
//! texture's small mips are laid out. Takes unpacked `.texture` files.
//!
//! cargo run --release --example texinfo -- <file.texture>...

use aceconv::io::pb::Msg;

fn main() -> anyhow::Result<()> {
    for p in std::env::args().skip(1) {
        let h = Msg::parse(&std::fs::read(&p)?)?;
        let mips = std::fs::metadata(format!("{p}mips")).map(|m| m.len()).unwrap_or(0);
        let v = |m: &Msg, f: u32| m.fields.iter().filter(|x| x.num == f).map(|x| format!("{:?}", x.val)).collect::<Vec<_>>().join(",");
        let pt = h.subs(12).into_iter().next();
        println!("{p}\n  {}x{} mips {} fmt {} | mipsfile {} ({} pages)", v(&h, 1), v(&h, 2), v(&h, 3), v(&h, 4), mips, mips as f64 / 65536.0);
        if let Some(t) = pt {
            println!("  tile {}x{} f3 {} first [{}] count [{}] f6 {} f7 n={}", v(&t, 1), v(&t, 2), v(&t, 3), v(&t, 4), v(&t, 5), v(&t, 6), t.fields.iter().filter(|x| x.num == 7).count());
        }
    }
    Ok(())
}
