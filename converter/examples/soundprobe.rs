//! Car sound probes (developer tool).
//!   soundprobe params <ac1 car dir>...
//!       the AC1 bank's parameters, and what places, gates and automates each engine layer
//!   soundprobe grid <bank dir | stock bank name> <event path> [rpm,...] [throttle,...] [name=value ...]
//!       RMS (dBFS) of an EVO event over an rpm x throttle grid, through the game's runtime. The bank
//!       dir holds master_bank(.strings).bank as `aceconv sound` writes them; a name (`ks_bmw_m4_gt3`)
//!       is a stock bank from content.kspkg. The game's master banks load first.
//!   soundprobe ac1grid <ac1 car dir> <event name> [rpm,...] [throttle,...] [name=value ...]
//!       the same for the AC1 event in Assetto Corsa's own runtime (fmodstudio64.dll 1.08)
//!   soundprobe voltest <ac1 car dir> <event name> <instrument guid | all> <delta> <rpm> <throttle>
//!       one AC1 cell with instrument volume fields moved by delta (the bank patched in memory)
//!   soundprobe fsb <bank>
//!       FSB5 sample headers (chunk types and payloads) with the bank's sample names
//! Parameters are set without their seek speed and held; each cell renders SECS seconds and the
//! RMS skips the first SKIP (env SOUNDPROBE_SECS / SOUNDPROBE_SKIP / SOUNDPROBE_DISTANCE, metres
//! between the listener and the event, default 1 for `*_int` events, else 6).
use aceconv::sound::bankread::{Effect, Instrument, Prop};
use aceconv::sound::bankwrite::prj::Guid;
use aceconv::sound::{ac1bank, map, mkfmod};
use anyhow::{anyhow, bail, Result};
use std::collections::BTreeSet;
use std::ffi::{c_char, c_int, c_uint, c_void, CString};
use std::path::{Path, PathBuf};

/// (parameter, tag, lo, hi) terms of a trigger code.
fn terms(code: &[u8]) -> Vec<(Guid, u32, f32, f32)> {
    let mut v = Vec::new();
    let mut i = 0;
    while i + 4 <= code.len() {
        let t = u32::from_le_bytes(code[i..i + 4].try_into().unwrap());
        i += 4;
        if t == 0x11 && i + 28 <= code.len() {
            let f = |k: usize| f32::from_le_bytes(code[i + k..i + k + 4].try_into().unwrap());
            v.push((Guid(code[i..i + 16].try_into().unwrap()), u32::from_le_bytes(code[i + 16..i + 20].try_into().unwrap()), f(20), f(24)));
            i += 28;
        }
    }
    v
}

fn props(a: &ac1bank::Ac1Bank, pn: &dyn Fn(&Guid) -> String, ps: &[Prop], what: &str) {
    for p in ps {
        for c in &p.curves {
            let Some(cv) = a.bank.curves.get(c) else { continue };
            let by = a.curve_param.get(c).map(pn).unwrap_or_else(|| "?".into());
            let pts: Vec<String> = cv.pts.iter().map(|q| {
                let x = f32::from_bits(q.0);
                if p.index == 0 { format!("({x:.3}, {:.1} dB)", ac1bank::fader_db(q.1)) } else { format!("({x:.3}, {:.3})", q.1) }
            }).collect();
            println!("      {what} prop {} by {by}: {}", p.index, pts.join(" "));
        }
        if !p.curves.is_empty() {
            let m = a.bank.maps.get(&p.map).map(|m| m.iter().map(|(x, y)| format!("({x}, {y})")).collect::<Vec<_>>().join(" ")).unwrap_or_else(|| "none".into());
            println!("      {what} prop {} mapping {} a {} b {}: {m}", p.index, p.map, p.a, p.b);
        }
        if !p.mods.is_empty() {
            println!("      {what} prop {} modulators {}", p.index, p.mods.len());
        }
    }
}

fn params(car: &Path) -> Result<()> {
    let s = map::open_source(car)?;
    let a = &s.ac1;
    println!("== {} ({})", car.display(), s.bank_path.display());
    let pn = |g: &Guid| a.bank.params.get(g).map(|p| p.name.clone()).unwrap_or_else(|| if a.bank.timelines.contains_key(g) { "timeline".into() } else { format!("?{g}") });
    for p in a.bank.params.values() {
        let seek = a.seek.get(&p.id).map(|m| format!(" seek {:?}", m.data.chunks_exact(4).skip(1).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect::<Vec<_>>())).unwrap_or_default();
        println!("  param {} type {} [{}..{}] init {}{seek} {}", p.name, p.ptype, p.min, p.max, p.init, p.id);
    }
    let mut evs: Vec<(&String, &Guid)> = s.paths.iter().map(|(p, g)| (p, g)).collect();
    evs.sort();
    for (path, ev) in evs {
        let leaf = path.rsplit('/').next().unwrap_or("");
        if !["engine_int", "engine_ext", "limiter", "turbo", "transmission"].contains(&leaf) {
            continue;
        }
        let Some(e) = a.bank.events.get(ev) else { continue };
        println!("  -- {path}: parameters {}", e.pinst.iter().map(|p| pn(&p.xor(ev))).collect::<Vec<_>>().join(", "));
        let mut buses: BTreeSet<Guid> = e.buses.iter().copied().collect();
        buses.insert(e.master_bus);
        buses.insert(e.mixer_input);
        for b in &buses {
            let Some(bus) = a.bank.buses.get(b) else { continue };
            let role = if *b == e.master_bus { "master" } else if *b == e.mixer_input { "input" } else { "group" };
            println!("    bus {role} {b} -> {} vol {:.1} dB", bus.output, bus.volume);
            props(a, &pn, &bus.props, "bus");
            for (k, fx) in bus.pre.iter().map(|x| ("pre", x)).chain(bus.post.iter().map(|x| ("post", x))) {
                let Some(x) = a.bank.effects.get(fx) else { continue };
                let pv = |ps: &[aceconv::sound::bankread::PVal]| ps.iter().map(|p| format!("{p:?}")).collect::<Vec<_>>().join(" ");
                match x {
                    Effect::Builtin { dsp, params, .. } => println!("      {k} effect dsp {dsp}: {}", pv(params)),
                    Effect::Plugin { ident, params, .. } => println!("      {k} effect {ident}: {}", pv(params)),
                    Effect::Send { ret, level, .. } => println!("      {k} send -> {ret} level {level}"),
                }
                props(a, &pn, x.props(), "effect");
            }
        }
        let mut top: Vec<Guid> = a.event_insts.get(ev).cloned().unwrap_or_default();
        for i in a.bank.instruments.values() {
            if buses.contains(&i.inst().bus) && !top.contains(&i.id()) {
                top.push(i.id());
            }
        }
        let mut todo: Vec<(Guid, usize)> = top.iter().map(|i| (*i, 0)).collect();
        let mut seen = BTreeSet::new();
        while let Some((i, depth)) = todo.pop() {
            if !seen.insert(i) {
                continue;
            }
            let Some(it) = a.bank.instruments.get(&i) else {
                println!("    {}inst {i}: not in the bank", "  ".repeat(depth));
                continue;
            };
            let ins = it.inst();
            let kind = match it {
                Instrument::Wave { .. } => "wave".to_string(),
                Instrument::Multi { sounds, .. } => format!("multi({})", sounds.len()),
                Instrument::Scatter { .. } => "scatter".to_string(),
            };
            let sheet = a.sheet.get(&i).map(|(gp, lo, len)| format!(" sheet {} [{lo}..{}]", pn(gp), lo + len)).unwrap_or_else(|| " timeline".into());
            let cond: Vec<String> = a.cond.get(&i).map(|c| terms(c)).unwrap_or_default().iter().map(|(gp, tag, lo, hi)| format!("{} {tag:#x} [{lo}..{hi}]", pn(gp))).collect();
            let ap = if ins.ap_param.is_zero() { String::new() } else { format!(" autopitch {} root {} min {}", pn(&ins.ap_param), ins.ap_root, ins.ap_min) };
            let lays: Vec<String> = a.sheets.get(&i).map(|v| v.iter().map(|(gp, s, l)| format!("{} [{s}..{}]", pn(gp), s + l)).collect()).unwrap_or_default();
            let audio = match it {
                Instrument::Wave { audio, .. } => a.bank.waves.get(audio).and_then(|ix| a.bank.samples.get(*ix as usize)).map(|s| format!(" sample [{}]", s.name)).unwrap_or_default(),
                _ => String::new(),
            };
            println!("    {}inst {i} {kind} bus {}{sheet} cond [{}] vol {:.1} pitch {:.2} loops {} voices {} steal {} start {}{ap} layouts [{}]{audio}", "  ".repeat(depth), ins.bus,
                cond.join("; "), ins.volume, ins.pitch, ins.loops, ins.max_voices, ins.steal, ins.start_off, lays.join(" "));
            props(a, &pn, &ins.props, "inst");
            match it {
                Instrument::Multi { sounds, .. } => todo.extend(sounds.iter().map(|x| (x.0, depth + 1))),
                Instrument::Scatter { sound, .. } => todo.push((*sound, depth + 1)),
                Instrument::Wave { .. } => {}
            }
        }
    }
    Ok(())
}

// ------------------------------------------------------------------ offline renders (both runtimes)
const RATE: c_int = 48000;
const BLOCK: c_uint = 512;

#[repr(C)]
#[derive(Clone, Copy)]
struct Vec3 {
    x: f32,
    y: f32,
    z: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Attr3d {
    position: Vec3,
    velocity: Vec3,
    forward: Vec3,
    up: Vec3,
}

/// At `z` metres ahead of the listener, facing away (a chase camera's view).
fn attr(z: f32) -> Attr3d {
    let v = |x, y, z| Vec3 { x, y, z };
    Attr3d { position: v(0.0, 0.0, z), velocity: v(0.0, 0.0, 0.0), forward: v(0.0, 0.0, 1.0), up: v(0.0, 1.0, 0.0) }
}

type P = *mut c_void;

enum Ev {
    Path(String),
    Id(Guid),
}

/// The game's runtime (2.03: fmod.dll / fmodstudio.dll) or Assetto Corsa's (1.08: fmod64.dll /
/// fmodstudio64.dll); one fresh system per cell, output to FMOD's WAV writer only.
struct Rt {
    dir: PathBuf,
    ac1: bool,
}

macro_rules! sym {
    ($lib:expr, $name:expr, $t:ty) => {
        unsafe { *$lib.get::<$t>($name.as_bytes()).map_err(|e| anyhow!("{}: {e}", $name))? }
    };
}

fn ck(r: c_int, what: &str) -> Result<()> {
    if r == 0 { Ok(()) } else { bail!("FMOD {what}: error {r}") }
}

impl Rt {
    /// RMS (dBFS) of the event with each cell's parameters held.
    fn cells(&self, banks: &[&[u8]], ev: &Ev, dist: f32, cells: &[Vec<(String, f32)>], secs: f32, skip: f32, tmp: &Path) -> Result<Vec<f32>> {
        let (core_dll, st_dll, ver) = if self.ac1 { ("fmod64.dll", "fmodstudio64.dll", 0x0001_0812u32) } else { ("fmod.dll", "fmodstudio.dll", 0x0002_0313) };
        let core = unsafe { libloading::Library::new(self.dir.join(core_dll)) }.map_err(|e| anyhow!("{}: {e}", self.dir.join(core_dll).display()))?;
        let st = unsafe { libloading::Library::new(self.dir.join(st_dll)) }.map_err(|e| anyhow!("{}: {e}", self.dir.join(st_dll).display()))?;
        let create = sym!(st, "FMOD_Studio_System_Create", unsafe extern "system" fn(*mut P, c_uint) -> c_int);
        let get_core = sym!(st, if self.ac1 { "FMOD_Studio_System_GetLowLevelSystem" } else { "FMOD_Studio_System_GetCoreSystem" }, unsafe extern "system" fn(P, *mut P) -> c_int);
        let init = sym!(st, "FMOD_Studio_System_Initialize", unsafe extern "system" fn(P, c_int, c_uint, c_uint, *mut c_void) -> c_int);
        let release = sym!(st, "FMOD_Studio_System_Release", unsafe extern "system" fn(P) -> c_int);
        let update = sym!(st, "FMOD_Studio_System_Update", unsafe extern "system" fn(P) -> c_int);
        let load_mem = sym!(st, "FMOD_Studio_System_LoadBankMemory", unsafe extern "system" fn(P, *const c_char, c_int, c_int, c_uint, *mut P) -> c_int);
        let get_event = sym!(st, "FMOD_Studio_System_GetEvent", unsafe extern "system" fn(P, *const c_char, *mut P) -> c_int);
        // FMOD_GUID: the 16 bytes as the banks store them
        let get_event_id = sym!(st, "FMOD_Studio_System_GetEventByID", unsafe extern "system" fn(P, *const [u8; 16], *mut P) -> c_int);
        let listener = sym!(st, "FMOD_Studio_System_SetListenerAttributes", unsafe extern "system" fn(P, c_int, *const Attr3d, *const Vec3) -> c_int);
        let load_samples = sym!(st, "FMOD_Studio_EventDescription_LoadSampleData", unsafe extern "system" fn(P) -> c_int);
        let loading = sym!(st, "FMOD_Studio_EventDescription_GetSampleLoadingState", unsafe extern "system" fn(P, *mut c_int) -> c_int);
        let instance = sym!(st, "FMOD_Studio_EventDescription_CreateInstance", unsafe extern "system" fn(P, *mut P) -> c_int);
        let set3d = sym!(st, "FMOD_Studio_EventInstance_Set3DAttributes", unsafe extern "system" fn(P, *const Attr3d) -> c_int);
        // 2.03: (instance, name, value, ignore seek speed); 1.08: (instance, name, value)
        let set_param = sym!(st, if self.ac1 { "FMOD_Studio_EventInstance_SetParameterValue" } else { "FMOD_Studio_EventInstance_SetParameterByName" },
            unsafe extern "system" fn(P, *const c_char, f32, c_int) -> c_int);
        let start = sym!(st, "FMOD_Studio_EventInstance_Start", unsafe extern "system" fn(P) -> c_int);
        let stop = sym!(st, "FMOD_Studio_EventInstance_Stop", unsafe extern "system" fn(P, c_int) -> c_int);
        let inst_release = sym!(st, "FMOD_Studio_EventInstance_Release", unsafe extern "system" fn(P) -> c_int);
        let set_output = sym!(core, "FMOD_System_SetOutput", unsafe extern "system" fn(P, c_int) -> c_int);
        let set_format = sym!(core, "FMOD_System_SetSoftwareFormat", unsafe extern "system" fn(P, c_int, c_int, c_int) -> c_int);
        let set_dsp = sym!(core, "FMOD_System_SetDSPBufferSize", unsafe extern "system" fn(P, c_uint, c_int) -> c_int);
        let load_plugin = sym!(core, "FMOD_System_LoadPlugin", unsafe extern "system" fn(P, *const c_char, *mut c_uint, c_uint) -> c_int);
        std::fs::create_dir_all(tmp)?;
        let mut out = Vec::new();
        for (k, cell) in cells.iter().enumerate() {
            let wav = tmp.join(format!("cell_{k}.wav"));
            let wc = CString::new(wav.to_string_lossy().as_bytes())?;
            let mut sys: P = std::ptr::null_mut();
            ck(unsafe { create(&mut sys, ver) }, "Studio System_Create")?;
            let res = (|| -> Result<usize> {
                let mut cs: P = std::ptr::null_mut();
                ck(unsafe { get_core(sys, &mut cs) }, "core system")?;
                ck(unsafe { set_output(cs, 5) }, "SetOutput(WAVWRITER_NRT)")?;
                ck(unsafe { set_format(cs, RATE, 3, 0) }, "SetSoftwareFormat")?;
                ck(unsafe { set_dsp(cs, BLOCK, 4) }, "SetDSPBufferSize")?;
                if !self.ac1 {
                    for pl in ["fmod_distance_filter.dll", "fmod_gain.dll", "resonanceaudio.dll"] {
                        if let Ok(c) = CString::new(self.dir.join(pl).to_string_lossy().as_bytes()) {
                            let mut h: c_uint = 0;
                            unsafe { load_plugin(cs, c.as_ptr(), &mut h, 0) };
                        }
                    }
                }
                ck(unsafe { init(sys, 128, 2, 0, wc.as_ptr() as *mut c_void) }, "Initialize")?;
                for b in banks {
                    let mut bk: P = std::ptr::null_mut();
                    ck(unsafe { load_mem(sys, b.as_ptr() as *const c_char, b.len() as c_int, 0, 0, &mut bk) }, "LoadBankMemory")?;
                }
                let mut ed: P = std::ptr::null_mut();
                match ev {
                    Ev::Path(p) => {
                        let en = CString::new(p.as_str())?;
                        ck(unsafe { get_event(sys, en.as_ptr(), &mut ed) }, &format!("GetEvent {p}"))?;
                    }
                    Ev::Id(g) => ck(unsafe { get_event_id(sys, &g.0, &mut ed) }, &format!("GetEventByID {g}"))?,
                }
                ck(unsafe { load_samples(ed) }, "LoadSampleData")?;
                let mut pre = 0usize;
                for _ in 0..4000 {
                    let mut s: c_int = 0;
                    unsafe { loading(ed, &mut s) };
                    if s == 3 {
                        break;
                    }
                    ck(unsafe { update(sys) }, "Update")?;
                    pre += 1;
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
                let mut ins: P = std::ptr::null_mut();
                ck(unsafe { instance(ed, &mut ins) }, "CreateInstance")?;
                let lis = attr(0.0);
                unsafe { listener(sys, 0, &lis, std::ptr::null()) };
                let a = attr(dist);
                unsafe { set3d(ins, &a) };
                let names: Vec<CString> = cell.iter().map(|(n, _)| CString::new(n.as_str()).unwrap()).collect();
                for ((_, v), n) in cell.iter().zip(&names) {
                    unsafe { set_param(ins, n.as_ptr(), *v, 1) };
                }
                ck(unsafe { start(ins) }, "Start")?;
                let n = (secs * RATE as f32 / BLOCK as f32).ceil() as usize;
                for _ in 0..n {
                    ck(unsafe { update(sys) }, "Update")?;
                }
                unsafe {
                    stop(ins, 1);
                    inst_release(ins);
                    update(sys);
                }
                Ok(pre)
            })();
            unsafe { release(sys) };
            let pre = res?;
            out.push(wav_rms(&wav, pre * BLOCK as usize + (skip * RATE as f32) as usize, (secs - skip).max(0.1))?);
            let _ = std::fs::remove_file(&wav);
        }
        Ok(out)
    }
}

/// RMS in dBFS of a 16-bit / float WAV: `secs` seconds after `skip` frames.
fn wav_rms(p: &Path, skip: usize, secs: f32) -> Result<f32> {
    let b = std::fs::read(p)?;
    let mut i = 12;
    let (mut ch, mut bits, mut fmt) = (2usize, 16u16, 1u16);
    while i + 8 <= b.len() {
        let id = &b[i..i + 4];
        let n = u32::from_le_bytes(b[i + 4..i + 8].try_into().unwrap()) as usize;
        let body = &b[i + 8..(i + 8 + n).min(b.len())];
        if id == b"fmt " && body.len() >= 16 {
            fmt = u16::from_le_bytes([body[0], body[1]]);
            ch = u16::from_le_bytes([body[2], body[3]]) as usize;
            bits = u16::from_le_bytes([body[14], body[15]]);
        }
        if id == b"data" {
            let fs = ch * bits as usize / 8;
            let d = &body[(skip * fs).min(body.len())..];
            let d = &d[..(((secs * RATE as f32) as usize) * fs).min(d.len())];
            let (mut s, mut n) = (0f64, 0usize);
            if bits == 16 {
                for c in d.chunks_exact(2) {
                    let x = i16::from_le_bytes([c[0], c[1]]) as f64 / 32768.0;
                    s += x * x;
                    n += 1;
                }
            } else if bits == 32 && fmt == 3 {
                for c in d.chunks_exact(4) {
                    let x = f32::from_le_bytes(c.try_into().unwrap()) as f64;
                    s += x * x;
                    n += 1;
                }
            } else {
                bail!("WAV format {fmt}/{bits} bits");
            }
            return Ok((10.0 * (s / n.max(1) as f64 + 1e-18).log10()) as f32);
        }
        i += 8 + n + (n & 1);
    }
    bail!("{}: no data chunk", p.display())
}

fn list(s: Option<&String>, def: &[f32]) -> Result<Vec<f32>> {
    match s {
        Some(s) => s.split(',').map(|x| x.trim().parse::<f32>().map_err(|e| anyhow!("{x}: {e}"))).collect(),
        None => Ok(def.to_vec()),
    }
}

fn env_f(name: &str, def: f32) -> f32 {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(def)
}

/// Print the grid: one row per rpm, one column per throttle.
fn run_grid(rt: &Rt, banks: &[&[u8]], ev: &Ev, label: &str, throttle_name: &str, rest: &[String], fixed_default: &[(&str, f32)]) -> Result<()> {
    let pos: Vec<&String> = rest.iter().filter(|a| !a.contains('=')).collect();
    let rpms = list(pos.first().copied(), &[800.0, 1000.0, 1500.0, 2000.0, 2500.0, 3000.0, 4000.0, 6000.0])?;
    let thr = list(pos.get(1).copied(), &[0.0, 0.2, 0.5, 1.0])?;
    let mut fixed: Vec<(String, f32)> = fixed_default.iter().map(|(k, v)| (k.to_string(), *v)).collect();
    for a in rest.iter().filter(|a| a.contains('=')) {
        let (k, v) = a.split_once('=').unwrap();
        fixed.retain(|f| f.0 != k);
        fixed.push((k.to_string(), v.parse()?));
    }
    let dist = env_f("SOUNDPROBE_DISTANCE", if label.ends_with("_int") { 1.0 } else { 6.0 });
    let (secs, skip) = (env_f("SOUNDPROBE_SECS", 3.0), env_f("SOUNDPROBE_SKIP", 1.0));
    let tmp = std::env::temp_dir().join(format!("soundprobe-{}", std::process::id()));
    println!("{label} at {dist} m{}{}", if fixed.is_empty() { "" } else { ", fixed " }, fixed.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(" "));
    println!("{:>8}{}", "rpm", thr.iter().map(|t| format!("{:>9}", format!("thr {t}"))).collect::<String>());
    for r in &rpms {
        let cells: Vec<Vec<(String, f32)>> = thr.iter().map(|t| {
            let mut c = vec![("rpms".to_string(), *r), (throttle_name.to_string(), *t)];
            c.extend(fixed.iter().cloned());
            c
        }).collect();
        let lv = rt.cells(banks, ev, dist, &cells, secs, skip, &tmp)?;
        println!("{r:>8}{}", lv.iter().map(|v| format!("{v:>9.1}")).collect::<String>());
    }
    let _ = std::fs::remove_dir_all(&tmp);
    Ok(())
}

fn main() -> Result<()> {
    let a: Vec<String> = std::env::args().collect();
    match a.get(1).map(|s| s.as_str()) {
        Some("params") => {
            for c in &a[2..] {
                params(&PathBuf::from(c))?;
            }
        }
        Some("grid") if a.len() >= 4 => {
            let content = aceconv::io::gamedir::evo_content();
            let dir = content.parent().map(Path::to_path_buf).ok_or_else(|| anyhow!("no game folder"))?;
            let src = mkfmod::Src::open(&content)?;
            let mut banks = vec![src.bank("master_bank.strings.bank")?, src.bank("master_bank.bank")?];
            let d = Path::new(&a[2]);
            if d.is_dir() {
                banks.push(std::fs::read(d.join("master_bank.strings.bank"))?);
                banks.push(std::fs::read(d.join("master_bank.bank"))?);
            } else {
                banks.push(src.bank(&format!("{}.bank", a[2]))?);
            }
            let refs: Vec<&[u8]> = banks.iter().map(|b| b.as_slice()).collect();
            run_grid(&Rt { dir, ac1: false }, &refs, &Ev::Path(a[3].clone()), &a[3], "throttle_position", &a[4..], &[("ignition", 2.0), ("starter", 2.0)])?;
        }
        Some("stocklevel") if a.len() >= 3 => {
            // the level pass's sweep on stock cars, per rpm band: <car id> TAB <limiter rpm> per line
            let content = aceconv::io::gamedir::evo_content();
            let dir = content.parent().map(Path::to_path_buf).ok_or_else(|| anyhow!("no game folder"))?;
            let src = mkfmod::Src::open(&content)?;
            let (ms, mb) = (src.bank("master_bank.strings.bank")?, src.bank("master_bank.bank")?);
            let n: usize = a.get(3).map(|s| s.parse()).transpose()?.unwrap_or(6);
            let tmp = std::env::temp_dir().join(format!("soundprobe-{}", std::process::id()));
            println!("car\tlimiter\tside\tall\t{}", (0..n).map(|k| format!("b{k}")).collect::<Vec<_>>().join("\t"));
            for line in std::fs::read_to_string(&a[2])?.lines() {
                let mut it = line.trim_start_matches('\u{feff}').split('\t');
                let (Some(id), Some(lim)) = (it.next(), it.next().and_then(|v| v.trim().parse::<f32>().ok())) else { continue };
                // a third column: a folder of converted banks (master_bank(.strings).bank) instead of the stock bank
                let banks: Result<Vec<Vec<u8>>> = match it.next().map(str::trim).filter(|d| !d.is_empty()) {
                    Some(d) => [Path::new(d).join("master_bank.strings.bank"), Path::new(d).join("master_bank.bank")].iter()
                        .map(|p| std::fs::read(p).map_err(|e| anyhow!("{}: {e}", p.display()))).collect(),
                    None => src.bank(&format!("{id}.bank")).map(|b| vec![b]),
                };
                let banks = match banks {
                    Ok(b) => b,
                    Err(e) => {
                        eprintln!("{id}: {e:#}");
                        continue;
                    }
                };
                let (ext, int) = (format!("event:/evo_cars/{id}/engine_ext"), format!("event:/evo_cars/{id}/engine_int"));
                let shots = map::level_shots(&ext, &int, lim);
                let mut refs: Vec<&[u8]> = vec![&ms, &mb];
                refs.extend(banks.iter().map(|b| b.as_slice()));
                match aceconv::sound::render::bands(&dir, &refs, &shots, &tmp, n) {
                    Ok(b) => {
                        for (side, v) in ["ext", "int"].iter().zip(&b) {
                            // the whole sweep: the power mean of equal stretches
                            let t = 10.0 * (v.iter().map(|x| 10f32.powf(x / 10.0)).sum::<f32>() / v.len() as f32).log10();
                            println!("{id}\t{lim}\t{side}\t{t:.2}\t{}", v.iter().map(|x| format!("{x:.2}")).collect::<Vec<_>>().join("\t"));
                        }
                    }
                    Err(e) => eprintln!("{id}: {e:#}"),
                }
            }
            let _ = std::fs::remove_dir_all(&tmp);
        }
        Some("fsb") if a.len() >= 3 => {
            // FSB5 sample headers: chunk types and payloads, with the bank's sample names
            let b = std::fs::read(&a[2])?;
            let names: Vec<String> = aceconv::sound::ac1bank::read(&b).map(|x| x.bank.samples.iter().map(|s| s.name.clone()).collect()).unwrap_or_default();
            let mut k = 0usize;
            for base in aceconv::sound::fsb5::blobs_of(&b)? {
                let info = aceconv::sound::fsb5::parse_blob(&b, base)?;
                for s in &info.samples {
                    let ch: Vec<String> = s.chunks.iter().map(|c| {
                        let f: Vec<String> = c.payload.chunks_exact(4).take(4).map(|q| format!("{:.4}", f32::from_le_bytes(q.try_into().unwrap()))).collect();
                        format!("type {:#x} size {} f32 [{}]", c.ctype, c.size, f.join(" "))
                    }).collect();
                    println!("{k:3} {:<34} ch {} frames {:>9}  {}", names.get(k).map(|s| s.as_str()).unwrap_or("?"), s.channels, s.frames, ch.join("; "));
                    k += 1;
                }
            }
        }
        Some("voltest") if a.len() >= 8 => {
            // how AC1's runtime applies an instrument's volume field: the cell with the field as
            // stored and moved by <delta>, the bank patched in memory
            let car = PathBuf::from(&a[2]);
            let s = map::open_source(&car)?;
            let ev = s.paths.iter().find(|(p, _)| p.rsplit('/').next().map(|l| l.eq_ignore_ascii_case(&a[3])).unwrap_or(false)).map(|x| x.1)
                .ok_or_else(|| anyhow!("no event {}", a[3]))?;
            let (delta, rpm, thr): (f32, f32, f32) = (a[5].parse()?, a[6].parse()?, a[7].parse()?);
            let raw = std::fs::read(&s.bank_path)?;
            // the volume fields to move: one instrument's (its guid, then - after the audio guid of a
            // sound - its INST item), or every INST item's ("all")
            let vos: Vec<usize> = if a[4] == "all" {
                (0..raw.len().saturating_sub(8)).filter(|&i| &raw[i..i + 8] == b"INST\x7b\x00\x00\x00").map(|i| i + 8 + 16).collect()
            } else {
                let g = Guid::parse(&a[4]).ok_or_else(|| anyhow!("bad guid {}", a[4]))?;
                let at = (0..raw.len().saturating_sub(64)).find(|&i| raw[i..i + 16] == g.0 && (16..=40).any(|k| &raw[i + k..i + k + 4] == b"INST"))
                    .ok_or_else(|| anyhow!("no INST for {g}"))?;
                vec![at + (16..=40).find(|k| &raw[at + k..at + k + 4] == b"INST").unwrap() + 8 + 16]
            };
            let v = f32::from_le_bytes(raw[vos[0]..vos[0] + 4].try_into().unwrap());
            let mut patched = raw.clone();
            for vo in &vos {
                let x = f32::from_le_bytes(raw[*vo..*vo + 4].try_into().unwrap());
                patched[*vo..*vo + 4].copy_from_slice(&(x + delta).to_le_bytes());
            }
            let g = format!("{} instrument(s)", vos.len());
            let dir = aceconv::io::gamedir::ac1_dir().ok_or_else(|| anyhow!("no Assetto Corsa install"))?;
            let sfx = dir.join("content").join("sfx");
            let (cs, cb) = (std::fs::read(sfx.join("common.strings.bank"))?, std::fs::read(sfx.join("common.bank"))?);
            let rt = Rt { dir, ac1: true };
            let tmp = std::env::temp_dir().join(format!("soundprobe-{}", std::process::id()));
            let cell = vec![vec![("rpms".to_string(), rpm), ("throttle".to_string(), thr)]];
            let dist = env_f("SOUNDPROBE_DISTANCE", if a[3].ends_with("_int") { 1.0 } else { 6.0 });
            let l0 = rt.cells(&[&cs, &cb, &raw], &Ev::Id(ev), dist, &cell, 3.0, 1.0, &tmp)?[0];
            let l1 = rt.cells(&[&cs, &cb, &patched], &Ev::Id(ev), dist, &cell, 3.0, 1.0, &tmp)?[0];
            let _ = std::fs::remove_dir_all(&tmp);
            println!("{} {g}: volume field {v} -> {} at rpm {rpm} throttle {thr}: {l0:.1} -> {l1:.1} dBFS ({:+.1} dB for {delta:+})", car.display(), v + delta, l1 - l0);
        }
        Some("ac1grid") if a.len() >= 4 => {
            let car = PathBuf::from(&a[2]);
            let s = map::open_source(&car)?;
            let ev = s.paths.iter().find(|(p, _)| p.rsplit('/').next().map(|l| l.eq_ignore_ascii_case(&a[3])).unwrap_or(false)).map(|x| x.1)
                .ok_or_else(|| anyhow!("{}: no event {}", car.display(), a[3]))?;
            let dir = aceconv::io::gamedir::ac1_dir().ok_or_else(|| anyhow!("no Assetto Corsa install (ACECONV_AC1_DIR)"))?;
            let sfx = dir.join("content").join("sfx");
            let banks = vec![std::fs::read(sfx.join("common.strings.bank"))?, std::fs::read(sfx.join("common.bank"))?, std::fs::read(&s.bank_path)?];
            let refs: Vec<&[u8]> = banks.iter().map(|b| b.as_slice()).collect();
            run_grid(&Rt { dir, ac1: true }, &refs, &Ev::Id(ev), &a[3], "throttle", &a[4..], &[])?;
        }
        _ => bail!("soundprobe params <ac1 car dir>... | grid <bank dir | stock bank> <event path> [rpms] [throttles] [name=value ...] | ac1grid <ac1 car dir> <event> [rpms] [throttles] [name=value ...] | voltest <ac1 car dir> <event> <guid | all> <delta> <rpm> <throttle> | fsb <bank>"),
    }
    Ok(())
}
