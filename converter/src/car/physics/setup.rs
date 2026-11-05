//! The stock setup (`.carsetup`) and its limits (`.carsetuplimits`).
//!
//! - Every value of the setup lies inside its limit; every limit covers the
//!   car's own value.
//! - Items AC1's setup.ini adjusts are modifiable with AC1's range and step
//!   (units converted: AC1 differential locks, brake bias and brake power
//!   are percent; EVO's diff locks are 0..1). Everything else is fixed at
//!   the car's value (min = max), as on stock road cars, except what the
//!   profile rules (profile.rs) open: front bias (rule 3), pressure,
//!   camber and toe (rule 5, with the stock class's alignment) and the slow
//!   dampers (rule 8).
//! - Ranges and steps are clamped to the game-wide envelope read off all 81
//!   stock `.carsetuplimits` (`ENVELOPE`, measured with examples/setupenv.rs).
//! - Ride height: the setup's `front/rear_target_height` (mm) is the ride
//!   height at the car.ini pickup points (PICKUP_FRONT/REAR_HEIGHT below
//!   the CoG) of the drawn AC1 model; the collar positions (mm) are the
//!   CarData collars. Stock setups are solved from the targets: their
//!   collars differ from CarData's (mx5_na 106 / 98 mm against 122 / 107).
//!   AC1 ROD_LENGTH_xx ranges become a target-height range.

use anyhow::Result;

use super::pbx::{self, f};
use super::Ctx;
use crate::ac1::ini::Ini;
use crate::io::pb::{Msg, Val};

/// (item, min, max, smallest step): the game-wide envelope of the 81 stock
/// `.carsetuplimits`, measured by examples/setupenv.rs
/// (tools/setupenv_stock.tsv): min / max over the limits in use (modifiable
/// ranges and fixed values), the smallest step of a modifiable limit.
const ENVELOPE: [(&str, f64, f64, f64); 17] = [
    ("abs", 0.0, 11.0, 1.0),
    ("arbs", 0.0, 130000.0, 1.0),
    ("bump_rate", 0.0, 10000.0, 100.0),
    ("camber", -6.0, 1.5, 0.01),
    ("damper", 0.0, 26500.0, 1.0),
    ("diff", 0.0, 1.0, 0.01),
    ("front_bias", 48.0, 90.0, 0.1),
    ("fuel", 0.0, 150.0, 1.0),
    ("preload", 0.0, 300.0, 1.0),
    ("pressure", 13.0, 45.0, 0.1),
    ("steer_ratio", -18.0, 20.0, 0.5),
    ("target_height", 30.0, 190.0, 0.5),
    ("tc", 0.0, 12.0, 1.0),
    ("toe", -0.5, 0.5, 0.01),
    ("torque_multiplier", 75.0, 110.0, 1.0),
    ("turbo", 0.0, 100.0, 10.0),
    ("wheel_rate", 0.0, 400000.0, 1.0),
];

/// Stock setup alignment per EVO tyre class (examples/handlingsurvey.rs,
/// tools/handling_stock.tsv; the class of each stock car's front tyre):
/// (class, cold pressure psi front / rear (median), camber range front
/// and rear (p10 .. p90, degrees), toe front / rear (median, the setup's
/// `toe`)). Classes with one or two stock cars borrow the nearest
/// populated class's ranges.
struct Align {
    class: &'static str,
    psi: [f64; 2],
    camber: [(f64, f64); 2],
    toe: [f64; 2],
}
const ALIGN: [Align; 6] = [
    Align { class: "eco", psi: [27.0, 27.0], camber: [(-1.8, -0.3), (-1.75, -0.4)], toe: [-0.05, 0.12] },
    Align { class: "road", psi: [28.0, 28.0], camber: [(-1.8, -0.6), (-1.75, -0.4)], toe: [-0.01, 0.08] },
    Align { class: "supercar", psi: [27.0, 27.0], camber: [(-1.9, -0.9), (-1.9, -1.2)], toe: [-0.05, 0.10] },
    Align { class: "hypercar", psi: [28.0, 27.0], camber: [(-2.3, -0.5), (-2.3, -1.0)], toe: [-0.01, 0.15] },
    Align { class: "racing_slicks", psi: [25.5, 25.0], camber: [(-4.0, -2.5), (-3.5, -2.8)], toe: [-0.05, 0.10] },
    Align { class: "f1_2004", psi: [23.0, 23.0], camber: [(-4.0, -2.5), (-3.5, -2.5)], toe: [0.05, 0.15] },
];

fn env(item: &str) -> (f64, f64, f64) {
    ENVELOPE.iter().find(|e| e.0 == item).map(|e| (e.1, e.2, e.3)).unwrap_or((f64::MIN, f64::MAX, 0.0))
}

/// A SetupSettingLimits {1 step, 2 min, 3 max, 4 is_modifiable}.
fn lim(item: &str, value: f64, range: Option<(f64, f64, f64)>) -> Msg {
    let (emin, emax, estep) = env(item);
    let mut m = Msg::default();
    match range {
        Some((lo, hi, step)) => {
            let lo = lo.max(emin).min(value);
            let hi = hi.min(emax).max(value);
            f(&mut m, 1, step.max(estep));
            f(&mut m, 2, lo);
            f(&mut m, 3, hi);
            pbx::b(&mut m, 4, hi > lo);
        }
        None => {
            let _ = estep;
            f(&mut m, 1, if value.abs() < 10.0 { 0.01 } else { 1.0 });
            f(&mut m, 2, value);
            f(&mut m, 3, value);
        }
    }
    m
}

/// An AC1 setup.ini item's (min, max, step) times `scale`, when present.
fn ac1_range(s: &Ini, sec: &str, scale: f64) -> Option<(f64, f64, f64)> {
    let lo = s.num(sec, "MIN")?;
    let hi = s.num(sec, "MAX")?;
    let step = s.num(sec, "STEP").filter(|v| *v > 0.0).unwrap_or(1.0);
    (hi > lo).then(|| (lo * scale, hi * scale, step * scale))
}

fn msg(fields: Vec<(u32, Val)>) -> Msg {
    let mut m = Msg::default();
    for (n, v) in fields {
        m.push(n, v);
    }
    m
}

fn fv(x: f64) -> Val {
    Val::F32((x as f32).to_le_bytes())
}

fn mv(m: Msg) -> Val {
    Val::Msg(m)
}

/// Stock setups' largest rear toe-in, degrees (60 stock cars, p50 0.10, p90
/// 0.20; tools/handling_stock.tsv).
const REAR_TOE_MAX: f64 = 0.35;
/// The reference corner for the rear-steer stand-in: speed, steering wheel
/// angle as a share of AC1's lock, off throttle, rear not sliding.
const REF_KMH: f64 = 120.0;
const REF_LOCK_SHARE: f64 = 0.15;
/// Share of the rear axle's load on the outside wheel's side of the
/// difference near the limit ((outer - inner) / (outer + inner)): toe-in t
/// steers the loaded outside wheel into the corner, so it acts like about
/// t x this much rear steer in phase. OURS, a round figure for a GT car.
const TOE_IN_PHASE: f64 = 0.6;

/// `ACECONV_REAR_STEER=toe` (test builds): AC1's ctrl_4ws.ini rear steer,
/// which EVO does not simulate, stood in for by rear toe-in. The AC1 911 GT3
/// RS steers the rear in phase above 80 km/h, most off the throttle (gas 0 ->
/// x1.2, full gas x0.1) and while the rear is not sliding: a corner-entry
/// stabiliser. Its controllers are evaluated at the reference corner (output
/// read as radians); the in-phase angle over TOE_IN_PHASE is added to the
/// stock setup's rear toe-in. Returns (in-phase deg, toe added deg).
fn rear_steer_toe(c: &Ctx) -> Option<(f64, f64)> {
    if !std::env::var("ACECONV_REAR_STEER").map(|v| v.eq_ignore_ascii_case("toe")).unwrap_or(false) {
        return None;
    }
    let t = c.ini("ctrl_4ws.ini");
    let lock = c.ini("car.ini").num("CONTROLS", "STEER_LOCK").unwrap_or(400.0);
    let input = |name: &str| match name {
        "STEER_DEG" => lock * REF_LOCK_SHARE,
        "SPEED_KMH" => REF_KMH,
        _ => 0.0, // GAS, slip angles, LATG ...: off throttle, not sliding
    };
    let mut v = 0.0;
    let mut any = false;
    for k in 0..16 {
        let sec = format!("CONTROLLER_{k}");
        let Some(input_name) = t.get(&sec, "INPUT") else { continue };
        let l = crate::ac1::ini::lut(t.get(&sec, "LUT").unwrap_or(""));
        let x = crate::ac1::ini::lut_at(&l, input(input_name.trim())).unwrap_or(0.0);
        v = match t.get(&sec, "COMBINATOR").map(str::trim) {
            Some("MULT") => v * x,
            _ => v + x,
        };
        if let (Some(lo), Some(hi)) = (t.num(&sec, "DOWN_LIMIT"), t.num(&sec, "UP_LIMIT")) {
            v = v.clamp(lo.min(hi), hi.max(lo));
        }
        any = true;
    }
    let deg = v.abs().to_degrees();
    (any && deg > 0.01).then_some((deg, deg / TOE_IN_PHASE))
}

/// `ACECONV_TEST_AIDS` (test builds), e.g. `tc=0,abs=1,telemetry=10,turbo=15`:
/// the stock setup's TC and ABS levels, MoTeC telemetry laps (0 .. 99) and
/// boost (% of an adjustable turbo), so drive tests start on the same aids
/// with logging on.
fn test_aids() -> std::collections::BTreeMap<String, f64> {
    std::env::var("ACECONV_TEST_AIDS").unwrap_or_default().split(',')
        .filter_map(|kv| kv.split_once('='))
        .filter_map(|(k, v)| Some((k.trim().to_lowercase(), v.trim().parse().ok()?)))
        .collect()
}

/// The setup's front / rear wing angle (AeroSettings 4 / 5): AC1's aero.ini
/// ANGLE, with AC1's setup.ini WING_n range. The game flies the wings at the
/// setup's angle, not the wing's own (WingData f13): the converter used to
/// leave the setup's at 0, and the converted AC1 911 GT3 RS (wings 20 / 50
/// deg) ran a MoTeC AERO_BALANCE of 144 % front (its wings at 0 deg: about
/// 150 %); with the setup at 20 / 50 it read 34.4 %, AC1's own 34 %
/// (Brands Hatch Indy, 1 Nov 2026), and slid a third less. Kunos's stock
/// setups fill both (992 GT3 Cup 3 / 2). Front is the wing furthest ahead of
/// the CoG, rear the furthest behind (AC1 WING_0 is usually the body); which
/// WingData the game ties each setup angle to is inferred from that one car.
/// `ACECONV_SETUP_WINGS=off` leaves them at 0 (A / B builds).
/// Returns [front, rear] (angle, setup range).
fn setup_wings(c: &Ctx) -> [Option<(f64, Option<(f64, f64, f64)>)>; 2] {
    if std::env::var("ACECONV_SETUP_WINGS").map(|v| v.eq_ignore_ascii_case("off")).unwrap_or(false) {
        return [None, None];
    }
    let a = c.ini("aero.ini");
    let s = c.ini("setup.ini");
    let wings: Vec<(String, f64, f64)> = a.with_prefix("WING_")
        .filter_map(|(sec, _)| Some((sec.to_string(), super::ac1_v3(a, sec, "POSITION")?[2], a.num(sec, "ANGLE").unwrap_or(0.0))))
        .collect();
    let pick = |front: bool| {
        wings.iter().filter(|w| if front { w.1 > 0.3 } else { w.1 < -0.3 })
            .max_by(|x, y| if front { x.1.total_cmp(&y.1) } else { y.1.total_cmp(&x.1) })
            .map(|(sec, _, angle)| (*angle, ac1_range(s, sec, 1.0)))
    };
    [pick(true), pick(false)]
}

pub fn write(c: &mut Ctx) -> Result<()> {
    let s = c.ini("setup.ini").clone();
    let si = c.ini("suspensions.ini").clone();
    let d = c.d.clone();
    let corners = ["LF", "RF", "LR", "RR"];
    let ax = |k: usize| if k < 2 { 0 } else { 1 };
    let arb = d.arb;
    // the diff the drivetrain stage made `differential_data` (the rear diff
    // of the four-wheel types), so a setup load never undoes it
    let diff = c.d.diff;
    // slow dampers as the suspension stage left them (rule 9), fast ones AC1's
    let slow = c.d.damp_slow;
    // heave springs as the suspension stage wrote them (front, rear): the
    // setup and its limits carry them after the four corners, springs and
    // dampers 5 / 6, as Kunos's EVO F2004 does; AC1's _HF / _HR setup ranges
    let heaves: Vec<(Msg, &str)> = c.cd.sub(2).map(|x| x.subs(17)).unwrap_or_default().into_iter().zip(["HF", "HR"]).collect();
    let hv = |h: &Msg, p: &[u32]| pbx::rf_at(h, p).unwrap_or(0.0);
    let raw = |a: usize, key: &str, dflt: f64| si.num(if a == 0 { "FRONT" } else { "REAR" }, key).filter(|v| *v > 0.0).unwrap_or(dflt);
    let damp = |a: usize, key: &str, dflt: f64| match key {
        "DAMP_BUMP" => slow[a][0],
        "DAMP_REBOUND" => slow[a][1],
        _ => si.num(if a == 0 { "FRONT" } else { "REAR" }, key).filter(|v| *v > 0.0).unwrap_or(dflt),
    };
    let donor_pressure = c.cd.str(10).and_then(|p| c.read_any(&p)).and_then(|b| Msg::parse(&b).ok()).and_then(|m| pbx::rf(&m, 11)).filter(|v| (13.0..=45.0).contains(v)).unwrap_or(30.0);
    let tyre_ini = c.ini("tyres.ini").clone();
    let pressure = [0usize, 1].map(|axle| {
        let section = if d.default_compound == 0 {
            if axle == 0 { "FRONT".to_string() } else { "REAR".to_string() }
        } else if axle == 0 {
            format!("FRONT_{}", d.default_compound)
        } else {
            format!("REAR_{}", d.default_compound)
        };
        tyre_ini.num(&section, "PRESSURE_IDEAL").filter(|v| (13.0..=45.0).contains(v)).unwrap_or(donor_pressure)
    });
    let ac1_camber = |a: usize| si.num(if a == 0 { "FRONT" } else { "REAR" }, "STATIC_CAMBER").unwrap_or(0.0);
    // rule 5: the stock setup alignment of the EVO tyre class (AC1's
    // PRESSURE_IDEAL is a hot pressure for AC1's tyre model, and its camber
    // was tuned with AC1's), AC1's camber kept where it is inside the
    // class's stock range; camber and toe stay adjustable
    // A slick on a wider-peaked model (tyres.rs `slick_model`: the road
    // model, as Kunos's EVO 190E on the same AC1 tyre) is neither class's
    // tyre, so it keeps AC1's alignment: pressure, camber and the geometry's
    // toe. Drive (1 Nov, Neonstride, packers on both): racing-slick camber
    // (rear -2.8 against AC1's -1.4) left the rear tyres 58 .. 62 deg C
    // against 88 .. 90 with AC1's (AC1 itself 92 .. 110), with a little less
    // cornering grip and smaller held slides; the user felt it too
    let class_align = |cl: &str| ALIGN.iter().find(|a| a.class == cl);
    let slick_on_other = d.slick && d.tyre_class[0] != "racing_slicks";
    let align = d.profile.rules().alignment.then(|| class_align(&d.tyre_class[0])).flatten();
    let (pressure, camber_v, toe_v) = match align {
        Some(_) if slick_on_other => {
            c.note(format!(
                "physics rule 5 (alignment): a slick on the {} model keeps AC1's alignment: pressure {:.1} / {:.1} psi, camber {:.2} / {:.2}, the geometry's toe",
                d.tyre_class[0], pressure[0], pressure[1], ac1_camber(0), ac1_camber(1)
            ));
            (pressure, [ac1_camber(0), ac1_camber(1)], [0.0, 0.0])
        }
        Some(model) => {
            let cam = [0, 1].map(|a| ac1_camber(a).clamp(model.camber[a].0, model.camber[a].1));
            c.note(format!(
                "physics rule 5 (alignment): {} stock setup: pressure {:.1} / {:.1} psi (AC1 ideal {:.1} / {:.1}), camber {:.2} / {:.2} (AC1 {:.2} / {:.2}, stock range {:.2} .. {:.2} / {:.2} .. {:.2}), toe {:.2} / {:.2} (stock median; AC1 TOE_OUT stays the geometry's toe)",
                model.class, model.psi[0], model.psi[1], pressure[0], pressure[1], cam[0], cam[1], ac1_camber(0), ac1_camber(1),
                model.camber[0].0, model.camber[0].1, model.camber[1].0, model.camber[1].1, model.toe[0], model.toe[1]
            ));
            (model.psi, cam, model.toe)
        }
        None => {
            c.note(format!("physics: default tyre pressure {:.1} / {:.1} psi (AC1 ideal where available; EVO donor fallback)", pressure[0], pressure[1]));
            (pressure, [ac1_camber(0), ac1_camber(1)], [0.0, 0.0])
        }
    };
    let toe_v = match rear_steer_toe(c) {
        Some((steer, add)) => {
            let to = (toe_v[1] + add).min(REAR_TOE_MAX);
            c.note(format!(
                "physics: TEST BUILD: ctrl_4ws.ini steers the rear {steer:.2} deg in phase at {REF_KMH} km/h off throttle ({:.0} % of the AC1 lock); EVO has no rear steer, so rear toe-in {:.2} -> {to:.2} deg stands in",
                REF_LOCK_SHARE * 100.0, toe_v[1]
            ));
            [toe_v[0], to]
        }
        None => toe_v,
    };
    let camber = |a: usize| camber_v[a];
    let toe_lin = |a: usize| si.num(if a == 0 { "FRONT" } else { "REAR" }, "TOE_OUT").unwrap_or(0.0);
    let bump_rate = |a: usize| si.num(if a == 0 { "FRONT" } else { "REAR" }, "BUMP_STOP_RATE").unwrap_or(50000.0) * 0.01;
    // AC1's DEFAULT_TURBO_ADJUSTMENT is a fraction (every installed car: 0 ..
    // 1, ks_porsche_935_78_moby_dick 0.90); EVO's turbo item is a percent
    // (stock limits 0 .. 100). Read as-is, 1.00 gave the S14 9 Race 1 %
    // boost (about 250 kW at the wheels of its 713 kW, 2 Nov).
    let turbo_default = (c.ini("engine.ini").num("ENGINE_DATA", "DEFAULT_TURBO_ADJUSTMENT").unwrap_or(1.0) * 100.0).clamp(0.0, 100.0);
    // ---- values
    let mut set = Msg::default();
    set.push(1, mv(msg(vec![
        (1, fv(arb[0])),
        (1, fv(arb[1])),
        (2, fv(d.steer_ratio)),
        (3, mv(msg(vec![(1, fv(d.front_bias)), (2, fv(100.0))]))),
        (4, mv(msg(vec![(1, fv(diff[0])), (2, fv(diff[1])), (3, fv(diff[2]))]))),
    ])));
    for k in 0..4 {
        let a = ax(k);
        set.push(2, mv(msg(vec![
            (1, fv(d.spring[a])),
            (2, mv(msg(vec![(1, fv(d.bump_up[a])), (2, fv(bump_rate(a)))]))),
            (3, mv(msg(vec![(1, fv(d.bump_dn[a])), (2, fv(bump_rate(a)))]))),
        ])));
    }
    for (h, _) in &heaves {
        set.push(2, mv(msg(vec![
            (1, fv(hv(h, &[1]))),
            (2, mv(msg(vec![(1, fv(hv(h, &[3, 1]))), (2, fv(hv(h, &[3, 3])))]))),
            (3, mv(msg(vec![(1, fv(hv(h, &[4, 1]))), (2, fv(hv(h, &[4, 3])))]))),
        ])));
    }
    for k in 0..4 {
        let a = ax(k);
        let sb = damp(a, "DAMP_BUMP", 3000.0);
        let sr = damp(a, "DAMP_REBOUND", 6000.0);
        set.push(3, mv(msg(vec![(1, fv(sb)), (2, fv(damp(a, "DAMP_FAST_BUMP", raw(a, "DAMP_BUMP", 3000.0) * 0.5))), (3, fv(sr)), (4, fv(damp(a, "DAMP_FAST_REBOUND", raw(a, "DAMP_REBOUND", 6000.0) * 0.5)))])));
    }
    for (h, _) in &heaves {
        set.push(3, mv(msg(vec![(1, fv(hv(h, &[6, 2, 1]))), (2, fv(hv(h, &[6, 1, 1]))), (3, fv(hv(h, &[6, 2, 2]))), (4, fv(hv(h, &[6, 1, 2])))])));
    }
    for k in 0..4 {
        let a = ax(k);
        let mut al = msg(vec![(1, fv(pressure[a])), (2, fv(camber(a))), (3, fv(toe_v[a])), (5, fv(camber(a))), (6, fv(toe_lin(a)))]);
        if d.default_compound > 0 {
            al.push(7, fv(d.default_compound as f64));
        }
        set.push(4, mv(al));
    }
    let mut el = Msg::default();
    // the level AC1 starts on (chassis.rs `default_level`), counted in
    // EVO's lightest-first order
    let mut tc_default = if d.tc_on { d.tc_default.max(1) as f64 } else { 0.0 };
    let mut abs_default = if d.abs_on { d.abs_default.max(1) as f64 } else { 0.0 };
    let aids = test_aids();
    if let Some(v) = aids.get("tc").filter(|_| d.has_tc) {
        tc_default = v.clamp(0.0, d.tc_levels as f64);
    }
    if let Some(v) = aids.get("abs").filter(|_| d.has_abs) {
        abs_default = v.clamp(0.0, d.abs_levels.max(1) as f64);
    }
    let telemetry = aids.get("telemetry").map(|v| v.clamp(0.0, 99.0));
    let turbo_default = aids.get("turbo").filter(|_| d.turbo_adjustable).map(|v| v.clamp(0.0, 100.0)).unwrap_or(turbo_default);
    if !aids.is_empty() {
        c.note(format!("physics: TEST BUILD: stock setup aids TC {tc_default} / ABS {abs_default}, telemetry laps {}, boost {turbo_default} %", telemetry.unwrap_or(0.0)));
    }
    let wings = setup_wings(c);
    if d.has_tc {
        el.push(1, fv(tc_default));
    }
    el.push(3, fv(abs_default));
    if let Some(t) = telemetry {
        el.push(5, fv(t));
    }
    if d.turbo_adjustable {
        el.push(7, fv(turbo_default));
    }
    set.push(5, mv(el));
    let mut aero = Msg::default();
    for k in 0..4 {
        aero.push(1, fv(d.collar[ax(k)] * 1000.0));
    }
    aero.push(2, fv(d.target_height_mm[0]));
    aero.push(3, fv(d.target_height_mm[1]));
    for (field, w) in [(4u32, &wings[0]), (5, &wings[1])] {
        if let Some((angle, _)) = w {
            aero.push(field, fv(*angle));
        }
    }
    set.push(6, mv(aero));
    set.push(7, mv(msg(vec![(1, fv(d.fuel))])));
    // ---- limits
    // rule 3: front bias always adjustable (AC1's range, else the span the
    // stock adjustable setups cover: 50 .. 75, widened to +-5 around the car)
    let bias_range = ac1_range(&s, "FRONT_BIAS", 1.0).or_else(|| {
        d.profile.rules().ebb_bias.then(|| ((d.front_bias - 5.0).min(50.0), (d.front_bias + 5.0).max(75.0), 0.5))
    });
    if let Some((lo, hi, _)) = bias_range.filter(|_| s.num("FRONT_BIAS", "MIN").is_none()) {
        c.note(format!("physics rule 3 (brake bias): front bias adjustable in the setup ({:.0} .. {:.0} %, step 0.5; AC1's setup.ini had no FRONT_BIAS)", lo.max(48.0), hi.min(90.0)));
    }
    let mut li = Msg::default();
    let ab = |sec: &str| ac1_range(&s, sec, 1.0).filter(|r| r.1 > 100.0);
    li.push(1, mv(msg(vec![
        (1, mv(lim("arbs", arb[0], ab("ARB_FRONT")))),
        (1, mv(lim("arbs", arb[1], ab("ARB_REAR")))),
        (2, mv(lim("steer_ratio", d.steer_ratio, None))),
        (3, mv(msg(vec![
            (1, mv(lim("front_bias", d.front_bias, bias_range))),
            (2, mv(lim("torque_multiplier", 100.0, ac1_range(&s, "BRAKE_POWER_MULT", 1.0)))),
        ]))),
        (4, mv(msg(vec![
            (1, mv(lim("diff", diff[0], ac1_range(&s, "DIFF_POWER", 0.01)))),
            (2, mv(lim("diff", diff[1], ac1_range(&s, "DIFF_COAST", 0.01)))),
            (3, mv(lim("preload", diff[2], ac1_range(&s, "DIFF_PRELOAD", 1.0)))),
        ]))),
    ])));
    for (k, cn) in corners.iter().enumerate() {
        let a = ax(k);
        let mut sl = msg(vec![
            (1, mv(lim("wheel_rate", d.spring[a], ac1_range(&s, &format!("SPRING_RATE_{cn}"), 1.0).filter(|r| r.1 > 1000.0)))),
            (2, mv(msg(vec![(1, mv(lim("bump_range", d.bump_up[a], None))), (2, mv(lim("bump_rate", bump_rate(a), None)))]))),
            (3, mv(msg(vec![(1, mv(lim("bump_range", d.bump_dn[a], None))), (2, mv(lim("bump_rate", bump_rate(a), None)))]))),
        ]);
        sl.push(4, mv(msg(vec![(1, fv(1.0))])));
        sl.push(5, mv(msg(vec![(1, fv(1.0))])));
        li.push(2, mv(sl));
    }
    for (h, hn) in &heaves {
        let mut sl = msg(vec![
            (1, mv(lim("wheel_rate", hv(h, &[1]), ac1_range(&s, &format!("SPRING_RATE_{hn}"), 1.0).filter(|r| r.1 > 1000.0)))),
            (2, mv(msg(vec![(1, mv(lim("bump_range", hv(h, &[3, 1]), None))), (2, mv(lim("bump_rate", hv(h, &[3, 3]), None)))]))),
            (3, mv(msg(vec![(1, mv(lim("bump_range", hv(h, &[4, 1]), None))), (2, mv(lim("bump_rate", hv(h, &[4, 3]), None)))]))),
        ]);
        sl.push(4, mv(msg(vec![(1, fv(1.0))])));
        sl.push(5, mv(msg(vec![(1, fv(1.0))])));
        li.push(2, mv(sl));
    }
    // rule 8: the slow dampers adjustable around AC1's values (x0.5 .. x1.5,
    // about the span of the stock adjustable road setups, e.g. the 992 GT3
    // RS's 4500 .. 12500 around 8500), item by item where AC1's setup.ini
    // has no range for that corner (a heave-damper car may lack LF / RF)
    let open = d.profile.rules().dampers;
    let around = |v: f64| open.then(|| ((v * 0.5 / 100.0).round() * 100.0, (v * 1.5 / 100.0).round() * 100.0, 100.0));
    let ac1_damp = |key: &str, cn: &str| ac1_range(&s, &format!("{key}_{cn}"), 1.0).filter(|r| r.1 > 100.0);
    let opened: Vec<String> = corners.iter().flat_map(|cn| ["DAMP_BUMP", "DAMP_REBOUND"].map(|k| (k, *cn))).filter(|(k, cn)| open && ac1_damp(k, cn).is_none()).map(|(k, cn)| format!("{k}_{cn}")).collect();
    if !opened.is_empty() {
        c.note(format!(
            "physics rule 8 (dampers): slow bump / rebound adjustable in the setup ({}), x0.5 .. x1.5 around the car's {:.0} / {:.0} front, {:.0} / {:.0} rear (AC1's setup.ini had no range for them)",
            if opened.len() == 8 { "all four corners".to_string() } else { opened.join(" ") },
            damp(0, "DAMP_BUMP", 3000.0), damp(0, "DAMP_REBOUND", 6000.0), damp(1, "DAMP_BUMP", 3000.0), damp(1, "DAMP_REBOUND", 6000.0)
        ));
    }
    for (k, cn) in corners.iter().enumerate() {
        let a = ax(k);
        let sb = damp(a, "DAMP_BUMP", 3000.0);
        let sr = damp(a, "DAMP_REBOUND", 6000.0);
        let r = |key: &str| ac1_damp(key, cn).or_else(|| if key.contains("FAST") { None } else { around(if key == "DAMP_BUMP" { sb } else { sr }) });
        li.push(3, mv(msg(vec![
            (1, mv(lim("damper", sb, r("DAMP_BUMP")))),
            (2, mv(lim("damper", damp(a, "DAMP_FAST_BUMP", raw(a, "DAMP_BUMP", 3000.0) * 0.5), r("DAMP_FAST_BUMP")))),
            (3, mv(lim("damper", sr, r("DAMP_REBOUND")))),
            (4, mv(lim("damper", damp(a, "DAMP_FAST_REBOUND", raw(a, "DAMP_REBOUND", 6000.0) * 0.5), r("DAMP_FAST_REBOUND")))),
        ])));
    }
    for (h, hn) in &heaves {
        li.push(3, mv(msg(vec![
            (1, mv(lim("damper", hv(h, &[6, 2, 1]), ac1_damp("DAMP_BUMP", hn)))),
            (2, mv(lim("damper", hv(h, &[6, 1, 1]), ac1_damp("DAMP_FAST_BUMP", hn)))),
            (3, mv(lim("damper", hv(h, &[6, 2, 2]), ac1_damp("DAMP_REBOUND", hn)))),
            (4, mv(lim("damper", hv(h, &[6, 1, 2]), ac1_damp("DAMP_FAST_REBOUND", hn)))),
        ])));
    }
    for (k, cn) in corners.iter().enumerate() {
        let a = ax(k);
        let mut cam = ac1_range(&s, &format!("CAMBER_{cn}"), 1.0).map(|(lo, hi, st)| {
            // AC1 camber clicks of 1 over a few degrees are tenths of a degree in the game's UI
            (lo, hi, if st >= 1.0 && hi - lo <= 10.0 { 0.1 } else { st })
        });
        let mut toe = s.has(&format!("TOE_OUT_{cn}")).then_some((-0.2, 0.2, 0.01));
        let mut psi = ac1_range(&s, &format!("PRESSURE_{cn}"), 1.0);
        if let Some(al) = align {
            // rule 5: always adjustable, around the stock class's values
            // (a slick keeping AC1's camber: the range takes it in)
            cam = cam.or(Some(((al.camber[a].0 - 1.0).min(camber(a) - 1.0), (al.camber[a].1 + 1.0).max(camber(a) + 1.0), 0.1)));
            toe = Some((-0.3, 0.3f64.max(toe_v[a] + 0.05), 0.01));
            psi = psi.or(Some((pressure[a] - 5.0, pressure[a] + 5.0, 0.5)));
        }
        let mut al = msg(vec![
            (1, mv(lim("pressure", pressure[a], psi))),
            (2, mv(lim("camber", camber(a), cam))),
            (3, mv(lim("toe", toe_v[a], toe))),
            (4, mv(Msg::default())),
            (5, mv(lim("camber", camber(a), None))),
            (6, mv(lim("toe_lin", toe_lin(a), None))),
        ]);
        let n = d.compounds.max(1) as f64 - 1.0;
        al.push(7, mv(lim("compound", d.default_compound as f64, (n > 0.0).then_some((0.0, n, 1.0)))));
        li.push(4, mv(al));
    }
    let mut el = Msg::default();
    el.push(1, mv(lim("tc", tc_default, d.has_tc.then_some((0.0, d.tc_levels as f64, 1.0)))));
    el.push(2, mv(lim("tc", 0.0, None)));
    // every ABS level selectable, as on the stock multi-level cars (M4 GT3 0 .. 10)
    el.push(3, mv(lim("abs", abs_default, d.has_abs.then_some((0.0, d.abs_levels.max(1) as f64, 1.0)))));
    el.push(4, mv(lim("engine_map", 0.0, None)));
    el.push(5, mv(msg(vec![(1, fv(1.0)), (3, fv(99.0)), (4, Val::Varint(1))])));
    el.push(6, mv(lim("esc", 0.0, None)));
    let tl = s.with_prefix("TURBO_").next().and_then(|(sec, _)| ac1_range(&s, sec, 1.0));
    el.push(7, mv(lim("turbo", if d.turbo_adjustable { turbo_default } else { 0.0 }, if d.turbo_adjustable { tl.or(Some((0.0, 100.0, 1.0))) } else { None })));
    li.push(5, mv(el));
    let mut aero = Msg::default();
    for k in 0..4 {
        aero.push(1, mv(lim("collar", d.collar[ax(k)] * 1000.0, None)));
    }
    for (a, cns) in [(0usize, ["LF", "RF"]), (1, ["LR", "RR"])] {
        // AC1's rod range (0.1 mm units -> mm) around the rod the car runs:
        // the ride height moves 1:1 with the rod
        let rod = cns.iter().find_map(|cn| ac1_range(&s, &format!("ROD_LENGTH_{cn}"), super::suspension::ROD_UNIT * 1000.0));
        let t = d.target_height_mm[a];
        let now = d.rod[a] * 1000.0;
        let r = rod.map(|(lo, hi, st)| (t + lo - now, t + hi - now, st.abs().max(0.5)));
        aero.push(2 + a as u32, mv(lim("target_height", t, r)));
    }
    for (field, w) in [(4u32, &wings[0]), (5, &wings[1])] {
        match w {
            Some((angle, range)) => aero.push(field, mv(lim("wing", *angle, *range))),
            None => aero.push(field, mv(Msg::default())),
        }
    }
    li.push(6, mv(aero));
    li.push(7, mv(msg(vec![(1, mv(lim("fuel", d.fuel, Some((1.0, d.max_fuel, 1.0)))))])));
    // write over the donor's files (CarData names them)
    for (num, new) in [(19u32, set), (16u32, li)] {
        let Some(p) = c.cd.str(num) else { continue };
        let mut out = new;
        if num == 19 {
            // keep the donor's final state name and version
            if let Some(old) = c.files.find_ci(&p).and_then(|k| c.files.get(&k)).and_then(|b| Msg::parse(b).ok()) {
                for k in [9u32, 10, 11] {
                    if let Some(v) = old.get(k) {
                        out.push(k, v.clone());
                    }
                }
            }
        }
        c.put(&p, out.encode());
    }
    // the AI and wet setups are the stock one
    for num in [190u32, 191] {
        c.cd.remove(num);
    }
    // what AC1's setup.ini adjusts, then what the rules opened on top
    let mut by_rules: Vec<String> = Vec::new();
    if bias_range.is_some() && s.num("FRONT_BIAS", "MIN").is_none() {
        by_rules.push("FRONT_BIAS".into());
    }
    if align.is_some() {
        by_rules.extend(corners.iter().flat_map(|cn| ["PRESSURE", "CAMBER", "TOE_OUT"].map(|k| format!("{k}_{cn}"))).filter(|n| !s.has(n)));
    }
    by_rules.extend(opened.iter().cloned());
    c.note(format!(
        "physics: setup: ride height targets {:.0} / {:.0} mm, collars {:.0} / {:.0} mm; modifiable: {}{}",
        d.target_height_mm[0], d.target_height_mm[1], d.collar[0] * 1000.0, d.collar[1] * 1000.0,
        s.names().filter(|n| !n.is_empty() && *n != "DISPLAY_METHOD" && *n != "GEARS").collect::<Vec<_>>().join(" "),
        if by_rules.is_empty() { String::new() } else { format!("; opened by the profile rules: {}", by_rules.join(" ")) }
    ));
    Ok(())
}
