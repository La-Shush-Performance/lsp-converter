//! AC1 wheels -> EVO rims, rim blur and tyre meshes.
//!
//! Stock layout (the donor and ks_mazda_mx5_na): one rim mesh per corner side
//! (`rim_lf` / `rim_rf`, and `_lr` / `_rr` on staggered cars) and its blur
//! twin, centred on the wheel (no bones: the actor attaches them at
//! WHEEL_xx); a `.rimmesh` per axle naming its left / right meshes, blur
//! meshes and compatible `.tyremesh`es; a `.compatiblerim` naming the front
//! and rear rimmesh and tyremesh; one symmetric tyre mesh per size, attached
//! at TYRE_xx.
//!
//! Rims are written in the wheel frame (`Frame::to_wheel`: relative to the
//! wheel centre, car axes, small WHEEL node camber / toe removed). An axle
//! whose AC1 geometry has no blur gets its static rim in the blur slot.
//!
//! Tyres are generated: a surface of revolution (80 segments, like the stock
//! tyre) whose profile is the outer envelope of the AC1 tyre measured in the
//! wheel frame. Fitment rule (ours): the AC1 model's rim was made around the
//! model's tyre, not tyres.ini, so the tyre keeps the model's width, centre
//! and bead radius and only its tread moves to the tyres.ini RADIUS (the
//! ground contact). A tyre off the wheel node (the S14's by 18..22 mm) is
//! centred on it by moving that corner's rims the same distance, since EVO
//! draws one symmetric tyre at the node. Sized from tyres.ini instead, the
//! PS13's 235 tyre sat inside its 264 mm rim barrel and the S14's bead
//! floated 8 mm off the rim lip. Without a model tyre: tyres.ini and the
//! stock profile. UVs follow the stock
//! convention, measured on the stock 215/35 R18 tyre: u = angle / 360 deg
//! around the axle, v = arc length along the profile normalised to 0..1 from
//! the +x bead to the -x bead (tread centre at 0.5).

use anyhow::Result;
use std::collections::BTreeMap;

use crate::ac1::kn5::Kn5;
use crate::car::frame::{self, Frame, CORNERS};
use crate::car::geometry::{self, Built, Geo, Inst, Section, Vert};
use crate::car::materials::{self, Kind, RimSlot, Templates, Tex};
use crate::files::Files;
use crate::io::pb::Msg;
use crate::io::texture::Image;

pub struct Wheels {
    pub notes: Vec<String>,
    /// measured AC1 tyre radius per corner (wheel frame), for the report
    pub measured_radius: BTreeMap<String, f64>,
    /// written tyre radius per axle (front, rear)
    pub tyre_radius: [f64; 2],
    /// every rim material written (path, kn5 material, slot), built under
    /// the default skin: the liveries give a skin its own when its rim
    /// textures differ (`skins::write`)
    pub rim_mats: Vec<(String, usize, materials::RimSlot)>,
    /// the AC1 tyre's own artwork baked for the generated tyre (`bake_tyre`)
    pub tyre_look: Option<TyreLook>,
}

/// The AC1 tyre's colour map baked onto the generated tyre's UVs (stock
/// layout: u around the axle, v bead to bead), and its motion-blur twin.
#[derive(Clone, Debug)]
pub struct TyreLook {
    pub colour: String,
    pub blur: String,
}

/// Baked tyre size: half the stock 4096 x 1024 (AC1 tyre maps are 1024..2048).
const BAKE_W: u32 = 2048;
const BAKE_H: u32 = 512;
/// Below this share of the baked texels covered by the AC1 tyre, the bake
/// is dropped (a tyre made of separate parts, or mapped in a way the
/// projection cannot follow) and the game's tread stays.
const BAKE_MIN_COVER: f64 = 0.6;
/// The stock tyre rubber's tone, sRGB 0..1: the median of the game's generic
/// slick and road maps (racing_generic_1, road_1: means 0.216).
const STOCK_RUBBER: f32 = 0.216;

/// Project (axial, radius) onto a profile polyline: (segment, fraction, distance).
fn project(prof: &[(f64, f64)], a: f64, r: f64) -> (usize, f64, f64) {
    let mut best = (0usize, 0.0, f64::MAX);
    for j in 0..prof.len().saturating_sub(1) {
        let (p, q) = (prof[j], prof[j + 1]);
        let (da, dr) = (q.0 - p.0, q.1 - p.1);
        let l2 = (da * da + dr * dr).max(1e-12);
        let t = (((a - p.0) * da + (r - p.1) * dr) / l2).clamp(0.0, 1.0);
        let (x, y) = (p.0 + t * da - a, p.1 + t * dr - r);
        let d = (x * x + y * y).sqrt();
        if d < best.2 {
            best = (j, t, d);
        }
    }
    best
}

fn sample(img: &Image, u: f64, v: f64) -> [f32; 4] {
    let (w, h) = (img.w as i64, img.h as i64);
    let x = u * w as f64 - 0.5;
    let y = v * h as f64 - 0.5;
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = ((x - x0) as f32, (y - y0) as f32);
    let px = |xi: i64, yi: i64| -> [f32; 4] {
        let (xi, yi) = (xi.rem_euclid(w), yi.rem_euclid(h));
        let i = ((yi * w + xi) * 4) as usize;
        [img.px[i] as f32, img.px[i + 1] as f32, img.px[i + 2] as f32, img.px[i + 3] as f32]
    };
    let (a, b, c, d) = (px(x0 as i64, y0 as i64), px(x0 as i64 + 1, y0 as i64), px(x0 as i64, y0 as i64 + 1), px(x0 as i64 + 1, y0 as i64 + 1));
    let mut o = [0f32; 4];
    for k in 0..4 {
        o[k] = (a[k] * (1.0 - fx) + b[k] * fx) * (1.0 - fy) + (c[k] * (1.0 - fx) + d[k] * fx) * fy;
    }
    o
}

/// The AC1 tyre's own look on the generated tyre (ours). EVO draws a tyre
/// with a texture set (`.tyretextures`) laid out on the stock UVs, and the
/// converter's tyre is generated, so the AC1 model's sidewall artwork (the
/// brand, the markings) was lost: a converted car showed the game's road or
/// plain slick tread instead (bob76 on OverTake, 14 Nov 2026: "the slicks
/// need brands and markings"). Each AC1 tyre triangle facing out of the
/// tyre and within 20 mm of the generated profile is drawn into the stock
/// layout: u = its angle about the axle (the lathe's), v = its place along
/// the profile, projected onto the AC1-sized profile and read off the
/// generated one (the same points; so the sidewall stays on the sidewall
/// when the tread moves to the tyres.ini radius). The nearest surface wins
/// a texel; see-through parts (decal letters) are blended over. Gaps close
/// from their neighbours. `prof_model` / `prof`: the same profile at the
/// AC1 model's radius and at the written one; `mid`: the tread's axial
/// centre. None below `BAKE_MIN_COVER`.
#[allow(clippy::too_many_arguments)]
pub fn bake_tyre(geos: &BTreeMap<usize, Geo>, k: &Kn5, tex: &Tex, skin: Option<&std::path::Path>, prof_model: &[(f64, f64)], prof: &[(f64, f64)], mid: f64) -> Option<(Image, f64)> {
    let mut layers: Vec<(usize, &Geo, Image, bool)> = Vec::new();
    for (mi, g) in geos {
        let m = &k.materials[*mi];
        let Some((img, _, _)) = m.texture("txDiffuse").and_then(|d| tex.image(d, skin)) else { continue };
        // see-through only as AC1 draws it: blended or alpha-tested (a tyre
        // map's alpha can hold other data: Neonstride's, whose bake came
        // out empty when its alpha marked it a decal)
        layers.push((*mi, g, img, m.blend_mode == 1 || m.alpha_tested));
    }
    // opaque parts first, then the see-through ones over them
    layers.sort_by_key(|x| (x.3, x.0));
    let l: Vec<(&Geo, &Image, bool)> = layers.iter().map(|x| (x.1, &x.2, x.3)).collect();
    bake_layers(&l, prof_model, prof, mid)
}

/// `bake_tyre` past the texture lookup: (geometry, its colour map,
/// see-through) in drawing order.
fn bake_layers(layers: &[(&Geo, &Image, bool)], prof_model: &[(f64, f64)], prof: &[(f64, f64)], mid: f64) -> Option<(Image, f64)> {
    if prof_model.len() != prof.len() || prof.len() < 3 {
        return None;
    }
    let mut arc = vec![0f64];
    for w in prof.windows(2) {
        arc.push(arc.last().unwrap() + ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt());
    }
    let total = arc.last().copied().unwrap_or(1.0).max(1e-9);
    let seg_len: Vec<f64> = (0..prof.len() - 1).map(|j| arc[j + 1] - arc[j]).collect();
    let (w, h) = (BAKE_W as usize, BAKE_H as usize);
    let mut col = vec![[0f32; 4]; w * h];
    let mut dist = vec![f32::MAX; w * h];
    for &(g, img, see) in layers {
        for t in g.idx.chunks_exact(3) {
            let vs = [&g.verts[t[0] as usize], &g.verts[t[1] as usize], &g.verts[t[2] as usize]];
            let mut tu = [0f64; 3];
            let mut tv = [0f64; 3];
            let mut dsum = 0.0;
            let mut out = 0.0;
            for (n, v) in vs.iter().enumerate() {
                let (x, y, z) = (v.pos[0] as f64 - mid, v.pos[1] as f64, v.pos[2] as f64);
                let r = (y * y + z * z).sqrt().max(1e-9);
                tu[n] = (y.atan2(z) + std::f64::consts::PI) / std::f64::consts::TAU;
                let (j, f, d) = project(prof_model, x, r);
                tv[n] = (arc[j] + f * seg_len[j]) / total;
                dsum += d;
                // the surface faces out of the tyre: along the profile's outward normal
                let (da, dr) = (prof_model[j + 1].0 - prof_model[j].0, prof_model[j + 1].1 - prof_model[j].1);
                let l = (da * da + dr * dr).sqrt().max(1e-12);
                let (na, nr) = (dr / l, -da / l);
                let vr = (v.nrm[1] as f64 * y + v.nrm[2] as f64 * z) / r;
                out += v.nrm[0] as f64 * na + vr * nr;
            }
            let d = dsum / 3.0;
            if d > 0.02 || out / 3.0 < 0.3 {
                continue;
            }
            // across the seam at u = 0 / 1
            let umin = tu.iter().copied().fold(f64::MAX, f64::min);
            let umax = tu.iter().copied().fold(f64::MIN, f64::max);
            if umax - umin > 0.5 {
                for u in tu.iter_mut() {
                    if *u < 0.5 {
                        *u += 1.0;
                    }
                }
            }
            let px: Vec<(f64, f64)> = (0..3).map(|n| (tu[n] * w as f64, tv[n] * h as f64)).collect();
            let area = (px[1].0 - px[0].0) * (px[2].1 - px[0].1) - (px[2].0 - px[0].0) * (px[1].1 - px[0].1);
            if area.abs() < 1e-9 {
                continue;
            }
            let x0 = px.iter().map(|p| p.0).fold(f64::MAX, f64::min).floor() as i64;
            let x1 = px.iter().map(|p| p.0).fold(f64::MIN, f64::max).ceil() as i64;
            let y0 = (px.iter().map(|p| p.1).fold(f64::MAX, f64::min).floor() as i64).max(0);
            let y1 = (px.iter().map(|p| p.1).fold(f64::MIN, f64::max).ceil() as i64).min(h as i64 - 1);
            if x1 - x0 > w as i64 {
                continue;
            }
            for yy in y0..=y1 {
                for xx in x0..=x1 {
                    let (cx, cy) = (xx as f64 + 0.5, yy as f64 + 0.5);
                    let b1 = ((cx - px[0].0) * (px[2].1 - px[0].1) - (px[2].0 - px[0].0) * (cy - px[0].1)) / area;
                    let b2 = ((px[1].0 - px[0].0) * (cy - px[0].1) - (cx - px[0].0) * (px[1].1 - px[0].1)) / area;
                    let b0 = 1.0 - b1 - b2;
                    if b0 < -1e-6 || b1 < -1e-6 || b2 < -1e-6 {
                        continue;
                    }
                    let su = b0 * vs[0].uv[0] as f64 + b1 * vs[1].uv[0] as f64 + b2 * vs[2].uv[0] as f64;
                    let sv = b0 * vs[0].uv[1] as f64 + b1 * vs[1].uv[1] as f64 + b2 * vs[2].uv[1] as f64;
                    let s = sample(img, su, sv);
                    let i = yy as usize * w + xx.rem_euclid(w as i64) as usize;
                    if see {
                        if dist[i] == f32::MAX {
                            continue;
                        }
                        let a = s[3] / 255.0;
                        for c in 0..3 {
                            col[i][c] = col[i][c] * (1.0 - a) + s[c] * a;
                        }
                    } else if (d as f32) < dist[i] {
                        dist[i] = d as f32;
                        col[i] = [s[0], s[1], s[2], 255.0];
                    }
                }
            }
        }
    }
    let covered = dist.iter().filter(|d| **d < f32::MAX).count();
    let cover = covered as f64 / (w * h) as f64;
    if cover < BAKE_MIN_COVER {
        return Some((Image { w: 0, h: 0, px: Vec::new() }, cover));
    }
    // close the gaps from their neighbours (u wraps), the rest the mean
    let mut have: Vec<bool> = dist.iter().map(|d| *d < f32::MAX).collect();
    for _ in 0..32 {
        let mut grew = false;
        let prev = have.clone();
        let src = col.clone();
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                if prev[i] {
                    continue;
                }
                let mut acc = [0f32; 3];
                let mut n = 0.0;
                for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
                    let yy = y as i64 + dy;
                    if yy < 0 || yy >= h as i64 {
                        continue;
                    }
                    let j = yy as usize * w + (x as i64 + dx).rem_euclid(w as i64) as usize;
                    if prev[j] {
                        for c in 0..3 {
                            acc[c] += src[j][c];
                        }
                        n += 1.0;
                    }
                }
                if n > 0.0 {
                    col[i] = [acc[0] / n, acc[1] / n, acc[2] / n, 255.0];
                    have[i] = true;
                    grew = true;
                }
            }
        }
        if !grew {
            break;
        }
    }
    let mut mean = [0f32; 3];
    for (c, _) in col.iter().zip(&have).filter(|(_, h)| **h) {
        for k in 0..3 {
            mean[k] += c[k];
        }
    }
    let nh = have.iter().filter(|h| **h).count().max(1) as f32;
    let filled: Vec<[f32; 4]> = col.iter().zip(&have).map(|(c, h)| if *h { *c } else { [mean[0] / nh, mean[1] / nh, mean[2] / nh, 255.0] }).collect();
    // the rubber to the stock tone: AC1 lights a tyre map its own way, and
    // its maps run far darker (Momentum Ice: mean 0.084 sRGB) than every
    // stock EVO tyre's (0.21 .. 0.25, the generic ones 0.216), which in game
    // reads as ink black. A screen blend lifts the median (the rubber) to
    // STOCK_RUBBER and leaves light lettering nearly as it was.
    let mut luma: Vec<f32> = filled.iter().step_by(7).map(|c| (c[0] + c[1] + c[2]) / 765.0).collect();
    luma.sort_by(|a, b| a.total_cmp(b));
    let med = luma.get(luma.len() / 2).copied().unwrap_or(STOCK_RUBBER);
    let lift = if med < STOCK_RUBBER { (STOCK_RUBBER - med) / (1.0 - med) } else { 0.0 };
    let px: Vec<u8> = filled.iter().flat_map(|c| {
        let s = |x: f32| ((x / 255.0 + lift - x / 255.0 * lift) * 255.0).round().clamp(0.0, 255.0) as u8;
        [s(c[0]), s(c[1]), s(c[2]), 255]
    }).collect();
    Some((Image { w: BAKE_W, h: BAKE_H, px }, cover))
}

/// Both sidewalls wear the outer one's artwork, as a real tyre does. EVO
/// draws the right-hand tyres as the same mesh turned 180 deg about the
/// vertical ((x, z) -> (-x, -z): u -> 0.5 - u, v -> 1 - v), so their outside
/// is the half that faces in on the left; the stock maps carry the
/// sidewall writing twice, one band the other turned 180 deg. An AC1 tyre's
/// inner face is often a mirror of its outer one (the tyre mirrored in the
/// model), and baked as it is, the right-hand tyres of the 14 Nov test build
/// (Momentum Ice) read backwards. `outer_low_v`: the outer sidewall is the
/// v < 0.5 half (+x, a left-hand tyre in car axes, `frame`).
pub fn outer_both_sides(img: &mut Image, outer_low_v: bool) {
    let (w, h) = (img.w as usize, img.h as usize);
    let src = img.px.clone();
    for y in 0..h {
        let inner = if outer_low_v { y >= h / 2 } else { y < h / 2 };
        if !inner {
            continue;
        }
        let sy = h - 1 - y;
        for x in 0..w {
            let sx = (w / 2 + w - x - 1) % w;
            let (i, j) = ((y * w + x) * 4, (sy * w + sx) * 4);
            img.px[i..i + 4].copy_from_slice(&src[j..j + 4]);
        }
    }
}

/// The spinning tyre's colour (the stock `_blur_c` is half size and smeared
/// around the axle): half size, then a box blur along u (1/32 of a turn).
pub fn blur_tyre(img: &Image) -> Image {
    let half = crate::io::texture::half(img, true);
    let (w, h) = (half.w as usize, half.h as usize);
    let r = (w / 64).max(1) as i64;
    let mut px = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let mut acc = [0u32; 3];
            for dx in -r..=r {
                let j = (y * w + (x as i64 + dx).rem_euclid(w as i64) as usize) * 4;
                for c in 0..3 {
                    acc[c] += half.px[j + c] as u32;
                }
            }
            let n = (2 * r + 1) as u32;
            let i = (y * w + x) * 4;
            for c in 0..3 {
                px[i + c] = (acc[c] / n) as u8;
            }
            px[i + 3] = 255;
        }
    }
    Image { w: half.w, h: half.h, px }
}

/// Every tyre texture set the car names (the actor's f103.f18 and each
/// `.compatibletyres` entry's f2) is swapped for a car-local twin wearing
/// the baked AC1 colour (`bake_tyre`): its colour and blur colour maps are
/// ours, its normal and occlusion maps stay the set's own (the tread the
/// tyre rule picked: slick, road or wet). Returns the note.
pub fn apply_tyre_look(files: &mut Files, id: &str, look: &TyreLook, game: Option<&crate::validate::Game>) -> Result<Option<String>> {
    let base = format!("content\\cars\\{id}");
    let bl = base.to_lowercase();
    let read = |files: &Files, p: &str| -> Option<Vec<u8>> { files.get(p).cloned().or_else(|| game.and_then(|g| g.read(p))) };
    let mut twins: BTreeMap<String, String> = BTreeMap::new();
    let mut twin = |files: &mut Files, p: &str| -> Option<String> {
        let key = p.to_lowercase();
        if let Some(t) = twins.get(&key) {
            return Some(t.clone());
        }
        let mut m = Msg::parse(&read(files, p)?).ok()?;
        m.set_str(5, &look.colour);
        m.set_str(9, &look.blur);
        let stem = key.rsplit('\\').next().unwrap_or("tyre").trim_end_matches(".tyretextures").to_string();
        let out = format!("{base}\\texture\\tyre\\{stem}_ac1.tyretextures");
        files.put(&out, m.encode());
        twins.insert(key, out.clone());
        Some(out)
    };
    let keys: Vec<String> = files.map.keys().cloned().collect();
    for p in &keys {
        let l = p.to_lowercase();
        if !l.starts_with(&bl) {
            continue;
        }
        if l.ends_with(".actor") && !l[bl.len() + 1..].contains('\\') {
            let mut m = Msg::parse(files.get(p).unwrap())?;
            let a = m.msg_mut(103)?;
            if let Some(t) = a.str(18).filter(|s| s.to_lowercase().ends_with(".tyretextures")) {
                if let Some(n) = twin(files, &t) {
                    a.set_str(18, &n);
                    files.put(p, m.encode());
                }
            }
        } else if l.ends_with(".compatibletyres") {
            let mut m = Msg::parse(files.get(p).unwrap())?;
            for i in m.idx(3) {
                let g = m.open(i)?;
                if let Some(t) = g.str(2) {
                    if let Some(n) = twin(files, &t) {
                        g.set_str(2, &n);
                    }
                }
            }
            files.put(p, m.encode());
        }
    }
    Ok((!twins.is_empty()).then(|| format!("wheels: the AC1 tyre's own artwork (sidewall markings) on {} tyre texture set(s); their tread normals stay", twins.len())))
}

/// One corner's rim or blur geometry in the wheel frame, by kn5 material;
/// `shift` moves it along the axle (the corner's tyre centre to 0).
#[allow(clippy::too_many_arguments)]
fn corner_geo(insts: &[Inst], parts: &[usize], f: &Frame, c: &str, kind: Kind, k: &Kn5, h: &materials::Hints, shift: f64) -> BTreeMap<usize, Geo> {
    let mut by: BTreeMap<usize, Geo> = BTreeMap::new();
    let unrot = frame::mul(&f.wheel_unrotate(c), &frame::translation([-shift, 0.0, 0.0]));
    let centre = f.wheel_centres.get(c).copied().unwrap_or([0.0; 3]);
    for &ii in parts {
        let i = &insts[ii];
        if i.corner != Some(c) {
            continue;
        }
        let Some(m) = k.materials.get(i.material) else { continue };
        if materials::kind_of(m, &i.node, true, i.blur, false, h) != kind {
            continue;
        }
        // AC1 world -> EVO -> wheel frame
        let to_evo = f.evo_m(&i.world);
        let to_wheel = frame::mul(&frame::mul(&to_evo, &frame::translation([-centre[0], -centre[1], -centre[2]])), &unrot);
        geometry::append(by.entry(i.material).or_default(), i.mesh, &to_wheel, 0);
    }
    for g in by.values_mut() {
        geometry::compact(g);
    }
    by
}

/// Each wheel centre moved along its axle to the middle of its AC1 tyre's
/// tread, before the body and wheel stages: the HUB_xx bone (and the DISC /
/// SUSP bones under it) and EVO's one symmetric tyre then sit where AC1's
/// tyre is, and everything at that corner keeps AC1's layout around it.
/// The Old Man's WHEEL_xx nodes sit about 80 mm inboard of its tyres (at
/// the hub face; suspensions.ini's track agrees with the tyres). Moving only
/// the rims to centre the tyre put the five-spoke face 82 mm in, behind the
/// brake disc and caliper, which stayed on the HUB bone at AC1's place.
/// A corner without its own tyre takes its axle partner's offset, mirrored;
/// under 5 mm is measuring noise. Returns the note.
pub fn centre_on_tyres(f: &mut Frame, insts: &[Inst], k: &Kn5, h: &materials::Hints) -> Option<String> {
    let parts: Vec<usize> = insts.iter().enumerate().filter(|(_, i)| i.under == geometry::Under::Wheel).map(|(n, _)| n).collect();
    let own: BTreeMap<&str, Option<f64>> = CORNERS.iter().map(|c| (*c, tread_mid(&tyre_points(insts, &parts, f, c, k, h)))).collect();
    let partner = |c: &str| match c { "LF" => "RF", "RF" => "LF", "LR" => "RR", _ => "LR" };
    let mut moved = Vec::new();
    for c in CORNERS {
        let Some(mid) = own[c].or(own[partner(c)].map(|m| -m)).filter(|m| m.abs() >= 0.005) else { continue };
        // the wheel frame's axle (x) in car space
        let axle = frame::inv(&f.wheel_unrotate(c)).map(|r| frame::xdir(&r, [1.0, 0.0, 0.0])).unwrap_or([1.0, 0.0, 0.0]);
        if let Some(p) = f.wheel_centres.get_mut(c) {
            for a in 0..3 {
                p[a] += axle[a] * mid;
            }
            moved.push(format!("{c} {:+.0} mm", mid * 1000.0));
        }
    }
    (!moved.is_empty()).then(|| format!("wheels: the AC1 tyres sit off the wheel nodes ({}): each wheel centre (hub, disc and suspension bones, EVO's tyre) moved to its tyre", moved.join(", ")))
}

/// (axial |x|, radius) of every vertex of a corner's tyre, in the wheel frame.
fn tyre_points(insts: &[Inst], parts: &[usize], f: &Frame, c: &str, k: &Kn5, h: &materials::Hints) -> Vec<(f64, f64)> {
    let geos = corner_geo(insts, parts, f, c, Kind::Tyre, k, h, 0.0);
    let mut pts = Vec::new();
    for g in geos.values() {
        for v in &g.verts {
            let (x, y, z) = (v.pos[0] as f64, v.pos[1] as f64, v.pos[2] as f64);
            pts.push((x, (y * y + z * z).sqrt()));
        }
    }
    pts
}

/// The axial middle of a tyre's tread (the vertices within 3 % of its
/// largest radius): an AC1 tyre can sit off the wheel node.
pub fn tread_mid(pts: &[(f64, f64)]) -> Option<f64> {
    let rmax = pts.iter().map(|p| p.1).fold(0.0, f64::max);
    let tread: Vec<f64> = pts.iter().filter(|p| p.1 >= rmax * 0.97).map(|p| p.0).collect();
    (tread.len() >= 4).then(|| (tread.iter().copied().fold(f64::MAX, f64::min) + tread.iter().copied().fold(f64::MIN, f64::max)) / 2.0)
}

/// (width: its axial extent, bead radius) of the AC1 model's tyre; None
/// for fewer than 50 vertices.
pub fn model_size(pts: &[(f64, f64)]) -> Option<(f64, f64)> {
    if pts.len() < 50 {
        return None;
    }
    let w = pts.iter().map(|p| p.0).fold(f64::MIN, f64::max) - pts.iter().map(|p| p.0).fold(f64::MAX, f64::min);
    let bead = pts.iter().map(|p| p.1).fold(f64::MAX, f64::min);
    (w > 0.04 && bead > 0.05).then_some((w, bead))
}

/// The outer profile of a tyre: (axial offset, radius) from the +x bead over
/// the tread to the -x bead, symmetric. `pts` = (axial, radius) samples.
/// Measured, the bead stays at the model's radius (it sits on the model's
/// rim) and the sidewall stretches to `radius`; `width` sets the axial
/// scale. Falls back to the stock 215/35 R18 profile shape.
pub fn profile(pts: &[(f64, f64)], radius: f64, width: f64, rim_radius: f64) -> Vec<(f64, f64)> {
    // stock profile (half, from the bead up to the tread centre), normalised:
    // x / half width, (r - bead r) / (R - bead r)
    const STOCK: [(f64, f64); 10] = [(0.877, 0.0), (0.98, 0.15), (1.0, 0.186), (0.997, 0.22), (0.975, 0.67), (0.929, 0.78), (0.866, 0.87), (0.772, 0.92), (0.673, 0.96), (0.0, 1.0)];
    let half = width / 2.0;
    let bead = rim_radius.min(radius - 0.02);
    let fallback = || -> Vec<(f64, f64)> { STOCK.iter().map(|(x, r)| (x * half, bead + r * (radius - bead))).collect() };
    if pts.len() < 50 {
        return mirror(&fallback());
    }
    let rmax = pts.iter().map(|p| p.1).fold(0.0, f64::max);
    // measure from the tyre's own axial centre, the middle of its tread: an
    // AC1 tyre can sit off the wheel node (the Supra's rears by 18 mm), and
    // folding both sidewalls about the node skews the profile
    let mid = tread_mid(pts).unwrap_or(0.0);
    let pts: Vec<(f64, f64)> = pts.iter().map(|&(a, r)| (a - mid, r)).collect();
    let amax = pts.iter().map(|p| p.0.abs()).fold(0.0, f64::max);
    if rmax < 0.1 || amax < 0.02 {
        return mirror(&fallback());
    }
    // envelope around a centre on the profile's mid radius
    let rc = (bead.max(rmax * 0.6) + rmax) / 2.0;
    const N: usize = 48;
    let mut best = vec![None::<(f64, f64, f64)>; N + 1];
    for &(a, r) in &pts {
        let (a, r) = (a.abs(), r);
        if r < bead * 0.98 {
            continue;
        }
        let (dx, dy) = (a, r - rc);
        let phi = dy.atan2(dx); // -pi/2 (down) .. pi/2 (up, tread centre)
        let t = ((phi + std::f64::consts::FRAC_PI_2) / std::f64::consts::PI * N as f64).round();
        if !(0.0..=N as f64).contains(&t) {
            continue;
        }
        // below the centre only the sidewall counts (not the inner liner)
        if phi < 0.0 && a < amax * 0.6 {
            continue;
        }
        let d = (dx * dx + dy * dy).sqrt();
        let slot = &mut best[t as usize];
        if slot.map(|s| d > s.0).unwrap_or(true) {
            *slot = Some((d, a, r));
        }
    }
    let mut ordered: Vec<(f64, f64)> = best.iter().flatten().map(|&(_, a, r)| (a, r)).collect();
    if ordered.len() < 6 {
        return mirror(&fallback());
    }
    // the tread centre point at a = 0
    let top = ordered.last().copied().unwrap_or((0.0, rmax));
    if top.0 > 1e-4 {
        ordered.push((0.0, top.1.max(rmax)));
    }
    // a tyre's outer section is convex: an angle where a sparse sidewall has
    // no vertex keeps a point deep inside the tyre (the Supra's rear: 9 mm
    // from the centre plane at mid-sidewall), which the lathe turned into a
    // groove all round - "a tyre on the rim and one floating above it"
    let ordered = convex_chain(&ordered);
    // axial to `width`; radially the bead stays put on its rim and the
    // sidewall stretches to the tyres.ini radius (scaled about the axle,
    // a bigger tyres.ini radius lifted the bead off the rim lip)
    let amax2 = ordered.iter().map(|p| p.0).fold(0.0, f64::max).max(1e-6);
    let rmax2 = ordered.iter().map(|p| p.1).fold(0.0, f64::max).max(1e-6);
    let rmin2 = ordered.iter().map(|p| p.1).fold(f64::MAX, f64::min).min(radius - 0.02);
    let k = (radius - rmin2) / (rmax2 - rmin2).max(1e-6);
    let scaled: Vec<(f64, f64)> = ordered.iter().map(|&(a, r)| (a * half / amax2, rmin2 + (r - rmin2).max(0.0) * k)).collect();
    // thin to about 10 points (the stock tyre has 19 for both halves)
    let mut thin = vec![scaled[0]];
    let step = (scaled.len() as f64 / 9.0).max(1.0);
    let mut t = step;
    while (t as usize) < scaled.len() - 1 {
        thin.push(scaled[t as usize]);
        t += step;
    }
    thin.push(*scaled.last().unwrap());
    mirror(&thin)
}

/// EVO's GT3 slick section (half, bead -> tread centre), normalised as
/// `profile`'s STOCK: x / half width, (r - bead r) / (R - bead r). Measured
/// on the stock 30/68 R18 and 325/705 R18 racing tyres (common_assets): the
/// sidewall stands nearly straight (3 % wider than the bead at a third of its
/// height) and turns in at the shoulder.
const STOCK_RACING: [(f64, f64); 9] = [(0.97, 0.0), (1.0, 0.31), (0.99, 0.48), (0.955, 0.64), (0.89, 0.83), (0.82, 0.915), (0.74, 0.96), (0.37, 0.985), (0.0, 1.0)];
/// EVO's road tyre section (the stock 215/35 R18), normalised the same way.
const STOCK_ROAD: [(f64, f64); 10] = [(0.877, 0.0), (0.98, 0.15), (1.0, 0.186), (0.997, 0.22), (0.975, 0.67), (0.929, 0.78), (0.866, 0.87), (0.772, 0.92), (0.673, 0.96), (0.0, 1.0)];

/// Points per half section of a generated tyre (the stock tyres carry about
/// 10 per half; a few more keep the shoulder round).
pub const SECTION_POINTS: usize = 14;

/// Arc-length fractions along a half section (bead first).
fn arc_fractions(h: &[(f64, f64)]) -> Vec<f64> {
    let mut a = vec![0f64];
    for w in h.windows(2) {
        a.push(a.last().unwrap() + ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt());
    }
    let t = a.last().copied().unwrap_or(1.0).max(1e-12);
    a.iter().map(|x| x / t).collect()
}

/// The point at arc fraction `t` along a half section.
fn at_fraction(h: &[(f64, f64)], fr: &[f64], t: f64) -> (f64, f64) {
    let j = fr.iter().rposition(|&x| x <= t).unwrap_or(0).min(h.len().saturating_sub(2));
    let span = (fr[j + 1] - fr[j]).max(1e-12);
    let f = ((t - fr[j]) / span).clamp(0.0, 1.0);
    (h[j].0 + (h[j + 1].0 - h[j].0) * f, h[j].1 + (h[j + 1].1 - h[j].1) * f)
}

/// A full section (+x bead .. -x bead, symmetric) resampled to `n` points
/// per half, evenly by arc length.
pub fn resample(full: &[(f64, f64)], n: usize) -> Vec<(f64, f64)> {
    let m = full.len() / 2;
    let half = &full[..=m];
    let fr = arc_fractions(half);
    let h: Vec<(f64, f64)> = (0..n).map(|i| at_fraction(half, &fr, i as f64 / (n - 1) as f64)).collect();
    mirror(&h)
}

/// The section a converted tyre is built with: EVO's own tyre shape (the GT3
/// slick's for race cars, the road tyre's otherwise) at the model's size -
/// its bead where the model's sits on its rim (radius and width), as wide
/// as the model's at its widest, the tread at `measured`'s radius
/// (tyres.ini's) - resampled like `resample`, so
/// point i lies as far along the section as point i of the model's own
/// (`bake_tyre` maps the artwork by index).
///
/// RULE: the shape is the game's, the size the mod's. Example: the RSS GTM
/// Mercer's AC1 tyre bulges 26 mm a side past its bead at mid-sidewall (the
/// stock GT3 slick: 5 mm, low down); built to that shape it looked "like
/// balloons" in EVO (bob76 on OverTake, 15 Nov 2026), once the smooth slick
/// tread showed the bulge.
pub fn stock_shaped(measured: &[(f64, f64)], racing: bool, n: usize) -> Vec<(f64, f64)> {
    let m = measured.len() / 2;
    let (bead_x, bead_r) = measured[0];
    let top = measured[..=m].iter().map(|p| p.1).fold(0.0, f64::max);
    let shape: &[(f64, f64)] = if racing { &STOCK_RACING } else { &STOCK_ROAD };
    // as wide as the model's tyre at its widest (sized from the bead alone,
    // the Mercer's tread came out 75 mm narrower than AC1's and the wheels
    // sat back in their arches), never narrower than the stock shape at
    // this bead; the bead itself stays where the model's sits
    let model_half = measured[..=m].iter().map(|p| p.0.abs()).fold(0.0, f64::max);
    let hw = model_half.max(bead_x.abs() / shape[0].0);
    let mut s: Vec<(f64, f64)> = shape.iter().map(|&(x, h)| (x * hw, bead_r + h * (top - bead_r))).collect();
    s[0].0 = bead_x.abs();
    let fr = arc_fractions(&s);
    let h: Vec<(f64, f64)> = (0..n).map(|i| at_fraction(&s, &fr, i as f64 / (n - 1) as f64)).collect();
    mirror(&h)
}

/// The convex chain of points ordered counter-clockwise around a point
/// inside them (the profile's centre: bead first, tread centre last): every
/// point that makes a right turn or lies on a straight line is dropped
/// (Graham's scan).
fn convex_chain(p: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut out: Vec<(f64, f64)> = Vec::with_capacity(p.len());
    for &q in p {
        while out.len() >= 2 {
            let (o, a) = (out[out.len() - 2], out[out.len() - 1]);
            if (a.0 - o.0) * (q.1 - o.1) - (a.1 - o.1) * (q.0 - o.0) <= 0.0 {
                out.pop();
            } else {
                break;
            }
        }
        out.push(q);
    }
    out
}

/// Half profile (bead -> centre, +x side) -> full (+x bead .. -x bead).
fn mirror(h: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut out: Vec<(f64, f64)> = h.to_vec();
    for p in h.iter().rev().skip(1) {
        out.push((-p.0, p.1));
    }
    out
}

/// A tyre mesh section from a profile (axial, radius), `seg` segments.
pub fn lathe(prof: &[(f64, f64)], seg: usize) -> Geo {
    let mut arc = vec![0f64];
    for w in prof.windows(2) {
        let d = ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt();
        arc.push(arc.last().unwrap() + d);
    }
    let total = arc.last().copied().unwrap_or(1.0).max(1e-9);
    let np = prof.len();
    let mut g = Geo::default();
    for s in 0..=seg {
        let th = s as f64 / seg as f64 * std::f64::consts::TAU - std::f64::consts::PI;
        let (sn, cs) = th.sin_cos();
        for (j, &(a, r)) in prof.iter().enumerate() {
            // profile normal: perpendicular to the profile tangent, outward
            let (da, dr) = if j + 1 < np { (prof[j + 1].0 - a, prof[j + 1].1 - r) } else { (a - prof[j - 1].0, r - prof[j - 1].1) };
            let (na, nr) = { let l = (da * da + dr * dr).sqrt().max(1e-12); (dr / l, -da / l) };
            // going from +x to -x over the top, (dr, -da) points outward
            let pos = [a as f32, (r * sn) as f32, (r * cs) as f32];
            let nrm = [na as f32, (nr * sn) as f32, (nr * cs) as f32];
            // along u (the circumference); w = -1 like the stock tyre
            let tan = [0.0, cs as f32, -sn as f32, -1.0];
            g.verts.push(Vert { pos, nrm, uv: [s as f32 / seg as f32, (arc[j] / total) as f32], uv1: [0.0; 2], tan, bone: 0, pad: 0 });
        }
    }
    for s in 0..seg {
        for j in 0..np - 1 {
            let a = (s * np + j) as u32;
            let b = ((s + 1) * np + j) as u32;
            // front faces: cross(b - a, c - a) along the normal, as on every
            // stock mesh (checked by the test below)
            g.idx.extend_from_slice(&[a, b, a + 1, b, b + 1, a + 1]);
        }
    }
    g
}

/// Body vertices (not wheel / suspension / cockpit parts) inside the rubber
/// of a tyre of `radius` and half width `hw` at a corner, at rest: between
/// the rim radius and the tread (the validator's measure, src/validate/rigcheck.rs).
fn intrusion(insts: &[Inst], f: &Frame, c: &str, radius: f64, hw: f64) -> usize {
    let rim = f.tyre(c).rim_radius.min(radius - 0.03);
    let mut n = 0;
    for i in insts.iter().filter(|i| i.under == geometry::Under::Body && !i.cockpit) {
        let m = f.evo_m(&i.world);
        for v in 0..i.mesh.vertex_count {
            let p = i.mesh.position(v);
            let q = f.to_wheel(c, frame::xform(&m, [p[0] as f64, p[1] as f64, p[2] as f64]));
            let r = (q[1] * q[1] + q[2] * q[2]).sqrt();
            if q[0].abs() <= hw - 0.004 && r > rim + 0.015 && r < radius - 0.006 {
                n += 1;
            }
        }
    }
    n
}

fn nominal(rim_radius: f64) -> f64 {
    // AC1 RIM_RADIUS is "1 inch more than nominal" (tyres.ini comment)
    ((2.0 * rim_radius / 0.0254) - 1.0).round().clamp(10.0, 24.0)
}

fn round_to(v: f64, s: f64) -> f64 {
    (v / s).round() * s
}

/// Split a corner's geometry groups (kn5 material -> geometry) into the
/// stock batch slots, largest group first: on a rim the main opaque group
/// is EXT_RIM and everything else EXT_RIM_DECAL; on a blur mesh the main
/// see-through group (texture alpha, or AC1 blend / alpha test) is
/// EXT_RIM_BLUR, the main opaque one EXT_RIM_BLUR_STATIC and the next one
/// EXT_RIM_DECAL_BLUR. Further groups merge into the slot of their kind
/// (their geometry keeps its UVs; the slot wears its main group's material).
fn assign(groups: &BTreeMap<usize, Geo>, k: &Kn5, tex: &Tex, skin: Option<&(&str, std::path::PathBuf)>, blur: bool, merged: &mut usize) -> Vec<(RimSlot, usize, Geo)> {
    let mut order: Vec<(&usize, &Geo)> = groups.iter().collect();
    order.sort_by_key(|(mi, g)| (std::cmp::Reverse(g.tris()), **mi));
    let see_through = |mi: usize| -> bool {
        let m = &k.materials[mi];
        let a = m.texture("txDiffuse").and_then(|d| tex.image(d, skin.map(|s| s.1.as_path()))).map(|(img, _, _)| materials::mean(&img)[3]).unwrap_or(1.0);
        m.blend_mode == 1 || m.alpha_tested || a < 0.9
    };
    let mut out: Vec<(RimSlot, usize, Geo)> = Vec::new();
    let mut put = |slot: RimSlot, mi: usize, g: &Geo, out: &mut Vec<(RimSlot, usize, Geo)>| {
        match out.iter_mut().find(|x| x.0 == slot) {
            Some(x) => {
                geometry::merge(&mut x.2, g);
                *merged += 1;
            }
            None => out.push((slot, mi, g.clone())),
        }
    };
    if !blur {
        // the main rim: the largest group that is not a see-through decal
        let main = order.iter().find(|(mi, _)| !see_through(**mi)).or(order.first()).map(|(mi, _)| **mi);
        for (mi, g) in &order {
            let slot = if Some(**mi) == main { RimSlot::Rim } else { RimSlot::Decal };
            put(slot, **mi, g, &mut out);
        }
    } else {
        for (mi, g) in &order {
            let slot = if see_through(**mi) {
                if out.iter().any(|x| x.0 == RimSlot::Blur) && !out.iter().any(|x| x.0 == RimSlot::DecalBlur) { RimSlot::DecalBlur } else { RimSlot::Blur }
            } else if !out.iter().any(|x| x.0 == RimSlot::BlurStatic) {
                RimSlot::BlurStatic
            } else {
                RimSlot::DecalBlur
            };
            put(slot, **mi, g, &mut out);
        }
    }
    out
}

/// Write rims, blur, tyres and their customisation files; point the actor at them.
#[allow(clippy::too_many_arguments)]
pub fn write(files: &mut Files, id: &str, stem: &str, k: &Kn5, insts: &[Inst], parts: &[usize], f: &Frame, h: &materials::Hints, tex: &mut Tex, t: &Templates, default_skin: Option<(&str, std::path::PathBuf)>, game: Option<&crate::validate::Game>, racing: bool) -> Result<Wheels> {
    let base = format!("content\\cars\\{id}");
    let mut notes = Vec::new();
    // (kn5 material, slot) -> material path; the first material of a slot is
    // <BATCH>.material (the rim design's slots name those), later ones
    // <BATCH>_<n>.material - the batch name itself is always exact
    let mut mats: BTreeMap<(usize, RimSlot), String> = BTreeMap::new();
    let mut used_names: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut rim_paths: BTreeMap<String, (String, String)> = BTreeMap::new();
    let mut merged = 0usize;
    // each corner's tyre centre along the axle (a corner without its own
    // tyre mirrors its axle partner's): its rims move by it, so the AC1 tyre
    // centre lands on the node, where EVO draws its symmetric tyre
    let mids: BTreeMap<&str, f64> = {
        let own: BTreeMap<&str, Option<f64>> = CORNERS.iter().map(|c| (*c, tread_mid(&tyre_points(insts, parts, f, c, k, h)))).collect();
        let partner = |c: &str| match c { "LF" => "RF", "RF" => "LF", "LR" => "RR", _ => "LR" };
        // under 5 mm is measuring noise (the PS13's centred tyres read +2 mm)
        CORNERS.iter().map(|c| (*c, own[c].or(own[partner(c)].map(|m| -m)).filter(|m| m.abs() >= 0.005).unwrap_or(0.0))).collect()
    };
    let shifted: Vec<String> = CORNERS.iter().filter(|c| mids[*c] != 0.0).map(|c| format!("{c} {:+.0} mm", mids[*c] * 1000.0)).collect();
    if !shifted.is_empty() {
        notes.push(format!("wheels: the AC1 tyres sit off the wheel nodes ({}): rims moved with them so each tyre is centred, as EVO draws it", shifted.join(", ")));
    }
    for c in CORNERS {
        let lc = c.to_lowercase();
        let rims = corner_geo(insts, parts, f, c, Kind::Rim, k, h, mids[c]);
        if rims.is_empty() {
            continue;
        }
        let blurs = corner_geo(insts, parts, f, c, Kind::RimBlur, k, h, mids[c]);
        // rim mesh: the largest opaque material is EXT_RIM, the rest one
        // EXT_RIM_DECAL (stock rims carry exactly these two)
        let rim_slots = assign(&rims, k, tex, default_skin.as_ref(), false, &mut merged);
        // blur mesh: the see-through disc EXT_RIM_BLUR, the opaque rest
        // EXT_RIM_BLUR_STATIC (+ one EXT_RIM_DECAL_BLUR); without AC1 blur
        // the static rim is the whole rim (AC1 then shows the rim at every
        // speed) and the disc an empty batch
        let blur_slots = if blurs.is_empty() {
            rim_slots.iter().map(|(s, mi, g)| (if *s == RimSlot::Rim { RimSlot::BlurStatic } else { RimSlot::DecalBlur }, *mi, g.clone())).collect()
        } else {
            assign(&blurs, k, tex, default_skin.as_ref(), true, &mut merged)
        };
        let main_mi = rim_slots.first().map(|x| x.1).unwrap_or(0);
        for (file, slots, need) in [(format!("{stem}_rim_{lc}"), rim_slots, &[RimSlot::Rim][..]), (format!("{stem}_rim_blur_{lc}"), blur_slots, &[RimSlot::Blur, RimSlot::BlurStatic][..])] {
            let mut slots = slots;
            for n in need {
                if !slots.iter().any(|x| x.0 == *n) {
                    let ph = geometry::placeholder("", "");
                    slots.push((*n, main_mi, ph.geo));
                }
            }
            slots.sort_by_key(|x| x.0);
            let mut secs = Vec::new();
            for (slot, mi, g) in slots {
                let key = (mi, slot);
                if !mats.contains_key(&key) {
                    let batch = slot.batch();
                    let n = used_names.entry(batch).or_insert(0);
                    *n += 1;
                    let name = if *n == 1 { batch.to_string() } else { format!("{batch}_{n}") };
                    let mm = materials::build_rim(files, tex, t, &k.materials[mi], default_skin.clone(), slot);
                    let path = format!("{base}\\materials\\{name}.material");
                    files.put(&path, mm.encode());
                    mats.insert(key, path);
                }
                secs.push(Section { name: slot.batch().to_string(), material: mats[&key].clone(), geo: g });
            }
            let mut b = Built { bones: vec![], lods: vec![secs], lod_in: vec![0.0], skinned: false };
            geometry::make_lods(&mut b, &geometry::WHEEL_LODS);
            // every LOD keeps every batch (the renderer looks them up by name)
            for lod in b.lods.iter_mut() {
                for s in lod.iter_mut().filter(|s| s.geo.tris() == 0) {
                    s.geo = geometry::placeholder("", "").geo;
                }
            }
            let mp = format!("{base}\\meshes\\{file}.mesh");
            let tmpl = files.get(&format!("{base}\\meshes\\{stem}_rim_lf.mesh")).cloned();
            files.put(&mp, b.encode(tmpl.as_deref()));
            let e = rim_paths.entry(c.to_string()).or_default();
            if file.contains("_rim_blur_") { e.1 = mp } else { e.0 = mp }
        }
    }
    if merged > 0 {
        notes.push(format!("wheels: {merged} extra rim material group(s) merged into a stock batch (stock rims carry one material per batch)"));
    }
    // a corner without its own rim borrows the other side / the other axle's
    let fallback = |c: &str, rp: &BTreeMap<String, (String, String)>| -> (String, String) {
        let order: &[&str] = match c { "LF" => &["LF", "RF", "LR", "RR"], "RF" => &["RF", "LF", "RR", "LR"], "LR" => &["LR", "RR", "LF", "RF"], _ => &["RR", "LR", "RF", "LF"] };
        let rim = order.iter().filter_map(|o| rp.get(*o)).map(|x| x.0.clone()).find(|p| !p.is_empty()).unwrap_or_default();
        let blur = order.iter().filter_map(|o| rp.get(*o)).map(|x| x.1.clone()).find(|p| !p.is_empty()).unwrap_or_default();
        (rim, blur)
    };
    let paths: BTreeMap<&str, (String, String)> = CORNERS.iter().map(|c| (*c, fallback(c, &rim_paths))).collect();
    if paths.values().all(|p| p.0.is_empty()) {
        notes.push("wheels: no rim geometry under WHEEL_xx - the donor rims stay".into());
        return Ok(Wheels { notes, measured_radius: BTreeMap::new(), tyre_radius: [f.tyres[0].radius, f.tyres[1].radius], rim_mats: Vec::new(), tyre_look: None });
    }
    // tyres
    let mut measured = BTreeMap::new();
    let mut tyre_meshes: [String; 2] = Default::default();
    let tyre_mat = "content\\cars\\common_assets\\parts\\tyres\\materials\\EXT_TYRE.material".to_string();
    let tyre_mat = if game.map(|g| g.has(&tyre_mat)).unwrap_or(true) { tyre_mat } else { format!("{base}\\materials\\EXT_TYRE.material") };
    let mut sizes = [(0.0, 0.0, 0.0); 2];
    // the AC1 tyre's artwork, baked once (front axle first): one texture
    // set serves both axles (a compound names one), v is bead to bead
    let mut baked: Option<Image> = None;
    for (ax, (c1, c2)) in [("LF", "RF"), ("LR", "RR")].iter().enumerate() {
        let ty = f.tyres[ax];
        let mut pts = tyre_points(insts, parts, f, c1, k, h);
        let mut src_c = *c1;
        if pts.len() < 50 {
            pts = tyre_points(insts, parts, f, c2, k, h);
            src_c = *c2;
        }
        if !pts.is_empty() {
            let r = pts.iter().map(|p| p.1).fold(0.0, f64::max);
            measured.insert(c1.to_string(), r);
            if (r - ty.radius).abs() / ty.radius > 0.03 {
                notes.push(format!("wheels: the AC1 {c1} tyre is {r:.3} m in radius, tyres.ini says {:.3} m (tyres.ini wins)", ty.radius));
            }
        } else {
            notes.push(format!("wheels: no tyre mesh under WHEEL_{c1} - stock profile"));
        }
        // fitment rule (module doc): the model's width, centre and bead, the
        // tyres.ini radius (the ground contact)
        let mut ty = ty;
        let axle = if ax == 0 { "front" } else { "rear" };
        if let Some((kw, kb)) = model_size(&pts) {
            if (kw - ty.width).abs() >= 0.005 {
                notes.push(format!("wheels: {axle} tyres {:.0} mm wide as the AC1 model's (its rims are made to them); tyres.ini says {:.0} mm", kw * 1000.0, ty.width * 1000.0));
            }
            ty.width = kw;
            ty.rim_radius = kb;
            let mid = mids[*c1];
            let kr = pts.iter().map(|p| p.1).fold(0.0, f64::max);
            let ours = intrusion(insts, f, c1, ty.radius, ty.width / 2.0) + intrusion(insts, f, c2, ty.radius, ty.width / 2.0);
            let theirs = intrusion(insts, f, c1, kr, ty.width / 2.0 + mid.abs()) + intrusion(insts, f, c2, kr, ty.width / 2.0 + mid.abs());
            if ours >= 25 && ours > theirs + 10 {
                notes.push(format!("wheels: {axle} tyres put {ours} body vertices inside the tyre at rest; the AC1 model's own tyre has {theirs}"));
            }
        }
        // the game's tyre shape at the model's size (`stock_shaped`)
        let model_prof = profile(&pts, ty.radius, ty.width, ty.rim_radius);
        let prof = stock_shaped(&model_prof, racing, SECTION_POINTS);
        let model_w = model_prof.iter().map(|p| p.0.abs()).fold(0.0, f64::max) * 2.0;
        let built_w = prof.iter().map(|p| p.0.abs()).fold(0.0, f64::max) * 2.0;
        notes.push(format!("wheels: {axle} tyres in the game's {} tyre shape at the model's size: bead {:.0} mm wide at r {:.3} m, {:.0} mm at the widest (the AC1 model's bulges to {:.0} mm)",
            if racing { "GT3 slick" } else { "road" }, model_prof[0].0.abs() * 2000.0, model_prof[0].1, built_w * 1000.0, model_w * 1000.0));
        ty.width = built_w;
        if baked.is_none() && model_size(&pts).is_some() {
            let kr = pts.iter().map(|p| p.1).fold(0.0, f64::max);
            let prof_model = resample(&profile(&pts, kr, model_w, ty.rim_radius), SECTION_POINTS);
            let geos = corner_geo(insts, parts, f, src_c, Kind::Tyre, k, h, 0.0);
            let mid = tread_mid(&pts).unwrap_or(0.0);
            match bake_tyre(&geos, k, tex, default_skin.as_ref().map(|s| s.1.as_path()), &prof_model, &prof, mid) {
                Some((mut img, cover)) if img.w > 0 => {
                    outer_both_sides(&mut img, src_c.starts_with('L'));
                    notes.push(format!("wheels: AC1 tyre artwork baked from {src_c} ({:.0} % covered)", cover * 100.0));
                    baked = Some(img);
                }
                Some((_, cover)) => notes.push(format!("wheels: the AC1 tyre covers only {:.0} % of the tyre texture - the game's tread stays", cover * 100.0)),
                None => {}
            }
        }
        let g = lathe(&prof, 80);
        // the size label keeps tyres.ini's rim (the bead above is the model's)
        let rim_in = nominal(f.tyres[ax].rim_radius);
        let w_mm = (ty.width * 1000.0).round();
        let ratio = round_to(((ty.radius - rim_in * 0.0254 / 2.0) / ty.width * 100.0).clamp(20.0, 90.0), 5.0);
        sizes[ax] = (w_mm, ratio, rim_in);
        let tag = format!("{}_{}_{}", w_mm as u32, ratio as u32, rim_in as u32);
        let dir = format!("{base}\\parts\\tyres\\meshes\\{tag}");
        let mp = format!("{dir}\\tyre_{tag}{}.mesh", if ax == 1 { "_r" } else { "" });
        let mut b = Built { bones: vec![], lods: vec![vec![Section { name: "EXT_TYRE".into(), material: tyre_mat.clone(), geo: g }]], lod_in: vec![0.0], skinned: false };
        geometry::make_lods(&mut b, &geometry::WHEEL_LODS);
        files.put(&mp, b.encode(None));
        tyre_meshes[ax] = mp;
    }
    // the donor's tyremesh as the template (textures list, shadow texture)
    let donor_tm = files.under(&base).into_iter().find(|p| p.to_lowercase().ends_with(".tyremesh"));
    let tm_tmpl = donor_tm.as_ref().and_then(|p| files.get(p)).and_then(|b| Msg::parse(b).ok()).unwrap_or_default();
    let mut tyremesh = [String::new(), String::new()];
    for ax in 0..2 {
        let ty = f.tyres[ax];
        let (w, r, d) = sizes[ax];
        let mut m = tm_tmpl.clone();
        m.set_f32(2, w as f32);
        m.set_f32(3, r as f32);
        m.set_f32(4, d as f32);
        m.set_f32(5, (ty.width * 1.3) as f32);
        m.set_f32(10, (ty.radius * 2.0) as f32);
        m.set_f32(6, -(ty.radius as f32));
        m.set_str(7, &tyre_meshes[ax]);
        let p = tyre_meshes[ax].replace(".mesh", ".tyremesh");
        files.put(&p, m.encode());
        tyremesh[ax] = p;
    }
    if let Some(d) = donor_tm {
        if !tyremesh.iter().any(|p| p.eq_ignore_ascii_case(&d)) {
            files.remove(&d);
        }
    }
    // rimmesh per axle
    let donor_rm = files.under(&format!("{base}\\skins")).into_iter().find(|p| p.to_lowercase().ends_with(".rimmesh"));
    let rm_tmpl = donor_rm.as_ref().and_then(|p| files.get(p)).and_then(|b| Msg::parse(b).ok()).unwrap_or_default();
    let mut rimmesh = [String::new(), String::new()];
    for (ax, (l, r)) in [("LF", "RF"), ("LR", "RR")].iter().enumerate() {
        let (w, _, d) = sizes[ax];
        let rim_w = round_to(w / 25.4 * 0.85, 0.5);
        let mut m = rm_tmpl.clone();
        m.set_f32(2, d as f32);
        m.set_f32(3, rim_w as f32);
        m.set_str(5, &paths[l].0);
        m.set_str(6, &paths[r].0);
        m.set_str(7, &paths[l].1);
        m.set_str(8, &paths[r].1);
        m.remove(9);
        m.push(9, crate::io::pb::Val::Bytes(tyremesh[ax].as_bytes().to_vec()));
        let p = format!("{base}\\skins\\rim_{stem}_{}{}jx{}.rimmesh", if ax == 0 { "f_" } else { "r_" }, rim_w, d as u32);
        files.put(&p, m.encode());
        rimmesh[ax] = p;
    }
    if let Some(d) = donor_rm {
        files.remove(&d);
    }
    // compatible rim
    if let Some(cr) = files.under(&format!("{base}\\skins")).into_iter().find(|p| p.to_lowercase().ends_with(".compatiblerim")) {
        let mut m = Msg::parse(files.get(&cr).unwrap())?;
        m.set_str(1, &rimmesh[0]);
        m.remove(3);
        m.set_str(4, &rimmesh[1]);
        m.remove(6);
        // keep field order: 1, 3, 4, 6, 7, 10
        let rest: Vec<_> = m.fields.drain(..).collect();
        for fl in rest {
            let n = fl.num;
            m.fields.push(fl);
            if n == 1 {
                m.push(3, crate::io::pb::Val::Bytes(tyremesh[0].as_bytes().to_vec()));
            }
            if n == 4 {
                m.push(6, crate::io::pb::Val::Bytes(tyremesh[1].as_bytes().to_vec()));
            }
        }
        files.put(&cr, m.encode());
    }
    // the actor's base meshes
    let ap = format!("{base}\\{id}.actor");
    if let Some(ab) = files.get(&ap).cloned() {
        let mut a = Msg::parse(&ab)?;
        let car = a.msg_mut(103)?;
        let bm = car.msg_mut(12)?;
        let set = |bm: &mut Msg, field: u32, mesh: &str| -> Result<()> {
            let s = bm.msg_mut(field)?;
            s.set_str(1, mesh);
            Ok(())
        };
        // tyres f2..f5, rims f6..f9, blur f18..f21 (LF, RF, LR, RR)
        for (i, c) in CORNERS.iter().enumerate() {
            set(bm, 2 + i as u32, &tyre_meshes[if c.ends_with('F') { 0 } else { 1 }])?;
            set(bm, 6 + i as u32, &paths[c].0)?;
            set(bm, 18 + i as u32, &paths[c].1)?;
        }
        files.put(&ap, a.encode());
    }
    notes.push(format!("wheels: tyres {}/{}R{} front, {}/{}R{} rear; rims {} meshes", sizes[0].0, sizes[0].1, sizes[0].2, sizes[1].0, sizes[1].1, sizes[1].2,
        rim_paths.values().map(|p| (!p.0.is_empty()) as usize + (!p.1.is_empty()) as usize).sum::<usize>()));
    let rim_mats = mats.into_iter().map(|((mi, slot), p)| (p, mi, slot)).collect();
    let tyre_look = baked.map(|img| {
        let colour = tex.put_own(files, "texture\\tyre", "tyre_ac1_c", &img, crate::io::texture::Fmt::Bc7, true, "tyre (baked)");
        let blur = tex.put_own(files, "texture\\tyre", "tyre_ac1_blur_c", &blur_tyre(&img), crate::io::texture::Fmt::Bc7, true, "tyre (baked)");
        TyreLook { colour, blur }
    });
    Ok(Wheels { notes, measured_radius: measured, tyre_radius: [f.tyres[0].radius, f.tyres[1].radius], rim_mats, tyre_look })
}

/// The radius of a written tyre mesh (LOD0, wheel frame): max over vertices.
pub fn mesh_radius(mesh: &[u8]) -> Option<f64> {
    let top = Msg::parse(mesh).ok()?;
    let lod = top.subs(5).into_iter().next()?;
    let p = lod.bytes(5)?;
    let v: Vec<f32> = p.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect();
    Some(v.chunks_exact(3).map(|c| ((c[1] * c[1] + c[2] * c[2]) as f64).sqrt()).fold(0.0, f64::max))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tyre whose own UVs are the stock layout bakes back to its map.
    #[test]
    fn bake_reproduces_a_stock_mapped_tyre() {
        let prof = profile(&[], 0.33, 0.25, 0.23);
        let g = lathe(&prof, 80);
        // a map with a bright band on the +x sidewall only (v < 0.15)
        let (w, h) = (64u32, 64u32);
        let px: Vec<u8> = (0..w * h).flat_map(|i| { let v = (i / w) as f32 / h as f32; if v < 0.15 { [200, 200, 0, 255] } else { [20, 20, 20, 255] } }).collect();
        let img = Image { w, h, px };
        let (out, cover) = bake_layers(&[(&g, &img, false)], &prof, &prof, 0.0).unwrap();
        assert!(cover > 0.95, "cover {cover}");
        let at = |u: f64, v: f64| { let i = ((v * BAKE_H as f64) as usize * BAKE_W as usize + (u * BAKE_W as f64) as usize) * 4; [out.px[i], out.px[i + 1], out.px[i + 2]] };
        // the band stays on the +x sidewall, yellow over lifted rubber
        let band = at(0.3, 0.05);
        assert!(band[0] > 190 && band[2] < 80, "band {band:?}");
        let rubber = at(0.3, 0.5);
        assert!((rubber[0] as f32 / 255.0 - STOCK_RUBBER).abs() < 0.03, "rubber {rubber:?}");
        let other = at(0.3, 0.95);
        assert!(other[0] < 100, "-x sidewall {other:?}");
    }

    /// The inner sidewall becomes the outer one turned 180 deg about the vertical.
    #[test]
    fn inner_sidewall_is_the_outer_turned() {
        let (w, h) = (8u32, 4u32);
        let mut img = Image { w, h, px: (0..w * h).flat_map(|i| [i as u8, 0, 0, 255]).collect() };
        outer_both_sides(&mut img, true);
        let at = |x: u32, y: u32| img.px[((y * w + x) * 4) as usize];
        // the outer half is untouched
        assert_eq!((at(0, 0), at(5, 1)), (0, 13));
        // (u, v) -> (0.5 - u, 1 - v), texel centres: x -> (w/2 + w - 1 - x) % w
        for y in 2..h {
            for x in 0..w {
                let (sx, sy) = ((w / 2 + w - 1 - x) % w, h - 1 - y);
                assert_eq!(at(x, y), (sy * w + sx) as u8);
            }
        }
    }

    /// The inner liner (facing into the tyre) never reaches the bake.
    #[test]
    fn inward_faces_are_skipped() {
        let prof = profile(&[], 0.33, 0.25, 0.23);
        let mut g = lathe(&prof, 40);
        for v in g.verts.iter_mut() {
            v.nrm = [-v.nrm[0], -v.nrm[1], -v.nrm[2]];
        }
        let img = Image { w: 4, h: 4, px: vec![255; 64] };
        let (out, cover) = bake_layers(&[(&g, &img, false)], &prof, &prof, 0.0).unwrap();
        assert_eq!((out.w, cover), (0, 0.0));
    }

    #[test]
    fn sparse_off_centre_tyre_has_no_groove() {
        // a tyre centred 18 mm off the wheel node: a dense tread, sidewalls
        // with only bead / bulge / shoulder rings, and one inner vertex near
        // the centre plane at mid-sidewall (the AC1 Supra's rear)
        let off = -0.018;
        let mut pts = Vec::new();
        for i in -12..=12 {
            pts.push((off + i as f64 * 0.01, 0.318));
        }
        for s in [-1.0, 1.0] {
            for (a, r) in [(0.11, 0.229), (0.135, 0.255), (0.14, 0.28), (0.13, 0.305)] {
                for _ in 0..4 {
                    pts.push((off + s * a, r));
                }
            }
        }
        pts.push((off + 0.009, 0.274));
        let prof = profile(&pts, 0.318, 0.255, 0.2286);
        let bead = prof.iter().map(|p| p.1).fold(f64::MAX, f64::min);
        for &(a, r) in &prof {
            // nothing between the lower sidewall and the shoulder comes near the centre plane
            if r > bead + 0.01 && r < 0.30 {
                assert!(a.abs() > 0.08, "groove at ({a:.3}, {r:.3}) in {prof:?}");
            }
        }
        // centred on the tyre itself, mirrored evenly
        let (lo, hi) = (prof.iter().map(|p| p.0).fold(f64::MAX, f64::min), prof.iter().map(|p| p.0).fold(f64::MIN, f64::max));
        assert!((lo + hi).abs() < 1e-9 && (hi - 0.1275).abs() < 1e-6, "{lo} .. {hi}");
    }

    #[test]
    fn bead_stays_on_the_rim_when_the_tread_grows() {
        // the S14's front: a model tyre of 0.298 m on a 0.2357 m bead,
        // tyres.ini 0.318 m; scaled about the axle the bead rose to 0.252,
        // past the 0.2435 rim lip
        let mut pts = Vec::new();
        for i in -12..=12 {
            pts.push((0.018 + i as f64 * 0.01, 0.298));
        }
        for s in [-1.0, 1.0] {
            for (a, r) in [(0.10, 0.2357), (0.125, 0.25), (0.127, 0.27), (0.12, 0.29)] {
                for _ in 0..4 {
                    pts.push((0.018 + s * a, r));
                }
            }
        }
        let (w, bead) = model_size(&pts).unwrap();
        assert!((w - 0.254).abs() < 1e-6 && (bead - 0.2357).abs() < 1e-6, "{w} {bead}");
        assert!((tread_mid(&pts).unwrap() - 0.018).abs() < 1e-9);
        let prof = profile(&pts, 0.318, w, bead);
        let lo = prof.iter().map(|p| p.1).fold(f64::MAX, f64::min);
        let hi = prof.iter().map(|p| p.1).fold(0.0, f64::max);
        assert!((lo - 0.2357).abs() < 1e-3 && (hi - 0.318).abs() < 1e-6, "{lo} .. {hi}");
    }

    #[test]
    fn ballooned_tyre_takes_the_stock_shape_at_its_size() {
        // the RSS Mercer's front: bead 318 mm wide at r 0.231, bulging to
        // 368 mm at mid-sidewall, tread at r 0.340
        let half = [(0.159, 0.231), (0.176, 0.25), (0.184, 0.285), (0.178, 0.31), (0.167, 0.33), (0.12, 0.338), (0.0, 0.34)];
        let full = mirror(&half);
        for racing in [true, false] {
            let p = stock_shaped(&full, racing, SECTION_POINTS);
            assert_eq!(p.len(), 2 * SECTION_POINTS - 1, "same points as resample");
            assert_eq!(p.len(), resample(&full, SECTION_POINTS).len());
            // the bead stays on the model's rim; the tread at the model's radius
            assert!((p[0].0 - 0.159).abs() < 1e-9 && (p[0].1 - 0.231).abs() < 1e-9, "{:?}", p[0]);
            let top = p.iter().map(|q| q.1).fold(0.0, f64::max);
            assert!((top - 0.34).abs() < 1e-9);
            // symmetric
            assert!((p[0].0 + p[p.len() - 1].0).abs() < 1e-9);
            // as wide as the model at its widest (it fills the arch as in AC1)
            let widest = p.iter().map(|q| q.0.abs()).fold(0.0, f64::max);
            assert!(widest <= 0.184 + 1e-6 && widest > 0.18, "racing {racing}: {widest}");
            // but no balloon: widest in the lower sidewall, not at mid-height,
            // and the tread stays wide (the stock shoulder)
            let at = p[..SECTION_POINTS].iter().find(|q| (q.0 - widest).abs() < 1e-9).unwrap().1;
            assert!((at - 0.231) / (0.34 - 0.231) < 0.4, "racing {racing}: widest at r {at}");
            let shoulder = p[..SECTION_POINTS].iter().filter(|q| q.1 >= 0.231 + 0.83 * 0.109).map(|q| q.0).fold(0.0, f64::max);
            // (the road tyre's shoulder is rounder than the GT3 slick's)
            assert!(shoulder > if racing { 0.15 } else { 0.14 }, "racing {racing}: shoulder half-width {shoulder}");
        }
    }

    #[test]
    fn tyre_is_sized_and_outward() {
        let prof = profile(&[], 0.3, 0.2, 0.2);
        let g = lathe(&prof, 80);
        let r = g.verts.iter().map(|v| ((v.pos[1] * v.pos[1] + v.pos[2] * v.pos[2]) as f64).sqrt()).fold(0.0, f64::max);
        assert!((r - 0.3).abs() < 1e-4, "{r}");
        let w = g.verts.iter().map(|v| v.pos[0].abs()).fold(0.0, f32::max);
        assert!((w - 0.1).abs() < 1e-4, "{w}");
        // a tread triangle faces away from the axle
        let t = &g.idx[..3];
        let p = |i: u32| g.verts[i as usize].pos;
        let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
        let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let n = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
        let v = g.verts[t[0] as usize].nrm;
        assert!(n[0] * v[0] + n[1] * v[1] + n[2] * v[2] > 0.0);
        // uv law
        assert!(g.verts.iter().all(|v| (0.0..=1.0).contains(&v.uv[1])));
    }
}
