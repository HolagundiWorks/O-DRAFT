//! The project wizard's one step: details, units, floors, layer sets, standard tabs and symbols
//! applied to a new drawing together. It is the same work as `project`, `drawing.units`,
//! `level.setup`, `hcwlayers` and `sheetset.standard`, run in order inside one undo step; if any part
//! is refused the drawing is put back as it was. The window that collects the answers is the
//! shell's; this command is what scripts and the control channel call too.

use serde_json::{Value, json};

use super::*;
use crate::{Result, Session};

/// The architectural layer sets `hcwlayers` knows (`standard`, `permit`, `presentation`).
const LAYER_SETS: [&str; 3] = ["standard", "permit", "presentation"];

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("project.wizard", "New Project Wizard", run_wizard)
            .menu(&["File", "New Project Wizard"])
            .alias(&["projectwizard"])
            .params("{details?: {title, owner, location, address, pid, siteArea, plotUse, authority, architect, engineer, date, summary}, units?: mm|cm|m|in|ft (the drawing unit; sets the unit only), floors?: [{name, height?, elevation?}] | {count (1..60, default 1), basements?, terrace?, floorToFloor? (metres, 2..30, default 3)}, levels?: bool (default true: one level per floor), slabMm?, beamMm?, layers?: [\"standard\"|\"permit\"|\"presentation\"] (default [\"standard\"]), sheetSets?: bool | [names] (default true: Working drawing, Electrical, Plumbing, Interior, False ceiling with their layers and mapping), symbols?: bool (default true)} → {floors, levels, layersCreated, sheetSets, blocksCreated}. For a new drawing only: one undo step, and a refused part changes nothing."),
    ]
}

fn run_wizard(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "project.wizard";
    if s.block_edit.is_some() {
        return Err(bad(CMD, "close the block editor first"));
    }
    {
        let d = s.doc()?;
        if d.has_levels() || !d.sheet_sets.is_empty() {
            return Err(bad(CMD, "this drawing already has levels or sheet sets; run the wizard on a new drawing"));
        }
    }
    let backup = s.doc()?.clone();
    let result = apply(s, p);
    if result.is_err() {
        // A part was refused: the drawing goes back exactly as it was.
        *s.doc_mut()? = backup;
    }
    result
}

fn apply(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "project.wizard";
    let layer_sets: Vec<&str> = match p.get("layers") {
        None => vec!["standard"],
        Some(Value::Null | Value::Bool(false)) => Vec::new(),
        Some(Value::Array(a)) if a.len() <= LAYER_SETS.len() => {
            let mut out = Vec::new();
            for v in a {
                let name = v.as_str().map(str::to_ascii_lowercase).unwrap_or_default();
                let known =
                    LAYER_SETS.iter().find(|k| **k == name).ok_or_else(|| bad(CMD, "`layers` names are standard, permit and presentation"))?;
                if !out.contains(known) {
                    out.push(*known);
                }
            }
            out
        }
        Some(_) => return Err(bad(CMD, "`layers` is a list of layer set names")),
    };
    let tabs: Option<Vec<String>> = match p.get("sheetSets") {
        None | Some(Value::Bool(true)) => Some(super::sheetstd::tab_names().map(|(n, _)| n.to_string()).collect()),
        Some(Value::Null | Value::Bool(false)) => None,
        Some(Value::Array(a)) if a.len() <= 16 => {
            let names: Vec<String> = a.iter().filter_map(|v| v.as_str().map(|t| t.trim().to_string())).collect();
            if let Some(bad_name) = names.iter().find(|n| !super::sheetstd::tab_names().any(|(t, _)| t.eq_ignore_ascii_case(n))) {
                return Err(bad(CMD, format!("`{bad_name}` is not a standard tab")));
            }
            Some(names)
        }
        Some(_) => return Err(bad(CMD, "`sheetSets` is true, false or a list of standard tab names")),
    };
    let symbols = bool_or(p, "symbols", true);
    let floors_param = p.get("floors").cloned().unwrap_or_else(|| json!({ "count": 1 }));
    // Check the floors before anything changes.
    let floors = super::levels::setup_floors(&json!({ "floors": floors_param }))?;
    let details = match p.get("details") {
        None | Some(Value::Null) => None,
        Some(v @ Value::Object(_)) => Some(v.clone()),
        Some(_) => return Err(bad(CMD, "`details` is an object of project details")),
    };

    if let Some(units) = str_param(p, "units") {
        super::annot::run_drawing_units(s, &json!({ "units": units, "mode": "metadata" }))?;
    }
    let mut project = serde_json::Map::new();
    if let Some(d) = details {
        project.insert("details".into(), d);
    }
    for key in ["slabMm", "beamMm"] {
        if let Some(v) = p.get(key) {
            project.insert(key.into(), v.clone());
        }
    }
    if !project.is_empty() {
        super::project::run_project(s, &Value::Object(project))?;
    }
    let levels = if bool_or(p, "levels", true) {
        let r = super::levels::run_setup(s, &json!({ "floors": floors_param, "sheetSets": false }))?;
        r["created"].clone()
    } else {
        super::project::run_floors(s, &json!({ "floors": floors }))?;
        json!([])
    };
    let mut layers_created = 0u64;
    for set in &layer_sets {
        layers_created += super::hcw::run_layers(s, &json!({ "set": set }))?["created"].as_u64().unwrap_or(0);
    }
    let (mut sheet_sets, mut blocks_created) = (Vec::new(), 0usize);
    if let Some(names) = tabs {
        let mut d = s.doc()?.clone();
        let r = super::sheetstd::apply_standard(&mut d, Some(&names), true, symbols)?;
        *s.doc_mut()? = d;
        sheet_sets = r.sets;
        layers_created += r.layers_created as u64;
        blocks_created = r.blocks_created;
    }
    Ok(json!({ "floors": floors.len(), "levels": levels, "layersCreated": layers_created, "sheetSets": sheet_sets, "blocksCreated": blocks_created }))
}
