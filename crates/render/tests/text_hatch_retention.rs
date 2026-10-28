//! Phase D: glyph×band triangulated mesh cache and hatch pattern expansion cache.

use std::sync::Arc;

use aad_doc::{Common, Drawing, EntityKind, Hatch, HatchLoop, Line, Space, Text};
use aad_geom::{PolyVertex, Vec2, Vec3};
use aad_render::scene::{build_scene, rebuild_scene, scene_tile_edge, scene_tiling_viable};
use aad_render::{Options, build, glyph_mesh_cache_clear, glyph_mesh_cache_len, hatch_pattern_cache_clear, hatch_pattern_cache_len};

fn ansi31_square() -> Hatch {
    let v = [(0.0, 0.0), (40.0, 0.0), (40.0, 30.0), (0.0, 30.0)].into_iter().map(|(x, y)| PolyVertex::new(Vec2::new(x, y))).collect();
    Hatch {
        pattern: "ANSI31".into(),
        solid: false,
        loops: vec![HatchLoop { vertices: v, outer: true }],
        scale: 1.0,
        angle: 0.0,
        associative: false,
        style: 0,
        elevation: 0.0,
        gradient: None,
        origin: Vec2::ZERO,
        background: None,
        pattern_lines: Vec::new(),
    }
}

#[test]
fn hatch_pattern_cache_reuses_same_boundary_pattern_scale_band() {
    hatch_pattern_cache_clear();
    let mut d = Drawing::new_metric();
    d.add(&Space::Model, Common::default(), EntityKind::Hatch(ansi31_square())).unwrap();
    let opts = Options { view_height: 100.0, tolerance: 0.05, fill: true, ..Default::default() };
    let before = hatch_pattern_cache_len();
    let list = build(&d, &Space::Model, &opts);
    assert!(list.prims.iter().any(|p| matches!(p.kind, aad_render::Kind::Polyline)), "ANSI31 emits pattern lines");
    let mid = hatch_pattern_cache_len();
    assert!(mid > before, "first hatch build inserts a pattern cache entry");
    let _ = build(&d, &Space::Model, &opts);
    assert_eq!(hatch_pattern_cache_len(), mid, "identical rebuild reuses the hatch pattern entry");
}

#[test]
fn glyph_mesh_survives_scene_dirty_tile_rebuild() {
    glyph_mesh_cache_clear();
    // Exercise the cache via shape_line (DRAWING_FONT may be missing on Linux CI images).
    let Some(face) = ["DejaVuSans", "LiberationSans-Regular", "Arial", "Segoe UI"].into_iter().find_map(aad_fonts::ttf::find) else {
        return;
    };
    let font = aad_fonts::TextFont::Outline(face);
    let shaped = aad_fonts::with_world_chord_tol(0.02, || aad_fonts::shape_line(&font, "Aa", 2.5, 1.0, 0.0));
    assert!(!shaped.glyphs.is_empty());
    assert!(shaped.glyph_keys.iter().all(|k| k.is_some()));
    for (i, g) in shaped.glyphs.iter().enumerate() {
        let _ =
            aad_render::filled_tris(shaped.glyph_keys.get(i).copied().flatten(), shaped.glyph_places.get(i).copied().flatten(), shaped.post_xf, g);
    }
    let after_shape = glyph_mesh_cache_len();
    assert!(after_shape > 0, "filled TrueType glyphs retain triangulated meshes");

    // Phase C: dirty tile rebuild must not flush the process-wide glyph mesh cache.
    let mut d = Drawing::new_metric();
    for i in 0..6 {
        let x = i as f64 * 500.0;
        d.add(&Space::Model, Common::default(), EntityKind::Line(Line { a: Vec3::new(x, -10.0, 0.0), b: Vec3::new(x + 20.0, -10.0, 0.0) })).unwrap();
        // Keep a Text entity so scene content is mixed; fill may be stroke if DRAWING_FONT missing.
        d.add(
            &Space::Model,
            Common::default(),
            EntityKind::Text(Text {
                insert: Vec3::new(x, 0.0, 0.0),
                align_pt: None,
                height: 2.5,
                value: "Aa".into(),
                rotation: 0.0,
                width_factor: 1.0,
                oblique: 0.0,
                style: "Standard".into(),
                halign: Default::default(),
                valign: Default::default(),
            }),
        )
        .unwrap();
    }
    let opts = Options { view_height: 80.0, tolerance: 0.02, text: true, fill: true, ..Default::default() };
    let edge = scene_tile_edge(opts.view_height);
    if !scene_tiling_viable(&d, edge) {
        assert_eq!(glyph_mesh_cache_len(), after_shape);
        return;
    }
    let scene = build_scene(&d, &opts);
    let before_ptrs: Vec<_> = scene.tiles.values().map(|t| Arc::as_ptr(&t.list) as usize).collect();
    let handles: Vec<_> = d.model.iter().filter(|e| matches!(e.kind, EntityKind::Line(_))).map(|e| e.handle).collect();
    let edit = *handles.last().unwrap();
    let mut new = d.clone();
    new.model.modify(edit, |e| {
        if let EntityKind::Line(l) = &mut e.kind {
            l.b.x += 5.0;
        }
    });
    let next = rebuild_scene(&scene, &d, &new, &opts).expect("dirty rebuild");
    assert!(next.tiles_rebuilt >= 1);
    let shared = next.tiles.values().filter(|t| before_ptrs.contains(&(Arc::as_ptr(&t.list) as usize))).count();
    assert!(shared >= 1, "clean tiles retain Arc lists");
    // Dirty rebuild may insert more glyph meshes for Text entities; it must not flush prior entries.
    let after_rebuild = glyph_mesh_cache_len();
    assert!(after_rebuild >= after_shape, "tile rebuild must not flush glyph×band meshes (had {after_shape}, now {after_rebuild})");
}
