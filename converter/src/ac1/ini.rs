//! AC1 ini / lut text.
//!
//! INI (AC1 conventions): `[SECTION]` headers, `KEY=VALUE` lines, comments
//! from `;` or `//` to the end of the line, section and key names
//! upper-cased, values trimmed and kept as text, a later duplicate key
//! replaces the earlier value (keeping its place) unless it is mistyped (a
//! second `=` in a value that had none), a UTF-8 BOM tolerated.
//! Lines before the first section belong to the section "".
//! Sections keep file order.
//!
//! LUT: `x|y` lines, blank and comment lines skipped; an inline LUT as AC1
//! writes it in an ini value is `(|0=0|1000=1|)`.

use indexmap::IndexMap;

/// Section -> key -> value, both in file order.
#[derive(Clone, Debug, Default)]
pub struct Ini {
    pub sections: IndexMap<String, IndexMap<String, String>>,
}

fn strip_comment(l: &str) -> &str {
    let mut end = l.len();
    if let Some(p) = l.find(';') {
        end = end.min(p);
    }
    if let Some(p) = l.find("//") {
        end = end.min(p);
    }
    &l[..end]
}

/// Section names of AC1's car data files (Kunos cars), and their numbered
/// families (`TURBO_0`, `WING_2`, `LIGHT_5`, ...).
fn ac1_section(name: &str) -> bool {
    const NAMES: &[&str] = &["HEADER", "INFO", "BASIC", "GRAPHICS", "CONTROLS", "FUEL", "FUEL_EXT", "PIT_STOP", "RIDE", "RULES",
        "ENGINE_DATA", "COAST_REF", "COAST_DATA", "COAST_CURVE", "DAMAGE", "TRACTION", "GEARS", "GEARBOX", "CLUTCH", "DIFFERENTIAL",
        "AUTOCLUTCH", "AUTOBLIP", "AUTO_SHIFTER", "DOWNSHIFT_PROFILE", "UPSHIFT_PROFILE", "AWD", "AWD2", "FRONT", "REAR", "ARB",
        "GRAPHICS_OFFSETS", "FRONT_BRAKE", "REAR_BRAKE", "DATA", "DISCS_GRAPHICS", "COMPOUND_DEFAULT", "THERMAL_FRONT", "THERMAL_REAR"];
    let n = name.trim_end_matches(|c: char| c.is_ascii_digit());
    NAMES.contains(&name)
        || (n.len() < name.len() && n.ends_with('_') && ["TURBO_", "WING_", "LIGHT_", "GEAR_", "FRONT_", "REAR_", "THERMAL_FRONT_", "THERMAL_REAR_", "DYNAMIC_CONTROLLER_"].contains(&n))
}

pub fn parse(text: &str) -> Ini {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut ini = Ini::default();
    let mut cur = String::new();
    for raw in text.lines() {
        let mut l = strip_comment(raw).trim();
        if l.is_empty() {
            continue;
        }
        // a header at the end of a key line starts that section, as AC1 reads it: the
        // HillClimb Ferrari 488 GTE's car.ini has `SHORT_NAME=488 GTE [BASIC]`, and its
        // GRAPHICS_OFFSET (0.405 m down), TOTALMASS and INERTIA under it were lost to [INFO]
        // (the model 19.5 cm out, 1200 kg instead of 1330). Only AC1's own section names
        // count, so a name like `Ferrari 488 [GT3]` stays a value.
        let mut next: Option<String> = None;
        if !l.starts_with('[') && l.ends_with(']') {
            if let Some(p) = l.rfind('[') {
                let name = l[p + 1..l.len() - 1].trim();
                if l[..p].contains('=') && ac1_section(name) {
                    next = Some(name.to_string());
                    l = l[..p].trim();
                }
            }
        }
        if let Some(n) = next {
            if let Some((k, v)) = l.split_once('=') {
                let k = k.trim().to_uppercase();
                if !k.is_empty() {
                    ini.sections.entry(cur.clone()).or_default().insert(k, v.trim().to_string());
                }
            }
            cur = n;
            ini.sections.entry(cur.clone()).or_default();
            continue;
        }
        if let Some(rest) = l.strip_prefix('[') {
            if let Some(end) = rest.find(']') {
                cur = rest[..end].trim().to_uppercase();
                ini.sections.entry(cur.clone()).or_default();
                continue;
            }
        }
        if let Some((k, v)) = l.split_once('=') {
            let k = k.trim().to_uppercase();
            if k.is_empty() {
                continue;
            }
            let v = v.trim();
            let sec = ini.sections.entry(cur.clone()).or_default();
            // a mistyped repeat keeps the value before it: the dthwsh 240SX /
            // PS13 drift pack's `GEAR_5=6=0` under `GEAR_5=0.759` (a GEAR_6
            // line gone wrong) made fifth gear 6.0, top speed 36 km/h; the
            // rest is the S13's stock 5-speed (3.321 .. 0.759)
            if v.contains('=') && sec.get(&k).is_some_and(|old| !old.contains('=')) {
                continue;
            }
            sec.insert(k, v.to_string());
        }
    }
    ini
}

impl Ini {
    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.sections.get(&section.to_uppercase())?.get(&key.to_uppercase()).map(|s| s.as_str())
    }
    pub fn section(&self, section: &str) -> Option<&IndexMap<String, String>> {
        self.sections.get(&section.to_uppercase())
    }
    pub fn has(&self, section: &str) -> bool {
        self.sections.contains_key(&section.to_uppercase())
    }
    /// Section names in file order.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.sections.keys().map(|s| s.as_str())
    }
    /// Sections whose name starts with `prefix` (`LIGHT_0`, `LIGHT_1`, ...), in file order.
    pub fn with_prefix<'a>(&'a self, prefix: &'a str) -> impl Iterator<Item = (&'a str, &'a IndexMap<String, String>)> + 'a {
        let p = prefix.to_uppercase();
        self.sections.iter().filter(move |(k, _)| k.starts_with(&p)).map(|(k, v)| (k.as_str(), v))
    }
    pub fn num(&self, section: &str, key: &str) -> Option<f64> {
        self.get(section, key).and_then(num_of)
    }
    pub fn nums(&self, section: &str, key: &str) -> Vec<f64> {
        self.get(section, key).map(nums_of).unwrap_or_default()
    }
}

/// The leading number of a value (`"7000 ; rpm"` is already cut; `"0.5f"`,
/// `"1,"` tolerated).
pub fn num_of(v: &str) -> Option<f64> {
    let v = v.trim();
    if let Ok(x) = v.parse() {
        return Some(x);
    }
    let end = v.char_indices().take_while(|(i, c)| c.is_ascii_digit() || *c == '.' || ((*c == '-' || *c == '+') && *i == 0) || *c == 'e' || *c == 'E').map(|(i, c)| i + c.len_utf8()).last()?;
    v[..end].parse().ok()
}

/// Every number in a comma / space separated value.
pub fn nums_of(v: &str) -> Vec<f64> {
    v.split(|c: char| c == ',' || c.is_whitespace()).filter(|s| !s.is_empty()).filter_map(num_of).collect()
}

/// Free-function forms of the accessors.
pub fn get<'a>(ini: &'a Ini, section: &str, key: &str) -> Option<&'a str> {
    ini.get(section, key)
}
pub fn num(ini: &Ini, section: &str, key: &str) -> Option<f64> {
    ini.num(section, key)
}
pub fn nums(ini: &Ini, section: &str, key: &str) -> Vec<f64> {
    ini.nums(section, key)
}

/// A `.lut` file, or an inline `(|x=y|x=y|)` value.
pub fn lut(text: &str) -> Vec<(f64, f64)> {
    let t = text.trim();
    if t.starts_with('(') && t.ends_with(')') {
        return t[1..t.len() - 1]
            .split('|')
            .filter_map(|p| p.split_once('='))
            .filter_map(|(a, b)| Some((num_of(a)?, num_of(b)?)))
            .collect();
    }
    let t = t.strip_prefix('\u{feff}').unwrap_or(t);
    t.lines()
        .map(|l| strip_comment(l).trim())
        .filter(|l| !l.is_empty())
        .filter_map(|l| l.split_once('|'))
        .filter_map(|(a, b)| Some((num_of(a)?, num_of(b)?)))
        .collect()
}

/// Linear interpolation in a LUT (clamped at the ends).
pub fn lut_at(l: &[(f64, f64)], x: f64) -> Option<f64> {
    let first = l.first()?;
    if x <= first.0 {
        return Some(first.1);
    }
    for w in l.windows(2) {
        let ((x0, y0), (x1, y1)) = (w[0], w[1]);
        if x <= x1 {
            return Some(if x1 > x0 { y0 + (y1 - y0) * (x - x0) / (x1 - x0) } else { y1 });
        }
    }
    Some(l.last()?.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules() {
        let i = parse("\u{feff}top=1\n[header]\nVersion = 3 ; comment\n// full comment\n[ENGINE_DATA]\nlimiter=7000\nLIMITER=7200\nidle=900//x\n[Header]\nextra=a b\n");
        assert_eq!(i.get("", "TOP"), Some("1"));
        assert_eq!(i.get("HEADER", "version"), Some("3"));
        assert_eq!(i.num("engine_data", "LIMITER"), Some(7200.0));
        assert_eq!(i.num("ENGINE_DATA", "IDLE"), Some(900.0));
        assert_eq!(i.get("HEADER", "EXTRA"), Some("a b"));
        assert_eq!(i.names().collect::<Vec<_>>(), vec!["", "HEADER", "ENGINE_DATA"]);
        assert_eq!(i.section("ENGINE_DATA").unwrap().keys().collect::<Vec<_>>(), vec!["LIMITER", "IDLE"]);
        // a mistyped repeat keeps the value before it (dthwsh 240SX); an
        // inline LUT is still read, and may still be replaced
        let g = parse("[GEARS]\nCOUNT=6\nGEAR_4=1.000\nGEAR_5=0.759\nGEAR_5=6=0\nFINAL=4.08\n[X]\nC=(|0=0|1=1|)\nC=(|0=1|1=0|)\n");
        assert_eq!(g.num("GEARS", "GEAR_5"), Some(0.759));
        assert_eq!(g.get("GEARS", "GEAR_6"), None);
        assert_eq!(g.get("X", "C"), Some("(|0=1|1=0|)"));
    }

    #[test]
    fn header_at_line_end() {
        // the HillClimb 488 GTE's car.ini
        let i = parse("[INFO]\nSCREEN_NAME=Ferrari 488 GTE\n\n \nSHORT_NAME=488 GTE [BASIC]\nGRAPHICS_OFFSET=0.0,-0.405,-0.195\t; x\nTOTALMASS=1330\n[GRAPHICS]\nX=1\n");
        assert_eq!(i.get("INFO", "SHORT_NAME"), Some("488 GTE"));
        assert_eq!(i.nums("BASIC", "GRAPHICS_OFFSET"), vec![0.0, -0.405, -0.195]);
        assert_eq!(i.num("BASIC", "TOTALMASS"), Some(1330.0));
        assert_eq!(i.get("INFO", "TOTALMASS"), None);
        assert_eq!(i.num("GRAPHICS", "X"), Some(1.0));
        // a name in brackets is a value, not a section
        let n = parse("[INFO]\nSCREEN_NAME=Ferrari 488 [GT3]\nSHORT_NAME=488\n");
        assert_eq!(n.get("INFO", "SCREEN_NAME"), Some("Ferrari 488 [GT3]"));
        assert_eq!(n.get("INFO", "SHORT_NAME"), Some("488"));
        // numbered families
        let t = parse("[ENGINE_DATA]\nLIMITER=8000 [TURBO_0]\nMAX_BOOST=1\n");
        assert_eq!(t.num("TURBO_0", "MAX_BOOST"), Some(1.0));
    }

    #[test]
    fn luts() {
        assert_eq!(lut("0|0\n; c\n\n1000|0.5\n2000 | 1 // x\n"), vec![(0.0, 0.0), (1000.0, 0.5), (2000.0, 1.0)]);
        assert_eq!(lut("(|0=0|1000=1|)"), vec![(0.0, 0.0), (1000.0, 1.0)]);
        assert_eq!(lut_at(&lut("(|0=0|1000=1|)"), 250.0), Some(0.25));
        assert_eq!(nums_of("0.1, -2,3e1"), vec![0.1, -2.0, 30.0]);
    }
}
