//! CSP model changes per skin: `[MODEL_REPLACEMENT_...]` sections in a
//! skin's `ext_config.ini` (or the car's own CSP config, for every skin)
//! that INSERT a small kn5 into the car (INSERT_AFTER a node: beside it,
//! INSERT_IN a node: under it) or HIDE nodes of it. RSS's GTM Mercer
//! Verstappen skins add a steering-wheel hub decal, a dashboard plate and
//! LED windscreen banners this way, and hide the stock strips and banners;
//! converted without them the wheel hub was a bare black disc.
//!
//! EVO's geometry is one for every livery; a livery swaps section
//! materials. So:
//! - every distinct insert (file content + place) is merged into LOD0 once,
//!   its materials appended (same names, so CSP material rules still find
//!   them) and its textures added (renamed only when a name is taken by a
//!   different image);
//! - the meshes a skin hides get copies of their materials (same name), so
//!   they land in sections of their own;
//! - `Vis` says, per material index, which skins show it. Such a section's
//!   own material draws nothing (`materials::hidden`) and every livery (the
//!   first one and Single Colour too) names it: its real material where the
//!   skin shows it, `hidden` where it does not. Checked in EVO on the
//!   Mercer: the N24 skin's own banner clean over the stock one it hides,
//!   the other skins with their stock banner and strips and none of the
//!   inserts. (A 4 x 4 opacity map drew the hidden sections solid black:
//!   `materials::FLAT_TILE`.)
//!
//! Only the LOD0 model (the inserts' other FILE entries are author LODs);
//! glass, mirrors and gauge glass share one section per role, so an insert
//! there shows on every skin. EMISSIVE_LIGHT and other per-skin CSP effects
//! are not converted.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::ac1::car::Car;
use crate::ac1::kn5::{self, Kn5, Node, NodeKind};
use crate::ac1::kn5_protection::{self, Kn5Bytes};
use crate::car::fx::CspSection;
use crate::car::materials::Kind;

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

/// Material index -> the skins (folders) that show it; materials not listed
/// show on every skin.
#[derive(Default, Debug)]
pub struct Vis {
    shown: HashMap<usize, BTreeSet<String>>,
}

impl Vis {
    /// Does `skin` show material `mat`? None: not a per-skin material.
    pub fn shows(&self, mat: usize, kind: Kind, skin: &str) -> Option<bool> {
        if !per_skin(kind) {
            return None;
        }
        self.shown.get(&mat).map(|s| s.contains(skin))
    }
    pub fn is_empty(&self) -> bool {
        self.shown.is_empty()
    }
}

/// Sections that can differ per skin: not the ones every material of a kind
/// shares (one EXT_WINDOWS, one INT_GLASS, a section per mirror).
fn per_skin(kind: Kind) -> bool {
    // paint carries the livery colour (and Single Colour's layered paint)
    !matches!(kind, Kind::Mirror | Kind::Glass | Kind::GaugeGlass | Kind::Paint)
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

/// Every mesh node in DFS order, with the names of the nodes above it
/// (itself included), as a flat list of `&mut` (one walk).
fn meshes_mut<'n, 'a>(n: &'n mut Node<'a>, up: &mut Vec<String>, out: &mut Vec<(Vec<String>, &'n mut kn5::Mesh<'a>)>) {
    up.push(n.name.to_lowercase());
    if let NodeKind::Mesh(m) = &mut n.kind {
        out.push((up.clone(), m));
    }
    for c in &mut n.children {
        meshes_mut(c, up, out);
    }
    up.pop();
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

/// Apply the changes to LOD0: HIDE first (on the car's own nodes), then the
/// inserts. Returns which skins show the per-skin materials.
pub fn apply<'a>(k: &mut Kn5<'a>, ch: &'a Changes, skins: &[String]) -> (Vis, Vec<String>) {
    let mut vis = Vis::default();
    let mut notes = Vec::new();
    let all: BTreeSet<String> = skins.iter().cloned().collect();
    // HIDE: each hidden mesh -> the skins that hide it; meshes with the same
    // material and the same hiding skins share one material copy
    if !ch.hides.is_empty() {
        let hide: Vec<(String, HashSet<String>)> = ch.hides.iter().map(|(s, v)| (s.clone(), v.iter().map(|n| n.to_lowercase()).collect())).collect();
        let mut meshes = Vec::new();
        meshes_mut(&mut k.root, &mut Vec::new(), &mut meshes);
        let mut copies: HashMap<(i32, BTreeSet<String>), i32> = HashMap::new();
        let mut new_mats = Vec::new();
        let mut found: BTreeSet<String> = BTreeSet::new();
        for (up, m) in meshes {
            let mut by: BTreeSet<String> = BTreeSet::new();
            for (s, names) in &hide {
                if let Some(n) = up.iter().find(|n| names.contains(*n)) {
                    found.insert(n.clone());
                    by.insert(s.clone());
                }
            }
            if by.is_empty() || m.material_id < 0 || m.material_id as usize >= k.materials.len() {
                continue;
            }
            let base = k.materials.len() as i32;
            let next = base + new_mats.len() as i32;
            let mi = *copies.entry((m.material_id, by.clone())).or_insert_with(|| {
                new_mats.push((k.materials[m.material_id as usize].clone(), all.difference(&by).cloned().collect::<BTreeSet<String>>()));
                next
            });
            m.material_id = mi;
        }
        let n_copies = new_mats.len();
        for (mat, shown) in new_mats {
            vis.shown.insert(k.materials.len(), shown);
            k.materials.push(mat);
        }
        for (skin, names) in &ch.hides {
            let missing: Vec<&String> = names.iter().filter(|n| !found.contains(&n.to_lowercase())).collect();
            notes.push(format!("csp: {skin}: hides {}{}", names.join(", "),
                if missing.is_empty() { String::new() } else { format!(" ({} not in the model)", missing.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")) }));
        }
        if n_copies > 0 {
            notes.push(format!("csp: {n_copies} material copies for meshes some skins hide"));
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
            vis.shown.insert(k.materials.len(), ins.skins.clone());
            k.materials.push(m);
        }
        let mut root = std::mem::replace(&mut sub.root, Node { kind: NodeKind::Other, name: String::new(), active: true, children: vec![] });
        shift_materials(&mut root, off);
        root.name = format!("CSP_INSERT_{st}");
        let mut n_mesh = Vec::new();
        meshes_mut(&mut root, &mut Vec::new(), &mut n_mesh);
        let n_mesh = n_mesh.len();
        let mut new = Some(root);
        let skins_s = ins.skins.iter().cloned().collect::<Vec<_>>().join(", ");
        let (how, anchor) = match &ins.place {
            Place::After(a) => ("after", a),
            Place::In(a) => ("in", a),
        };
        if place_node(&mut k.root, &ins.place, &mut new) {
            notes.push(format!("csp: inserted {st} ({n_mesh} meshes) {how} {anchor} for {skins_s}"));
        } else {
            notes.push(format!("csp: insert {st}: no node {anchor} in the model - left out ({skins_s})"));
            // its materials stay unused
        }
    }
    (vis, notes)
}

/// Every per-skin section's own material draws nothing; the liveries that
/// show it switch it on (`skins::write`).
pub fn hide_default(files: &mut crate::files::Files, sections: &[(String, Kind, usize, bool, String)], vis: &Vis, default: Option<&str>,
                    t: &crate::car::materials::Templates, id: &str) -> Vec<String> {
    let Some(d) = default else { return vec![] };
    let mut hidden = Vec::new();
    for (name, kind, mi, int, path) in sections {
        if vis.shows(*mi, *kind, d).is_some() {
            let m = crate::car::materials::hidden(files, t, *int, id).encode();
            files.put(path, m);
            hidden.push(name.clone());
        }
    }
    if hidden.is_empty() {
        vec![]
    } else {
        vec![format!("csp: per-skin sections (drawn by the liveries that show them): {}", hidden.join(", "))]
    }
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

    #[test]
    fn vis_skips_shared_sections() {
        let mut v = Vis::default();
        v.shown.insert(3, ["b".to_string()].into_iter().collect());
        assert_eq!(v.shows(3, Kind::Generic, "a"), Some(false));
        assert_eq!(v.shows(3, Kind::Generic, "b"), Some(true));
        assert_eq!(v.shows(3, Kind::Glass, "a"), None);
        assert_eq!(v.shows(4, Kind::Generic, "a"), None);
    }
}
