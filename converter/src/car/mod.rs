//! The car side: AC1 car folder -> EVO car mod package.
//!
//! `convert` runs the stages in order: identity + donor clone, frame, body
//! (geometry, LODs, materials), wheels and tyres, actor clean-up, animated
//! parts, liveries, identity finish (names, final-state ids, brand),
//! physics (every physics, setup and AI file, `physics`), collider, pruning
//! of donor files nothing references, then the package and its `<id>.json`.
//! The car functions (lights, LEDs, display, cameras, exhaust, ground
//! shadow, driver) run after the actor clean-up; the sound stage edits the
//! same `Files`.

pub mod anim;
pub mod animparts;
pub mod belts;
pub mod brand;
pub mod body;
pub mod cameras;
pub mod display;
pub mod driver;
pub mod exhaust;
pub mod frame;
pub mod fx;
pub mod geometry;
pub mod identity;
pub mod leds;
pub mod lights;
pub mod materials;
pub mod needles;
pub mod physics;
pub mod skins;
pub mod usersettings;
pub mod wheels;

use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::ac1::car::Car;
use crate::files::Files;
use crate::io::pb::{Msg, Val};

/// OURS: a mesh wholly this far outside the box of the wheel centres is not
/// part of the car. A margin, not a survey: well past any overhang, roof or
/// wing, far short of the 25 m leftover it was made for.
const STRAY_M: f64 = 3.0;

pub struct Opts {
    pub ac1: PathBuf,
    pub out: PathBuf,
    /// the mod id (default: `ids::car_id` of the folder)
    pub id: Option<String>,
    pub donor_zip: PathBuf,
    pub refmats: PathBuf,
    /// the game's content.kspkg (stock ids, brands, common assets); None = offline
    pub content: Option<PathBuf>,
    /// the handling profile (physics::profile): auto-detected, or the user's
    pub profile: physics::profile::Choice,
    /// menu name and brand over the AC1 ui_car.json ones (None = AC1's)
    pub name: Option<String>,
    pub brand: Option<String>,
    /// keep the mod's brakes however strong (physics::chassis::brake_strength)
    pub keep_brakes: bool,
    /// a steering lock below this many degrees is raised to it (physics::chassis::min_lock); None = the mod's own
    pub min_lock: Option<f64>,
}

pub struct Report {
    pub id: String,
    pub package: PathBuf,
    pub notes: Vec<String>,
}

/// Textures any AC1 skin of the car replaces (lower-case file names).
fn skin_textures(car: &Car) -> HashSet<String> {
    let mut out = HashSet::new();
    for s in &car.skins {
        for e in std::fs::read_dir(car.skin_dir(s)).into_iter().flatten().flatten() {
            let n = e.file_name().to_string_lossy().to_lowercase();
            if n.ends_with(".dds") || n.ends_with(".png") || n.ends_with(".jpg") || n.ends_with(".tga") {
                out.insert(n);
            }
        }
    }
    out
}

/// What the donor's actor names that the converted car replaces or drops:
/// car animation clips (the animated-parts stage writes ours), the
/// Abarth's belts and number plate meshes, lamp / LED batch names that no
/// section carries.
fn actor_cleanup(files: &mut Files, id: &str, stem: &str, lamp_sections: &[String]) -> Result<Vec<String>> {
    let base = format!("content\\cars\\{id}");
    let ap = format!("{base}\\{id}.actor");
    let Some(b) = files.get(&ap).cloned() else { return Ok(vec![]) };
    let mut a = Msg::parse(&b)?;
    let car = a.msg_mut(103)?;
    // car animations: every slot loses its clips; legacy lists go
    if let Some(i) = car.idx(27).first().copied() {
        let an = car.open(i)?;
        an.fields.retain(|f| !(2..=19).contains(&f.num));
        for j in 0..an.fields.len() {
            if an.fields[j].num >= 100 {
                let s = an.open(j)?;
                s.remove(1);
            }
        }
    }
    // base meshes: belts (f23 / f24) and the plate (f27) are the Abarth's
    let bm = car.msg_mut(12)?;
    for f in [23u32, 24, 27] {
        if let Some(i) = bm.idx(f).first().copied() {
            let s = bm.open(i)?;
            s.remove(1);
            s.remove(2);
        }
    }
    // lamp batches: our lamp sections; LED batches: none until the LED stage
    if let Some(i) = car.idx(25).first().copied() {
        let lb = car.open(i)?;
        lb.remove(1);
        for n in lamp_sections {
            lb.push(1, Val::Bytes(n.as_bytes().to_vec()));
        }
        lb.fields.sort_by_key(|f| f.num);
    }
    if let Some(i) = car.idx(26).first().copied() {
        car.open(i)?.remove(1);
    }
    files.put(&ap, a.encode());
    let mut removed = 0;
    for p in files.under(&format!("{base}\\animations")) {
        let l = p.to_lowercase();
        if !l.contains("driver_animations") && l.ends_with(".animation") {
            files.remove(&p);
            removed += 1;
        }
    }
    for p in [format!("{base}\\meshes\\{stem}_belt_auto.mesh"), format!("{base}\\meshes\\{stem}_harness_off.mesh")] {
        files.remove(&p);
    }
    Ok(vec![format!("actor: donor car clips ({removed}), belts and plate removed")])
}

/// The AC1 collider -> the collider mesh (one section on the editor
/// default material, like stock colliders). Ours: vertices welded on a 1 cm
/// grid; a collider with no triangles, or more than 1.5 x the body's size,
/// keeps the donor's.
fn collider(files: &mut Files, car: &Car, f: &frame::Frame, id: &str, stem: &str, body: ([f32; 3], [f32; 3])) -> Vec<String> {
    let mut notes = Vec::new();
    let path = format!("content\\cars\\{id}\\collider\\{stem}_collider.mesh");
    let Some(cp) = &car.collider else {
        notes.push("collider: no collider.kn5 - the donor's stays".into());
        return notes;
    };
    let Ok(map) = crate::ac1::kn5_protection::open(cp) else {
        notes.push("collider: collider.kn5 is protected - the donor's stays".into());
        return notes;
    };
    if crate::ac1::kn5::protected(&map) {
        notes.push("collider: collider.kn5 is protected - the donor's stays".into());
        return notes;
    }
    let Ok(k) = crate::ac1::kn5::parse(&map) else {
        notes.push("collider: collider.kn5 unreadable - the donor's stays".into());
        return notes;
    };
    let mut dropped = Vec::new();
    let insts = geometry::flatten(&k, &mut dropped);
    let mut g = geometry::Geo::default();
    for i in &insts {
        geometry::append(&mut g, i.mesh, &f.evo_m(&i.world), 0);
    }
    for v in &mut g.verts {
        v.pos = v.pos.map(|x| (x * 100.0).round() / 100.0);
        v.uv = [0.0, 0.0];
        v.nrm = [0.0, 1.0, 0.0];
        v.tan = [1.0, 0.0, 0.0, 1.0];
    }
    geometry::compact(&mut g);
    let b = geometry::Built { bones: vec![], lods: vec![vec![geometry::Section { name: "COLLIDER".into(), material: "editor/default.material".into(), geo: g }]], lod_in: vec![0.0], skinned: false };
    let (lo, hi) = b.bounds();
    let size = |l: [f32; 3], h: [f32; 3]| ((h[0] - l[0]).max(0.0), (h[1] - l[1]).max(0.0), (h[2] - l[2]).max(0.0));
    let (cs, bs) = (size(lo, hi), size(body.0, body.1));
    if b.tris(0) == 0 || cs.0 > bs.0 * 1.5 + 0.2 || cs.2 > bs.2 * 1.5 + 0.2 || cs.1 > bs.1 * 1.5 + 0.2 {
        notes.push(format!("collider: implausible AC1 collider ({} tris, {:.2} x {:.2} x {:.2} m) - the donor's stays", b.tris(0), cs.0, cs.1, cs.2));
        return notes;
    }
    let tmpl = files.get(&path).cloned();
    files.put(&path, b.encode(tmpl.as_deref()));
    notes.push(format!("collider: {} tris", b.tris(0)));
    notes
}

/// Every `content\cars\...` path string the package's files mention
/// (lower case, raw byte scan: a protobuf string runs to the next
/// non-printable byte, so a path may carry trailing bytes - callers match
/// prefixes).
fn mentioned(files: &Files, skip: &HashSet<String>) -> Vec<String> {
    let mut out = Vec::new();
    let pat = b"content\\cars\\";
    for (p, d) in &files.map {
        if skip.contains(&p.to_lowercase()) || p.to_lowercase().ends_with(".texturemips") {
            continue;
        }
        let mut i = 0;
        while let Some(q) = memchr::memmem::find(&d[i..], pat) {
            let s = i + q;
            let mut e = s;
            while e < d.len() && (0x20..0x7f).contains(&d[e]) {
                e += 1;
            }
            out.push(String::from_utf8_lossy(&d[s..e]).to_lowercase());
            // the next path may follow within the printable run (a field
            // tag and length can both be printable bytes)
            i = s + pat.len();
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Drop donor files nothing references any more (materials, textures,
/// meshes, colours, designs), until nothing changes.
fn prune(files: &mut Files, id: &str) -> usize {
    let base = format!("content\\cars\\{id}\\").to_lowercase();
    let prunable = |l: &str| -> bool {
        let rel = &l[base.len()..];
        let dir_ok = ["materials\\", "texture\\", "meshes\\", "customization\\", "parts\\", "skins\\"].iter().any(|d| rel.starts_with(d));
        let ext_ok = [".material", ".texture", ".mesh", ".oemmultilayercolor", ".design", ".cardesign", ".tyremesh", ".rimmesh"].iter().any(|e| rel.ends_with(e));
        dir_ok && ext_ok
    };
    let mut total = 0;
    loop {
        let skip = HashSet::new();
        let refs = mentioned(files, &skip);
        let has = |p: &str| -> bool {
            let i = refs.partition_point(|r| r.as_str() < p);
            refs.get(i).map(|r| r.starts_with(p)).unwrap_or(false)
        };
        let gone: Vec<String> = files.map.keys().filter(|k| {
            let l = k.to_lowercase();
            l.starts_with(&base) && prunable(&l) && !has(&l)
        }).cloned().collect();
        if gone.is_empty() {
            break;
        }
        for g in &gone {
            files.remove(g);
            files.remove(&format!("{g}mips"));
            total += 1;
        }
    }
    total
}

/// LOD0 bounds of an encoded mesh.
pub fn mesh_bounds(files: &Files, path: &str) -> ([f32; 3], [f32; 3]) {
    files.get(path).and_then(|b| Msg::parse(b).ok()).map(|m| {
        let v = |x: Option<Msg>| x.map(|m| [m.f32(1).unwrap_or(0.0), m.f32(2).unwrap_or(0.0), m.f32(3).unwrap_or(0.0)]).unwrap_or([0.0; 3]);
        (v(m.sub(6)), v(m.sub(7)))
    }).unwrap_or(([-1.0, 0.0, -2.0], [1.0, 1.5, 2.0]))
}

/// The car functions after the body: lamps, LEDs, cameras, the dash
/// display, exhaust and the ground shadow.
#[allow(clippy::too_many_arguments)]
fn functions(files: &mut Files, id: &str, stem: &str, car: &Car, k: &crate::ac1::kn5::Kn5, f: &frame::Frame, insts: &[geometry::Inst], plan: &body::Plan,
             written: &[(String, materials::Kind, usize, bool, String)], anim: &animparts::Plan,
             lamp_plan: &lights::LampPlan, led_plan: &leds::LedPlan, disp_plan: &display::DisplayPlan) -> Result<Vec<String>> {
    let mut notes = Vec::new();
    let popups = anim.clips.iter().any(|c| c.slots.contains(&119));
    notes.extend(lights::write(files, id, stem, plan, insts, lamp_plan, written, popups)?);
    notes.extend(leds::write(files, id, plan, insts, led_plan, written)?);
    let body = mesh_bounds(files, &format!("content\\cars\\{id}\\meshes\\{stem}_exterior.mesh"));
    let cams = cameras::eye(car, f, body);
    notes.extend(cameras::write(files, id, car, f, &cams, body)?);
    notes.extend(display::write(files, id, stem, disp_plan, cams.eye)?);
    let (outlets, tier, n) = exhaust::outlets(car, f, k, insts);
    notes.extend(n);
    notes.extend(exhaust::write(files, id, car, &outlets, tier, led_plan.limiter as f64)?);
    notes.extend(shadow(files, id, car, f, body)?);
    Ok(notes)
}

/// Stock body shadow textures: the median p99 grey of 11 stock cars'
/// `body_shadow.texture` (199..255; f2004 232, sf_25 233, mx5_na 229).
/// AC1's baked `body_shadow.png` peaks at 191.
const STOCK_SHADOW_P99: f32 = 233.0;

/// AC1's ground-shadow quad, full (width, length): `ambient_shadows.ini`
/// [SETTINGS] WIDTH / LENGTH are its half sizes (1.0 / 2.8 on lotus_98t,
/// tatuusfa1, ks_mazda_miata and the La Shush cars, 1.5 / 3.5 on
/// lotus_exos_125), centred on the kn5 origin. None when missing or absurd.
fn ac1_shadow_size(ini: &crate::ac1::ini::Ini) -> Option<(f64, f64)> {
    let w = ini.num("SETTINGS", "WIDTH")?;
    let l = ini.num("SETTINGS", "LENGTH")?;
    ((0.3..=5.0).contains(&w) && (0.5..=8.0).contains(&l)).then_some((2.0 * w, 2.0 * l))
}

/// AC1's `body_shadow.png` as the EVO decal image: the grey (R x alpha) in
/// R, its p99 brought to the stock p99, turned half a turn. AC1 lays the
/// image with the car's front at the top (lotus_98t's front wing; the
/// Taipan's front-wing edge lands 4 cm from its nose), stock EVO with the
/// front at the bottom (sf_25's nose and wheelbase; the Abarth's axle bands
/// at +-wheelbase/2 about its decal centre); both are views from above, so
/// left and right swap too. None for an image with no shadow in it.
fn shadow_image(img: &crate::io::texture::Image) -> Option<crate::io::texture::Image> {
    let g: Vec<f32> = img.px.chunks_exact(4).map(|c| c[0] as f32 * c[3] as f32 / 255.0).collect();
    let mut s = g.clone();
    s.sort_by(f32::total_cmp);
    let p99 = *s.get(s.len() * 99 / 100)?;
    if p99 < 8.0 {
        return None;
    }
    let k = STOCK_SHADOW_P99 / p99;
    let px = g.iter().rev().flat_map(|&v| {
        let x = (v * k).round().clamp(0.0, 255.0) as u8;
        [x, x, x, 255]
    }).collect();
    Some(crate::io::texture::Image { w: img.w, h: img.h, px })
}

/// The body ground shadow (actor f103.f28 ShadowDecal: f1 / f2 x / z size,
/// f8 / f5 x / z position, f4 texture). The AC1 car's own shadow where AC1
/// draws it: `body_shadow.png` (`shadow_image`, 512 at most like stock) on
/// AC1's quad (`ac1_shadow_size`) at the kn5 origin (the datum in EVO
/// space). Without one, the donor's, sized and centred on the converted
/// body (calibrated on 10 stock cars: decal length 1.02..1.07 x body length,
/// median 1.04, centred on the body's mid z; width 2.12 on every one).
/// Ours: the donor's is a hatchback's rounded box, and under the La Shush
/// Taipan (an open-wheeler) its corners darkened the track beside the nose
/// and behind the rear wheels, off the car's outline.
fn shadow(files: &mut Files, id: &str, car: &Car, f: &frame::Frame, body: ([f32; 3], [f32; 3])) -> Result<Vec<String>> {
    use crate::io::texture;
    let ap = format!("content\\cars\\{id}\\{id}.actor");
    let Some(mut actor) = files.get(&ap).and_then(|b| Msg::parse(b).ok()) else { return Ok(vec![]) };
    let size = ac1_shadow_size(car.ini("ambient_shadows.ini"));
    let img = crate::ac1::car::find_ci(&car.dir, "body_shadow.png").and_then(|p| std::fs::read(p).ok())
        .and_then(|b| texture::decode_any(&b).ok()).and_then(|i| shadow_image(&texture::resize_to(&i, 512, false)));
    let a = actor.msg_mut(103)?;
    let sd = a.msg_mut(28)?;
    let note = match (size, img) {
        (Some((sx, sz)), Some(img)) => {
            let tex = sd.str(4).filter(|t| !t.is_empty()).unwrap_or_else(|| format!("content\\cars\\{id}\\texture\\body_shadow.texture"));
            let e = texture::encode(&img, texture::Fmt::Bc4, false, "body_shadow.png");
            files.put(&tex, e.header);
            files.put(&format!("{tex}mips"), e.mips);
            let (px, pz) = (f.datum[0] as f32, f.datum[2] as f32);
            sd.set_str(4, &tex);
            sd.set_f32(1, sx as f32);
            sd.set_f32(2, sz as f32);
            sd.set_f32(5, pz);
            if px.abs() > 0.001 {
                sd.set_f32(8, px);
            } else {
                sd.remove(8);
            }
            format!("shadow: AC1's body_shadow.png, {sx:.2} x {sz:.2} m at z {pz:.3} (ambient_shadows.ini)")
        }
        (size, _) => {
            let (lo, hi) = body;
            let (w, l) = (hi[0] - lo[0], hi[2] - lo[2]);
            let (sx, sz, pz) = ((1.05 * w).max(2.12), 1.04 * l, (hi[2] + lo[2]) * 0.5);
            sd.set_f32(1, sx);
            sd.set_f32(2, sz);
            sd.set_f32(5, pz);
            sd.remove(8);
            let why = if size.is_none() { "no ambient_shadows.ini size" } else { "no usable body_shadow.png" };
            format!("shadow: the donor's, {sx:.2} x {sz:.2} m at z {pz:.3} (body {w:.2} x {l:.2}; {why})")
        }
    };
    files.put(&ap, actor.encode());
    Ok(vec![note])
}

/// Convert one AC1 car.
pub fn convert(o: &Opts) -> Result<Report> {
    let t0 = std::time::Instant::now();
    let (donor_zip, refmats) = if o.donor_zip.as_os_str().is_empty() || o.refmats.as_os_str().is_empty() {
        let (d, r) = crate::batch::donor_paths()?;
        (if o.donor_zip.as_os_str().is_empty() { d } else { o.donor_zip.clone() }, if o.refmats.as_os_str().is_empty() { r } else { o.refmats.clone() })
    } else {
        (o.donor_zip.clone(), o.refmats.clone())
    };
    let car = Car::open(&o.ac1)?;
    let game = match &o.content {
        Some(c) if c.is_file() => Some(crate::validate::Game::open(c)?),
        _ => None,
    };
    let taken = game.as_ref().map(|g| g.ids("cars")).unwrap_or_default();
    let brands = game.as_ref().map(identity::stock_brands).unwrap_or_default();
    let mut ident = identity::identity(&car, o.id.as_deref(), &taken, &brands);
    // the user's menu name and brand over AC1's ui_car.json (test builds, a
    // car group of one's own); the brand resolves like an AC1 brand
    if let Some(n) = o.name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
        ident.display = n.to_string();
    }
    if let Some(b) = o.brand.as_deref().map(str::trim).filter(|b| !b.is_empty()) {
        let r = brand::resolve(Some(b), None, &brands);
        (ident.brand, ident.brand_name, ident.brand_display, ident.stock_brand) = (r.key, r.name, r.label, r.stock);
        ident.text_badge = true;
    }
    let (id, stem) = (ident.id.clone(), ident.stem.clone());
    let mut notes = vec![format!("identity: {} ({} {}, brand {}{})", id, ident.display, ident.year, ident.brand, if ident.stock_brand { ", stock" } else { "" })];
    let donor = crate::files::load_donor(&donor_zip, identity::DONOR_ID).with_context(|| format!("donor {}", donor_zip.display()))?;
    let mut files = identity::clone_donor(donor, &ident);
    let base = ident.base();
    let mut k = car.kn5()?;
    // details tiled as CSP draws them (its DetailScale over the kn5's)
    if let Some((_, secs)) = fx::csp_config(&car) {
        let changed = materials::csp_detail_scale(&mut k.materials, &secs);
        if !changed.is_empty() {
            notes.push(format!("materials: detail tiling from the CSP config: {}", changed.join(", ")));
        }
        let carbon = materials::csp_carbon(&mut k.materials, &secs);
        if !carbon.is_empty() {
            notes.push(format!("materials: carbon from the CSP config: {}", carbon.join(", ")));
        }
    }
    let mut frame = frame::Frame::build(&car, &k);
    notes.extend(frame.notes.iter().cloned());
    let insts = geometry::flatten(&k, &mut notes);
    let mut hints = materials::Hints::from_data(car.ini("lights.ini"), car.ini("mirrors.ini"), skin_textures(&car));
    hints.paint = materials::paint_fallback(&k.materials, &hints);
    // each wheel centred on its AC1 tyre (before anything places parts at it)
    notes.extend(wheels::centre_on_tyres(&mut frame, &insts, &k, &hints));
    // meshes AC1 never draws (wholly transparent cut-outs) stay out
    {
        let skin_dirs: Vec<std::path::PathBuf> = car.skins.iter().map(|s| car.skin_dir(s)).collect();
        let unseen = materials::invisible(&materials::Tex::new(&k, &id), &k.materials, &skin_dirs);
        let mat_of = |i: &geometry::Inst| k.materials.get(i.material).map(|m| m.name.to_lowercase()).unwrap_or_default();
        let mut gone: Vec<String> = insts.iter().map(|i| i.node.to_lowercase())
            .filter(|n| insts.iter().filter(|i| i.node.eq_ignore_ascii_case(n)).all(|i| unseen.contains(&mat_of(i)))).collect();
        gone.sort();
        gone.dedup();
        if !gone.is_empty() {
            notes.push(format!("materials: {} wholly transparent (AC1 never draws them): left out {}", unseen.iter().cloned().collect::<Vec<_>>().join(", "), gone.join(", ")));
        }
        hints.drop.extend(gone);
    }
    // modelling leftovers far from the car stay out too: gravygarage_street_omega's
    // Object187 (a 12 cm sheet of glass 25 m over the roof) took the body's
    // top with it, and the chase cameras failed check-car under it
    if !frame.wheel_centres.is_empty() {
        let (mut lo, mut hi) = ([f64::MAX; 3], [f64::MIN; 3]);
        for p in frame.wheel_centres.values() {
            for a in 0..3 {
                lo[a] = lo[a].min(p[a] - STRAY_M);
                hi[a] = hi[a].max(p[a] + STRAY_M);
            }
        }
        let mut near: HashMap<String, bool> = HashMap::new();
        for i in &insts {
            let (a, b) = body::evo_bounds(i, &frame);
            let far = (0..3).any(|k| b[k] < lo[k] || a[k] > hi[k]);
            *near.entry(i.node.to_lowercase()).or_insert(false) |= !far;
        }
        let mut gone: Vec<String> = near.into_iter().filter(|(_, n)| !n).map(|(k, _)| k).collect();
        gone.sort();
        if !gone.is_empty() {
            notes.push(format!("geometry: left out {} (wholly over {STRAY_M} m from the wheels: modelling leftovers)", gone.join(", ")));
        }
        hints.drop.extend(gone);
    }
    if !hints.paint.is_empty() {
        let mut v: Vec<&String> = hints.paint.iter().collect();
        v.sort();
        notes.push(format!("paint: no car-paint material; painting {} (named like the body)", v.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")));
    }
    // a side mirror mirrors.ini leaves out (its entry given to something else)
    let twins = body::mirror_twins(&k, &insts, &frame, &hints, &mut notes);
    hints.mirrors.extend(twins);
    let templates = materials::Templates::load(&refmats, &files, &id)?;
    // car functions that shape the body's sections (read before the body
    // overwrites the donor's materials)
    let lamp_plan = lights::prepare(&car, &insts, &frame, &mut hints);
    let led_plan = leds::prepare(&car, &insts, &files, &id, &mut hints);
    let disp_plan = display::prepare(&car, &frame, &files, &id);
    notes.extend(lamp_plan.notes.iter().cloned());
    notes.extend(led_plan.notes.iter().cloned());
    let paint_tmpl = files.get(&format!("{base}\\materials\\EXT_SKIN.material")).and_then(|b| Msg::parse(b).ok());
    let donor_ext = files.get(&format!("{base}\\meshes\\{stem}_exterior.mesh")).map(|b| body::bones_of(b)).unwrap_or_default();
    let d = frame.datum;
    let mut anim = animparts::plan(&animparts::Input { k: &k, ac1_dir: &car.dir, data: &car.data, ah: [d[0] as f32, d[1] as f32, d[2] as f32] });
    let plan = body::plan(&k, &insts, &frame, &hints, &donor_ext, Some(&mut anim));
    notes.extend(plan.notes.iter().cloned());
    let mut tex = materials::Tex::new(&k, &id);
    tex.default_skin = car.skins.first().cloned();
    let default_skin = car.skins.first().map(|s| (s.as_str(), car.skin_dir(s)));
    let author = body::Author { files: car.lods.iter().map(|l| l.file.clone()).collect(), hints: &hints, anim: Some(&anim) };
    let written = body::write(&mut files, &id, &stem, &k, &insts, &frame, &plan, &mut tex, &templates, game.as_ref(), default_skin.clone(), &author)?;
    notes.extend(written.notes.iter().cloned());
    let wheels = wheels::write(&mut files, &id, &stem, &k, &insts, &plan.wheel_parts, &frame, &hints, &mut tex, &templates, default_skin.clone(), game.as_ref())?;
    notes.extend(wheels.notes.iter().cloned());
    notes.extend(belts::write(&mut files, &id, &stem, &k, &insts, &frame, &mut tex, &templates, default_skin.clone())?);
    let lamps: Vec<String> = written.sections.iter().filter(|s| s.1 == materials::Kind::Lamp).map(|s| s.0.clone()).collect();
    notes.extend(actor_cleanup(&mut files, &id, &stem, &lamps)?);
    notes.extend(functions(&mut files, &id, &stem, &car, &k, &frame, &insts, &plan, &written.sections, &anim, &lamp_plan, &led_plan, &disp_plan)?);
    notes.extend(animparts::write(&mut files, &id, &anim)?);
    let secs: Vec<skins::Section> = written.sections.iter().map(|(n, kind, m, int, p)| skins::Section { name: n, kind: *kind, material: *m, interior: *int, path: p }).collect();
    let sk = skins::write(&mut files, &ident, &car, &k, &secs, &wheels.rim_mats, &mut tex, &templates, paint_tmpl)?;
    notes.extend(sk.notes);
    notes.extend(identity::apply(&mut files, &ident, &car)?);
    let phys = physics::write(&mut files, &id, &stem, &car, &frame, o.content.as_deref(), o.profile, o.keep_brakes, o.min_lock)?;
    notes.extend(phys.notes);
    let states = identity::set_final_state_pi(&mut files, &base, phys.perf)?;
    notes.push(format!("identity: garage rating {:.1} in {states} final state(s)", phys.perf));
    // sound: a failed sound stage is rolled back, the donor's (the game's shared) sounds stay
    if let Some(c) = o.content.as_deref().filter(|c| c.is_file()) {
        files.begin();
        match crate::sound::car_stage(&mut files, &id, &car.dir, c, &o.out.join(format!(".sound_{id}"))) {
            Ok(n) => {
                files.commit();
                notes.extend(n);
            }
            Err(e) => {
                files.rollback();
                notes.push(format!("sound: failed ({e:#}) - the donor's sound kept"));
            }
        }
    }
    let body_bounds = {
        let ep = format!("{base}\\meshes\\{stem}_exterior.mesh");
        files.get(&ep).and_then(|b| Msg::parse(b).ok()).map(|m| {
            let v = |x: Option<Msg>| x.map(|m| [m.f32(1).unwrap_or(0.0), m.f32(2).unwrap_or(0.0), m.f32(3).unwrap_or(0.0)]).unwrap_or([0.0; 3]);
            (v(m.sub(6)), v(m.sub(7)))
        }).unwrap_or(([-1.0, 0.0, -2.0], [1.0, 1.5, 2.0]))
    };
    notes.extend(collider(&mut files, &car, &frame, &id, &stem, body_bounds));
    // the driver last: a failed driver stage is rolled back, the donor's stays
    files.begin();
    match driver::write(&mut files, &id, &car, &frame) {
        Ok(n) => {
            files.commit();
            notes.extend(n);
        }
        Err(e) => {
            files.rollback();
            notes.push(format!("driver: failed ({e:#}) - the donor's driver kept"));
        }
    }
    let pruned = prune(&mut files, &id);
    notes.extend(tex.notes.iter().cloned());
    notes.push(format!("textures: {} written, {:.0} MB of mips; {} unreferenced donor files pruned", tex.count, tex.bytes as f64 / 1048576.0, pruned));
    let perf = phys.perf;
    let pkg = crate::files::write_mod(&mut files, &o.out, &crate::files::ModMeta {
        id: &id, mech_preset: &ident.mech_preset(), display_name: &ident.display, perf, source_folder: Some(&car.folder), ac1_traction: Some(&phys.derived.ac1_traction),
    })?;
    notes.push(format!("done in {:.1} s: {} files -> {}", t0.elapsed().as_secs_f64(), files.map.len(), pkg.display()));
    Ok(Report { id, package: pkg, notes })
}

/// Options with the defaults: the game's content.kspkg; the donor and
/// refmats are left empty and found (or built) by `convert` only when the
/// caller does not name them.
pub fn default_opts(ac1: &Path, out: &Path) -> Opts {
    let c = crate::io::gamedir::evo_content();
    Opts { ac1: ac1.to_path_buf(), out: out.to_path_buf(), id: None, donor_zip: PathBuf::new(), refmats: PathBuf::new(), content: c.is_file().then_some(c), profile: physics::profile::Choice::from_env(), name: None, brand: None, keep_brakes: false, min_lock: None }
}

/// The donor's files the physics stage reads (no meshes, textures, sounds),
/// for `physics_dry_run`.
pub fn physics_donor(donor_zip: &Path) -> Result<std::collections::BTreeMap<String, Vec<u8>>> {
    let d = crate::files::load_donor(donor_zip, identity::DONOR_ID)?;
    Ok(d.map.into_iter().filter(|(k, _)| {
        let l = k.to_lowercase();
        ![".mesh", ".texture", ".texturemips", ".bank", ".dds", ".png"].iter().any(|e| l.ends_with(e))
    }).collect())
}

/// Only the physics stage of one AC1 car, on a copy of the donor's physics
/// files: what the converter would write for its drivetrain, engine, tyres,
/// setup and AI, without building the package (a library sweep).
pub fn physics_dry_run(ac1: &Path, donor: &std::collections::BTreeMap<String, Vec<u8>>, content: Option<&Path>) -> Result<serde_json::Value> {
    let car = Car::open(ac1)?;
    let ident = identity::identity(&car, None, &HashSet::new(), &Default::default());
    let mut d = Files::default();
    for (k, v) in donor {
        d.put(k, v.clone());
    }
    let mut files = identity::clone_donor(d, &ident);
    let k = car.kn5()?;
    let frame = frame::Frame::build(&car, &k);
    let r = physics::write(&mut files, &ident.id, &ident.stem, &car, &frame, content, physics::profile::Choice::from_env(), false, None)?;
    let cdp = files.find_ci(&format!("{}\\data\\cardata.car", ident.base())).ok_or_else(|| anyhow::anyhow!("no CarData"))?;
    let cd = Msg::parse(files.get(&cdp).unwrap())?;
    let dt = car.ini("drivetrain.ini");
    let sec = |s: &str| dt.section(s).map(|m| m.iter().map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone()))).collect::<serde_json::Map<_, _>>());
    let ctrl: Vec<&String> = car.data.keys().filter(|k| k.starts_with("ctrl_") && (k.contains("awd") || k.contains("lock") || k.contains("diff"))).collect();
    let setup_diff = cd.str(19).and_then(|p| files.get(&p).cloned()).and_then(|b| Msg::parse(&b).ok())
        .and_then(|s| s.sub(1)).and_then(|g| g.sub(4))
        .map(|m| [m.f32(1).unwrap_or(0.0), m.f32(2).unwrap_or(0.0), m.f32(3).unwrap_or(0.0)]);
    let (de, dw) = physics::dtcheck::check(&cd, Some(&r.derived.ac1_traction));
    Ok(serde_json::json!({
        "car": car.folder,
        "ac1": { "traction": r.derived.ac1_traction, "differential": sec("DIFFERENTIAL"), "awd": sec("AWD"), "awd2": sec("AWD2"), "ctrl": ctrl },
        "evo": physics::dtcheck::summary(&cd),
        "setup_diff": setup_diff,
        "check": { "errors": de, "warnings": dw },
        "notes": r.notes.iter().filter(|n| n.contains("drivetrain") || n.contains("AWD") || n.contains("lock")).collect::<Vec<_>>(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::texture::Image;

    #[test]
    fn ac1_shadow_quad_is_twice_the_ini_half_sizes() {
        let ini = crate::ac1::ini::parse("[SETTINGS]\nWIDTH=1.0\nLENGTH=2.8\n");
        assert_eq!(ac1_shadow_size(&ini), Some((2.0, 5.6)));
        assert_eq!(ac1_shadow_size(&crate::ac1::ini::parse("")), None);
        assert_eq!(ac1_shadow_size(&crate::ac1::ini::parse("[SETTINGS]\nWIDTH=0\nLENGTH=2.8\n")), None);
    }

    #[test]
    fn ac1_shadow_turns_half_a_turn_at_stock_strength() {
        // 10 x 10, AC1's full shadow (191) in the top three rows, front at the top
        let (w, h) = (10u32, 10u32);
        let mut px = vec![0u8; (w * h * 4) as usize];
        for (i, p) in px.chunks_exact_mut(4).enumerate() {
            let v = if i < 30 { 191 } else { 0 };
            p.copy_from_slice(&[v, v, v, 255]);
        }
        // a mark on the car's left of the front row, and a transparent white texel
        px[4..8].copy_from_slice(&[96, 96, 96, 255]);
        px[(9 * w as usize + 9) * 4..][..4].copy_from_slice(&[255, 255, 255, 0]);
        let out = shadow_image(&Image { w, h, px }).unwrap();
        let at = |x: u32, y: u32| out.px[((y * w + x) * 4) as usize];
        // the front now at the bottom, at the stock p99
        assert_eq!(at(0, 9), STOCK_SHADOW_P99 as u8);
        assert_eq!(at(9, 7), STOCK_SHADOW_P99 as u8);
        assert_eq!(at(5, 5), 0);
        // left and right swap (half a turn, not a mirror)
        assert_eq!(at(8, 9), (96.0 * STOCK_SHADOW_P99 / 191.0).round() as u8);
        // alpha counts: the transparent texel casts nothing
        assert_eq!(at(0, 0), 0);
        // no shadow at all: nothing to use
        assert!(shadow_image(&Image { w: 4, h: 4, px: vec![0; 64] }).is_none());
    }
}
