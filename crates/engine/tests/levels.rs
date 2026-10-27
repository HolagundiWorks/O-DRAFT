//! Levels: named model spaces in one drawing. Phase 1 core (types, switching, commands).

use aad_engine::Session;
use serde_json::{Value, json};

fn line(s: &mut Session, y: f64) {
    s.execute("line", &json!({"points": [[0.0, y], [10.0, y]]})).unwrap();
}

fn levels(s: &mut Session) -> Vec<Value> {
    s.execute("level.list", &json!({})).unwrap()["levels"].as_array().unwrap().clone()
}

fn objects(v: &Value) -> u64 {
    v["objects"].as_u64().unwrap()
}

#[test]
fn a_drawing_without_levels_is_one_implicit_level_until_the_first_new_level() {
    let mut s = Session::new();
    line(&mut s, 0.0);
    let r = s.execute("level.list", &json!({})).unwrap();
    assert_eq!(r["implicit"], true);
    assert_eq!(r["levels"][0]["name"], "Level 1");
    assert_eq!(objects(&r["levels"][0]), 1);
    assert!(!s.doc().unwrap().has_levels(), "listing never changes the drawing");

    s.execute("level.new", &json!({"name": "First", "elevation": 3000.0, "height": 3000.0})).unwrap();
    let ls = levels(&mut s);
    assert_eq!(ls.len(), 2);
    assert_eq!((ls[0]["name"].as_str(), ls[0]["active"].as_bool(), objects(&ls[0])), (Some("Level 1"), Some(true), 1));
    assert_eq!((ls[1]["name"].as_str(), ls[1]["active"].as_bool(), objects(&ls[1])), (Some("First"), Some(false), 0));
    assert_eq!(ls[1]["elevation"], 3000.0);
}

#[test]
fn switching_levels_keeps_each_levels_objects_and_undo_restores_the_tab() {
    let mut s = Session::new();
    line(&mut s, 0.0);
    s.execute("level.new", &json!({"name": "First", "first": "Ground"})).unwrap();
    s.execute("level.activate", &json!({"level": "First"})).unwrap();
    assert_eq!(s.doc().unwrap().model.len(), 0, "the new level starts empty");
    line(&mut s, 5.0);
    line(&mut s, 6.0);

    let ls = levels(&mut s);
    assert_eq!((objects(&ls[0]), objects(&ls[1])), (1, 2));
    assert_eq!(s.doc().unwrap().entity_count(), 3, "drawing-wide counts see every level");

    // A handle on the inactive level is still found, but is not part of the level being edited.
    let ground_handle = s.doc().unwrap().level_store(aad_engine::doc::LevelId(1)).unwrap().iter().next().unwrap().handle;
    assert!(s.doc().unwrap().entity(ground_handle).is_some());
    assert!(!s.doc().unwrap().model.contains(ground_handle));
    let before = s.doc().unwrap().entity(ground_handle).unwrap().clone();
    let moved = s.execute("move", &json!({"handles": [ground_handle.hex()], "from": [0, 0], "to": [1, 1]}));
    assert!(moved.map(|r| r["moved"] == 0).unwrap_or(true), "an object on another level is not moved from this one");
    assert_eq!(*s.doc().unwrap().entity(ground_handle).unwrap(), before, "and it is unchanged");

    s.execute("level.activate", &json!({"level": 1})).unwrap();
    assert_eq!(s.doc().unwrap().model.len(), 1);
    s.execute("undo", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().active_level_ref().unwrap().name, "First", "undo returns to the tab the user was on");
    assert_eq!(s.doc().unwrap().model.len(), 2);
}

#[test]
fn levels_are_renamed_reordered_and_names_stay_unique() {
    let mut s = Session::new();
    s.execute("level.new", &json!({"name": "Ground", "first": "Basement"})).unwrap();
    s.execute("level.new", &json!({"name": "Terrace"})).unwrap();
    s.execute("level.new", &json!({"name": "First", "after": "Ground"})).unwrap();
    let names = |s: &mut Session| levels(s).iter().map(|l| l["name"].as_str().unwrap().to_string()).collect::<Vec<_>>();
    assert_eq!(names(&mut s), ["Basement", "Ground", "First", "Terrace"]);

    assert!(s.execute("level.new", &json!({"name": "ground"})).is_err(), "names are unique ignoring case");
    assert!(s.execute("level.set", &json!({"level": "First", "name": "BASEMENT"})).is_err());
    s.execute("level.set", &json!({"level": "Terrace", "name": "Roof", "elevation": 9000.0, "order": 0})).unwrap();
    assert_eq!(names(&mut s), ["Roof", "Basement", "Ground", "First"]);
    assert_eq!(levels(&mut s)[0]["elevation"], 9000.0);
    s.execute("level.set", &json!({"level": "Roof", "name": "Roof"})).unwrap();
}

#[test]
fn deleting_a_level_needs_force_when_it_holds_objects_and_the_last_one_stays() {
    let mut s = Session::new();
    s.execute("level.new", &json!({"name": "First", "first": "Ground"})).unwrap();
    s.execute("level.activate", &json!({"level": "First"})).unwrap();
    line(&mut s, 0.0);
    assert!(s.execute("level.delete", &json!({"level": "Ground"})).is_ok(), "an empty level goes without force");
    assert_eq!(levels(&mut s).len(), 1);
    assert!(s.execute("level.delete", &json!({"level": "First", "force": true})).is_err(), "the last level cannot be deleted");

    s.execute("level.new", &json!({"name": "Second"})).unwrap();
    let r = s.execute("level.delete", &json!({"level": "First"}));
    assert!(r.is_err(), "First holds an object");
    let r = s.execute("level.delete", &json!({"level": "First", "force": true})).unwrap();
    assert_eq!(r["active"], "Second", "deleting the active level activates its neighbour");
    assert_eq!(s.doc().unwrap().entity_count(), 0);
}

#[test]
fn hostile_level_parameters_never_panic() {
    let mut s = Session::new();
    s.execute("level.new", &json!({"name": "A", "first": "B"})).unwrap();
    let long = "x".repeat(500);
    for (cmd, p) in [
        ("level.new", json!({})),
        ("level.new", json!({"name": ""})),
        ("level.new", json!({"name": long})),
        ("level.new", json!({"name": "ctl\u{7}"})),
        ("level.new", json!({"name": "N", "elevation": "high"})),
        ("level.new", json!({"name": "N", "height": -1.0})),
        ("level.new", json!({"name": "N", "after": "nope"})),
        ("level.set", json!({"level": u64::MAX, "name": "Z"})),
        ("level.set", json!({"level": "A", "order": u64::MAX})),
        ("level.set", json!({"level": "A", "order": -1})),
        ("level.set", json!({"level": "A", "height": f64::NAN})),
        ("level.set", json!({"level": [1]})),
        ("level.delete", json!({"level": null})),
        ("level.activate", json!({"level": "-1"})),
        ("level.activate", json!({})),
    ] {
        let _ = s.execute(cmd, &p);
    }
    let n = levels(&mut s).len();
    assert!((2..=3).contains(&n), "{n} levels");

    // The 64-level cap holds on the JSON path.
    for i in 0..80 {
        let _ = s.execute("level.new", &json!({"name": format!("L{i}")}));
    }
    assert_eq!(levels(&mut s).len(), 64);
}

/// Two levels: `Ground` (active) and `First`, with a block `B` whose only use is on `First`.
fn block_used_only_on_the_other_level() -> Session {
    let mut s = Session::new();
    s.execute("layer.new", &json!({"name": "S-WALL"})).unwrap();
    s.execute("layer.new", &json!({"name": "S-ONLY-FIRST"})).unwrap();
    s.execute("level.new", &json!({"name": "First", "first": "Ground"})).unwrap();
    s.execute("level.activate", &json!({"level": "First"})).unwrap();
    s.execute("layer.current", &json!({"name": "S-ONLY-FIRST"})).unwrap();
    s.execute("line", &json!({"points": [[0, 0], [5, 0]]})).unwrap();
    s.execute("layer.current", &json!({"name": "0"})).unwrap();
    let h = s.doc().unwrap().model.iter().next().unwrap().handle;
    s.execute("block", &json!({"name": "B", "base": [0, 0], "handles": [h.hex()], "keep": "delete"})).unwrap();
    s.execute("insert", &json!({"name": "B", "at": [1, 1]})).unwrap();
    s.execute("level.activate", &json!({"level": "Ground"})).unwrap();
    s
}

#[test]
fn purge_and_layer_delete_see_the_objects_of_every_level() {
    let mut s = block_used_only_on_the_other_level();
    s.execute("purge", &json!({"type": "blocks"})).ok();
    assert!(s.doc().unwrap().block("B").is_some(), "a block used only on another level is not purged");
    assert!(s.execute("layer.delete", &json!({"name": "S-ONLY-FIRST"})).is_err(), "a layer used only on another level is in use");
    assert_eq!(s.doc().unwrap().entity_count(), 1);
}

#[test]
fn renaming_a_layer_repoints_objects_on_every_level() {
    let mut s = block_used_only_on_the_other_level();
    s.execute("layer.set", &json!({"name": "S-WALL", "newName": "S-WALL-2"})).unwrap();
    // the layer the other level's block lives on keeps working after a rename of an unrelated layer
    s.execute("layer.set", &json!({"name": "S-ONLY-FIRST", "newName": "S-FIRST"})).unwrap();
    s.execute("level.activate", &json!({"level": "First"})).unwrap();
    let layers: Vec<String> = s.doc().unwrap().model.iter().map(|e| e.common.layer.clone()).collect();
    assert!(layers.iter().all(|l| l != "S-ONLY-FIRST"), "{layers:?}");
}

#[test]
fn audit_and_extents_do_not_lose_the_other_levels() {
    let mut s = block_used_only_on_the_other_level();
    let r = s.execute("audit", &json!({})).unwrap();
    assert!(r.get("error").is_none());
    assert_eq!(s.doc().unwrap().entity_count(), 1, "audit does not drop objects on a level it is not editing");
}

#[test]
fn levels_round_trip_through_a_dxf_file() {
    let mut s = Session::new();
    line(&mut s, 0.0);
    s.execute("level.new", &json!({"name": "First", "first": "Ground", "elevation": 3000.0})).unwrap();
    s.execute("level.activate", &json!({"level": "First"})).unwrap();
    line(&mut s, 5.0);
    let bytes = aad_io::write(s.doc().unwrap(), "a.dxf").unwrap();
    assert!(aad_io::write(s.doc().unwrap(), "a.svg").is_ok(), "images show the active level");

    let back = aad_io::read(&bytes, "a.dxf").unwrap();
    assert_eq!(back.levels.len(), 2);
    assert_eq!(back.active_level_ref().unwrap().name, "First");
    assert_eq!(back.entity_count(), 2);
    assert_eq!(back.level_by_name("First").unwrap().elevation, 3000.0);
}

fn xs(s: &Session, level: &str) -> usize {
    let d = s.doc().unwrap();
    d.level_store(d.level_by_name(level).unwrap().id).unwrap().len()
}

#[test]
fn objects_move_and_copy_between_levels_at_the_same_coordinates() {
    let mut s = Session::new();
    for y in [0.0, 1.0, 2.0] {
        line(&mut s, y);
    }
    s.execute("level.new", &json!({"name": "First", "first": "Ground"})).unwrap();
    s.execute("level.new", &json!({"name": "Roof"})).unwrap();
    let hs: Vec<String> = s.doc().unwrap().model.iter().map(|e| e.handle.hex()).collect();

    let r = s.execute("level.copyto", &json!({"levels": ["First", "Roof"], "handles": [hs[0], hs[1]]})).unwrap();
    assert_eq!(r["copied"], 4);
    assert_eq!((xs(&s, "Ground"), xs(&s, "First"), xs(&s, "Roof")), (3, 2, 2));
    let d = s.doc().unwrap();
    let first = d.level_store(d.level_by_name("First").unwrap().id).unwrap();
    assert!(first.iter().all(|e| !d.model.contains(e.handle)), "copies get new handles");

    let r = s.execute("level.moveto", &json!({"level": "Roof", "handles": [hs[2]]})).unwrap();
    assert_eq!(r["moved"], 1);
    assert_eq!((xs(&s, "Ground"), xs(&s, "Roof")), (2, 3));
    assert_eq!(s.doc().unwrap().entity_count(), 7);

    assert!(s.execute("level.moveto", &json!({"level": "Ground", "handles": [hs[0]]})).is_err(), "already there");
    assert!(s.execute("level.copyto", &json!({"levels": ["Ground"], "handles": [hs[0]]})).is_err(), "the source level");
    assert!(s.execute("level.copyto", &json!({"levels": [], "handles": [hs[0]]})).is_err());
    assert!(s.execute("level.moveto", &json!({"level": "First", "handles": ["FFFFFF"]})).is_err(), "nothing of this level");

    s.execute("undo", &json!({})).unwrap();
    assert_eq!((xs(&s, "Ground"), xs(&s, "Roof")), (3, 2), "undo restores the move");
}

#[test]
fn objects_on_a_locked_layer_do_not_move_to_another_level() {
    let mut s = Session::new();
    s.execute("layer.new", &json!({"name": "LOCKED"})).unwrap();
    s.execute("layer.current", &json!({"name": "LOCKED"})).unwrap();
    line(&mut s, 0.0);
    s.execute("layer.current", &json!({"name": "0"})).unwrap();
    s.execute("layer.set", &json!({"name": "LOCKED", "locked": true})).unwrap();
    s.execute("level.new", &json!({"name": "First"})).unwrap();
    let h = s.doc().unwrap().model.iter().next().unwrap().handle.hex();
    assert!(s.execute("level.moveto", &json!({"level": "First", "handles": [h]})).is_err());
    assert_eq!(s.doc().unwrap().model.len(), 1);
}

#[test]
fn levels_come_from_the_floor_list_and_a_rename_keeps_the_floor_data_in_step() {
    let mut s = Session::new();
    s.execute("drawing.units", &json!({"units": "mm", "mode": "metadata"})).unwrap();
    line(&mut s, 0.0);
    assert!(s.execute("level.fromfloors", &json!({})).is_err(), "no floor list yet");
    s.execute("floors", &json!({"floors": [{"name": "Ground", "floorToFloor": 3.0}, {"name": "First", "floorToFloor": 3.2}, {"name": "Terrace", "floorToFloor": 3.0}]})).unwrap();
    let r = s.execute("level.fromfloors", &json!({})).unwrap();
    assert_eq!(r["created"], json!(["Ground", "First", "Terrace"]));
    let ls = levels(&mut s);
    assert_eq!(ls.iter().map(|l| l["elevation"].as_f64().unwrap()).collect::<Vec<_>>(), [0.0, 3000.0, 6200.0]);
    assert_eq!(objects(&ls[0]), 1, "the existing objects stay on the first floor");
    assert_eq!(s.execute("level.fromfloors", &json!({})).unwrap()["created"], json!([]), "running it again adds nothing");

    s.execute("level.set", &json!({"level": "First", "name": "Mezzanine"})).unwrap();
    let names: Vec<String> =
        s.execute("floors", &json!({})).unwrap()["floors"].as_array().unwrap().iter().map(|f| f["name"].as_str().unwrap().to_string()).collect();
    assert_eq!(names, ["Ground", "Mezzanine", "Terrace"]);
}

#[test]
fn a_renamed_level_updates_the_floor_field_of_the_title_block() {
    let mut s = Session::new();
    s.execute("floors", &json!({"floors": [{"name": "Ground"}, {"name": "First"}]})).unwrap();
    s.execute("level.fromfloors", &json!({})).unwrap();
    s.execute("layout.set", &json!({"name": "Layout1"})).unwrap();
    s.execute("sheet.titleblock", &json!({"fields": {"FLOOR": "First"}})).unwrap();
    s.execute("level.set", &json!({"level": "First", "name": "Podium"})).unwrap();
    assert_eq!(s.execute("sheet.fields", &json!({})).unwrap()["fields"]["FLOOR"], "Podium");
}

#[test]
fn audit_finds_and_repairs_errors_on_a_level_that_is_not_being_edited() {
    use aad_engine::doc::{Common, Entity, EntityKind, Line};
    use aad_engine::geom::Vec3;
    let mut s = Session::new();
    s.execute("level.new", &json!({"name": "First", "first": "Ground"})).unwrap();
    let first = s.doc().unwrap().level_by_name("First").unwrap().id;
    // An object on a layer that does not exist, on the inactive level.
    {
        let d = s.doc_mut().unwrap();
        let h = d.new_handle();
        let kind = EntityKind::Line(Line { a: Vec3::new(0.0, 0.0, 0.0), b: Vec3::new(1.0, 0.0, 0.0) });
        let e = Entity { handle: h, common: Common { layer: "Ghost".into(), ..Common::default() }, kind };
        d.levels.iter_mut().find(|l| l.id == first).unwrap().entities.push(e);
    }
    let r = s.execute("audit", &json!({})).unwrap();
    assert!(r["errors"].as_u64().unwrap() >= 1, "the error on the other level is reported: {r}");
    s.execute("audit", &json!({"fix": true})).unwrap();
    assert!(s.doc().unwrap().layer("Ghost").is_some(), "and repaired");
    assert_eq!(s.execute("audit", &json!({})).unwrap()["errors"], 0);
}
