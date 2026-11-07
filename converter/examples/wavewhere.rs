//! Where an AC1 bank's sample is used: the audio item, every instrument
//! that plays it (directly or in a multi / scatterer), the timeline, layout
//! or event holding that instrument, and the reader's notes on items it
//! does not model. For finding why a sound the mod plays is not converted.
//!
//! cargo run --release --example wavewhere -- <ac1 .bank> <sample name substring>

use aceconv::sound::ac1bank;
use aceconv::sound::bankread::Instrument;

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let bk = ac1bank::read(&std::fs::read(&a[0])?)?;
    let b = &bk.bank;
    let want = a.get(1).map(|s| s.to_lowercase()).unwrap_or_default();
    for n in &b.notes {
        println!("note: {n}");
    }
    let audio: Vec<_> = b.waves.iter().filter(|(_, ix)| b.samples.get(**ix as usize).is_some_and(|s| s.name.to_lowercase().contains(&want))).map(|(g, ix)| (*g, *ix)).collect();
    for (g, ix) in &audio {
        println!("audio {g} = sample {ix} {}", b.samples[*ix as usize].name);
    }
    let ids: Vec<_> = audio.iter().map(|x| x.0).collect();
    let mut insts = Vec::new();
    for (id, i) in &b.instruments {
        let hit = match i {
            Instrument::Wave { audio, .. } => ids.contains(audio),
            Instrument::Multi { sounds, .. } => sounds.iter().any(|(s, _)| matches!(b.instruments.get(s), Some(Instrument::Wave { audio, .. }) if ids.contains(audio)) || ids.contains(s)),
            Instrument::Scatter { sound, .. } => ids.contains(sound) || matches!(b.instruments.get(sound), Some(Instrument::Wave { audio, .. }) if ids.contains(audio)),
        };
        if hit {
            println!("instrument {id} {:?}", std::mem::discriminant(i));
            insts.push(*id);
        }
    }
    // wave instruments inside a multi / scatterer: the container is what is placed
    let mut placed = insts.clone();
    for (id, i) in &b.instruments {
        let inner = match i {
            Instrument::Multi { sounds, .. } => sounds.iter().any(|(s, _)| insts.contains(s)),
            Instrument::Scatter { sound, .. } => insts.contains(sound),
            _ => false,
        };
        if inner {
            println!("  inside container {id}");
            placed.push(*id);
        }
    }
    let pname = |g: &aceconv::sound::bankwrite::prj::Guid| b.params.get(g).map(|p| p.name.clone()).unwrap_or_else(|| g.to_string());
    for (ev, v) in &bk.event_insts {
        for i in v.iter().filter(|i| placed.contains(i)) {
            println!("event {ev} places {i}");
            if let Some(e) = b.events.get(ev) {
                let ps: Vec<String> = e.pinst.iter().map(|p| pname(&p.xor(ev))).collect();
                println!("  event parameters {ps:?}, {} instruments in all", v.len());
            }
            if let Some(ins) = b.instruments.get(i) {
                let x = match ins { Instrument::Wave { inst, .. } | Instrument::Multi { inst, .. } | Instrument::Scatter { inst, .. } => inst };
                println!("  volume {} pitch {} loops {} timeline {} ap {} root {} code {:02x?}", x.volume, x.pitch, x.loops, x.timeline, pname(&x.ap_param), x.ap_root, x.code);
                for p in &x.props {
                    println!("  prop {} curves {:?}", p.index, p.curves.iter().map(|c| bk.curve_param.get(c).map(|g| pname(g))).collect::<Vec<_>>());
                }
            }
        }
    }
    for t in b.timelines.values() {
        for (i, s, l) in t.insts.iter().filter(|x| placed.contains(&x.0)) {
            println!("timeline {} places {i} at {s} len {l}", t.id);
        }
    }
    for i in &placed {
        if let Some(sh) = bk.sheets.get(i) {
            println!("sheets of {i}: {sh:?}");
        }
    }
    if placed.is_empty() {
        println!("no instrument plays it");
    }
    // instruments no event places (with `--orphans`)
    if a.iter().any(|x| x == "--orphans") {
        let all: std::collections::HashSet<_> = bk.event_insts.values().flatten().copied().collect();
        for (id, i) in &b.instruments {
            if !all.contains(id) {
                let name = match i {
                    Instrument::Wave { audio, .. } => b.waves.get(audio).and_then(|ix| b.samples.get(*ix as usize)).map(|s| s.name.clone()).unwrap_or_default(),
                    _ => "(container)".into(),
                };
                println!("orphan {id} {name} sheets {:?}", bk.sheets.get(id));
            }
        }
        println!("{} events, {} instruments, {} audio, {} samples", b.events.len(), b.instruments.len(), b.waves.len(), b.samples.len());
    }
    Ok(())
}
