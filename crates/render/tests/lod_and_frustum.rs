//! Far-zoom LOD and model frustum cull cut display-list work when zoomed in.

use aad_doc::{Circle, Common, Drawing, EntityKind, Hatch, HatchLoop, Space};
use aad_geom::{PolyVertex, Vec2, Vec3};
use aad_render::{Options, build, lod_flags_for_view, lod_for_view};

fn hatch_and_circles() -> Drawing {
    let mut d = Drawing::new_metric();
    for i in 0..20 {
        d.add(&Space::Model, Common::default(), EntityKind::Circle(Circle { center: Vec3::new(f64::from(i) * 50.0, 0.0, 0.0), radius: 5.0 }))
            .unwrap();
    }
    let sq = vec![
        PolyVertex::new(Vec2::new(-10.0, -10.0)),
        PolyVertex::new(Vec2::new(10.0, -10.0)),
        PolyVertex::new(Vec2::new(10.0, 10.0)),
        PolyVertex::new(Vec2::new(-10.0, 10.0)),
    ];
    let hatch = Hatch {
        pattern: "ANSI31".into(),
        solid: false,
        loops: vec![HatchLoop { vertices: sq, outer: true }],
        scale: 1.0,
        angle: 0.0,
        associative: false,
        style: 0,
        elevation: 0.0,
        gradient: None,
        origin: Vec2::ZERO,
        background: None,
        pattern_lines: Vec::new(),
    };
    d.add(&Space::Model, Common::default(), EntityKind::Hatch(hatch)).unwrap();
    d
}

#[test]
fn lod_turns_off_text_and_fill_when_zoomed_out() {
    let (text, fill) = lod_for_view(100.0, 1.0); // 100 world units per pixel
    assert!(!text && !fill);
    let (text, fill) = lod_for_view(0.01, 1.0); // zoomed in
    assert!(text && fill);
}

#[test]
fn lod_uses_greek_text_and_hatch_tint_in_the_mid_band() {
    let close = lod_flags_for_view(0.01, 1.0);
    assert!(close.text && close.fill && !close.text_greek && !close.fill_tint);
    let tint = lod_flags_for_view(20.0, 1.0);
    assert!(tint.fill && tint.fill_tint && tint.text && !tint.text_greek);
    let greek = lod_flags_for_view(40.0, 1.0);
    assert!(greek.text && greek.text_greek && greek.fill && greek.fill_tint);
    let fill_off = lod_flags_for_view(60.0, 1.0);
    assert!(fill_off.text && fill_off.text_greek && !fill_off.fill);
    let far = lod_flags_for_view(100.0, 1.0);
    assert!(!far.text && !far.fill && !far.text_greek && !far.fill_tint);
}

#[test]
fn fill_tint_replaces_pattern_lines_with_a_solid() {
    let d = hatch_and_circles();
    let full = build(&d, &Space::Model, &Options { text: true, fill: true, fill_tint: false, view_height: 40.0, tolerance: 0.01, ..Default::default() });
    let tint = build(&d, &Space::Model, &Options { text: true, fill: true, fill_tint: true, view_height: 40.0, tolerance: 0.01, ..Default::default() });
    assert!(tint.prims.len() < full.prims.len(), "tint={} full={}", tint.prims.len(), full.prims.len());
    assert!(tint.prims.iter().any(|p| matches!(p.kind, aad_render::Kind::Tris)), "tint should keep a solid fill");
}

#[test]
fn fill_off_skips_hatch_pattern_lines() {
    let d = hatch_and_circles();
    let with_fill = build(&d, &Space::Model, &Options { text: true, fill: true, view_height: 40.0, tolerance: 0.01, ..Default::default() });
    let no_fill = build(&d, &Space::Model, &Options { text: false, fill: false, view_height: 40.0, tolerance: 0.01, ..Default::default() });
    assert!(no_fill.prims.len() < with_fill.prims.len(), "no_fill={} with_fill={}", no_fill.prims.len(), with_fill.prims.len());
}

#[test]
fn frustum_skips_entities_outside_the_view() {
    let d = hatch_and_circles();
    let full = build(&d, &Space::Model, &Options { view_height: 2000.0, ..Default::default() });
    // Far from the origin cell of the circles (x≈0..950): view_height 30 → cell 1920, centre at 9000.
    let corner = build(&d, &Space::Model, &Options { view_height: 30.0, view_center: Some(Vec2::new(9000.0, 0.0)), ..Default::default() });
    assert!(corner.prims.len() < full.prims.len(), "corner={} full={}", corner.prims.len(), full.prims.len());
}

#[test]
fn frustum_covers_the_origin_cell_so_a_medium_pan_keeps_geometry() {
    let d = hatch_and_circles();
    let h = 20.0;
    // Circles sit near x=0..950. Cell size = 64*h = 1280; origin at 0 for centre near 0.
    let at = build(&d, &Space::Model, &Options { view_height: h, view_center: Some(Vec2::new(100.0, 0.0)), ..Default::default() });
    let panned = build(&d, &Space::Model, &Options { view_height: h, view_center: Some(Vec2::new(500.0, 0.0)), ..Default::default() });
    assert_eq!(at.prims.len(), panned.prims.len(), "pan within the origin cell must keep the same culled set");
    assert!(aad_render::model_pan_cell(Vec2::new(100.0, 0.0), h) == aad_render::model_pan_cell(Vec2::new(500.0, 0.0), h));
}
