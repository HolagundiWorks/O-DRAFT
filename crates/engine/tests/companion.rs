//! Companion pipe commands: clear errors without a host; status shape; hostile pipe names.

use aad_engine::Session;
use aad_engine::doc::EntityKind;
use serde_json::json;

fn metric() -> Session {
    let mut s = Session::empty();
    s.new_drawing(true);
    s
}

#[test]
fn connect_without_host_is_a_clear_error() {
    let mut s = metric();
    let err = s.execute("companion.connect", &json!({})).unwrap_err();
    let msg = err.to_string();
    // Windows: pipe open fails. Other targets: not available.
    assert!(msg.contains("cannot open pipe") || msg.contains("only available on Windows") || msg.contains("companion"), "{msg}");
    let st = s.execute("companion.status", &json!({})).unwrap();
    assert_eq!(st["connected"], false);
}

#[test]
fn disconnect_and_preview_never_panic() {
    let mut s = metric();
    assert_eq!(s.execute("companion.disconnect", &json!({})).unwrap()["connected"], false);
    assert!(s.execute("companion.preview", &json!({})).unwrap()["preview"].is_null());
    assert_eq!(s.execute("companion.unsubscribe", &json!({})).unwrap()["subscribed"], false);
}

#[test]
fn preview_stash_feeds_companion_ghost() {
    let mut s = metric();
    s.execute(
        "companion.preview",
        &json!({
            "result": {
                "lines": [{"x0": 0.0, "y0": 0.0, "x1": 10.0, "y1": 0.0}],
                "circles": [{"cx": 1.0, "cy": 1.0, "r": 2.0}],
                "arcs": [{"cx": 0.0, "cy": 0.0, "r": 5.0, "startAngle": 0.0, "endAngle": 1.0}]
            }
        }),
    )
    .unwrap();
    assert_eq!(s.companion_ghost().len(), 3);
    s.execute("companion.preview", &json!({ "result": null })).unwrap();
    assert!(s.companion_ghost().is_empty());
}

#[test]
fn hostile_pipe_name_is_rejected() {
    let mut s = metric();
    let err = s.execute("companion.connect", &json!({ "pipe": r"..\evil" })).unwrap_err();
    assert!(err.to_string().contains("pipe"), "{err}");
    let err = s.execute("companion.connect", &json!({ "pipe": "x".repeat(100) })).unwrap_err();
    assert!(err.to_string().contains("pipe"), "{err}");
}

#[test]
fn evaluate_without_connect_is_refused() {
    let mut s = metric();
    let err = s.execute("companion.evaluate", &json!({})).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("not connected") || msg.contains("only available on Windows"), "{msg}");
}

#[test]
fn status_reports_availability_flag() {
    let mut s = metric();
    let st = s.execute("companion.status", &json!({})).unwrap();
    assert_eq!(st["available"], cfg!(windows));
    assert_eq!(st["pipe"], "ODraft.Dynamo");
}

#[test]
fn commit_puts_geometry_on_named_levels_and_undo_clears_it() {
    let mut s = metric();
    let result = json!({
        "lines": [
            {"x0": 0.0, "y0": 0.0, "x1": 4000.0, "y1": 0.0, "layer": "A-WALL", "level": "Ground"},
            {"x0": 0.0, "y0": 0.0, "x1": 1000.0, "y1": 0.0, "layer": "S-STRS", "level": "First"}
        ],
        "circles": [{"cx": 500.0, "cy": 500.0, "r": 100.0, "layer": "S-GRID", "level": "Ground"}],
        "arcs": []
    });
    let r = s.execute("companion.commit", &json!({ "result": result })).unwrap();
    assert_eq!(r["committed"], true);
    assert_eq!(r["lines"], 2);
    assert_eq!(r["circles"], 1);
    let levels = r["levels"].as_array().unwrap();
    assert!(levels.iter().any(|v| v == "Ground"));
    assert!(levels.iter().any(|v| v == "First"));

    {
        let d = s.doc().unwrap();
        let ground = d.level_by_name("Ground").expect("Ground level");
        let first = d.level_by_name("First").expect("First level");
        assert_eq!(d.level_store(ground.id).map(|st| st.len()), Some(2), "Ground: wall + circle");
        assert_eq!(d.level_store(first.id).map(|st| st.len()), Some(1), "First: stair line");
    }

    s.undo().unwrap();
    let after = s.doc().unwrap();
    let total: usize = after.model_stores().map(|st| st.len()).sum();
    assert_eq!(total, 0, "undo must remove every committed companion entity");
}

#[test]
fn commit_without_result_is_refused() {
    let mut s = metric();
    let err = s.execute("companion.commit", &json!({})).unwrap_err();
    assert!(err.to_string().contains("no result") || err.to_string().contains("evaluate"), "{err}");
}

#[test]
fn hostile_commit_coords_are_refused() {
    let mut s = metric();
    let err = s
        .execute(
            "companion.commit",
            &json!({ "result": { "lines": [{"x0": "bad", "y0": 0, "x1": 1, "y1": 0, "layer": "0"}], "circles": [], "arcs": [] } }),
        )
        .unwrap_err();
    assert!(err.to_string().contains("finite") || err.to_string().contains("number"), "{err}");
}

#[test]
fn commit_places_known_block_and_warns_on_missing() {
    let mut s = metric();
    let h = s.execute("line", &json!({"points": [[0.0, 0.0], [100.0, 0.0]]})).unwrap()["handles"][0].as_str().unwrap().to_string();
    s.execute("block", &json!({"name": "DOOR-STD", "base": [0, 0], "handles": [h], "keep": "delete"})).unwrap();

    let r = s
        .execute(
            "companion.commit",
            &json!({
                "result": {
                    "lines": [],
                    "circles": [],
                    "arcs": [],
                    "inserts": [
                        {"blockName": "DOOR-STD", "x": 500.0, "y": 0.0, "rotation": 0.0, "level": "Ground"},
                        {"blockName": "MISSING", "x": 0.0, "y": 0.0, "level": "Ground"}
                    ]
                }
            }),
        )
        .unwrap();
    assert_eq!(r["committed"], true);
    assert_eq!(r["inserts"], 1);
    let warnings = r["warnings"].as_array().unwrap();
    assert!(warnings.iter().any(|w| w.as_str().unwrap_or("").contains("MISSING")), "{warnings:?}");

    let d = s.doc().unwrap();
    let ground = d.level_by_name("Ground").expect("Ground");
    let store = d.level_store(ground.id).expect("store");
    let inserts: Vec<_> = store
        .iter()
        .filter_map(|e| match &e.kind {
            EntityKind::Insert(i) => Some(i.block.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(inserts, vec!["DOOR-STD".to_string()]);
}

#[test]
fn commit_puts_annotation_on_sheet_set_not_design() {
    let mut s = metric();
    s.execute("level.new", &json!({"name": "Ground"})).unwrap();
    s.execute("sheetset.new", &json!({"name": "Electrical", "layerPrefix": "E-"})).unwrap();

    let r = s
        .execute(
            "companion.commit",
            &json!({
                "result": {
                    "lines": [],
                    "circles": [],
                    "arcs": [],
                    "texts": [{"text": "DB-1", "x": 100.0, "y": 200.0, "height": 250.0, "level": "Ground", "sheetSet": "Electrical"}],
                    "dimensions": [{"x0": 0.0, "y0": 0.0, "x1": 4000.0, "y1": 0.0, "offset": 500.0, "level": "Ground", "sheetSet": "Electrical"}]
                }
            }),
        )
        .unwrap();
    assert_eq!(r["committed"], true);
    assert_eq!(r["texts"], 1);
    assert_eq!(r["dimensions"], 1);

    let d = s.doc().unwrap();
    assert!(d.active_sheet.is_none(), "commit restores design mode");
    let set = d.sheet_set_by_name("Electrical").expect("Electrical");
    let ground = d.level_by_name("Ground").expect("Ground");
    let ann = d.annotation_store(set.id, ground.id).expect("annotation store");
    assert_eq!(ann.len(), 2);
    assert!(ann.iter().any(|e| matches!(&e.kind, EntityKind::Text(t) if t.value == "DB-1")));
    assert!(ann.iter().any(|e| matches!(&e.kind, EntityKind::Dimension(_))));
    assert_eq!(d.level_store(ground.id).map(|st| st.len()), Some(0), "design store stays empty");
}

#[test]
fn commit_writes_title_fields_on_title_block() {
    let mut s = metric();
    s.execute("layout.set", &json!({"name": "Layout1"})).unwrap();
    s.execute("sheet.titleblock", &json!({"fields": {"REV": "A"}})).unwrap();

    let r = s
        .execute(
            "companion.commit",
            &json!({
                "result": {
                    "lines": [],
                    "circles": [],
                    "arcs": [],
                    "titleFields": [{"field": "REV", "value": "B"}, {"field": "SCALE", "value": "1:100"}]
                }
            }),
        )
        .unwrap();
    assert_eq!(r["committed"], true);
    assert!(r["titleFields"].as_u64().unwrap() >= 1, "{r}");

    let fields = s.execute("sheet.fields", &json!({"layout": "Layout1"})).unwrap()["fields"].clone();
    assert_eq!(fields["REV"], "B");
    assert_eq!(fields["SCALE"], "1:100");
}
