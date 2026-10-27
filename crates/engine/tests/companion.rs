//! Companion pipe commands: clear errors without a host; status shape; hostile pipe names.

use aad_engine::Session;
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
    assert!(
        msg.contains("cannot open pipe") || msg.contains("only available on Windows") || msg.contains("companion"),
        "{msg}"
    );
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
    assert!(
        msg.contains("not connected") || msg.contains("only available on Windows"),
        "{msg}"
    );
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
