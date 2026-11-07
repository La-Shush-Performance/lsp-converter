//! CarData general block, colliders, body offset, brakes, steering and the
//! electronics (ABS / TC / EDL / ESP).
//!
//! Field meanings checked on stock twins: `general.totalMass`, `fuel`,
//! `maxFuel`, `tankPosition` (from the CoG), `pickupFront/RearHeight` and
//! the box colliders are car.ini / colliders.ini values unchanged on
//! ks_mazda_mx5_na; `bodyMeshOffset.position` is AC1's GRAPHICS_OFFSET
//! (mx5_na: (0, -0.5, -0.08) against AC1 (0, -0.47, -0.075)), i.e. the
//! mesh origin relative to the physics CoG. Brakes: `frontBias` is
//! FRONT_SHARE in percent (supra_mkiv 0.695 -> 69.5) and `totalTorque` is
//! MAX_TORQUE (supra 2800 -> 2800). Controls are car.ini [CONTROLS].

use anyhow::Result;

use super::pbx::{self, at, f};
use super::{ac1_v3, profile, Ctx, G, KG_PER_LITRE};
use crate::io::pb::{Msg, Val};

/// Stock ranges (71 cars) the converter clamps to; outside them a note says so.
const BOX_X: (f64, f64) = (1.5, 2.06);
const BOX_Y: (f64, f64) = (0.55, 1.46);
const BOX_Z: (f64, f64) = (3.0, 5.0);
const FF_MULT: (f64, f64) = (0.7, 5.5);

pub fn clamp_note(c: &mut Ctx, what: &str, v: f64, r: (f64, f64)) -> f64 {
    let o = v.clamp(r.0, r.1);
    if (o - v).abs() > 1e-9 {
        c.note(format!("physics: {what} {v:.4} is outside the stock range {:.4} .. {:.4}; clamped", r.0, r.1));
    }
    o
}

pub fn general(c: &mut Ctx) -> Result<()> {
    let ci = c.ini("car.ini").clone();
    let mass = ci.num("BASIC", "TOTALMASS").filter(|m| *m > 100.0).unwrap_or(1200.0);
    let fuel = ci.num("FUEL", "FUEL").filter(|v| *v > 0.0).unwrap_or(30.0);
    let max_fuel = ci.num("FUEL", "MAX_FUEL").filter(|v| *v > 0.0).unwrap_or(fuel.max(50.0));
    let cg = c.ini("suspensions.ini").num("BASIC", "CG_LOCATION").filter(|v| *v > 0.05 && *v < 0.95).unwrap_or(0.5);
    let wb = c.ini("suspensions.ini").num("BASIC", "WHEELBASE").or(c.frame.wheelbase()).unwrap_or(2.5);
    c.d.mass = mass;
    c.d.fuel = fuel.min(max_fuel);
    c.d.max_fuel = max_fuel;
    c.d.mass_static = mass + c.d.fuel * KG_PER_LITRE;
    c.d.cg_front = cg;
    c.d.wheelbase = wb;
    let mut inertia = ac1_v3(&ci, "BASIC", "INERTIA").unwrap_or([1.8, 1.2, 4.2]);
    // test builds: `ACECONV_BODY_BOX=kunos` scales AC1's box (the car's real
    // size, by AC1's own instructions) by the median of Kunos's EVO twins'
    // box over AC1's (x 1.088, y 0.708, z 0.923: 190E Evo II, Supra, AE86,
    // MX-5 NA, Sport quattro), which shrank the yaw inertia on 4 of 5
    if std::env::var("ACECONV_BODY_BOX").map(|v| v.eq_ignore_ascii_case("kunos")).unwrap_or(false) {
        let k = [1.088, 0.708, 0.923];
        let old = inertia;
        inertia = [inertia[0] * k[0], inertia[1] * k[1], inertia[2] * k[2]];
        c.note(format!("physics: TEST BUILD: body box {:.3} / {:.3} / {:.3} -> {:.3} / {:.3} / {:.3} (Kunos twins' median scale)", old[0], old[1], old[2], inertia[0], inertia[1], inertia[2]));
    }
    let bx = [clamp_note(c, "body box x (car.ini INERTIA)", inertia[0], BOX_X), clamp_note(c, "body box y", inertia[1], BOX_Y), clamp_note(c, "body box z", inertia[2], BOX_Z)];
    let tank = ac1_v3(&ci, "FUELTANK", "POSITION").unwrap_or([0.0, -0.1, -1.0]);
    let cog = c.frame.cog;
    let g = at(&mut c.cd, &[1])?;
    f(g, 1, mass);
    pbx::set_v3(g, 4, tank);
    f(g, 5, c.d.fuel);
    f(g, 6, max_fuel);
    f(g, 8, KG_PER_LITRE);
    pbx::set_v3(g, 9, bx);
    f(g, 10, ci.num("RIDE", "PICKUP_FRONT_HEIGHT").unwrap_or(-0.3));
    f(g, 11, ci.num("RIDE", "PICKUP_REAR_HEIGHT").unwrap_or(-0.3));
    pbx::fo(g, 13, ci.num("RULES", "MIN_HEIGHT").filter(|v| *v > 0.0));
    // the mesh origin (on the ground, between the axles) relative to the CoG
    let bmo = at(&mut c.cd, &[18])?;
    pbx::set_v3(bmo, 1, [-cog[0], -cog[1], -cog[2]]);
    pbx::set_v3(bmo, 2, [0.0, 0.0, 0.0]);
    pbx::set_v3(bmo, 3, [1.0, 1.0, 1.0]);
    // box colliders (from the CoG): colliders.ini, else one floor plate
    let col = c.ini("colliders.ini").clone();
    let mut boxes: Vec<([f64; 3], [f64; 3])> = col.with_prefix("COLLIDER_").filter_map(|(s, _)| Some((ac1_v3(&col, s, "CENTRE")?, ac1_v3(&col, s, "SIZE")?))).collect();
    if boxes.is_empty() {
        let tr = c.frame.track_width(true).unwrap_or(1.5);
        boxes.push(([0.0, 0.15 - cog[1], 0.0], [tr * 0.9, 0.1, wb * 0.95]));
        c.note("physics: no colliders.ini - one floor plate collider from the track and wheelbase");
    }
    c.cd.remove(9);
    for (centre, size) in boxes {
        let mut b = Msg::default();
        b.set(1, pbx::v3(centre));
        b.set(2, pbx::v3(size));
        c.cd.push(9, Val::Msg(b));
    }
    // no tuning-part performance modes (stock road cars carry none)
    c.cd.remove(30);
    Ok(())
}

/// The front share AC1's electronic brake balance (`ctrl_ebb.ini`) moves to:
/// the highest value of its ADD stages (the base share it computes; MULT
/// stages only scale it). The GT3 RS's LOAD_SPREAD_LF lut runs 0.54 (even
/// load, a straight stop) to 0.625 (load moved across: trail braking into a
/// turn, where the lift-off spin happened); Kunos's EVO 992 runs 62 %.
fn ebb_share(c: &Ctx) -> Option<(f64, String)> {
    let e = c.ini("ctrl_ebb.ini");
    let mut best: Option<(f64, String)> = None;
    for (sec, _) in e.with_prefix("CONTROLLER_") {
        if !e.get(sec, "COMBINATOR").map(|m| m.trim().eq_ignore_ascii_case("ADD")).unwrap_or(false) {
            continue;
        }
        let input = e.get(sec, "INPUT").unwrap_or("?").split(';').next().unwrap_or("?").trim().to_string();
        let hi = c.lut(e.get(sec, "LUT").unwrap_or("")).into_iter().map(|(_, y)| y).filter(|y| *y > 0.3 && *y < 0.9).fold(f64::NAN, f64::max);
        if hi.is_finite() && best.as_ref().map(|b| hi > b.0).unwrap_or(true) {
            best = Some((hi, input));
        }
    }
    best
}

/// Brake strength: what the brakes could stop with at full pedal, in g
/// (2 x MAX_TORQUE over the mean tyre radius and the car's weight: AC1's
/// MAX_TORQUE x FRONT_SHARE is each front wheel's torque, measured in EVO on
/// the S14 9 Race), over the tyres' lateral mu at the static load. Stock
/// EVO cars (stockstats, 71): 0.68 .. 1.64 for road and race cars, the two
/// F1 cars 1.66 / 1.93, p90 about 1.25. Above BRAKE_RATIO_MAX the converted
/// brakes come down to BRAKE_RATIO_TO, unless the user keeps the mod's
/// (`--brakes mod`, the app's "keep the mod's own brakes"). The S14 9 Race's
/// 10000 Nm (about 3.0) locked the fronts at 43 % pedal and stopped at
/// 2 g in EVO; the user: "the shush made the brakes too strong" (2 Nov).
const BRAKE_RATIO_MAX: f64 = 1.70;
const BRAKE_RATIO_TO: f64 = 1.25;

fn brake_ratio(torque: f64, radius: f64, mass: f64, mu: f64) -> f64 {
    2.0 * torque / radius / (mass * G) / mu
}

/// The capped MAX_TORQUE, or None when the brakes are within the stock range.
fn brake_cap(torque: f64, radius: f64, mass: f64, mu: f64) -> Option<f64> {
    (brake_ratio(torque, radius, mass, mu) > BRAKE_RATIO_MAX).then(|| (BRAKE_RATIO_TO * mu * mass * G * radius / 2.0 / 50.0).round() * 50.0)
}

/// Run after the tyres (it needs their grip and radius).
pub fn brake_strength(c: &mut Ctx) -> Result<()> {
    let r = (c.d.tyre_radius[0] + c.d.tyre_radius[1]) / 2.0;
    let mu = c.d.mu_y[0] * c.d.cg_front + c.d.mu_y[1] * (1.0 - c.d.cg_front);
    if r <= 0.0 || mu <= 0.0 || c.d.mass_static <= 0.0 {
        return Ok(());
    }
    let t = c.d.brake_torque;
    let Some(capped) = brake_cap(t, r, c.d.mass_static, mu) else { return Ok(()) };
    let ratio = brake_ratio(t, r, c.d.mass_static, mu);
    if c.keep_brakes {
        c.note(format!("physics: brakes kept as the mod's ({t:.0} Nm: {ratio:.2} x the tyres' grip, past every stock car's {BRAKE_RATIO_MAX:.2}; the user chose to keep them)"));
        return Ok(());
    }
    c.d.brake_torque = capped;
    let b = at(&mut c.cd, &[5])?;
    f(b, 1, capped);
    c.note(format!(
        "physics: brakes {t:.0} -> {capped:.0} Nm: AC1's could stop at {ratio:.2} x the tyres' grip, past every stock car's ({BRAKE_RATIO_MAX:.2}); now {BRAKE_RATIO_TO:.2}, the stock p90 (keep the mod's with --brakes mod)"
    ));
    Ok(())
}

pub fn brakes_steering(c: &mut Ctx) -> Result<()> {
    let rules = c.d.profile.rules();
    let bi = c.ini("brakes.ini").clone();
    let torque = bi.num("DATA", "MAX_TORQUE").filter(|v| *v > 0.0).unwrap_or(2000.0);
    let mut share = bi.num("DATA", "FRONT_SHARE").filter(|v| *v > 0.0 && *v < 1.0).unwrap_or(0.65);
    // rule 3: AC1's FRONT_SHARE is only the electronic balance's starting point
    if rules.ebb_bias {
        if let Some((e, input)) = ebb_share(c) {
            let s = e.clamp(share, share + 0.15);
            if s > share + 1e-4 {
                c.note(format!("physics rule 3 (brake bias): front bias {:.1} -> {:.1} % from ctrl_ebb.ini (its {input} stage reaches {:.1} %; EVO has no AC1-style electronic balance, so the static bias takes the value it moves to)", share * 100.0, s * 100.0, e * 100.0));
            }
            share = s;
        }
    }
    c.d.brake_torque = torque;
    c.d.front_bias = share * 100.0;
    let b = at(&mut c.cd, &[5])?;
    f(b, 1, torque);
    f(b, 2, share * 100.0);
    f(b, 3, bi.num("DATA", "HANDBRAKE_TORQUE").unwrap_or(torque * 0.4));
    pbx::b(b, 4, bi.num("DATA", "COCKPIT_ADJUSTABLE").unwrap_or(0.0) > 0.0);
    f(b, 5, bi.num("DATA", "ADJUST_STEP").filter(|v| *v > 0.0).unwrap_or(0.5));
    // no EVO electronic brake balance (AC1's ctrl_ebb.ini is folded into the
    // static bias above): empty controllers, EBB off
    for n in [8u32, 9, 14] {
        b.set(n, Val::Msg(Msg::default()));
    }
    b.remove(16);
    b.remove(10);
    b.remove(11);
    b.remove(15);
    let ci = c.ini("car.ini").clone();
    let ac1_lock = ci.num("CONTROLS", "STEER_LOCK").filter(|v| *v > 30.0).unwrap_or(450.0);
    let ac1_ratio = ci.num("CONTROLS", "STEER_RATIO").filter(|v| v.abs() > 1.0).unwrap_or(14.0);
    let ac1_rod = ci.num("CONTROLS", "LINEAR_STEER_ROD_RATIO").unwrap_or(0.002);
    // rule 4 (road / race): a road-wheel lock above the stock p90 comes down
    // by a slower ratio (the p75 at AC1's lock), then the lock range opens to
    // the stock median at that ratio. The rod ratio follows the ratio, so the
    // wheels turn as far per degree of rack as before. EVO has no rear steer
    // to tame a quick rack.
    let (lock, ratio) = steering(ac1_lock, ac1_ratio, rules.steering);
    if (lock, ratio) != (ac1_lock, ac1_ratio) {
        let why = if c.car.data.contains_key("ctrl_4ws.ini") { "; the AC1 car's ctrl_4ws.ini rear steer, which EVO does not simulate, likely let it run quick" } else { "" };
        c.note(format!(
            "physics rule 4 (steering): road-wheel lock {:.1} deg is above the stock p90 {}; ratio {:.2} -> {:.2} (the p75 {} deg at AC1's {ac1_lock:.0} deg lock), then lock {ac1_lock:.0} -> {lock:.0} deg (stock median, same ratio): {:.1} deg at the wheels; linear steer rod ratio {ac1_rod:.5} -> {:.5}{why}",
            ac1_lock / ac1_ratio.abs(), profile::WHEEL_LOCK_P90, ac1_ratio.abs(), ratio.abs(), profile::WHEEL_LOCK_P75, lock / ratio.abs(), ac1_rod * ac1_ratio / ratio
        ));
    }
    // the user's minimum lock (the Convert page's "Minimum steering lock"):
    // off unless asked for
    let (lock, ratio) = match c.min_lock.and_then(|m| min_lock(lock, ratio, m)) {
        Some((l, r)) => {
            c.note(format!(
                "physics: steering lock {lock:.0} deg is below the chosen minimum: lock -> {l:.0} deg, ratio {:.2} -> {:.2}: {:.1} -> {:.1} deg at the wheels",
                ratio.abs(), r.abs(), lock / ratio.abs(), l / r.abs()
            ));
            (l, r)
        }
        None => (lock, ratio),
    };
    let rod = ac1_rod * ac1_ratio / ratio;
    let (ratio, rod) = match evo_signs(ratio, rod) {
        Some((r, k)) => {
            c.note(format!(
                "physics: steering signs: AC1 steer ratio {ratio:.2} with linear steer rod ratio {rod:.5} -> {r:.2} / {k:.5} (EVO keeps the rod positive and the rack's side in the ratio's sign, as every stock car does; the rack moves the same way)"
            ));
            (r, k)
        }
        None => (ratio, rod),
    };
    c.d.steer_lock = lock;
    c.d.steer_ratio = ratio;
    // the stock range holds: the La Shush Old Man's FFMULT 10 was drive-tested at 5.5, 7, 8.5 and
    // 10 (7 Nov 2026) and kept at the stock top
    let ff = clamp_note(c, "FFMULT", ci.num("CONTROLS", "FFMULT").unwrap_or(2.0), FF_MULT);
    let k = at(&mut c.cd, &[8])?;
    f(k, 1, ff);
    f(k, 2, lock);
    f(k, 3, ratio);
    f(k, 4, rod);
    f(k, 5, ci.num("CONTROLS", "STEER_ASSIST").unwrap_or(1.0));
    Ok(())
}

/// AC1 drives the rack by the product of the steer ratio's and the rod
/// ratio's signs, so a mod may put the rack-behind-the-axle sign on either
/// (LSP Mini Van: 18 / -0.00161). Kunos's AC1 cars and all 35 stock EVO
/// CarData decoded put it on the ratio (AE86, 190E, NSX: negative ratio,
/// positive rod) and EVO reads the ratio's sign as the rack's side: a
/// negative rod with a positive ratio is moved over, the product kept.
/// None when the signs already follow EVO's.
fn evo_signs(ratio: f64, rod: f64) -> Option<(f64, f64)> {
    (rod < 0.0).then(|| (-ratio, -rod))
}

/// Rule 4's (lock, ratio): a road-wheel lock above the stock p90 comes down
/// to the p75 by a slower ratio at AC1's own lock (the GT3 RS: 400 / 10.5 ->
/// 12.58), then the lock range opens to the stock median at that ratio
/// (440: 35.0 deg at the wheels), so the steering keeps the quickness that
/// drove best and gains angle to catch a slide. Driven 30 Oct: 400 / 12.58
/// "felt better" than 445 / 14.0 (a slower ratio at the same 31.8 deg), and
/// the user asked for more range instead. The sign of the ratio is kept.
fn steering(lock: f64, ratio: f64, rule: bool) -> (f64, f64) {
    let wheel = lock / ratio.abs();
    if !rule || wheel <= profile::WHEEL_LOCK_P90 {
        return (lock, ratio);
    }
    ((lock.max(profile::STEER_LOCK_P50)), (lock / profile::WHEEL_LOCK_P75).copysign(ratio))
}

/// The user's minimum steering lock: a lock below `min` is raised to it at
/// the same ratio, so the road wheels turn further, but no further than the
/// stock p90 (or the car's own angle when already past it, a drift car's):
/// past that the ratio slows instead. None when the lock already reaches it.
fn min_lock(lock: f64, ratio: f64, min: f64) -> Option<(f64, f64)> {
    if lock >= min {
        return None;
    }
    let cap = (lock / ratio.abs()).max(profile::WHEEL_LOCK_P90);
    let r = if min / ratio.abs() > cap { (min / cap).copysign(ratio) } else { ratio };
    Some((min, r))
}

/// Levels of an AC1 electronics LUT (`CURVE=abs.lut`: index -> slip ratio),
/// else the single SLIP_RATIO_LIMIT.
fn levels(c: &Ctx, sec: &str, ini: &crate::ac1::ini::Ini) -> Vec<f64> {
    let l: Vec<f64> = ini.get(sec, "CURVE").filter(|v| !v.trim().is_empty()).map(|v| c.lut(v)).unwrap_or_default().into_iter().map(|(_, y)| y).filter(|y| *y > 0.0 && *y < 1.0).collect();
    if !l.is_empty() {
        return l;
    }
    vec![ini.num(sec, "SLIP_RATIO_LIMIT").filter(|v| *v > 0.0 && *v < 1.0).unwrap_or(0.12)]
}

/// Levels sorted by slip-ratio limit, highest (least intervention) first,
/// and whether that changed AC1's order.
fn lightest_first(mut l: Vec<f64>) -> (Vec<f64>, bool) {
    let before = l.clone();
    l.sort_by(|a, b| b.total_cmp(a));
    let flipped = l != before;
    (l, flipped)
}

/// AC1's starting level, 1-based in the lightest-first list: the level
/// nearest the [ABS] / [TRACTION_CONTROL] SLIP_RATIO_LIMIT (on every AC1
/// car checked it equals one LUT entry), else level 1.
fn default_level(levels: &[f64], limit: Option<f64>) -> usize {
    let Some(l) = limit.filter(|_| !levels.is_empty()) else { return 1 };
    let mut best = 0;
    for (k, v) in levels.iter().enumerate() {
        if (v - l).abs() < (levels[best] - l).abs() {
            best = k;
        }
    }
    best + 1
}

/// Where stock multi-level TC starts: a quarter of the way from the
/// lightest level (two levels: level 1).
fn fleet_default(n: usize) -> usize {
    if n <= 1 { 1 } else { ((n - 1) as f64 * 0.25).round() as usize + 1 }
}

/// TC activation slip angles of the stock fleet's multi-level systems
/// (tools/handling_stock.tsv: 992 GT3 RS 12 .. 5, M2 8.5 .. 4, GT2s 7 .. 4):
/// the least intervention lets the rear slide furthest.
/// ABS frequency, Hz: the stock median (49 of 67 stock ABS blocks).
const ABS_HZ: f64 = 40.0;
const TC_ANGLE_MAX: f64 = 12.0;
const TC_ANGLE_MIN: f64 = 5.0;

/// One angle per AC1 TC level: the level with the highest slip-ratio limit
/// (the least intervention) gets TC_ANGLE_MAX, the lowest TC_ANGLE_MIN,
/// the rest by rank in between; a single level sits mid-range.
fn tc_angles(levels: &[f64]) -> Vec<f64> {
    let n = levels.len();
    if n <= 1 {
        return vec![(TC_ANGLE_MAX + TC_ANGLE_MIN) / 2.0; n];
    }
    levels.iter().map(|s| {
        let rank = levels.iter().filter(|o| *o > s).count() as f64;
        let ties = levels.iter().filter(|o| *o == s).count() as f64 - 1.0;
        let r = (rank + ties / 2.0) / (n as f64 - 1.0);
        ((TC_ANGLE_MAX - r * (TC_ANGLE_MAX - TC_ANGLE_MIN)) * 2.0).round() / 2.0
    }).collect()
}

/// The electronics.ini section holding AC1's ABS: [ABS], or [ABS_V2] when
/// that is all the car has (31 of the user's 355 AC1 cars: both La Shush
/// Bullets, 22 dthwsh drift cars, 7 Street Car Pack cars; none has both).
/// Same keys plus CHANNELS, and AC1 runs it: the Sure Bullet's ABS was busy
/// 3.4 % of an AC1 stint, where the converted car had none (4 Nov 2026).
fn abs_section(ini: &crate::ac1::ini::Ini) -> &'static str {
    if !ini.has("ABS") && ini.has("ABS_V2") { "ABS_V2" } else { "ABS" }
}

pub fn electronics(c: &mut Ctx) -> Result<()> {
    let ei = c.ini("electronics.ini").clone();
    let on = |s: &str, k: &str| ei.num(s, k).unwrap_or(0.0) > 0.0;
    let abs = abs_section(&ei);
    c.d.has_abs = on(abs, "PRESENT");
    c.d.abs_on = c.d.has_abs && on(abs, "ACTIVE");
    if abs != "ABS" && c.d.has_abs {
        c.note(format!("physics: AC1 ABS read from [{abs}] (no [ABS] section)"));
    }
    c.d.has_tc = on("TRACTION_CONTROL", "PRESENT");
    c.d.tc_on = c.d.has_tc && on("TRACTION_CONTROL", "ACTIVE");
    // EVO numbers its levels lightest first: slip ratios fall level by
    // level on 40 of the 44 multi-level stock TC systems (992 GT3 RS 0.35 ..
    // 0.12; the exceptions are four two-mode FWD / AWD road cars) and on
    // all 24 multi-level ABS ones. AC1 LUTs list the most intrusive level
    // first, which made EVO's TC 1 the strongest. This is the order, not a
    // handling rule: every profile gets it, and Exact keeps AC1's starting
    // level as the default.
    let (abs_levels, abs_flipped) = lightest_first(levels(c, abs, &ei));
    let (tc_levels, tc_flipped) = lightest_first(levels(c, "TRACTION_CONTROL", &ei));
    if (abs_flipped && c.d.has_abs) || (tc_flipped && c.d.has_tc) {
        let what = [(abs_flipped && c.d.has_abs, "ABS"), (tc_flipped && c.d.has_tc, "TC")].iter().filter(|x| x.0).map(|x| x.1).collect::<Vec<_>>().join(" and ");
        c.note(format!("physics: {what} levels written lightest first, EVO's order (40 of 44 multi-level stock TC systems and all 24 ABS ones); AC1's list started with a stronger level"));
    }
    c.d.abs_levels = if c.d.has_abs { abs_levels.len() } else { 0 };
    c.d.abs_default = default_level(&abs_levels, ei.num(abs, "SLIP_RATIO_LIMIT"));
    let ac1_tc = default_level(&tc_levels, ei.num("TRACTION_CONTROL", "SLIP_RATIO_LIMIT"));
    // rule 6 also starts TC where stock cars do (two levels: the lighter,
    // as 18 of the 19 two-level stock cars; more: a quarter of the way in,
    // as the 992 GT3 RS's 3 of 7 and the M2's 3 of 10)
    c.d.tc_default = if c.d.profile.rules().tc_angles { fleet_default(tc_levels.len()) } else { ac1_tc };
    if c.d.has_tc && tc_levels.len() > 1 {
        c.note(format!(
            "physics: TC starts on level {} of {}{}",
            c.d.tc_default, tc_levels.len(),
            if c.d.tc_default == ac1_tc { " (AC1's starting level)".to_string() } else { format!(" (rule 6: where stock cars start; AC1 started on level {ac1_tc}, slip ratio {:.2})", tc_levels[ac1_tc - 1]) }
        ));
    }
    c.d.tc_levels = if c.d.has_tc { tc_levels.len() } else { 0 };
    let rules = c.d.profile.rules();
    let (mut tc_note, mut edl_note): (Option<String>, Option<String>) = (None, None);
    let e = at(&mut c.cd, &[7])?;
    // ABS (f2): stock cars keep the block as the game's assist even when
    // the real car has none; the setup decides whether it is available
    {
        let a = e.msg_mut(2)?;
        let tmpl = a.subs(1).get(1).cloned().unwrap_or_default();
        a.remove(1);
        let mut off = Msg::default();
        f(&mut off, 1, -1.0);
        f(&mut off, 2, -1.0);
        f(&mut off, 5, 0.7);
        a.push(1, Val::Msg(off));
        for s in &abs_levels {
            let mut m = tmpl.clone();
            f(&mut m, 1, s * 0.75);
            f(&mut m, 2, s * 1.25);
            if m.get(4).is_none() {
                f(&mut m, 4, 0.35);
            }
            if m.get(5).is_none() {
                f(&mut m, 5, 0.8);
            }
            a.push(1, Val::Msg(m));
        }
        a.fields.sort_by_key(|x| x.num);
        // the ABS frequency is the stock median, not AC1's RATE_HZ: stock ABS
        // runs at 15 .. 250 Hz, 49 of 67 cars at 40 (measured on the stock cars),
        // where the AC1 911 GT3 RS gives 330. At 330 the converted car's ABS
        // was busy 88 % of a full-pedal stop and it fell to 1.11 g; at 40 a
        // longer stop held 1.26 g with ABS busy 45 % (Brands Hatch, 1 Nov
        // 2026). `ACECONV_ABS_HZ` (test builds) sets it: a number, or `ac1`
        let ac1_hz = ei.num(abs, "RATE_HZ").filter(|v| *v > 1.0);
        let hz = match std::env::var("ACECONV_ABS_HZ").ok().as_deref() {
            Some(v) if v.eq_ignore_ascii_case("ac1") => ac1_hz.unwrap_or(ABS_HZ),
            Some(v) => v.parse::<f64>().ok().filter(|v| *v > 1.0).unwrap_or(ABS_HZ),
            None => ABS_HZ,
        };
        if hz != ABS_HZ {
            c.notes.push(format!("physics: TEST BUILD: ABS frequency {hz} Hz (AC1 RATE_HZ {ac1_hz:?}, default {ABS_HZ})"));
        }
        f(a, 2, hz);
        pbx::i(a, 3, 4);
    }
    // TC (f1): only when the AC1 car has it (stock road cars without TC carry no block)
    if c.d.has_tc {
        let t = e.msg_mut(1)?;
        let tmpl = t.subs(7).get(1).cloned().unwrap_or_default();
        t.remove(7);
        let mut off = Msg::default();
        f(&mut off, 1, -1.0);
        f(&mut off, 2, -1.0);
        t.push(7, Val::Msg(off));
        let angles = if rules.tc_angles { Some(tc_angles(&tc_levels)) } else { None };
        for (k, s) in tc_levels.iter().enumerate() {
            let mut m = tmpl.clone();
            f(&mut m, 1, *s);
            f(&mut m, 2, s + 0.05);
            if let Some(a) = &angles {
                f(&mut m, 3, a[k]);
                f(&mut m, 7, a[k]);
            }
            t.push(7, Val::Msg(m));
        }
        if let Some(a) = &angles {
            let list: Vec<String> = a.iter().map(|x| format!("{x:.1}")).collect();
            tc_note = Some(format!("physics rule 6 (TC): activation / reference slip angles {} deg by level (stock range {TC_ANGLE_MAX} .. {TC_ANGLE_MIN} from the least to the most intervention; the donor's {:.0} deg made TC fire through every slide)", list.join(", "), pbx::rf(&tmpl, 7).unwrap_or(4.0)));
        }
        t.fields.sort_by_key(|x| x.num);
        f(t, 2, ei.num("TRACTION_CONTROL", "RATE_HZ").filter(|v| *v > 1.0).unwrap_or(150.0));
        f(t, 3, ei.num("TRACTION_CONTROL", "MIN_SPEED_KMH").unwrap_or(20.0));
    } else {
        e.remove(1);
    }
    // EDL (f3): AC1 [EDL] values; rule 1 writes it inactive (EVO's EDL at
    // AC1's dead zones brakes the inside rear hard enough to spin the car;
    // Kunos's stock cars carry the block inactive)
    let edl = ei.num("EDL", "PRESENT").unwrap_or(0.0) > 0.0;
    if edl {
        let d = e.msg_mut(3)?;
        if rules.edl_off {
            d.remove(1);
            edl_note = Some(format!("physics rule 1 (EDL): AC1 [EDL] (ACTIVE={}) written inactive; EVO's EDL is far more aggressive at AC1's dead zones and stock EVO cars keep it off", ei.num("EDL", "ACTIVE").unwrap_or(0.0)));
        } else {
            pbx::b(d, 1, true);
        }
        let g = |k: &str, dflt: f64| ei.num("EDL", k).unwrap_or(dflt);
        f(d, 2, g("BRAKE_TORQUE_POWER", 200.0));
        f(d, 3, g("BRAKE_TORQUE_COAST", 200.0));
        f(d, 4, g("DEAD_ZONE_COAST", 0.05));
        f(d, 5, g("DEAD_ZONE_POWER", 0.1));
        f(d, 6, g("MAX_SPIN_POWER", 0.2));
        f(d, 7, g("MAX_SPIN_COAST", 0.3));
        f(d, 8, g("MIN_SPEED_KMH", 20.0));
    } else {
        e.remove(3);
    }
    // ESP (f4): an empty block, like stock road cars without stability control
    e.set(4, Val::Msg(Msg::default()));
    for n in [edl_note, tc_note].into_iter().flatten() {
        c.note(n);
    }
    c.note(format!("physics: electronics ABS {}{} TC {}{}", if c.d.has_abs { "yes" } else { "no (assist only)" }, if c.d.abs_on { ", on" } else { "" }, if c.d.has_tc { format!("{} levels", c.d.tc_levels) } else { "no".into() }, if c.d.tc_on { ", on" } else { "" }));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{abs_section, brake_cap, brake_ratio, default_level, evo_signs, fleet_default, lightest_first, min_lock, profile, steering, tc_angles};

    #[test]
    fn brake_strength_rule() {
        // S14 9 Race: 10000 Nm, r 0.32, 1212 kg, mu 1.78 -> about 3.0, capped
        // to 1.25 x grip (about 4200 Nm)
        assert!((brake_ratio(10000.0, 0.32, 1212.0, 1.78) - 2.95).abs() < 0.05);
        let t = brake_cap(10000.0, 0.32, 1212.0, 1.78).unwrap();
        assert!((t - 4250.0).abs() <= 50.0 && (brake_ratio(t, 0.32, 1212.0, 1.78) - 1.25).abs() < 0.02);
        // VDC R33 (0.95) and a stock-like car stay as they are
        assert_eq!(brake_cap(3000.0, 0.32, 1301.0, 1.55), None);
        assert_eq!(brake_cap(2800.0, 0.33, 1585.0, 1.25), None);
    }

    #[test]
    fn steering_signs_follow_evo() {
        // LSP Mini Van: rack behind the axle, sign on the rod
        assert_eq!(evo_signs(18.0, -0.00161), Some((-18.0, 0.00161)));
        // Kunos's convention is already EVO's
        assert_eq!(evo_signs(-14.0, 0.002), None);
        assert_eq!(evo_signs(17.0, 0.0027), None);
    }

    #[test]
    fn min_lock_rule() {
        // already long enough: untouched
        assert_eq!(min_lock(450.0, 14.0, 360.0), None);
        // the Shaggn: 220 / -22.5 (9.8 deg) -> 360 at the same ratio (16 deg)
        assert_eq!(min_lock(220.0, -22.5, 360.0), Some((360.0, -22.5)));
        // a quick rack: 180 / 6 (30 deg) at 360 would be 60: the ratio slows to the p90
        let (l, r) = min_lock(180.0, 6.0, 360.0).unwrap();
        assert_eq!(l, 360.0);
        assert!((l / r - profile::WHEEL_LOCK_P90).abs() < 1e-9);
        // a drift car already past the p90 keeps its own angle
        let (l, r) = min_lock(200.0, 4.0, 400.0).unwrap();
        assert!((l / r - 50.0).abs() < 1e-9);
    }

    #[test]
    fn steering_rule() {
        // AC1 GT3 RS / La Shush 400 / 10.5 (38.1 deg): ratio for the p75 31.8
        // deg at 400 (12.58), then 440 deg of lock at that ratio (35.0 deg)
        let (l, r) = steering(400.0, 10.5, true);
        assert_eq!(l, 440.0);
        assert!((400.0 / r - 31.8).abs() < 1e-9 && (l / r - 35.0).abs() < 0.05);
        // a lock already past the median keeps it (ratio still from the p75)
        let (l, r) = steering(600.0, 12.0, true);
        assert_eq!(l, 600.0);
        assert!((l / r - 31.8).abs() < 1e-9);
        // in range, rule off (drift / exact), negative ratios keep their sign
        assert_eq!(steering(450.0, 14.0, true), (450.0, 14.0));
        assert_eq!(steering(540.0, -16.5, true), (540.0, -16.5));
        assert_eq!(steering(400.0, 10.5, false), (400.0, 10.5));
        let (_, r) = steering(400.0, -10.5, true);
        assert!(r < 0.0);
    }

    #[test]
    fn starting_levels() {
        // AC1 GT3 RS: SLIP_RATIO_LIMIT 0.08 is the strong level, 2nd after sorting
        assert_eq!(default_level(&[0.18, 0.08], Some(0.08)), 2);
        // a mid-list AC1 default (R8 LMS style: 0.10 inside 0.20 .. 0.08)
        assert_eq!(default_level(&[0.20, 0.18, 0.16, 0.14, 0.12, 0.10, 0.09, 0.08], Some(0.10)), 6);
        assert_eq!(default_level(&[0.12], None), 1);
        // stock: 2 levels -> 1, 7 -> 3 (992 GT3 RS), 10 -> 3 (M2), 12 -> 4
        assert_eq!([2, 7, 10, 12].map(fleet_default), [1, 3, 3, 4]);
    }

    #[test]
    fn abs_from_v2_section() {
        use crate::ac1::ini::parse;
        // La Shush Sure Bullet: ABS only in [ABS_V2]
        let v2 = parse("[ABS_V2]\nSLIP_RATIO_LIMIT=0.32\nPRESENT=1\nACTIVE=1\nRATE_HZ=200\nCHANNELS=1\n[TRACTION_CONTROL]\nPRESENT=1\n");
        assert_eq!(abs_section(&v2), "ABS_V2");
        assert_eq!(v2.num(abs_section(&v2), "SLIP_RATIO_LIMIT"), Some(0.32));
        // [ABS] wins when present, and stays the section for cars with neither
        assert_eq!(abs_section(&parse("[ABS]\nPRESENT=1\n[ABS_V2]\nPRESENT=1\n")), "ABS");
        assert_eq!(abs_section(&parse("[TRACTION_CONTROL]\nPRESENT=1\n")), "ABS");
    }

    #[test]
    fn levels_lightest_first() {
        // AC1 GT3 RS / La Shush tc.lut: 0.08 then 0.18
        assert_eq!(lightest_first(vec![0.08, 0.18]), (vec![0.18, 0.08], true));
        assert_eq!(lightest_first(vec![0.2, 0.1]), (vec![0.2, 0.1], false));
        assert_eq!(lightest_first(vec![0.12]), (vec![0.12], false));
    }

    #[test]
    fn tc_angles_by_level() {
        // least intervention (highest slip-ratio limit) gets the widest angle
        assert_eq!(tc_angles(&[0.2, 0.15, 0.1]), vec![12.0, 8.5, 5.0]);
        assert_eq!(tc_angles(&[0.1, 0.2]), vec![5.0, 12.0]);
        assert_eq!(tc_angles(&[0.12]), vec![8.5]);
        assert!(tc_angles(&[]).is_empty());
    }
}
