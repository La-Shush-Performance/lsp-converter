//! Aero: AC1 aero.ini wings and fins -> EVO WingData (CarData f12.f3 /
//! f12.f4 and one `.wing` file per wing).
//!
//! Same vocabulary on both sides (ks_mazda_mx5_na against ks_mazda_miata):
//! NAME, CHORD, SPAN, POSITION (from the CoG), LUT_AOA_CL / LUT_AOA_CD (AoA
//! degrees -> coefficient, positive CL = downforce), CL_GAIN / CD_GAIN,
//! ANGLE; the four damage values per coefficient are ZONE_FRONT / REAR /
//! LEFT / RIGHT. Ground-height LUTs (LUT_GH_*) become `lutGH_*_mult`
//! unchanged: both games look them up at the wing's own height. AC1
//! (acs.exe `Wing::step`) puts POSITION on the body and asks
//! `RaceEngineer::getPointGroundHeight`, the mean height over the planes
//! through the tyre contact patches (not the car.ini [RIDE] pickups); the
//! La Shush Taipan's EVO log (3 Nov) fits that reading to 2 % rms in
//! AERO_BALANCE (the pickups miss by 42 %), and Kunos copied the 190E's
//! front-wing table and POSITION 1:1. A car whose wings run on the steep part
//! of a table in EVO but not in AC1 sits differently: see suspension.rs,
//! heave springs.
//! AC1 wing controllers (DYNAMIC_CONTROLLER) and adjustable wing angles in
//! the setup are not converted (a note says so).

use anyhow::Result;

use super::pbx::{self, f};
use super::{ac1_v3, Ctx};
use crate::io::pb::{Msg, Val};

pub fn write(c: &mut Ctx) -> Result<()> {
    let a = c.ini("aero.ini").clone();
    let tmpl = c.cd.sub(12).and_then(|x| x.subs(3).into_iter().next()).unwrap_or_default();
    let mut wings: Vec<(Msg, bool)> = Vec::new();
    let mut curves: Vec<(String, Vec<u8>)> = Vec::new();
    let (mut cl_area, mut cd_area) = (0.0, 0.0);
    let mut n = 0;
    for (prefix, vertical) in [("WING_", false), ("FIN_", true)] {
        for (sec, _) in a.with_prefix(prefix).map(|(s, m)| (s.to_string(), m.clone())).collect::<Vec<_>>() {
            let mut w = tmpl.clone();
            for k in [1u32, 2, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17] {
                w.remove(k);
            }
            pbx::b(&mut w, 1, vertical);
            w.set_str(2, a.get(&sec, "NAME").unwrap_or(&sec));
            let chord = a.num(&sec, "CHORD").unwrap_or(1.0);
            let span = a.num(&sec, "SPAN").unwrap_or(1.0);
            f(&mut w, 3, chord);
            f(&mut w, 4, span);
            w.set(6, pbx::v3(ac1_v3(&a, &sec, "POSITION").unwrap_or([0.0; 3])));
            let angle = a.num(&sec, "ANGLE").unwrap_or(0.0);
            let cl_gain = a.num(&sec, "CL_GAIN").unwrap_or(0.0);
            let cd_gain = a.num(&sec, "CD_GAIN").unwrap_or(0.0);
            let mut cl_at = 0.0;
            let mut cd_at = 0.0;
            for (key, field, lo) in [("LUT_AOA_CL", 7u32, "aoa_cl"), ("LUT_AOA_CD", 8, "aoa_cd"), ("LUT_GH_CL", 9, "gh_cl"), ("LUT_GH_CD", 10, "gh_cd")] {
                let named = a.get(&sec, key).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
                let mut l = named.as_deref().map(|v| c.lut(v)).unwrap_or_default();
                if l.is_empty() {
                    // OURS: EVO loads both angle-of-attack curves of every wing and fin (every
                    // stock one has both; the ground-height pair is optional). The Formula
                    // Truck Volvo FH names `front_wind_AOA_CD.lut` for its front_wing table:
                    // left out, the empty path crashed EVO's physics thread ("Trying to load
                    // a message with an empty path") whenever the car loaded. AC1 logs "Lut
                    // file not found" and reads the missing table as 0: a flat zero curve.
                    if field > 8 {
                        continue;
                    }
                    let what = match &named {
                        Some(v) => format!("{key} {v} is missing or empty"),
                        None => format!("names no {key}"),
                    };
                    c.note(format!("physics: aero.ini {sec} ({}) {what} - a zero curve, as AC1 reads it (the wing adds no {})",
                        a.get(&sec, "NAME").unwrap_or(&sec), if field == 7 { "lift" } else { "drag" }));
                    l = vec![(-90.0, 0.0), (90.0, 0.0)];
                }
                if field == 7 {
                    cl_at = pbx::lerp_lut(&l, angle);
                }
                if field == 8 {
                    cd_at = pbx::lerp_lut(&l, angle);
                }
                let p = format!("{}\\data\\wing_{n}_{lo}.curve", c.base);
                w.set_str(field, &p);
                curves.push((p, pbx::curve(&l)));
            }
            if cl_gain != 0.0 {
                f(&mut w, 12, cl_gain);
            }
            if cd_gain != 0.0 {
                f(&mut w, 11, cd_gain);
            }
            if angle != 0.0 {
                f(&mut w, 13, angle);
            }
            if vertical {
                if let Some(y) = a.num(&sec, "YAW_CL_GAIN") {
                    f(&mut w, 14, y);
                }
            }
            for (field, kind) in [(15u32, "CD"), (16, "CL")] {
                for z in ["FRONT", "REAR", "LEFT", "RIGHT"] {
                    w.push(field, Val::F32((a.num(&sec, &format!("ZONE_{z}_{kind}")).unwrap_or(0.0) as f32).to_le_bytes()));
                }
            }
            if !vertical {
                cl_area += cl_at * cl_gain * chord * span;
                cd_area += cd_at * cd_gain * chord * span;
            }
            w.fields.sort_by_key(|x| x.num);
            wings.push((w, vertical));
            n += 1;
        }
    }
    if a.sections.keys().any(|k| k.starts_with("DYNAMIC_CONTROLLER")) || a.has("WING_ANIMATIONS") {
        c.note("physics: AC1 dynamic wing controllers are not converted (fixed angles)");
    }
    if wings.iter().all(|w| w.1) {
        c.note("physics: aero.ini has no wings; the car has no drag (top speed unlimited by aero)");
    }
    c.d.cl_area = cl_area;
    c.d.cd_area = cd_area;
    for (p, b) in curves {
        c.put(&p, b);
    }
    let base = c.base.clone();
    let id = c.id.clone();
    let ae = pbx::at(&mut c.cd, &[12])?;
    ae.remove(3);
    ae.remove(4);
    ae.remove(5);
    ae.remove(10);
    let mut k = 0;
    for (w, vertical) in wings {
        if vertical {
            ae.push(4, Val::Msg(w));
        } else {
            ae.push(3, Val::Msg(w));
            ae.push(10, Val::Bytes(format!("{base}\\data\\{id}{k}.wing").into_bytes()));
            k += 1;
        }
    }
    ae.fields.sort_by_key(|x| x.num);
    c.note(format!("physics: aero {k} wings, CL.A {cl_area:+.3} m2, CD.A {cd_area:.3} m2"));
    Ok(())
}
