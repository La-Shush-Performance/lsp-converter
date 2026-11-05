//! Dev tool: the game's own schemas for car data.
//!   phys_json schema MESSAGE        - field numbers and names, nested messages expanded once
//!   phys_json json MESSAGE FILE     - decode FILE (a loose file, not an archive entry) as JSON
//!   phys_json strict MESSAGE FILE   - decode FILE strictly (as the SDK editor does) and report why it fails
use std::collections::BTreeSet;

use prost_reflect::{Kind, MessageDescriptor};

fn schema(d: &MessageDescriptor, indent: usize, seen: &mut BTreeSet<String>) {
    for f in d.fields() {
        let ty = match f.kind() {
            Kind::Message(m) => m.full_name().to_string(),
            Kind::Enum(e) => format!("enum {}", e.full_name()),
            k => format!("{k:?}").to_lowercase(),
        };
        let rep = if f.is_list() { "repeated " } else { "" };
        println!("{}{} {} : {rep}{ty}", "  ".repeat(indent), f.number(), f.name());
        if let Kind::Message(m) = f.kind() {
            if seen.insert(m.full_name().to_string()) {
                schema(&m, indent + 1, seen);
            }
        }
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    acecm::protos::extract(false);
    match a.get(1).map(|s| s.as_str()) {
        Some("schema") => {
            let d = acecm::protos::find(&a[2]).unwrap_or_else(|| panic!("{}", acecm::protos::missing(&a[2])));
            println!("{}", d.full_name());
            schema(&d, 1, &mut BTreeSet::new());
        }
        Some("json") => {
            let raw = std::fs::read(&a[3]).expect("read file");
            let m = acecm::protos::decode_lenient(&a[2], &raw).expect("decode");
            println!("{}", serde_json::to_string_pretty(&m).expect("json"));
        }
        Some("strict") => {
            let raw = std::fs::read(&a[3]).expect("read file");
            match acecm::protos::decode(&a[2], &raw) {
                Ok(m) => println!("ok: {} fields", m.fields().count()),
                Err(e) => println!("FAIL: {e}"),
            }
        }
        _ => eprintln!("usage: phys_json schema MESSAGE | json MESSAGE FILE | strict MESSAGE FILE"),
    }
}
