//! Export for exchange: one plain DXF per level, and per sheet set and level with annotation.

use std::path::PathBuf;
use std::sync::Once;

use aad_engine::Session;
use aad_engine::cmd::file::{IoHooks, set_io};
use serde_json::{Value, json};

fn install_io() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        set_io(IoHooks {
            read: |b, name| aad_io::read(b, name).map_err(|e| e.to_string()),
            write: |d, name| aad_io::write(d, name).map_err(|e| e.to_string()),
            plot: None,
        });
    });
}

fn temp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("aad-exchange-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

/// Ground (1 wall), First (1 wall) with an Electrical note, Roof (empty), and the five preset sets.
fn project() -> Session {
    install_io();
    let mut s = Session::new();
    s.execute("line", &json!({"points": [[0, 0], [10, 0]]})).unwrap();
    s.execute("level.new", &json!({"name": "First floor", "first": "Ground"})).unwrap();
    s.execute("level.new", &json!({"name": "Roof"})).unwrap();
    s.execute("level.activate", &json!({"level": "First floor"})).unwrap();
    s.execute("line", &json!({"points": [[0, 5], [10, 5]]})).unwrap();
    s.execute("sheetset.new", &json!({"preset": true})).unwrap();
    s.execute("sheetset.activate", &json!({"set": "Electrical"})).unwrap();
    s.execute("text", &json!({"at": [1.0, 6.0], "text": "DB", "height": 0.5})).unwrap();
    s.execute("sheetset.activate", &json!({"set": null})).unwrap();
    s
}

fn read(path: &std::path::Path) -> aad_engine::doc::Drawing {
    aad_io::read(&std::fs::read(path).unwrap(), "x.dxf").unwrap()
}

#[test]
fn each_level_and_each_annotated_sheet_set_becomes_a_plain_dxf() {
    let mut s = project();
    let dir = temp("all");
    let r = s.execute("exchange.export", &json!({"folder": dir.to_string_lossy()})).unwrap();
    let files: Vec<&str> = r["files"].as_array().unwrap().iter().map(|f| f["file"].as_str().unwrap()).collect();
    assert_eq!(files, ["01_Ground.dxf", "02_First_floor.dxf", "02_First_floor__Electrical.dxf", "03_Roof.dxf"]);

    let ground = read(&dir.join("01_Ground.dxf"));
    let first = read(&dir.join("02_First_floor.dxf"));
    let electrical = read(&dir.join("02_First_floor__Electrical.dxf"));
    assert_eq!((ground.model.len(), first.model.len(), electrical.model.len()), (1, 1, 2), "geometry, and geometry plus the note");
    assert_eq!(read(&dir.join("03_Roof.dxf")).model.len(), 0);
    for (name, d) in [("ground", &ground), ("first", &first), ("electrical", &electrical)] {
        assert!(d.levels.is_empty() && d.sheet_sets.is_empty(), "{name}: no level or sheet-set data");
        assert!(d.layouts.iter().all(|l| l.entities.is_empty()), "{name}: no layout content or viewports");
        assert!(d.blocks.keys().all(|k| !k.starts_with("ODRAFT$")), "{name}: no plumbing blocks");
    }
    assert!(electrical.model.iter().any(|e| e.common.layer == "E-TEXT"), "the note keeps its sheet-set layer");

    // The drawing itself is untouched.
    assert_eq!(s.doc().unwrap().levels.len(), 3);
    assert_eq!(s.doc().unwrap().active_level_ref().unwrap().name, "First floor");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn sheet_sets_can_be_left_out_and_the_active_sheet_view_is_exported_too() {
    let mut s = project();
    s.execute("sheetset.activate", &json!({"set": "Electrical"})).unwrap();
    let dir = temp("nosets");
    let r = s.execute("exchange.export", &json!({"folder": dir.to_string_lossy(), "sheetSets": false})).unwrap();
    assert_eq!(r["files"].as_array().unwrap().len(), 3, "levels only");
    assert_eq!(read(&dir.join("02_First_floor.dxf")).model.len(), 1, "the level's geometry, not the annotation in the store being edited");
    let dir2 = temp("sets");
    let r = s.execute("exchange.export", &json!({"folder": dir2.to_string_lossy()})).unwrap();
    assert_eq!(r["files"].as_array().unwrap().len(), 4, "the annotation being edited is exported like any other");
    assert_eq!(read(&dir2.join("02_First_floor__Electrical.dxf")).model.len(), 2);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&dir2);
}

#[test]
fn existing_files_are_kept_unless_overwrite_is_given_and_a_failure_writes_nothing() {
    let mut s = project();
    let dir = temp("over");
    s.execute("exchange.export", &json!({"folder": dir.to_string_lossy()})).unwrap();
    std::fs::write(dir.join("01_Ground.dxf"), b"mine").unwrap();
    assert!(s.execute("exchange.export", &json!({"folder": dir.to_string_lossy()})).is_err());
    assert_eq!(std::fs::read(dir.join("01_Ground.dxf")).unwrap(), b"mine", "an existing file is not replaced");
    s.execute("exchange.export", &json!({"folder": dir.to_string_lossy(), "overwrite": true})).unwrap();
    assert_ne!(std::fs::read(dir.join("01_Ground.dxf")).unwrap(), b"mine");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_drawing_with_one_level_and_bad_folders_are_refused_without_writing() {
    install_io();
    let mut s = Session::new();
    s.execute("line", &json!({"points": [[0, 0], [1, 0]]})).unwrap();
    let dir = temp("one");
    assert!(s.execute("exchange.export", &json!({"folder": dir.to_string_lossy()})).is_err());
    assert!(!dir.exists(), "nothing was created");

    let mut s = project();
    let file = temp("isfile");
    std::fs::write(&file, b"x").unwrap();
    for folder in [Value::Null, json!(""), json!("   "), json!(5), json!("a\0b"), json!(file.to_string_lossy()), json!("x".repeat(2000))] {
        assert!(s.execute("exchange.export", &json!({"folder": folder})).is_err(), "{folder}");
    }
    let _ = std::fs::remove_file(&file);
}

#[test]
fn level_names_that_are_not_file_names_are_made_safe() {
    install_io();
    let mut s = Session::new();
    s.execute("level.new", &json!({"name": "B/2: *Plant?", "first": "..\\evil"})).unwrap();
    let dir = temp("names");
    let r = s.execute("exchange.export", &json!({"folder": dir.to_string_lossy()})).unwrap();
    for f in r["files"].as_array().unwrap() {
        let name = f["file"].as_str().unwrap();
        assert!(!name.contains(['/', '\\', ':', '*', '?']) && name.ends_with(".dxf"), "{name}");
        assert!(dir.join(name).exists());
    }
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2, "files only inside the folder");
    let _ = std::fs::remove_dir_all(&dir);
}
