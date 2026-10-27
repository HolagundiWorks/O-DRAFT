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
