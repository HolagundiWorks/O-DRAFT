//! Several programs may have one project file open and write to it. Each handle here is its own SQLite connection, as two programs would have; the
//! writers take turns (a second waits for the first), so every commit becomes exactly one revision and nothing is lost or interleaved.

use aad_odp::{Odp, TakeoffRow};

#[test]
fn four_writers_and_a_reader_on_one_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("busy.osp");
    drop(Odp::create(&path, "Busy").unwrap());

    const WRITERS: u32 = 4;
    const EACH: u32 = 40;
    let mut threads = Vec::new();
    for w in 0..WRITERS {
        let p = path.clone();
        threads.push(std::thread::spawn(move || {
            let mut odp = Odp::open(&p).unwrap();
            for n in 0..EACH {
                let mut e = odp.edit().unwrap();
                e.put_takeoff_row(&TakeoffRow { slot: w + 1, kind: 3, n, json: serde_json::json!({ "writer": w, "n": n }) }).unwrap();
                e.commit().unwrap();
            }
        }));
    }
    // a reader polling the revision while they write never sees it go backwards
    let reader_path = path.clone();
    let reader = std::thread::spawn(move || {
        let odp = Odp::open(&reader_path).unwrap();
        let mut last = 0;
        for _ in 0..200 {
            let now = odp.revision().unwrap();
            assert!(now >= last, "the revision went backwards: {last} then {now}");
            last = now;
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    });
    for t in threads {
        t.join().unwrap();
    }
    reader.join().unwrap();

    let odp = Odp::open(&path).unwrap();
    assert_eq!(odp.revision().unwrap(), i64::from(WRITERS * EACH), "one revision per commit");
    for w in 0..WRITERS {
        assert_eq!(odp.takeoff_rows(w + 1).unwrap().len(), EACH as usize, "writer {w} lost no row");
    }
    let log = odp.changes_since(0).unwrap();
    assert_eq!(log.len(), (WRITERS * EACH) as usize);
    let revisions: Vec<i64> = log.iter().map(|c| c.0).collect();
    assert_eq!(revisions, (1..=i64::from(WRITERS * EACH)).collect::<Vec<_>>(), "the log has every revision once, in order");
}
