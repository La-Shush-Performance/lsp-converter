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

/// DynamicControllerData {2 stages} with one `.curve` per stage, named
/// `<prefix><n>.curve`; the curve files are returned for the caller to put.
pub fn message(stages: &[Stage], prefix: &str) -> (Msg, Vec<(String, Vec<u8>)>) {
    let mut dc = Msg::default();
    let mut files = Vec::new();
    for (j, s) in stages.iter().enumerate() {
        let p = format!("{prefix}{j}.curve");
        let mut st = Msg::default();
        i(&mut st, 1, s.input as i64);
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

/// The inputs an engine's turbo and wastegate controllers really get in
/// EVO: Gas, Const and RPMS. The game binds every controller's inputs to
/// the car when it builds it (exe 0x1423EC0B0), but the engine's turbo
/// controller loader (0x14237C160) ends in its max-power sweep (0x14237C8B0,
/// which calls 0x14237A2B0 per rpm), and that sweep copies its own input
/// table over the input pointers of the engine's stored turbo controllers
/// and never puts them back. Only RPMS and Gas point at live engine state
/// (engine +0x424 / +0x414, refreshed every step); every other input points
/// into the sweep's dead stack frame (Gear at a slot that held 1, the rest
/// at zeros, later whatever the stack holds). Every stock turbo controller
/// is an RPMS stage (the Ferrari 296 GTB adds a Gear stage). Driven: on
/// 8 Nov 2026 the La Shush Taipan's AC1 gear table (1st 0.4 .. 5th+ 1.3)
/// gave next to no boost in some gears while a Const 1.0 stage pulled full
/// boost; on 9 Nov the same table rewritten as road speed held 0.4 bar all
/// lap (its value at 0 km/h), and with the stage fixed (`turbo_fixed`) the
/// gauge read 1.0 bar in every gear and the top speed rose from 290 to
/// 311 km/h (AC1 316). ERS, differential and other controllers are built
/// apart from the engine and keep every input.
pub const TURBO_LIVE: [u64; 3] = [2, 17, 18];

/// The constant a turbo controller stage on an input EVO does not feed is
/// fixed at (OURS; None for a live input): a Gear stage at the top forward
/// gear (AC1's controllers read gear k as k, 1 = first), a Speed stage at
/// the car's top speed, any other input at 0 (straight line, no brake, no
/// steering). EVO caps the boost at the wastegate as AC1 does (boost =
/// target x spool, at most WASTEGATE x the boost level: exe 0x1424B1200),
/// so a gear table reaching past the wastegate in top gear runs at the
/// wastegate in every gear: the La Shush Taipan gets AC1's 1.0 bar from
/// 4th up, and more than AC1's 0.4 / 0.6 / 0.8 in 1st to 3rd.
pub fn turbo_fixed(s: &Stage, top_gear: f64, top_kmh: f64) -> Option<f64> {
    if TURBO_LIVE.contains(&s.input) {
        return None;
    }
    let x = match s.input {
        7 => top_gear,
        6 => top_kmh,
        _ => 0.0,
    };
    Some(pbx::lerp_lut(&s.lut, x))
}

/// Rewrite every turbo / wastegate controller stage of the engine (CarData
/// f4 f25 / f26) whose input EVO does not feed as a Const stage at
/// `turbo_fixed`'s value. Runs after the drivetrain and tyre stages (top
/// gear and top speed: the limiter in top gear on the driven tyres).
pub fn fix_turbo_inputs(c: &mut Ctx) {
    let d = &c.d;
    let r = match d.traction {
        1 => d.tyre_radius[0],
        2 => (d.tyre_radius[0] + d.tyre_radius[1]) / 2.0,
        _ => d.tyre_radius[1],
    }
    .max(0.15);
    let top_gear = d.gears.iter().filter(|g| **g > 0.0).count() as f64;
    let top_kmh = d.gears.iter().filter(|g| **g > 0.0).last()
        .map(|g| d.limiter / 60.0 * 2.0 * std::f64::consts::PI * r / (g * d.final_drive.max(0.1)) * 3.6)
        .unwrap_or(300.0);
    let names = |n: u64| match n {
        1 => "Brake", 3 => "LatG", 4 => "LonG", 5 => "Steer", 6 => "Speed", 7 => "Gear", 14 => "OversteerFactor", 16 => "SteerDEG",
        _ => "input",
    };
    let mut notes = Vec::new();
    let Some(ei) = c.cd.idx(4).first().copied() else { return };
    let files = &c.files;
    let Ok(e) = c.cd.open(ei) else { return };
    for field in [25u32, 26] {
        for (k, ci) in e.idx(field).into_iter().enumerate() {
            let Ok(dc) = e.open(ci) else { continue };
            for si in dc.idx(2) {
                let Ok(st) = dc.open(si) else { continue };
                let input = st.varint(1).unwrap_or(0);
                let lut = st.str(3).and_then(|p| files.find_ci(&p).and_then(|key| files.get(&key).map(|b| pbx::read_curve(b)))).unwrap_or_default();
                let stage = Stage { input, add: st.varint(2) != Some(2), lut, filter: 0.0, up: None, down: None };
                let Some(v) = turbo_fixed(&stage, top_gear, top_kmh) else { continue };
                st.set_varint(1, 17);
                f(st, 8, v);
                st.remove(3);
                st.fields.sort_by_key(|x| x.num);
                let at = match input { 7 => format!("top gear {top_gear:.0}"), 6 => format!("top speed {top_kmh:.0} km/h"), _ => "0".to_string() };
                notes.push(format!(
                    "physics: {} controller {k}: {} stage fixed at {v:.2} ({at}); EVO's turbo controllers see only rpm and throttle",
                    if field == 25 { "turbo" } else { "wastegate" }, names(input)
                ));
            }
        }
    }
    for n in notes {
        c.note(n);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stage(input: u64, lut: Vec<(f64, f64)>) -> Stage {
        Stage { input, add: true, lut, filter: 0.9, up: Some(7.0), down: Some(0.0) }
    }

    #[test]
    fn turbo_gear_stage_is_fixed_at_top_gear() {
        let s = stage(7, vec![(0.0, 0.0), (1.0, 0.4), (2.0, 0.6), (3.0, 0.8), (4.0, 1.0), (5.0, 1.3), (7.0, 1.3)]);
        assert_eq!(turbo_fixed(&s, 7.0, 320.0), Some(1.3));
        assert_eq!(turbo_fixed(&s, 4.0, 320.0), Some(1.0));
    }

    #[test]
    fn turbo_speed_stage_is_fixed_at_top_speed_others_at_zero() {
        let s = stage(6, vec![(0.0, 0.4), (200.0, 1.0), (400.0, 2.0)]);
        assert_eq!(turbo_fixed(&s, 6.0, 300.0), Some(1.5));
        let steer = stage(5, vec![(-1.0, 0.5), (0.0, 1.0), (1.0, 0.5)]);
        assert_eq!(turbo_fixed(&steer, 6.0, 300.0), Some(1.0));
    }

    #[test]
    fn turbo_rpm_gas_const_stay_live() {
        for input in [2, 17, 18] {
            assert_eq!(turbo_fixed(&stage(input, vec![(0.0, 1.0), (9000.0, 2.0)]), 6.0, 300.0), None);
        }
    }

    #[test]
    fn controllers_keep_their_inputs() {
        let (m, files) = message(&[stage(7, vec![(0.0, 0.0), (1.0, 0.4)])], "x/ctrl_ers_0_controller");
        let st = &m.subs(2)[0];
        assert_eq!(st.varint(1), Some(7));
        assert!(files[0].0.ends_with("controller0.curve"));
    }
}
