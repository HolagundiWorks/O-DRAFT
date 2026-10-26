//! Drawing legends for Format: layer swatches with descriptions, and block symbols with
//! descriptions (a symbol key beside the plan).

use aad_doc::{EntityKind, Space};
use aad_geom::{Bounds2, Vec2, Vec3};
use serde_json::{Map, Value, json};

use super::draw::make_text;
use super::hcw::{add_on, ensure_layer};
use super::helpers::{line, lwpoly, rect_vertices};
use super::takeoff::{table, table_ctx};
use super::*;
use crate::annot::{self as ua};
use crate::{Accept, Input, Interactive, Prompt, Result, Session, Step};

const LAYER: &str = "AN-LEGEND";
const SLOT: &str = "legend";

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("legend", "Legend", run_legend)
            .menu(&["Format", "Legend", "Layer Legend"])
            .alias(&["layerlegend", "leg"])
            .params("{at?: [x,y], layers?: [name] (default: used layers with a description, else all used layers)} → {rows}")
            .interactive(|_| Ok(Box::new(PlaceLegendM { kind: LegendKind::Layers }))),
        CommandSpec::new("legend.edit", "Edit Legend Entries…", run_edit)
            .menu(&["Format", "Legend", "Edit Legend Entries…"])
            .params("{entries?: [{layer|symbol, description}], clear?: bool} → {entries} (optional overrides stored in the drawing)"),
        CommandSpec::new("symbol.legend", "Symbol Legend", run_symbols)
            .menu(&["Format", "Legend", "Symbol Legend"])
            .alias(&["blocklegend", "symlegend"])
            .params("{at?: [x,y], blocks?: [name] (default: non-anonymous blocks with a description, or every used block), scale?: number, table?: bool} → {rows}")
            .interactive(|_| Ok(Box::new(PlaceLegendM { kind: LegendKind::Symbols }))),
        CommandSpec::new("symbol.desc", "Block Description", run_block_desc)
            .menu(&["Format", "Legend", "Block Description…"])
            .alias(&["blockdesc"])
            .params("{name, description?} → {name, description} (sets or reads a block's description for the symbol legend)"),
    ]
}

#[derive(Clone, Copy, Default)]
enum LegendKind {
    #[default]
    Layers,
    Symbols,
}

#[derive(Default)]
struct PlaceLegendM {
    kind: LegendKind,
}

impl Interactive for PlaceLegendM {
    fn name(&self) -> &'static str {
        match self.kind {
            LegendKind::Layers => "legend",
            LegendKind::Symbols => "symbol.legend",
        }
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        Prompt::new(
            match self.kind {
                LegendKind::Layers => "Specify insertion point for layer legend",
                LegendKind::Symbols => "Specify insertion point for symbol legend",
            },
            Accept::POINT,
        )
    }
    fn input(&mut self, s: &mut Session, input: Input) -> Result<Step> {
        match input {
            Input::Point(p) => {
                let params = json!({ "at": [p.x, p.y] });
                match self.kind {
                    LegendKind::Layers => {
                        let _ = run_legend(s, &params)?;
                    }
                    LegendKind::Symbols => {
                        let _ = run_symbols(s, &params)?;
                    }
                }
                Ok(Step::Done)
            }
            Input::Cancel => Ok(Step::Cancel),
            _ => Ok(Step::Continue),
        }
    }
}

fn th(s: &Session) -> f64 {
    ua::drawing_unit(s.doc().ok().unwrap_or(&aad_doc::Drawing::new_imperial())).map_or(0.18, |u| 0.18 / u.metres())
}

fn origin(s: &Session, p: &Value) -> Vec2 {
    if let Some(at) = point_param(p, "at") {
        return at;
    }
    let t = th(s);
    let ext = s.doc().map(|d| d.extents(&Space::Model)).unwrap_or(Bounds2::EMPTY);
    if ext.is_empty() { Vec2::ZERO } else { Vec2::new(ext.max.x + 4.0 * t, ext.max.y) }
}

fn overrides(d: &aad_doc::Drawing) -> Map<String, Value> {
    match project::get(d, SLOT) {
        Value::Object(o) => o.get("entries").and_then(Value::as_object).cloned().unwrap_or_default(),
        _ => Map::new(),
    }
}

fn override_desc(o: &Map<String, Value>, key: &str) -> Option<String> {
    o.get(key).and_then(|v| {
        if let Some(s) = v.as_str() {
            return Some(s.to_string());
        }
        v.get("description").and_then(Value::as_str).map(str::to_string)
    })
}

fn used_layers(d: &aad_doc::Drawing) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    for e in std::iter::once(&d.model).chain(d.layouts.iter().map(|l| &l.entities)).flat_map(|s| s.iter()) {
        if !e.common.layer.is_empty() {
            seen.insert(e.common.layer.clone());
        }
    }
    seen.into_iter().collect()
}

fn run_edit(s: &mut Session, p: &Value) -> Result<Value> {
    let mut store = match project::get(s.doc()?, SLOT) {
        Value::Object(o) => o,
        _ => Map::new(),
    };
    if bool_or(p, "clear", false) {
        store.insert("entries".into(), json!({}));
        project::set(s.doc_mut()?, SLOT, &Value::Object(store.clone()));
        return Ok(json!({ "entries": {} }));
    }
    let mut entries = store.get("entries").and_then(Value::as_object).cloned().unwrap_or_default();
    if let Some(list) = p.get("entries").and_then(Value::as_array) {
        if list.len() > 500 {
            return Err(bad("legend.edit", "at most 500 legend entries"));
        }
        for e in list {
            let key = e
                .get("layer")
                .or_else(|| e.get("symbol"))
                .or_else(|| e.get("name"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .ok_or_else(|| bad("legend.edit", "each entry needs a layer or symbol name"))?;
            if key.len() > 64 {
                return Err(bad("legend.edit", "entry names are at most 64 characters"));
            }
            let desc = e.get("description").and_then(Value::as_str).unwrap_or("").trim();
            if desc.len() > 200 {
                return Err(bad("legend.edit", "descriptions are at most 200 characters"));
            }
            if desc.is_empty() {
                entries.remove(key);
            } else {
                entries.insert(key.to_string(), json!(desc));
            }
        }
    }
    store.insert("entries".into(), Value::Object(entries.clone()));
    project::set(s.doc_mut()?, SLOT, &Value::Object(store));
    Ok(json!({ "entries": entries }))
}

fn run_legend(s: &mut Session, p: &Value) -> Result<Value> {
    let at = origin(s, p);
    let t = th(s);
    let ov = overrides(s.doc()?);
    let names: Vec<String> = if let Some(list) = p.get("layers").and_then(Value::as_array) {
        list.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()
    } else {
        let used = used_layers(s.doc()?);
        let with_desc: Vec<String> = used
            .iter()
            .filter(|n| override_desc(&ov, n).is_some() || s.doc().ok().and_then(|d| d.layer(n)).is_some_and(|l| !l.description.trim().is_empty()))
            .cloned()
            .collect();
        if with_desc.is_empty() { used } else { with_desc }
    };
    if names.is_empty() {
        return Err(bad("legend", "no layers to list — draw something, or pass `layers`"));
    }
    ensure_layer(s.doc_mut()?, LAYER, 7, "Continuous", 18, "Drawing legend");
    let (sw_w, sw_h, row_h, gap) = (t * 3.2, t * 1.4, t * 2.2, t * 0.5);
    let title = make_text(s, Vec2::new(at.x, at.y + t * 0.4), "LEGEND", t * 1.3, 0.0, (aad_doc::HAlign::Left, aad_doc::VAlign::Bottom));
    add_on(s, LAYER, title)?;
    let mut rows = Vec::new();
    for (i, name) in names.iter().enumerate() {
        let y = at.y - (i as f64 + 1.0) * row_h;
        let sw = Vec2::new(at.x, y);
        // Swatch on the layer so its colour/linetype show in the key.
        add_on(s, name, lwpoly(rect_vertices(sw, Vec2::new(sw.x + sw_w, sw.y + sw_h)), true))?;
        add_on(s, name, line(Vec2::new(sw.x, sw.y + sw_h / 2.0), Vec2::new(sw.x + sw_w, sw.y + sw_h / 2.0)))?;
        let desc = override_desc(&ov, name).or_else(|| s.doc().ok().and_then(|d| d.layer(name).map(|l| l.description.clone()))).unwrap_or_default();
        let label = if desc.trim().is_empty() { name.clone() } else { format!("{name}  —  {desc}") };
        let txt =
            make_text(s, Vec2::new(sw.x + sw_w + gap, sw.y + (sw_h - t) / 2.0), &label, t, 0.0, (aad_doc::HAlign::Left, aad_doc::VAlign::Baseline));
        add_on(s, LAYER, txt)?;
        rows.push(json!({ "layer": name, "description": desc }));
    }
    Ok(json!({ "rows": rows, "at": [at.x, at.y] }))
}

fn block_span(d: &aad_doc::Drawing, name: &str) -> f64 {
    let Some(blk) = d.block(name) else {
        return 1.0;
    };
    let mut b = Bounds2::EMPTY;
    for e in blk.entities.iter() {
        b = b.union(&aad_doc::entity_bounds(d, e, 0));
    }
    if b.is_empty() {
        return 1.0;
    }
    (b.max.x - b.min.x).max(b.max.y - b.min.y).max(1e-6)
}

fn used_blocks(d: &aad_doc::Drawing) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    for e in std::iter::once(&d.model).chain(d.layouts.iter().map(|l| &l.entities)).flat_map(|s| s.iter()) {
        if let EntityKind::Insert(i) = &e.kind {
            seen.insert(i.block.clone());
        }
    }
    seen.into_iter().collect()
}

fn run_symbols(s: &mut Session, p: &Value) -> Result<Value> {
    let at = origin(s, p);
    let t = th(s);
    let ov = overrides(s.doc()?);
    let as_table = bool_or(p, "table", false);
    let names: Vec<String> = if let Some(list) = p.get("blocks").and_then(Value::as_array) {
        list.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()
    } else {
        let d = s.doc()?;
        let mut named: Vec<String> = d
            .blocks
            .values()
            .filter(|b| !b.anonymous)
            .filter(|b| !b.description.trim().is_empty() || override_desc(&ov, &b.name).is_some())
            .map(|b| b.name.clone())
            .collect();
        if named.is_empty() {
            named = used_blocks(d).into_iter().filter(|n| d.block(n).is_some_and(|b| !b.anonymous)).collect();
        }
        named.sort();
        named
    };
    if names.is_empty() {
        return Err(bad("symbol.legend", "no blocks to list — define blocks with a description, or pass `blocks`"));
    }
    for n in &names {
        if s.doc()?.block(n).is_none() {
            return Err(bad("symbol.legend", format!("block `{n}` not found")));
        }
    }
    ensure_layer(s.doc_mut()?, LAYER, 7, "Continuous", 18, "Drawing legend");

    if as_table {
        let rows: Vec<Vec<String>> = names
            .iter()
            .map(|n| {
                let desc = override_desc(&ov, n).or_else(|| s.doc().ok().and_then(|d| d.block(n).map(|b| b.description.clone()))).unwrap_or_default();
                vec![n.clone(), desc]
            })
            .collect();
        table(s, &table_ctx(t), at, "SYMBOL LEGEND", &["Symbol".into(), "Description".into()], &rows)?;
        return Ok(json!({ "rows": rows, "at": [at.x, at.y], "table": true }));
    }

    let box_sz = f64_or(p, "scale", t * 4.0).max(t);
    let row_h = box_sz * 1.35;
    let gap = t * 0.6;
    let title = make_text(s, Vec2::new(at.x, at.y + t * 0.4), "SYMBOL LEGEND", t * 1.3, 0.0, (aad_doc::HAlign::Left, aad_doc::VAlign::Bottom));
    add_on(s, LAYER, title)?;
    let mut rows = Vec::new();
    for (i, name) in names.iter().enumerate() {
        let y = at.y - (i as f64 + 1.0) * row_h;
        let cell = Vec2::new(at.x, y);
        add_on(s, LAYER, lwpoly(rect_vertices(cell, Vec2::new(cell.x + box_sz, cell.y + box_sz)), true))?;
        let span = block_span(s.doc()?, name);
        let sc = (box_sz * 0.7) / span;
        let centre = Vec2::new(cell.x + box_sz / 2.0, cell.y + box_sz / 2.0);
        let mut ins = aad_doc::Insert {
            block: name.clone(),
            insert: Vec3::new(centre.x, centre.y, 0.0),
            scale: Vec3::new(sc, sc, sc),
            ..aad_doc::Insert::default()
        };
        // Centre the block's local extents on the cell.
        if let Some(blk) = s.doc()?.block(name) {
            let mut b = Bounds2::EMPTY;
            for e in blk.entities.iter() {
                b = b.union(&aad_doc::entity_bounds(s.doc()?, e, 0));
            }
            if !b.is_empty() {
                let mid = Vec2::new((b.min.x + b.max.x) / 2.0, (b.min.y + b.max.y) / 2.0);
                let m = ins.transform(blk.base.xy());
                let landed = m.apply(mid);
                ins.insert = Vec3::new(centre.x + (centre.x - landed.x), centre.y + (centre.y - landed.y), 0.0);
            }
        }
        add_on(s, LAYER, EntityKind::Insert(ins))?;
        let desc = override_desc(&ov, name).or_else(|| s.doc().ok().and_then(|d| d.block(name).map(|b| b.description.clone()))).unwrap_or_default();
        let label = if desc.trim().is_empty() { name.clone() } else { format!("{name}  —  {desc}") };
        let txt = make_text(
            s,
            Vec2::new(cell.x + box_sz + gap, cell.y + (box_sz - t) / 2.0),
            &label,
            t,
            0.0,
            (aad_doc::HAlign::Left, aad_doc::VAlign::Baseline),
        );
        add_on(s, LAYER, txt)?;
        rows.push(json!({ "block": name, "description": desc }));
    }
    Ok(json!({ "rows": rows, "at": [at.x, at.y] }))
}

fn run_block_desc(s: &mut Session, p: &Value) -> Result<Value> {
    let name = str_param(p, "name").ok_or_else(|| bad("symbol.desc", "`name` is required"))?;
    let d = s.doc_mut()?;
    let Some(blk) = d.blocks.get_mut(name) else {
        return Err(bad("symbol.desc", format!("block `{name}` not found")));
    };
    let blk = std::sync::Arc::make_mut(blk);
    if let Some(desc) = p.get("description").and_then(Value::as_str) {
        if desc.len() > 200 {
            return Err(bad("symbol.desc", "descriptions are at most 200 characters"));
        }
        blk.description = desc.to_string();
    }
    Ok(json!({ "name": blk.name, "description": blk.description }))
}
