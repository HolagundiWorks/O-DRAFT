//! The project's details and the company's details, as the Project panel reads and edits them.

use aad_engine::Session;
use serde_json::json;

#[test]
fn project_reports_every_detail_and_the_company_the_drawing_holds() {
    let mut s = Session::new();
    let r = s.execute("project", &json!({})).unwrap();
    assert!(r["company"].is_null(), "no company file has been read yet");
    s.execute(
        "project",
        &json!({"title": "Rao House", "owner": "S. Rao", "location": "Hospet", "address": "Plot 4", "pid": "P-07", "siteArea": "186.5", "plotUse": "Residential", "authority": "HUDA", "date": "2026-10-27", "architect": "A. Mehta", "engineer": "K. Iyer", "summary": "Two storey"}),
    )
    .unwrap();
    let r = s.execute("project", &json!({})).unwrap();
    for (key, want) in [
        ("title", "Rao House"),
        ("owner", "S. Rao"),
        ("location", "Hospet"),
        ("address", "Plot 4"),
        ("pid", "P-07"),
        ("siteArea", "186.5"),
        ("plotUse", "Residential"),
        ("authority", "HUDA"),
        ("date", "2026-10-27"),
        ("architect", "A. Mehta"),
        ("engineer", "K. Iyer"),
        ("summary", "Two storey"),
    ] {
        assert_eq!(r["details"][key], want, "{key}");
    }
    // The company file's answer is kept in the drawing and reported back, and it also fills the
    // project's own details it knows (the architect, the date).
    s.execute(
        "companyfile",
        &json!({"context": {"v": 1, "company": {"name": "Aurelia Design", "address": "12 Park Road", "phone": "+91 99999 00000", "email": "studio@example.com", "architect_name": "A. Rao", "coa_registration": "CA/2001/12345"}, "prepared_on": "2026-10-28"}}),
    )
    .unwrap();
    let r = s.execute("project", &json!({})).unwrap();
    assert_eq!(r["company"]["name"], "Aurelia Design");
    assert_eq!(r["company"]["phone"], "+91 99999 00000");
    assert_eq!(r["company"]["architect_name"], "A. Rao");
    assert_eq!(r["details"]["architect"], "A. Rao", "the named architect prepares the drawing");
    assert_eq!(r["details"]["title"], "Rao House", "what the company file does not say is kept");
}

#[test]
fn project_details_are_bounded_and_the_company_cannot_be_edited_through_them() {
    let mut s = Session::new();
    assert!(s.execute("project", &json!({"details": {"title": "x".repeat(501)}})).is_err());
    assert!(s.execute("project", &json!({"details": {"company": "Evil"}})).is_err(), "company is not a project detail");
    assert!(s.execute("project", &json!({"details": {"title": {"a": 1}}})).is_err(), "a detail is text");
    let r = s.execute("project", &json!({})).unwrap();
    assert!(r["details"]["title"].is_null(), "refused edits changed nothing");
    assert!(r["company"].is_null());
}
