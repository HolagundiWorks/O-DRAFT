//! Layered PDF and SVG export (each drawing layer is a PDF optional-content group or an SVG layer
//! group) and PNG export at a chosen resolution for a layout.

use aad_color::Color;
use aad_doc::{Common, Drawing, EntityKind, Layer, Line, Space};
use aad_geom::Vec3;
use serde_json::json;

fn line(d: &mut Drawing, space: &Space, layer: &str, y: f64) {
    if d.layer(layer).is_none() {
        d.layers.push(Layer::new(layer));
    }
    let k = EntityKind::Line(Line { a: Vec3::new(10.0, y, 0.0), b: Vec3::new(100.0, y, 0.0) });
    d.add(space, Common { layer: layer.into(), color: Color::Index(1), ..Default::default() }, k).unwrap();
}

fn two_layers() -> Drawing {
    let mut d = Drawing::new_metric();
    let space = Space::Paper("Layout1".into());
    line(&mut d, &space, "A-WALL", 10.0);
    line(&mut d, &space, "E-LITE", 20.0);
    line(&mut d, &space, "A-WALL", 30.0);
    d
}

fn pdf_bytes(d: &Drawing, extra: serde_json::Value) -> Vec<u8> {
    let mut opts = json!({"compress": false});
    if let (Some(o), Some(e)) = (opts.as_object_mut(), extra.as_object()) {
        o.extend(e.clone());
    }
    aad_io::plot(d, &Space::Paper("Layout1".into()), &opts).unwrap()
}

#[test]
fn a_pdf_has_one_optional_content_group_per_layer_and_balanced_marked_content() {
    let d = two_layers();
    let bytes = pdf_bytes(&d, json!({}));
    let text = String::from_utf8_lossy(&bytes).into_owned();
    assert!(text.starts_with("%PDF-1.5"), "optional content needs PDF 1.5");
    assert_eq!(text.matches("/Type /OCG").count(), 2, "{text}");
    assert!(text.contains("/Name (A-WALL)") && text.contains("/Name (E-LITE)"));
    assert!(text.contains("/OCProperties"));
    // A-WALL, E-LITE, A-WALL again: three marked-content runs, each closed.
    assert_eq!(text.matches(" BDC").count(), 3);
    assert_eq!(text.matches("EMC").count(), 3);
    // The file still parses and the groups are listed in the catalog.
    let doc = lopdf::Document::load_mem(&bytes).unwrap();
    let catalog = doc.catalog().unwrap();
    let props = catalog.get(b"OCProperties").unwrap().as_dict().unwrap();
    assert_eq!(props.get(b"OCGs").unwrap().as_array().unwrap().len(), 2);
}

#[test]
fn layers_can_be_left_out_and_a_plain_pdf_keeps_version_one_four() {
    let d = two_layers();
    let text = String::from_utf8_lossy(&pdf_bytes(&d, json!({"layers": false}))).into_owned();
    assert!(text.starts_with("%PDF-1.4") && !text.contains("OCProperties") && !text.contains("BDC"));
}

#[test]
fn layer_names_are_escaped_in_both_formats() {
    let mut d = Drawing::new_metric();
    let space = Space::Paper("Layout1".into());
    line(&mut d, &space, "Wall <\"1\"> & (Ä)", 10.0);
    let pdf = pdf_bytes(&d, json!({}));
    assert!(lopdf::Document::load_mem(&pdf).is_ok(), "a name with brackets and non-ASCII still gives a valid file");
    let svg = aad_io::svg::export(&d, &space);
    assert!(svg.contains(r#"inkscape:label="Wall &lt;&quot;1&quot;&gt; &amp; (Ä)""#), "{svg}");
    assert!(!svg.contains("<\"1\">"));
}

#[test]
fn an_svg_groups_each_layer_and_can_stay_flat() {
    let d = two_layers();
    let space = Space::Paper("Layout1".into());
    let svg = aad_io::svg::export(&d, &space);
    assert_eq!(svg.matches(r#"inkscape:groupmode="layer""#).count(), 2, "{svg}");
    assert!(svg.contains(r#"inkscape:label="A-WALL""#) && svg.contains(r#"inkscape:label="E-LITE""#));
    // Both A-WALL lines are in its group: the layer has one group, not one per run.
    assert_eq!(svg.matches("<polyline").count(), 3);
    let flat = aad_io::svg::export_layers(&d, &space, None, false);
    assert!(!flat.contains("<g ") && flat.matches("<polyline").count() == 3);
}

#[test]
fn a_plot_svg_is_the_sheet_in_points_with_the_layers_as_groups() {
    let d = two_layers();
    let bytes = aad_io::plot(&d, &Space::Paper("Layout1".into()), &json!({"format": "svg", "paper": "A4", "landscape": true})).unwrap();
    let svg = String::from_utf8(bytes).unwrap();
    // A4 landscape: 297 x 210 mm = 841.89 x 595.28 pt.
    assert!(svg.contains(r#"viewBox="0 0 841.890 595.276""#) && svg.contains(r#"width="297.000mm""#), "{svg}");
    assert_eq!(svg.matches(r#"inkscape:groupmode="layer""#).count(), 2);
    assert!(svg.contains("clip-path") && svg.ends_with("</g></svg>"));
    let flat = aad_io::plot(&d, &Space::Paper("Layout1".into()), &json!({"format": "svg", "layers": false})).unwrap();
    assert!(!String::from_utf8(flat).unwrap().contains("inkscape:groupmode"));
}

/// The pixel size and the stored resolution of a PNG.
fn png_info(bytes: &[u8]) -> (u32, u32, Option<u32>) {
    assert!(bytes.starts_with(&[0x89, b'P', b'N', b'G']));
    let be = |i: usize| u32::from_be_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
    let mut dpm = None;
    let mut at = 8;
    while at + 8 <= bytes.len() {
        let len = be(at) as usize;
        if &bytes[at + 4..at + 8] == b"pHYs" {
            assert_eq!(be(at + 8), be(at + 12), "square pixels");
            assert_eq!(bytes[at + 16], 1, "metres");
            dpm = Some(be(at + 8));
        }
        at += 12 + len;
    }
    (be(16), be(20), dpm)
}

#[test]
fn a_layout_png_is_the_whole_sheet_at_the_requested_resolution() {
    let d = two_layers();
    let space = Space::Paper("Layout1".into());
    let at = |dpi: f64| {
        let bytes = aad_io::plot(&d, &space, &json!({"format": "png", "dpi": dpi, "paper": "A4", "landscape": true})).unwrap();
        png_info(&bytes)
    };
    // 297 x 210 mm at 100 dpi = 1169 x 827 px; the file says 100 dpi = 3937 px per metre.
    assert_eq!(at(100.0), (1169, 827, Some(3937)));
    // Twice the dpi, twice the pixels.
    assert_eq!(at(200.0), (2339, 1654, Some(7874)));
}

#[test]
fn raster_resolution_and_size_are_bounded() {
    let d = two_layers();
    let space = Space::Paper("Layout1".into());
    for dpi in [0.0, 10.0, -300.0, 5000.0, 1e300] {
        assert!(aad_io::plot(&d, &space, &json!({"format": "png", "dpi": dpi})).is_err(), "{dpi}");
    }
    let err = aad_io::plot(&d, &space, &json!({"format": "png", "dpi": 1200, "paper": "A0"})).unwrap_err().to_string();
    assert!(err.contains("too large") && err.contains("dpi"), "{err}");
    assert!(aad_io::plot(&d, &space, &json!({"format": "png", "dpi": "high"})).is_err(), "text is not a resolution");
    assert!(aad_io::plot(&d, &space, &json!({"format": "tiff"})).is_err());
}

#[test]
fn the_png_draws_the_layers_in_the_printed_colours_on_white() {
    let d = two_layers();
    let bytes = aad_io::plot(&d, &Space::Paper("Layout1".into()), &json!({"format": "png", "dpi": 72, "paper": "A4"})).unwrap();
    let pm = tiny_skia::Pixmap::decode_png(&bytes).unwrap();
    let px = pm.pixels();
    assert!(px.iter().any(|p| p.red() > 200 && p.green() < 60 && p.blue() < 60), "the red lines are drawn");
    assert!(px.iter().filter(|p| p.red() == 255 && p.green() == 255 && p.blue() == 255).count() > px.len() / 2, "the sheet is white");
}
