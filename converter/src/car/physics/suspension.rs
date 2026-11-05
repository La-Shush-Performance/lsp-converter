//! Suspension: geometry and hard points, springs, dampers, anti-roll bars,
//! bump stops, the collar (ride height) and the track / wheelbase / CoG.
//!
//! - Hard points are AC1's suspensions.ini points unchanged (relative to the
//!   wheel centre at the design position, x positive inboard): stock
//!   ks_mazda_mx5_na's double-wishbone points equal AC1 ks_mazda_miata's.
//!   DWB -> `dW_data`, STRUT -> `strut` (STRUT_CAR / STRUT_TYRE + the lower
//!   arm and steering points), AXLE -> `axle` (the [AXLE] J<n>_CAR / J<n>_AXLE
//!   links, ATTACH_REL_POS, TORQUE_REACTION, LEAF_SPRING_LAT_K), ML ->
//!   `multi_link_data` (JOINT<n>_CAR / JOINT<n>_TYRE).
//! - `springRate` is AC1's SPRING_RATE (a wheel rate on both sides: the
//!   setup calls it `wheel_rate`), dampers are DAMP_BUMP / DAMP_REBOUND
//!   (slow) and DAMP_FAST_* (fast) with the fast thresholds, anti-roll bars
//!   are [ARB] FRONT / REAR, base_y is BASEY: all equal on ae86 / supra.
//!   Kunos kept AC1's springs and bars 1:1 on the mx5, ae86, quattro, 190E,
//!   Exige and RX-7, so the units match. Rule 7 (profile.rs) moves bar
//!   stiffness front-ward when the roll balance is far outside the fleet's.
//! - Bump stops (BumpStopData): Force at Reference with exponent Gamma;
//!   stock 350 N at 0.01 m is AC1 mx5's BUMP_STOP_RATE 35000 N/m (ae86 30000
//!   / 50000 -> 300 / 500), so Force = BUMP_STOP_RATE x 0.01 with the stock
//!   Reference 0.01 and Gamma 1.5. Damping (6) is written only when the
//!   template car's stop has it, at the stock median (`BUMP_DAMPING_*`).
//! - Ride height (OURS): with a collar position c the coilover rests at
//!   x = sag - c (compression from the design position), sag the static
//!   wheel load over the wheel rate. Over the stock fleet the body's offset
//!   from its design pose follows sag - c within about +-3 cm. The converted
//!   car rests where AC1's physics settles it, x = sag - ROD_LENGTH (so c =
//!   ROD_LENGTH), which on Kunos's AC1 cars is 1-3.5 cm lower than the drawn
//!   model (the drawn pose left the converted Supra high in its arches).
//!   ROD_LENGTH is clamped into setup.ini's range as AC1 loads it, so this
//!   is the pose AC1 drives (and draws) however far it is from the model;
//!   only without a ROD_LENGTH (or `ACECONV_REST_POSE=drawn`) is the drawn
//!   model's pose used: wheel centre at `wheel_centres.y - cog.y` from
//!   the CoG, minus the tyre's static deflection, x = that - BASEY -
//!   deflection. The setup
//!   additionally carries the ride height at the pickup points as the target
//!   (setup.rs), which is what the game's setup solves collars from.
//! - Travel (OURS): the bump stop at AC1's BUMPSTOP_UP, a position from the
//!   design pose in both games, at least 1.5 cm above the rest x; droop
//!   room at rest = AC1's (BUMPSTOP_DN + x_ac1, x_ac1 = sag - ROD_LENGTH),
//!   at least |x| + 1.5 cm (the game's droop frame is unmeasured: MoTeC
//!   logs no droop-stop channel). Capped at 0.09 / 0.05 m (stock p90 / p50).
//! - Heave springs (`ACECONV_HEAVE=off` leaves them out, for A / B builds):
//!   AC1's [HEAVE_FRONT]
//!   / [HEAVE_REAR] third springs become CarData `heaveSprings` (f2.f17,
//!   front then rear, as on Kunos's EVO F2004, whose AC1 heave dampers the
//!   La Shush Taipan carries). AC1 (acs.exe `HeaveSpring::step`) pushes and
//!   pulls the axle with SPRING_RATE t + PROGRESSIVE_SPRING_RATE t^2, t the
//!   two hubs' mean travel from the design pose plus the two corners' mean
//!   ROD_LENGTH: it overwrites the section's own ROD_LENGTH with that every
//!   step, so the heave collar is the corners'. Its hard stops sit at
//!   BUMPSTOP_UP / BUMPSTOP_DN from the design pose and its packers at
//!   PACKER_RANGE (with the rod); the EVO stops go where the corners' do.
//!   The rest pose counts the heave spring: sag = wheel load / (k + K / 2).
//!   Without it the Taipan rested 14 / 15 mm lower than AC1 with half its
//!   heave stiffness, so its front wing and diffuser ran on the steep parts
//!   of their ground-height tables (aero.rs), which both games read at the
//!   wing's own height over the tyre contact patches. Driven 4 Nov (Brands
//!   Hatch Indy, MoTeC): aero balance p95 61 -> 39 % (median 39 / 39), swings
//!   of 10 points in 0.25 s 2.0 -> 0.2 a second, front travel at speed p95
//!   52 -> 10 mm, spins 5 in 77 s -> 1 in 193 s (a power slide, caught); the
//!   user: "fantastic".

use anyhow::Result;

use super::pbx::{self, at, f, i};
use super::{ac1_v3, profile, Ctx, G};
use crate::ac1::ini::Ini;
use crate::io::pb::{Msg, Val};

/// Travel caps (OURS): the stock p90 of bumpStopUp Range and the p50 of
/// bumpStopDown Range (0.09 / 0.045 m). AC1 BUMPSTOP_DN is often a far hard
/// limit (0.16 m on ks_mazda_miata's rear) that lets the wheel hang into
/// the arch at full droop (evoview anim_tyre_in_body).
const BUMP_MAX: f64 = 0.09;
/// BumpStopData 6 `damping`: the stock median over the stops that carry it,
/// bump 400 (135 of 142 stock stops) / droop 500 (132 of 142), measured by
/// examples/bumpstops.rs (tools/bumpstops_stock.tsv).
const BUMP_DAMPING_UP: f64 = 400.0;
const BUMP_DAMPING_DOWN: f64 = 500.0;
const DROOP_MAX: f64 = 0.05;

const DWB: [(&str, u32); 8] = [("WBCAR_TOP_FRONT", 1), ("WBCAR_TOP_REAR", 2), ("WBCAR_BOTTOM_FRONT", 3), ("WBCAR_BOTTOM_REAR", 4), ("WBTYRE_TOP", 5), ("WBTYRE_BOTTOM", 6), ("WBCAR_STEER", 7), ("WBTYRE_STEER", 8)];
const STRUT: [(&str, u32); 7] = [("STRUT_CAR", 1), ("STRUT_TYRE", 2), ("WBCAR_BOTTOM_FRONT", 3), ("WBCAR_BOTTOM_REAR", 4), ("WBTYRE_BOTTOM", 5), ("WBCAR_STEER", 6), ("WBTYRE_STEER", 7)];

fn points(s: &Ini, sec: &str, keys: &[(&str, u32)]) -> Option<Msg> {
    let mut m = Msg::default();
    for (k, n) in keys {
        m.set(*n, pbx::v3(ac1_v3(s, sec, k)?));
    }
    Some(m)
}

/// The geometry message of one axle; the kind name for notes.
fn geometry(c: &mut Ctx, s: &Ini, sec: &str) -> Msg {
    let kind = s.get(sec, "TYPE").unwrap_or("DWB").trim().to_uppercase();
    let mut g = Msg::default();
    let mut basic = Msg::default();
    let hub = s.num(sec, "HUB_MASS").unwrap_or(40.0);
    f(&mut basic, 3, if kind == "AXLE" { (hub * 0.3).max(20.0) } else { hub });
    f(&mut basic, 4, s.num(sec, "TOE_OUT").unwrap_or(0.0));
    f(&mut basic, 5, s.num(sec, "STATIC_CAMBER").unwrap_or(0.0));
    f(&mut basic, 6, s.num(sec, "RIM_OFFSET").unwrap_or(0.0));
    g.set(1, Val::Msg(basic));
    match kind.as_str() {
        "STRUT" => match points(s, sec, &STRUT) {
            Some(p) => g.set(3, Val::Msg(p)),
            None => c.note(format!("physics: [{sec}] STRUT without all its points")),
        },
        "AXLE" => {
            let mut a = Msg::default();
            let n = s.num("AXLE", "LINK_COUNT").unwrap_or(4.0) as usize;
            f(&mut a, 1, s.num("AXLE", "ATTACH_REL_POS").unwrap_or(0.5));
            let mut links = 0;
            for k in 0..n {
                if let (Some(cs), Some(ax)) = (ac1_v3(s, "AXLE", &format!("J{k}_CAR")), ac1_v3(s, "AXLE", &format!("J{k}_AXLE"))) {
                    a.push(3, pbx::v3(cs));
                    a.push(4, pbx::v3(ax));
                    links += 1;
                }
            }
            i(&mut a, 2, links);
            f(&mut a, 5, hub);
            f(&mut a, 6, s.num("AXLE", "LEAF_SPRING_LAT_K").unwrap_or(0.0));
            f(&mut a, 7, s.num("AXLE", "TORQUE_REACTION").unwrap_or(0.0));
            a.fields.sort_by_key(|x| x.num);
            g.set(4, Val::Msg(a));
        }
        "ML" => {
            let mut m = Msg::default();
            for k in 0..5 {
                if let (Some(cs), Some(ty)) = (ac1_v3(s, sec, &format!("JOINT{k}_CAR")), ac1_v3(s, sec, &format!("JOINT{k}_TYRE"))) {
                    m.push(1, pbx::v3(cs));
                    m.push(2, pbx::v3(ty));
                }
            }
            m.fields.sort_by_key(|x| x.num);
            if m.fields.len() >= 10 {
                g.set(5, Val::Msg(m));
            } else if let Some(p) = points(s, sec, &DWB) {
                c.note(format!("physics: [{sec}] ML joints incomplete; its double-wishbone points are used"));
                g.set(2, Val::Msg(p));
            } else {
                c.note(format!("physics: [{sec}] ML joints incomplete"));
            }
        }
        _ => match points(s, sec, &DWB) {
            Some(mut p) => {
                let a = if sec == "FRONT" { 0 } else { 1 };
                let tr = c.d.ac1_traction.to_uppercase();
                let driven = if a == 0 { tr != "RWD" } else { tr != "FWD" };
                // `ac1` = all the way to AC1's reading; a number 0..1 = that share of it
                let share = std::env::var("ACECONV_ANTISQUAT").ok().and_then(|v| if v.eq_ignore_ascii_case("ac1") { Some(1.0) } else { v.parse::<f64>().ok() }).filter(|x| *x > 0.0).map(|x| x.min(1.0));
                let pts: Option<Vec<[f64; 3]>> = DWB[..6].iter().map(|(k, _)| ac1_v3(s, sec, k)).collect();
                if let (Some(share), true, Some(pts)) = (share, driven, pts) {
                    let r = c.d.tyre_radius[a];
                    match tilt_for_ac1_antisquat([pts[0], pts[1], pts[2], pts[3], pts[4], pts[5]], r * share) {
                        Some((new, before, after)) => {
                            for (k, (_, n)) in DWB[..4].iter().enumerate() {
                                p.set(*n, pbx::v3(new[k]));
                            }
                            c.note(format!(
                                "physics: TEST BUILD: {sec} side-view pivot heights tilted {:.0} % of the way to AC1's anti-squat (force line to the instant centre, slope {:+.3}; AC1 reads it from the contact patch, EVO from the wheel centre, where it read {:+.3}): inboard pivots y {:+.3} / {:+.3} / {:+.3} / {:+.3} -> {:+.3} / {:+.3} / {:+.3} / {:+.3} m",
                                share * 100.0, after, before, pts[0][1], pts[1][1], pts[2][1], pts[3][1], new[0][1], new[1][1], new[2][1], new[3][1]
                            ));
                        }
                        None => c.note(format!("physics: TEST BUILD: {sec} anti-squat left alone (side-view arms near parallel: both games about equal)")),
                    }
                }
                g.set(2, Val::Msg(p))
            }
            None => c.note(format!("physics: [{sec}] {kind} without all its points")),
        },
    }
    g
}

/// Test builds (`ACECONV_ANTISQUAT=ac1`): AC1 takes a driven wheel's force
/// at the contact patch when it works out anti-squat from the double
/// wishbones' side-view instant centre, EVO at the wheel centre (the right
/// reading for a chassis-mounted diff). Measured, rear travel per g on the
/// straight (1 Nov, ACTI / MoTeC), AC1 against EVO: Neonstride 35.5 / 5.1,
/// Street 6.9 / 3.0, Bullet 4.2 / 4.3; from the hard points, contact patch
/// against wheel centre: 42.7 / 3.1, 6.5 / -2.1, 5.4 / 4.9. So the instant
/// centre is raised by the tyre radius, keeping its distance, which gives
/// EVO's wheel-centre line AC1's contact-patch slope: each arm's pivot axis
/// turns in side view about its point under the ball joint, so the front
/// view there is unchanged. Points are [top front, top rear, bottom front,
/// bottom rear, tyre top, tyre bottom] (x, y up, z forward, from the wheel
/// centre); returns the new four inboard points and the slope of the line
/// from the wheel centre to the instant centre before and after. None when the
/// arms are near parallel in side view (instant centre beyond 50 m: the
/// two readings differ by under R / 50 in slope). A partial tilt raises the
/// instant centre by less than R (`r` is the rise): on the Neonstride's short
/// rear arms the full rise moves the hub 50 mm forward and loses 3.5 deg of
/// negative camber at 100 mm of bump, half of it 21 mm and 1.2 deg (rigid-hub
/// solve, 2 Nov).
fn tilt_for_ac1_antisquat(p: [[f64; 3]; 6], r: f64) -> Option<([[f64; 3]; 4], f64, f64)> {
    let line = |a: [f64; 3], b: [f64; 3]| {
        let k = (a[1] - b[1]) / (a[2] - b[2]);
        (a[1] - k * a[2], k)
    };
    if (p[0][2] - p[1][2]).abs() < 1e-4 || (p[2][2] - p[3][2]).abs() < 1e-4 {
        return None;
    }
    let (au, ku) = line(p[0], p[1]);
    let (al, kl) = line(p[2], p[3]);
    if (ku - kl).abs() < 1e-6 {
        return None;
    }
    let zic = (al - au) / (ku - kl);
    if zic.abs() > 50.0 {
        return None;
    }
    let yic = au + ku * zic;
    let target = yic + r;
    let tilt = |a: f64, k: f64, hub: [f64; 3], i: [f64; 3], j: [f64; 3]| {
        let yp = a + k * hub[2];
        let k2 = (target - yp) / (zic - hub[2]);
        let at = |q: [f64; 3]| [q[0], yp + k2 * (q[2] - hub[2]), q[2]];
        (at(i), at(j))
    };
    let (tf, trr) = tilt(au, ku, p[4], p[0], p[1]);
    let (bf, br) = tilt(al, kl, p[5], p[2], p[3]);
    Some(([tf, trr, bf, br], yic / zic, target / zic))
}

pub fn write(c: &mut Ctx) -> Result<()> {
    let s = c.ini("suspensions.ini").clone();
    let ci = c.ini("car.ini").clone();
    let m = c.d.mass_static;
    let share = [c.d.cg_front, 1.0 - c.d.cg_front];
    let cog = c.frame.cog;
    let mut coil: Vec<Msg> = Vec::new();
    let mut geos: Vec<Msg> = Vec::new();
    let mut base_y = [0.0; 2];
    let mut track = [0.0; 2];
    // the rest pose: where AC1's physics settles the body (x_ac1 = sag -
    // ROD_LENGTH, compression from the design pose), when AC1 gives a
    // ROD_LENGTH; else the drawn model's pose. A 40 mm limit used to send
    // larger gaps to the drawn pose, from before the setup-range clamp: it
    // left the Neonstride's rear 56 mm higher than AC1 drives it, where its
    // short rear wishbones change camber and toe fast (1 Nov). AC1
    // cars are drawn higher than they drive: the Supra 35 / 15 mm, GT3 RS
    // 29 / 9, Countach 33 / -3, Quattro 29 / 25, which left the converted
    // cars sitting high in their arches (Kunos's Supra targets 140 / 125
    // mm against the drawn 185 / 185)
    // AC1's rod length per axle: suspensions.ini's, clamped into setup.ini's
    // range the way AC1 loads it
    let setup_ini = c.ini("setup.ini").clone();
    let rods: Vec<(f64, Option<(f64, f64, f64)>)> = (0..2).map(|a| ac1_rod(&s, &setup_ini, a)).collect();
    let heave = [0usize, 1].map(|a| heave_rate(&s, a));
    c.d.heave = heave;
    for (a, (eff, clamped)) in rods.iter().enumerate() {
        if let Some((raw, lo, hi)) = clamped {
            c.note(format!(
                "physics: {} ROD_LENGTH {raw:.3} m is outside the AC1 setup range {lo:.4} .. {hi:.4} m; AC1 runs it at {eff:.4} (as La Shush Street's saved AC1 setup shows)",
                if a == 0 { "FRONT" } else { "REAR" }
            ));
        }
    }
    let poses: Vec<(f64, f64, f64)> = ["FRONT", "REAR"].iter().enumerate().map(|(a, sec)| {
        let k = s.num(sec, "SPRING_RATE").filter(|v| *v > 1000.0).unwrap_or(40000.0);
        let hub = s.num(sec, "HUB_MASS").unwrap_or(40.0);
        let load = m * G * share[a] / 2.0;
        let sag = sag(load, hub, k, heave[a]);
        let by = s.num(sec, "BASEY").unwrap_or(-0.2);
        let cs = if a == 0 { ["LF", "RF"] } else { ["LR", "RR"] };
        let wy: Vec<f64> = cs.iter().filter_map(|k| c.frame.wheel_centres.get(*k)).map(|p| p[1]).collect();
        let w_vis = if wy.is_empty() { by } else { wy.iter().sum::<f64>() / wy.len() as f64 } - cog[1];
        let x_vis = w_vis - by - load / c.d.tyre_rate[a].max(1e5);
        (x_vis, sag - rods[a].0, sag)
    }).collect();
    let forced = std::env::var("ACECONV_REST_POSE").unwrap_or_default().to_lowercase();
    let trusted = s.num("FRONT", "ROD_LENGTH").is_some();
    let ac1_pose = match forced.as_str() {
        "drawn" => false,
        "ac1" => true,
        _ => trusted,
    };
    let test = if forced == "drawn" || forced == "ac1" { format!(" (TEST BUILD: ACECONV_REST_POSE={forced})") } else { String::new() };
    if ac1_pose {
        c.note(format!(
            "physics: rest pose from AC1's physics (sag - ROD_LENGTH): {:+.0} / {:+.0} mm lower than the drawn model, front / rear{test}",
            (poses[0].1 - poses[0].0) * 1000.0, (poses[1].1 - poses[1].0) * 1000.0
        ));
    } else {
        c.note(format!(
            "physics: rest pose from the drawn model: AC1's sag - ROD_LENGTH would move it {:+.0} / {:+.0} mm{}{test}",
            (poses[0].1 - poses[0].0) * 1000.0, (poses[1].1 - poses[1].0) * 1000.0,
            if trusted { String::new() } else { ", AC1 gives no ROD_LENGTH".to_string() }
        ));
    }
    if heave.iter().any(|k| *k > 0.0) {
        c.note(format!(
            "physics: AC1 heave springs {:.0} / {:.0} N/m carried over, on the corners' collars; the rest pose counts them",
            heave[0], heave[1]
        ));
    } else if heave_off() && ["HEAVE_FRONT", "HEAVE_REAR"].iter().any(|sec| s.num(sec, "SPRING_RATE").unwrap_or(0.0) > 0.0) {
        c.note("physics: TEST BUILD: AC1 heave springs left out (ACECONV_HEAVE=off)");
    }
    c.d.rod = [rods[0].0, rods[1].0];
    let mut rests = [(0.0, 0.0, 0.0); 2];
    for (a, sec) in ["FRONT", "REAR"].iter().enumerate() {
        let k = s.num(sec, "SPRING_RATE").filter(|v| *v > 1000.0).unwrap_or(40000.0);
        let hub = s.num(sec, "HUB_MASS").unwrap_or(40.0);
        let load = m * G * share[a] / 2.0;
        let sag = sag(load, hub, k, heave[a]);
        let x_ac1 = sag - rods[a].0;
        let by = s.num(sec, "BASEY").unwrap_or(-0.2);
        base_y[a] = by;
        track[a] = s.num(sec, "TRACK").or(c.frame.track_width(a == 0)).unwrap_or(1.5);
        // where the AC1 model draws the wheel, from the CoG
        let cs = if a == 0 { ["LF", "RF"] } else { ["LR", "RR"] };
        let wy: Vec<f64> = cs.iter().filter_map(|k| c.frame.wheel_centres.get(*k)).map(|p| p[1]).collect();
        let w_vis = if wy.is_empty() { by } else { wy.iter().sum::<f64>() / wy.len() as f64 } - cog[1];
        let tdef = load / c.d.tyre_rate[a].max(1e5);
        let x_vis = w_vis - by - tdef;
        let x = if ac1_pose { poses[a].1 } else { x_vis };
        let collar = sag - x;
        // the bump stop where AC1 has it: BUMPSTOP_UP from the design pose.
        // EVO's range is a position from the design pose too (MoTeC, 2 Nov:
        // the Neonstride spawns at 0 travel, settles at 24 mm, and its stop
        // engages at the 66 / 81 mm written), and so is AC1's (ACTI: the
        // Street's front load steepens at 70 mm, BUMPSTOP_UP 65). Writing
        // BUMPSTOP_UP - x_ac1 put every stop early by AC1's rest compression.
        // Test builds: `ACECONV_BUMP_CAP=ac1` lifts the cap to check-car's
        // 0.30 m. With AC1's anti-squat (ACECONV_ANTISQUAT) the Neonstride's
        // rear runs 60 .. 73 mm deep under power, and AC1 has it past 90 mm 8 %
        // of the time moving and past its 105 mm stop 2 % (ACTI, 2 Nov)
        let ac1_up = s.num(sec, "BUMPSTOP_UP").unwrap_or(0.08);
        let cap = if std::env::var("ACECONV_BUMP_CAP").map(|v| v.eq_ignore_ascii_case("ac1")).unwrap_or(false) { 0.30 } else { BUMP_MAX };
        if cap > BUMP_MAX && ac1_up > BUMP_MAX {
            c.note(format!("physics: TEST BUILD: {sec} bump stop at AC1's {:.0} mm, past the {:.0} mm cap", ac1_up.min(cap) * 1000.0, BUMP_MAX * 1000.0));
        }
        let mut up = ac1_up.clamp(0.01, cap).max(x + 0.015);
        // test builds: `ACECONV_PACKERS=ac1`. Not a rule: AC1's own logs (ACTI,
        // 1 Nov) rest the Neonstride's front at 29.5 mm, where its spring
        // alone puts it, not on packers at 5 mm, and it travels to 84 mm (61
        // at p99) where this build stopped at 44. The packer build drove
        // closer to AC1 (10.3 -> 7.0 slides a minute, AC1 6.2) by stiffening
        // the front, standing in for the rear squat EVO does not reproduce
        // (CONVERSION-TRIAL.md, "anti-squat"). Under this reading
        // AC1's PACKER_RANGE is the travel
        // from full droop (BUMPSTOP_DN below the design pose) to the packers,
        // which then push at BUMP_STOP_RATE, linearly. Where they come before
        // the bump stop, the EVO stop moves to them, linear too. EVO's stop
        // range is a signed position from the design pose, as the rest x is
        // (stock: the AE86 front rests 10 mm into its stop, some ranges are
        // negative), so a car AC1 rests on its packers rests on the stop:
        // the Neonstride's front packers start 5 mm into bump, under AC1's
        // 24 mm rest (17 mm with the packers pushing). The stop's length
        // (f5, stock 5 .. 60 mm, mostly 20 .. 30) is the rubber's travel; it
        // goes solid at its end, so the moved stop runs to the bump stop it
        // replaces (at 20 mm the 1 Nov drive knocked over kerbs: front stop
        // travel stalled at 17 .. 25 mm)
        let mut gamma = 1.5;
        let mut stop_len = 0.02;
        let packers = std::env::var("ACECONV_PACKERS").map(|v| v.eq_ignore_ascii_case("ac1")).unwrap_or(false);
        if let Some(p) = s.num(sec, "PACKER_RANGE").filter(|v| *v > 0.0 && packers) {
            let pos = x + (p - s.num(sec, "BUMPSTOP_DN").unwrap_or(0.08) - x_ac1);
            if pos < up {
                stop_len = (up - pos).max(0.02);
                c.note(format!("physics: TEST BUILD: {sec} AC1 packers at {:.0} mm (rest {:.0} mm, PACKER_RANGE {p:.3}) come before the bump stop {:.0} mm: EVO bump stop there, linear, {:.0} mm long", pos * 1000.0, x * 1000.0, up * 1000.0, stop_len * 1000.0));
                up = pos;
                gamma = 1.0;
            }
        }
        let dn = (s.num(sec, "BUMPSTOP_DN").unwrap_or(0.08) + x_ac1).clamp(0.01, DROOP_MAX).max(x.abs() + 0.015);
        rests[a] = (collar, x, x_ac1);
        c.d.spring[a] = k;
        c.d.sag[a] = sag;
        c.d.rest_x[a] = x;
        c.d.collar[a] = collar;
        c.d.bump_up[a] = up;
        c.d.bump_dn[a] = dn;
        let pickup = ci.num("RIDE", if a == 0 { "PICKUP_FRONT_HEIGHT" } else { "PICKUP_REAR_HEIGHT" }).unwrap_or(-0.3);
        // the drawn model's height at the pickup, lowered by the rest pose's
        // extra compression
        c.d.target_height_mm[a] = ((cog[1] + pickup - (x - x_vis)) * 1000.0).round();
        // CoiloverData
        let mut co = c.cd.sub(2).and_then(|x| x.sub(if a == 0 { 8 } else { 9 })).unwrap_or_default();
        f(&mut co, 1, k);
        pbx::fo(&mut co, 2, s.num(sec, "PROGRESSIVE_SPRING_RATE").filter(|v| *v > 0.0));
        let stop_rate = s.num(sec, "BUMP_STOP_RATE").filter(|v| *v > 0.0).unwrap_or(50000.0);
        for (num, range, damping) in [(3u32, up, BUMP_DAMPING_UP), (4u32, dn, BUMP_DAMPING_DOWN)] {
            // damping only where the template's stop has one (stock cars omit it on some stops)
            let template_damped = co.sub(num).map(|b| b.get(6).is_some()).unwrap_or(false);
            let b = co.msg_mut(num)?;
            f(b, 1, range);
            f(b, 2, 0.01);
            f(b, 3, stop_rate * 0.01);
            f(b, 4, if num == 3 { gamma } else { 1.5 });
            f(b, 5, if num == 3 { stop_len } else { 0.02 });
            pbx::fo(b, 6, template_damped.then_some(damping));
        }
        f(&mut co, 5, collar);
        let g = |key: &str, dflt: f64| s.num(sec, key).filter(|v| *v > 0.0).unwrap_or(dflt);
        let ac1_slow = [g("DAMP_BUMP", 3000.0), g("DAMP_REBOUND", 6000.0)];
        // rule 9: slow damping at least the stock fleet's median damping
        // ratio (AC1 cars often convert floaty: the Countach at 0.42 / 0.45
        // where Kunos's EVO one runs 0.92 / 0.82; profile::damping_floor)
        let mc = c.d.mass * share[a] / 2.0 - hub;
        let (floor, floor_name) = profile::damping_floor(a);
        let slow = if c.d.profile.rules().damping { raise_damping(k, mc, ac1_slow, floor) } else { ac1_slow };
        if slow != ac1_slow {
            let cc = critical(k, mc);
            c.note(format!(
                "physics rule 9 (damping): {sec} slow bump / rebound {:.0} / {:.0} -> {:.0} / {:.0} N s/m, damping ratio {:.2} / {:.2} -> {:.2} / {:.2} ({floor_name} {:.2} / {:.2}); fast dampers unchanged",
                ac1_slow[0], ac1_slow[1], slow[0], slow[1], ac1_slow[0] / cc, ac1_slow[1] / cc, slow[0] / cc, slow[1] / cc, floor[0], floor[1]
            ));
        }
        c.d.damp_slow[a] = slow;
        {
            let d = co.msg_mut(6)?;
            {
                let fa = d.msg_mut(1)?;
                f(fa, 1, g("DAMP_FAST_BUMP", ac1_slow[0] * 0.5));
                f(fa, 2, g("DAMP_FAST_REBOUND", ac1_slow[1] * 0.5));
            }
            {
                let sl = d.msg_mut(2)?;
                f(sl, 1, slow[0]);
                f(sl, 2, slow[1]);
            }
            f(d, 3, g("DAMP_FAST_BUMPTHRESHOLD", 0.1));
            f(d, 4, g("DAMP_FAST_REBOUNDTHRESHOLD", 0.1));
            d.remove(13);
            d.remove(14);
        }
        co.remove(7);
        co.remove(8);
        co.fields.sort_by_key(|x| x.num);
        coil.push(co);
        geos.push(geometry(c, &s, sec));
        c.note(format!(
            "physics: {sec} {} k {k:.0} N/m, sag {:.3} m, rest x {:+.3} m, collar {collar:.3} m, bump {up:.3} / droop {dn:.3} m, ride target {:.0} mm",
            s.get(sec, "TYPE").unwrap_or("DWB").trim(), sag, x, c.d.target_height_mm[a]
        ));
    }
    c.d.track = track;
    c.d.base_y = base_y;
    let mut arb = [s.num("ARB", "FRONT").unwrap_or(0.0).max(0.0), s.num("ARB", "REAR").unwrap_or(0.0).max(0.0)];
    // rule 7: an RWD car whose front roll-stiffness share is far below the
    // RWD stock fleet's (for its weight split) gets bar stiffness moved from
    // the rear to the front, up to the fleet's p25; FWD / AWD stock cars run
    // low margins by design and are left alone
    if c.d.profile.rules().roll_balance && c.d.ac1_traction == "RWD" {
        let spring = c.d.spring;
        let margin = roll_share(spring, arb, track) - c.d.cg_front;
        if let Some(mut new) = roll_balance(spring, arb, track, c.d.cg_front) {
            // inside AC1's own setup range and grid when it has one, and the
            // stock envelope's 130000 N/m
            let si = c.ini("setup.ini").clone();
            let mut kept = Vec::new();
            for (a, sec) in ["ARB_FRONT", "ARB_REAR"].iter().enumerate() {
                if let (Some(lo), Some(hi)) = (si.num(sec, "MIN"), si.num(sec, "MAX")) {
                    let step = si.num(sec, "STEP").filter(|v| *v > 0.0).unwrap_or(1.0);
                    let v = (lo + ((new[a].clamp(lo, hi) - lo) / step).round() * step).min(hi);
                    if (v - new[a]).abs() > 1.0 {
                        kept.push(format!("{sec} {:.0} kept on AC1's setup range {lo:.0} .. {hi:.0} step {step:.0}", v));
                    }
                    new[a] = v;
                }
                new[a] = new[a].min(ARB_MAX);
            }
            let total = |b: [f64; 2]| (spring[0] + 2.0 * b[0]) * track[0] * track[0] + (spring[1] + 2.0 * b[1]) * track[1] * track[1];
            let change = total(new) / total(arb) - 1.0;
            c.note(format!(
                "physics rule 7 (roll balance): RWD with a front roll-stiffness share of {:.3}, {margin:+.3} over its {:.0} % front weight (RWD stock p10 {:+.3}); anti-roll bars {:.0} / {:.0} -> {:.0} / {:.0} N/m, now {:+.3} (RWD stock p25 {:+.3}){}{}",
                margin + c.d.cg_front, c.d.cg_front * 100.0, profile::ROLL_MARGIN_RWD_P10, arb[0], arb[1], new[0], new[1],
                roll_share(spring, new, track) - c.d.cg_front, profile::ROLL_MARGIN_RWD_P25,
                if change.abs() > 0.01 { format!("; total roll stiffness {:+.0} %", change * 100.0) } else { String::new() },
                if kept.is_empty() { String::new() } else { format!("; {}", kept.join(", ")) }
            ));
            arb = new;
        }
    }
    c.d.arb = arb;
    let sp = at(&mut c.cd, &[2])?;
    f(sp, 2, c.d.wheelbase);
    f(sp, 3, c.d.cg_front);
    f(sp, 50, base_y[0]);
    f(sp, 51, base_y[1]);
    f(sp, 52, track[0]);
    f(sp, 53, track[1]);
    sp.set(8, Val::Msg(coil[0].clone()));
    sp.set(9, Val::Msg(coil[1].clone()));
    sp.set(40, Val::Msg(geos[0].clone()));
    sp.set(41, Val::Msg(geos[1].clone()));
    sp.remove(17);
    if heave.iter().any(|k| *k > 0.0) {
        for (a, sec) in ["HEAVE_FRONT", "HEAVE_REAR"].iter().enumerate() {
            sp.push(17, Val::Msg(heave_coilover(&s, sec, &coil[a], heave[a], rests[a])?));
        }
        sp.fields.sort_by_key(|x| x.num);
    }
    for (num, v) in [(18u32, arb[0]), (19, arb[1])] {
        let a = sp.msg_mut(num)?;
        f(a, 1, v);
        a.set(2, Val::Msg(Msg::default()));
    }
    // flex bars (twist beams) are the donor's: AC1 has none
    for num in [24u32, 25] {
        let mut fb = Msg::default();
        fb.set(2, Val::Msg(Msg::default()));
        sp.set(num, Val::Msg(fb));
    }
    sp.remove(60);
    sp.remove(61);
    Ok(())
}

/// AC1's rod length for an axle (0 front, 1 rear): suspensions.ini's
/// ROD_LENGTH, clamped into setup.ini's ROD_LENGTH_<corner> range when it
/// has one. Those ranges are absolute, in 0.1 mm (on 44 of 45 Kunos AC1 cars
/// ROD_LENGTH x 10000 lies inside them), and AC1 runs an out-of-range value
/// at the nearest end: La Shush Street's rear 0.09 m sits at MAX -100, its
/// saved setup's 30 clicks above MIN -400. Returns the rod and, when it was
/// clamped, (suspensions.ini value, range min, range max) in metres.
fn ac1_rod(susp: &Ini, setup: &Ini, a: usize) -> (f64, Option<(f64, f64, f64)>) {
    let raw = susp.num(if a == 0 { "FRONT" } else { "REAR" }, "ROD_LENGTH").unwrap_or(0.0);
    let corners = if a == 0 { ["LF", "RF"] } else { ["LR", "RR"] };
    let range = corners.iter().find_map(|cn| {
        let sec = format!("ROD_LENGTH_{cn}");
        Some((setup.num(&sec, "MIN")? * ROD_UNIT, setup.num(&sec, "MAX")? * ROD_UNIT))
    }).filter(|(lo, hi)| hi > lo);
    match range {
        Some((lo, hi)) if raw < lo - 1e-9 || raw > hi + 1e-9 => (raw.clamp(lo, hi), Some((raw, lo, hi))),
        _ => (raw, None),
    }
}

/// AC1 setup.ini ROD_LENGTH values are in 0.1 mm.
pub const ROD_UNIT: f64 = 0.0001;

/// A corner's static compression (m) with no rod: its sprung load (a wheel's
/// share of the car less the hub, at least half the share) over its wheel
/// rate plus half the axle's heave spring, which AC1 runs on the two hubs'
/// mean travel, so each wheel carries half of it.
pub fn sag(load: f64, hub_mass: f64, k: f64, heave: f64) -> f64 {
    (load - hub_mass * G).max(load * 0.5) / (k + heave / 2.0)
}

/// AC1's heave spring rate on an axle (0 front, 1 rear), N/m; 0 without a
/// [HEAVE_FRONT] / [HEAVE_REAR] spring, or in a test build that leaves them
/// out (`ACECONV_HEAVE=off`).
fn heave_rate(s: &Ini, a: usize) -> f64 {
    if heave_off() {
        return 0.0;
    }
    s.num(if a == 0 { "HEAVE_FRONT" } else { "HEAVE_REAR" }, "SPRING_RATE").filter(|v| *v > 0.0).unwrap_or(0.0)
}

fn heave_off() -> bool {
    std::env::var("ACECONV_HEAVE").map(|v| v.eq_ignore_ascii_case("off")).unwrap_or(false)
}

/// One axle's heave spring as a CoiloverData, on that axle's corner coilover
/// (its stop shape and template fields): AC1's rate, progressive rate and
/// dampers, the corners' collar (AC1 runs the heave on the corners' mean
/// ROD_LENGTH) and stops placed as the corners' are: BUMPSTOP_UP from the
/// design pose at BUMP_STOP_RATE, at least 1.5 cm past the rest x, and droop
/// room BUMPSTOP_DN past AC1's rest. An axle without one gets a zero spring
/// and no damping, so the rear stays the second entry. `rest` is the axle's
/// (collar, rest x, AC1 rest x).
fn heave_coilover(s: &Ini, sec: &str, corner: &Msg, k: f64, rest: (f64, f64, f64)) -> Result<Msg> {
    let (collar, x, x_ac1) = rest;
    let mut co = corner.clone();
    f(&mut co, 1, k);
    pbx::fo(&mut co, 2, s.num(sec, "PROGRESSIVE_SPRING_RATE").filter(|v| *v > 0.0 && k > 0.0));
    let up = s.num(sec, "BUMPSTOP_UP").unwrap_or(0.08).clamp(0.01, BUMP_MAX).max(x + 0.015);
    let dn = (s.num(sec, "BUMPSTOP_DN").unwrap_or(0.08) + x_ac1).clamp(0.01, DROOP_MAX).max(x.abs() + 0.015);
    let stop_rate = s.num(sec, "BUMP_STOP_RATE").filter(|v| *v > 0.0).unwrap_or(50000.0);
    for (num, range) in [(3u32, up), (4u32, dn)] {
        let b = co.msg_mut(num)?;
        f(b, 1, range);
        f(b, 3, stop_rate * 0.01);
        f(b, 4, 1.5);
        f(b, 5, 0.02);
    }
    f(&mut co, 5, collar);
    let g = |key: &str, dflt: f64| if k > 0.0 { s.num(sec, key).filter(|v| *v > 0.0).unwrap_or(dflt) } else { 0.0 };
    let slow = [g("DAMP_BUMP", 3000.0), g("DAMP_REBOUND", 6000.0)];
    let d = co.msg_mut(6)?;
    {
        let fa = d.msg_mut(1)?;
        f(fa, 1, g("DAMP_FAST_BUMP", slow[0] * 0.5));
        f(fa, 2, g("DAMP_FAST_REBOUND", slow[1] * 0.5));
    }
    {
        let sl = d.msg_mut(2)?;
        f(sl, 1, slow[0]);
        f(sl, 2, slow[1]);
    }
    f(d, 3, g("DAMP_FAST_BUMPTHRESHOLD", 0.1));
    f(d, 4, g("DAMP_FAST_REBOUNDTHRESHOLD", 0.1));
    co.fields.sort_by_key(|x| x.num);
    Ok(co)
}

/// The front axle's share of roll stiffness: per axle (wheel rate + 2 x
/// ARB) x track^2.
fn roll_share(spring: [f64; 2], arb: [f64; 2], track: [f64; 2]) -> f64 {
    let k = [0, 1].map(|a| (spring[a] + 2.0 * arb[a]) * track[a] * track[a]);
    k[0] / (k[0] + k[1]).max(1e-9)
}


/// Critical damping of a corner: 2 x sqrt(wheel rate x sprung corner mass).
fn critical(k: f64, mc: f64) -> f64 {
    2.0 * (k * mc.max(50.0)).sqrt()
}

/// Rule 9's slow [bump, rebound]: AC1's, raised to `p25` x critical where
/// below it, rounded up to 50 N s/m.
fn raise_damping(k: f64, mc: f64, ac1: [f64; 2], p25: [f64; 2]) -> [f64; 2] {
    let cc = critical(k, mc);
    [0, 1].map(|i| {
        let floor = (p25[i] * cc / 50.0).ceil() * 50.0;
        if ac1[i] < p25[i] * cc { floor } else { ac1[i] }
    })
}

/// The stock envelope's largest anti-roll bar (setup.rs ENVELOPE "arbs").
const ARB_MAX: f64 = 130000.0;

/// Rule 7's bars, when the front share's margin over the front weight is
/// below the RWD stock p10: the p25 margin, with the axles' total roll
/// stiffness kept (or, when that would need a negative rear bar, the rear
/// bar off and the front raised, which stiffens the car overall), rounded
/// to 100 N/m. None for a degenerate track.
fn roll_balance(spring: [f64; 2], arb: [f64; 2], track: [f64; 2], cg_front: f64) -> Option<[f64; 2]> {
    if track.iter().any(|t| *t < 0.5) || roll_share(spring, arb, track) - cg_front >= profile::ROLL_MARGIN_RWD_P10 {
        return None;
    }
    let s = (cg_front + profile::ROLL_MARGIN_RWD_P25).min(0.95);
    let t2 = [track[0] * track[0], track[1] * track[1]];
    let total = (spring[0] + 2.0 * arb[0]) * t2[0] + (spring[1] + 2.0 * arb[1]) * t2[1];
    let mut rear = ((1.0 - s) * total / t2[1] - spring[1]) / 2.0;
    let front = if rear < 0.0 {
        rear = 0.0;
        (s / (1.0 - s) * spring[1] * t2[1] / t2[0] - spring[0]) / 2.0
    } else {
        (s * total / t2[0] - spring[0]) / 2.0
    };
    Some([(front.max(0.0) / 100.0).round() * 100.0, (rear / 100.0).round() * 100.0])
}

#[cfg(test)]
mod tests {
    use super::{ac1_rod, critical, heave_coilover, raise_damping, roll_balance, roll_share, sag, tilt_for_ac1_antisquat};

    #[test]
    fn heave_spring_carries_its_half_of_the_axle() {
        use crate::car::physics::G;
        // La Shush Taipan front: 650.1 kg static, 44.8 % front, hub 19 kg,
        // corners 49000 N/m on rod 0.010, heave 120000 N/m (its progressive
        // 15000 N/m/m adds under 1 N at rest). AC1 (acs.exe
        // HeaveSpring::step) runs the heave on the corners' rod, so at rest
        // 2 k t + K t = 2 x the sprung wheel load: t = 11.4 mm, 1.4 mm into
        // bump; without the heave spring 25.4 mm, 15.4 mm into bump (the 3 Nov
        // log's 15.9 at standstill)
        let load = 650.1 * G * 0.448 / 2.0;
        let t = sag(load, 19.0, 49000.0, 120000.0);
        assert!((2.0 * 49000.0 * t + 120000.0 * t - 2.0 * (load - 19.0 * G)).abs() < 1e-6);
        assert!(((t - 0.010) * 1000.0 - 1.4).abs() < 0.1);
        assert!(((sag(load, 19.0, 49000.0, 0.0) - 0.010) * 1000.0 - 15.4).abs() < 0.1);
    }

    #[test]
    fn heave_coilover_from_ac1() {
        use crate::ac1::ini::parse;
        use crate::car::physics::pbx::{rf, rf_at};
        use crate::io::pb::Msg;
        let s = parse("[HEAVE_FRONT]\nROD_LENGTH=0.00\nSPRING_RATE=120000\nPROGRESSIVE_SPRING_RATE=15000\nBUMP_STOP_RATE=180000\nBUMPSTOP_UP=0.060\nBUMPSTOP_DN=0.080\nPACKER_RANGE=0.080\nDAMP_BUMP=4000\nDAMP_FAST_BUMP=1700\nDAMP_FAST_BUMPTHRESHOLD=0.088\nDAMP_REBOUND=5500\nDAMP_FAST_REBOUND=2660\nDAMP_FAST_REBOUNDTHRESHOLD=0.180\n");
        // the corner coilover it sits on: 49000 N/m, collar 0.010
        let mut corner = Msg::default();
        crate::car::physics::pbx::f(&mut corner, 1, 49000.0);
        crate::car::physics::pbx::f(&mut corner, 5, 0.010);
        let h = heave_coilover(&s, "HEAVE_FRONT", &corner, 120000.0, (0.010, 0.0014, 0.0014)).unwrap();
        assert_eq!(rf(&h, 1), Some(120000.0));
        assert_eq!(rf(&h, 2), Some(15000.0));
        // the corners' collar, not the section's ROD_LENGTH 0.00
        assert!((rf(&h, 5).unwrap() - 0.010).abs() < 1e-6);
        assert!((rf_at(&h, &[3, 1]).unwrap() - 0.060).abs() < 1e-6);
        assert!((rf_at(&h, &[3, 3]).unwrap() - 1800.0).abs() < 1e-3);
        assert_eq!(rf_at(&h, &[6, 2, 1]), Some(4000.0));
        assert_eq!(rf_at(&h, &[6, 1, 2]), Some(2660.0));
        // an axle without one: a zero spring, no damping
        let none = heave_coilover(&s, "HEAVE_REAR", &corner, 0.0, (0.020, 0.0079, 0.0079)).unwrap();
        assert_eq!(rf(&none, 1), Some(0.0));
        assert_eq!(rf(&none, 2), None);
        assert_eq!(rf_at(&none, &[6, 2, 1]), Some(0.0));
    }

    #[test]
    fn rod_clamped_into_the_setup_range() {
        use crate::ac1::ini::parse;
        // La Shush Street: rear 0.09 m against a -400 .. -100 (0.1 mm) range
        let susp = parse("[FRONT]\nROD_LENGTH=0.000\n[REAR]\nROD_LENGTH=0.09\n");
        let setup = parse("[ROD_LENGTH_LF]\nMIN=-100\nMAX=200\nSTEP=10\n[ROD_LENGTH_LR]\nMIN=-400\nMAX=-100\nSTEP=10\n");
        assert_eq!(ac1_rod(&susp, &setup, 0), (0.0, None));
        let (rear, clamped) = ac1_rod(&susp, &setup, 1);
        assert!((rear + 0.01).abs() < 1e-9 && clamped.is_some());
        // Kunos GT3 RS: 0.045 inside 300 .. 600; no setup range: as written
        let gt3 = parse("[FRONT]\nROD_LENGTH=0.045\n[REAR]\nROD_LENGTH=0.035\n");
        assert_eq!(ac1_rod(&gt3, &parse("[ROD_LENGTH_LF]\nMIN=300\nMAX=600\n"), 0), (0.045, None));
        assert_eq!(ac1_rod(&gt3, &parse(""), 1), (0.035, None));
    }

    #[test]
    fn damping_up_to_p25() {
        // the AC1 Countach front: 38500 N/m, 2800 / 5400 slow, about 0.42 / 0.81
        let (k, mc) = (38500.0, 1565.0 * 0.42 / 2.0 - 70.0);
        let new = raise_damping(k, mc, [2800.0, 5400.0], [profile::DAMP_BUMP_P25[0], profile::DAMP_REBOUND_P25[0]]);
        let cc = critical(k, mc);
        assert!((new[0] / cc - 0.72).abs() < 0.01 && (new[1] / cc - 0.90).abs() < 0.01);
        // already above: kept
        assert_eq!(raise_damping(k, mc, [9000.0, 12000.0], [0.72, 0.90]), [9000.0, 12000.0]);
    }
    use crate::car::physics::profile;

    #[test]
    fn roll_balance_to_p25() {
        // the AC1 Countach (tracks 1.536 / 1.606): about -0.007 at 42 % front weight
        let (k, a, t) = ([38500.0, 65500.0], [21000.0, 19500.0], [1.536, 1.606]);
        assert!((roll_share(k, a, t) - 0.42 + 0.007).abs() < 0.01);
        let new = roll_balance(k, a, t, 0.42).unwrap();
        assert!((roll_share(k, new, t) - 0.42 - profile::ROLL_MARGIN_RWD_P25).abs() < 0.005);
        assert!(new[0] > a[0] && new[1] < a[1]);
        // in range: untouched
        assert!(roll_balance([62337.0, 114867.0], [44182.0, 21290.0], [1.5, 1.5], 0.38).is_none());
        // a degenerate track: untouched
        assert!(roll_balance(k, a, [0.0, 1.6], 0.42).is_none());
    }

    #[test]
    fn roll_balance_rear_bar_floor() {
        // so rear-heavy that the target needs a negative rear bar: rear off,
        // front raised to the p25 share
        let (k, a, t) = ([20000.0, 90000.0], [5000.0, 30000.0], [1.5, 1.5]);
        let new = roll_balance(k, a, t, 0.45).unwrap();
        assert_eq!(new[1], 0.0);
        assert!((roll_share(k, new, t) - 0.45 - profile::ROLL_MARGIN_RWD_P25).abs() < 0.005);
    }

    #[test]
    fn antisquat_tilt_gives_evo_ac1s_slope() {
        // La Shush Neonstride rear (pro-squat in AC1, anti-squat as EVO reads it)
        let p = [[0.55, -0.0825, 0.12], [0.55, -0.0825, -0.12], [0.36, -0.135, 0.28], [0.44, -0.12, 0.03], [0.065, -0.04, 0.02], [0.06, -0.104, 0.025]];
        let r = 0.315;
        let (new, before, after) = tilt_for_ac1_antisquat(p, r).unwrap();
        // instant centre 0.59 m behind, 0.083 below the wheel centre
        assert!((before - 0.083 / 0.59).abs() < 0.005);
        assert!((after - (0.315 - 0.083) / -0.59).abs() < 0.005);
        let line = |a: [f64; 3], b: [f64; 3]| {
            let k = (a[1] - b[1]) / (a[2] - b[2]);
            (a[1] - k * a[2], k)
        };
        let (au, ku) = line(new[0], new[1]);
        let (al, kl) = line(new[2], new[3]);
        let zic = (al - au) / (ku - kl);
        assert!(((au + ku * zic) / zic - after).abs() < 1e-9);
        // the front view under each ball joint is kept
        let (au0, ku0) = line(p[0], p[1]);
        let (al0, kl0) = line(p[2], p[3]);
        assert!((au + ku * p[4][2] - (au0 + ku0 * p[4][2])).abs() < 1e-12);
        assert!((al + kl * p[5][2] - (al0 + kl0 * p[5][2])).abs() < 1e-12);
        // near-parallel arms (La Shush Bullet-like) are left alone
        let flat = [[0.5, -0.1, 0.12], [0.5, -0.1, -0.12], [0.4, -0.2, 0.25], [0.4, -0.2, -0.05], p[4], p[5]];
        assert!(tilt_for_ac1_antisquat(flat, r).is_none());
    }
}
