//! Layout viewports bound to a level, a sheet set and traces (`viewport.set`, `viewport.add`).

use aad_engine::Session;
use aad_engine::doc::{EntityKind, Handle, Space, Viewport};
use serde_json::{Value, json};

fn wall(s: &mut Session, y: f64) {
    s.execute("line", &json!({"points": [[0.0, y], [10.0, y]]})).unwrap();
}

/// Ground (wall y=0), First (wall y=5) and an Electrical note on First; Layout1 is current.
fn project() -> Session {
    let mut s = Session::new();
    wall(&mut s, 0.0);
    s.execute("level.new", &json!({"name": "First", "first": "Ground"})).unwrap();
    s.execute("level.activate", &json!({"level": "First"})).unwrap();
    wall(&mut s, 5.0);
    s.execute("sheetset.new", &json!({"preset": true})).unwrap();
    s.execute("sheetset.activate", &json!({"set": "Electrical"})).unwrap();
    s.execute("text", &json!({"at": [1.0, 6.0], "text": "DB", "height": 0.5})).unwrap();
    s.execute("sheetset.activate", &json!({"set": null})).unwrap();
    s.execute("layout.set", &json!({"name": "Layout1"})).unwrap();
    s
}

fn add_viewport(s: &mut Session, extra: Value) -> Handle {
    let mut p = json!({"scale": "1:1"});
    if let (Some(o), Some(e)) = (p.as_object_mut(), extra.as_object()) {
        o.extend(e.clone());
    }
    let r = s.execute("viewport.add", &p).unwrap();
    Handle::parse_hex(r["handle"].as_str().unwrap()).unwrap()
}

fn viewport(s: &Session, h: Handle) -> Viewport {
    match &s.doc().unwrap().entity(h).unwrap().kind {
        EntityKind::Viewport(v) => v.clone(),
        k => panic!("{k:?}"),
    }
}

#[test]
fn a_viewport_is_bound_to_a_level_a_sheet_set_and_traces_by_name() {
    let mut s = project();
    let h = add_viewport(&mut s, json!({}));
    let v = viewport(&s, h);
    assert_eq!((v.level, v.sheet_set, v.traces.len()), (None, None, 0), "a new viewport is unbound");

    s.execute(
        "viewport.set",
        &json!({"handle": h.hex(), "level": "First", "sheetSet": "Electrical", "traces": [{"level": "Ground", "fade": 70, "order": "above"}]}),
    )
    .unwrap();
    let d = s.doc().unwrap();
    let v = viewport(&s, h);
    assert_eq!(v.level, Some(d.level_by_name("First").unwrap().id));
    assert_eq!(v.sheet_set, Some(d.sheet_set_by_name("Electrical").unwrap().id));
    assert_eq!((v.traces[0].level, v.traces[0].fade), (d.level_by_name("Ground").unwrap().id, 70));

    // The listings show it, by id.
    let list = s.execute("viewport.list", &json!({})).unwrap();
    let row = list["viewports"].as_array().unwrap().iter().find(|r| r["handle"] == h.hex()).unwrap();
    assert_eq!(row["binding"]["level"], v.level.unwrap().0);
    assert_eq!(row["binding"]["traces"][0]["order"], "above");

    // Clearing, and each part on its own; other viewport settings are untouched.
    s.execute("viewport.set", &json!({"handle": h.hex(), "sheetSet": null})).unwrap();
    assert_eq!(viewport(&s, h).sheet_set, None);
    assert!(viewport(&s, h).level.is_some() && viewport(&s, h).traces.len() == 1);
    s.execute("viewport.set", &json!({"handle": h.hex(), "level": null, "traces": null})).unwrap();
    let v = viewport(&s, h);
    assert_eq!((v.level, v.traces.len()), (None, 0));
    assert!((v.height / v.view_height - 1.0).abs() < 1e-9, "the scale is unchanged");
    s.execute("undo", &json!({})).unwrap();
    assert_eq!(viewport(&s, h).traces.len(), 1, "one undo step");
}

#[test]
fn viewport_add_can_bind_in_the_same_call() {
    let mut s = project();
    let h = add_viewport(&mut s, json!({"level": "Ground", "traces": [{"level": "First"}]}));
    let v = viewport(&s, h);
    assert_eq!(v.level, Some(s.doc().unwrap().level_by_name("Ground").unwrap().id));
    assert_eq!(v.traces.len(), 1);
}

#[test]
fn a_viewport_cannot_trace_the_level_it_shows_and_bad_bindings_change_nothing() {
    let mut s = project();
    let h = add_viewport(&mut s, json!({"level": "First"}));
    let before = viewport(&s, h);
    let bad = |s: &mut Session, p: Value| {
        let mut q = json!({"handle": h.hex()});
        q.as_object_mut().unwrap().extend(p.as_object().unwrap().clone());
        assert!(s.execute("viewport.set", &q).is_err(), "{p}");
    };
    bad(&mut s, json!({"traces": [{"level": "First"}]}));
    bad(&mut s, json!({"level": "Ground", "traces": [{"level": "Ground"}]}));
    bad(&mut s, json!({"level": "Nope"}));
    bad(&mut s, json!({"sheetSet": "Nope"}));
    bad(&mut s, json!({"traces": [{"level": "Ground", "fade": 95}]}));
    bad(&mut s, json!({"traces": [{"level": "Ground"}, {"level": "Ground"}]}));
    bad(&mut s, json!({"traces": "all"}));
    bad(&mut s, json!({"traces": [{}]}));
    assert_eq!(viewport(&s, h), before, "nothing changed");

    // An unbound viewport shows the first level, so it cannot trace that one either.
    let u = add_viewport(&mut s, json!({}));
    assert!(s.execute("viewport.set", &json!({"handle": u.hex(), "traces": [{"level": "Ground"}]})).is_err());
    s.execute("viewport.set", &json!({"handle": u.hex(), "traces": [{"level": "First"}]})).unwrap();
}

#[test]
fn a_locked_viewport_keeps_its_binding_until_unlocked() {
    let mut s = project();
    let h = add_viewport(&mut s, json!({"locked": true}));
    assert!(s.execute("viewport.set", &json!({"handle": h.hex(), "level": "Ground"})).is_err());
    s.execute("viewport.set", &json!({"handle": h.hex(), "level": "Ground", "locked": false})).unwrap();
    assert!(viewport(&s, h).level.is_some());
}

#[test]
fn a_drawing_with_one_level_cannot_bind_but_can_clear() {
    let mut s = Session::new();
    s.execute("layout.set", &json!({"name": "Layout1"})).unwrap();
    let h = add_viewport(&mut s, json!({}));
    assert!(s.execute("viewport.set", &json!({"handle": h.hex(), "level": "Ground"})).is_err());
    assert!(
        s.execute("viewport.set", &json!({"handle": h.hex(), "level": null, "traces": null, "sheetSet": null})).is_ok(),
        "clearing is always allowed"
    );
}

#[test]
fn the_binding_survives_a_dxf_round_trip_and_the_sheet_shows_each_level() {
    let mut s = project();
    // Two viewports side by side on the sheet: Ground, and First with its Electrical annotation.
    let a = add_viewport(&mut s, json!({"p1": [10, 10], "p2": [60, 60], "level": "Ground"}));
    let b = add_viewport(&mut s, json!({"p1": [100, 10], "p2": [150, 60], "level": "First", "sheetSet": "Electrical"}));
    s.execute("viewport.set", &json!({"handle": a.hex(), "viewCenter": [5, 0]})).ok();

    let bytes = aad_io::write(s.doc().unwrap(), "a.dxf").unwrap();
    let back = aad_io::read(&bytes, "a.dxf").unwrap();
    let vp = |h: Handle| match &back.layouts[0].entities.get(h).unwrap().kind {
        EntityKind::Viewport(v) => v.clone(),
        k => panic!("{k:?}"),
    };
    assert_eq!(vp(a).level, Some(back.level_by_name("Ground").unwrap().id));
    assert_eq!(
        (vp(b).level, vp(b).sheet_set),
        (Some(back.level_by_name("First").unwrap().id), Some(back.sheet_set_by_name("Electrical").unwrap().id))
    );

    // The two viewports render different content from one drawing.
    let list = aad_engine::render::build(&back, &Space::Paper("Layout1".into()), &aad_engine::render::Options::default());
    let content = |x0: f64, x1: f64| {
        list.prims
            .iter()
            .filter(|p| p.handle == aad_engine::render::VIEWPORT_CONTENT && list.points(p).first().is_some_and(|q| q.x >= x0 && q.x <= x1))
            .count()
    };
    // First's viewport has its wall plus the annotation text; Ground's has its wall only.
    assert!(content(100.0, 150.0) > content(10.0, 60.0), "{} vs {}", content(100.0, 150.0), content(10.0, 60.0));
}

#[test]
fn hostile_binding_parameters_never_panic() {
    let mut s = project();
    let h = add_viewport(&mut s, json!({}));
    let long = "x".repeat(400);
    for p in [
        json!({"level": u64::MAX}),
        json!({"level": [1]}),
        json!({"level": {"a": 1}}),
        json!({"sheetSet": -1}),
        json!({"sheetSet": long}),
        json!({"traces": [null]}),
        json!({"traces": [[]]}),
        json!({"traces": [{"level": "Ground", "layers": [1]}]}),
        json!({"traces": [{"level": "Ground", "colour": 5}]}),
        json!({"traces": (0..50).map(|i| json!({"level": i})).collect::<Vec<_>>()}),
        json!({"traces": [{"level": "Ground", "fade": f64::NAN}]}),
    ] {
        let mut q = json!({"handle": h.hex()});
        q.as_object_mut().unwrap().extend(p.as_object().unwrap().clone());
        let _ = s.execute("viewport.set", &q);
    }
}

#[test]
fn entering_a_viewport_makes_what_it_shows_the_store_edits_go_to() {
    let mut s = project();
    let a = add_viewport(&mut s, json!({"p1": [10, 10], "p2": [60, 60], "level": "Ground"}));
    let b = add_viewport(&mut s, json!({"p1": [100, 10], "p2": [150, 60], "level": "First", "sheetSet": "Electrical"}));
    let at = |s: &Session| {
        let d = s.doc().unwrap();
        (d.active_level_ref().unwrap().name.clone(), d.active_sheet_ref().map(|x| x.name.clone()))
    };
    assert_eq!(at(&s), ("First".to_string(), None), "the First tab is active in Design");

    s.execute("mspace", &json!({"handle": a.hex()})).unwrap();
    assert_eq!(at(&s), ("Ground".to_string(), None), "Ground's viewport edits Ground");
    s.execute("mspace", &json!({"handle": b.hex()})).unwrap();
    assert_eq!(at(&s), ("First".to_string(), Some("Electrical".to_string())), "the Electrical viewport edits that annotation");
    s.execute("text", &json!({"at": [2.0, 2.0], "text": "from the sheet", "height": 0.5})).unwrap();
    assert_eq!(s.doc().unwrap().model.len(), 2, "the old note and the new one");

    // Entering the same viewport again changes nothing (no spurious edit of the drawing).
    let before = s.state().unwrap().doc.clone();
    s.execute("mspace", &json!({"handle": b.hex()})).unwrap();
    assert!(std::sync::Arc::ptr_eq(&before, &s.state().unwrap().doc), "the same drawing object: nothing was edited");
}

#[test]
fn a_drawing_without_levels_is_not_touched_by_entering_a_viewport() {
    let mut s = Session::new();
    s.execute("layout.set", &json!({"name": "Layout1"})).unwrap();
    let h = add_viewport(&mut s, json!({}));
    let before = s.doc().unwrap().clone();
    s.execute("mspace", &json!({"handle": h.hex()})).unwrap();
    assert_eq!(before, *s.doc().unwrap());
}
