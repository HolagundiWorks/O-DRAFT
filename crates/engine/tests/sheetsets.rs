//! Sheet sets: discipline annotation per level, sheet views, layer routing, reference protection.

use aad_engine::Session;
use serde_json::{Value, json};

fn wall(s: &mut Session, y: f64) {
    s.execute("line", &json!({"points": [[0.0, y], [10.0, y]]})).unwrap();
}

fn note(s: &mut Session, text: &str) {
    s.execute("text", &json!({"at": [1.0, 1.0], "text": text, "height": 0.5})).unwrap();
}

fn sets(s: &mut Session) -> Vec<Value> {
    s.execute("sheetset.list", &json!({})).unwrap()["sheetSets"].as_array().unwrap().clone()
}

fn model_layers(s: &Session) -> Vec<String> {
    s.doc().unwrap().model.iter().map(|e| e.common.layer.clone()).collect()
}

/// Two levels (Ground, First), a wall on each, the five preset sheet sets.
fn project() -> Session {
    let mut s = Session::new();
    wall(&mut s, 0.0);
    s.execute("level.new", &json!({"name": "First", "first": "Ground"})).unwrap();
    s.execute("level.activate", &json!({"level": "First"})).unwrap();
    wall(&mut s, 5.0);
    s.execute("sheetset.new", &json!({"preset": true})).unwrap();
    s
}

#[test]
fn the_five_presets_are_created_once_and_names_stay_unique() {
    let mut s = Session::new();
    let r = s.execute("sheetset.new", &json!({"preset": true})).unwrap();
    assert_eq!(r["created"].as_array().unwrap().len(), 5);
    let ls = sets(&mut s);
    let names: Vec<_> = ls.iter().map(|x| x["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Working drawing", "Electrical", "Plumbing", "Interior", "False ceiling"]);
    assert_eq!(ls[1]["layerPrefix"], "E-");
    assert_eq!(s.execute("sheetset.new", &json!({"preset": true})).unwrap()["created"], json!([]), "presets are not duplicated");

    s.execute("sheetset.new", &json!({"name": "Landscape"})).unwrap();
    assert_eq!(sets(&mut s).last().unwrap()["layerPrefix"], "L-", "the default prefix is the first letter");
    assert!(s.execute("sheetset.new", &json!({"name": "electrical"})).is_err(), "names are unique ignoring case");
    s.execute("sheetset.set", &json!({"set": "Landscape", "name": "Garden", "layerPrefix": "G-", "dimStyle": "ISO-25"})).unwrap();
    let g = sets(&mut s).into_iter().find(|x| x["name"] == "Garden").unwrap();
    assert_eq!((g["layerPrefix"].as_str(), g["dimStyle"].as_str()), (Some("G-"), Some("ISO-25")));
    s.execute("sheetset.set", &json!({"set": "Garden", "dimStyle": null})).unwrap();
    assert_eq!(sets(&mut s).into_iter().find(|x| x["name"] == "Garden").unwrap()["dimStyle"], Value::Null);
}

#[test]
fn a_sheet_view_edits_annotation_and_shows_the_level_for_reference() {
    let mut s = project();
    let level_objects = s.doc().unwrap().model.len();
    let wall_handle = s.doc().unwrap().model.iter().next().unwrap().handle;
    let r = s.execute("sheetset.activate", &json!({"set": "Electrical"})).unwrap();
    assert_eq!((r["active"].as_str(), r["level"].as_str(), r["objects"].as_u64()), (Some("Electrical"), Some("First"), Some(0)));
    assert_eq!(s.doc().unwrap().model.len(), 0, "the annotation store starts empty");
    assert_eq!(s.doc().unwrap().reference_store().unwrap().len(), level_objects, "the level is the reference");

    note(&mut s, "DB-1");
    assert_eq!(model_layers(&s), ["E-TEXT"], "new annotation goes on the sheet set's layer, whatever was current");
    assert_eq!(s.doc().unwrap().reference_store().unwrap().len(), level_objects, "the level is untouched");

    // The level's objects are found but cannot be changed from the sheet view.
    assert!(s.doc().unwrap().entity(wall_handle).is_some());
    let moved = s.execute("move", &json!({"handles": [wall_handle.hex()], "from": [0, 0], "to": [1, 1]}));
    assert!(moved.map(|r| r["moved"] == 0).unwrap_or(true));
    assert_eq!(s.doc().unwrap().reference_store().unwrap().len(), level_objects);

    // Back in design mode the level is what is edited again; the annotation waits in the set.
    s.execute("sheetset.activate", &json!({"set": null})).unwrap();
    assert_eq!(s.doc().unwrap().model.len(), level_objects);
    assert!(model_layers(&s).iter().all(|l| l != "E-TEXT"));
    assert_eq!(sets(&mut s)[1]["objects"], 1, "the set holds its one note");
}

#[test]
fn annotation_is_kept_per_level_and_per_sheet_set() {
    let mut s = project();
    s.execute("sheetset.activate", &json!({"set": "Electrical"})).unwrap();
    note(&mut s, "first-floor electrical");
    // Another level in the same sheet set: its own annotation.
    s.execute("level.activate", &json!({"level": "Ground"})).unwrap();
    assert_eq!(s.doc().unwrap().active_sheet_ref().unwrap().name, "Electrical", "the sheet set stays when the level changes");
    assert_eq!(s.doc().unwrap().model.len(), 0);
    note(&mut s, "ground electrical");
    note(&mut s, "ground electrical 2");
    // Another sheet set on the same level: separate again.
    s.execute("sheetset.activate", &json!({"set": "Plumbing"})).unwrap();
    assert_eq!(s.doc().unwrap().model.len(), 0);
    note(&mut s, "ground plumbing");

    s.execute("sheetset.activate", &json!({"set": "Electrical"})).unwrap();
    assert_eq!(s.doc().unwrap().model.len(), 2);
    s.execute("level.activate", &json!({"level": "First"})).unwrap();
    assert_eq!(s.doc().unwrap().model.len(), 1);
    let ls = sets(&mut s);
    assert_eq!((ls[1]["objects"].as_u64(), ls[2]["objects"].as_u64()), (Some(3), Some(1)));
    assert_eq!(s.doc().unwrap().entity_count(), 2 + 4, "two walls and four notes, all found drawing-wide");
}

#[test]
fn undo_restores_the_view_and_a_sheet_view_survives_it() {
    let mut s = project();
    s.execute("sheetset.activate", &json!({"set": "Electrical"})).unwrap();
    note(&mut s, "a");
    s.execute("undo", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().model.len(), 0, "the note is undone");
    assert_eq!(s.doc().unwrap().active_sheet_ref().unwrap().name, "Electrical");
    s.execute("undo", &json!({})).unwrap();
    assert!(s.doc().unwrap().active_sheet.is_none(), "undoing the activation returns to design");
    assert_eq!(s.doc().unwrap().model.len(), 1, "and the level's wall is back in the edited store");
}

#[test]
fn level_edits_that_write_other_stores_are_refused_in_a_sheet_view() {
    let mut s = project();
    s.execute("sheetset.activate", &json!({"set": "Plumbing"})).unwrap();
    note(&mut s, "x");
    let h = s.doc().unwrap().model.iter().next().unwrap().handle.hex();
    assert!(s.execute("level.moveto", &json!({"level": "Ground", "handles": [h]})).is_err());
    assert!(s.execute("level.copyto", &json!({"levels": ["Ground"], "handles": [h]})).is_err());
    assert_eq!(s.doc().unwrap().model.len(), 1);
}

#[test]
fn deleting_a_level_or_a_sheet_set_takes_its_annotation_with_it() {
    let mut s = project();
    s.execute("sheetset.activate", &json!({"set": "Electrical"})).unwrap();
    note(&mut s, "e1");
    s.execute("level.activate", &json!({"level": "Ground"})).unwrap();
    note(&mut s, "e2");
    assert!(s.execute("sheetset.delete", &json!({"set": "Electrical"})).is_err(), "annotation needs force");
    // Deleting the level the user is on moves the view to the neighbour, in the same sheet set.
    s.execute("level.delete", &json!({"level": "Ground", "force": true})).unwrap();
    assert_eq!(s.doc().unwrap().active_level_ref().unwrap().name, "First");
    assert_eq!(s.doc().unwrap().active_sheet_ref().unwrap().name, "Electrical");
    assert_eq!(s.doc().unwrap().entity_count(), 2, "First's wall and its one note remain");
    let r = s.execute("sheetset.delete", &json!({"set": "Electrical", "force": true})).unwrap();
    assert_eq!(r["deleted"], "Electrical");
    assert!(s.doc().unwrap().active_sheet.is_none(), "deleting the active set returns to design");
    assert_eq!(s.doc().unwrap().entity_count(), 1);
    assert_eq!(s.doc().unwrap().model.len(), 1);
}

#[test]
fn next_and_previous_sheet_set_cycle_through_design_and_the_sets() {
    let mut s = Session::new();
    assert!(s.execute("sheetset.next", &json!({})).is_err(), "no sheet sets yet");
    s.execute("sheetset.new", &json!({"name": "A"})).unwrap();
    s.execute("sheetset.new", &json!({"name": "B"})).unwrap();
    let active = |s: &Session| s.doc().unwrap().active_sheet_ref().map(|x| x.name.clone());
    assert_eq!(active(&s), None);
    s.execute("sheetset.next", &json!({})).unwrap();
    assert_eq!(active(&s).as_deref(), Some("A"));
    s.execute("sheetset.next", &json!({})).unwrap();
    s.execute("sheetset.next", &json!({})).unwrap();
    assert_eq!(active(&s), None, "after the last set comes Design");
    s.execute("sheetset.prev", &json!({})).unwrap();
    assert_eq!(active(&s).as_deref(), Some("B"));
    let key = |id: &str| aad_engine::find_command(id).and_then(|c| c.shortcut);
    assert_eq!((key("sheetset.next"), key("sheetset.prev")), (Some("Cmd+Shift+PageDown"), Some("Cmd+Shift+PageUp")));
    assert!(s.doc().unwrap().has_levels(), "entering a sheet view made the implicit level explicit");
}

#[test]
fn purge_and_layer_checks_see_annotation_on_every_level_and_sheet_set() {
    let mut s = project();
    s.execute("layer.new", &json!({"name": "NOTE-ONLY"})).unwrap();
    s.execute("sheetset.activate", &json!({"set": "Interior"})).unwrap();
    s.execute("layer.current", &json!({"name": "NOTE-ONLY"})).unwrap();
    // Annotation already on a sheet-set layer stays where the user put it.
    s.execute("layer.new", &json!({"name": "I-SPECIAL"})).unwrap();
    s.execute("layer.current", &json!({"name": "I-SPECIAL"})).unwrap();
    note(&mut s, "kept");
    assert_eq!(model_layers(&s), ["I-SPECIAL"]);
    s.execute("layer.current", &json!({"name": "0"})).unwrap();
    s.execute("sheetset.activate", &json!({"set": null})).unwrap();
    assert!(s.execute("layer.delete", &json!({"name": "I-SPECIAL"})).is_err(), "a layer used only by a sheet set's annotation is in use");
    s.execute("purge", &json!({"type": "layers"})).ok();
    assert!(s.doc().unwrap().layer("I-SPECIAL").is_some(), "purge keeps it");
}

#[test]
fn hostile_sheet_set_parameters_never_panic() {
    let mut s = project();
    let long = "x".repeat(500);
    for (cmd, p) in [
        ("sheetset.new", json!({})),
        ("sheetset.new", json!({"name": ""})),
        ("sheetset.new", json!({"name": long})),
        ("sheetset.new", json!({"name": "ok", "layerPrefix": "TOO-LONG-PREFIX"})),
        ("sheetset.new", json!({"name": "ok2", "layerPrefix": "a b"})),
        ("sheetset.new", json!({"name": "ok3", "layerPrefix": 5})),
        ("sheetset.new", json!({"name": "ok4", "space": 3})),
        ("sheetset.set", json!({})),
        ("sheetset.set", json!({"set": u64::MAX})),
        ("sheetset.set", json!({"set": "Electrical", "layerPrefix": "bad prefix"})),
        ("sheetset.set", json!({"set": [1]})),
        ("sheetset.delete", json!({"set": null})),
        ("sheetset.activate", json!({"set": "nope"})),
        ("sheetset.activate", json!({"set": -1})),
        ("sheetset.activate", json!({"set": {"a": 1}})),
    ] {
        let _ = s.execute(cmd, &p);
    }
    // The 32-set cap holds on the JSON path.
    for i in 0..40 {
        let _ = s.execute("sheetset.new", &json!({"name": format!("S{i}")}));
    }
    assert_eq!(sets(&mut s).len(), 32);
}

fn reopen(s: &Session) -> aad_engine::doc::Drawing {
    let bytes = aad_io::write(s.doc().unwrap(), "a.dxf").unwrap();
    aad_io::read(&bytes, "a.dxf").unwrap()
}

#[test]
fn sheet_sets_and_their_annotation_survive_a_dxf_round_trip_with_the_sheet_view() {
    let mut s = project();
    s.execute("sheetset.activate", &json!({"set": "Electrical"})).unwrap();
    note(&mut s, "first-floor note");
    s.execute("level.activate", &json!({"level": "Ground"})).unwrap();
    note(&mut s, "ground note");
    s.execute("sheetset.set", &json!({"set": "Electrical", "dimStyle": "ISO-25", "space": "Power"})).unwrap();
    s.execute("sheetset.activate", &json!({"set": "Plumbing"})).unwrap();

    let bytes = aad_io::write(s.doc().unwrap(), "a.dxf").unwrap();
    assert!(String::from_utf8_lossy(&bytes).contains("ODRAFT$ANN$"), "other CAD programs see the annotation as blocks");
    let d = aad_io::read(&bytes, "a.dxf").unwrap();

    let names: Vec<_> = d.sheet_sets.iter().map(|x| x.name.as_str()).collect();
    assert_eq!(names, ["Working drawing", "Electrical", "Plumbing", "Interior", "False ceiling"]);
    let e = d.sheet_set_by_name("Electrical").unwrap();
    assert_eq!((e.layer_prefix.as_str(), e.dim_style.as_deref(), e.space.as_deref()), ("E-", Some("ISO-25"), Some("Power")));
    assert_eq!(d.active_sheet_ref().unwrap().name, "Plumbing", "the sheet view is restored");
    assert_eq!(d.active_level_ref().unwrap().name, "Ground");
    let count = |set: &str, level: &str| {
        d.annotation_store(d.sheet_set_by_name(set).unwrap().id, d.level_by_name(level).unwrap().id).map_or(0, aad_engine::doc::EntityStore::len)
    };
    assert_eq!((count("Electrical", "First"), count("Electrical", "Ground"), count("Plumbing", "Ground")), (1, 1, 0));
    assert_eq!(d.entity_count(), 2 + 2, "two walls and two notes, nothing lost or doubled");
    assert!(d.blocks.keys().all(|k| !k.starts_with("ODRAFT$")), "the plumbing is hidden again");
    assert!(d.layer("ODRAFT-STORES").is_none());
}

#[test]
fn a_drawing_with_sheet_sets_but_one_implicit_level_round_trips() {
    let mut s = Session::new();
    wall(&mut s, 0.0);
    s.execute("sheetset.new", &json!({"name": "Electrical", "layerPrefix": "E-"})).unwrap();
    assert!(!s.doc().unwrap().has_levels());
    let d = reopen(&s);
    assert_eq!(d.sheet_sets.len(), 1);
    assert!(!d.has_levels(), "still the implicit level");
    assert_eq!(d.model.len(), 1);
    assert!(d.active_sheet.is_none());
}

#[test]
fn a_dimension_in_a_sheet_view_attaches_to_the_wall_and_follows_it_when_the_level_is_edited() {
    use aad_engine::doc::{EntityKind, Handle};
    let mut s = Session::new();
    s.execute("line", &json!({"points": [[0, 0], [10, 0]]})).unwrap();
    let wall_h = s.doc().unwrap().model.iter().next().unwrap().handle;
    s.execute("sheetset.new", &json!({"name": "Working drawing", "layerPrefix": "W-"})).unwrap();
    s.execute("sheetset.activate", &json!({"set": "Working drawing"})).unwrap();
    s.execute("dimlinear", &json!({"p1": [0, 0], "p2": [10, 0], "at": [5, 2]})).unwrap();
    assert_eq!(model_layers(&s), ["W-DIM"], "a dimension goes on the set's DIM layer");
    let dim_h: Handle = s.doc().unwrap().model.iter().next().unwrap().handle;
    let assoc = |s: &Session| match &s.doc().unwrap().entity(dim_h).unwrap().kind {
        EntityKind::Dimension(dm) => (dm.assoc.iter().map(|a| a.handle).collect::<Vec<_>>(), dm.p14.x),
        k => panic!("{k:?}"),
    };
    let (hs, x2) = assoc(&s);
    assert!(hs.contains(&wall_h), "the dimension is associated with the wall on the level: {hs:?}");
    assert!((x2 - 10.0).abs() < 1e-9);

    // Edit the wall in Design: the dimension in the annotation store follows it.
    s.execute("sheetset.activate", &json!({"set": null})).unwrap();
    s.execute("stretch", &json!({"window": [[9, -1], [11, 1]], "from": [10, 0], "to": [14, 0]})).unwrap();
    let wall_end = match &s.doc().unwrap().entity(wall_h).unwrap().kind {
        EntityKind::Line(l) => l.b.x,
        k => panic!("{k:?}"),
    };
    assert!((wall_end - 14.0).abs() < 1e-9, "the wall was stretched: {wall_end}");
    let (_, x2) = assoc(&s);
    assert!((x2 - 14.0).abs() < 1e-9, "the dimension in the sheet set followed the wall: {x2}");
}
