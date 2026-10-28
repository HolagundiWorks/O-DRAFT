//! Project data kept in the drawing: details, structure defaults, floors, beam depths and levels
//! (the kit's HCWPROJECT, HCWFLOORS, HCWBEAMS and HCWLEVELS). Stored in `Drawing::project` as JSON
//! text and written to DXF/DWG with the drawing, so schedules and take-off read what is entered once.
//!
//! Slab thickness and beam depth (including slab) are project settings. Floor-to-floor, ceiling and
//! lintel heights stay in storage for take-off/bridge with defaults; they are not edited in the UI.

use aad_doc::Drawing;
use serde_json::{Map, Value, json};

use super::*;
use crate::{Result, Session};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("project", "Project Settings", run_project)
            .menu(&["Architecture", "Project", "Project Settings"])
            .alias(&["hcwproject"])
            .params("{details?: {title, owner, architect, engineer, pid, siteArea, plotUse, address, authority, date, location, summary}, slabMm?, beamMm?, structure?: {slabMm, beamMm}} → {details, structure, company, suite}. `company` is what the company file gave the drawing (name, address, phone, email, pan, gstin, coa_registration, architect_name, prepared_on), read only here: set it with `companyfile`. When `suite.refresh_only`, company-owned detail keys in `suite.locked_details` cannot be changed here — edit them in Studio and refresh."),
        CommandSpec::new("floors", "Floors", run_floors)
            .menu(&["Architecture", "Project", "Floors"])
            .alias(&["hcwfloors"])
            .params("{count?: 1..60, floors?: [{name, floorToFloor?, ceiling?, lintel?, slab?, beam?}]} → {floors} (names in the dialog; heights default from project settings)"),
        CommandSpec::new("beams", "Beam Depth List", run_beams)
            .menu(&["Architecture", "Project", "Beam Depth List"])
            .alias(&["hcwbeams"])
            .params("{depths?: [mm, ...]} → {depths}"),
        CommandSpec::new("levels", "Building Levels", run_levels)
            .menu(&["Architecture", "Project", "Building Levels"])
            .alias(&["hcwlevels"])
            .params("{} → {levels: [{name, level, floorToFloor, ceiling, lintel, slab, beam}]} (finished floor levels in metres, from the plinth)")
            .noundo(),
        CommandSpec::new("site.origin", "Site Origin", run_site_origin)
            .menu(&["Architecture", "Project", "Site Origin"])
            .alias(&["siteorigin", "basepoint"])
            .params("{x?: number, y?: number, point?: [x, y]} → {origin: [x, y]}. Shared world XY for every level; omit fields to read. Canvas mark follows the UCS Icon visibility."),
    ]
}

/// A project entry as JSON, or `Null` when absent or unreadable.
pub(crate) fn get(d: &Drawing, key: &str) -> Value {
    d.project.get(key).and_then(|t| serde_json::from_str(t).ok()).unwrap_or(Value::Null)
}

/// Store a project entry.
pub(crate) fn set(d: &mut Drawing, key: &str, v: &Value) {
    d.project.insert(key.to_string(), v.to_string());
}

const FLOOR_NAMES: [&str; 5] = ["Ground", "First", "Second", "Third", "Fourth"];
const DETAIL_KEYS: [&str; 12] =
    ["title", "owner", "architect", "engineer", "pid", "siteArea", "plotUse", "address", "authority", "date", "location", "summary"];

fn floor_name(i: usize) -> String {
    FLOOR_NAMES.get(i).map_or_else(|| format!("Floor {i}"), |s| (*s).to_string())
}

/// Default slab thickness (mm) and beam depth including slab (mm).
pub(crate) fn structure_mm(d: &Drawing) -> (f64, f64) {
    let s = get(d, "structure");
    let slab = s.get("slabMm").and_then(Value::as_f64).filter(|n| n.is_finite() && *n > 0.0).unwrap_or(150.0);
    let beam = s.get("beamMm").and_then(Value::as_f64).filter(|n| n.is_finite() && *n > 0.0).unwrap_or(450.0);
    (slab, beam)
}

fn default_floor(i: usize, slab: f64, beam: f64) -> Value {
    json!({ "name": floor_name(i), "floorToFloor": 3.0, "ceiling": 2.85, "lintel": 2.1, "slab": slab, "beam": beam })
}

/// The stored floors, or a single default floor.
pub(crate) fn floors(d: &Drawing) -> Vec<Value> {
    let (slab, beam) = structure_mm(d);
    match get(d, "floors") {
        Value::Array(a) if !a.is_empty() => a
            .into_iter()
            .enumerate()
            .map(|(i, mut f)| {
                if f.get("slab").and_then(Value::as_f64).is_none() {
                    f["slab"] = json!(slab);
                }
                if f.get("beam").and_then(Value::as_f64).is_none() {
                    f["beam"] = json!(beam);
                }
                if f.get("name").and_then(Value::as_str).is_none() {
                    f["name"] = json!(floor_name(i));
                }
                if f.get("floorToFloor").and_then(Value::as_f64).is_none() {
                    f["floorToFloor"] = json!(3.0);
                }
                if f.get("ceiling").and_then(Value::as_f64).is_none() {
                    f["ceiling"] = json!(2.85);
                }
                if f.get("lintel").and_then(Value::as_f64).is_none() {
                    f["lintel"] = json!(2.1);
                }
                f
            })
            .collect(),
        _ => vec![default_floor(0, slab, beam)],
    }
}

/// Lintel height in metres for the active floor, or 2.1 when the floor has none.
pub(crate) fn standard_lintel(d: &Drawing) -> f64 {
    let list = floors(d);
    let chosen = get(d, "activeFloor");
    let want = chosen.as_str().unwrap_or("");
    let floor = list.iter().find(|f| f["name"].as_str() == Some(want)).unwrap_or(&list[0]);
    floor["lintel"].as_f64().filter(|n| n.is_finite() && *n > 0.0).unwrap_or(2.1)
}

fn num(v: &Value, key: &str, default: f64) -> f64 {
    v.get(key).and_then(Value::as_f64).filter(|x| x.is_finite()).unwrap_or(default)
}

fn text_detail(v: &Value) -> Option<String> {
    v.as_str().map(str::to_string).or_else(|| v.as_f64().map(|n| n.to_string()))
}

fn apply_structure(d: &mut Drawing, slab: f64, beam: f64) -> Result<()> {
    if !(slab.is_finite() && slab > 0.0 && slab < 5000.0 && beam.is_finite() && beam > 0.0 && beam < 5000.0) {
        return Err(bad("project", "slab and beam depths must be positive millimetres under 5000"));
    }
    set(d, "structure", &json!({ "slabMm": slab, "beamMm": beam }));
    // Keep each floor's slab/beam in sync with the project defaults (take-off / bridge).
    let mut fl = floors(d);
    for f in &mut fl {
        f["slab"] = json!(slab);
        f["beam"] = json!(beam);
    }
    set(d, "floors", &Value::Array(fl));
    Ok(())
}

pub(super) fn run_project(s: &mut Session, p: &Value) -> Result<Value> {
    let mut cur = match get(s.doc()?, "details") {
        Value::Object(o) => o,
        _ => Map::new(),
    };
    // Flat Project Settings form fields, or a nested `details` object.
    let mut incoming = Map::new();
    let from_details = matches!(p.get("details"), Some(Value::Object(_)));
    if let Some(Value::Object(new)) = p.get("details") {
        for (k, v) in new {
            incoming.insert(k.clone(), v.clone());
        }
    }
    for k in DETAIL_KEYS {
        if let Some(v) = p.get(k) {
            incoming.insert(k.to_string(), v.clone());
        }
    }
    if !incoming.is_empty() {
        let locked = super::companyfile::locked_detail_keys(s.doc()?);
        for (k, v) in &incoming {
            if k == "slabMm" || k == "beamMm" {
                continue;
            }
            if !DETAIL_KEYS.contains(&k.as_str()) {
                if from_details {
                    return Err(bad("project", format!("unknown detail `{k}`")));
                }
                continue;
            }
            let text = text_detail(v).ok_or_else(|| bad("project", format!("`{k}` must be text")))?;
            if text.len() > 500 {
                return Err(bad("project", format!("`{k}` is too long")));
            }
            if locked.contains(&k.as_str()) {
                let current = cur.get(k).and_then(Value::as_str).unwrap_or("");
                if text != current {
                    return Err(bad(
                        "project",
                        format!("`{k}` comes from Studio (company file) and cannot be edited here; change it in Studio and refresh with companyfile"),
                    ));
                }
                continue;
            }
            cur.insert(k.clone(), json!(text));
        }
        set(s.doc_mut()?, "details", &Value::Object(cur.clone()));
    }

    let mut structure_changed = false;
    let (mut slab, mut beam) = structure_mm(s.doc()?);
    if let Some(Value::Object(st)) = p.get("structure") {
        if let Some(n) = st.get("slabMm").and_then(Value::as_f64) {
            slab = n;
            structure_changed = true;
        }
        if let Some(n) = st.get("beamMm").and_then(Value::as_f64) {
            beam = n;
            structure_changed = true;
        }
    }
    if let Some(n) = p.get("slabMm").and_then(Value::as_f64).or_else(|| incoming.get("slabMm").and_then(Value::as_f64)) {
        slab = n;
        structure_changed = true;
    }
    if let Some(n) = p.get("beamMm").and_then(Value::as_f64).or_else(|| incoming.get("beamMm").and_then(Value::as_f64)) {
        beam = n;
        structure_changed = true;
    }
    if structure_changed {
        apply_structure(s.doc_mut()?, slab, beam)?;
    }
    let (slab, beam) = structure_mm(s.doc()?);
    let d = s.doc()?;
    Ok(json!({
        "details": cur,
        "structure": { "slabMm": slab, "beamMm": beam },
        "company": super::companyfile::company(d),
        "suite": super::companyfile::suite_status(d),
    }))
}

pub(super) fn run_floors(s: &mut Session, p: &Value) -> Result<Value> {
    let (slab_def, beam_def) = structure_mm(s.doc()?);
    let mut fl = floors(s.doc()?);
    let mut changed = false;
    if let Some(Value::Array(given)) = p.get("floors") {
        if given.is_empty() || given.len() > 60 {
            return Err(bad("floors", "between 1 and 60 floors"));
        }
        fl = given
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let prev = fl.get(i).cloned().unwrap_or_else(|| default_floor(i, slab_def, beam_def));
                json!({
                    "name": f.get("name").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| floor_name(i)),
                    "floorToFloor": num(f, "floorToFloor", num(&prev, "floorToFloor", 3.0)),
                    "ceiling": num(f, "ceiling", num(&prev, "ceiling", 2.85)),
                    "lintel": num(f, "lintel", num(&prev, "lintel", 2.1)),
                    "slab": num(f, "slab", num(&prev, "slab", slab_def)),
                    "beam": num(f, "beam", num(&prev, "beam", beam_def)),
                })
            })
            .collect();
        for f in &fl {
            let (ftf, c, l) = (num(f, "floorToFloor", 0.0), num(f, "ceiling", 0.0), num(f, "lintel", 0.0));
            if !(ftf > 0.0 && c > 0.0 && l > 0.0) {
                return Err(bad("floors", "heights must be positive"));
            }
            if c > ftf {
                return Err(bad(
                    "floors",
                    format!("floor `{}`: ceiling height is taller than the floor-to-floor height", f["name"].as_str().unwrap_or("")),
                ));
            }
            if l > c {
                return Err(bad("floors", format!("floor `{}`: lintel bottom is above the ceiling", f["name"].as_str().unwrap_or(""))));
            }
        }
        changed = true;
    }
    if let Some(n) = p.get("count").and_then(Value::as_u64) {
        if !(1..=60).contains(&n) {
            return Err(bad("floors", "`count` must be 1..60"));
        }
        let n = n as usize;
        fl.truncate(n);
        while fl.len() < n {
            let mut next = fl.last().cloned().unwrap_or_else(|| default_floor(0, slab_def, beam_def));
            next["name"] = json!(floor_name(fl.len()));
            fl.push(next);
        }
        changed = true;
    }
    if changed {
        set(s.doc_mut()?, "floors", &Value::Array(fl.clone()));
    }
    Ok(json!({ "floors": fl }))
}

fn run_beams(s: &mut Session, p: &Value) -> Result<Value> {
    let mut depths = match get(s.doc()?, "beams") {
        Value::Array(a) => a,
        _ => vec![json!(230), json!(300), json!(375), json!(450), json!(600)],
    };
    if let Some(Value::Array(given)) = p.get("depths") {
        if given.is_empty() || given.len() > 50 || given.iter().any(|v| !v.as_f64().is_some_and(|x| x.is_finite() && x > 0.0 && x < 5000.0)) {
            return Err(bad("beams", "`depths` must be 1..50 positive depths in mm"));
        }
        depths = given.clone();
        set(s.doc_mut()?, "beams", &Value::Array(depths.clone()));
    }
    Ok(json!({ "depths": depths }))
}

fn run_levels(s: &mut Session, _p: &Value) -> Result<Value> {
    let mut level = 0.0;
    let mut out = Vec::new();
    for f in floors(s.doc()?) {
        out.push(json!({
            "name": f["name"], "level": (level * 1000.0_f64).round() / 1000.0, "floorToFloor": f["floorToFloor"],
            "ceiling": f["ceiling"], "lintel": f["lintel"], "slab": f["slab"], "beam": f["beam"],
        }));
        level += num(&f, "floorToFloor", 0.0);
    }
    Ok(json!({ "levels": out }))
}

fn run_site_origin(s: &mut Session, p: &Value) -> Result<Value> {
    let mut at = s.doc()?.site_origin();
    let mut set = false;
    if let Some(arr) = p.get("point").and_then(Value::as_array) {
        let x = arr.first().and_then(Value::as_f64).filter(|n| n.is_finite()).ok_or_else(|| bad("site.origin", "`point` needs finite [x, y]"))?;
        let y = arr.get(1).and_then(Value::as_f64).filter(|n| n.is_finite()).ok_or_else(|| bad("site.origin", "`point` needs finite [x, y]"))?;
        at = aad_geom::Vec2::new(x, y);
        set = true;
    }
    if p.get("x").is_some() {
        let x = p.get("x").and_then(Value::as_f64).filter(|n| n.is_finite()).ok_or_else(|| bad("site.origin", "`x` must be a finite number"))?;
        at.x = x;
        set = true;
    }
    if p.get("y").is_some() {
        let y = p.get("y").and_then(Value::as_f64).filter(|n| n.is_finite()).ok_or_else(|| bad("site.origin", "`y` must be a finite number"))?;
        at.y = y;
        set = true;
    }
    if set {
        s.doc_mut()?.set_site_origin(at).map_err(|e| bad("site.origin", e.to_string()))?;
    }
    let origin = s.doc()?.site_origin();
    Ok(json!({ "origin": [origin.x, origin.y] }))
}

/// The two-letter code of a floor, as used in room names (`GF`, `FF`, `SF`, `TF`, then `F4`...).
pub(crate) fn floor_code(name: &str, index: usize) -> String {
    match name.trim().to_ascii_lowercase().as_str() {
        "ground" | "ground floor" => "GF".into(),
        "first" | "first floor" => "FF".into(),
        "second" | "second floor" => "SF".into(),
        "third" | "third floor" => "TF".into(),
        _ => format!("F{index}"),
    }
}

/// The name of the active floor of a single drawing: the stored choice when it is still one of the
/// floors, else the first floor. `None` when the drawing has no floors defined.
pub(crate) fn active_floor(d: &Drawing) -> Option<(usize, String)> {
    let Value::Array(list) = get(d, "floors") else { return None };
    let names: Vec<String> = list.iter().filter_map(|f| f.get("name").and_then(Value::as_str).map(str::to_string)).collect();
    let chosen = get(d, "activeFloor");
    let want = chosen.as_str().unwrap_or("");
    let i = names.iter().position(|n| n == want).or(if names.is_empty() { None } else { Some(0) })?;
    Some((i, names[i].clone()))
}

/// Room-name prefix for the active floor, or empty when floors are not defined for this drawing.
pub(crate) fn active_floor_prefix(d: &Drawing) -> String {
    active_floor(d).map(|(i, n)| floor_code(&n, i)).unwrap_or_default()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod site_origin_cmd {
    use super::*;
    use crate::Session;

    #[test]
    fn site_origin_reads_and_sets() {
        let mut s = Session::new();
        let got = s.execute("site.origin", &json!({})).unwrap();
        assert_eq!(got["origin"], json!([0.0, 0.0]));
        let set = s.execute("site.origin", &json!({ "point": [10.0, 20.0] })).unwrap();
        assert_eq!(set["origin"], json!([10.0, 20.0]));
        assert_eq!(s.doc().unwrap().site_origin(), aad_geom::Vec2::new(10.0, 20.0));
        assert!(s.execute("site.origin", &json!({ "x": null })).is_err());
        assert!(s.execute("site.origin", &json!({ "point": [1e20, 0.0] })).is_err());
    }
}
