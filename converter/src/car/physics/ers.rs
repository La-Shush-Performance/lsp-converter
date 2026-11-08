//! AC1 hybrid systems (`data\ers.ini` + `ctrl_ers_<n>.ini` /
//! `ctrl_ers_front_<n>.ini`) -> EVO CarData f15 ERSData.
//!
//! EVO's `ERS` class is a port of AC1's (same error strings, same front motor
//! code); stock EVO cars ship without it, but the game builds it for any car
//! whose CarData carries f15 (exe 0x14238DB90, read 8 Nov 2026). ERSData:
//! 2 torqueLut, 3 coastLut, 4 batteryChargeKj, 5 hasButtonOverride,
//! 6 maxKjPerLap, 7 heatChargePerc, 8 heatPowerKw, 9 defaultPowerControllerIndex,
//! 11 powerControllersFront, 12 powerControllersRear (DynamicControllerData),
//! 13 brakeRearCorrection, 14 hasCockpitControls, 15 cockpitControls
//! {1 deliveryProfile, 2 mguHMode, 3 recovery}, 16 hasFrontMotors,
//! 17 frontMotors {1 torqueLut, 3 torqueVectoringBias}, 18 maxChargeKjPerLap.
//!
//! What both games do per step (AC1 acs.exe `ERS::step`, EVO 0x14238EAC0):
//! - rear (kinetic) motor torque = torqueLut(engine rpm) x rear level, added
//!   at the engine; the rear level is the active rear controller (0..1);
//! - front motors: frontLut(mean front wheel rpm) x front level, split left /
//!   right by the vectoring bias, on the front wheels whatever the traction;
//! - the coast curve brakes the engine shaft while the driver brakes and the
//!   battery is not full (AC1 always; EVO only while the rear level is below 0,
//!   scaled by it).
//! Where they differ, and the rules:
//! - Battery. AC1 keeps a 0..1 charge and drains it by level / DISCHARGE_TIME
//!   (whatever the power); EVO keeps joules and drains the motors' power.
//!   batteryChargeKj = rear peak power x DISCHARGE_TIME + front peak power x
//!   front DISCHARGE_TIME (OURS): at full delivery and peak power EVO empties
//!   it when AC1 does, below peak power it lasts longer.
//! - EVO drains the battery only while the rear level is above 0: a car that
//!   delivers with the front motors alone (La Shush Taipan: rear curve and
//!   rear controller 0) never empties it in EVO, where AC1 empties it after
//!   the front DISCHARGE_TIME and refills it under braking (game behaviour,
//!   kept: forcing a drain without AC1's brake recharge left the battery flat).
//! - Recharge. AC1 charges from the brakes' work x CHARGE_K; EVO only from
//!   the coast curve's power x the setup's recovery (`ers_recharge_lv` %).
//!   Rule: every rear controller gets a last BRAKE stage that takes the level
//!   to -1 while braking (OURS), so EVO runs the coast curve under braking as
//!   AC1 does; the setup's recovery is 100 % (AC1's default 1.0; EVO's own
//!   default 0.5 is overwritten by the setup, and an absent value is 0, no
//!   recharge). CHARGE_K has no EVO field: EVO recharges less than AC1.
//! - MAX_KJ_PER_LAP: AC1 compares it with joules spent, EVO multiplies it by
//!   1000; maxKjPerLap = MAX_KJ_PER_LAP / 1000 keeps AC1's limit.
//! - [HEAT] (MGU-H): AC1 charges boost ratio x CHARGE_K of the battery per
//!   second; EVO charges boost ratio x gas x heatPowerKw x heatChargePerc:
//!   heatPowerKw = CHARGE_K x batteryChargeKj, heatChargePerc 1 (OURS).
//!   TORQUE_PERC has no EVO field (noted).
//! - The setup's `ers_deployment_map` picks the controller (0-based, as
//!   AC1's DEFAULT_CONTROLLER); the converter writes DEFAULT_CONTROLLER there.

use super::ctrl::{self, Stage};
use super::pbx::{self, b, f, i};
use super::Ctx;
use crate::io::pb::{Msg, Val};
use anyhow::Result;

/// What the setup stage needs to know (Derived::ers).
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct Ers {
    pub controllers: usize,
    pub default: usize,
    /// AC1 [COCKPIT_CONTROLS] RECOVERY: the driver may change the recovery
    pub recovery_adjustable: bool,
    pub battery_kj: f64,
    pub rear_kw: f64,
    pub front_kw: f64,
}

/// Peak power (W) of a torque curve over its own x range (rpm).
fn peak_power(l: &[(f64, f64)], x_max: Option<f64>) -> f64 {
    let (Some(a), Some(z)) = (l.first(), l.last()) else { return 0.0 };
    let hi = x_max.map(|m| m.min(z.0)).unwrap_or(z.0);
    let mut best: f64 = 0.0;
    let mut x = a.0.max(0.0);
    while x <= hi {
        best = best.max(pbx::lerp_lut(l, x) * x * std::f64::consts::TAU / 60.0);
        x += 10.0;
    }
    best
}

/// AC1 `<prefix><n>.ini` controllers, n = 0, 1, ... while present, with the
/// file's [HEADER] NAME.
fn controllers(c: &mut Ctx, prefix: &str, what: &str) -> Vec<(String, Vec<Stage>)> {
    let mut out = Vec::new();
    for n in 0.. {
        let file = format!("{prefix}{n}.ini");
        let name = c.data_text(&file).map(crate::ac1::ini::parse).and_then(|t| t.get("HEADER", "NAME").map(|s| s.trim().to_string()));
        let Some(st) = ctrl::read(c, &file, &format!("{what} {n}")) else { break };
        out.push((name.unwrap_or_else(|| format!("{n}")), st));
    }
    out
}

pub fn write(c: &mut Ctx) -> Result<()> {
    if c.data_text("ers.ini").is_none() {
        return Ok(());
    }
    let e = c.ini("ers.ini").clone();
    let base = c.base.clone();
    let mut rear = controllers(c, "ctrl_ers_", "ERS");
    let mut front = controllers(c, "ctrl_ers_front_", "ERS front");
    if rear.is_empty() {
        // AC1 refuses the car too ("Error, no ctrl_ers_*.ini controllers found")
        c.note("physics: ers.ini without ctrl_ers_<n>.ini controllers (AC1 refuses it too); no hybrid system converted");
        return Ok(());
    }
    let torque = e.get("KINETIC", "TORQUE_CURVE").map(|v| c.lut(v)).unwrap_or_default();
    let coast = e.get("KINETIC", "COAST_CURVE").map(|v| c.lut(v)).unwrap_or_default();
    let mut has_front = e.has("FRONT_MOTORS");
    if has_front && front.is_empty() {
        c.note("physics: ERS: [FRONT_MOTORS] without ctrl_ers_front_<n>.ini (AC1 refuses it); front motors not converted");
        has_front = false;
    }
    let ftorque = if has_front { e.get("FRONT_MOTORS", "TORQUE_CURVE").map(|v| c.lut(v)).unwrap_or_default() } else { Vec::new() };
    if has_front && front.len() != rear.len() {
        // AC1 and EVO both stop the car on unequal counts
        let n = front.len().min(rear.len());
        c.note(format!("physics: ERS: {} rear and {} front controllers; the first {n} of each kept (the game needs equal counts)", rear.len(), front.len()));
        rear.truncate(n);
        front.truncate(n);
    }
    // battery (module notes)
    let t_rear = e.num("KINETIC", "DISCHARGE_TIME").unwrap_or(0.0).max(0.0) / 1000.0;
    let t_front = if has_front { e.num("FRONT_MOTORS", "DISCHARGE_TIME").unwrap_or(0.0).max(0.0) / 1000.0 } else { 0.0 };
    let p_rear = peak_power(&torque, Some(c.d.limiter.max(1000.0)));
    // front x is wheel rpm: no faster than the wheels turn at the limiter in
    // top gear (the La Shush Hypo's front curve runs to 9000 rpm, 860 km/h)
    let wheel_max = c.d.gears.last().map(|g| c.d.limiter / (g * c.d.final_drive).max(0.1)).filter(|v| *v > 0.0);
    let p_front = peak_power(&ftorque, wheel_max);
    let mut kj = (p_rear * t_rear + p_front * t_front) / 1000.0;
    if kj <= 0.0 {
        kj = 1.0;
        c.note("physics: ERS: no deliverable power x DISCHARGE_TIME; battery 1 kJ");
    }
    let default = (e.num("KINETIC", "DEFAULT_CONTROLLER").unwrap_or(0.0).max(0.0) as usize).min(rear.len() - 1);
    let recovery_adjustable = e.num("COCKPIT_CONTROLS", "RECOVERY").unwrap_or(0.0) > 0.0;

    let mut m = Msg::default();
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let p_torque = format!("{base}\\data\\ers_torque.curve");
    files.push((p_torque.clone(), pbx::curve(if torque.is_empty() { &[(0.0, 0.0), (10000.0, 0.0)] } else { &torque })));
    m.set_str(2, &p_torque);
    let coast_any = coast.iter().any(|p| p.1.abs() > 0.0);
    if !coast.is_empty() {
        let p = format!("{base}\\data\\ers_coast.curve");
        files.push((p.clone(), pbx::curve(&coast)));
        m.set_str(3, &p);
    }
    f(&mut m, 4, kj);
    b(&mut m, 5, e.num("KINETIC", "HAS_BUTTON_OVERRIDE").unwrap_or(0.0) > 0.0);
    if let Some(v) = e.num("KINETIC", "MAX_KJ_PER_LAP").filter(|v| *v > 0.0) {
        f(&mut m, 6, v / 1000.0);
    }
    let heat_k = e.num("HEAT", "CHARGE_K").unwrap_or(0.0);
    if heat_k > 0.0 {
        f(&mut m, 7, 1.0);
        f(&mut m, 8, heat_k * kj);
    }
    if e.num("HEAT", "TORQUE_PERC").unwrap_or(0.0) > 0.0 {
        c.note("physics: ERS: [HEAT] TORQUE_PERC has no EVO field; not converted");
    }
    i(&mut m, 9, default as i64);
    let brake_stage = |first: &[Stage]| Stage {
        input: 1,
        add: true,
        lut: vec![(0.0, 0.0), (0.02, -1.0), (1.0, -1.0)],
        filter: first.first().map(|s| s.filter).unwrap_or(0.95),
        up: Some(1.0),
        down: Some(-1.0),
    };
    for (num, list, tag) in [(11u32, &front, "ctrl_ers_front_"), (12, &rear, "ctrl_ers_")] {
        if num == 11 && !has_front {
            continue;
        }
        for (n, (name, st)) in list.iter().enumerate() {
            let mut st = st.clone();
            if num == 12 && coast_any {
                let bs = brake_stage(&st);
                st.push(bs);
            }
            let (mut dc, fl) = ctrl::message(&st, &format!("{base}\\data\\{tag}{n}_controller"));
            dc.set_str(1, name);
            dc.fields.sort_by_key(|x| x.num);
            files.extend(fl);
            m.push(num, Val::Msg(dc));
        }
    }
    if let Some(v) = e.num("KINETIC", "BRAKE_REAR_CORRECTION").filter(|v| *v != 0.0) {
        f(&mut m, 13, v);
    }
    if e.has("COCKPIT_CONTROLS") {
        b(&mut m, 14, true);
        let mut cc = Msg::default();
        b(&mut cc, 1, e.num("COCKPIT_CONTROLS", "DELIVERY_PROFILE").unwrap_or(0.0) > 0.0);
        b(&mut cc, 2, e.num("COCKPIT_CONTROLS", "MGU_H_MODE").unwrap_or(0.0) > 0.0);
        b(&mut cc, 3, recovery_adjustable);
        m.push(15, Val::Msg(cc));
    }
    if has_front {
        b(&mut m, 16, true);
        let p = format!("{base}\\data\\ers_front_torque.curve");
        files.push((p.clone(), pbx::curve(if ftorque.is_empty() { &[(0.0, 0.0), (10000.0, 0.0)] } else { &ftorque })));
        let mut fm = Msg::default();
        fm.set_str(1, &p);
        f(&mut fm, 3, e.num("FRONT_MOTORS", "FRONT_TORQUE_VECTORING_BIAS").unwrap_or(0.0).clamp(0.0, 1.0));
        m.push(17, Val::Msg(fm));
    }
    m.fields.sort_by_key(|x| x.num);
    c.cd.set(15, Val::Msg(m));
    for (p, d) in files {
        c.put(&p, d);
    }
    let peak_t = |l: &[(f64, f64)]| l.iter().map(|p| p.1).fold(0.0, f64::max);
    c.note(format!(
        "physics: ERS: rear motor {:.0} N m / {:.0} kW ({:.1} s), front motors {} ; battery {kj:.0} kJ, {} controller(s), default {default}{}",
        peak_t(&torque), p_rear / 1000.0, t_rear,
        if has_front { format!("{:.0} N m / {:.0} kW ({:.1} s)", peak_t(&ftorque), p_front / 1000.0, t_front) } else { "none".into() },
        rear.len(), if coast_any { ", coast regen under braking" } else { "" }
    ));
    c.d.ers = Some(Ers { controllers: rear.len(), default, recovery_adjustable, battery_kj: kj, rear_kw: p_rear / 1000.0, front_kw: p_front / 1000.0 });
    Ok(())
}

/// check-car: what the game needs of an ERSData (it stops the car on the
/// first two). (errors, warnings)
pub fn check(cd: &Msg, has: &dyn Fn(&str) -> bool) -> (Vec<String>, Vec<String>) {
    let (mut err, mut warn) = (Vec::new(), Vec::new());
    let Some(e) = cd.sub(15) else { return (err, warn) };
    let nr = e.subs(12).len();
    let nf = e.subs(11).len();
    if nr == 0 {
        err.push("ERS: no rear power controllers (ERSData f12); the game stops the car".into());
    }
    if e.varint(16).unwrap_or(0) != 0 && nf != nr {
        err.push(format!("ERS: {nf} front and {nr} rear power controllers; the game needs equal counts"));
    }
    let mut paths: Vec<String> = [2u32, 3].iter().filter_map(|n| e.str(*n)).collect();
    paths.extend(e.sub(17).and_then(|m| m.str(1)));
    for dc in e.subs(11).iter().chain(e.subs(12).iter()) {
        paths.extend(dc.subs(2).iter().filter_map(|s| s.str(3)));
    }
    for p in paths {
        if !has(&p) {
            err.push(format!("ERS: {p} missing"));
        }
    }
    if pbx::rf(&e, 4).unwrap_or(0.0) <= 0.0 {
        warn.push("ERS: battery 0 kJ".into());
    }
    if (e.varint(9).unwrap_or(0) as usize) >= nr.max(1) {
        warn.push("ERS: default controller index past the last controller".into());
    }
    (err, warn)
}
