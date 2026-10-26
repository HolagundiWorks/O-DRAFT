//! Place from the Blocks panel: name then point at drawing scale.

use aad_engine::Session;
use aad_engine::doc::EntityKind;
use serde_json::json;

#[test]
fn place_asks_only_for_the_insertion_point() {
    let mut s = Session::new();
    s.execute("line", &json!({ "points": [[0, 0], [10, 0]] })).unwrap();
    s.execute("block", &json!({ "name": "B1", "base": [0, 0], "keep": "delete", "handles": ["100"] })).unwrap();
    s.script("INSERT.PLACE B1 50,25\n").unwrap();
    assert!(s.running.is_none(), "place finished: {}", s.prompt_text());
    let inserts: Vec<_> = s
        .doc()
        .unwrap()
        .model
        .iter()
        .filter_map(|e| match &e.kind {
            EntityKind::Insert(i) => Some((i.insert.x, i.insert.y, i.scale.x, i.scale.y, i.rotation)),
            _ => None,
        })
        .collect();
    assert_eq!(inserts.len(), 1);
    assert!((inserts[0].0 - 50.0).abs() < 1e-9);
    assert!((inserts[0].1 - 25.0).abs() < 1e-9);
    assert!((inserts[0].2 - 1.0).abs() < 1e-9);
    assert!((inserts[0].3 - 1.0).abs() < 1e-9);
    assert!(inserts[0].4.abs() < 1e-9);
}

#[test]
fn blocks_preview_returns_unit_square_segments() {
    let mut s = Session::new();
    s.execute("line", &json!({ "points": [[0, 0], [10, 0]] })).unwrap();
    s.execute("block", &json!({ "name": "B1", "base": [0, 0], "keep": "delete", "handles": ["100"] })).unwrap();
    let v = s.execute("blocks.preview", &json!({ "name": "B1" })).unwrap();
    assert_eq!(v["solid"], false);
    let segs = v["segments"].as_array().expect("segments");
    assert!(!segs.is_empty());
    for seg in segs {
        for pt in seg.as_array().unwrap() {
            let x = pt[0].as_f64().unwrap();
            let y = pt[1].as_f64().unwrap();
            assert!((0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y), "{x},{y}");
        }
    }
}
