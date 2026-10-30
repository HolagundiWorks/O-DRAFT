//! Print what a project file holds. Run: cargo run -p aad-odp --example inspect -- file.osp
fn main() {
    let path = std::env::args().nth(1).expect("usage: inspect <file.osp>");
    let odp = aad_odp::Odp::open(std::path::Path::new(&path)).expect("open");
    let m = odp.meta().expect("meta");
    println!("{} (id {}), revision {}, created {}", m.name, m.project_id, m.revision, m.created);
    for name in odp.drawing_names().expect("drawings") {
        if let Some(d) = odp.drawing(&name).expect("drawing") {
            println!("drawing {name}: {} {} bytes, sha256 {}", d.ext, d.bytes.len(), &d.sha256[..12]);
        }
    }
    for h in odp.takeoff_headers().expect("headers") {
        let rows = odp.takeoff_rows(h.slot).expect("rows");
        println!("take-off slot {}: {} ({} rows)", h.slot, h.state, rows.len());
    }
    println!("project document: {}", odp.project_doc().expect("project").map_or("none".to_string(), |j| format!("{} bytes", j.len())));
    for id in odp.asset_ids().expect("assets") {
        println!("asset {id}");
    }
    for (rev, table, key, op) in odp.changes_since(0).expect("changes") {
        println!("  r{rev}: {table} {key} {op}");
    }
}
