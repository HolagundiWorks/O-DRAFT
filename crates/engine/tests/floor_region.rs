//! Floor region declare / export / restore of take-off tables from the project file.

use std::sync::Once;

use aad_engine::Session;
use aad_engine::cmd::file::{IoHooks, set_io};
use serde_json::json;

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

fn session() -> Session {
    install_io();
    let mut s = Session::new();
    s.execute("drawing.units", &json!({"units": "mm", "mode": "metadata"})).unwrap();
    s
}

#[test]
fn declare_assigns_entities_in_window_and_export_writes_them() {
    let mut s = session();
    s.execute("floors", &json!({"floors": [{"name": "Ground", "floorToFloor": 3.0, "ceiling": 2.85, "lintel": 2.1, "slab": 150, "beam": 450}]}))
        .unwrap();
    s.execute("line", &json!({"points": [[0.0, 0.0], [5000.0, 0.0]]})).unwrap();
    s.execute("line", &json!({"points": [[0.0, 1000.0], [5000.0, 1000.0]]})).unwrap();
    s.execute("line", &json!({"points": [[20000.0, 0.0], [25000.0, 0.0]]})).unwrap();

    let r = s.execute("floor.declare", &json!({"name": "Ground", "window": [[-100.0, -100.0], [6000.0, 2000.0]], "crossing": true})).unwrap();
    assert_eq!(r["name"], "Ground");
    assert_eq!(r["entities"], 2, "only the two lines inside the window");

    let list = s.execute("floor.regions", &json!({})).unwrap();
    assert_eq!(list["regions"].as_array().unwrap().len(), 1);

    let dir = std::env::temp_dir().join(format!("aad-floor-region-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("Ground.dxf");
    let exp = s.execute("floor.export", &json!({"name": "Ground", "path": path.to_string_lossy()})).unwrap();
    assert_eq!(exp["entities"], 2);
    assert!(path.is_file(), "exported floor drawing exists");
}

#[test]
fn takeoff_tables_round_trip_through_the_project_file() {
    let mut s = session();
    s.execute("takeoff.start", &json!({})).unwrap();
    s.execute("layer.current", &json!({"name": "MEASURE-FULLBRICK"})).unwrap();
    s.execute("line", &json!({"points": [[0.0, 0.0], [9000.0, 0.0]]})).unwrap();
    s.execute("takeoff.brick", &json!({"table": true})).unwrap();

    let dir = std::env::temp_dir().join(format!("aad-takeoff-restore-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let project = dir.join("house.osp");
    s.execute("takeoff.record", &json!({"path": project.to_string_lossy(), "annotate": false})).unwrap();

    s.execute("takeoff.clear", &json!({})).unwrap();
    // Drop the project takeoff entry so restore must reload from the project file.
    {
        let d = s.doc_mut().unwrap();
        d.project.remove("takeoff:Bricks");
    }
    let r = s.execute("takeoff.restore", &json!({"path": project.to_string_lossy(), "clear": true})).unwrap();
    assert_eq!(r["source"], "project");
    assert!(r["restored"].as_array().unwrap().iter().any(|n| n.as_str() == Some("Bricks")));
    let listed = s.execute("takeoff.list", &json!({})).unwrap();
    assert!(listed["takeoffs"].as_array().unwrap().iter().any(|n| n.as_str() == Some("Bricks")));
}
