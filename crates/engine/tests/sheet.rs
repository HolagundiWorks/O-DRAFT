//! Sheets: the title block and its fields, the title-plate layout, saved sheet templates, and viewports.

use aad_engine::Session;
use aad_engine::doc::EntityKind;
use serde_json::{Value, json};

fn session() -> Session {
    let mut s = Session::new();
    s.execute("drawing.units", &json!({"units": "mm", "mode": "metadata"})).unwrap();
    s.execute("project", &json!({"details": {"title": "Rao House", "owner": "S. Rao", "architect": "HC Works"}})).unwrap();
    s.execute("layout.set", &json!({"name": "Layout1"})).unwrap();
    s
}

fn layout_entities(s: &Session, layout: &str) -> Vec<(String, EntityKind)> {
    let d = s.doc().unwrap();
    d.layouts.iter().find(|l| l.name == layout).unwrap().entities.iter().map(|e| (e.common.layer.clone(), e.kind.clone())).collect()
}

fn fields(s: &mut Session) -> Value {
    s.execute("sheet.fields", &json!({})).unwrap()["fields"].clone()
}

#[test]
fn a_title_block_starts_from_the_project_and_its_fields_can_be_edited() {
    let mut s = session();
    let r = s.execute("sheet.titleblock", &json!({})).unwrap();
    assert_eq!(r["created"], true);
    assert_eq!(r["fields"]["PROJECT"], "Rao House");
    assert_eq!(r["fields"]["CLIENT"], "S. Rao");
    assert_eq!(r["fields"]["DRAWN"], "HC Works");
    assert_eq!(r["fields"]["SHEET"], "Layout1");
    assert!(s.doc().unwrap().block("AAD-TITLEBLOCK").is_some());
    let ents = layout_entities(&s, "Layout1");
    assert!(ents.iter().any(|(l, k)| l == "A-ANNO-BORD" && matches!(k, EntityKind::LwPolyline(_))), "the border");

    // The panel sits at the lower right of the printable area, at its real size in millimetres.
    let sheet = aad_engine::render::sheet(s.doc().unwrap(), "Layout1").unwrap();
    let ins = ents.iter().find_map(|(_, k)| if let EntityKind::Insert(i) = k { Some(i.clone()) } else { None }).unwrap();
    assert!((ins.insert.x - sheet.printable.max.x).abs() < 1e-9 && (ins.insert.y - sheet.printable.min.y).abs() < 1e-9);
    assert!((ins.scale.x - 1.0 / sheet.unit_mm).abs() < 1e-9);

    // Fields are set by tag, read back, and a second title block is not added.
    s.execute("sheet.fields", &json!({"fields": {"title": "Ground Floor Plan", "SHEET": "A-101", "rev": "B"}})).unwrap();
    let f = fields(&mut s);
    assert_eq!((f["TITLE"].as_str(), f["SHEET"].as_str(), f["REV"].as_str()), (Some("Ground Floor Plan"), Some("A-101"), Some("B")));
    let again = s.execute("sheet.titleblock", &json!({"fields": {"DATE": "2026-10-11"}})).unwrap();
    assert_eq!(again["created"], false);
    assert_eq!(again["fields"]["DATE"], "2026-10-11");
    assert_eq!(again["fields"]["SHEET"], "A-101", "edited fields are kept");
    assert_eq!(layout_entities(&s, "Layout1").iter().filter(|(_, k)| matches!(k, EntityKind::Insert(_))).count(), 1);

    // Refusals.
    assert!(s.execute("sheet.fields", &json!({"fields": {"TITLE": {"a": 1}}})).is_err());
    assert!(s.execute("sheet.fields", &json!({"fields": {"TITLE": "x".repeat(600)}})).is_err());
    let blank = s.execute("layout.new", &json!({"viewport": false})).unwrap()["name"].as_str().unwrap().to_string();
    assert!(s.execute("sheet.fields", &json!({"layout": blank, "fields": {"TITLE": "x"}})).is_err(), "no title block on the new sheet");
    s.execute("layout.set", &json!({"name": "Model"})).unwrap();
    assert!(s.execute("sheet.titleblock", &json!({})).is_err(), "not in the Model tab");
    assert!(s.execute("sheet.titleblock", &json!({"layout": "Nope"})).is_err());
}

#[test]
fn a_new_drawing_has_landscape_and_portrait_nameplates() {
    let mut s = Session::new();
    let r = s.execute("sheet.designer", &json!({})).unwrap();
    let names = r["layouts"].as_array().unwrap();
    assert!(names.iter().any(|n| n == "Nameplate L"));
    assert!(names.iter().any(|n| n == "Nameplate P"));
    assert_eq!(r["name"], "Nameplate L");
    assert_eq!(r["created"], true);
    assert!(matches!(s.layout_space(), aad_engine::doc::Space::Model), "preparing the plate stays in model space");
    let again = s.execute("sheet.designer", &json!({"landscape": false, "open": true})).unwrap();
    assert_eq!(again["name"], "Nameplate P");
    assert!(matches!(s.layout_space(), aad_engine::doc::Space::Paper(n) if n == "Nameplate P"));
    let page = s.doc().unwrap().layouts.iter().find(|l| l.name == "Nameplate P").unwrap().page.clone();
    assert!(!page.landscape, "portrait plate");
    let page_l = s.doc().unwrap().layouts.iter().find(|l| l.name == "Nameplate L").unwrap().page.clone();
    assert!(page_l.landscape, "landscape plate");

    let mut drawn = Session::new();
    drawn.execute("layout.set", &json!({"name": "Layout1"})).unwrap();
    drawn.execute("line", &json!({"points": [[0, 0], [10, 0]]})).unwrap();
    let kept = drawn.execute("sheet.designer", &json!({})).unwrap();
    let names = kept["layouts"].as_array().unwrap();
    assert!(names.iter().any(|n| n == "Layout1"));
    assert!(names.iter().any(|n| n == "Nameplate L") && names.iter().any(|n| n == "Nameplate P"));
}

#[test]
fn plot_window_fits_the_model_at_a_standard_scale_on_the_nameplate() {
    let mut s = Session::new();
    s.execute("drawing.units", &json!({"units": "mm", "mode": "metadata"})).unwrap();
    // Apartment-sized extents → needs about 1:100 on A1 landscape above the title block.
    s.execute("line", &json!({"points": [[0.0, 0.0], [12000.0, 0.0]]})).unwrap();
    s.execute("line", &json!({"points": [[0.0, 0.0], [0.0, 7000.0]]})).unwrap();
    s.execute("sheet.designer", &json!({})).unwrap();
    let r = s.execute("sheet.plotwindow", &json!({"layout": "Nameplate L"})).unwrap();
    assert_eq!(r["layout"], "Nameplate L");
    assert_eq!(r["created"], true);
    assert!(r["locked"].as_bool().unwrap());
    let scale = r["scaleText"].as_str().unwrap();
    assert!(scale.starts_with("1:"), "standard shrink scale, got {scale}");
    assert_eq!(s.execute("sheet.fields", &json!({"layout": "Nameplate L"})).unwrap()["fields"]["SCALE"], scale);
    assert!(matches!(s.layout_space(), aad_engine::doc::Space::Paper(n) if n == "Nameplate L"));
    let again = s.execute("sheet.plotwindow", &json!({"layout": "Nameplate L"})).unwrap();
    assert_eq!(again["created"], false);
    assert_eq!(again["handle"], r["handle"]);
    assert_eq!(s.execute("viewport.list", &json!({"layout": "Nameplate L"})).unwrap()["viewports"].as_array().unwrap().len(), 1);

    let win = s.execute("sheet.plotwindow", &json!({"layout": "Nameplate L", "window": [0.0, 0.0, 4000.0, 3000.0]})).unwrap();
    assert_eq!(win["created"], false);
    let v = &s.execute("viewport.list", &json!({"layout": "Nameplate L"})).unwrap()["viewports"][0];
    assert!((v["viewCenter"][0].as_f64().unwrap() - 2000.0).abs() < 1.0);
    assert!((v["viewCenter"][1].as_f64().unwrap() - 1500.0).abs() < 1.0);

    // Portrait nameplate is independent.
    let p = s.execute("sheet.plotwindow", &json!({"layout": "Nameplate P"})).unwrap();
    assert_eq!(p["layout"], "Nameplate P");
    assert_eq!(p["created"], true);
}

#[test]
fn viewports_are_listed_added_scaled_and_removed() {
    let mut s = session();
    s.execute("line", &json!({"points": [[0, 0], [20000, 0]]})).ok();
    s.execute("sheet.titleblock", &json!({})).unwrap();
    let r = s.execute("viewport.add", &json!({"scale": "1:100"})).unwrap();
    let h = r["handle"].as_str().unwrap().to_string();
    let l = s.execute("viewport.list", &json!({})).unwrap();
    let v = &l["viewports"][0];
    assert_eq!((v["handle"].as_str(), v["scaleText"].as_str()), (Some(h.as_str()), Some("1:100")));
    let sheet = aad_engine::render::sheet(s.doc().unwrap(), "Layout1").unwrap();
    let rect: Vec<f64> = v["rect"].as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect();
    assert!((rect[1] - (sheet.printable.min.y + 56.0 / sheet.unit_mm)).abs() < 1e-6, "above the title block: {rect:?}");
    assert_eq!(s.execute("sheet.fields", &json!({})).unwrap()["fields"]["SCALE"], "1:100", "the title block shows the new viewport's scale");
    s.execute("viewport.set", &json!({"handle": h, "scale": "1:50", "locked": true})).unwrap();
    assert_eq!(s.execute("viewport.list", &json!({})).unwrap()["viewports"][0]["scaleText"], "1:50");
    assert_eq!(s.execute("viewport.list", &json!({})).unwrap()["viewports"][0]["locked"], true);

    assert!(s.execute("viewport.remove", &json!({"handle": "FFFF"})).is_err(), "not a viewport");
    assert!(s.execute("viewport.remove", &json!({})).is_err());
    s.execute("viewport.remove", &json!({"handle": h})).unwrap();
    assert_eq!(s.execute("viewport.list", &json!({})).unwrap()["viewports"].as_array().unwrap().len(), 0);
}

#[test]
fn a_saved_sheet_is_applied_as_a_title_card_and_replaced_without_touching_the_users_work() {
    let mut s = session();
    s.execute("pagesetup", &json!({"paper": "A3", "landscape": true})).unwrap();
    s.execute("sheet.titleblock", &json!({"fields": {"REV": "A"}})).unwrap();
    s.execute("viewport.add", &json!({"scale": "1:100"})).unwrap();
    let saved = s.execute("sheet.template.save", &json!({"name": "A3 office"})).unwrap();
    assert!(saved["entities"].as_u64().unwrap() >= 2 && saved["blocks"].as_u64().unwrap() >= 1);
    let list = s.execute("sheet.template.list", &json!({})).unwrap();
    assert_eq!((list["templates"][0]["name"].as_str(), list["templates"][0]["scope"].as_str()), (Some("A3 office"), Some("drawing")));

    // A new sheet gets the page setup, the border and the title block, filled from the project.
    let name = s.execute("layout.new", &json!({"viewport": false})).unwrap()["name"].as_str().unwrap().to_string();
    let before = layout_entities(&s, &name).len();
    let r = s.execute("sheet.template.apply", &json!({"name": "A3 office", "layout": name, "fields": {"TITLE": "First Floor Plan"}})).unwrap();
    assert!(r["placed"].as_u64().unwrap() >= 2);
    assert_eq!(r["replaced"], 0);
    let page = s.doc().unwrap().layouts.iter().find(|l| l.name == name).unwrap().page.clone();
    assert!(page.landscape && page.paper.contains("A3"), "{}", page.paper);
    let f = s.execute("sheet.fields", &json!({"layout": name})).unwrap()["fields"].clone();
    assert_eq!((f["PROJECT"].as_str(), f["TITLE"].as_str(), f["REV"].as_str()), (Some("Rao House"), Some("First Floor Plan"), Some("A")));
    assert!(layout_entities(&s, &name).len() > before);
    assert!(layout_entities(&s, &name).iter().all(|(_, k)| !matches!(k, EntityKind::Viewport(v) if v.id != 1)), "templates carry no viewports");

    // The user draws a note; applying the template again replaces only what the template placed.
    s.execute("layout.set", &json!({"name": name})).unwrap();
    s.execute("line", &json!({"points": [[10, 10], [60, 10]]})).unwrap();
    let count = layout_entities(&s, &name).len();
    let r = s.execute("sheet.template.apply", &json!({"name": "A3 office", "layout": name})).unwrap();
    assert!(r["replaced"].as_u64().unwrap() >= 2);
    assert_eq!(layout_entities(&s, &name).len(), count, "no duplicates, and the user's line stayed");
    assert!(layout_entities(&s, &name).iter().any(|(_, k)| matches!(k, EntityKind::Line(_))));

    // Unknown names and damaged templates are refused and change nothing.
    assert!(s.execute("sheet.template.apply", &json!({"name": "Nope"})).is_err());
    let mut all = s.doc().unwrap().project.get("sheetTemplates").cloned().map(|t| serde_json::from_str::<Value>(&t).unwrap()).unwrap();
    all["Broken"] = json!({"format": "aad-sheet-template", "page": 3, "entities": []});
    all["Bad object"] = json!({"format": "aad-sheet-template", "page": all["A3 office"]["page"].clone(), "entities": [{"nonsense": true}]});
    s.doc_mut().unwrap().project.insert("sheetTemplates".into(), all.to_string());
    for n in ["Broken", "Bad object"] {
        let c = layout_entities(&s, &name).len();
        assert!(s.execute("sheet.template.apply", &json!({"name": n, "layout": name})).is_err(), "{n}");
        assert_eq!(layout_entities(&s, &name).len(), c, "{n} changed nothing");
    }
    assert!(s.execute("sheet.template.save", &json!({"name": ""})).is_err());
    assert!(s.execute("sheet.template.save", &json!({"name": "x", "scope": "cloud"})).is_err());
    s.execute("sheet.template.delete", &json!({"name": "Broken"})).unwrap();
    assert!(s.execute("sheet.template.delete", &json!({"name": "Broken"})).is_err());
}

#[test]
fn templates_can_live_in_a_shared_folder_for_the_whole_office() {
    let mut s = session();
    let dir = std::env::temp_dir().join(format!("aad-sheet-tpl-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    let folder = dir.to_str().unwrap();
    s.execute("sheet.titleblock", &json!({})).unwrap();
    s.execute("sheet.template.save", &json!({"name": "Office A4", "scope": "user", "folder": folder})).unwrap();
    assert!(dir.join("Office A4.json").is_file());
    let l = s.execute("sheet.template.list", &json!({"folder": folder})).unwrap();
    assert!(l["templates"].as_array().unwrap().iter().any(|t| t["name"] == "Office A4" && t["scope"] == "user"));
    // Another drawing finds it and applies it.
    let mut t = session();
    t.execute("layout.new", &json!({})).unwrap();
    t.execute("sheet.template.apply", &json!({"name": "Office A4", "folder": folder})).unwrap();
    assert_eq!(t.execute("sheet.fields", &json!({})).unwrap()["fields"]["PROJECT"], "Rao House");
    t.execute("sheet.template.delete", &json!({"name": "Office A4", "scope": "user", "folder": folder})).unwrap();
    assert!(!dir.join("Office A4.json").exists());
    // A junk file in the folder is ignored, not a crash.
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("junk.json"), "{not json").unwrap();
    assert!(s.execute("sheet.template.list", &json!({"folder": folder})).unwrap()["templates"].as_array().unwrap().is_empty());
    std::fs::remove_dir_all(&dir).ok();
}

fn three_sheets() -> Session {
    let mut s = session();
    for (i, title) in ["Ground Floor Plan", "First Floor Plan"].into_iter().enumerate() {
        let name = if i == 0 { "Layout1".to_string() } else { "Layout3".to_string() };
        if i > 0 {
            s.execute("layout.new", &json!({"name": name})).unwrap();
        }
        s.execute("sheet.titleblock", &json!({"layout": name, "fields": {"TITLE": title}})).unwrap();
    }
    s
}

#[test]
fn the_sheet_index_lists_sheets_and_renumbers_them() {
    let mut s = three_sheets();
    let r = s.execute("sheet.index", &json!({})).unwrap();
    let rows = r["sheets"].as_array().unwrap();
    assert!(rows.iter().any(|x| x["layout"] == "Layout3" && x["title"] == "First Floor Plan"));
    assert!(rows.iter().all(|x| !x["layout"].as_str().unwrap().starts_with("Nameplate")));
    assert_eq!(r["renumbered"], 0);

    let r = s.execute("sheet.index", &json!({"renumber": {"prefix": "A-", "start": 101, "step": 10}})).unwrap();
    assert_eq!(r["renumbered"], 2);
    let numbers: Vec<&str> =
        r["sheets"].as_array().unwrap().iter().filter(|x| x["titleBlock"] == true).map(|x| x["number"].as_str().unwrap()).collect();
    assert_eq!(numbers, ["A-101", "A-111"]);
}

#[test]
fn drawing_the_sheet_index_twice_replaces_the_first_table() {
    let mut s = three_sheets();
    s.execute("layout.set", &json!({"name": "Layout1"})).unwrap();
    let count = |s: &Session| layout_entities(s, "Layout1").iter().filter(|(l, _)| l == "A-ANNO-SIDX").count();
    let r = s.execute("sheet.index", &json!({"draw": true})).unwrap();
    let first = count(&s);
    assert!(first > 0 && r["drawn"]["objects"].as_u64() == Some(first as u64));
    s.execute("sheet.index", &json!({"draw": true})).unwrap();
    assert_eq!(count(&s), first);
    s.execute("undo", &json!({})).unwrap();
    assert_eq!(count(&s), first, "undo restores the first table");
}

#[test]
fn hostile_sheet_index_params_never_panic() {
    let mut s = three_sheets();
    for p in [
        json!({"renumber": {"start": i64::MAX, "step": 5}}),
        json!({"renumber": {"start": "x"}}),
        json!({"renumber": {"prefix": "A".repeat(500)}}),
        json!({"draw": true, "height": 0}),
        json!({"draw": true, "height": f64::NAN}),
        json!({"draw": true, "layout": "nope"}),
        json!({"draw": true, "at": [1e300, -1e300]}),
    ] {
        let _ = s.execute("sheet.index", &p);
    }
    let r = s.execute("sheet.index", &json!({})).unwrap();
    assert_ne!(r["sheets"][0]["number"], "A-9223372036854775807", "an overflowing range changes nothing");
}
