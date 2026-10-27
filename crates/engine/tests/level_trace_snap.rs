//! Object snaps on traced levels: only traces with snap on, only their snap points.

use aad_engine::Session;
use aad_engine::doc::Space;
use aad_engine::geom::Vec2;
use aad_engine::snap::{mode, osnap};
use serde_json::json;

/// Ground has a line (0,0)-(10,0); First (active) is empty and traces Ground.
fn session(snap: bool) -> Session {
    let mut s = Session::new();
    s.execute("line", &json!({"points": [[0, 0], [10, 0]]})).unwrap();
    s.execute("level.new", &json!({"name": "First", "first": "Ground"})).unwrap();
    s.execute("level.activate", &json!({"level": "First"})).unwrap();
    s.execute("level.trace", &json!({"add": "Ground", "snap": snap})).unwrap();
    s
}

fn endpoint_near(s: &Session, at: Vec2) -> Option<Vec2> {
    osnap(s.doc().unwrap(), &Space::Model, at, 0.5, mode::END, None, false).map(|h| h.point)
}

#[test]
fn a_trace_with_snap_on_offers_its_endpoints() {
    let s = session(true);
    assert_eq!(endpoint_near(&s, Vec2::new(10.1, 0.1)), Some(Vec2::new(10.0, 0.0)));
    assert_eq!(endpoint_near(&s, Vec2::new(0.1, -0.1)), Some(Vec2::new(0.0, 0.0)));
}

#[test]
fn a_trace_without_snap_offers_nothing_and_the_level_itself_still_snaps() {
    let mut s = session(false);
    assert_eq!(endpoint_near(&s, Vec2::new(10.1, 0.1)), None);
    s.execute("line", &json!({"points": [[3, 3], [4, 3]]})).unwrap();
    assert_eq!(endpoint_near(&s, Vec2::new(4.1, 3.0)), Some(Vec2::new(4.0, 3.0)));
}

#[test]
fn snapping_to_a_trace_does_not_pick_or_select_its_objects() {
    let s = session(true);
    let d = s.doc().unwrap();
    assert!(aad_engine::select::pick(d, &Space::Model, Vec2::new(5.0, 0.0), 0.5).is_none(), "traced objects are not pickable");
}

#[test]
fn the_trace_layer_list_limits_the_snap_points() {
    let mut s = session(true);
    s.execute("level.trace", &json!({"set": "Ground", "layers": ["OTHER"]})).unwrap();
    assert_eq!(endpoint_near(&s, Vec2::new(10.1, 0.1)), None, "the line is on layer 0, not in the list");
    s.execute("level.trace", &json!({"set": "Ground", "layers": ["0"]})).unwrap();
    assert!(endpoint_near(&s, Vec2::new(10.1, 0.1)).is_some());
}

#[test]
fn in_a_sheet_view_annotation_snaps_to_the_level_geometry_but_cannot_pick_it() {
    let mut s = Session::new();
    s.execute("line", &json!({"points": [[0, 0], [10, 0]]})).unwrap();
    s.execute("sheetset.new", &json!({"name": "Electrical"})).unwrap();
    s.execute("sheetset.activate", &json!({"set": "Electrical"})).unwrap();
    assert_eq!(endpoint_near(&s, Vec2::new(10.1, 0.1)), Some(Vec2::new(10.0, 0.0)), "a dimension can pick up the wall's end");
    let d = s.doc().unwrap();
    assert!(aad_engine::select::pick(d, &Space::Model, Vec2::new(5.0, 0.0), 0.5).is_none(), "but the wall is not selectable here");
    s.execute("sheetset.activate", &json!({"set": null})).unwrap();
    assert!(aad_engine::select::pick(s.doc().unwrap(), &Space::Model, Vec2::new(5.0, 0.0), 0.5).is_some(), "it is in Design");
}
