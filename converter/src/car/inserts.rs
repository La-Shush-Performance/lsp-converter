//! CSP model changes per skin: `[MODEL_REPLACEMENT_...]` sections in a
//! skin's `ext_config.ini` (or the car's own CSP config, for every skin)
//! that INSERT a small kn5 into the car (INSERT_AFTER a node: beside it,
//! INSERT_IN a node: under it) or HIDE nodes of it. RSS's GTM Mercer
//! Verstappen skins add a steering-wheel hub decal, a dashboard plate and
//! LED windscreen banners this way, and hide the stock strips and banners;
//! converted without them the wheel hub was a bare black disc.
//!
//! EVO's car body is one mesh for every livery, and a livery only swaps its
//! section materials; nothing a livery's material does hides geometry (a
//! hidden banner drawn with a wholly transparent cut-out, coloured red as a
//! test, showed solid red fighting the banner its skin inserts in the same
//! place). What a livery adds or leaves out is a car part, as on stock cars
//! (the MX-5's rollbar, the Camaro's fuel cap): its own mesh hung on a bone
//! of the car, which a livery's visual preset lists. So:
//! - every distinct insert (file content + place) is merged into LOD0 once
//!   under a `CSP_INSERT_<file>` node, its materials appended (same names,
//!   so CSP material rules still find them) and its textures added (renamed
//!   only when a name is taken by a different image);
//! - `assign` takes out of the body every mesh of an insert, and every mesh
//!   a skin hides, and groups them into parts by the skins that show them
//!   and the bone they hang on (`parts` writes them).
//!
//! Only the LOD0 model (the inserts' other FILE entries are author LODs;
//! hidden nodes stay out of the author LODs too). EMISSIVE_LIGHT and other
//! per-skin CSP effects are not converted.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::ac1::car::Car;
use crate::ac1::kn5::{self, Kn5, Node, NodeKind};
use crate::ac1::kn5_protection::{self, Kn5Bytes};
use crate::car::fx::CspSection;
use crate::car::geometry::{Inst, Under};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Place {
    /// beside the node, after it
    After(String),
    /// under the node
    In(String),
}

/// One distinct insert and the skins that use it.
pub struct Insert {
    pub file: PathBuf,
    pub place: Place,
    pub skins: BTreeSet<String>,
    bytes: Kn5Bytes,
}

/// What the skins' CSP configs ask of the model.
#[derive(Default)]
pub struct Changes {
    pub inserts: Vec<Insert>,
    /// skin folder -> node names it hides
    pub hides: Vec<(String, Vec<String>)>,
    pub notes: Vec<String>,
}

/// What `apply` did to the model, for `assign`.
#[derive(Default, Debug)]
pub struct Plan {
    /// every skin folder
    pub all: BTreeSet<String>,
    /// an insert's root node (lower case) -> the skins that show it
    pub inserts: Vec<(String, BTreeSet<String>)>,
    /// a hidden node name (lower case) -> the skins that hide it
    pub hides: HashMap<String, BTreeSet<String>>,
}

/// The meshes of one car part: the skins that show it, the bone it hangs on.
#[derive(Debug)]
pub struct PartSet {
    pub shown: BTreeSet<String>,
    /// CAR_BODY, COCKPIT_HR or STEER_BASE
    pub bone: &'static str,
    pub insts: Vec<usize>,
    /// what it is, for the notes
    pub what: String,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    Insert(usize),
    /// the skins that hide it
    Hidden(BTreeSet<String>),
}

/// Every instance an insert brought or a skin hides, out of the body and into
/// parts: (parts, the instance paths taken (lower case), notes). A mesh every
/// skin hides is taken and in no part. Parts under a wheel stay in the body.
pub fn assign(plan: &Plan, insts: &[Inst]) -> (Vec<PartSet>, HashSet<String>, Vec<String>) {
    let mut groups: BTreeMap<(Key, &'static str), Vec<usize>> = BTreeMap::new();
    let mut taken = HashSet::new();
    let mut notes = Vec::new();
    let mut gone = BTreeSet::new();
    for (ii, i) in insts.iter().enumerate() {
        let segs: Vec<String> = i.path.split('/').map(|s| s.to_lowercase()).collect();
        let key = if let Some(n) = plan.inserts.iter().position(|(root, _)| segs.contains(root)) {
            Key::Insert(n)
        } else {
            let hiding: BTreeSet<String> = segs.iter().filter_map(|s| plan.hides.get(s)).flatten().cloned().collect();
            if hiding.is_empty() {
                continue;
            }
            Key::Hidden(hiding)
        };
        if i.under != Under::Body {
            notes.push(format!("csp: {} hangs on a wheel - kept on every skin", i.node));
            continue;
        }
        taken.insert(i.path.to_lowercase());
        if let Key::Hidden(h) = &key {
            if plan.all.is_subset(h) {
                gone.insert(i.node.clone());
                continue;
            }
        }
        let bone = if i.steer { "STEER_BASE" } else if i.cockpit { crate::car::body::INT_ROOT } else { crate::car::body::EXT_ROOT };
        groups.entry((key, bone)).or_default().push(ii);
    }
    if !gone.is_empty() {
        notes.push(format!("csp: hidden on every skin, left out: {}", gone.into_iter().collect::<Vec<_>>().join(", ")));
    }
    let parts = groups.into_iter().map(|((key, bone), insts_of)| {
        let (shown, what) = match &key {
            Key::Insert(n) => (plan.inserts[*n].1.clone(), plan.inserts[*n].0.trim_start_matches("csp_insert_").to_string()),
            Key::Hidden(h) => (plan.all.difference(h).cloned().collect(), format!("the stock parts {} hide{}", h.iter().cloned().collect::<Vec<_>>().join(", "), if h.len() == 1 { "s" } else { "" })),
        };
        PartSet { shown, bone, insts: insts_of, what }
    }).collect();
    (parts, taken, notes)
}

fn list(v: &str) -> Vec<String> {
    v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

/// The MODEL_REPLACEMENT sections of a CSP config that change `model` (the
/// LOD0 file name): (insert file, place, hidden nodes).
fn replacements(secs: &[CspSection], model: &str) -> Vec<(Option<(String, Place)>, Vec<String>)> {
    let mut out = Vec::new();
    for s in secs.iter().filter(|s| s.name.to_uppercase().starts_with("MODEL_REPLACEMENT")) {
        if s.get("ACTIVE").map(|a| a.trim() == "0").unwrap_or(false) {
            continue;
        }
        if !s.get("FILE").map(|f| list(f).iter().any(|x| x.eq_ignore_ascii_case(model))).unwrap_or(false) {
            continue;
        }
        let hide = s.get("HIDE").map(list).unwrap_or_default();
        let insert = s.get("INSERT").map(str::trim).filter(|f| !f.is_empty()).and_then(|f| {
            let place = s.get("INSERT_AFTER").map(str::trim).filter(|n| !n.is_empty()).map(|n| Place::After(n.into()))
                .or_else(|| s.get("INSERT_IN").map(str::trim).filter(|n| !n.is_empty()).map(|n| Place::In(n.into())))?;
            Some((f.to_string(), place))
        });
        if insert.is_some() || !hide.is_empty() {
            out.push((insert, hide));
        }
    }
    out
}

fn hash(b: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    b.hash(&mut h);
    h.finish()
}

/// Read every skin's CSP model changes (the car's own CSP config applies to
/// every skin). An insert file is looked up in the skin folder, then the
/// car folder.
pub fn read(car: &Car) -> Changes {
    let mut ch = Changes::default();
    let model = car.kn5_path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
    let car_wide = crate::car::fx::csp_config(car).map(|(_, s)| replacements(&s, &model)).unwrap_or_default();
    let mut by_key: HashMap<(u64, Place), usize> = HashMap::new();
    let mut hides: HashMap<String, Vec<String>> = HashMap::new();
    for skin in &car.skins {
        let dir = car.skin_dir(skin);
        let own = crate::ac1::car::find_ci(&dir, "ext_config.ini").and_then(|p| std::fs::read(p).ok())
            .map(|b| replacements(&crate::car::fx::parse_csp(&String::from_utf8_lossy(&b)), &model)).unwrap_or_default();
        for (insert, hide) in car_wide.iter().chain(own.iter()) {
            hides.entry(skin.clone()).or_default().extend(hide.iter().cloned());
            let Some((file, place)) = insert else { continue };
            let Some(path) = crate::ac1::car::find_ci(&dir, file).or_else(|| crate::ac1::car::find_ci(&car.dir, file)) else {
                ch.notes.push(format!("csp: {skin}: insert {file} not found - left out"));
                continue;
            };
            let bytes = match kn5_protection::open(&path) {
                Ok(b) if !kn5::protected(&b) && kn5::parse(&b).is_ok() => b,
                _ => {
                    ch.notes.push(format!("csp: {skin}: insert {file} is protected or unreadable - left out"));
                    continue;
                }
            };
            let key = (hash(&bytes), place.clone());
            let i = *by_key.entry(key).or_insert_with(|| {
                ch.inserts.push(Insert { file: path.clone(), place: place.clone(), skins: BTreeSet::new(), bytes });
                ch.inserts.len() - 1
            });
            ch.inserts[i].skins.insert(skin.clone());
        }
    }
    let mut hides: Vec<(String, Vec<String>)> = hides.into_iter().filter(|(_, v)| !v.is_empty()).collect();
    hides.sort();
    ch.hides = hides;
    ch
}

fn stem(p: &Path) -> String {
    p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Insert `new` by `place` into the tree under `n`; false when the anchor
/// node is not there.
fn place_node<'a>(n: &mut Node<'a>, place: &Place, new: &mut Option<Node<'a>>) -> bool {
    match place {
        Place::In(a) if n.name.eq_ignore_ascii_case(a) => {
            n.children.push(new.take().unwrap());
            return true;
        }
        Place::After(a) => {
            if let Some(i) = n.children.iter().position(|c| c.name.eq_ignore_ascii_case(a)) {
                n.children.insert(i + 1, new.take().unwrap());
                return true;
            }
        }
        _ => {}
    }
    n.children.iter_mut().any(|c| place_node(c, place, new))
}

fn shift_materials(n: &mut Node, by: i32) {
    if let NodeKind::Mesh(m) = &mut n.kind {
        m.material_id += by;
    }
    for c in &mut n.children {
        shift_materials(c, by);
    }
}

/// Apply the changes to LOD0: the inserts placed, and which nodes each
/// skin hides (`Plan`, for `assign`).
pub fn apply<'a>(k: &mut Kn5<'a>, ch: &'a Changes, skins: &[String]) -> (Plan, Vec<String>) {
    let mut plan = Plan { all: skins.iter().cloned().collect(), ..Default::default() };
    let mut notes = Vec::new();
    // HIDE: the car's own nodes (inserted ones are never hidden: CSP hides
    // before it inserts)
    if !ch.hides.is_empty() {
        let mut names = HashSet::new();
        node_names(&k.root, &mut names);
        for (skin, hidden) in &ch.hides {
            let missing: Vec<&String> = hidden.iter().filter(|n| !names.contains(&n.to_lowercase())).collect();
            for n in hidden.iter().filter(|n| names.contains(&n.to_lowercase())) {
                plan.hides.entry(n.to_lowercase()).or_default().insert(skin.clone());
            }
            notes.push(format!("csp: {skin}: hides {}{}", hidden.join(", "),
                if missing.is_empty() { String::new() } else { format!(" ({} not in the model)", missing.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")) }));
        }
    }
    for ins in &ch.inserts {
        let Ok(mut sub) = kn5::parse(&ins.bytes) else { continue };
        let st = stem(&ins.file);
        // textures: a name already taken by another image gets the insert's prefix
        let mut renamed: HashMap<String, String> = HashMap::new();
        for t in sub.textures.drain(..) {
            match k.textures.iter().find(|x| x.name.eq_ignore_ascii_case(&t.name)) {
                Some(x) if x.data == t.data => {}
                Some(_) => {
                    let n = format!("{st}_{}", t.name);
                    renamed.insert(t.name.to_lowercase(), n.clone());
                    k.textures.push(kn5::Texture { active: t.active, name: n, data: t.data });
                }
                None => k.textures.push(t),
            }
        }
        let off = k.materials.len() as i32;
        for mut m in sub.materials.drain(..) {
            for (_, t) in &mut m.textures {
                if let Some(n) = renamed.get(&t.to_lowercase()) {
                    *t = n.clone();
                }
            }
            k.materials.push(m);
        }
        let mut root = std::mem::replace(&mut sub.root, Node { kind: NodeKind::Other, name: String::new(), active: true, children: vec![] });
        shift_materials(&mut root, off);
        // one root name per insert (one file inserted at two places)
        let mut name = format!("CSP_INSERT_{st}");
        while plan.inserts.iter().any(|(r, _)| *r == name.to_lowercase()) {
            name.push('_');
        }
        root.name = name.clone();
        let n_mesh = count_meshes(&root);
        let mut new = Some(root);
        let skins_s = ins.skins.iter().cloned().collect::<Vec<_>>().join(", ");
        let (how, anchor) = match &ins.place {
            Place::After(a) => ("after", a),
            Place::In(a) => ("in", a),
        };
        if place_node(&mut k.root, &ins.place, &mut new) {
            plan.inserts.push((name.to_lowercase(), ins.skins.clone()));
            notes.push(format!("csp: inserted {st} ({n_mesh} meshes) {how} {anchor} for {skins_s}"));
        } else {
            notes.push(format!("csp: insert {st}: no node {anchor} in the model - left out ({skins_s})"));
        }
    }
    (plan, notes)
}

fn node_names(n: &Node, out: &mut HashSet<String>) {
    out.insert(n.name.to_lowercase());
    for c in &n.children {
        node_names(c, out);
    }
}

fn count_meshes(n: &Node) -> usize {
    usize::from(matches!(n.kind, NodeKind::Mesh(_))) + n.children.iter().map(count_meshes).sum::<usize>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacement_sections() {
        let text = "[MODEL_REPLACEMENT_...]\nFILE=car.kn5\nINSERT=decal.kn5\nHIDE=\nINSERT_AFTER=GEO_STEER\n\n\
                    [MODEL_REPLACEMENT_...]\nACTIVE = 1\nFILE = car.kn5, car_lod_b.kn5\nHIDE=A, B \n\n\
                    [MODEL_REPLACEMENT_...]\nACTIVE=0\nFILE=car.kn5\nINSERT=off.kn5\nINSERT_IN=X\n\n\
                    [MODEL_REPLACEMENT_...]\nFILE=car_lod_b.kn5\nINSERT=lod.kn5\nINSERT_IN=X\n\n\
                    [MODEL_REPLACEMENT_...]\nFILE=car.kn5\nINSERT=plate.kn5\nINSERT_IN=COCKPIT_HR\n";
        let r = replacements(&crate::car::fx::parse_csp(text), "CAR.kn5");
        assert_eq!(r.len(), 3);
        assert_eq!(r[0].0, Some(("decal.kn5".into(), Place::After("GEO_STEER".into()))));
        assert!(r[0].1.is_empty());
        assert_eq!(r[1].0, None);
        assert_eq!(r[1].1, vec!["A".to_string(), "B".to_string()]);
        assert_eq!(r[2].0, Some(("plate.kn5".into(), Place::In("COCKPIT_HR".into()))));
    }

    fn node<'a>(name: &str, children: Vec<Node<'a>>) -> Node<'a> {
        Node { kind: NodeKind::Other, name: name.into(), active: true, children }
    }

    #[test]
    fn places_beside_or_under() {
        let mut root = node("root", vec![node("STEER_HR", vec![node("GEO_STEER", vec![]), node("OTHER", vec![])])]);
        let mut n = Some(node("INS", vec![]));
        assert!(place_node(&mut root, &Place::After("geo_steer".into()), &mut n));
        let names: Vec<&str> = root.children[0].children.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["GEO_STEER", "INS", "OTHER"]);
        let mut n = Some(node("IN2", vec![]));
        assert!(place_node(&mut root, &Place::In("STEER_HR".into()), &mut n));
        assert_eq!(root.children[0].children.last().unwrap().name, "IN2");
        let mut n = Some(node("NONE", vec![]));
        assert!(!place_node(&mut root, &Place::After("MISSING".into()), &mut n));
        assert!(n.is_some());
    }
}
