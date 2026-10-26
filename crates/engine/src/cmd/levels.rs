//! Levels: named model spaces in one drawing (design: `docs/levels-and-sheet-sets.md`).
//!
//! `Drawing::model` holds the active level's objects, so every drafting command keeps working on
//! the active level. These commands add, rename, reorder, delete and switch levels. A drawing with
//! no explicit levels is one implicit level; the first `level.new` makes it explicit, named after
//! the current floor (else `Level 1`), and adds the new level after it.

use aad_doc::{Level, LevelId};
use serde_json::{Value, json};

use super::project;
use super::*;
use crate::{Result, Session};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("level.list", "List Levels", run_list)
            .menu(&["Architecture", "Levels", "List"])
            .params("{} → {levels: [{id, name, active, elevation, height, objects, traces}], implicit}")
            .noundo(),
        CommandSpec::new("level.new", "New Level", run_new)
            .menu(&["Architecture", "Levels", "New Level"])
            .params("{name, elevation?, height?, after? (level id or name), first? (name for the existing level when the drawing has none yet)} → {id, name, levels}"),
        CommandSpec::new("level.set", "Level Settings", run_set)
            .menu(&["Architecture", "Levels", "Level Settings"])
            .params("{level (id or name), name?, elevation?, height?, order? (0-based tab position)} → {level}"),
        CommandSpec::new("level.delete", "Delete Level", run_delete)
            .menu(&["Architecture", "Levels", "Delete Level"])
            .params("{level, force? (delete although objects or traces are on it)} → {deleted, active}"),
        CommandSpec::new("level.activate", "Activate Level", run_activate)
            .menu(&["Architecture", "Levels", "Activate Level"])
            .alias(&["levelactivate"])
            .params("{level} → {active, objects}. Makes the level the one drafting commands edit."),
    ]
}

/// A level named by id (number or numeric text) or by name.
fn find(s: &Session, cmd: &str, p: &Value, key: &str) -> Result<LevelId> {
    let d = s.doc()?;
    let v = p.get(key).ok_or_else(|| bad(cmd, format!("`{key}` (a level id or name) is required")))?;
    if let Some(n) = v.as_u64().or_else(|| v.as_str().and_then(|t| t.trim().parse().ok())) {
        let id = LevelId(u32::try_from(n).unwrap_or(u32::MAX));
        if d.level(id).is_some() {
            return Ok(id);
        }
    }
    if let Some(name) = v.as_str()
        && let Some(l) = d.level_by_name(name)
    {
        return Ok(l.id);
    }
    Err(bad(cmd, format!("no level `{v}`; the levels are {}", names(d))))
}

fn names(d: &aad_doc::Drawing) -> String {
    d.levels.iter().map(|l| l.name.as_str()).collect::<Vec<_>>().join(", ")
}

fn level_json(d: &aad_doc::Drawing, l: &Level) -> Value {
    let objects = d.level_store(l.id).map_or(0, aad_doc::EntityStore::len);
    json!({
        "id": l.id.0, "name": l.name, "active": d.active_level == Some(l.id),
        "elevation": l.elevation, "height": l.height, "objects": objects,
        "traces": l.traces.iter().map(|t| t.level.0).collect::<Vec<_>>(),
    })
}

fn all(d: &aad_doc::Drawing) -> Vec<Value> {
    d.levels.iter().map(|l| level_json(d, l)).collect()
}

fn num(p: &Value, cmd: &str, key: &str) -> Result<Option<f64>> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v.as_f64().filter(|n| n.is_finite()).map(Some).ok_or_else(|| bad(cmd, format!("`{key}` must be a finite number"))),
    }
}

fn run_list(s: &mut Session, _p: &Value) -> Result<Value> {
    let d = s.doc()?;
    if d.has_levels() {
        return Ok(json!({ "levels": all(d), "implicit": false }));
    }
    let name = implicit_name(d);
    let one = json!({
        "id": 0, "name": name, "active": true, "elevation": 0.0, "height": 0.0,
        "objects": d.model.len(), "traces": Vec::<u32>::new(),
    });
    Ok(json!({ "levels": [one], "implicit": true }))
}

/// The name an implicit level takes when it becomes explicit: the current floor, else `Level 1`.
fn implicit_name(d: &aad_doc::Drawing) -> String {
    project::active_floor(d).map(|(_, n)| n).filter(|n| aad_doc::level_name_ok(n)).unwrap_or_else(|| "Level 1".to_string())
}

fn run_new(s: &mut Session, p: &Value) -> Result<Value> {
    let name = str_param(p, "name").ok_or_else(|| bad("level.new", "`name` is required"))?.to_string();
    let elevation = num(p, "level.new", "elevation")?.unwrap_or(0.0);
    let height = num(p, "level.new", "height")?.unwrap_or(0.0);
    if s.block_edit.is_some() {
        return Err(bad("level.new", "close the block editor first"));
    }
    let first = match str_param(p, "first") {
        Some(f) => f.to_string(),
        None => implicit_name(s.doc()?),
    };
    let after = if p.get("after").is_some_and(|v| !v.is_null()) { Some(find(s, "level.new", p, "after")?) } else { None };
    let d = s.doc_mut()?;
    if !d.has_levels() {
        if first.trim().eq_ignore_ascii_case(name.trim()) {
            return Err(bad("level.new", format!("`{}` is the name of the existing level; pick another name", first.trim())));
        }
        d.materialize_levels(&first, 0.0, 0.0)?;
    }
    let id = d.add_level(&name, elevation, height, after)?;
    Ok(json!({ "id": id.0, "name": name.trim(), "levels": all(d) }))
}

fn run_set(s: &mut Session, p: &Value) -> Result<Value> {
    let id = find(s, "level.set", p, "level")?;
    let elevation = num(p, "level.set", "elevation")?;
    let height = num(p, "level.set", "height")?;
    if height.is_some_and(|h| h < 0.0) {
        return Err(bad("level.set", "`height` cannot be negative"));
    }
    let order = match p.get("order") {
        None | Some(Value::Null) => None,
        Some(v) => Some(v.as_u64().and_then(|n| usize::try_from(n).ok()).ok_or_else(|| bad("level.set", "`order` must be a whole number from 0"))?),
    };
    let d = s.doc_mut()?;
    if let Some(n) = str_param(p, "name") {
        d.rename_level(id, n)?;
    }
    if let Some(l) = d.levels.iter_mut().find(|l| l.id == id) {
        if let Some(e) = elevation {
            l.elevation = e;
        }
        if let Some(h) = height {
            l.height = h;
        }
    }
    if let Some(o) = order {
        d.move_level(id, o)?;
    }
    let out = d.level(id).map(|l| level_json(d, l)).unwrap_or(Value::Null);
    Ok(json!({ "level": out }))
}

fn run_delete(s: &mut Session, p: &Value) -> Result<Value> {
    let id = find(s, "level.delete", p, "level")?;
    if s.block_edit.is_some() {
        return Err(bad("level.delete", "close the block editor first"));
    }
    let force = bool_or(p, "force", false);
    let d = s.doc()?;
    if d.levels.len() <= 1 {
        return Err(bad("level.delete", "the last level cannot be deleted"));
    }
    let objects = d.level_store(id).map_or(0, aad_doc::EntityStore::len);
    if !force && objects > 0 {
        return Err(bad("level.delete", format!("the level has {objects} objects; pass `force` to delete them")));
    }
    if !force && d.level_is_traced(id) {
        return Err(bad("level.delete", "another level traces this one; pass `force` to delete it and those traces"));
    }
    let name = d.level(id).map(|l| l.name.clone()).unwrap_or_default();
    let was_active = d.active_level == Some(id);
    s.doc_mut()?.remove_level(id)?;
    if was_active {
        s.set_selection(Vec::new());
    }
    let d = s.doc()?;
    Ok(json!({ "deleted": name, "active": d.active_level_ref().map(|l| l.name.clone()) }))
}

fn run_activate(s: &mut Session, p: &Value) -> Result<Value> {
    let id = find(s, "level.activate", p, "level")?;
    if s.block_edit.is_some() {
        return Err(bad("level.activate", "close the block editor first"));
    }
    if s.doc()?.active_level != Some(id) {
        s.doc_mut()?.switch_level(id)?;
        s.set_selection(Vec::new());
    }
    let d = s.doc()?;
    Ok(json!({ "active": d.active_level_ref().map(|l| l.name.clone()), "objects": d.model.len() }))
}
