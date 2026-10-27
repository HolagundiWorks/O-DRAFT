//! O-Dynamo companion bridge (named pipe `ODraft.Dynamo` on Windows).
//!
//! Commands talk JSON-lines to the companion host. Context (`context.levels`,
//! `context.project`) is pushed on connect; `companion.evaluate` returns lines,
//! circles and arcs. `companion.subscribe` keeps a background reader that stores
//! pushed `evaluation` messages into [`crate::Session::companion`] for preview.
//!
//! `companion.commit` writes evaluate output onto named levels (Phase 3) and
//! places block inserts / title-block fields (Phase 4). Ghost overlay rendering
//! is still deferred. Non-Windows builds return a clear "not available" error
//! for pipe methods (wasm stays green); commit from a supplied `result` works
//! on every target.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use aad_doc::{DimKind, Dimension, EditTarget, EntityKind, HAlign, VAlign};
use aad_geom::{Arc as GeomArc, Vec2, Vec3};
use serde_json::{Value, json};

use super::draw::make_text;
use super::hcw::{add_on, ensure_layer};
use super::helpers::{arc, circle, line, v3};
use super::project;
use super::*;
use crate::{Result, Session};

const DEFAULT_PIPE: &str = "ODraft.Dynamo";
const MAX_PIPE_NAME: usize = 64;
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
const MAX_COMMIT_ITEMS: usize = 20_000;
const MAX_LEVEL_NAME: usize = 64;

/// Connection and preview state for the O-Dynamo companion.
#[derive(Clone)]
pub struct CompanionState {
    pub pipe_name: String,
    pub connected: bool,
    pub subscribed: bool,
    /// Last evaluate result or pushed evaluation (`result` object).
    pub preview: Option<Value>,
    slot: Arc<Mutex<Option<Value>>>,
    stop: Arc<AtomicBool>,
    next_id: Arc<AtomicU64>,
}

impl Default for CompanionState {
    fn default() -> Self {
        Self {
            pipe_name: DEFAULT_PIPE.into(),
            connected: false,
            subscribed: false,
            preview: None,
            slot: Arc::new(Mutex::new(None)),
            stop: Arc::new(AtomicBool::new(true)),
            next_id: Arc::new(AtomicU64::new(1)),
        }
    }
}

impl CompanionState {
    fn pull_slot(&mut self) {
        let Ok(guard) = self.slot.lock() else {
            return;
        };
        if let Some(v) = guard.as_ref() {
            self.preview = Some(v.clone());
        }
    }
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("companion.connect", "Connect Companion", run_connect)
            .menu(&["Architecture", "Companion", "Connect"])
            .params("{pipe?: string (default ODraft.Dynamo)} → {connected, pipe, levels, project}")
            .noundo(),
        CommandSpec::new("companion.disconnect", "Disconnect Companion", run_disconnect)
            .menu(&["Architecture", "Companion", "Disconnect"])
            .params("{} → {connected: false}")
            .noundo(),
        CommandSpec::new("companion.status", "Companion Status", run_status)
            .menu(&["Architecture", "Companion", "Status"])
            .params("{} → {connected, subscribed, pipe, preview}")
            .noundo(),
        CommandSpec::new("companion.evaluate", "Evaluate Companion Graph", run_evaluate)
            .menu(&["Architecture", "Companion", "Evaluate"])
            .params("{graph?: graph JSON (else the host's current graph)} → {ok, result}")
            .noundo(),
        CommandSpec::new("companion.subscribe", "Subscribe Companion Preview", run_subscribe)
            .menu(&["Architecture", "Companion", "Subscribe"])
            .params("{} → {subscribed}")
            .noundo(),
        CommandSpec::new("companion.unsubscribe", "Unsubscribe Companion Preview", run_unsubscribe)
            .menu(&["Architecture", "Companion", "Unsubscribe"])
            .params("{} → {subscribed: false}")
            .noundo(),
        CommandSpec::new("companion.preview", "Companion Preview", run_preview)
            .params("{} → {preview}")
            .noundo(),
        CommandSpec::new("companion.commit", "Commit Companion Graph", run_commit)
            .menu(&["Architecture", "Companion", "Commit"])
            .params("{result?: evaluate result (else last preview)} → {committed, levels, lines, circles, arcs, inserts, titleFields, texts, dimensions, warnings} (one undo step; creates missing levels; unknown blocks/sheet sets are skipped)"),
    ]
}

fn run_connect(s: &mut Session, p: &Value) -> Result<Value> {
    let pipe = str_param(p, "pipe").unwrap_or(DEFAULT_PIPE).trim();
    if pipe.is_empty() || pipe.len() > MAX_PIPE_NAME || pipe.contains(['\\', '/', '\0']) {
        return Err(bad("companion.connect", "`pipe` must be a short pipe name without path separators"));
    }
    #[cfg(not(windows))]
    {
        let _ = (s, pipe);
        return Err(bad("companion.connect", "the companion pipe is only available on Windows"));
    }
    #[cfg(windows)]
    {
        stop_subscriber(&mut s.companion);
        s.companion.pipe_name = pipe.to_string();
        let id = next_id(&s.companion);
        let pong = pipe_roundtrip(&s.companion.pipe_name, &json!({ "v": 2, "id": id, "method": "ping" }))
            .map_err(|e| bad("companion.connect", e))?;
        if pong.get("ok") != Some(&Value::Bool(true)) {
            return Err(bad("companion.connect", format!("ping failed: {pong}")));
        }
        let levels = levels_payload(s)?;
        let project = project_payload(s)?;
        let id_l = next_id(&s.companion);
        let lvl_resp = pipe_roundtrip(
            &s.companion.pipe_name,
            &json!({ "v": 2, "id": id_l, "method": "context.levels", "params": { "levels": levels } }),
        )
        .map_err(|e| bad("companion.connect", e))?;
        if lvl_resp.get("ok") != Some(&Value::Bool(true)) {
            return Err(bad("companion.connect", format!("context.levels failed: {lvl_resp}")));
        }
        let id_p = next_id(&s.companion);
        let proj_resp = pipe_roundtrip(
            &s.companion.pipe_name,
            &json!({ "v": 2, "id": id_p, "method": "context.project", "params": { "project": project } }),
        )
        .map_err(|e| bad("companion.connect", e))?;
        if proj_resp.get("ok") != Some(&Value::Bool(true)) {
            return Err(bad("companion.connect", format!("context.project failed: {proj_resp}")));
        }
        s.companion.connected = true;
        Ok(json!({
            "connected": true,
            "pipe": s.companion.pipe_name,
            "levels": levels,
            "project": project,
        }))
    }
}

fn run_disconnect(s: &mut Session, _p: &Value) -> Result<Value> {
    stop_subscriber(&mut s.companion);
    s.companion.connected = false;
    s.companion.preview = None;
    Ok(json!({ "connected": false }))
}

fn run_status(s: &mut Session, _p: &Value) -> Result<Value> {
    s.companion.pull_slot();
    Ok(json!({
        "connected": s.companion.connected,
        "subscribed": s.companion.subscribed,
        "pipe": s.companion.pipe_name,
        "preview": s.companion.preview,
        "available": cfg!(windows),
    }))
}

fn run_evaluate(s: &mut Session, p: &Value) -> Result<Value> {
    #[cfg(not(windows))]
    {
        let _ = (s, p);
        return Err(bad("companion.evaluate", "the companion pipe is only available on Windows"));
    }
    #[cfg(windows)]
    {
        ensure_connected(s)?;
        let id = next_id(&s.companion);
        let mut req = json!({ "v": 2, "id": id, "method": "evaluate", "params": {} });
        if let Some(g) = p.get("graph") {
            req["params"]["graph"] = g.clone();
        }
        let resp = pipe_roundtrip(&s.companion.pipe_name, &req).map_err(|e| bad("companion.evaluate", e))?;
        if let Some(result) = resp.get("result").cloned() {
            s.companion.preview = Some(result.clone());
            if let Ok(mut slot) = s.companion.slot.lock() {
                *slot = Some(result);
            }
        }
        Ok(resp)
    }
}

fn run_subscribe(s: &mut Session, _p: &Value) -> Result<Value> {
    #[cfg(not(windows))]
    {
        let _ = s;
        return Err(bad("companion.subscribe", "the companion pipe is only available on Windows"));
    }
    #[cfg(windows)]
    {
        ensure_connected(s)?;
        if s.companion.subscribed {
            return Ok(json!({ "subscribed": true }));
        }
        let pipe = s.companion.pipe_name.clone();
        let slot = Arc::clone(&s.companion.slot);
        let stop = Arc::clone(&s.companion.stop);
        stop.store(false, Ordering::SeqCst);
        let id = next_id(&s.companion);
        std::thread::Builder::new()
            .name("companion-sub".into())
            .spawn(move || subscribe_loop(pipe, id, slot, stop))
            .map_err(|e| bad("companion.subscribe", format!("could not start subscriber: {e}")))?;
        // Give the thread a moment to open the pipe and send subscribe.
        std::thread::sleep(Duration::from_millis(100));
        s.companion.subscribed = true;
        Ok(json!({ "subscribed": true }))
    }
}

fn run_unsubscribe(s: &mut Session, _p: &Value) -> Result<Value> {
    stop_subscriber(&mut s.companion);
    Ok(json!({ "subscribed": false }))
}

fn run_preview(s: &mut Session, _p: &Value) -> Result<Value> {
    s.companion.pull_slot();
    Ok(json!({ "preview": s.companion.preview }))
}

fn run_commit(s: &mut Session, p: &Value) -> Result<Value> {
    if s.block_edit.is_some() {
        return Err(bad("companion.commit", "close the block editor first"));
    }
    s.companion.pull_slot();
    let result = p
        .get("result")
        .cloned()
        .or_else(|| s.companion.preview.clone())
        .ok_or_else(|| bad("companion.commit", "no result to commit; evaluate first or pass `result`"))?;
    if !result.is_object() {
        return Err(bad(
            "companion.commit",
            "`result` must be an object with lines, circles, arcs, inserts, titleFields, texts and/or dimensions",
        ));
    }

    let active_name = s.doc()?.active_level_ref().map(|l| l.name.clone());
    let groups = group_by_level(&result, active_name.as_deref())?;
    let title_fields = parse_title_fields(&result)?;
    let annotations = group_annotations(&result, active_name.as_deref())?;
    if groups.is_empty() && title_fields.is_empty() && annotations.is_empty() {
        return Ok(json!({
            "committed": false,
            "levels": [],
            "lines": 0,
            "circles": 0,
            "arcs": 0,
            "inserts": 0,
            "titleFields": 0,
            "texts": 0,
            "dimensions": 0,
            "warnings": [],
        }));
    }

    let restore_level = s.doc()?.active_level;
    let restore_sheet = s.doc()?.active_sheet;
    let mut total_lines = 0usize;
    let mut total_circles = 0usize;
    let mut total_arcs = 0usize;
    let mut total_inserts = 0usize;
    let mut total_texts = 0usize;
    let mut total_dims = 0usize;
    let mut level_names = Vec::new();
    let mut warnings = Vec::new();

    for (level_name, items) in groups {
        ensure_named_level(s, &level_name)?;
        let id = s
            .doc()?
            .level_by_name(&level_name)
            .map(|l| l.id)
            .ok_or_else(|| bad("companion.commit", format!("level `{level_name}` could not be created")))?;
        if s.doc()?.active_level != Some(id) {
            s.doc_mut()?.switch_level(id).map_err(|e| bad("companion.commit", e.to_string()))?;
            s.set_selection(Vec::new());
        }
        level_names.push(level_name);
        for item in items {
            match item {
                CommitItem::Line { a, b, layer } => {
                    ensure_layer(s.doc_mut()?, &layer, 7, "Continuous", 25, "Companion commit");
                    add_on(s, &layer, line(a, b))?;
                    total_lines += 1;
                }
                CommitItem::Circle { c, r, layer } => {
                    ensure_layer(s.doc_mut()?, &layer, 7, "Continuous", 25, "Companion commit");
                    add_on(s, &layer, circle(c, r))?;
                    total_circles += 1;
                }
                CommitItem::Arc { c, r, start, end, layer } => {
                    ensure_layer(s.doc_mut()?, &layer, 7, "Continuous", 25, "Companion commit");
                    let geom = GeomArc::new(c, r, start, end);
                    add_on(s, &layer, arc(&geom))?;
                    total_arcs += 1;
                }
                CommitItem::Insert { name, at, rotation_deg, attribs } => {
                    if s.doc()?.block(&name).is_none() {
                        warnings.push(format!("block `{name}` is not in the drawing; skipped"));
                        continue;
                    }
                    let rot = rotation_deg.to_radians();
                    if !rot.is_finite() {
                        warnings.push(format!("block `{name}` has a non-finite rotation; skipped"));
                        continue;
                    }
                    match super::blocks::insert(s, &name, at, 1.0, rot, &attribs) {
                        Ok(_) => total_inserts += 1,
                        Err(e) => warnings.push(format!("block `{name}`: {e}")),
                    }
                }
            }
        }
    }

    let mut title_written = 0usize;
    if !title_fields.is_empty() {
        match layout_for_title_fields(s)? {
            Some(layout) => {
                if super::sheet::title_inserts(s.doc()?, &layout).is_empty() {
                    warnings.push(format!("layout `{layout}` has no title block; title fields skipped"));
                } else {
                    title_written = super::sheet::write_fields(s.doc_mut()?, &layout, &title_fields);
                }
            }
            None => warnings.push("no paper layout for title fields; skipped".into()),
        }
    }

    for ((set_name, level_name), items) in annotations {
        ensure_named_level(s, &level_name)?;
        let Some(set) = s.doc()?.sheet_set_by_name(&set_name).map(|x| x.id) else {
            warnings.push(format!("sheet set `{set_name}` is not in the drawing; annotation skipped"));
            continue;
        };
        let level = s
            .doc()?
            .level_by_name(&level_name)
            .map(|l| l.id)
            .ok_or_else(|| bad("companion.commit", format!("level `{level_name}` could not be created")))?;
        if s.doc()?.target() != Some(EditTarget::Sheet(set, level)) {
            s.doc_mut()?
                .switch_target(EditTarget::Sheet(set, level))
                .map_err(|e| bad("companion.commit", e.to_string()))?;
            s.set_selection(Vec::new());
        }
        if !level_names.iter().any(|n| n == &level_name) {
            level_names.push(level_name.clone());
        }
        let prefix = s.doc()?.sheet_set(set).map(|x| x.layer_prefix.clone()).unwrap_or_default();
        for item in items {
            match item {
                AnnoItem::Text { text, at, height, rotation_deg, layer } => {
                    let layer = anno_layer(&layer, &prefix, "TEXT");
                    ensure_layer(s.doc_mut()?, &layer, 7, "Continuous", 25, "Companion annotation");
                    let rot = rotation_deg.to_radians();
                    if !rot.is_finite() || !(height > 0.0) {
                        warnings.push("text note has invalid height or rotation; skipped".into());
                        continue;
                    }
                    let kind = make_text(s, at, &text, height, rot, (HAlign::Left, VAlign::Baseline));
                    add_on(s, &layer, kind)?;
                    total_texts += 1;
                }
                AnnoItem::Dim { a, b, offset, layer } => {
                    let layer = anno_layer(&layer, &prefix, "DIM");
                    ensure_layer(s.doc_mut()?, &layer, 7, "Continuous", 25, "Companion annotation");
                    let Some(at) = dim_at(a, b, offset) else {
                        warnings.push("dimension endpoints are coincident; skipped".into());
                        continue;
                    };
                    let kind = linear_dim_entity(s, a, b, at);
                    add_on(s, &layer, kind)?;
                    total_dims += 1;
                }
            }
        }
    }

    restore_edit_target(s, restore_level, restore_sheet);

    let committed =
        total_lines + total_circles + total_arcs + total_inserts + title_written + total_texts + total_dims > 0;
    Ok(json!({
        "committed": committed,
        "levels": level_names,
        "lines": total_lines,
        "circles": total_circles,
        "arcs": total_arcs,
        "inserts": total_inserts,
        "titleFields": title_written,
        "texts": total_texts,
        "dimensions": total_dims,
        "warnings": warnings,
    }))
}

enum CommitItem {
    Line { a: Vec2, b: Vec2, layer: String },
    Circle { c: Vec2, r: f64, layer: String },
    Arc { c: Vec2, r: f64, start: f64, end: f64, layer: String },
    Insert {
        name: String,
        at: Vec2,
        rotation_deg: f64,
        attribs: serde_json::Map<String, Value>,
    },
}

enum AnnoItem {
    Text {
        text: String,
        at: Vec2,
        height: f64,
        rotation_deg: f64,
        layer: String,
    },
    Dim {
        a: Vec2,
        b: Vec2,
        offset: f64,
        layer: String,
    },
}

fn anno_layer(requested: &str, prefix: &str, kind: &str) -> String {
    let trimmed = requested.trim();
    if !trimmed.is_empty() && trimmed.len() <= 255 {
        return trimmed.to_string();
    }
    if prefix.is_empty() {
        format!("A-{kind}")
    } else {
        format!("{prefix}{kind}")
    }
}

fn dim_at(a: Vec2, b: Vec2, offset: f64) -> Option<Vec2> {
    let d = b - a;
    let len = d.len();
    if !(len > 1e-12) || !offset.is_finite() {
        return None;
    }
    let perp = Vec2::new(-d.y / len, d.x / len) * offset;
    Some((a + b) * 0.5 + perp)
}

fn linear_dim_entity(s: &Session, a: Vec2, b: Vec2, at: Vec2) -> EntityKind {
    let rotation = {
        let (x0, x1) = (a.x.min(b.x), a.x.max(b.x));
        let (y0, y1) = (a.y.min(b.y), a.y.max(b.y));
        let outside_y = at.y < y0 || at.y > y1;
        let outside_x = at.x < x0 || at.x > x1;
        if outside_x && !outside_y {
            std::f64::consts::FRAC_PI_2
        } else {
            0.0
        }
    };
    let style = s.doc().map(|d| d.header.str("DIMSTYLE", "Standard")).unwrap_or_else(|_| "Standard".into());
    let overrides = s.doc().map(|d| d.dim_overrides()).unwrap_or_default();
    EntityKind::Dimension(Dimension {
        kind: DimKind::Linear { rotation },
        defpt: v3(at),
        text_mid: Vec3::ZERO,
        p13: v3(a),
        p14: v3(b),
        p15: Vec3::ZERO,
        p16: Vec3::ZERO,
        text: String::new(),
        style,
        measurement: 0.0,
        text_rotation: 0.0,
        user_text_pos: false,
        block: None,
        overrides,
        assoc: Vec::new(),
    })
}

fn restore_edit_target(s: &mut Session, level: Option<aad_doc::LevelId>, sheet: Option<aad_doc::SheetSetId>) {
    let Some(level) = level else {
        return;
    };
    let target = match sheet {
        Some(set) => EditTarget::Sheet(set, level),
        None => EditTarget::Level(level),
    };
    if s.doc().ok().and_then(|d| d.target()) == Some(target) {
        return;
    }
    let _ = s.doc_mut().map(|d| d.switch_target(target));
}

fn group_annotations(
    result: &Value,
    active: Option<&str>,
) -> Result<BTreeMap<(String, String), Vec<AnnoItem>>> {
    let mut out: BTreeMap<(String, String), Vec<AnnoItem>> = BTreeMap::new();
    let mut count = 0usize;
    let default_level = active.unwrap_or("Level 1").to_string();

    let push = |out: &mut BTreeMap<(String, String), Vec<AnnoItem>>,
                key: (String, String),
                item: AnnoItem,
                count: &mut usize|
     -> Result<()> {
        *count = count.saturating_add(1);
        if *count > MAX_COMMIT_ITEMS {
            return Err(bad("companion.commit", format!("at most {MAX_COMMIT_ITEMS} entities per commit")));
        }
        out.entry(key).or_default().push(item);
        Ok(())
    };

    if let Some(arr) = result.get("texts").and_then(Value::as_array) {
        for (i, v) in arr.iter().enumerate() {
            let Some(set) = sheet_set_of(v)? else {
                continue;
            };
            let level = level_of(v, &default_level)?;
            let text = v
                .get("text")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty() && s.len() <= 1024)
                .ok_or_else(|| bad("companion.commit", format!("text {i}: `text` is required")))?
                .to_string();
            let x = num_field(v, "x", i, "text")?;
            let y = num_field(v, "y", i, "text")?;
            let height = v.get("height").and_then(Value::as_f64).filter(|n| n.is_finite() && *n > 0.0).unwrap_or(250.0);
            let rotation_deg = v.get("rotation").and_then(Value::as_f64).filter(|n| n.is_finite()).unwrap_or(0.0);
            let layer = layer_of(v, "");
            push(
                &mut out,
                (set, level),
                AnnoItem::Text {
                    text,
                    at: Vec2::new(x, y),
                    height,
                    rotation_deg,
                    layer,
                },
                &mut count,
            )?;
        }
    }
    if let Some(arr) = result.get("dimensions").and_then(Value::as_array) {
        for (i, v) in arr.iter().enumerate() {
            let Some(set) = sheet_set_of(v)? else {
                continue;
            };
            let level = level_of(v, &default_level)?;
            let x0 = num_field(v, "x0", i, "dimension")?;
            let y0 = num_field(v, "y0", i, "dimension")?;
            let x1 = num_field(v, "x1", i, "dimension")?;
            let y1 = num_field(v, "y1", i, "dimension")?;
            let offset = v.get("offset").and_then(Value::as_f64).filter(|n| n.is_finite()).unwrap_or(500.0);
            let layer = layer_of(v, "");
            push(
                &mut out,
                (set, level),
                AnnoItem::Dim {
                    a: Vec2::new(x0, y0),
                    b: Vec2::new(x1, y1),
                    offset,
                    layer,
                },
                &mut count,
            )?;
        }
    }
    Ok(out)
}

fn sheet_set_of(v: &Value) -> Result<Option<String>> {
    match v.get("sheetSet").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()) {
        Some(name) if name.len() <= 64 && aad_doc::level_name_ok(name) => Ok(Some(name.to_string())),
        Some(_) => Err(bad("companion.commit", "a sheet set name is 1 to 64 characters without control characters")),
        None => Ok(None),
    }
}

fn group_by_level(result: &Value, active: Option<&str>) -> Result<BTreeMap<String, Vec<CommitItem>>> {
    let mut out: BTreeMap<String, Vec<CommitItem>> = BTreeMap::new();
    let mut count = 0usize;
    let default_level = active.unwrap_or("Level 1").to_string();

    let push = |out: &mut BTreeMap<String, Vec<CommitItem>>, level: String, item: CommitItem, count: &mut usize| -> Result<()> {
        *count = count.saturating_add(1);
        if *count > MAX_COMMIT_ITEMS {
            return Err(bad("companion.commit", format!("at most {MAX_COMMIT_ITEMS} entities per commit")));
        }
        out.entry(level).or_default().push(item);
        Ok(())
    };

    if let Some(arr) = result.get("lines").and_then(Value::as_array) {
        for (i, v) in arr.iter().enumerate() {
            let layer = layer_of(v, "0");
            let level = level_of(v, &default_level)?;
            let x0 = num_field(v, "x0", i, "line")?;
            let y0 = num_field(v, "y0", i, "line")?;
            let x1 = num_field(v, "x1", i, "line")?;
            let y1 = num_field(v, "y1", i, "line")?;
            push(
                &mut out,
                level,
                CommitItem::Line { a: Vec2::new(x0, y0), b: Vec2::new(x1, y1), layer },
                &mut count,
            )?;
        }
    }
    if let Some(arr) = result.get("circles").and_then(Value::as_array) {
        for (i, v) in arr.iter().enumerate() {
            let layer = layer_of(v, "0");
            let level = level_of(v, &default_level)?;
            let cx = num_field(v, "cx", i, "circle")?;
            let cy = num_field(v, "cy", i, "circle")?;
            let r = num_field(v, "r", i, "circle")?;
            if !(r > 0.0) {
                return Err(bad("companion.commit", format!("circle {i}: `r` must be positive")));
            }
            push(&mut out, level, CommitItem::Circle { c: Vec2::new(cx, cy), r, layer }, &mut count)?;
        }
    }
    if let Some(arr) = result.get("arcs").and_then(Value::as_array) {
        for (i, v) in arr.iter().enumerate() {
            let layer = layer_of(v, "0");
            let level = level_of(v, &default_level)?;
            let cx = num_field(v, "cx", i, "arc")?;
            let cy = num_field(v, "cy", i, "arc")?;
            let r = num_field(v, "r", i, "arc")?;
            let start = num_field(v, "startAngle", i, "arc")?;
            let end = num_field(v, "endAngle", i, "arc")?;
            if !(r > 0.0) {
                return Err(bad("companion.commit", format!("arc {i}: `r` must be positive")));
            }
            push(
                &mut out,
                level,
                CommitItem::Arc { c: Vec2::new(cx, cy), r, start, end, layer },
                &mut count,
            )?;
        }
    }
    if let Some(arr) = result.get("inserts").and_then(Value::as_array) {
        for (i, v) in arr.iter().enumerate() {
            let name = v
                .get("blockName")
                .or_else(|| v.get("name"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty() && s.len() <= 255)
                .ok_or_else(|| bad("companion.commit", format!("insert {i}: `blockName` is required")))?
                .to_string();
            let level = level_of(v, &default_level)?;
            let x = num_field(v, "x", i, "insert")?;
            let y = num_field(v, "y", i, "insert")?;
            let rotation_deg = v.get("rotation").and_then(Value::as_f64).filter(|n| n.is_finite()).unwrap_or(0.0);
            let mut attribs = serde_json::Map::new();
            if let Some(obj) = v.get("attributes").or_else(|| v.get("attribs")).and_then(Value::as_object) {
                if obj.len() > 64 {
                    return Err(bad("companion.commit", format!("insert {i}: too many attributes")));
                }
                for (k, val) in obj {
                    if k.is_empty() || k.len() > 64 {
                        continue;
                    }
                    let text = match val {
                        Value::String(s) => s.clone(),
                        Value::Number(n) => n.to_string(),
                        Value::Bool(b) => if *b { "1".into() } else { "0".into() },
                        Value::Null => String::new(),
                        _ => continue,
                    };
                    if text.chars().count() > 256 {
                        return Err(bad("companion.commit", format!("insert {i}: attribute `{k}` is too long")));
                    }
                    attribs.insert(k.clone(), Value::String(text));
                }
            }
            push(
                &mut out,
                level,
                CommitItem::Insert {
                    name,
                    at: Vec2::new(x, y),
                    rotation_deg,
                    attribs,
                },
                &mut count,
            )?;
        }
    }
    Ok(out)
}

fn parse_title_fields(result: &Value) -> Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    let Some(arr) = result.get("titleFields").and_then(Value::as_array) else {
        return Ok(out);
    };
    if arr.len() > 64 {
        return Err(bad("companion.commit", "at most 64 title fields per commit"));
    }
    for (i, v) in arr.iter().enumerate() {
        let field = v
            .get("field")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty() && s.len() <= 32)
            .ok_or_else(|| bad("companion.commit", format!("titleField {i}: `field` is required")))?
            .to_ascii_uppercase();
        let value = match v.get("value") {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Number(n)) => n.to_string(),
            Some(Value::Null) | None => String::new(),
            Some(Value::Bool(b)) => if *b { "1".into() } else { "0".into() },
            _ => return Err(bad("companion.commit", format!("titleField {i}: `value` must be text"))),
        };
        if value.chars().count() > 256 {
            return Err(bad("companion.commit", format!("titleField {i}: value is too long")));
        }
        out.insert(field, value);
    }
    Ok(out)
}

fn layout_for_title_fields(s: &Session) -> Result<Option<String>> {
    let d = s.doc()?;
    if let crate::doc::Space::Paper(n) = s.layout_space() {
        if d.layout(&n).is_some() {
            return Ok(Some(n));
        }
    }
    for layout in &d.layouts {
        if !super::sheet::title_inserts(d, &layout.name).is_empty() {
            return Ok(Some(layout.name.clone()));
        }
    }
    Ok(d.layouts.first().map(|l| l.name.clone()))
}

fn layer_of(v: &Value, fallback: &str) -> String {
    v.get("layer")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty() && s.len() <= 255)
        .unwrap_or(fallback)
        .to_string()
}

fn level_of(v: &Value, fallback: &str) -> Result<String> {
    match v.get("level").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()) {
        Some(name) if name.len() <= MAX_LEVEL_NAME && aad_doc::level_name_ok(name) => Ok(name.to_string()),
        Some(_) => Err(bad("companion.commit", "a level name is 1 to 64 characters without control characters")),
        None => Ok(fallback.to_string()),
    }
}

fn num_field(v: &Value, key: &str, index: usize, kind: &str) -> Result<f64> {
    v.get(key)
        .and_then(Value::as_f64)
        .filter(|n| n.is_finite())
        .ok_or_else(|| bad("companion.commit", format!("{kind} {index}: `{key}` must be a finite number")))
}

fn ensure_named_level(s: &mut Session, name: &str) -> Result<()> {
    if s.doc()?.level_by_name(name).is_some() {
        return Ok(());
    }
    let d = s.doc_mut()?;
    if !d.has_levels() {
        let first = super::levels::implicit_name(d);
        if first.eq_ignore_ascii_case(name) {
            d.materialize_levels(name, 0.0, 3.0).map_err(|e| bad("companion.commit", e.to_string()))?;
            return Ok(());
        }
        d.materialize_levels(&first, 0.0, 3.0).map_err(|e| bad("companion.commit", e.to_string()))?;
    }
    if d.level_by_name(name).is_some() {
        return Ok(());
    }
    d.add_level(name, 0.0, 3.0, None).map_err(|e| bad("companion.commit", e.to_string()))?;
    Ok(())
}

fn ensure_connected(s: &Session) -> Result<()> {
    if s.companion.connected {
        Ok(())
    } else {
        Err(bad("companion", "not connected; run companion.connect first"))
    }
}

fn next_id(c: &CompanionState) -> u64 {
    c.next_id.fetch_add(1, Ordering::Relaxed)
}

fn stop_subscriber(c: &mut CompanionState) {
    c.stop.store(true, Ordering::SeqCst);
    c.subscribed = false;
}

fn levels_payload(s: &Session) -> Result<Value> {
    let d = s.doc()?;
    let active = project::active_floor(d).map(|(_, n)| n);
    let mut elev = 0.0_f64;
    let mut out = Vec::new();
    for f in project::floors(d) {
        let name = f.get("name").and_then(Value::as_str).unwrap_or("").to_string();
        if name.is_empty() {
            continue;
        }
        let height = f.get("floorToFloor").and_then(Value::as_f64).filter(|n| n.is_finite() && *n > 0.0).unwrap_or(3.0);
        let is_active = active.as_deref() == Some(name.as_str());
        out.push(json!({
            "name": name,
            "elevation": (elev * 1000.0).round() / 1000.0,
            "height": height,
            "active": is_active,
        }));
        elev += height;
    }
    Ok(Value::Array(out))
}

fn project_payload(s: &Session) -> Result<Value> {
    let d = s.doc()?;
    let details = project::get(d, "details");
    let structure = project::get(d, "structure");
    let floors = Value::Array(project::floors(d));
    let mut map = serde_json::Map::new();
    if let Value::Object(d) = details {
        for (k, v) in d {
            map.insert(k, v);
        }
    }
    map.insert("details".into(), project::get(d, "details"));
    map.insert("structure".into(), structure);
    map.insert("floors".into(), floors);
    Ok(Value::Object(map))
}

#[cfg(windows)]
fn pipe_roundtrip(pipe_name: &str, request: &Value) -> std::result::Result<Value, String> {
    use std::io::{BufRead, BufReader, Write};

    let path = format!(r"\\.\pipe\{pipe_name}");
    let payload = serde_json::to_string(request).map_err(|e| e.to_string())?;
    if payload.len() > MAX_RESPONSE_BYTES {
        return Err("request is too large".into());
    }
    let mut last = String::from("pipe not found");
    for _ in 0..40 {
        match std::fs::OpenOptions::new().read(true).write(true).open(&path) {
            Ok(mut file) => {
                writeln!(file, "{payload}").map_err(|e| e.to_string())?;
                let mut reader = BufReader::new(file);
                let mut line = String::new();
                reader.read_line(&mut line).map_err(|e| e.to_string())?;
                if line.len() > MAX_RESPONSE_BYTES {
                    return Err("response is too large".into());
                }
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    return Err("empty response from companion".into());
                }
                return serde_json::from_str(trimmed).map_err(|e| format!("invalid companion JSON: {e}"));
            }
            Err(e) => {
                last = e.to_string();
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
    Err(format!("cannot open pipe `{pipe_name}`: {last}"))
}

#[cfg(windows)]
fn subscribe_loop(pipe_name: String, subscribe_id: u64, slot: Arc<Mutex<Option<Value>>>, stop: Arc<AtomicBool>) {
    use std::io::{BufRead, BufReader, Write};

    let path = format!(r"\\.\pipe\{pipe_name}");
    let mut file = None;
    for _ in 0..40 {
        if stop.load(Ordering::SeqCst) {
            return;
        }
        match std::fs::OpenOptions::new().read(true).write(true).open(&path) {
            Ok(f) => {
                file = Some(f);
                break;
            }
            Err(_) => std::thread::sleep(Duration::from_millis(50)),
        }
    }
    let Some(mut file) = file else {
        return;
    };
    let req = json!({ "v": 2, "id": subscribe_id, "method": "subscribe" });
    let Ok(payload) = serde_json::to_string(&req) else {
        return;
    };
    if writeln!(file, "{payload}").is_err() {
        return;
    }
    let mut reader = BufReader::new(file);
    let mut line = String::new();
    while !stop.load(Ordering::SeqCst) {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                if line.len() > MAX_RESPONSE_BYTES {
                    break;
                }
                let Ok(msg) = serde_json::from_str::<Value>(line.trim()) else {
                    continue;
                };
                // First reply is the subscribe ack; later pushes are method=evaluation.
                if msg.get("method").and_then(Value::as_str) == Some("evaluation") {
                    let payload = msg.get("result").cloned().unwrap_or(msg);
                    if let Ok(mut guard) = slot.lock() {
                        *guard = Some(payload);
                    }
                }
            }
            Err(_) => break,
        }
    }
}
