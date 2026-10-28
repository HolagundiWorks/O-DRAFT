//! Phase C: tiled model-space scene — frustum skips far tiles; dirty rebuild; hostile caps.

use std::sync::Arc;

use aad_doc::{Common, Drawing, EntityKind, Line, Space};
use aad_geom::{Vec2, Vec3};
use aad_render::scene::{MAX_SCENE_TILES, build_scene, flatten_scene, rebuild_scene, scene_tile_edge, scene_tiling_viable};
use aad_render::{Options, build};

fn spaced_lines(n: usize, spacing: f64) -> Drawing {
    let mut d = Drawing::new_metric();
    for i in 0..n {
        let x = i as f64 * spacing;
        d.add(&Space::Model, Common::default(), EntityKind::Line(Line { a: Vec3::new(x, 0.0, 0.0), b: Vec3::new(x + 1.0, 0.0, 0.0) })).unwrap();
    }
    d
}

#[test]
fn scene_flatten_matches_build_in_document_order() {
    let d = spaced_lines(10, 400.0);
    let opts = Options { view_height: 80.0, tolerance: 0.01, ..Default::default() };
    assert!(scene_tiling_viable(&d, scene_tile_edge(opts.view_height)));
    let scene = build_scene(&d, &opts);
    assert!(scene.tiles.len() >= 2);
    let flat = flatten_scene(&scene);
    let mono = build(&d, &Space::Model, &opts);
    assert_eq!(flat.prims.len(), mono.prims.len());
    let handles_a: Vec<_> = flat.prims.iter().map(|p| p.handle).collect();
    let handles_b: Vec<_> = mono.prims.iter().map(|p| p.handle).collect();
    assert_eq!(handles_a, handles_b, "flatten keeps document order");
}

#[test]
fn frustum_build_skips_far_tiles_but_keeps_extents() {
    let d = spaced_lines(5, 2500.0);
    let full = Options { view_height: 100.0, tolerance: 0.01, ..Default::default() };
    let zoomed = Options { view_height: 100.0, view_center: Some(Vec2::new(0.5, 0.0)), tolerance: 0.01, ..Default::default() };
    let all = build_scene(&d, &full);
    let near = build_scene(&d, &zoomed);
    assert!(near.assignment.len() < all.assignment.len(), "frustum drops far entities from tessellation");
    assert!(near.extents.width() >= all.extents.width() * 0.9, "extents still span the model");
    let flat = flatten_scene(&near);
    assert!(!flat.prims.is_empty());
    assert!(flat.prims.len() < flatten_scene(&all).prims.len());
}

#[test]
fn dirty_rebuild_retains_clean_tile_arcs() {
    let old = spaced_lines(8, 500.0);
    let opts = Options { view_height: 100.0, tolerance: 0.01, ..Default::default() };
    let scene = build_scene(&old, &opts);
    let before_ptrs: Vec<_> = scene.tiles.values().map(|t| Arc::as_ptr(&t.list) as usize).collect();
    let handles: Vec<_> = old.model.iter().map(|e| e.handle).collect();
    let mut new = old.clone();
    let edit = *handles.last().unwrap();
    new.model.modify(edit, |e| {
        if let EntityKind::Line(l) = &mut e.kind {
            l.b.x += 3.0;
        }
    });
    let next = rebuild_scene(&scene, &old, &new, &opts).expect("scene rebuild");
    assert_eq!(next.tiles_rebuilt, 1);
    let mut shared = 0usize;
    for t in next.tiles.values() {
        if before_ptrs.contains(&(Arc::as_ptr(&t.list) as usize)) {
            shared += 1;
        }
    }
    assert!(shared >= next.tiles.len().saturating_sub(1), "clean tiles keep their Arc");
    assert_eq!(flatten_scene(&next).prims.len(), build(&new, &Space::Model, &opts).prims.len());
}

#[test]
fn hostile_tile_count_is_capped() {
    // Extremely fine edge relative to a huge span → many tiles; viability must refuse.
    let d = spaced_lines(MAX_SCENE_TILES + 10, 1.0);
    let edge = 0.05;
    assert!(!scene_tiling_viable(&d, edge), "above MAX_SCENE_TILES must not claim tiling is viable");
    // build_scene still returns a usable single-tile fallback.
    let opts = Options { view_height: edge / 4.0, tolerance: 0.01, ..Default::default() };
    let scene = build_scene(&d, &opts);
    assert!(!scene.tiles.is_empty());
    assert_eq!(flatten_scene(&scene).prims.len(), build(&d, &Space::Model, &opts).prims.len());
}
