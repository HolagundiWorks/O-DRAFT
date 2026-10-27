//! Smart blocks: insert with parameters, edit, read back, survive a DXF round trip, and refuse
//! hostile definitions and values without changing the drawing.

use aad_engine::Session;
use aad_engine::doc::EntityKind;
use serde_json::{Value, json};

fn metric() -> Session {
    let mut s = Session::empty();
    s.new_drawing(true);
    s
}

/// (block name, x, y, scale, layer) of the reference `h`.
fn reference(s: &Session, h: &str) -> (String, f64, f64, f64, String) {
    let h = aad_engine::doc::Handle::parse_hex(h).unwrap();
    let e = s.doc().unwrap().entity(h).unwrap();
    let EntityKind::Insert(i) = &e.kind else { panic!("not a block reference") };
    (i.block.clone(), i.insert.x, i.insert.y, i.scale.x, e.common.layer.clone())
}

fn bars(s: &Session, block: &str) -> usize {
    let b = s.doc().unwrap().block(block).unwrap();
    b.entities.iter().filter(|e| matches!(&e.kind, EntityKind::LwPolyline(p) if p.const_width > 0.0)).count()
}

#[test]
fn insert_reuses_blocks_and_edit_repoints_the_reference() {
    let mut s = metric();
    let a = s.execute("smartblock.insert", &json!({ "id": "rcc.column.rect", "at": [1000, 2000] })).unwrap();
    let b = s.execute("smartblock.insert", &json!({ "id": "rcc.column.rect", "at": [5000, 2000] })).unwrap();
    assert_eq!(a["block"], b["block"], "same parameters share one block");
    let h = a["handle"].as_str().unwrap().to_string();
    let (name, x, y, scale, layer) = reference(&s, &h);
    assert_eq!((x, y, scale, layer.as_str()), (1000.0, 2000.0, 1.0, "S-RCC"));
    assert_eq!(bars(&s, &name), 6, "2 bars on each width face and 1 more on each depth face");

    let e = s.execute("smartblock.edit", &json!({ "handle": h, "params": "ny=4, bar=20" })).unwrap();
    let (edited, x2, y2, _, _) = reference(&s, &h);
    assert_ne!(edited, name);
    assert_eq!(e["blocks"][0], edited.as_str());
    assert_eq!((x2, y2), (1000.0, 2000.0), "an edit keeps the position");
    assert_eq!(bars(&s, &edited), 8);
    let got = s.execute("smartblock.get", &json!({ "handle": h })).unwrap();
    assert_eq!(got["id"], "rcc.column.rect");
    assert_eq!(got["params"]["ny"], 4.0);
    assert_eq!(got["params"]["bar"], 20.0);
    assert_eq!(got["params"]["width"], 230.0, "unchanged parameters stay");
    // The other reference is untouched.
    assert_eq!(reference(&s, b["handle"].as_str().unwrap()).0, name);

    s.execute("undo", &json!({})).unwrap();
    assert_eq!(reference(&s, &h).0, name, "undo restores the previous block");
}

#[test]
fn choices_by_label_and_text_labels() {
    let mut s = metric();
    let r = s.execute("smartblock.insert", &json!({ "id": "plumb.riser", "at": [0, 0], "params": { "kind": "RWP", "dia": 100 } })).unwrap();
    let b = s.doc().unwrap().block(r["block"].as_str().unwrap()).unwrap().clone();
    let texts: Vec<String> = b.entities.iter().filter_map(|e| if let EntityKind::Text(t) = &e.kind { Some(t.value.clone()) } else { None }).collect();
    assert_eq!(texts, vec!["RWP 100".to_string()]);
    assert_eq!(r["params"]["kind"], 3.0);
}

#[test]
fn drawing_units_scale_the_millimetre_geometry() {
    let mut s = metric();
    let r = s.execute("smartblock.insert", &json!({ "id": "int.bed", "at": [0, 0], "units": "m" })).unwrap();
    let (_, _, _, scale, _) = reference(&s, r["handle"].as_str().unwrap());
    assert!((scale - 0.001).abs() < 1e-12);
    let r = s.execute("smartblock.insert", &json!({ "id": "int.bed", "at": [0, 0], "scale": 2 })).unwrap();
    assert_eq!(reference(&s, r["handle"].as_str().unwrap()).3, 2.0);
}

#[test]
fn every_library_block_inserts_with_defaults() {
    let mut s = metric();
    let list = s.execute("smartblock.list", &json!({})).unwrap();
    let blocks = list["blocks"].as_array().unwrap();
    assert!(blocks.len() >= 39);
    for (i, b) in blocks.iter().enumerate() {
        let id = b["id"].as_str().unwrap();
        let r = s.execute("smartblock.insert", &json!({ "id": id, "at": [i as f64 * 3000.0, 0] }));
        assert!(r.is_ok(), "{id}: {r:?}");
    }
    let rcc = s.execute("smartblock.list", &json!({ "discipline": "rcc" })).unwrap();
    assert!(rcc["blocks"].as_array().unwrap().iter().all(|b| b["discipline"] == "rcc"));
}

#[test]
fn smart_blocks_survive_a_dxf_round_trip() {
    let mut s = metric();
    let r = s.execute("smartblock.insert", &json!({ "id": "int.sofa", "at": [0, 0], "params": { "seats": 2 } })).unwrap();
    let def = json!({
        "id": "my.tag", "title": "My tag", "layer": "A-TAG",
        "params": [{ "name": "size", "default": 300, "min": 100, "max": 900 }],
        "items": [{ "crect": [0, 0, "size", "size"] }]
    });
    let t = s.execute("smartblock.insert", &json!({ "definition": def, "at": [5000, 0] })).unwrap();
    let text = aad_io::write_dxf(s.doc().unwrap());
    let back = aad_io::read(text.as_bytes(), "sb.dxf").unwrap();
    let mut u = Session::empty();
    u.open_drawing(back, "sb", None);
    let h = r["handle"].as_str().unwrap();
    assert_eq!(u.execute("smartblock.get", &json!({ "handle": h })).unwrap()["params"]["seats"], 2.0);
    u.execute("smartblock.edit", &json!({ "handle": h, "params": { "seats": 4 } })).unwrap();
    assert_eq!(u.execute("smartblock.get", &json!({ "handle": h })).unwrap()["params"]["seats"], 4.0);
    // The inline definition was kept in the drawing, so its reference stays editable.
    let th = t["handle"].as_str().unwrap();
    u.execute("smartblock.edit", &json!({ "handle": th, "params": "size=500" })).unwrap();
    assert_eq!(u.execute("smartblock.get", &json!({ "handle": th })).unwrap()["params"]["size"], 500.0);
    let listed = u.execute("smartblock.list", &json!({})).unwrap();
    assert!(listed["blocks"].as_array().unwrap().iter().any(|b| b["id"] == "my.tag" && b["stored"] == true));
}

fn refused(s: &mut Session, p: Value) {
    let before = s.doc().unwrap().blocks.len();
    let ents = s.doc().unwrap().model.iter().count();
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| s.execute("smartblock.insert", &p)));
    assert!(matches!(r, Ok(Err(_))), "{p} must be refused without a panic: {r:?}");
    assert_eq!(s.doc().unwrap().blocks.len(), before, "{p} made a block");
    assert_eq!(s.doc().unwrap().model.iter().count(), ents, "{p} added objects");
}

#[test]
fn hostile_params_never_panic() {
    let mut s = metric();
    let col = "rcc.column.rect";
    for p in [
        json!({ "id": col }),
        json!({ "id": "nope", "at": [0, 0] }),
        json!({ "id": col, "at": [0, 0], "params": { "width": 10 } }),
        json!({ "id": col, "at": [0, 0], "params": { "width": 1e308 } }),
        json!({ "id": col, "at": [0, 0], "params": { "nx": 2.5 } }),
        json!({ "id": col, "at": [0, 0], "params": { "bar": 17 } }),
        json!({ "id": col, "at": [0, 0], "params": { "bogus": 1 } }),
        json!({ "id": col, "at": [0, 0], "params": "width" }),
        json!({ "id": col, "at": [0, 0], "params": "width=abc" }),
        json!({ "id": col, "at": [0, 0], "params": [1, 2] }),
        json!({ "id": col, "at": [0, 0], "scale": 0 }),
        json!({ "id": col, "at": [0, 0], "scale": -1 }),
        json!({ "id": col, "at": [0, 0], "layer": "a/b" }),
        json!({ "definition": "x", "at": [0, 0] }),
        json!({ "definition": { "id": "x", "title": "X", "items": [{ "repeat": { "var": "i", "count": 1e9, "items": [{ "line": [0, 0, 1, 1] }] } }] }, "at": [0, 0] }),
        json!({ "definition": { "id": "x", "title": "X", "items": [{ "repeat": { "var": "i", "count": 1000, "items": [{ "repeat": { "var": "j", "count": 1000, "items": [{ "line": [0, 0, 1, 1] }] } }] } }] }, "at": [0, 0] }),
        json!({ "definition": { "id": "x", "title": "X", "items": [{ "line": [0, 0, "1/0", 1] }] }, "at": [0, 0] }),
        json!({ "definition": { "id": "x", "title": "X", "items": [{ "line": [0, 0, "sqrt(-1)", 1] }] }, "at": [0, 0] }),
        json!({ "definition": { "id": "x", "title": "X", "items": [{ "line": [0, 0, "1e300*1e300", 1] }] }, "at": [0, 0] }),
        json!({ "definition": { "id": "x", "title": "X", "items": [{ "line": [0, 0, format!("{}1{}", "(".repeat(5000), ")".repeat(5000)), 1] }] }, "at": [0, 0] }),
        json!({ "definition": { "id": "x", "title": "X", "items": [{ "line": [0, 0, "undefined_name", 1] }] }, "at": [0, 0] }),
        json!({ "definition": { "id": "x", "title": "X", "items": [{ "hatch": { "pts": [[0, 0], [1, 0], [1, 1]], "pattern": "NOPE" } }] }, "at": [0, 0] }),
        json!({ "definition": { "id": "x", "title": "X", "items": [{ "line": [0, 0, 1, 1], "linetype": "NOPE" }] }, "at": [0, 0] }),
        json!({ "definition": { "id": "x", "title": "X", "items": [] }, "at": [0, 0] }),
        json!({ "definition": { "id": "bad id!", "title": "X", "items": [{ "line": [0, 0, 1, 1] }] }, "at": [0, 0] }),
        json!({ "definition": { "id": "x", "title": "X", "params": [{ "name": "mm", "default": 1 }], "items": [{ "line": [0, 0, 1, 1] }] }, "at": [0, 0] }),
        json!({ "definition": { "id": "x", "title": "X", "params": [{ "name": "a", "default": 5, "max": 1 }], "items": [{ "line": [0, 0, 1, 1] }] }, "at": [0, 0] }),
        json!({ "definition": { "id": "x", "title": "X", "items": [{ "text": { "at": [0, 0], "height": 0, "value": "x" } }] }, "at": [0, 0] }),
        json!({ "definition": { "id": "x", "title": "X", "items": [{ "poly": { "pts": [[0, 0]] } }] }, "at": [0, 0] }),
        json!({ "definition": { "id": "x", "title": "X", "extra": 1, "items": [{ "line": [0, 0, 1, 1] }] }, "at": [0, 0] }),
    ] {
        refused(&mut s, p);
    }
    // Edits of something that is not a smart block, or with bad values, are refused too.
    let l = s.execute("line", &json!({ "points": [[0, 0], [10, 0]] })).unwrap();
    assert!(s.execute("smartblock.edit", &json!({ "handle": l["handle"], "params": {} })).is_err());
    let r = s.execute("smartblock.insert", &json!({ "id": "rcc.column.rect", "at": [0, 0] })).unwrap();
    assert!(s.execute("smartblock.edit", &json!({ "handle": r["handle"], "params": { "width": -5 } })).is_err());
    assert!(s.execute("smartblock.get", &json!({ "handle": l["handle"] })).is_err());
}

#[test]
fn the_insert_form_lists_the_library() {
    let f = aad_engine::forms::get("smartblock.insert").unwrap();
    let p = aad_engine::forms::parse_input(f, &json!({ "id": "elec.socket", "params": "amp=16", "at": [10, 20] })).unwrap();
    let mut s = metric();
    let r = s.execute("smartblock.insert", &p).unwrap();
    assert_eq!(r["params"]["amp"], 16.0);
}

#[test]
fn the_edit_form_changes_the_selection() {
    let mut s = metric();
    let r = s.execute("smartblock.insert", &json!({ "id": "int.sofa", "at": [0, 0] })).unwrap();
    let h = aad_engine::doc::Handle::parse_hex(r["handle"].as_str().unwrap()).unwrap();
    s.set_selection(vec![h]);
    let f = aad_engine::forms::get("smartblock.edit").unwrap();
    let p = aad_engine::forms::parse_input(f, &json!({ "params": "seats=5" })).unwrap();
    s.execute("smartblock.edit", &p).unwrap();
    assert_eq!(s.execute("smartblock.get", &json!({})).unwrap()["params"]["seats"], 5.0);
}

#[test]
fn every_discipline_dxf_in_the_catalogs_reads() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/blocks");
    let mut n = 0;
    for d in ["plumbing", "hvac", "electrical", "structural", "interior"] {
        let dir = root.join(d);
        let cat: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("catalog.json")).unwrap()).unwrap();
        assert_eq!(cat["licence"], "CC0 1.0");
        for it in cat["items"].as_array().unwrap() {
            let file = dir.join(it["file"].as_str().unwrap());
            let d = aad_io::read(&std::fs::read(&file).unwrap(), &file.to_string_lossy()).unwrap();
            assert!(d.model.iter().count() > 0, "{} is empty", file.display());
            assert!(it["source"].as_str().unwrap().starts_with("https://www.innerscene.com/"));
            n += 1;
        }
    }
    assert_eq!(n, 79);
}
