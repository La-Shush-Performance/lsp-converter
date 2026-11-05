//! Handling probes (developer tool): what the converter wrote for the tyre
//! slip curve and the steering, beside the AC1 car's own values.
//!   handlingprobe <converted.kspkg> [<ac1 car dir>]
//! Per tyre file: grip (Dy0 at Fz0, lsExpY), the slip curve (peak angle,
//! use_slip_falloff, falloff level / slope / shape) and the aligning-torque
//! scale; CarData controls (ffMult, lock, ratio, linear steer rod ratio) and
//! the stock setup's steer ratio. With the AC1 dir: its tyres.ini compounds
//! (FRICTION_LIMIT_ANGLE, FALLOFF_LEVEL / SPEED, DY_REF, LS_EXPY) and
//! car.ini [CONTROLS]. Field numbers are TyresCompoundData's (modelData f4:
//! 1 Dy0, 3 lsExpY, 5 Fz0, 6 friction_limit_angle, 20 mzScale, 79
//! use_slip_falloff, 80 / 81 / 82 falloff level / slope / shape).
use aceconv::car::physics::pbx::rf;
use aceconv::io::pb::{Msg, Val};
use anyhow::{anyhow, Result};
use std::path::Path;

fn flag(m: &Msg, num: u32) -> bool {
    m.fields.iter().any(|f| f.num == num && matches!(f.val, Val::Varint(v) if v != 0))
}

fn main() -> Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let pkg = a.get(1).ok_or_else(|| anyhow!("usage: handlingprobe <converted.kspkg> [<ac1 car dir>]"))?;
    let files = aceconv::io::kspkg::read(Path::new(pkg), "")?;
    let n = |x: Option<f64>| x.map(|v| format!("{v:.4}")).unwrap_or_else(|| "-".into());
    println!("tyre\tDy0\tFz0\tlsExpY\tpeak_deg\tslip_falloff\tlevel\tslope\tshape\tmzScale");
    for (p, b) in files.iter().filter(|(p, _)| p.to_lowercase().ends_with(".tyre")) {
        let Ok(t) = Msg::parse(b) else { continue };
        let md = t.sub(4).unwrap_or_default();
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            p.rsplit('\\').next().unwrap_or(p), n(rf(&md, 1)), n(rf(&md, 5)), n(rf(&md, 3)), n(rf(&md, 6)),
            flag(&md, 79), n(rf(&md, 80)), n(rf(&md, 81)), n(rf(&md, 82)), n(rf(&md, 20))
        );
    }
    let low = |s: &str| s.to_lowercase();
    if let Some((_, b)) = files.iter().find(|(p, _)| low(p).ends_with("\\data\\cardata.car")) {
        let k = Msg::parse(b)?.sub(8).unwrap_or_default();
        println!("controls\tffMult {}\tlock {}\tratio {}\trod {}", n(rf(&k, 1)), n(rf(&k, 2)), n(rf(&k, 3)), n(rf(&k, 4)));
    }
    if let Some((_, b)) = files.iter().find(|(p, _)| low(p).ends_with(".carsetup")) {
        let mb = Msg::parse(b)?.sub(1).unwrap_or_default();
        println!("setup\tsteer ratio {}", n(rf(&mb, 2)));
    }
    if let Some(dir) = a.get(2) {
        let d = aceconv::ac1::acd::read_car_data(Path::new(dir))?;
        let t = aceconv::ac1::ini::parse(d.get("tyres.ini").map(|s| s.as_str()).unwrap_or(""));
        for s in t.names().filter(|s| s.starts_with("FRONT") || s.starts_with("REAR")) {
            let g = |k: &str| t.get(s, k).unwrap_or("-").to_string();
            println!(
                "ac1 [{s}]\t{}\tpeak {}\tfalloff {} at speed {}\tDY_REF {} at FZ0 {}\tLS_EXPY {}",
                g("NAME"), g("FRICTION_LIMIT_ANGLE"), g("FALLOFF_LEVEL"), g("FALLOFF_SPEED"), g("DY_REF"), g("FZ0"), g("LS_EXPY")
            );
        }
        let c = aceconv::ac1::ini::parse(d.get("car.ini").map(|s| s.as_str()).unwrap_or(""));
        let g = |k: &str| c.get("CONTROLS", k).unwrap_or("-").to_string();
        println!("ac1 controls\tFFMULT {}\tlock {}\tratio {}\trod {}", g("FFMULT"), g("STEER_LOCK"), g("STEER_RATIO"), g("LINEAR_STEER_ROD_RATIO"));
    }
    Ok(())
}
