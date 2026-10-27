//! Sheet sets: discipline annotation on top of levels (design: `docs/levels-and-sheet-sets.md`).
//!
//! A sheet set (Working drawing, Electrical, Plumbing, Interior, False ceiling, or your own) keeps
//! one annotation store per level. `sheetset.activate` enters a *sheet view*: the active level's
//! annotation for that set is the store drawing commands write to, and the level's own objects are
//! shown for reference and cannot change. New annotation is put on the set's layers (`E-DIM`,
//! `E-TEXT`, ...) whatever layer was current. `sheetset.activate` with `null` returns to design mode.

use std::sync::Arc;

use aad_doc::{Drawing, EntityKind, SheetSet, SheetSetId};
use serde_json::{Value, json};

use super::*;
use crate::{Result, Session};

/// The sheet sets `sheetset.new {preset: true}` creates: name and layer prefix.
pub const PRESETS: [(&str, &str); 5] =
    [("Working drawing", "W-"), ("Electrical", "E-"), ("Plumbing", "P-"), ("Interior", "I-"), ("False ceiling", "FC-")];

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("sheetset.list", "List Sheet Sets", run_list)
            .menu(&["Architecture", "Sheet Sets", "List"])
            .params("{} → {sheetSets: [{id, name, layerPrefix, space, dimStyle, textStyle, active, objects}], active}")
            .noundo(),
        CommandSpec::new("sheetset.new", "New Sheet Set", run_new)
            .menu(&["Architecture", "Sheet Sets", "New Sheet Set"])
            .params("{name, layerPrefix? (default the first letter and `-`), space?, dimStyle?, textStyle?} or {preset: true} for Working drawing, Electrical, Plumbing, Interior and False ceiling → {created}"),
        CommandSpec::new("sheetset.set", "Sheet Set Settings", run_set)
            .menu(&["Architecture", "Sheet Sets", "Sheet Set Settings"])
            .params("{set (id or name), name?, layerPrefix?, space? (text or null), dimStyle? (text or null), textStyle? (text or null)} → {sheetSet}"),
        CommandSpec::new("sheetset.delete", "Delete Sheet Set", run_delete)
            .menu(&["Architecture", "Sheet Sets", "Delete Sheet Set"])
            .params("{set, force? (delete although it holds annotation)} → {deleted}"),
        CommandSpec::new("sheetset.activate", "Activate Sheet Set", run_activate)
            .menu(&["Architecture", "Sheet Sets", "Activate Sheet Set"])
            .alias(&["sheetset"])
            .params("{set: id or name | null} → {active, level, objects}. Enters the sheet view of the active level, or null returns to design mode."),
        CommandSpec::new("sheetset.next", "Next Sheet Set", run_next)
            .menu(&["Architecture", "Sheet Sets", "Next Sheet Set"])
            .key("Cmd+Shift+PageDown")
            .params("{} → {active}. Design, then each sheet set in turn, then Design again."),
        CommandSpec::new("sheetset.prev", "Previous Sheet Set", run_prev)
            .menu(&["Architecture", "Sheet Sets", "Previous Sheet Set"])
            .key("Cmd+Shift+PageUp")
            .params("{} → {active}"),
    ]
}

pub(super) fn find(s: &Session, cmd: &str, v: &Value) -> Result<SheetSetId> {
    let d = s.doc()?;
    if let Some(n) = v.as_u64().or_else(|| v.as_str().and_then(|t| t.trim().parse().ok())) {
        let id = SheetSetId(u32::try_from(n).unwrap_or(u32::MAX));
        if d.sheet_set(id).is_some() {
            return Ok(id);
        }
    }
    if let Some(name) = v.as_str()
        && let Some(x) = d.sheet_set_by_name(name)
    {
        return Ok(x.id);
    }
    let names = d.sheet_sets.iter().map(|x| x.name.as_str()).collect::<Vec<_>>().join(", ");
    Err(bad(cmd, format!("no sheet set `{v}`; the sheet sets are {names}")))
}

fn objects_of(d: &Drawing, set: &SheetSet) -> usize {
    let in_model = if d.active_sheet == Some(set.id) { d.model.len() } else { 0 };
    in_model + set.annotations.values().map(aad_doc::EntityStore::len).sum::<usize>()
}

fn set_json(d: &Drawing, x: &SheetSet) -> Value {
    json!({
        "id": x.id.0, "name": x.name, "layerPrefix": x.layer_prefix, "space": x.space,
        "dimStyle": x.dim_style, "textStyle": x.text_style,
        "active": d.active_sheet == Some(x.id), "objects": objects_of(d, x),
    })
}

fn run_list(s: &mut Session, _p: &Value) -> Result<Value> {
    let d = s.doc()?;
    Ok(
        json!({ "sheetSets": d.sheet_sets.iter().map(|x| set_json(d, x)).collect::<Vec<_>>(), "active": d.active_sheet_ref().map(|x| x.name.clone()) }),
    )
}

/// A text parameter that may be null (clears it): `None` = not given, `Some(None)` = clear.
fn opt_text(p: &Value, cmd: &str, key: &str) -> Result<Option<Option<String>>> {
    match p.get(key) {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(None)),
        Some(Value::String(t)) if t.chars().count() <= 255 => Ok(Some(Some(t.clone()))),
        Some(_) => Err(bad(cmd, format!("`{key}` is text of at most 255 characters, or null"))),
    }
}

fn default_prefix(name: &str) -> String {
    name.trim().chars().next().filter(|c| c.is_alphanumeric()).map(|c| format!("{}-", c.to_ascii_uppercase())).unwrap_or_default()
}

fn run_new(s: &mut Session, p: &Value) -> Result<Value> {
    if bool_or(p, "preset", false) {
        let d = s.doc_mut()?;
        let mut created = Vec::new();
        for (name, prefix) in PRESETS {
            if d.sheet_set_by_name(name).is_none() {
                d.add_sheet_set(name, prefix)?;
                created.push(name);
            }
        }
        return Ok(json!({ "created": created }));
    }
    let name = str_param(p, "name").ok_or_else(|| bad("sheetset.new", "`name` (or `preset: true`) is required"))?.to_string();
    let prefix = match p.get("layerPrefix") {
        None | Some(Value::Null) => default_prefix(&name),
        Some(Value::String(t)) => t.clone(),
        Some(_) => return Err(bad("sheetset.new", "`layerPrefix` is text")),
    };
    let (space, dim, text) =
        (opt_text(p, "sheetset.new", "space")?, opt_text(p, "sheetset.new", "dimStyle")?, opt_text(p, "sheetset.new", "textStyle")?);
    let d = s.doc_mut()?;
    let id = d.add_sheet_set(&name, &prefix)?;
    if let Some(x) = d.sheet_sets.iter_mut().find(|x| x.id == id) {
        x.space = space.flatten();
        x.dim_style = dim.flatten();
        x.text_style = text.flatten();
    }
    Ok(json!({ "created": [name.trim()], "id": id.0 }))
}

fn run_set(s: &mut Session, p: &Value) -> Result<Value> {
    let id = find(s, "sheetset.set", p.get("set").ok_or_else(|| bad("sheetset.set", "`set` (a sheet set id or name) is required"))?)?;
    let (space, dim, text) =
        (opt_text(p, "sheetset.set", "space")?, opt_text(p, "sheetset.set", "dimStyle")?, opt_text(p, "sheetset.set", "textStyle")?);
    let prefix = match p.get("layerPrefix") {
        None => None,
        Some(Value::String(t)) if aad_doc::layer_prefix_ok(t) => Some(t.clone()),
        Some(_) => return Err(bad("sheetset.set", "`layerPrefix` is at most 8 characters without spaces")),
    };
    let d = s.doc_mut()?;
    if let Some(n) = str_param(p, "name") {
        d.rename_sheet_set(id, n)?;
    }
    if let Some(x) = d.sheet_sets.iter_mut().find(|x| x.id == id) {
        if let Some(pre) = prefix {
            x.layer_prefix = pre;
        }
        if let Some(v) = space {
            x.space = v;
        }
        if let Some(v) = dim {
            x.dim_style = v;
        }
        if let Some(v) = text {
            x.text_style = v;
        }
    }
    let out = d.sheet_set(id).map(|x| set_json(d, x)).unwrap_or(Value::Null);
    Ok(json!({ "sheetSet": out }))
}

fn run_delete(s: &mut Session, p: &Value) -> Result<Value> {
    let id = find(s, "sheetset.delete", p.get("set").ok_or_else(|| bad("sheetset.delete", "`set` is required"))?)?;
    if s.block_edit.is_some() {
        return Err(bad("sheetset.delete", "close the block editor first"));
    }
    let d = s.doc()?;
    let objects = d.sheet_set(id).map_or(0, |x| objects_of(d, x));
    if objects > 0 && !bool_or(p, "force", false) {
        return Err(bad("sheetset.delete", format!("the sheet set holds {objects} annotation objects; pass `force` to delete them")));
    }
    let name = d.sheet_set(id).map(|x| x.name.clone()).unwrap_or_default();
    let was_active = d.active_sheet == Some(id);
    s.doc_mut()?.remove_sheet_set(id)?;
    if was_active {
        s.set_selection(Vec::new());
    }
    Ok(json!({ "deleted": name }))
}

fn state_json(s: &Session) -> Result<Value> {
    let d = s.doc()?;
    Ok(json!({
        "active": d.active_sheet_ref().map(|x| x.name.clone()),
        "level": d.active_level_ref().map(|l| l.name.clone()),
        "objects": d.model.len(),
    }))
}

fn go(s: &mut Session, cmd: &str, to: Option<SheetSetId>) -> Result<Value> {
    if s.block_edit.is_some() {
        return Err(bad(cmd, "close the block editor first"));
    }
    if s.doc()?.active_sheet == to {
        return state_json(s);
    }
    if let Some(id) = to {
        if !s.doc()?.has_levels() {
            // A sheet view is keyed by level: the single implicit level becomes explicit first.
            let name = super::levels::implicit_name(s.doc()?);
            s.doc_mut()?.materialize_levels(&name, 0.0, 0.0)?;
        }
        s.doc_mut()?.enter_sheet_view(id)?;
    } else {
        s.doc_mut()?.leave_sheet_view()?;
    }
    s.set_selection(Vec::new());
    state_json(s)
}

fn run_activate(s: &mut Session, p: &Value) -> Result<Value> {
    let to = match p.get("set") {
        None | Some(Value::Null) => None,
        Some(v) => Some(find(s, "sheetset.activate", v)?),
    };
    go(s, "sheetset.activate", to)
}

fn cycle(s: &mut Session, cmd: &str, delta: isize) -> Result<Value> {
    let d = s.doc()?;
    if d.sheet_sets.is_empty() {
        return Err(bad(cmd, "the drawing has no sheet sets; create them with sheetset.new"));
    }
    // Design is position 0, the sheet sets follow in order.
    let n = d.sheet_sets.len() as isize + 1;
    let at = d.active_sheet.and_then(|a| d.sheet_sets.iter().position(|x| x.id == a)).map_or(0, |i| i as isize + 1);
    let to = (at + delta).rem_euclid(n);
    let target = if to == 0 { None } else { d.sheet_sets.get((to - 1) as usize).map(|x| x.id) };
    go(s, cmd, target)
}

fn run_next(s: &mut Session, _p: &Value) -> Result<Value> {
    cycle(s, "sheetset.next", 1)
}

fn run_prev(s: &mut Session, _p: &Value) -> Result<Value> {
    cycle(s, "sheetset.prev", -1)
}

// ---------------- what a sheet view does to the commands ----------------

/// The layer new annotation of this kind goes on in a sheet set with `prefix`.
fn layer_for(prefix: &str, kind: &EntityKind) -> String {
    let what = match kind {
        EntityKind::Dimension(_) => "DIM",
        EntityKind::Text(_) | EntityKind::MText(_) => "TEXT",
        EntityKind::MLeader(_) => "LEADER",
        EntityKind::Hatch(_) => "HATCH",
        _ => "ANNO",
    };
    format!("{prefix}{what}")
}

/// After a command in a sheet view: objects it added to the annotation store that are not already
/// on one of the sheet set's layers move to the set's layer for their kind (created on first use).
/// A drawing that is not in a sheet view, or whose sheet view the command changed, is left alone.
pub(crate) fn route_new_objects(doc: &mut Arc<Drawing>, before: &Drawing) {
    let Some(set) = doc.active_sheet_ref() else { return };
    if before.target() != doc.target() || set.layer_prefix.is_empty() || doc.model.same_as(&before.model) {
        return;
    }
    let prefix = set.layer_prefix.clone();
    let moves: Vec<(aad_doc::Handle, String)> = doc
        .model
        .changed_since(&before.model)
        .into_iter()
        .filter(|e| before.model.get(e.handle).is_none() && !e.common.layer.to_ascii_uppercase().starts_with(&prefix.to_ascii_uppercase()))
        .map(|e| (e.handle, layer_for(&prefix, &e.kind)))
        .collect();
    if moves.is_empty() {
        return;
    }
    let d = Arc::make_mut(doc);
    for (h, layer) in moves {
        d.ensure_layer(&layer);
        d.model.modify(h, |e| e.common.layer = layer.clone());
    }
}

/// The message when a command changed the level shown for reference in a sheet view.
pub(crate) fn reference_changed(cmd: &str, before: &Drawing, after: &Drawing) -> Option<String> {
    // Commands that rewrite objects of every level on purpose, and the ones that switch views.
    if cmd.starts_with("layer")
        || cmd.starts_with("level.")
        || cmd.starts_with("sheetset.")
        || matches!(cmd, "audit" | "recover" | "purge" | "laytrans" | "layermap")
    {
        return None;
    }
    if before.active_sheet.is_none() || before.target() != after.target() {
        return None;
    }
    let (b, a) = (before.reference_store()?, after.reference_store()?);
    if b.same_as(a) {
        return None;
    }
    Some(format!(
        "level `{}` is shown for reference in sheet set `{}`: its objects cannot be changed here. Nothing was changed. Switch to Design (sheetset.activate with null) to edit the level",
        before.active_level_ref().map_or("?", |l| l.name.as_str()),
        before.active_sheet_ref().map_or("?", |x| x.name.as_str()),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use aad_doc::{Common, Entity, Line};
    use aad_geom::Vec3;

    fn sheet_view() -> Drawing {
        let mut d = Drawing::new_metric();
        d.materialize_levels("Ground", 0.0, 0.0).unwrap();
        let set = d.add_sheet_set("Electrical", "E-").unwrap();
        d.enter_sheet_view(set).unwrap();
        d
    }

    #[test]
    fn a_command_that_changed_the_level_shown_for_reference_is_reported_by_name() {
        let before = sheet_view();
        let mut after = before.clone();
        let h = after.new_handle();
        let kind = EntityKind::Line(Line { a: Vec3::new(0.0, 0.0, 0.0), b: Vec3::new(1.0, 0.0, 0.0) });
        after.levels[0].entities.push(Entity { handle: h, common: Common::default(), kind });
        let msg = reference_changed("erase", &before, &after).expect("the reference level changed");
        assert!(msg.contains("Ground") && msg.contains("Electrical"), "{msg}");
        // Commands that rewrite every level on purpose, and no change at all, are not reported.
        for cmd in ["layer.set", "level.moveto", "sheetset.activate", "audit", "purge"] {
            assert!(reference_changed(cmd, &before, &after).is_none(), "{cmd}");
        }
        assert!(reference_changed("erase", &before, &before).is_none());
        // Annotation changes are what a sheet view is for.
        let mut annotated = before.clone();
        let h = annotated.new_handle();
        let kind = EntityKind::Line(Line { a: Vec3::new(0.0, 0.0, 0.0), b: Vec3::new(1.0, 0.0, 0.0) });
        annotated.model.push(Entity { handle: h, common: Common::default(), kind });
        assert!(reference_changed("line", &before, &annotated).is_none());
    }

    #[test]
    fn leaving_the_view_is_not_a_reference_change() {
        let before = sheet_view();
        let mut after = before.clone();
        after.leave_sheet_view().unwrap();
        assert!(reference_changed("erase", &before, &after).is_none(), "the target changed, so the check does not apply");
    }
}
