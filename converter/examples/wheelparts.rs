//! List every kn5 part under an AC1 car's wheels with the kind the
//! converter files it as (`materials::kind_of`: tyre, rim, rim blur, disc)
//! and its size in the wheel frame (radius and axial range), next to
//! tyres.ini's RADIUS / RIM_RADIUS / WIDTH and the generated tyre profile.
//! A tyre filed as a rim or rim blur ends up drawn beside the generated tyre;
//! a profile point near the centre plane at mid-sidewall lathes into a groove
//! all round the tyre ("GROOVE").
//!
//! cargo run --release --example wheelparts -- [--brief] <ac1 car dir>...
//! (--brief: one line per axle, the groove check only)

use aceconv::ac1::car::Car;
use aceconv::car::frame::{self, Frame};
use aceconv::car::{geometry, materials, wheels};

/// A profile point between the lower sidewall and the shoulder closer to the
/// centre plane than half the tyre's half width.
fn groove(prof: &[(f64, f64)]) -> Option<(f64, f64)> {
    let bead = prof.iter().map(|p| p.1).fold(f64::MAX, f64::min);
    let top = prof.iter().map(|p| p.1).fold(0.0, f64::max);
    let half = prof.iter().map(|p| p.0.abs()).fold(0.0, f64::max);
    prof.iter().copied().find(|&(a, r)| r > bead + 0.25 * (top - bead) && r < top - 0.1 * (top - bead) && a.abs() < 0.5 * half)
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let brief = args.iter().any(|a| a == "--brief");
    for dir in args.iter().filter(|a| !a.starts_with("--")) {
        let Ok(car) = Car::open(std::path::Path::new(dir)) else { continue };
        let Ok(k) = car.kn5() else { continue };
        let f = Frame::build(&car, &k);
        let mut notes = Vec::new();
        let insts = geometry::flatten(&k, &mut notes);
        let h = materials::Hints::default();
        let name = std::path::Path::new(dir).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        if !brief {
            println!("== {name}");
            for (ax, t) in f.tyres.iter().enumerate() {
                println!("  tyres.ini {}: RADIUS {:.4} RIM_RADIUS {:.4} WIDTH {:.4}", if ax == 0 { "front" } else { "rear" }, t.radius, t.rim_radius, t.width);
            }
        }
        for c in ["LF", "LR"] {
            let mut tyre_pts = Vec::new();
            for i in insts.iter().filter(|i| i.corner == Some(c)) {
                let m = &k.materials[i.material];
                let kind = materials::kind_of(m, &i.node, true, i.blur, false, &h);
                let w = f.evo_m(&i.world);
                let (mut rmin, mut rmax, mut xmin, mut xmax) = (f64::MAX, 0.0f64, f64::MAX, f64::MIN);
                for v in 0..i.mesh.vertex_count {
                    let p = i.mesh.position(v);
                    let q = f.to_wheel(c, frame::xform(&w, [p[0] as f64, p[1] as f64, p[2] as f64]));
                    let r = (q[1] * q[1] + q[2] * q[2]).sqrt();
                    rmin = rmin.min(r);
                    rmax = rmax.max(r);
                    xmin = xmin.min(q[0]);
                    xmax = xmax.max(q[0]);
                    if kind == materials::Kind::Tyre {
                        tyre_pts.push((q[0], r));
                    }
                }
                if !brief {
                    println!("  {c} {:<8} r {:.4} .. {:.4}  x {:+.4} .. {:+.4}  blur={:<5} node={} | {} | {}", format!("{kind:?}"), rmin, rmax, xmin, xmax, i.blur, i.node, m.name, m.shader);
                }
            }
            if tyre_pts.len() < 50 {
                println!("  {name} {c}: no tyre mesh (stock profile)");
                continue;
            }
            let t = f.tyre(c);
            let new = wheels::profile(&tyre_pts, t.radius, t.width, t.rim_radius);
            let old = old_profile(&tyre_pts, t.radius, t.width, t.rim_radius);
            let show = |g: Option<(f64, f64)>| g.map(|(a, r)| format!("GROOVE at ({a:.3}, {r:.3})")).unwrap_or_else(|| "ok".into());
            println!("  {name} {c}: before {} | after {}", show(groove(&old)), show(groove(&new)));
            if !brief {
                let half = |p: &[(f64, f64)]| p.iter().take(p.len() / 2 + 1).map(|q| format!("({:.3}, {:.4})", q.0, q.1)).collect::<Vec<_>>().join(" ");
                println!("    before: {}\n    after:  {}", half(&old), half(&new));
            }
        }
    }
    Ok(())
}

// The profile code as it was before the convex-outline fix (git HEAD), for
// the before / after comparison.
fn old_profile(pts: &[(f64, f64)], radius: f64, width: f64, rim_radius: f64) -> Vec<(f64, f64)> {
    // stock profile (half, from the bead up to the tread centre), normalised:
    // x / half width, (r - bead r) / (R - bead r)
    const STOCK: [(f64, f64); 10] = [(0.877, 0.0), (0.98, 0.15), (1.0, 0.186), (0.997, 0.22), (0.975, 0.67), (0.929, 0.78), (0.866, 0.87), (0.772, 0.92), (0.673, 0.96), (0.0, 1.0)];
    let half = width / 2.0;
    let bead = rim_radius.min(radius - 0.02);
    let fallback = || -> Vec<(f64, f64)> { STOCK.iter().map(|(x, r)| (x * half, bead + r * (radius - bead))).collect() };
    if pts.len() < 50 {
        return old_mirror(&fallback());
    }
    let rmax = pts.iter().map(|p| p.1).fold(0.0, f64::max);
    let amax = pts.iter().map(|p| p.0.abs()).fold(0.0, f64::max);
    if rmax < 0.1 || amax < 0.02 {
        return old_mirror(&fallback());
    }
    // envelope around a centre on the profile's mid radius
    let rc = (bead.max(rmax * 0.6) + rmax) / 2.0;
    const N: usize = 48;
    let mut best = vec![None::<(f64, f64, f64)>; N + 1];
    for &(a, r) in pts {
        let (a, r) = (a.abs(), r);
        if r < bead * 0.98 {
            continue;
        }
        let (dx, dy) = (a, r - rc);
        let phi = dy.atan2(dx); // -pi/2 (down) .. pi/2 (up, tread centre)
        let t = ((phi + std::f64::consts::FRAC_PI_2) / std::f64::consts::PI * N as f64).round();
        if !(0.0..=N as f64).contains(&t) {
            continue;
        }
        // below the centre only the sidewall counts (not the inner liner)
        if phi < 0.0 && a < amax * 0.6 {
            continue;
        }
        let d = (dx * dx + dy * dy).sqrt();
        let slot = &mut best[t as usize];
        if slot.map(|s| d > s.0).unwrap_or(true) {
            *slot = Some((d, a, r));
        }
    }
    let mut ordered: Vec<(f64, f64)> = best.iter().flatten().map(|&(_, a, r)| (a, r)).collect();
    if ordered.len() < 6 {
        return old_mirror(&fallback());
    }
    // the tread centre point at a = 0
    let top = ordered.last().copied().unwrap_or((0.0, rmax));
    if top.0 > 1e-4 {
        ordered.push((0.0, top.1.max(rmax)));
    }
    // scale to tyres.ini
    let amax2 = ordered.iter().map(|p| p.0).fold(0.0, f64::max).max(1e-6);
    let rmax2 = ordered.iter().map(|p| p.1).fold(0.0, f64::max).max(1e-6);
    let scaled: Vec<(f64, f64)> = ordered.iter().map(|&(a, r)| (a * half / amax2, r * radius / rmax2)).collect();
    // thin to about 10 points (the stock tyre has 19 for both halves)
    let mut thin = vec![scaled[0]];
    let step = (scaled.len() as f64 / 9.0).max(1.0);
    let mut t = step;
    while (t as usize) < scaled.len() - 1 {
        thin.push(scaled[t as usize]);
        t += step;
    }
    thin.push(*scaled.last().unwrap());
    old_mirror(&thin)
}
fn old_mirror(h: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut out: Vec<(f64, f64)> = h.to_vec();
    for p in h.iter().rev().skip(1) {
        out.push((-p.0, p.1));
    }
    out
}
