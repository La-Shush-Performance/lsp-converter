//! AC1 dynamic controllers (`ctrl_*.ini`) -> EVO DynamicControllerData.
//!
//! AC1 controller sections `[CONTROLLER_n]` (INPUT, COMBINATOR ADD / MULT,
//! LUT, FILTER, UP_LIMIT, DOWN_LIMIT) map one to one onto
//! DynamicControllerStageData (1 inputVar, 2 combinatorMode 1 Add / 2 Mult,
//! 3 lut path, 4 filterGain, 5 upLimit, 6 downLimit, 8 constValue). Stock
//! Kunos cars converted that way keep AC1's file names: ks_bmw_m8_competition's
//! rear lock controller stages are `ctrl_single_lockCONTROLLER_<n>.curve`.
//! The input enum is the game's DynamicControllerStageData.Input.

use super::pbx::{self, f, i};
use super::Ctx;
use crate::io::pb::{Msg, Val};

/// AC1 controller INPUT -> DynamicControllerStageData.Input.
pub fn input_code(name: &str) -> Option<u64> {
    let n = name.trim().to_uppercase().replace(['_', ' '], "");
    Some(match n.as_str() {
        "BRAKE" => 1,
        "GAS" | "THROTTLE" => 2,
        "LATG" => 3,
        "LONG" | "LONGG" => 4,
        "STEER" => 5,
        "SPEEDKMH" | "SPEED" => 6,
        "GEAR" => 7,
        "SLIPANGLEFRONTAVERAGE" | "SLIPANGLEFRONTAVG" => 10,
        "SLIPANGLEREARAVERAGE" | "SLIPANGLEREARAVG" => 11,
        "SLIPANGLEFRONTMAX" => 12,
        "SLIPANGLEREARMAX" => 13,
        "OVERSTEERFACTOR" => 14,
        "REARSPEEDRATIO" => 15,
        "STEERDEG" => 16,
        "CONST" => 17,
        "RPMS" | "RPM" => 18,
        _ => return None,
    })
}

#[derive(Clone, Debug)]
pub struct Stage {
    pub input: u64,
    pub add: bool,
    pub lut: Vec<(f64, f64)>,
    pub filter: f64,
    pub up: Option<f64>,
    pub down: Option<f64>,
}

/// The stages of an AC1 controller file (None when the car has no such file).
/// Stages whose input has no EVO equivalent are dropped with a note.
pub fn read(c: &mut Ctx, file: &str, what: &str) -> Option<Vec<Stage>> {
    let t = c.data_text(file).map(crate::ac1::ini::parse)?;
    let mut out = Vec::new();
    for (s, _) in t.with_prefix("CONTROLLER_") {
        let input = t.get(s, "INPUT").unwrap_or("").split_whitespace().next().unwrap_or("").to_string();
        let Some(code) = input_code(&input) else {
            c.note(format!("physics: {what} controller input {input} has no EVO equivalent; stage dropped"));
            continue;
        };
        let lut = t.get(s, "LUT").map(|v| c.lut(v)).unwrap_or_default();
        if lut.is_empty() && code != 17 {
            continue;
        }
        out.push(Stage {
            input: code,
            add: !t.get(s, "COMBINATOR").unwrap_or("ADD").trim().eq_ignore_ascii_case("MULT"),
            lut,
            filter: t.num(s, "FILTER").unwrap_or(0.95),
            up: t.num(s, "UP_LIMIT"),
            down: t.num(s, "DOWN_LIMIT"),
        });
    }
    Some(out)
}

/// Scale a controller's output by `k` (ADD stage values and every limit;
/// MULT factors stay): the clamps are linear, so the output scales exactly.
pub fn scaled(stages: &[Stage], k: f64) -> Vec<Stage> {
    stages.iter().map(|s| Stage {
        lut: if s.add { s.lut.iter().map(|p| (p.0, p.1 * k)).collect() } else { s.lut.clone() },
        up: s.up.map(|v| v * k),
        down: s.down.map(|v| v * k),
        ..s.clone()
    }).collect()
}

/// Curve files of stages AC1 keys on GEAR end in this; `gear_to_speed`
/// rewrites them once the gearing and tyres are known.
const GEAR_CURVE: &str = "_gear.curve";

/// DynamicControllerData {2 stages} with one `.curve` per stage, named
/// `<prefix><n>.curve`; the curve files are returned for the caller to put.
pub fn message(stages: &[Stage], prefix: &str) -> (Msg, Vec<(String, Vec<u8>)>) {
    message_with(stages, prefix, false)
}

/// `message` for a turbo controller: a GEAR stage is written as a SPEED
/// stage (OURS), because EVO's Gear input starves converted turbo
/// controllers. The La Shush Taipan's gear boost table (1st 0.4 .. 5th+ 1.3,
/// AC1 wastegate 1.0) gave about 0 boost in 2nd and 4th at Brands Hatch Indy
/// and switched on only on some 3rd-gear runs; the same table with Kunos's
/// stage settings (filter 0.99, up limit 10000) did the same, while a
/// constant 1.0 controller and no controller pulled +0.9 .. +1.1 in every
/// gear, and the table as road speed (`gear_to_speed`) +0.6 .. +1.4 with a
/// 287 km/h top speed against 251 (8 Nov 2026). Other controllers keep the
/// Gear input: the La Shush Hypo's ERS delivery table (2nd 0.2, 3rd+ 1.0)
/// delivered 94 % in 3rd and 92 % in 4th in EVO. The stage keeps its curve
/// with gear keys under `..._gear.curve` until `gear_to_speed` rewrites it.
pub fn message_turbo(stages: &[Stage], prefix: &str) -> (Msg, Vec<(String, Vec<u8>)>) {
    message_with(stages, prefix, true)
}

fn message_with(stages: &[Stage], prefix: &str, gear_as_speed: bool) -> (Msg, Vec<(String, Vec<u8>)>) {
    let mut dc = Msg::default();
    let mut files = Vec::new();
    for (j, s) in stages.iter().enumerate() {
        let gear = gear_as_speed && s.input == 7;
        let p = if gear { format!("{prefix}{j}{GEAR_CURVE}") } else { format!("{prefix}{j}.curve") };
        let mut st = Msg::default();
        i(&mut st, 1, if gear { 6 } else { s.input as i64 });
        i(&mut st, 2, if s.add { 1 } else { 2 });
        st.set_str(3, &p);
        f(&mut st, 4, s.filter);
        pbx::fo(&mut st, 5, s.up);
        pbx::fo(&mut st, 6, s.down);
        if s.input == 17 {
            f(&mut st, 8, s.lut.first().map(|p| p.1).unwrap_or(0.0));
        }
        dc.push(2, Val::Msg(st));
        files.push((p, pbx::curve(&s.lut)));
    }
    (dc, files)
}

/// Turn every gear-keyed turbo stage curve (`message_turbo`) into road speed (km/h):
/// gear k's value holds from the speed where gear k-1 reaches the upshift
/// rpm to where gear k does, so a car shifted near the top of each gear
/// gets AC1's value in every gear. R / N share 1st's speeds and take 1st's
/// value. Runs after the drivetrain and tyre stages.
pub fn gear_to_speed(c: &mut Ctx) {
    let paths: Vec<String> = c.files.under(&format!("{}\\data", c.base)).into_iter().filter(|p| p.to_lowercase().ends_with(GEAR_CURVE)).collect();
    if paths.is_empty() {
        return;
    }
    let d = &c.d;
    let r = match d.traction {
        1 => d.tyre_radius[0],
        2 => (d.tyre_radius[0] + d.tyre_radius[1]) / 2.0,
        _ => d.tyre_radius[1],
    }
    .max(0.15);
    let up = if d.shift_up_rpm > 0.0 { d.shift_up_rpm } else { d.limiter };
    // km/h at the upshift rpm in each forward gear
    let top: Vec<f64> = d.gears.iter().filter(|g| **g > 0.0).map(|g| up / 60.0 * 2.0 * std::f64::consts::PI * r / (g * d.final_drive.max(0.1)) * 3.6).collect();
    if top.is_empty() {
        c.note("physics: gear-keyed controller stages kept as speed stages without gearing - left as written");
        return;
    }
    for p in paths {
        let gear_lut = c.files.get(&p).map(|b| pbx::read_curve(b)).unwrap_or_default();
        if gear_lut.is_empty() {
            continue;
        }
        let at = |k: usize| pbx::lerp_lut(&gear_lut, k as f64);
        let mut lut = vec![(0.0, at(1))];
        for k in 1..top.len() {
            let v = top[k - 1];
            lut.push((v - 1.0, at(k)));
            lut.push((v, at(k + 1)));
        }
        lut.push((top[top.len() - 1].max(lut.last().unwrap().0 + 1.0), at(top.len())));
        lut.push((1000.0, at(top.len())));
        let name = p.rsplit('\\').next().unwrap_or(&p).to_string();
        c.note(format!(
            "physics: controller {name}: AC1 gear table -> road speed (EVO's Gear input starves converted turbo controllers); shifts at {} km/h",
            top.iter().map(|v| format!("{v:.0}")).collect::<Vec<_>>().join(" / ")
        ));
        c.put(&p, pbx::curve(&lut));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gear_stage() -> Vec<Stage> {
        vec![Stage { input: 7, add: true, lut: vec![(0.0, 0.0), (1.0, 0.4), (5.0, 1.3)], filter: 0.9, up: Some(7.0), down: Some(0.0) }]
    }

    #[test]
    fn turbo_gear_stage_becomes_speed() {
        let (m, files) = message_turbo(&gear_stage(), "x/ctrl_turbo0_controller");
        let st = &m.subs(2)[0];
        assert_eq!(st.varint(1), Some(6));
        assert!(st.str(3).unwrap().ends_with(GEAR_CURVE));
        assert!(files[0].0.ends_with(GEAR_CURVE));
    }

    #[test]
    fn other_gear_stage_keeps_gear() {
        let (m, files) = message(&gear_stage(), "x/ctrl_ers_0_controller");
        let st = &m.subs(2)[0];
        assert_eq!(st.varint(1), Some(7));
        assert!(!files[0].0.ends_with(GEAR_CURVE));
    }
}
