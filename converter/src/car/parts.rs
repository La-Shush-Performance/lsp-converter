//! Car parts for what only some liveries carry (`inserts`): each part set
//! becomes its own mesh hung on a bone of the car (CAR_BODY, COCKPIT_HR or
//! STEER_BASE), a `.carpart` naming it and a `.compatiblepart` naming the
//! bone, as stock cars carry theirs (the MX-5's rollbar: no skeleton, the
//! vertices in the bone's space, five levels like any exterior mesh). The
//! liveries that show a part list it in their visual preset and final state
//! with a part design of their own (`skins`).

use std::collections::{BTreeSet, HashMap};

use crate::ac1::car::Car;
use crate::ac1::kn5::Kn5;
use crate::car::body::{self, Plan};
use crate::car::frame::{self, Frame};
use crate::car::geometry::{self, Built, Geo, Inst, Section};
use crate::car::inserts::PartSet;
use crate::car::materials::{self, Hints, Kind, MatIn, Templates, Tex};
use crate::files::Files;
use crate::io::pb::{Msg, Val};

/// One written part.
pub struct Part {
    pub n: usize,
    pub compat: String,
    /// the skins (folders) that show it
    pub shown: BTreeSet<String>,
    /// its one design (the `.carpart` names it)
    pub design: String,
    /// (section name, material)
    pub slots: Vec<(String, String)>,
}

/// Per texture of the given materials: a hash of the skin's own file of that
/// name, or None where the skin has none (the kn5's texture shows).
fn texture_signature(car: &Car, k: &Kn5, skin: &str, mats: impl Iterator<Item = usize>) -> Vec<Option<u64>> {
    use std::hash::{Hash, Hasher};
    let dir = car.skin_dir(skin);
    let mut out = Vec::new();
    for mi in mats {
        for (_, t) in &k.materials[mi].textures {
            out.push(crate::ac1::car::find_ci(&dir, t).and_then(|p| std::fs::read(p).ok()).map(|b| {
                let mut h = std::collections::hash_map::DefaultHasher::new();
                b.hash(&mut h);
                h.finish()
            }));
        }
    }
    out
}

/// A part design: every section a fixed slot (no colour pickers, f9 1), as
/// the Camaro fuel cap's carbon slot. A slot in "OEM colours only" mode with
/// no OEM colour listed crashed the game as the car loaded.
fn part_design(name: &str, slots: &[(String, String)]) -> Msg {
    let empty = || Val::Msg(Msg::default());
    let mut d = Msg::default();
    d.set_str(2, name);
    for (slot, mat) in slots {
        let mut s = Msg::default();
        s.set_str(1, slot);
        s.set_str(2, &slot.replace('_', " "));
        s.set_str(3, mat);
        let mut ml = Msg::default();
        ml.push(1, empty());
        ml.push(2, empty());
        ml.push(3, empty());
        s.push(6, Val::Msg(ml));
        s.set_varint(9, 1);
        d.push(3, Val::Msg(s));
    }
    d
}

/// The kind a part's section is built as: paint, mirrors and gauge glass are
/// shared body sections (the livery colour, the stock mirror materials);
/// in a part they are the AC1 material as it is.
fn part_kind(k: Kind) -> Kind {
    match k {
        Kind::Paint | Kind::Mirror | Kind::GaugeGlass | Kind::Tyre | Kind::Rim | Kind::RimBlur | Kind::Banner => Kind::Generic,
        k => k,
    }
}

/// Write the parts' meshes, `.carpart` and `.compatiblepart` files.
#[allow(clippy::too_many_arguments)]
pub fn write(files: &mut Files, base: &str, stem: &str, brand: &str, car: &Car, k: &Kn5, insts: &[Inst], f: &Frame, plan: &Plan, sets: &[PartSet], tex: &mut Tex, t: &Templates, h: &Hints) -> (Vec<Part>, Vec<String>) {
    let mut parts = Vec::new();
    let mut notes = Vec::new();
    let tmpl = files.get(&format!("{base}\\meshes\\{stem}_rim_lf.mesh")).cloned();
    let mut next_n = 0;
    for set in sets {
        let interior = set.bone != body::EXT_ROOT;
        // the bone's frame: vertices in its space (row vectors: p * evo * inv(bone))
        let bone_world = plan.int_bones.iter().chain(plan.ext_bones.iter()).find(|b| b.name == set.bone).map(|b| b.local).unwrap_or(frame::I4);
        let to_bone = frame::inv(&bone_world).unwrap_or(frame::I4);
        // one section per AC1 material
        let mut order: Vec<usize> = Vec::new();
        let mut geos: HashMap<usize, (Geo, Kind)> = HashMap::new();
        for &ii in &set.insts {
            let i = &insts[ii];
            let Some(m) = k.materials.get(i.material) else { continue };
            let kind = part_kind(materials::kind_of(m, &i.node, false, false, i.cockpit, h));
            let e = geos.entry(i.material).or_insert_with(|| {
                order.push(i.material);
                (Geo::default(), kind)
            });
            geometry::append(&mut e.0, i.mesh, &frame::mul(&f.evo_m(&i.world), &to_bone), 0);
        }
        // the sections' geometry (shared by every texture group)
        let mut used: HashMap<String, usize> = HashMap::new();
        let mut geo_secs: Vec<(String, Kind, usize, Geo)> = Vec::new();
        for mi in order {
            let (mut g, kind) = geos.remove(&mi).unwrap();
            geometry::split_double_sided(&mut g, 0.0004);
            geometry::compact(&mut g);
            if g.tris() == 0 {
                continue;
            }
            let mut name = materials::slug(&k.materials[mi].name).to_uppercase();
            let c = used.entry(name.clone()).or_insert(0);
            *c += 1;
            if *c > 1 {
                name = format!("{name}_{c}");
            }
            body::fix_uvs(&name, &mut g);
            geo_secs.push((name, kind, mi, g));
        }
        if geo_secs.is_empty() {
            continue;
        }
        // a part has one design (EVO looks a livery's part design up among
        // the designs the .carparts name: a design of its own per livery
        // crashed the game, "key not found"), so the skins that give the
        // part different textures (the GTWC / NLS / RSS skins' own
        // EXT_Banner.dds) get a part each
        let mut groups: Vec<(Vec<Option<u64>>, BTreeSet<String>)> = Vec::new();
        for skin in &set.shown {
            let sig = texture_signature(car, k, skin, geo_secs.iter().map(|x| x.2));
            match groups.iter_mut().find(|g| g.0 == sig) {
                Some(g) => {
                    g.1.insert(skin.clone());
                }
                None => groups.push((sig, [skin.clone()].into_iter().collect())),
            }
        }
        for (_, shown) in groups {
            next_n += 1;
            let n = next_n;
            let first = shown.iter().next().unwrap();
            let skin = Some((first.as_str(), car.skin_dir(first)));
            let mut secs = Vec::new();
            let mut slots = Vec::new();
            for (name, kind, mi, g) in &geo_secs {
                let inp = MatIn { kind: *kind, m: &k.materials[*mi], interior, skin: skin.clone(), metallic: false };
                let mm = materials::build(files, tex, t, &inp);
                let path = format!("{base}\\materials\\parts\\{stem}_part_{n}\\{name}.material");
                files.put(&path, mm.encode());
                secs.push(Section { name: name.clone(), material: path.clone(), geo: g.clone() });
                slots.push((name.clone(), path));
            }
            let mut b = Built { bones: vec![], lods: vec![secs], lod_in: vec![0.0], skinned: false };
            geometry::make_lods(&mut b, if interior { &geometry::INT_LODS } else { &geometry::EXT_LODS });
            // stock parts carry at most four levels
            b.lods.truncate(PART_LODS);
            b.lod_in.truncate(PART_LODS);
            // every level keeps every section (the renderer looks them up by name)
            for lod in b.lods.iter_mut() {
                for s in lod.iter_mut().filter(|s| s.geo.tris() == 0) {
                    s.geo = geometry::placeholder("", "").geo;
                }
            }
            let tris = b.tris(0);
            let mesh = format!("{base}\\meshes\\parts\\{stem}_part_{n}.mesh");
            files.put(&mesh, b.encode(tmpl.as_deref()));
            let carpart = format!("{base}\\presets\\parts\\{stem}_part_{n}.carpart");
            let compat = format!("{base}\\presets\\parts\\{stem}_part_{n}.compatiblepart");
            let design = design_path(base, stem, n);
            // as the Camaro's fuel cap: name, category, enabled, mesh, design, brand
            let mut cp = Msg::default();
            cp.set_str(2, &format!("{stem} part {n}"));
            cp.set_varint(3, 101);
            cp.set_varint(4, 1);
            cp.set_str(5, &mesh);
            cp.set_str(7, &design);
            cp.set_str(105, brand);
            cp.set_str(6, "Track");
            files.put(&carpart, cp.encode());
            files.put(&compat, compatible_part(&carpart, set.bone).encode());
            files.put(&design, part_design(&format!("{stem} part {n} design"), &slots).encode());
            notes.push(format!("parts: part {n} ({}, {tris} tris on {}) for {}", set.what, set.bone, shown.iter().cloned().collect::<Vec<_>>().join(", ")));
            parts.push(Part { n, compat, design, shown, slots });
        }
    }
    // the actor lists every part the car can carry (103.13 f8, after the
    // rims), as stock actors do
    if !parts.is_empty() {
        let id = base.rsplit('\\').next().unwrap_or(base);
        let ap = format!("{base}\\{id}.actor");
        if let Some(mut a) = files.get(&ap).and_then(|b| Msg::parse(b).ok()) {
            if let Ok(lists) = a.msg_mut(103).and_then(|c| c.msg_mut(13)) {
                lists.remove(8);
                for p in &parts {
                    lists.push(8, Val::Bytes(p.compat.as_bytes().to_vec()));
                }
                lists.fields.sort_by_key(|x| x.num);
                files.put(&ap, a.encode());
            }
        }
    }
    (parts, notes)
}

/// Stock parts carry at most four levels of detail.
const PART_LODS: usize = 4;

/// A `.compatiblepart` as the MX-5's rollbar carries it: the carpart, f2 1,
/// the bone, a transform {translation, rotation (both empty), scale 1} and
/// f6 with its empty slots. Without f2, the empty translation / rotation and
/// f6, the game threw "Vertex data is required for dynamic/in-memory mesh"
/// as the car loaded (the stock file under our name loaded).
fn compatible_part(carpart: &str, bone: &str) -> Msg {
    let empty = || Val::Msg(Msg::default());
    let mut co = Msg::default();
    co.set_str(1, carpart);
    co.set_varint(2, 1);
    co.set_str(3, bone);
    let mut tr = Msg::default();
    tr.push(1, empty());
    tr.push(2, empty());
    let mut one = Msg::default();
    for fnum in 1..=3 {
        one.set_f32(fnum, 1.0);
    }
    tr.push(3, Val::Msg(one));
    co.push(5, Val::Msg(tr));
    let mut six = Msg::default();
    six.push(1, empty());
    six.push(2, empty());
    for f in 3..=6 {
        let mut s = Msg::default();
        s.push(1, empty());
        s.push(2, empty());
        six.push(f, Val::Msg(s));
    }
    six.push(7, empty());
    co.push(6, Val::Msg(six));
    co
}

/// A part's one design.
pub fn design_path(base: &str, stem: &str, n: usize) -> String {
    format!("{base}\\skins\\partdesign_{stem}_{n}.design")
}
