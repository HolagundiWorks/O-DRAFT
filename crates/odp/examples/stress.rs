//! Write many revisions to a project file, for running beside another program that writes to it (see the .NET test `Stress_writes_when_asked`).
//! Run: cargo run -p aad-odp --example stress -- file.osp 200 [pause-ms]
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let path = std::path::PathBuf::from(&args[0]);
    let count: u32 = args.get(1).and_then(|n| n.parse().ok()).unwrap_or(100);
    let pause: u64 = args.get(2).and_then(|n| n.parse().ok()).unwrap_or(0);
    let mut odp = if path.exists() { aad_odp::Odp::open(&path) } else { aad_odp::Odp::create(&path, "Stress") }.expect("open");
    for n in 0..count {
        let mut e = odp.edit().expect("edit");
        e.put_takeoff_row(&aad_odp::TakeoffRow { slot: 1, kind: 3, n, json: serde_json::json!({ "from": "rust", "n": n }) }).expect("put");
        e.commit().expect("commit");
        if pause > 0 {
            std::thread::sleep(std::time::Duration::from_millis(pause));
        }
    }
    println!("rust wrote {count} revisions; file is at revision {}", odp.revision().expect("revision"));
}
