//! Electrical layout: block mapping, IDs, wiring analysis and the schedule.

use aad_engine::Session;
use aad_engine::doc::EntityKind;
use serde_json::json;

fn session() -> Session {
    let mut s = Session::new();
    s.execute("drawing.units", &json!({"units": "mm", "mode": "metadata"})).unwrap();
    for name in ["SB", "LP", "SW1"] {
        s.execute("rectang", &json!({"p1": [0, 0], "p2": [200, 200]})).unwrap();
        let h = s.doc().unwrap().model.iter().last().unwrap().handle.hex();
        s.execute("block", &json!({"name": name, "base": [0, 0], "handles": [h], "keep": "delete"})).unwrap();
    }
    s
}

fn place(s: &mut Session, block: &str, x: i64, y: i64) {
    s.execute("insert", &json!({"name": block, "at": [x, y]})).unwrap();
}

fn wire(s: &mut Session, a: [i64; 2], b: [i64; 2]) {
    s.execute("layer.new", &json!({"name": "E-WIRE"})).ok();
    s.execute("layer.current", &json!({"name": "E-WIRE"})).unwrap();
    s.execute("line", &json!({"points": [a, b]})).unwrap();
    s.execute("layer.current", &json!({"name": "0"})).unwrap();
}

fn texts(s: &Session, layer: &str) -> Vec<String> {
    let mut v: Vec<String> = s
        .doc()
        .unwrap()
        .model
        .iter()
        .filter(|e| e.common.layer == layer)
        .filter_map(|e| if let EntityKind::Text(t) = &e.kind { Some(t.value.clone()) } else { None })
        .collect();
    v.sort();
    v
}

#[test]
fn blocks_are_mapped_numbered_and_labelled_and_ids_are_stable() {
    let mut s = session();
    place(&mut s, "LP", 4000, 0);
    place(&mut s, "LP", 2000, 0);
    place(&mut s, "SB", 0, 0);
    let r = s.execute("electrical.blocks", &json!({})).unwrap();
    assert_eq!(r["found"], json!({"LP": 2, "SB": 1}));
    assert!(s.execute("electrical.blocks", &json!({"blocks": {"XX": ["A"]}})).is_err());
    let e = s.execute("electrical.blocks", &json!({"blocks": {"LP": ["LP"], "FP": ["LP"]}})).unwrap_err().to_string();
    assert!(e.contains("both"), "{e}");

    let r = s.execute("electrical.number", &json!({})).unwrap();
    assert_eq!(r["numbered"], 0, "insert already numbered the mapped blocks");
    assert_eq!(texts(&s, "EL-LABELS"), ["LP-01", "LP-02", "SB-01"], "left to right: the LP at x=2000 is LP-01");
    // A new block takes the next number on insert; existing IDs are kept.
    place(&mut s, "LP", 1000, 0);
    assert_eq!(s.execute("electrical.number", &json!({})).unwrap()["numbered"], 0);
    assert_eq!(texts(&s, "EL-LABELS"), ["LP-01", "LP-02", "LP-03", "SB-01"]);
    // Renumbering is explicit.
    s.execute("electrical.number", &json!({"codes": ["LP"], "renumber": true})).unwrap();
    assert_eq!(texts(&s, "EL-LABELS"), ["LP-01", "LP-02", "LP-03", "SB-01"]);
    assert!(s.execute("electrical.number", &json!({"codes": ["ZZ"]})).is_err());
}

#[test]
fn wiring_reaches_boards_through_junctions_and_flags_what_is_not_wired() {
    let mut s = session();
    place(&mut s, "SB", 0, 0);
    place(&mut s, "LP", 2000, 0);
    place(&mut s, "LP", 4000, 0);
    place(&mut s, "LP", 9000, 0);
    place(&mut s, "SB", 0, 5000);
    place(&mut s, "SW1", 2000, 5000);
    wire(&mut s, [200, 100], [2000, 100]);
    wire(&mut s, [2200, 100], [4000, 100]);
    wire(&mut s, [200, 5100], [2000, 5100]);
    let r = s.execute("electrical.connect", &json!({})).unwrap();
    let boards = r["boards"].as_array().unwrap();
    assert_eq!(boards[0]["id"], "SB-01");
    assert_eq!(boards[0]["points"], json!(["LP-01", "LP-02"]), "the run through the first light reaches the second");
    assert_eq!(boards[1]["counts"], json!({"SW1": 1}));
    assert_eq!(r["unwired"], json!(["LP-03"]));
    assert!(r["emptyBoards"].as_array().unwrap().is_empty());
    assert!(texts(&s, "EL-FLAGS").contains(&"NOT WIRED".to_string()));

    // Wiring the stray light clears the flag on the next check.
    wire(&mut s, [4200, 100], [9000, 100]);
    let r = s.execute("electrical.connect", &json!({})).unwrap();
    assert!(r["unwired"].as_array().unwrap().is_empty(), "{r}");
    assert!(texts(&s, "EL-FLAGS").is_empty());
}

#[test]
fn a_run_between_two_boards_is_ambiguous_and_the_schedule_counts_by_board() {
    let mut s = session();
    place(&mut s, "SB", 0, 0);
    place(&mut s, "LP", 2000, 0);
    place(&mut s, "SB", 4000, 0);
    wire(&mut s, [200, 100], [2000, 100]);
    wire(&mut s, [2200, 100], [4000, 100]);
    let r = s.execute("electrical.connect", &json!({})).unwrap();
    assert_eq!(r["ambiguous"], json!(["LP-01"]));

    let r = s.execute("electrical.schedule", &json!({"at": [0, -3000]})).unwrap();
    assert_eq!(r["header"], json!(["SB no", "LP"]));
    assert_eq!(r["rows"][0], json!(["SB-01", "0"]), "a point two boards reach is not counted for either");
    assert_eq!(r["rows"][2], json!(["REACHES 2 BOARDS", "1"]));
    assert_eq!(r["rows"][3], json!(["TOTAL", "1"]), "{r}");
    assert!(texts(&s, "EL-TABLE").contains(&"ELECTRICAL SCHEDULE".to_string()));
    let r = s.execute("electrical.schedule", &json!({"mode": "point", "at": [0, -3000]})).unwrap();
    assert_eq!(r["header"], json!(["Point", "Kind", "Board"]));
    assert!(s.execute("electrical.schedule", &json!({"mode": "weird"})).is_err());

    // UPDATE numbers new blocks, re-checks and redraws the schedule where it was.
    place(&mut s, "LP", 6000, 3000);
    let r = s.execute("electrical.update", &json!({})).unwrap();
    assert_eq!(r["numbered"], 0, "the new light was numbered when it was inserted");
    assert_eq!(texts(&s, "EL-TABLE").iter().filter(|t| *t == "ELECTRICAL POINTS").count(), 1, "one schedule, redrawn");
    assert!(s.execute("electrical.layers", &json!({"lighting": []})).is_err());
    assert_eq!(s.execute("electrical.layers", &json!({"lighting": ["E-LIGHT"]})).unwrap()["lighting"], json!(["E-LIGHT"]));
}

#[test]
fn connect_persists_the_switchboard_wire_table_for_takeoff_record() {
    let mut s = session();
    place(&mut s, "SB", 0, 0);
    place(&mut s, "LP", 2000, 0);
    place(&mut s, "SW1", 2000, 5000);
    place(&mut s, "SB", 0, 5000);
    wire(&mut s, [200, 100], [2000, 100]);
    wire(&mut s, [200, 5100], [2000, 5100]);
    let r = s.execute("electrical.connect", &json!({})).unwrap();
    let wires = r["wires"].as_array().unwrap();
    assert_eq!(wires.len(), 2, "{r}");
    for col in ["switchboard", "height", "5amp_socket", "15amp_socket", "connected_fixture", "two_way_switches", "one_way_switches"] {
        assert!(wires[0].get(col).is_some(), "missing {col} in {r}");
    }
    assert!(wires[0]["height"].is_null());
    let circuits = r["circuits"].as_array().unwrap();
    assert!(!circuits.is_empty(), "{r}");
    for c in circuits {
        assert!(c["connected_fixture"].as_u64().is_some(), "circuit connected_fixture must be a count: {c}");
        assert!(c["kind"].is_string(), "kind stays the point code: {c}");
    }
    let lp = circuits.iter().find(|c| c["kind"] == "LP").expect("LP circuit");
    assert_eq!(lp["connected_fixture"], json!(1));
    let sw = circuits.iter().find(|c| c["kind"] == "SW1").expect("SW1 circuit");
    assert_eq!(sw["connected_fixture"], json!(0), "a switch is not a connected fixture");
    let elec: serde_json::Value = s
        .doc()
        .unwrap()
        .project
        .get("electrical")
        .map(|t| serde_json::from_str(t).unwrap())
        .unwrap_or_else(|| json!({}));
    assert_eq!(elec["wires"].as_array().unwrap().len(), 2);
    assert_eq!(elec["circuits"].as_array().unwrap().len(), circuits.len());

    let path = std::env::temp_dir().join(format!("aad-elec-takeoff-{}.vdb", std::process::id()));
    let rec = s.execute("takeoff.record", &json!({"path": path.to_string_lossy(), "annotate": false})).unwrap();
    assert_eq!(rec["counts"]["electrical"], 2, "{rec}");
    assert!(
        rec["counts"]["electrical_circuits"].as_u64().is_some_and(|n| n >= 2),
        "{rec}"
    );
    let store = aad_vdb::load_path(&path).unwrap();
    let kinds: Vec<u64> = store.records().iter().map(|r| r.id >> 56).collect();
    assert!(kinds.contains(&10), "kind 10 board missing: {kinds:?}");
    assert!(kinds.contains(&11), "kind 11 circuit missing: {kinds:?}");
    let _ = std::fs::remove_file(&path);
}
