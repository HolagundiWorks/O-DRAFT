//! The project wizard command: details, floors, layer sets, standard tabs and symbols in one step.

use aad_engine::Session;
use serde_json::{Value, json};

fn full() -> Value {
    json!({
        "details": {"title": "Rao House", "owner": "S. Rao", "location": "Hospet", "address": "Plot 4", "architect": "A. Mehta"},
        "units": "mm",
        "floors": {"count": 2, "basements": 1, "terrace": true, "floorToFloor": 3.3},
        "slabMm": 175,
        "beamMm": 450,
        "layers": ["standard"],
        "sheetSets": true,
        "symbols": true
    })
}

fn names(s: &mut Session, cmd: &str, key: &str) -> Vec<String> {
    s.execute(cmd, &json!({})).unwrap()[key].as_array().unwrap().iter().map(|v| v["name"].as_str().unwrap().to_string()).collect()
}

#[test]
fn the_wizard_sets_up_a_whole_project_in_one_undo_step() {
    let mut s = Session::new();
    let r = s.execute("project.wizard", &full()).unwrap();
    assert_eq!(r["floors"], 4, "basement, ground, first, terrace");
    assert_eq!(r["sheetSets"].as_array().unwrap().len(), 5);
    assert!(r["layersCreated"].as_u64().unwrap() > 20 && r["blocksCreated"].as_u64().unwrap() == 29, "{r}");
    assert_eq!(names(&mut s, "level.list", "levels"), ["Basement 1", "Ground Floor", "First Floor", "Terrace"].map(String::from));
    let levels = s.execute("level.list", &json!({})).unwrap()["levels"].clone();
    let elevations: Vec<f64> = levels.as_array().unwrap().iter().map(|l| l["elevation"].as_f64().unwrap()).collect();
    assert!((elevations[0] + 3300.0).abs() < 1e-6 && elevations[1].abs() < 1e-6 && (elevations[2] - 3300.0).abs() < 1e-6, "{elevations:?}");
    let p = s.execute("project", &json!({})).unwrap();
    assert_eq!(
        (p["details"]["title"].as_str(), p["details"]["owner"].as_str(), p["structure"]["slabMm"].as_f64()),
        (Some("Rao House"), Some("S. Rao"), Some(175.0))
    );
    assert!(s.doc().unwrap().layer("A-WALL").is_some() && s.doc().unwrap().layer("E-POINT").is_some() && s.doc().unwrap().block("P-WC").is_some());
    assert_eq!(s.doc().unwrap().active_level_ref().unwrap().name, "Ground Floor", "drafting starts on the ground floor");
    // The floor list carries the chosen height.
    let floors = s.execute("floors", &json!({})).unwrap()["floors"].clone();
    assert!((floors[1]["floorToFloor"].as_f64().unwrap() - 3.3).abs() < 1e-9, "{floors}");
    s.execute("u", &json!({})).unwrap();
    assert!(!s.doc().unwrap().has_levels() && s.doc().unwrap().sheet_sets.is_empty() && s.doc().unwrap().layer("E-POINT").is_none());
    assert!(s.execute("project", &json!({})).unwrap()["details"]["title"].is_null(), "one undo takes the whole wizard back");
}

#[test]
fn the_wizard_can_skip_levels_tabs_symbols_and_layers() {
    let mut s = Session::new();
    s.execute("project.wizard", &json!({"floors": {"count": 3}, "levels": false, "sheetSets": false, "layers": []})).unwrap();
    let d = s.doc().unwrap();
    assert!(!d.has_levels() && d.sheet_sets.is_empty() && d.layer("A-WALL").is_none());
    assert_eq!(s.execute("floors", &json!({})).unwrap()["floors"].as_array().unwrap().len(), 3, "the floor list is still set");
    let mut t = Session::new();
    t.execute("project.wizard", &json!({"sheetSets": ["Electrical"], "symbols": false, "layers": ["standard", "presentation"]})).unwrap();
    assert_eq!(t.doc().unwrap().sheet_sets.len(), 1);
    assert!(t.doc().unwrap().layer("PR-TONE").is_some() && t.doc().unwrap().block("E-SW1").is_none());
}

#[test]
fn a_refused_part_changes_nothing_and_a_second_run_is_refused() {
    let mut s = Session::new();
    for bad in [
        json!({"details": {"title": "x"}, "floors": {"count": 0}}),
        json!({"details": {"title": "x"}, "floors": {"count": 2, "floorToFloor": 1.0}}),
        json!({"details": {"title": "x"}, "layers": ["nonsense"]}),
        json!({"details": {"title": "x"}, "sheetSets": ["Landscape"]}),
        json!({"details": {"title": "x"}, "units": "furlongs"}),
        json!({"details": "x"}),
        json!({"details": {"nonsense": "x"}}),
        json!({"details": {"title": "x"}, "slabMm": -5}),
    ] {
        assert!(s.execute("project.wizard", &bad).is_err(), "{bad}");
        let p = s.execute("project", &json!({})).unwrap();
        assert!(p["details"]["title"].is_null(), "{bad} left something behind");
        assert!(!s.doc().unwrap().has_levels() && s.doc().unwrap().sheet_sets.is_empty());
    }
    s.execute("project.wizard", &json!({})).unwrap();
    assert!(s.execute("project.wizard", &json!({})).is_err(), "the wizard is for a new drawing");
}

#[test]
fn hostile_wizard_parameters_never_panic() {
    for params in [
        json!({"floors": {"count": 1e308}}),
        json!({"floors": {"count": 61}}),
        json!({"floors": {"count": 2, "basements": 1e9, "floorToFloor": f64::MAX}}),
        json!({"floors": [null, 5, {"name": 3}]}),
        json!({"floors": "three"}),
        json!({"layers": vec!["standard"; 100]}),
        json!({"sheetSets": vec!["Electrical"; 100]}),
        json!({"details": {"title": "x".repeat(10_000)}}),
        json!({"units": null, "slabMm": null}),
    ] {
        let mut s = Session::new();
        let _ = s.execute("project.wizard", &params);
    }
}
