//! Print every stock car's sub-categories from the game's `system\cars.table`
//! (CarContentData f30: CarSubCategorizations { f1 CarSubCategoryEnum }), by
//! name, or (`--ac1`) the ones the converter writes into a mod's
//! `.moddedcarcontent` for AC1 cars (`identity::sub_categories`), to check the
//! converter against Kunos's own choices.
//!
//! cargo run --release --example carcats -- <content.kspkg> [id filter]
//! cargo run --release --example carcats -- --ac1 <ac1 car dir>...

use aceconv::ac1::car::Car;
use aceconv::car::identity::{category_name, sub_categories};
use aceconv::io::kspkg::Pkg;
use aceconv::io::pb::Msg;

fn names(v: &[u8]) -> String {
    v.iter().map(|c| category_name(*c).map(str::to_string).unwrap_or_else(|| c.to_string())).collect::<Vec<_>>().join(", ")
}

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    if a.get(1).map(String::as_str) == Some("--ac1") {
        for dir in &a[2..] {
            let p = std::path::Path::new(dir);
            let car = match Car::open(p) { Ok(c) => c, Err(e) => { println!("{}: (cannot open: {e})", p.display()); continue } };
            println!("{}: {}", p.file_name().map(|s| s.to_string_lossy()).unwrap_or_default(), names(&sub_categories(&car)));
        }
        return Ok(());
    }
    let pkg = Pkg::open(std::path::Path::new(&a[1]))?;
    let filt = a.get(2).map(|s| s.to_lowercase()).unwrap_or_default();
    let e = pkg.entries.iter().find(|e| e.path.eq_ignore_ascii_case("system\\cars.table")).ok_or_else(|| anyhow::anyhow!("no system\\cars.table"))?;
    let t = Msg::parse(&pkg.read(e)?)?;
    for list in t.subs(2) {
        for row in list.subs(3) {
            for car in row.subs(1) {
                let id = car.str(1).unwrap_or_default();
                if !id.to_lowercase().contains(&filt) {
                    continue;
                }
                let cats: Vec<u8> = car.subs(30).iter().flat_map(|c| c.bytes(1).map(|b| b.to_vec()).unwrap_or_default()).collect();
                println!("{id}: {}", names(&cats));
            }
        }
    }
    Ok(())
}
