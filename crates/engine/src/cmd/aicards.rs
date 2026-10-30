//! Compact context for OLMo: digests and capped ContextCards.
//!
//! Prefer these over dumping full `/drawing` / `/view` JSON into the prompt.

use aad_doc::{Drawing, Entity, EntityKind, Space};
use aad_geom::{Bounds2, Vec2};
use serde_json::{Map, Value, json};

use super::*;
use crate::{Result, Session};

/// Hard cap on cards returned to the model in one shot.
pub const MAX_CARDS: usize = 24;
/// Soft character budget for a cards array JSON (prompt load).
pub const MAX_CARDS_CHARS: usize = 2_800;
const MAX_SCAN: usize = 2_000_000;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("ai.digest", "Drawing digest for AI", run_digest)
            .params("{} → compact overview (counts by layer/type, extents, rooms) for OLMo prompts")
            .noundo(),
        CommandSpec::new("ai.cards", "Context cards for AI", run_cards)
            .params(
                "{window?: [x0,y0,x1,y1], layers?: [..], types?: [..], limit?: 1..24} → capped ContextCards \
                 (default window = current view)",
            )
            .noundo(),
    ]
}

fn run_digest(s: &mut Session, _p: &Value) -> Result<Value> {
    drawing_digest(s)
}

fn run_cards(s: &mut Session, p: &Value) -> Result<Value> {
    context_cards(s, p)
}

/// Compact overview (~200–400 tokens) for the open drawing.
pub fn drawing_digest(s: &Session) -> Result<Value> {
    let d = s.doc()?;
    let space = s.space();
    let ext = d.extents(&space);
    let st = s.state().ok();
    let mut by_type: std::collections::BTreeMap<&'static str, usize> = Default::default();
    let mut by_layer: std::collections::BTreeMap<String, usize> = Default::default();
    let mut total = 0usize;
    if let Some(store) = d.space(&space) {
        for e in store.iter().take(MAX_SCAN) {
            total += 1;
            *by_type.entry(e.kind.type_name()).or_default() += 1;
            *by_layer.entry(e.common.layer.clone()).or_default() += 1;
        }
    }
    // Top layers by count (keep prompt small).
    let mut layer_rows: Vec<(String, usize)> = by_layer.into_iter().collect();
    layer_rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let layer_top: Map<String, Value> = layer_rows.into_iter().take(16).map(|(k, n)| (k, json!(n))).collect();

    let rooms = room_tally(d);
    let openings = match super::project::get(d, "schedule") {
        Value::Object(o) => o.get("openings").and_then(Value::as_array).map(|a| a.len()).unwrap_or(0),
        _ => 0,
    };

    Ok(json!({
        "kind": "drawing_digest",
        "title": st.map(|x| x.title.clone()),
        "path": st.and_then(|x| x.path.clone()),
        "space": match &space {
            Space::Model => "Model".to_string(),
            Space::Paper(n) => n.clone(),
        },
        "unit": d.header.i64("INSUNITS", 0),
        "objects": total,
        "byType": by_type,
        "byLayerTop": layer_top,
        "layerCount": d.layers.len(),
        "currentLayer": d.header.str("CLAYER", "0"),
        "extents": extents_json(&ext),
        "rooms": rooms,
        "openingsInSchedule": openings,
        "selection": s.selection().len(),
        "note": "Use ai.cards or spatial/query tools for local detail; do not invent geometry.",
    }))
}

/// Capped ContextCards for a window (or current view).
pub fn context_cards(s: &Session, p: &Value) -> Result<Value> {
    let d = s.doc()?;
    let limit = (p.get("limit").and_then(Value::as_u64).unwrap_or(MAX_CARDS as u64) as usize).clamp(1, MAX_CARDS);
    let layers = str_list(p, "layers");
    let types = str_list(p, "types");
    let win = match p.get("window") {
        Some(_) => parse_window(p)?,
        None => view_bounds(s).unwrap_or_else(|| {
            let ext = d.extents(&s.space());
            if ext.is_empty() { Bounds2::new(Vec2::new(0.0, 0.0), Vec2::new(1.0, 1.0)) } else { Bounds2::new(ext.min, ext.max) }
        }),
    };

    let mut cards = Vec::new();
    let mut matched = 0usize;
    if let Some(store) = d.space(&s.space()) {
        for e in store.iter().take(MAX_SCAN) {
            if !layer_type_ok(e, &layers, &types) {
                continue;
            }
            let b = aad_doc::entity_bounds(d, e, 0);
            if b.is_empty() || !b.intersects(&win) {
                continue;
            }
            matched += 1;
            if cards.len() < limit {
                cards.push(context_card(d, e));
            }
        }
    }

    let mut out = json!({
        "kind": "context_cards",
        "window": [round(win.min.x), round(win.min.y), round(win.max.x), round(win.max.y)],
        "matched": matched,
        "shown": cards.len(),
        "truncated": matched > cards.len(),
        "cards": cards,
    });
    trim_cards_json(&mut out);
    Ok(out)
}

/// One compact card for prompts / Shilpi-style reasoning.
pub fn context_card(d: &Drawing, e: &Entity) -> Value {
    let b = aad_doc::entity_bounds(d, e, 0);
    let mut card = json!({
        "id": e.handle.hex(),
        "layer": e.common.layer,
        "type": e.kind.type_name(),
    });
    let Some(o) = card.as_object_mut() else { return card };
    if !b.is_empty() {
        o.insert("bbox".into(), json!([round(b.min.x), round(b.min.y), round(b.max.x), round(b.max.y)]));
    }
    match &e.kind {
        EntityKind::Line(l) => {
            let len = (l.b.xy() - l.a.xy()).len();
            o.insert("length".into(), json!(round(len)));
            o.insert("geom".into(), json!(format!("line {:.0},{:.0} → {:.0},{:.0}", l.a.x, l.a.y, l.b.x, l.b.y)));
        }
        EntityKind::Circle(c) => {
            o.insert("radius".into(), json!(round(c.radius)));
            o.insert("geom".into(), json!(format!("circle @ {:.0},{:.0} r={:.0}", c.center.x, c.center.y, c.radius)));
        }
        EntityKind::Arc(a) => {
            o.insert("radius".into(), json!(round(a.radius)));
            o.insert("geom".into(), json!(format!("arc @ {:.0},{:.0} r={:.0}", a.center.x, a.center.y, a.radius)));
        }
        EntityKind::LwPolyline(p) => {
            o.insert("vertices".into(), json!(p.vertices.len()));
            o.insert("closed".into(), json!(p.closed));
            o.insert("geom".into(), json!(format!("pline n={} closed={}", p.vertices.len(), p.closed)));
        }
        EntityKind::Text(t) => {
            let text: String = t.value.chars().take(48).collect();
            o.insert("text".into(), json!(text));
            o.insert("geom".into(), json!("text"));
        }
        EntityKind::MText(t) => {
            let text: String = t.contents.chars().take(48).collect();
            o.insert("text".into(), json!(text));
            o.insert("geom".into(), json!("mtext"));
        }
        EntityKind::Insert(i) => {
            o.insert("block".into(), json!(i.block));
            o.insert("geom".into(), json!(format!("insert {} @ {:.0},{:.0}", i.block, i.insert.x, i.insert.y)));
        }
        _ => {
            o.insert("geom".into(), json!(e.kind.type_name()));
        }
    }
    card
}

fn room_tally(d: &Drawing) -> Value {
    const LABELS: &str = "ROOM-LABELS";
    if d.layer(LABELS).is_none() {
        return json!({ "count": 0 });
    }
    let texts: Vec<&aad_doc::Text> = d
        .model
        .iter()
        .filter(|e| e.common.layer.eq_ignore_ascii_case(LABELS))
        .filter_map(|e| if let EntityKind::Text(t) = &e.kind { Some(t) } else { None })
        .collect();
    let mut names = Vec::new();
    for c in texts.chunks(3) {
        if let [name, size, area] = c
            && area.value.starts_with("Area:")
        {
            names.push(json!({
                "name": name.value,
                "size": size.value,
                "area": area.value.trim_start_matches("Area:").trim(),
            }));
        }
    }
    let count = names.len();
    names.truncate(12);
    json!({ "count": count, "sample": names })
}

fn extents_json(ext: &aad_geom::Bounds2) -> Value {
    if ext.is_empty() {
        return Value::Null;
    }
    json!([round(ext.min.x), round(ext.min.y), round(ext.max.x), round(ext.max.y)])
}

fn view_bounds(s: &Session) -> Option<Bounds2> {
    let v = super::spatialq::view_json(s);
    let (Some(a), Some(b)) = (v["min"].as_array(), v["max"].as_array()) else {
        return None;
    };
    let x0 = a.first()?.as_f64()?;
    let y0 = a.get(1)?.as_f64()?;
    let x1 = b.first()?.as_f64()?;
    let y1 = b.get(1)?.as_f64()?;
    if x0 == x1 || y0 == y1 {
        return None;
    }
    Some(Bounds2::new(Vec2::new(x0.min(x1), y0.min(y1)), Vec2::new(x0.max(x1), y0.max(y1))))
}

fn parse_window(p: &Value) -> Result<Bounds2> {
    match p.get("window").and_then(Value::as_array).map(Vec::as_slice) {
        Some([a, b, c, d]) => match (num(a), num(b), num(c), num(d)) {
            (Some(x0), Some(y0), Some(x1), Some(y1)) if x0 != x1 && y0 != y1 => {
                Ok(Bounds2::new(Vec2::new(x0.min(x1), y0.min(y1)), Vec2::new(x0.max(x1), y0.max(y1))))
            }
            _ => Err(bad("ai.cards", "`window` must be [x0,y0,x1,y1] with two different corners")),
        },
        _ => Err(bad("ai.cards", "`window` must be [x0,y0,x1,y1]")),
    }
}

fn num(v: &Value) -> Option<f64> {
    v.as_f64().filter(|x| x.is_finite() && x.abs() < 1e12)
}

fn str_list(p: &Value, key: &str) -> Vec<String> {
    p.get(key).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).take(32).map(str::to_string).collect()).unwrap_or_default()
}

fn layer_type_ok(e: &Entity, layers: &[String], types: &[String]) -> bool {
    (layers.is_empty() || layers.iter().any(|l| l.eq_ignore_ascii_case(&e.common.layer)))
        && (types.is_empty() || types.iter().any(|t| t.eq_ignore_ascii_case(e.kind.type_name())))
}

fn round(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

fn trim_cards_json(v: &mut Value) {
    loop {
        let len = v.to_string().chars().count();
        let n = v.get("cards").and_then(Value::as_array).map(Vec::len).unwrap_or(0);
        if len <= MAX_CARDS_CHARS || n <= 1 {
            break;
        }
        if let Some(cards) = v.get_mut("cards").and_then(Value::as_array_mut) {
            cards.pop();
        } else {
            break;
        }
    }
    let over = v.to_string().chars().count() > MAX_CARDS_CHARS;
    let n = v.get("cards").and_then(Value::as_array).map(Vec::len).unwrap_or(0);
    if let Some(o) = v.as_object_mut() {
        if over {
            o.insert("truncated".into(), json!(true));
        }
        o.insert("shown".into(), json!(n));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aad_doc::{Common, Entity, Line};
    use aad_geom::Vec3;

    #[test]
    fn card_for_line_has_length() {
        let mut d = Drawing::new_metric();
        let h = d.new_handle();
        let e = Entity {
            handle: h,
            common: Common { layer: "A-WALL".into(), ..Common::default() },
            kind: EntityKind::Line(Line { a: Vec3::new(0.0, 0.0, 0.0), b: Vec3::new(4.0, 0.0, 0.0) }),
        };
        d.space_mut(&Space::Model).unwrap().push(e);
        let stored = d.space(&Space::Model).unwrap().iter().next().unwrap();
        let card = context_card(&d, stored);
        assert_eq!(card["layer"], "A-WALL");
        assert_eq!(card["type"], "Line");
        assert!((card["length"].as_f64().unwrap() - 4.0).abs() < 1e-6);
    }
}
