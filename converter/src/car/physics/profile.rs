//! The handling profile: what kind of car the AC1 data describes (Road,
//! Race, Drift), which decides the handling rules the physics stages apply
//! on top of the 1:1 mapping. The user may override it (`--profile`, or
//! `ACECONV_PROFILE`); `exact` turns every rule off (AC1 values 1:1).
//!
//! The rules came from in-game A/B drives of the converted AC1 911 GT3 RS
//! against Kunos's EVO 992 GT3 RS (CONVERSION-TRIAL.md, "Goal (user, 30
//! Oct)"); every number below is a stock-fleet statistic or AC1's own data,
//! never one donor car's value. The fleet numbers are measured by
//! examples/handlingsurvey.rs (tools/handling_stock.tsv) and the stock
//! `phys.wheel_lock_deg` of `aceconv stockstats`.
//!
//! | rule                          | road | race | drift | exact |
//! |-------------------------------|------|------|-------|-------|
//! | 1 EDL inactive                | yes  | yes  | yes   | no    |
//! | 2 tyre grip: one scale        | yes  | yes  | no    | no    |
//! | 3 bias from ctrl_ebb, adjust. | yes  | yes  | yes   | no    |
//! | 4 steering: lock into range   | yes  | yes  | no    | no    |
//! | 5 stock-class alignment       | yes  | yes  | no    | no    |
//! | 6 TC slip angles by level     | yes  | yes  | no    | no    |
//! | 7 roll balance into range     | yes  | yes  | no    | no    |
//! | 8 dampers adjustable          | yes  | yes  | no    | no    |
//! | 9 damping up to stock median  | yes  | no   | no    | no    |
//! | 10 stock ECU start assist     | 2000+| yes  | no    | no    |
//!
//! Rule 10 on a road car: built 2000 or later (ui_car.json "year") or with
//! traction control (engine.rs, `start_assist`).
//!
//! Not a rule: TC and ABS levels are always written in EVO's lightest-first
//! order (chassis.rs); Exact keeps AC1's starting level as the default.
//!
//! One-change test builds: `ACECONV_SKIP_RULES` (rule numbers, comma
//! separated) turns rules off on top of the profile, and the profile note
//! says so; `ACECONV_REST_POSE` (`drawn` / `ac1`) forces the rest pose
//! (suspension.rs); `ACECONV_FULL_LOCK=lsd` keeps an AC1 1.0 / 1.0 diff an
//! LSD instead of a Spool and `ACECONV_DIFF=power,coast,preload` replaces the
//! diff (drivetrain.rs); `ACECONV_TYRE_FALLOFF=ac1` gives
//! road-type tyre models AC1's falloff past the peak and
//! `ACECONV_TYRE_PEAK=ac1` AC1's peak slip angle; `ACECONV_TYRE_MZ=<scale>`
//! sets every compound's aligning-torque scale (tyres.rs);
//! `ACECONV_BODY_BOX=kunos` scales the inertia box as Kunos's twins
//! (chassis.rs); `ACECONV_PACKERS=ac1` moves the bump stop to AC1's
//! packers where they come first and `ACECONV_ANTISQUAT=ac1` tilts a driven
//! axle's wishbone pivots in side view to AC1's anti-squat (a number 0..1
//! tilts that share of the way); `ACECONV_BUMP_CAP=ac1`
//! puts the bump stop at AC1's BUMPSTOP_UP past the 0.09 m cap (suspension.rs).
//! `ACECONV_REAR_STEER=toe` stands
//! in for AC1 ctrl_4ws.ini rear steer with rear toe-in (setup.rs).
//! `ACECONV_SETUP_WINGS=off` leaves the setup's wing angles at 0 and
//! `ACECONV_TEST_AIDS` (`tc=0,abs=1,telemetry=10`) sets the stock setup's
//! aids and MoTeC laps for drive tests (setup.rs). `ACECONV_ABS_HZ` (a number or `ac1`)
//! overrides the stock 40 Hz ABS frequency (chassis.rs).

use crate::ac1::car::Car;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub enum Profile {
    #[default]
    Road,
    Race,
    Drift,
    /// every handling rule off: AC1 values 1:1
    Exact,
}

/// The user's choice: detect, or one profile.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Choice {
    #[default]
    Auto,
    Fixed(Profile),
}

impl Choice {
    pub fn parse(s: &str) -> Option<Choice> {
        Some(match s.trim().to_lowercase().as_str() {
            "" | "auto" => Choice::Auto,
            "road" => Choice::Fixed(Profile::Road),
            "race" => Choice::Fixed(Profile::Race),
            "drift" => Choice::Fixed(Profile::Drift),
            "exact" | "exact-ac1" | "exact_ac1" | "ac1" => Choice::Fixed(Profile::Exact),
            _ => return None,
        })
    }
    /// `ACECONV_PROFILE`, else auto.
    pub fn from_env() -> Choice {
        std::env::var("ACECONV_PROFILE").ok().and_then(|v| Choice::parse(&v)).unwrap_or_default()
    }
}

/// Stock road-wheel lock (STEER_LOCK / |ratio|, degrees) over the 72 stock
/// cars (stockstats `phys.wheel_lock_deg`): p90 and p75.
pub const WHEEL_LOCK_P90: f64 = 33.3;
pub const WHEEL_LOCK_P75: f64 = 31.8;
/// Stock steering lock (degrees each way) over the 71 stock cars: median
/// (p25 341, p75 495; the 992 GT3 RS 450). Rule 4 opens a lock it had to
/// cut down to this, at the same ratio: a slower ratio (14.0, the stock p25)
/// drove worse in the 30 Oct GT3 RS test than the same lock range at 12.6.
pub const STEER_LOCK_P50: f64 = 440.0;
/// Road-wheel lock above which a car is built to drift (no stock car
/// passes 39 deg; AC1 drift cars run 45-70).
const DRIFT_WHEEL_LOCK: f64 = 45.0;
/// Differential locks at or above this on power and coast = a spool.
const SPOOL: f64 = 0.9;
/// Roll balance: the front axle's share of roll stiffness ((wheel rate + 2
/// x ARB) x track^2) minus its share of the weight, over the 54 RWD stock
/// cars' stock setups: p10 and p25 (median +0.097). FWD and AWD stock cars
/// sit far lower by design (medians -0.04 / +0.02), so rule 7 judges RWD
/// cars only. Kunos's EVO Countach sits at +0.089; the AC1 one converts to
/// -0.007 and oversteers.
pub const ROLL_MARGIN_RWD_P10: f64 = 0.047;
pub const ROLL_MARGIN_RWD_P25: f64 = 0.071;
/// Damping ratio of the slow dampers, slow damper / (2 x sqrt(wheel rate x
/// sprung corner mass)), over the 46 stock setups with dampers: p25, front
/// / rear (for the `ACECONV_DAMPING=p25` test build). Kunos's EVO
/// Countach runs bump 0.92 / 0.82 and its Quattro 0.71 / 0.74, where the AC1
/// cars convert to 0.42 / 0.45 and 0.36 / 0.34 and feel floaty and nervous
/// under braking.
pub const DAMP_BUMP_P25: [f64; 2] = [0.72, 0.73];
pub const DAMP_REBOUND_P25: [f64; 2] = [0.90, 0.93];
/// The same fleet's medians: rule 9's floor. Driven on the GT3 RS (30 Oct):
/// no rule 9 brought back lift-off oversteer and nervousness, the p25
/// helped, the median was "heaps better, so close".
pub const DAMP_BUMP_P50: [f64; 2] = [0.83, 0.86];
pub const DAMP_REBOUND_P50: [f64; 2] = [1.03, 1.08];
/// The same fleet's p75 (Kunos's 992 GT3 RS: 0.90 / 1.17 front, 0.92 / 1.13
/// rear), for the `ACECONV_DAMPING=p75` test build.
pub const DAMP_BUMP_P75: [f64; 2] = [0.95, 0.96];
pub const DAMP_REBOUND_P75: [f64; 2] = [1.17, 1.17];

/// Rule 9's damping-ratio floor for an axle, [bump, rebound], and its name:
/// the stock median, or the p25 / p75 in a test build (`ACECONV_DAMPING`).
pub fn damping_floor(axle: usize) -> ([f64; 2], &'static str) {
    match std::env::var("ACECONV_DAMPING").unwrap_or_default().to_lowercase().as_str() {
        "p25" => ([DAMP_BUMP_P25[axle], DAMP_REBOUND_P25[axle]], "stock p25, TEST BUILD"),
        "p75" => ([DAMP_BUMP_P75[axle], DAMP_REBOUND_P75[axle]], "stock p75, TEST BUILD"),
        _ => ([DAMP_BUMP_P50[axle], DAMP_REBOUND_P50[axle]], "stock median"),
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Rules {
    pub edl_off: bool,
    pub tyre_balance: bool,
    pub ebb_bias: bool,
    pub steering: bool,
    pub alignment: bool,
    pub tc_angles: bool,
    pub roll_balance: bool,
    pub dampers: bool,
    pub damping: bool,
    pub start_assist: bool,
}

impl Profile {
    pub fn name(self) -> &'static str {
        match self {
            Profile::Road => "road",
            Profile::Race => "race",
            Profile::Drift => "drift",
            Profile::Exact => "exact AC1",
        }
    }
    pub fn rules(self) -> Rules {
        let grip = matches!(self, Profile::Road | Profile::Race);
        let all = self != Profile::Exact;
        let off = skipped_rules();
        let on = |n: u32, v: bool| v && !off.contains(&n);
        Rules {
            edl_off: on(1, all),
            tyre_balance: on(2, grip),
            ebb_bias: on(3, all),
            steering: on(4, grip),
            alignment: on(5, grip),
            tc_angles: on(6, grip),
            roll_balance: on(7, grip),
            dampers: on(8, grip),
            // road only: a race car's AC1 dampers are its modder's set-up
            // (the Neonstride, 1 Nov: AC1 damping drove a lot better than the
            // median floor; the GT3 RS road car, 30 Oct: the floor helped)
            damping: on(9, self == Profile::Road),
            // not drift: the launch rev limit and slip target fight a clutch kick
            start_assist: on(10, grip),
        }
    }
}

/// `ACECONV_SKIP_RULES`: rule numbers a one-change test build turns off.
pub fn skipped_rules() -> Vec<u32> {
    std::env::var("ACECONV_SKIP_RULES").unwrap_or_default().split(',').filter_map(|s| s.trim().parse().ok()).collect()
}

/// `ui_car.json` "tags" (lower case).
pub fn ui_tags(ui: &str) -> Vec<String> {
    let l = ui.to_lowercase();
    let Some(i) = l.find("\"tags\"") else { return Vec::new() };
    let rest = &l[i + 6..];
    let (Some(a), Some(b)) = (rest.find('['), rest.find(']')) else { return Vec::new() };
    if b < a {
        return Vec::new();
    }
    rest[a + 1..b].split(',').map(|t| t.trim().trim_matches('"').trim_start_matches('#').trim().to_string()).filter(|t| !t.is_empty()).collect()
}

/// The profile the AC1 data describes, with the reasons.
pub fn detect(car: &Car) -> (Profile, Vec<String>) {
    let mut why = Vec::new();
    let tags = ui_tags(&car.ui_text);
    let class = car.ui("class").unwrap_or_default().to_lowercase();
    let name = car.ui("name").unwrap_or_default().to_lowercase();
    let folder = car.folder.to_lowercase();
    let ci = car.ini("car.ini");
    let lock = ci.num("CONTROLS", "STEER_LOCK").unwrap_or(450.0);
    let ratio = ci.num("CONTROLS", "STEER_RATIO").filter(|v| v.abs() > 1.0).unwrap_or(14.0);
    let wheel = lock / ratio.abs();
    let dt = car.ini("drivetrain.ini");
    let rwd = dt.get("TRACTION", "TYPE").map(|t| t.trim().eq_ignore_ascii_case("RWD")).unwrap_or(true);
    let spool = dt.num("DIFFERENTIAL", "POWER").unwrap_or(0.0) >= SPOOL && dt.num("DIFFERENTIAL", "COAST").unwrap_or(0.0) >= SPOOL;
    let t = car.ini("tyres.ini");
    let def = t.num("COMPOUND_DEFAULT", "INDEX").unwrap_or(0.0).max(0.0) as usize;
    let sec = |ax: &str| if def == 0 { ax.to_string() } else { format!("{ax}_{def}") };
    let tyre = format!("{} {}", t.get(&sec("FRONT"), "NAME").unwrap_or(""), t.get(&sec("REAR"), "NAME").unwrap_or("")).to_lowercase();
    // drift: named so, or a spool with drift-car lock, or lock no stock car has
    if folder.contains("drift") || name.contains("drift") || class.contains("drift") || tags.iter().any(|t| t.contains("drift")) || tyre.contains("drift") {
        why.push("the folder, name, class, tags or tyres say \"drift\"".into());
    }
    if spool && rwd && wheel > WHEEL_LOCK_P90 {
        why.push(format!("RWD with a near-spool diff and {wheel:.1} deg of road-wheel lock (stock p90 {WHEEL_LOCK_P90})"));
    }
    if wheel > DRIFT_WHEEL_LOCK {
        why.push(format!("{wheel:.1} deg of road-wheel lock (above {DRIFT_WHEEL_LOCK}; no stock car passes 39)"));
    }
    if !why.is_empty() {
        return (Profile::Drift, why);
    }
    let slick = tyre.split_whitespace().any(|w| w.contains("slick") && !w.contains("semi"));
    if slick {
        why.push(format!("default compound is a slick ({})", tyre.trim()));
    }
    if class == "race" {
        why.push("ui class \"race\"".into());
    }
    if !why.is_empty() {
        return (Profile::Race, why);
    }
    why.push(format!("no drift or race marks (class \"{class}\", default tyres \"{}\")", tyre.trim()));
    (Profile::Road, why)
}

/// The profile to convert with, and the note that explains it.
pub fn resolve(car: &Car, choice: Choice) -> (Profile, String) {
    let (auto, why) = detect(car);
    let off = skipped_rules();
    let test = if off.is_empty() { String::new() } else { format!("; TEST BUILD: rule(s) {} turned off by ACECONV_SKIP_RULES", off.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(", ")) };
    match choice {
        Choice::Auto => (auto, format!("physics profile: {} (auto: {}){test}", auto.name(), why.join("; "))),
        Choice::Fixed(p) => (p, format!("physics profile: {} (chosen by the user; auto would pick {}: {}){test}", p.name(), auto.name(), why.join("; "))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags() {
        assert_eq!(ui_tags("{\"name\": \"x\", \"tags\": [\"#drift\", \"rwd\"],\n \"class\": \"street\"}"), vec!["drift", "rwd"]);
        assert!(ui_tags("{\"name\": \"x\"}").is_empty());
    }

    #[test]
    fn choice() {
        assert_eq!(Choice::parse("Auto"), Some(Choice::Auto));
        assert_eq!(Choice::parse("exact-ac1"), Some(Choice::Fixed(Profile::Exact)));
        assert_eq!(Choice::parse("drift"), Some(Choice::Fixed(Profile::Drift)));
        assert_eq!(Choice::parse("rally"), None);
    }

    #[test]
    fn rules() {
        let d = Profile::Drift.rules();
        assert!(d.edl_off && d.ebb_bias && !d.steering && !d.alignment && !d.tyre_balance);
        let e = Profile::Exact.rules();
        assert!(!e.edl_off && !e.ebb_bias && !e.steering);
        assert!(Profile::Road.rules().steering && Profile::Race.rules().alignment);
        assert!(Profile::Road.rules().roll_balance && !d.roll_balance && !e.dampers);
        assert!(Profile::Road.rules().start_assist && Profile::Race.rules().start_assist && !d.start_assist && !e.start_assist);
        assert!(Profile::Road.rules().damping && !Profile::Race.rules().damping && Profile::Race.rules().dampers);
    }
}
