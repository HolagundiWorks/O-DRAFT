//! Detect-only drawing checks via engine `audit` / `validate` (`fix: false`).

use aad_engine::Session;
use serde_json::{Value, json};

/// Run `audit` with `fix: false`. Never repairs.
pub fn audit_drawing(s: &mut Session) -> Result<Value, String> {
    s.execute("audit", &json!({ "fix": false })).map_err(|e| e.to_string()).map(|mut v| {
        if let Some(o) = v.as_object_mut() {
            o.insert("detectOnly".into(), json!(true));
            o.insert("tool".into(), json!("audit_drawing"));
        }
        v
    })
}

/// Run `validate` with `fix: false`. Never repairs.
pub fn validate_drawing(s: &mut Session) -> Result<Value, String> {
    s.execute("validate", &json!({ "fix": false })).map_err(|e| e.to_string()).map(|mut v| {
        if let Some(o) = v.as_object_mut() {
            o.insert("detectOnly".into(), json!(true));
            o.insert("tool".into(), json!("validate_drawing"));
        }
        v
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_and_validate_are_detect_only_on_empty_drawing() {
        let mut s = Session::new();
        let a = audit_drawing(&mut s).expect("audit");
        assert_eq!(a["detectOnly"], true);
        assert_eq!(a["tool"], "audit_drawing");
        assert_ne!(a["fixed"], true);
        let v = validate_drawing(&mut s).expect("validate");
        assert_eq!(v["detectOnly"], true);
        assert_eq!(v["tool"], "validate_drawing");
        assert_ne!(v["fixed"], true);
        assert_eq!(v["ok"], true);
    }

    #[test]
    fn validate_reports_counts_without_mutating() {
        let mut s = Session::new();
        s.execute("line", &json!({"points": [[0, 0], [10, 0]]})).expect("line");
        let before = s.doc().expect("doc").entity_count();
        let v = validate_drawing(&mut s).expect("validate");
        assert_eq!(s.doc().expect("doc").entity_count(), before);
        assert!(v["entityCount"].as_u64().unwrap_or(0) >= 1);
        assert!(v["counts"].get("Line").is_some());
    }
}
