//! Tyres: one EVO compound (`TyreCompound_Mod_<n>`) per AC1 compound,
//! front and rear (one file for both when AC1's axles are identical).
//!
//! EVO's tyres are generated per class (eco, road, supercar, hypercar,
//! racing slicks, F1): inside a class most parameters are shared and the
//! grip (`Dy0` / `Dx0` at `Fz0`, load exponents `lsExpY` / `lsExpX`) grows
//! with the size. The load law is AC1's: mu(Fz) = D x (Fz / Fz0)^(ls - 1).
//!
//! Calibration (OURS, docs/FORMATS.md, "Car physics"): on 26 axles of the calibration pairs
//! the stock twin's lateral mu at 3 kN is 0.978 x AC1's (median; most
//! within +-5 %; the outliers are twins Kunos moved to another class, ae86
//! street -> road +16 %, 190E slicks -> road -13 %). So a converted
//! compound keeps the template's model and gets
//!   mu_evo(Fz_static) = 0.98 x mu_ac1(Fz_static)
//! on both axes, Dy0 / Dx0 solved for at the car's own static wheel load.
//! The class is chosen by that target mu and the AC1 name (slicks); the
//! template is the class's stock tyre closest in size; width, radius, rim
//! and the init data come from tyres.ini. Pressures, thermal and wear
//! models stay the template's (they belong to EVO's thermal model, which
//! AC1's numbers do not describe). On the racing-slick model the grip under
//! braking (`brakeDXMod`) is AC1's BRAKE_DX_MOD, not the class's -0.05
//! (`BrakeDx`).
//!
//! Rule 2 (road / race profiles, profile.rs; confirmed by drive: Kunos's
//! uncalibrated 992 tyres cured most of the converted GT3 RS's spin where
//! the per-axle solve did not): solving each axle to AC1's mu throws away
//! the template's own front / rear balance. So one factor per axis scales
//! both axles, chosen so the load-weighted mean mu still is 0.98 x AC1's,
//! and templates stock cars use (full-field ones) win over a closer size
//! that is slim or unused. Drift and exact-AC1 keep the per-axle solve.

use anyhow::Result;
use std::collections::HashSet;

use super::pbx::{self, f};
use super::{Ctx, G};
use crate::ac1::ini::Ini;
use crate::io::pb::{Msg, Val};

/// EVO lateral mu over AC1 lateral mu, both at the static load (OURS, from the pairs).
pub const GRIP_K: f64 = 0.98;

#[derive(Clone, Debug)]
struct Ax {
    name: String,
    short: String,
    width: f64,
    radius: f64,
    rim: f64,
    mu_y: f64,
    mu_x: f64,
    /// FRICTION_LIMIT_ANGLE, AC1's peak slip angle (degrees), if given
    peak: Option<f64>,
    /// FALLOFF_LEVEL / FALLOFF_SPEED: AC1's grip past the peak
    falloff: Option<(f64, f64)>,
    /// BRAKE_DX_MOD: AC1's extra longitudinal grip under braking
    brake_dx: Option<f64>,
    /// ANGULAR_INERTIA, kg m^2
    inertia: Option<f64>,
}

fn read_ax(t: &Ini, s: &str, fz: f64) -> Option<Ax> {
    if !t.has(s) {
        return None;
    }
    let g = |k: &str| t.num(s, k);
    let radius = g("RADIUS").filter(|v| *v > 0.1 && *v < 1.0)?;
    let width = g("WIDTH").filter(|v| *v > 0.05 && *v < 1.0).unwrap_or(0.2);
    let rim = g("RIM_RADIUS").filter(|v| *v > 0.05 && *v < radius).unwrap_or(radius - 0.1);
    let fz0 = g("FZ0").filter(|v| *v > 100.0).unwrap_or(fz);
    let mu = |dref: Option<f64>, d0: Option<f64>, d1: Option<f64>, ls: Option<f64>| -> f64 {
        match dref {
            Some(d) => d * (fz / fz0).powf(ls.unwrap_or(0.8) - 1.0),
            // pre-v10 tyres: D = DY0 + DY1 x Fz (kN)
            None => d0.unwrap_or(1.2) + d1.unwrap_or(0.0) * fz / 1000.0,
        }
    };
    let mu_y = mu(g("DY_REF"), g("DY0"), g("DY1"), g("LS_EXPY"));
    let mu_x = mu(g("DX_REF"), g("DX0"), g("DX1"), g("LS_EXPX"));
    Some(Ax {
        name: t.get(s, "NAME").unwrap_or("Tyre").to_string(),
        short: t.get(s, "SHORT_NAME").unwrap_or("T").to_string(),
        width,
        radius,
        rim,
        mu_y,
        mu_x,
        peak: g("FRICTION_LIMIT_ANGLE").filter(|v| *v > 1.0 && *v < 30.0),
        falloff: g("FALLOFF_LEVEL").filter(|v| *v > 0.3 && *v <= 1.0).zip(g("FALLOFF_SPEED").filter(|v| *v > 0.0)),
        brake_dx: g("BRAKE_DX_MOD").filter(|v| v.abs() < 0.5),
        inertia: g("ANGULAR_INERTIA").filter(|v| *v > 0.1 && *v < 20.0),
    })
}

/// AC1 peak slip angle (degrees) from which a compound is too wide-peaked
/// for EVO's racing-slick model. That class is a different tyre model from
/// every other: peak 4.9 .. 5.4 deg and use_slip_falloff (to 0.75 at slope
/// 5), where eco .. hypercar peak at 7.6 .. 8.9 deg with no extra falloff
/// (all 337 stock templates). Kunos's AC1 modern slicks peak at 6.0 .. 6.5
/// (GT3s, 911 Cup 2017) and the MX-5 Cup's 7.55 went to racing slicks in EVO;
/// the AC1 190E Evo2's DTM slicks (7.91 deg, falloff 0.85 at speed 2) became
/// `road_245_40_17` (8.08 deg) on Kunos's EVO 190E. The cut lies between.
pub const SLICK_MAX_PEAK: f64 = 7.75;

/// Aligning-torque scale (modelData f20 `mzScale`) of EVO's racing slicks:
/// all 38 stock templates; every other class has 0.12 (F1 0.11 .. 0.12). An
/// AC1 slick on another model keeps it. The Neonstride's first conversion, on
/// racing slicks, had AC1's steering tension and the road model lost it; on
/// the road model with 0.30 it came back, and the car drove as before (2 Nov:
/// 3.8 slides a minute against 5.0, laps alike).
const SLICK_MZ: f64 = 0.30;

/// Carcass stiffness (TyresData f11 `lateralFlexK` / f13 `longitudinalFlexK`)
/// of EVO's racing slicks: all 38 stock templates. It is a class constant
/// (road 30000 / 18000, supercar 38000 / 20000, hypercar 40000 / 22000), and
/// an AC1 slick on another model keeps it too. With the road model and
/// SLICK_MZ the Neonstride was still lighter than AC1 and gave less weight
/// under braking; with this it felt better (3 Nov) and needed less steering
/// at 0.8 .. 1.5 g (30 / 33 deg per degree of path against 33 / 41, AC1
/// 29 / 27).
const SLICK_FLEX: (f64, f64) = (60000.0, 30000.0);

/// Whether a compound takes EVO's racing-slick model: AC1 calls it a slick
/// (or its grip is slick grip) and its slip curve peaks like one.
fn slick_model(name: &str, mu: f64, peak: Option<f64>) -> bool {
    (named_slick(name) || mu >= 1.45) && peak.map(|p| p < SLICK_MAX_PEAK).unwrap_or(true)
}

/// AC1 names the compound a slick (not a semi-slick).
fn named_slick(name: &str) -> bool {
    let n = name.to_lowercase();
    n.contains("slick") && !n.contains("semi")
}

/// The game's brand-neutral slick tread (stock MX-5 ND Cup's): the look of
/// every AC1 slick, whatever grip model it is on. The tread a car shows is its
/// tyre textures (the actor's f18 for the default compound, each compound's
/// entry in the `.compatibletyres` list), not its physics class, and the
/// donor gives every compound a road tread. Found by bob76 on OverTake
/// (13 Nov 2026): a converted Ferrari 488 Challenge Evo and 488 GT3 showed
/// treaded road tyres on their slicks. Only the look changes: the compound
/// files, and so the grip, stay as they were.
const SLICK_TREAD: &str = "content\\cars\\common_assets\\parts\\tyres\\textures\\racing_generic_1\\tyre_racing_generic_1.tyretextures";

/// A compound that shows a slick tread: AC1 names it a slick, or it is on a
/// slick model (racing slicks, F1) and AC1 does not call it a semi-slick. A
/// wet or intermediate tyre keeps its tread (the La Shush Momentum Ice's
/// "Endurance WET").
fn slick_look(name: &str, class: &str) -> bool {
    let n = name.to_lowercase();
    if ["wet", "rain", "inter"].iter().any(|w| n.contains(w)) {
        return false;
    }
    named_slick(name) || (matches!(class, "racing_slicks" | "f1_2004") && !n.contains("semi"))
}

/// The EVO class directory for a compound. A wide-peaked slick takes the
/// road model whatever its grip (scaled to AC1's later), as Kunos's EVO 190E
/// did with the AC1 DTM tyre (`road_245_40_17`): driven 1 Nov on the 190E
/// DTM and the Neonstride, it was calmer in a swerve and easier to hold
/// sideways than the hypercar / supercar model its grip picks.
///
/// Grip of 1.70 or more takes F1 2004 (peak 6.8 deg, the stiff 60000
/// carcass) only when AC1's slip curve peaks under SLICK_MAX_PEAK, as every
/// F1 / LMP / formula tyre does (Kunos's 5.8 .. 7.4 deg, the La Shush ones
/// 6.3 .. 7.6). A wider one takes the model its name and grip pick otherwise
/// (road for a slick, else hypercar), grip still scaled to AC1's. The S14 9
/// Race (VDC drift tyres, DY_REF 1.82, peak 10 / 9.5 deg) on F1 2004 had
/// "insanely" too much grip and braking (user, 2 Nov): its fronts locked
/// and its rear spun 8.2 s a minute; on hypercar 1.6 s, and with the brakes
/// capped its lateral g matched the user's AC1 run (p90 1.56 against 1.57).
/// The same VDC tyres are on the Original (8.81 deg) and Spectre (13.0).
fn class_of(name: &str, mu: f64, peak: Option<f64>) -> &'static str {
    let wide = peak.map(|p| p >= SLICK_MAX_PEAK).unwrap_or(false);
    if mu >= 1.70 && !wide {
        "f1_2004"
    } else if slick_model(name, mu, peak) {
        "racing_slicks"
    } else if named_slick(name) {
        "road"
    } else if mu < 1.12 {
        "eco"
    } else if mu < 1.32 {
        "road"
    } else if mu < 1.40 {
        "supercar"
    } else {
        "hypercar"
    }
}

/// AC1 rims are one inch over nominal (tyres.ini: "use 1 inch more than
/// nominal"); EVO stores the nominal rim (14" = 0.1778).
fn nominal_rim(rim: f64) -> f64 {
    let d = 2.0 * rim / 0.0254;
    let nom = (d - 1.0).round();
    if (d - 1.0 - nom).abs() < 0.3 { nom * 0.0254 / 2.0 } else { rim }
}

struct Template {
    path: String,
    msg: Msg,
}

fn size_of(m: &Msg) -> (f64, f64, f64) {
    let t = m.sub(3).unwrap_or_default();
    (pbx::rf(&t, 1).unwrap_or(0.2), pbx::rf(&t, 2).unwrap_or(0.3), pbx::rf(&t, 6).unwrap_or(0.2))
}

/// Leaf fields of a tyre (nested messages expanded). EVO's templates come
/// in full (119-138 fields) and slim (106-110) variants; a slim one lacks
/// whole groups of the model (the `road_325_30_21` the GT3 RS got lacked 29).
fn leaves(m: &Msg) -> usize {
    m.fields.iter().map(|f| match &f.val {
        Val::Msg(s) => leaves(s),
        Val::Bytes(b) if !b.iter().all(|c| (0x20..0x7f).contains(c)) => Msg::parse(b).map(|s| leaves(&s)).unwrap_or(1),
        _ => 1,
    }).sum()
}

/// Every tyre file a stock car drives on (CarData front / rear compounds
/// and the `.compatibletyres` lists), lower case.
fn stock_used(g: &crate::io::kspkg::Pkg) -> HashSet<String> {
    let mut out = HashSet::new();
    let re = regex::bytes::Regex::new(r"(?i)content\\cars\\common_phsx\\tyres\\[ -~]*?\.tyre").unwrap();
    for e in &g.entries {
        let l = e.path.to_lowercase();
        if !(l.starts_with("content\\cars\\ks_") && (l.ends_with(".compatibletyres") || (l.ends_with(".car") && l.contains("\\data\\")))) {
            continue;
        }
        if let Ok(b) = g.read(e) {
            out.extend(re.find_iter(&b).map(|m| String::from_utf8_lossy(m.as_bytes()).to_lowercase()));
        }
    }
    out
}

/// Size-score penalties: a slim template no stock car uses, and any
/// template no stock car uses (worth a 2.5 cm width miss, so a stock size
/// wins over an unused one a centimetre closer).
const SLIM_UNUSED: f64 = 100.0;
const UNUSED: f64 = 0.25;

/// The stock tyre of `class` closest in size; offline: the donor's.
/// `used` (rule 2, the stock cars' tyres): full-field templates stock cars
/// use win over a slim or unused closer size.
fn template(c: &Ctx, class: &str, a: &Ax, used: Option<&HashSet<String>>) -> Option<Template> {
    let score = |path: &str, m: &Msg, full: usize| {
        let (w, r, rim) = size_of(m);
        let mut s = ((w - a.width) / 0.05).powi(2) + ((r - a.radius) / 0.02).powi(2) + ((rim - nominal_rim(a.rim)) / 0.0254).powi(2);
        if used.map(|u| !u.is_empty() && !u.contains(&path.to_lowercase())).unwrap_or(false) {
            s += if leaves(m) + 10 < full { SLIM_UNUSED } else { UNUSED };
        }
        s
    };
    let mut cands: Vec<(String, Vec<u8>)> = Vec::new();
    if let Some(g) = &c.game {
        let pre = format!("content\\cars\\common_phsx\\tyres\\{class}\\");
        for e in g.entries.iter().filter(|e| e.path.to_lowercase().starts_with(&pre) && e.path.to_lowercase().ends_with(".tyre")) {
            if let Ok(b) = g.read(e) {
                cands.push((e.path.clone(), b));
            }
        }
    }
    if cands.is_empty() {
        // offline: the donor's own (eco / road / supercar 215/35 R18)
        let alt = match class { "eco" => "eco", "road" => "road", _ => "supercar" };
        let pre = format!("{}\\tyres\\{alt}\\", c.base).to_lowercase();
        for (p, b) in c.files.map.iter().filter(|(k, _)| k.to_lowercase().starts_with(&pre) && k.to_lowercase().ends_with(".tyre")) {
            cands.push((p.clone(), b.clone()));
        }
    }
    let parsed: Vec<(String, Msg)> = cands.into_iter().filter_map(|(p, b)| Msg::parse(&b).ok().map(|m| (p, m))).collect();
    let full = parsed.iter().map(|(_, m)| leaves(m)).max().unwrap_or(0);
    let mut best: Option<(f64, Template)> = None;
    for (path, m) in parsed {
        {
            let s = score(&path, &m, full);
            if best.as_ref().map(|(bs, _)| s < *bs).unwrap_or(true) {
                best = Some((s, Template { path, msg: m }));
            }
        }
    }
    best.map(|b| b.1)
}

/// Every `content\...\*.curve` string in a file.
fn curve_refs(b: &[u8]) -> Vec<String> {
    let re = regex::bytes::Regex::new(r"(?i)content\\[ -~]*?\.curve").unwrap();
    re.find_iter(b).map(|m| String::from_utf8_lossy(m.as_bytes()).into_owned()).collect()
}

fn mu_at(m: &Msg, fz: f64, x: bool) -> f64 {
    let md = m.sub(4).unwrap_or_default();
    let (d, ls) = if x { (pbx::rf(&md, 2).unwrap_or(1.2), pbx::rf(&md, 4).unwrap_or(0.85)) } else { (pbx::rf(&md, 1).unwrap_or(1.2), pbx::rf(&md, 3).unwrap_or(0.85)) };
    let fz0 = pbx::rf(&md, 5).unwrap_or(4000.0);
    d * (fz / fz0).powf(ls - 1.0)
}

/// One EVO tyre resized to the AC1 axle, its grip not yet set.
struct Built {
    path: String,
    file: String,
    template: String,
    /// the EVO class directory (`racing_slicks`, `road`, ...)
    class: &'static str,
    m: Msg,
    /// the template's lateral / longitudinal mu at the axle's static load
    mu_y: f64,
    mu_x: f64,
    rate: f64,
    falloff: Option<(f64, f64)>,
    peak: Option<f64>,
    brake_dx: Option<f64>,
}

/// Peak slip angle range (degrees) of every stock template without slip
/// falloff (eco .. hypercar, racing wet, vintage, F1; 300 templates,
/// measured 1 Nov): F1 2025 6.54 .. road 8.86.
const PEAK_RANGE: (f64, f64) = (6.54, 8.86);

/// `ACECONV_TYRE_PEAK=ac1` (test builds): on a template without EVO's slip
/// falloff, the peak slip angle (friction_limit_angle) becomes AC1's
/// FRICTION_LIMIT_ANGLE, clamped into PEAK_RANGE, instead of the template's.
/// The match sweep (1 Nov) found every road-type compound on its template's
/// peak: eleven La Shush cars on the F1 2004 template's 6.81 deg, AC1 6.3 ..
/// 13.0. Whether the model's other slip-curve fields follow is not known.
/// Driven 3 Nov, not a rule: the S14 9 Race "felt much closer" (2 Nov), but
/// its two logs disagree (countersteer 14.5 and 7.4 s a minute, 11.2
/// without); on the Bullet (hypercar 7.67 -> 8.60 deg) it "felt like a bit
/// more grip" and slid less (4.1 slides a minute against 5.1, AC1 8.0); on
/// the Neonstride (8.04 -> 7.91) nothing changed.
fn ac1_peak_test() -> bool {
    std::env::var("ACECONV_TYRE_PEAK").map(|v| v.eq_ignore_ascii_case("ac1")).unwrap_or(false)
}

/// `ACECONV_TYRE_FALLOFF=ac1` (test builds): on a template without EVO's
/// slip falloff (every class but racing slicks), switch it on with AC1's
/// FALLOFF_LEVEL as `falloff_level` and FALLOFF_SPEED as `falloff_slope`
/// (shape 1.5, the racing slicks'), lateral only. The unit of the slope
/// against AC1's speed is not known.
fn ac1_falloff_test() -> bool {
    std::env::var("ACECONV_TYRE_FALLOFF").map(|v| v.eq_ignore_ascii_case("ac1")).unwrap_or(false)
}

/// `ACECONV_TYRE_MZ=<scale>` (test builds): the aligning-torque scale
/// (modelData f20 `mzScale`) on every compound. Racing slicks have 0.30,
/// every other class 0.12; the Neonstride's first conversion, on racing
/// slicks, had the steering tension of AC1 that the road model lost (2 Nov).
fn mz_test() -> Option<f64> {
    std::env::var("ACECONV_TYRE_MZ").ok().and_then(|v| v.trim().parse::<f64>().ok()).filter(|v| *v > 0.0)
}

/// Which compounds take AC1's BRAKE_DX_MOD as modelData f9 `brakeDXMod`.
///
/// Rule: those on the racing-slick model. EVO's value is a class constant
/// (racing slicks and historic formula -0.05, every other class +0.05, all
/// stock tyre files), and Kunos moved its AC1 slicks' 0.04 .. 0.09 to -0.05:
/// the MX-5 Cup 0.04 and Huracán Super Trofeo 0.06 went to -0.05 in EVO, the
/// AC1 GT3s carry 0.09. Driven ABS off at Brands Hatch Indy (3 Nov), Kunos's
/// EVO MX-5 ND Cup locked its fronts at about half the pedal of the AC1 MX-5
/// Cup (median 45 against 80 %), on the same brakes and ABS. On the La Shush
/// Street (Kunos's GT3 tyre, 0.09) AC1's value took front lock-ups from 2.4
/// to 1.1 a minute (AC1 1.4) and "felt better"; the user chose AC1's braking
/// over the EVO class's. Other classes keep the template's +0.05.
///
/// `ACECONV_TYRE_BRAKEDX` (test builds): `template` keeps the template's on
/// every compound (the converter before the rule), `ac1` takes AC1's on
/// every compound.
#[derive(Clone, Copy, PartialEq, Debug)]
enum BrakeDx {
    Slicks,
    Template,
    All,
}

fn brake_dx_mode() -> BrakeDx {
    match std::env::var("ACECONV_TYRE_BRAKEDX").map(|v| v.to_ascii_lowercase()).as_deref() {
        Ok("template") => BrakeDx::Template,
        Ok("ac1") => BrakeDx::All,
        _ => BrakeDx::Slicks,
    }
}

/// The brakeDXMod a compound on `class` takes: AC1's, or None to keep the template's.
fn brake_dx(mode: BrakeDx, class: &str, ac1: Option<f64>) -> Option<f64> {
    match mode {
        BrakeDx::Slicks => ac1.filter(|_| class == "racing_slicks"),
        BrakeDx::Template => None,
        BrakeDx::All => ac1,
    }
}

/// TyresData f5 `angularInertia` is AC1's ANGULAR_INERTIA (rim, tyre and
/// brake disc together in AC1); without one (or outside 0.1 .. 20) the
/// template's, scaled by width x radius^2. Until 9 Nov 2026 the scaled
/// template was the rule: it gave the La Shush Taipan 3.88 / 4.85 against
/// AC1's 1.23 / 1.42 (the Street 2.85 / 3.92 against 1.6 / 1.9; stock EVO
/// tyres 0.80 .. 5.77, median 2.38), and the converted Taipan accelerated as
/// if about 250 kg heavier than AC1's at the same gear and rpm. Driven
/// 9 Nov, Brands Hatch Indy, the same build with AC1's inertia: +7 .. 12 %
/// pull in 3rd, 4th and 6th, top speed 314 km/h against 311 (AC1 316), and
/// lock-ups under braking 2.4 a minute against 0.9, toward AC1's own lap
/// (8.4 a minute). (3 Nov, the Street with ABS off: lock-ups as long, 0.85 s,
/// and more frequent, 2.9 a minute against 0.8 .. 1.3; a released wheel
/// spins back up in 0.1 .. 0.3 s in either game.)
/// `ACECONV_TYRE_INERTIA=template` (test builds) keeps the scaled template.
fn template_inertia_test() -> bool {
    std::env::var("ACECONV_TYRE_INERTIA").map(|v| v.eq_ignore_ascii_case("template")).unwrap_or(false)
}

/// Set the grip (D x k on each axis) and put the file. `slick`: an AC1 slick
/// on a model other than the racing slicks', which keeps their aligning torque
/// and carcass stiffness.
fn finish(c: &mut Ctx, b: &Built, ky: f64, kx: f64, slick: bool) -> Result<()> {
    let mut m = b.m.clone();
    if slick {
        let td = m.msg_mut(3)?;
        f(td, 11, SLICK_FLEX.0);
        f(td, 13, SLICK_FLEX.1);
        td.fields.sort_by_key(|x| x.num);
    }
    {
        let md = m.msg_mut(4)?;
        let dy = pbx::rf(md, 1).unwrap_or(1.2);
        let dx = pbx::rf(md, 2).unwrap_or(1.2);
        f(md, 1, dy * ky);
        f(md, 2, dx * kx);
        if slick {
            f(md, 20, SLICK_MZ);
            md.fields.sort_by_key(|x| x.num);
        }
        let has_falloff = md.fields.iter().any(|x| x.num == 79 && matches!(x.val, Val::Varint(v) if v != 0));
        if let (true, false, Some((level, speed))) = (ac1_falloff_test(), has_falloff, b.falloff) {
            pbx::b(md, 79, true);
            f(md, 80, level);
            f(md, 81, speed);
            f(md, 82, 1.5);
            md.fields.sort_by_key(|x| x.num);
            c.note(format!("physics: TEST BUILD: {} gets EVO slip falloff from AC1: level {level}, slope {speed}, shape 1.5", b.file));
        }
        if let (true, false, Some(p)) = (ac1_peak_test(), has_falloff, b.peak) {
            let was = pbx::rf(md, 6).unwrap_or(0.0);
            let to = p.clamp(PEAK_RANGE.0, PEAK_RANGE.1);
            f(md, 6, to);
            c.note(format!("physics: TEST BUILD: {} peak slip angle {was:.2} -> {to:.2} deg (AC1 {p:.2})", b.file));
        }
        let mode = brake_dx_mode();
        if let Some(v) = brake_dx(mode, b.class, b.brake_dx) {
            let was = pbx::rf(md, 9).unwrap_or(0.0);
            if (v - was).abs() > 1e-6 {
                f(md, 9, v);
                md.fields.sort_by_key(|x| x.num);
                let test = if mode == BrakeDx::Slicks { "" } else { "TEST BUILD: " };
                c.note(format!("physics: {test}{} brake grip (brakeDXMod) {was:.2} -> {v:.2}, AC1's BRAKE_DX_MOD (EVO's racing slicks all carry -0.05)", b.file));
            }
        } else if mode == BrakeDx::Template && b.brake_dx.is_some() && b.class == "racing_slicks" {
            c.note(format!("physics: TEST BUILD: {} keeps the template's brake grip (brakeDXMod), not AC1's", b.file));
        }
        if let Some(mz) = mz_test() {
            let was = pbx::rf(md, 20).unwrap_or(0.0);
            f(md, 20, mz);
            md.fields.sort_by_key(|x| x.num);
            c.note(format!("physics: TEST BUILD: {} aligning-torque scale (mzScale) {was:.2} -> {mz:.2}", b.file));
        }
    }
    c.put(&b.path, m.encode());
    c.note(format!("physics: tyre {} from template {}", b.file, b.template));
    Ok(())
}

/// Build one EVO tyre from its template (size, init data, curves).
fn build(c: &mut Ctx, a: &Ax, fz: f64, file: &str, class: &'static str, used: Option<&HashSet<String>>) -> Result<Built> {
    let t = template(c, class, a, used).ok_or_else(|| anyhow::anyhow!("no tyre template for class {class} (no game content and no donor tyre)"))?;
    let dir = format!("{}\\tyres\\{class}", c.base);
    // the template's curves (wear, thermal performance) move into the car folder
    let tbytes = t.msg.encode();
    let mut pairs: Vec<(String, String)> = Vec::new();
    for r in curve_refs(&tbytes) {
        let name = r.rsplit('\\').next().unwrap_or("x.curve").to_lowercase();
        let dst = format!("{dir}\\{name}");
        if !r.eq_ignore_ascii_case(&dst) {
            if let Some(b) = c.read_any(&r) {
                c.put(&dst, b);
            }
            pairs.push((r, dst));
        }
    }
    let pr: Vec<(&str, &str)> = pairs.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    let mut m = Msg::parse(&crate::donor::rename_content(file, &tbytes, &pr))?;
    let rim = nominal_rim(a.rim);
    m.set_str(1, &a.name);
    m.set_str(2, &a.short.chars().take(3).collect::<String>());
    let (tw, tr, _) = size_of(&t.msg);
    {
        let td = m.msg_mut(3)?;
        let ai = pbx::rf(td, 5).unwrap_or(1.5);
        f(td, 1, a.width);
        f(td, 2, a.radius);
        f(td, 6, rim);
        // AC1's ANGULAR_INERTIA; the template's (scaling with width x
        // radius^2) without one (template_inertia_test)
        let scaled = ai * (a.width / tw) * (a.radius / tr).powi(2);
        match (template_inertia_test(), a.inertia) {
            (false, Some(i)) => f(td, 5, i),
            (true, Some(i)) => {
                f(td, 5, scaled);
                c.note(format!("physics: TEST BUILD: {file} angular inertia {i:.2} (AC1) -> {scaled:.2} (template)"));
            }
            (_, None) => {
                f(td, 5, scaled);
                c.note(format!("physics: {file} has no usable ANGULAR_INERTIA; the template's {scaled:.2}"));
            }
        }
    }
    let mu_y = mu_at(&m, fz, false);
    let mu_x = mu_at(&m, fz, true);
    {
        let init = m.msg_mut(12)?;
        f(init, 1, (a.width * 1000.0).round());
        f(init, 2, ((a.radius - rim) / a.width * 100.0).round().clamp(20.0, 90.0));
        f(init, 3, (rim * 2.0 / 0.0254).round());
    }
    let rate = m.sub(3).and_then(|t| pbx::rf(&t, 3)).unwrap_or(280000.0);
    Ok(Built { path: format!("{dir}\\{file}"), file: file.to_string(), template: t.path, class, m, mu_y, mu_x, rate, falloff: a.falloff, peak: a.peak, brake_dx: a.brake_dx })
}

pub fn write(c: &mut Ctx) -> Result<()> {
    let t = c.ini("tyres.ini").clone();
    let fz = [c.d.mass_static * G * c.d.cg_front / 2.0, c.d.mass_static * G * (1.0 - c.d.cg_front) / 2.0];
    let default = t.num("COMPOUND_DEFAULT", "INDEX").unwrap_or(0.0).max(0.0) as usize;
    let mut compounds: Vec<(Ax, Ax)> = Vec::new();
    for k in 0..16 {
        let (sf, sr) = if k == 0 { ("FRONT".to_string(), "REAR".to_string()) } else { (format!("FRONT_{k}"), format!("REAR_{k}")) };
        let (Some(fa), Some(ra)) = (read_ax(&t, &sf, fz[0]), read_ax(&t, &sr, fz[1])) else {
            if k == 0 {
                anyhow::bail!("tyres.ini has no [FRONT] / [REAR] compound");
            }
            continue;
        };
        compounds.push((fa, ra));
    }
    // the game's mod compounds are TyreCompound_Mod_1..10
    if compounds.len() > 10 {
        c.note(format!("physics: {} AC1 compounds; the first 10 are kept", compounds.len()));
        compounds.truncate(10);
    }
    let default = default.min(compounds.len() - 1);
    let balance = c.d.profile.rules().tyre_balance;
    let used = match (&c.game, balance) {
        (Some(g), true) => Some(stock_used(g)),
        _ => None,
    };
    let w = [c.d.cg_front, 1.0 - c.d.cg_front];
    let mut physic: Vec<(String, String)> = Vec::new();
    let mut slick_looks: Vec<bool> = Vec::new();
    for (k, (fa, ra)) in compounds.iter().enumerate() {
        let same = (fa.width - ra.width).abs() < 1e-4 && (fa.radius - ra.radius).abs() < 1e-4 && (fa.mu_y - ra.mu_y).abs() < 1e-3;
        let slug = |s: &str| s.to_lowercase().chars().map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' }).collect::<String>();
        // one class per compound (both axles), from the weight-averaged target mu
        let target = GRIP_K * (fa.mu_y * w[0] + ra.mu_y * w[1]);
        let is_slick = named_slick(&fa.name);
        let class = class_of(&fa.name, target, fa.peak);
        let slick_off_model = is_slick && class != "racing_slicks" && class != "f1_2004";
        slick_looks.push(slick_look(&fa.name, class));
        if k == default && slick_off_model {
            let sec = if k == 0 { "FRONT".to_string() } else { format!("FRONT_{k}") };
            c.note(format!(
                "physics: compound {k} {:?} is a slick whose AC1 slip curve peaks at {:.2} deg (falloff {} at speed {}), wider than EVO's racing-slick model (about 5 deg, falling to 0.75); it takes the {class} model (about 8 deg, grip scaled to AC1's), as Kunos's EVO 190E did with the same AC1 tyre, with the racing slicks' aligning torque (mzScale {SLICK_MZ:.2}) and carcass stiffness ({:.0} / {:.0}) for their steering feel; the setup keeps AC1's alignment (rule 5)",
                fa.name, fa.peak.unwrap_or(0.0), t.get(&sec, "FALLOFF_LEVEL").unwrap_or("?"), t.get(&sec, "FALLOFF_SPEED").unwrap_or("?"), SLICK_FLEX.0, SLICK_FLEX.1
            ));
        }
        let bf = build(c, fa, fz[0], &format!("{}_{k}_{}_front.tyre", c.stem, slug(&fa.short)), class, used.as_ref())?;
        let shared = same && (fz[0] - fz[1]).abs() / fz[0] < 0.15;
        let br = if shared { None } else { Some(build(c, ra, fz[1], &format!("{}_{k}_{}_rear.tyre", c.stem, slug(&ra.short)), class, used.as_ref())?) };
        let r = br.as_ref().unwrap_or(&bf);
        // the template mu at each axle's own static load (a shared file: the rear's)
        let tmu_y = [bf.mu_y, if shared { mu_at(&bf.m, fz[1], false) } else { r.mu_y }];
        let tmu_x = [bf.mu_x, if shared { mu_at(&bf.m, fz[1], true) } else { r.mu_x }];
        // rule 2 keeps whichever rear / front balance gives the rear more
        // grip: the template's (the GT3 RS, 30 Oct: AC1's 0.949 spun where
        // the template's 0.978 did not) or AC1's (the 190E DTM, 1 Nov: AC1's
        // 1.10 drove closer to AC1 than the template's 1.062)
        let ac1_rear = !shared && ra.mu_y / fa.mu_y > tmu_y[1] / tmu_y[0];
        let (mut ky, mut kx) = if balance && !ac1_rear {
            // rule 2: one factor per axis, the load-weighted mean at 0.98 x AC1
            let ky = GRIP_K * (fa.mu_y * w[0] + ra.mu_y * w[1]) / (tmu_y[0] * w[0] + tmu_y[1] * w[1]);
            let kx = GRIP_K * (fa.mu_x * w[0] + ra.mu_x * w[1]) / (tmu_x[0] * w[0] + tmu_x[1] * w[1]);
            ([ky, ky], [kx, kx])
        } else {
            ([GRIP_K * fa.mu_y / bf.mu_y, GRIP_K * ra.mu_y / r.mu_y], [GRIP_K * fa.mu_x / bf.mu_x, GRIP_K * ra.mu_x / r.mu_x])
        };
        if shared {
            (ky[1], kx[1]) = (ky[0], kx[0]);
        }
        finish(c, &bf, ky[0], kx[0], slick_off_model)?;
        if let Some(br) = &br {
            finish(c, br, ky[1], kx[1], slick_off_model)?;
        }
        let mu_y = [tmu_y[0] * ky[0], tmu_y[1] * ky[1]];
        let mu_x = [tmu_x[0] * kx[0], tmu_x[1] * kx[1]];
        let (pf, pr) = (bf.path.clone(), r.path.clone());
        c.note(format!(
            "physics: compound {k} {:?}: {class}, mu_y {:.3} / {:.3} (AC1 {:.3} / {:.3} at {:.0} / {:.0} N)",
            fa.name, mu_y[0], mu_y[1], fa.mu_y, ra.mu_y, fz[0], fz[1]
        ));
        if balance && k == default && ac1_rear {
            c.note(format!(
                "physics rule 2 (tyres): AC1's rear/front balance {:.3} kept (per-axle solve, lateral x{:.3} / x{:.3}): it gives the rear more grip than the EVO templates' {:.3}",
                ra.mu_y / fa.mu_y, ky[0], ky[1], tmu_y[1] / tmu_y[0]
            ));
        } else if balance && k == default {
            c.note(format!(
                "physics rule 2 (tyres): grip scaled by one factor on both axles (lateral x{:.3}, longitudinal x{:.3}), keeping the EVO templates' rear/front balance {:.3} (a per-axle solve to AC1 would give {:.3}); templates stock cars use preferred",
                ky[0], kx[0], mu_y[1] / mu_y[0], ra.mu_y / fa.mu_y
            ));
        }
        if k == default {
            c.d.tyre_radius = [fa.radius, ra.radius];
            c.d.mu_y = mu_y;
            c.d.mu_x = mu_x;
            c.d.tyre_rate = [bf.rate, r.rate];
            c.d.tyre_class = [class.to_string(), class.to_string()];
            c.d.slick = is_slick || class == "racing_slicks";
        }
        physic.push((pf, pr));
    }
    c.d.compounds = physic.len();
    c.d.default_compound = default;
    // CarData: the default compound's files
    c.cd.remove(10);
    c.cd.remove(11);
    c.cd.push(10, Val::Bytes(physic[default].0.clone().into_bytes()));
    c.cd.push(11, Val::Bytes(physic[default].1.clone().into_bytes()));
    // compatible tyres: Mod_1.. in AC1 order, each with the car's tyre textures
    // (a slick's: the game's generic slick tread, SLICK_TREAD)
    let slick_tread = c.game.as_ref().map(|g| g.entries.iter().any(|e| e.path.eq_ignore_ascii_case(SLICK_TREAD))).unwrap_or(true);
    let looks: Vec<bool> = slick_looks.iter().map(|s| *s && slick_tread).collect();
    if slick_looks.iter().any(|s| *s) {
        c.note(if slick_tread {
            format!("physics: slick tread (the game's generic racing slick) on compound(s) {:?}; grip unchanged", (0..looks.len()).filter(|k| looks[*k]).collect::<Vec<_>>())
        } else {
            "physics: the game's generic slick tread is missing - slicks keep the road tread".to_string()
        });
    }
    let pre = format!("{}\\presets\\", c.base).to_lowercase();
    let ct: Vec<String> = c.files.map.keys().filter(|k| k.to_lowercase().starts_with(&pre) && k.to_lowercase().ends_with(".compatibletyres")).cloned().collect();
    for p in ct {
        let mut m = Msg::parse(c.files.get(&p).unwrap())?;
        let tex = m.subs(3).first().and_then(|g| g.str(2));
        m.remove(2);
        m.remove(3);
        for (k, (pf, pr)) in physic.iter().enumerate() {
            let mut e = Msg::default();
            pbx::i(&mut e, 1, 5001 + k as i64);
            e.set_str(2, pf);
            e.set_str(3, pr);
            m.push(2, Val::Msg(e));
            let mut g = Msg::default();
            pbx::i(&mut g, 1, 5001 + k as i64);
            if looks[k] {
                g.set_str(2, SLICK_TREAD);
            } else if let Some(t) = &tex {
                g.set_str(2, t);
            }
            m.push(3, Val::Msg(g));
        }
        c.files.put(&p, m.encode());
    }
    // the actor's tyre textures (f103 actor data, f18): the default compound's tread
    if looks[default] {
        let actors: Vec<String> = c.files.map.keys().filter(|k| {
            let l = k.to_lowercase();
            l.starts_with(&format!("{}\\", c.base).to_lowercase()) && l.ends_with(".actor") && !l[c.base.len() + 1..].contains('\\')
        }).cloned().collect();
        for p in actors {
            let mut m = Msg::parse(c.files.get(&p).unwrap())?;
            let a = m.msg_mut(103)?;
            if a.str(18).map(|s| s.to_lowercase().ends_with(".tyretextures")).unwrap_or(false) {
                a.set_str(18, SLICK_TREAD);
                c.files.put(&p, m.encode());
            } else {
                c.note("physics: the actor names no tyre textures - its tread stays".to_string());
            }
        }
    }
    // the mechanical preset's default compound
    let mp: Vec<String> = c.files.map.keys().filter(|k| k.to_lowercase().starts_with(&pre) && k.to_lowercase().ends_with(".mechanicalcarpreset")).cloned().collect();
    for p in mp {
        let mut m = Msg::parse(c.files.get(&p).unwrap())?;
        pbx::i(&mut m, 7, 5001 + default as i64);
        c.files.put(&p, m.encode());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{brake_dx, class_of, slick_look, BrakeDx};

    #[test]
    fn ac1_brake_grip_on_racing_slicks_only() {
        // La Shush Street (Kunos's AC1 GT3 tyre, 0.09) on racing slicks: AC1's
        assert_eq!(brake_dx(BrakeDx::Slicks, "racing_slicks", Some(0.09)), Some(0.09));
        // other classes keep the template's +0.05, as does a tyre without the key
        assert_eq!(brake_dx(BrakeDx::Slicks, "road", Some(0.05)), None);
        assert_eq!(brake_dx(BrakeDx::Slicks, "hypercar", Some(0.09)), None);
        assert_eq!(brake_dx(BrakeDx::Slicks, "racing_slicks", None), None);
        // test switches: the converter before the rule, and AC1's everywhere
        assert_eq!(brake_dx(BrakeDx::Template, "racing_slicks", Some(0.09)), None);
        assert_eq!(brake_dx(BrakeDx::All, "road", Some(0.05)), Some(0.05));
    }

    #[test]
    fn slick_tread_follows_the_ac1_name_not_the_grip_model() {
        // a wide-peaked slick on the road model still looks like a slick
        assert!(slick_look("Slicks Soft GrpA80s", "road"));
        assert!(slick_look("Slick SuperSoft", "racing_slicks"));
        // an unnamed race tyre on a slick model
        assert!(slick_look("Endurance GT3", "racing_slicks"));
        assert!(slick_look("Spirelli Gp Super", "f1_2004"));
        // treads stay
        assert!(!slick_look("Endurance WET", "racing_slicks"));
        assert!(!slick_look("VDC Semi Slick 235/285", "racing_slicks"));
        assert!(!slick_look("Street", "road"));
        assert!(!slick_look("Hypercar Trofeo", "hypercar"));
    }

    #[test]
    fn slick_model_follows_the_ac1_peak() {
        // Kunos's AC1 GT3 slicks (6.06 deg) and MX-5 Cup (7.55): racing slicks in EVO
        assert_eq!(class_of("Slick Soft", 1.52, Some(6.06)), "racing_slicks");
        assert_eq!(class_of("Slicks", 1.43, Some(7.55)), "racing_slicks");
        // the 190E Evo2 / 155 V6 DTM slicks (7.91 deg): the road model, as Kunos's EVO 190E
        assert_eq!(class_of("Slicks Medium GrpA80s", 1.37, Some(7.91)), "road");
        assert_eq!(class_of("Slicks Medium DTM90s", 1.47, Some(7.91)), "road");
        // a non-slick keeps the class its grip picks, even past slick grip
        assert_eq!(class_of("Street 90s", 1.42, Some(8.3)), "hypercar");
        assert_eq!(class_of("Semislicks", 1.47, Some(8.0)), "hypercar");
        // no AC1 peak (pre-v10 tyres): the name decides, as before
        assert_eq!(class_of("Slicks", 1.40, None), "racing_slicks");
        assert_eq!(class_of("Street", 1.25, Some(8.4)), "road");
    }

    #[test]
    fn f1_model_only_for_narrow_peaked_grippy_tyres() {
        // the S14 9 Race / Spectre / Original VDC drift tyres (10 / 13.0 / 8.81 deg)
        assert_eq!(class_of("VDC 255/265", 1.81, Some(10.0)), "hypercar");
        assert_eq!(class_of("VDC 235/245", 2.62, Some(13.0)), "hypercar");
        assert_eq!(class_of("VDC 235/245", 1.76, Some(8.81)), "hypercar");
        // a grippy slick peaking wide takes the road model, as other wide slicks
        assert_eq!(class_of("Slicks Soft", 1.80, Some(7.91)), "road");
        // F1 / LMP / formula tyres and the Sure Bullet's "pirelli" (7.6) stay on F1
        assert_eq!(class_of("pirelli", 2.10, Some(7.6)), "f1_2004");
        assert_eq!(class_of("Qualifying GP86", 1.86, Some(7.4)), "f1_2004");
        assert_eq!(class_of("Slick Soft", 1.74, Some(6.29)), "f1_2004");
        // no AC1 peak: grip decides, as before
        assert_eq!(class_of("Street", 1.75, None), "f1_2004");
    }
}
