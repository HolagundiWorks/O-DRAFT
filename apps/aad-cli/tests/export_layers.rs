//! `export` as layered PDF/SVG and as PNG at a resolution, through the engine command.

use std::sync::Once;

use aad_engine::Session;
use serde_json::{Value, json};

fn install_io() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        aad_engine::cmd::file::set_io(aad_engine::cmd::file::IoHooks {
            read: |b, name| aad_io::read(b, name).map_err(|e| e.to_string()),
            write: |d, name| aad_io::write(d, name).map_err(|e| e.to_string()),
            plot: Some(|d, space, opts| aad_io::plot(d, space, opts).map_err(|e| e.to_string())),
        });
    });
}

fn bracket() -> Session {
    install_io();
    let mut s = Session::new();
    s.open_drawing(aad_engine::sample::default_sample(), "Bracket", None);
    s
}

/// Export with `params` to a temporary file and return its bytes.
fn export(s: &mut Session, tag: &str, mut params: Value) -> Result<Vec<u8>, String> {
    let path = std::env::temp_dir().join(format!("aad-export-layers-{}-{tag}", std::process::id()));
    params["path"] = json!(path.to_string_lossy());
    s.execute("export", &params).map_err(|e| e.to_string())?;
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&path);
    Ok(bytes)
}

fn count(bytes: &[u8], needle: &str) -> usize {
    String::from_utf8_lossy(bytes).matches(needle).count()
}

#[test]
fn export_pdf_keeps_the_layers_and_png_at_a_resolution_is_the_whole_sheet() {
    let mut s = bracket();
    let layers = s.doc().unwrap().layers.len();
    assert!(layers > 1);
    let pdf = export(&mut s, "a.pdf", json!({"format": "pdf", "compress": false})).unwrap();
    let ocgs = count(&pdf, "/Type /OCG");
    assert!(ocgs > 1 && ocgs <= layers + 1, "{ocgs} layer groups for {layers} layers");
    assert_eq!(count(&pdf, " BDC"), count(&pdf, "EMC"));
    let flat = export(&mut s, "b.pdf", json!({"format": "pdf", "layers": false})).unwrap();
    assert_eq!(count(&flat, "OCProperties"), 0);

    // A resolution asks for the sheet: A3 landscape at 50 dpi.
    let png = export(&mut s, "c.png", json!({"format": "png", "dpi": 50, "paper": "A3", "landscape": true})).unwrap();
    assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
    let (w, h) = (u32::from_be_bytes([png[16], png[17], png[18], png[19]]), u32::from_be_bytes([png[20], png[21], png[22], png[23]]));
    assert_eq!((w, h), (827, 585));
    // Without a layout or resolution, a PNG is still the extents of model space (2400 x 1600).
    let plain = export(&mut s, "d.png", json!({"format": "png"})).unwrap();
    assert_eq!(u32::from_be_bytes([plain[16], plain[17], plain[18], plain[19]]), 2400);
    let svg = String::from_utf8(export(&mut s, "e.svg", json!({"format": "svg", "paper": "A4"})).unwrap()).unwrap();
    assert!(svg.contains("mm\"") && svg.contains("inkscape:groupmode"));
    let model_svg = String::from_utf8(export(&mut s, "f.svg", json!({"format": "svg"})).unwrap()).unwrap();
    assert!(model_svg.contains("inkscape:groupmode"), "the default SVG keeps the layers too");
    assert!(export(&mut s, "g.png", json!({"format": "png", "dpi": 9000})).is_err());
}
