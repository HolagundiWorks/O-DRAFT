//! Declare a rectangular region of the drawing as a floor: every object in that part is assigned
//! to that floor. The assignment is stored with the drawing (`floorRegions` / `entityFloors`), so
//! take-off and export can filter by floor. `floor.export` writes the assigned objects as a
//! separate DXF/DWG; `floor.xref` does the same and attaches the file as an xref so each floor
//! can live as its own drawing while the host still shows them together.

use aad_doc::{Drawing, Handle};
use aad_geom::{Bounds2, Vec2};
use serde_json::{Map, Value, json};

use super::*;
use crate::{Accept, Input, Interactive, Prompt, Result, Session, Step};

const KEY_REGIONS: &str = "floorRegions";
const KEY_ENTITIES: &str = "entityFloors";
const MAX_NAME: usize = 64;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("floor.declare", "Declare Floor Region", run_declare)
            .menu(&["Architecture", "Floors", "Declare Floor Region"])
            .alias(&["floorregion", "fdeclare"])
            .params(
                "{name, window?: [[x,y],[x,y]], crossing?: bool (default true), handles?: [hex], \
                 add?: bool (keep existing assignments for this floor and merge)} \
                 → {name, entities, window?, handles}",
            )
            .interactive(|_| Ok(Box::new(DeclareM { name: None, first: None }))),
        CommandSpec::new("floor.refresh", "Refresh Floor Regions", run_refresh)
            .menu(&["Architecture", "Floors", "Refresh Floor Regions"])
            .alias(&["frefresh"])
            .params("{name?: one floor (default every declared region)} → {floors: [{name, entities}]}"),
        CommandSpec::new("floor.regions", "Floor Regions", run_regions)
            .menu(&["Architecture", "Floors", "Floor Regions"])
            .alias(&["fregions"])
            .params("{name?} → {regions: [{name, window, entities, crossing}]}")
            .noundo(),
        CommandSpec::new("floor.export", "Export Floor Drawing", run_export)
            .menu(&["Architecture", "Floors", "Export Floor Drawing"])
            .alias(&["fexport"])
            .params(
                "{name, path? (default <drawing>-<floor>.dxf beside the saved drawing)} \
                 → {name, path, entities, bytes}",
            )
            .noundo(),
        CommandSpec::new("floor.xref", "Floor as Xref", run_xref)
            .menu(&["Architecture", "Floors", "Floor as Xref"])
            .alias(&["fxref"])
            .params(
                "{name, path? (written then attached), at?: [x,y], scale?, rotation?, overlay?: bool, \
                 attach?: bool (default true)} → {name, path, entities, xref?}",
            )
            .noundo(),
    ]
}

fn regions_store(d: &Drawing) -> Value {
    project::get(d, KEY_REGIONS)
}

fn entity_map(d: &Drawing) -> Map<String, Value> {
    match project::get(d, KEY_ENTITIES) {
        Value::Object(o) => o,
        _ => Map::new(),
    }
}

fn ensure_floor_name(s: &mut Session, name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() || name.len() > MAX_NAME || name.chars().any(|c| c.is_control()) {
        return Err(bad("floor.declare", format!("`name` must be 1..{MAX_NAME} characters without control characters")));
    }
    let mut fl = project::floors(s.doc()?);
    let exists = fl.iter().any(|f| f.get("name").and_then(Value::as_str).is_some_and(|n| n.eq_ignore_ascii_case(name)));
    if !exists {
        let mut row = json!({ "name": name, "floorToFloor": 3.0, "ceiling": 2.85, "lintel": 2.1, "slab": 150, "beam": 450 });
        if let Some(first) = fl.first() {
            for k in ["floorToFloor", "ceiling", "lintel", "slab", "beam"] {
                if let Some(v) = first.get(k) {
                    row[k] = v.clone();
                }
            }
            row["name"] = json!(name);
        }
        fl.push(row);
        project::set(s.doc_mut()?, "floors", &Value::Array(fl));
    }
    let canonical = project::floors(s.doc()?)
        .iter()
        .find_map(|f| f.get("name").and_then(Value::as_str).filter(|n| n.eq_ignore_ascii_case(name)).map(str::to_string))
        .unwrap_or_else(|| name.to_string());
    project::set(s.doc_mut()?, "activeFloor", &json!(canonical));
    Ok(canonical)
}

fn window_of(p: &Value) -> Result<Option<(Bounds2, bool)>> {
    let Some(pts) = points_param(p, "window") else { return Ok(None) };
    if pts.len() != 2 {
        return Err(bad("floor.declare", "`window` needs two corners [[x,y],[x,y]]"));
    }
    Ok(Some((Bounds2::new(pts[0], pts[1]), bool_or(p, "crossing", true))))
}

fn handles_in_window(s: &Session, bx: Bounds2, crossing: bool) -> Vec<Handle> {
    crate::select::select_window(s.doc().unwrap(), &s.space(), bx, crossing)
}

fn assign(s: &mut Session, name: &str, handles: &[Handle], window: Option<(Bounds2, bool)>, add: bool) -> Result<Value> {
    let name = ensure_floor_name(s, name)?;
    let mut map = entity_map(s.doc()?);
    if !add {
        map.retain(|_, v| v.as_str().map(|n| !n.eq_ignore_ascii_case(&name)).unwrap_or(true));
    }
    for h in handles {
        map.insert(h.hex(), json!(name));
    }
    project::set(s.doc_mut()?, KEY_ENTITIES, &Value::Object(map));

    let mut store = match regions_store(s.doc()?) {
        Value::Object(o) => o,
        _ => {
            let mut o = Map::new();
            o.insert("version".into(), json!(1));
            o.insert("regions".into(), json!([]));
            o
        }
    };
    let mut regions = store.get("regions").and_then(Value::as_array).cloned().unwrap_or_default();
    regions.retain(|r| r.get("name").and_then(Value::as_str).map(|n| !n.eq_ignore_ascii_case(&name)).unwrap_or(true));
    // Full assignment lives in entityFloors. The region stores the window (for refresh) and,
    // when there is no window, the handle list so refresh can re-apply without a box.
    let mut region = json!({
        "name": name,
        "entities": handles.len(),
    });
    if let Some((bx, crossing)) = window {
        region["window"] = json!([[bx.min.x, bx.min.y], [bx.max.x, bx.max.y]]);
        region["crossing"] = json!(crossing);
    } else {
        region["handles"] = json!(handles.iter().map(|h| h.hex()).collect::<Vec<_>>());
    }
    regions.push(region);
    store.insert("regions".into(), Value::Array(regions));
    project::set(s.doc_mut()?, KEY_REGIONS, &Value::Object(store));

    s.set_selection(handles.to_vec());
    s.echo(format!("Floor `{name}`: {} object(s) assigned.", handles.len()));
    let shown: Vec<String> = handles.iter().take(50).map(|h| h.hex()).collect();
    Ok(json!({
        "name": name,
        "entities": handles.len(),
        "handles": shown,
        "truncated": handles.len() > 50,
        "window": window.map(|(bx, _)| json!([[bx.min.x, bx.min.y], [bx.max.x, bx.max.y]])),
    }))
}

fn run_declare(s: &mut Session, p: &Value) -> Result<Value> {
    let name = str_param(p, "name").ok_or_else(|| bad("floor.declare", "`name` is required"))?;
    let add = bool_or(p, "add", false);
    let win = window_of(p)?;
    let mut handles = if p.get("handles").is_some() || p.get("handle").is_some() {
        targets(s, p)?
    } else if let Some((bx, crossing)) = win {
        handles_in_window(s, bx, crossing)
    } else {
        let sel = s.selection();
        if sel.is_empty() {
            return Err(bad("floor.declare", "give `window` [[x,y],[x,y]], `handles`, or a selection"));
        }
        sel
    };
    handles.sort_by_key(|h| h.0);
    handles.dedup();
    assign(s, name, &handles, win, add)
}

fn run_refresh(s: &mut Session, p: &Value) -> Result<Value> {
    let store = regions_store(s.doc()?);
    let regions = store.get("regions").and_then(Value::as_array).cloned().unwrap_or_default();
    if regions.is_empty() {
        return Err(bad("floor.refresh", "no floor regions declared yet; run `floor.declare` first"));
    }
    let only = str_param(p, "name");
    let mut out = Vec::new();
    for r in &regions {
        let Some(name) = r.get("name").and_then(Value::as_str) else { continue };
        if only.is_some_and(|n| !n.eq_ignore_ascii_case(name)) {
            continue;
        }
        let crossing = r.get("crossing").and_then(Value::as_bool).unwrap_or(true);
        let (handles, win) = if let Some(w) = r.get("window").and_then(Value::as_array) {
            let a = w.first().and_then(point_value);
            let b = w.get(1).and_then(point_value);
            match (a, b) {
                (Some(a), Some(b)) => {
                    let bx = Bounds2::new(a, b);
                    (handles_in_window(s, bx, crossing), Some((bx, crossing)))
                }
                _ => (Vec::new(), None),
            }
        } else {
            // Prefer the full entityFloors map (region.handles may be absent or a legacy sample).
            let map = entity_map(s.doc()?);
            let mut hs: Vec<Handle> = map
                .iter()
                .filter(|(_, v)| v.as_str().is_some_and(|n| n.eq_ignore_ascii_case(name)))
                .filter_map(|(k, _)| Handle::parse_hex(k))
                .collect();
            if hs.is_empty() {
                hs = r
                    .get("handles")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|v| v.as_str().and_then(Handle::parse_hex))
                    .collect();
            }
            (hs, None)
        };
        let v = assign(s, name, &handles, win, false)?;
        out.push(v);
    }
    Ok(json!({ "floors": out }))
}

fn run_regions(s: &mut Session, p: &Value) -> Result<Value> {
    let store = regions_store(s.doc()?);
    let mut regions = store.get("regions").and_then(Value::as_array).cloned().unwrap_or_default();
    if let Some(n) = str_param(p, "name") {
        regions.retain(|r| r.get("name").and_then(Value::as_str).is_some_and(|x| x.eq_ignore_ascii_case(n)));
    }
    let map = entity_map(s.doc()?);
    for r in &mut regions {
        if let Some(name) = r.get("name").and_then(Value::as_str) {
            let n = map.values().filter(|v| v.as_str().is_some_and(|x| x.eq_ignore_ascii_case(name))).count();
            r["entities"] = json!(n);
        }
    }
    Ok(json!({ "regions": regions }))
}

fn handles_of_floor(d: &Drawing, name: &str) -> Result<Vec<Handle>> {
    let map = entity_map(d);
    let mut hs: Vec<Handle> = map
        .iter()
        .filter(|(_, v)| v.as_str().is_some_and(|n| n.eq_ignore_ascii_case(name)))
        .filter_map(|(k, _)| Handle::parse_hex(k))
        .collect();
    if hs.is_empty()
        && let Value::Object(store) = regions_store(d)
        && let Some(regions) = store.get("regions").and_then(Value::as_array)
    {
        for r in regions {
            if r.get("name").and_then(Value::as_str).is_some_and(|n| n.eq_ignore_ascii_case(name)) {
                if let Some(list) = r.get("handles").and_then(Value::as_array) {
                    hs = list.iter().filter_map(|v| v.as_str().and_then(Handle::parse_hex)).collect();
                }
                break;
            }
        }
    }
    hs.sort_by_key(|h| h.0);
    hs.dedup();
    if hs.is_empty() {
        return Err(bad("floor.export", format!("no objects assigned to floor `{name}`; run `floor.declare` first")));
    }
    Ok(hs)
}

fn default_export_path(s: &Session, name: &str) -> Result<String> {
    Ok(s.state()?
        .path
        .as_ref()
        .map(|p| {
            let p = std::path::Path::new(p);
            p.with_file_name(format!(
                "{}-{}.dxf",
                p.file_stem().and_then(|s| s.to_str()).unwrap_or("floor"),
                safe_file(name)
            ))
            .to_string_lossy()
            .to_string()
        })
        .unwrap_or_else(|| format!("floor-{}.dxf", safe_file(name))))
}

fn safe_file(raw: &str) -> String {
    let mut out = String::new();
    for c in raw.trim().chars() {
        if c.is_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    let s = out.trim_matches('_');
    if s.is_empty() { "floor".into() } else { s.chars().take(40).collect() }
}

fn run_export(s: &mut Session, p: &Value) -> Result<Value> {
    let name = str_param(p, "name").ok_or_else(|| bad("floor.export", "`name` is required"))?;
    let path = str_param(p, "path").map(str::to_string).unwrap_or(default_export_path(s, name)?);
    let handles = handles_of_floor(s.doc()?, name)?;
    let hexes: Vec<Value> = handles.iter().map(|h| json!(h.hex())).collect();
    let r = s.execute("wblock", &json!({ "path": path, "handles": hexes }))?;
    Ok(json!({
        "name": name,
        "path": r.get("path").cloned().unwrap_or(json!(path)),
        "entities": r.get("entities").cloned().unwrap_or(json!(handles.len())),
        "bytes": r.get("bytes").cloned().unwrap_or(Value::Null),
    }))
}

fn run_xref(s: &mut Session, p: &Value) -> Result<Value> {
    let name = str_param(p, "name").ok_or_else(|| bad("floor.xref", "`name` is required"))?;
    let exported = run_export(s, p)?;
    let path = exported["path"].as_str().unwrap_or("").to_string();
    if !bool_or(p, "attach", true) {
        return Ok(exported);
    }
    let mut attach = json!({
        "path": path,
        "name": format!("FLOOR-{}", safe_file(name)),
    });
    if let Some(at) = point_param(p, "at") {
        attach["at"] = json!([at.x, at.y]);
    }
    if let Some(v) = p.get("scale") {
        attach["scale"] = v.clone();
    }
    if let Some(v) = p.get("rotation") {
        attach["rotation"] = v.clone();
    }
    if let Some(v) = p.get("overlay") {
        attach["overlay"] = v.clone();
    }
    let xref = s.execute("xref.attach", &attach)?;
    Ok(json!({
        "name": name,
        "path": path,
        "entities": exported["entities"],
        "bytes": exported["bytes"],
        "xref": xref,
    }))
}

struct DeclareM {
    name: Option<String>,
    first: Option<Vec2>,
}

impl Interactive for DeclareM {
    fn name(&self) -> &'static str {
        "FLOOR.DECLARE"
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        if self.name.is_none() {
            Prompt::new("Floor name", Accept::TEXT)
        } else if self.first.is_none() {
            Prompt::new("First corner of floor region", Accept::POINT)
        } else {
            Prompt::new("Opposite corner", Accept::POINT)
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        match (&self.name, self.first, i) {
            (None, _, Input::Text(t)) => {
                let t = t.trim().to_string();
                if t.is_empty() {
                    s.echo("Enter a floor name.");
                    return Ok(Step::Continue);
                }
                self.name = Some(t);
                Ok(Step::Continue)
            }
            (Some(_), None, Input::Point(p)) => {
                self.first = Some(p);
                Ok(Step::Continue)
            }
            (Some(name), Some(a), Input::Point(b)) => {
                let name = name.clone();
                assign(s, &name, &handles_in_window(s, Bounds2::new(a, b), true), Some((Bounds2::new(a, b), true)), false)?;
                Ok(Step::Done)
            }
            (_, _, Input::Enter | Input::Cancel) => Ok(Step::Done),
            _ => Ok(Step::Continue),
        }
    }
}
