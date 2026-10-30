//! Make a project file from a drawing.
//! Run: cargo run -p aad-odp --example pack -- <out.osp> <name> [--drawing file.dwg]
use std::path::{Path, PathBuf};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: pack <out.osp> <name> [--drawing file.dwg]");
        std::process::exit(2);
    }
    let out = PathBuf::from(&args[0]);
    let mut odp = aad_odp::Odp::create(&out, &args[1]).expect("create");
    let mut i = 2;
    while i + 1 < args.len() {
        let path = Path::new(&args[i + 1]);
        match args[i].as_str() {
            "--drawing" => {
                let bytes = std::fs::read(path).expect("read drawing");
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("drawing.dwg").to_string();
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("dwg").to_string();
                let mut e = odp.edit().expect("edit");
                e.put_drawing(&name, &ext, &bytes).expect("put drawing");
                println!("drawing {name}: {} bytes, revision {}", bytes.len(), e.commit().expect("commit"));
            }
            other => eprintln!("unknown option {other}"),
        }
        i += 2;
    }
    println!("{}: revision {}", out.display(), odp.revision().expect("revision"));
}
