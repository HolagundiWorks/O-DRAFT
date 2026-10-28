//! Tiled model-space display-list scene (Phase C).
//!
//! Entities are assigned to a world grid by bounds centroid. Each tile holds an [`Arc`] of its
//! [`DisplayList`] fragment so rebuilds can remesh / retessellate only dirty tiles. Flattening
//! restores document draw order. Rays/xlines without a finite centroid land in [`GLOBAL_TILE`].
//!
//! Layout viewport model tessellation uses the same process-wide LRU ([`crate::vp_model_get`]) —
//! no longer `thread_local` — so worker threads can share it.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

use aad_doc::{Drawing, Entity, EntityKind, Handle, Space};
use aad_geom::{Bounds2, Mat3, Vec2};
use rayon::prelude::*;

use crate::mesh::{MAX_MESH_TILES, MIN_MESH_TILES, TileId, mesh_tile_edge, tile_id_at};
use crate::{Builder, DisplayList, MAX_BLOCK_EXPANSION, Options, PARALLEL_CHUNK, append, entity, model_cull_window, top_ctx, with_point_view};

/// Sentinel tile for infinites / entities without a finite centroid.
pub const GLOBAL_TILE: TileId = TileId { x: i32::MIN, y: i32::MIN };

/// Cap on scene tiles (hostile extents); above this, callers should use monolithic [`crate::build`].
pub const MAX_SCENE_TILES: usize = MAX_MESH_TILES;

/// Prefer tiling when at least this many non-empty tiles would be produced.
pub const MIN_SCENE_TILES: usize = MIN_MESH_TILES;

/// One retained tile: Arc so clean tiles survive a dirty rebuild without cloning prims.
#[derive(Clone, Debug)]
pub struct SceneTile {
    pub id: TileId,
    pub list: Arc<DisplayList>,
}

/// Model-space scene: per-tile display lists plus a document-order assignment for flattening.
#[derive(Clone, Debug, Default)]
pub struct Scene {
    pub edge: f64,
    pub tiles: BTreeMap<TileId, SceneTile>,
    /// Handles in model iteration order with the tile they were built into.
    pub assignment: Vec<(Handle, TileId)>,
    /// Cheap AABB of every model entity (including those skipped by the frustum).
    pub extents: Bounds2,
    pub expanded: usize,
    /// Tiles tessellated on the last [`build_scene`] / [`rebuild_scene`] (tests / inspect).
    pub tiles_rebuilt: usize,
}

/// World-space tile edge for the scene grid (same policy as mesh tiles).
pub fn scene_tile_edge(view_height: f64) -> f64 {
    mesh_tile_edge(view_height)
}

/// Whether model-space scene tiling is worthwhile for this drawing and edge.
pub fn scene_tiling_viable(d: &Drawing, edge: f64) -> bool {
    if !(edge.is_finite() && edge > 0.0) || d.model.is_empty() {
        return false;
    }
    let mut seen: HashSet<TileId> = HashSet::new();
    for e in d.model.iter() {
        let id = entity_tile(d, e, edge).unwrap_or(GLOBAL_TILE);
        seen.insert(id);
        if seen.len() > MAX_SCENE_TILES {
            return false;
        }
    }
    (MIN_SCENE_TILES..=MAX_SCENE_TILES).contains(&seen.len())
}

fn entity_tile(d: &Drawing, e: &Entity, edge: f64) -> Option<TileId> {
    if matches!(e.kind, EntityKind::Ray(_) | EntityKind::XLine(_)) {
        return Some(GLOBAL_TILE);
    }
    let b = aad_doc::entity_bounds(d, e, 0);
    if b.is_empty() {
        return None;
    }
    tile_id_at(b.center(), edge)
}

fn tile_intersects_cull(id: TileId, edge: f64, cull: &Bounds2) -> bool {
    if id == GLOBAL_TILE {
        return true;
    }
    if !(edge.is_finite() && edge > 0.0) || cull.is_empty() {
        return true;
    }
    let min = Vec2::new(f64::from(id.x) * edge, f64::from(id.y) * edge);
    let max = min + Vec2::new(edge, edge);
    Bounds2::new(min, max).intersects(cull)
}

fn build_entities_list(d: &Drawing, ents: &[&Entity], opts: &Options, plotting: bool, budget: &AtomicUsize) -> DisplayList {
    aad_fonts::with_world_chord_tol(opts.tolerance, || {
        let mut b = Builder { list: DisplayList::default(), opts, plotting, styles: None, named: None, budget: Some(budget) };
        for e in ents {
            entity(&mut b, &top_ctx(d, Mat3::IDENTITY, e.handle, &[], &[]), e);
        }
        b.list
    })
}

/// Partition model entities into tiles (document order preserved within each tile).
fn partition_model<'a>(d: &'a Drawing, edge: f64, cull: Option<&Bounds2>) -> (BTreeMap<TileId, Vec<&'a Entity>>, Vec<(Handle, TileId)>, Bounds2) {
    let mut map: BTreeMap<TileId, Vec<&Entity>> = BTreeMap::new();
    let mut assignment = Vec::new();
    let mut extents = Bounds2::default();
    for e in d.model.iter() {
        let eb = aad_doc::entity_bounds(d, e, 0);
        if !eb.is_empty() {
            extents = extents.union(&eb);
        }
        let id = entity_tile(d, e, edge).unwrap_or(GLOBAL_TILE);
        if let Some(win) = cull {
            if id != GLOBAL_TILE && !eb.is_empty() && !eb.intersects(win) {
                continue;
            }
            if id != GLOBAL_TILE && eb.is_empty() && !tile_intersects_cull(id, edge, win) {
                continue;
            }
        }
        assignment.push((e.handle, id));
        map.entry(id).or_default().push(e);
    }
    (map, assignment, extents)
}

fn tessellate_tiles(
    d: &Drawing,
    map: &BTreeMap<TileId, Vec<&Entity>>,
    opts: &Options,
    plotting: bool,
) -> (BTreeMap<TileId, SceneTile>, usize, usize) {
    let budget = AtomicUsize::new(0);
    let keys: Vec<TileId> = map.keys().copied().collect();
    let built: Vec<(TileId, DisplayList)> = if keys.len() >= 2 && map.values().map(Vec::len).sum::<usize>() >= 256 {
        keys.par_iter()
            .filter_map(|id| {
                let ents = map.get(id)?;
                if ents.is_empty() {
                    return None;
                }
                // Chunk large tiles the same way as the monolithic parallel path.
                if ents.len() >= PARALLEL_CHUNK * 2 {
                    let mut acc = DisplayList::default();
                    for chunk in ents.chunks(PARALLEL_CHUNK) {
                        append(&mut acc, build_entities_list(d, chunk, opts, plotting, &budget));
                    }
                    Some((*id, acc))
                } else {
                    Some((*id, build_entities_list(d, ents, opts, plotting, &budget)))
                }
            })
            .collect()
    } else {
        keys.iter()
            .filter_map(|id| {
                let ents = map.get(id)?;
                if ents.is_empty() {
                    return None;
                }
                Some((*id, build_entities_list(d, ents, opts, plotting, &budget)))
            })
            .collect()
    };
    let rebuilt = built.len();
    let mut tiles = BTreeMap::new();
    let mut expanded = 0usize;
    for (id, list) in built {
        expanded = expanded.saturating_add(list.expanded);
        tiles.insert(id, SceneTile { id, list: Arc::new(list) });
    }
    (tiles, expanded, rebuilt)
}

/// Build a model-space scene. Paper space and plotting should keep using [`crate::build`] /
/// [`crate::build_plot`]. When `opts.view_center` is set, only tiles intersecting the frustum
/// (plus global infinites) are tessellated; [`Scene::extents`] still covers the whole model.
pub fn build_scene(d: &Drawing, opts: &Options) -> Scene {
    let opts = with_point_view(d, &Space::Model, opts);
    let edge = scene_tile_edge(opts.view_height);
    if !scene_tiling_viable(d, edge) {
        // Single-tile fallback: one GLOBAL tile with a full model build so callers always get a Scene.
        let list = crate::build(d, &Space::Model, &opts);
        let mut tiles = BTreeMap::new();
        let assignment: Vec<(Handle, TileId)> = d.model.iter().map(|e| (e.handle, GLOBAL_TILE)).collect();
        let extents = list.bounds;
        let expanded = list.expanded;
        tiles.insert(GLOBAL_TILE, SceneTile { id: GLOBAL_TILE, list: Arc::new(list) });
        return Scene { edge, tiles, assignment, extents, expanded, tiles_rebuilt: 1 };
    }
    let cull = model_cull_window(&opts);
    let (map, assignment, extents) = partition_model(d, edge, cull.as_ref());
    let (tiles, expanded, rebuilt) = tessellate_tiles(d, &map, &opts, false);
    Scene { edge, tiles, assignment, extents, expanded, tiles_rebuilt: rebuilt }
}

/// Flatten a scene into a single display list in document order (handle assignment order).
pub fn flatten_scene(scene: &Scene) -> DisplayList {
    // Index each tile's prims by handle (contiguous runs).
    let mut runs: HashMap<Handle, (TileId, usize, usize)> = HashMap::new();
    for (id, tile) in &scene.tiles {
        let list = tile.list.as_ref();
        let mut i = 0usize;
        while i < list.prims.len() {
            let h = list.prims[i].handle;
            let mut j = i + 1;
            while list.prims.get(j).is_some_and(|p| p.handle == h) {
                j += 1;
            }
            runs.insert(h, (*id, i, j));
            i = j;
        }
    }
    let mut out = DisplayList { expanded: scene.expanded, ..DisplayList::default() };
    out.bounds = scene.extents;
    for (handle, tile_id) in &scene.assignment {
        let Some((id, a, b)) = runs.get(handle).copied() else { continue };
        if id != *tile_id {
            continue;
        }
        let Some(tile) = scene.tiles.get(tile_id) else { continue };
        let list = tile.list.as_ref();
        let Some(prims) = list.prims.get(a..b) else { continue };
        let is_tris = |p: &crate::DPrim| matches!(p.kind, crate::Kind::Tris | crate::Kind::Mask);
        let span = |tris: bool| -> Option<(u32, u32)> {
            let mut it = prims.iter().filter(|p| is_tris(p) == tris);
            let first = it.next()?.start;
            let end = prims.iter().filter(|p| is_tris(p) == tris).map(|p| p.start + p.len).max()?;
            Some((first, end))
        };
        let (vo, to) = (out.verts.len() as i64, out.tris.len() as i64);
        let vspan = span(false);
        let tspan = span(true);
        if let Some((s, e)) = vspan
            && let Some(slice) = list.verts.get(s as usize..e as usize)
        {
            out.verts.extend_from_slice(slice);
        }
        if let Some((s, e)) = tspan
            && let Some(slice) = list.tris.get(s as usize..e as usize)
        {
            out.tris.extend_from_slice(slice);
        }
        for p in prims {
            let mut p = *p;
            let (base, off) = if is_tris(&p) { (tspan.map(|s| s.0).unwrap_or(0), to) } else { (vspan.map(|s| s.0).unwrap_or(0), vo) };
            if let Ok(start) = u32::try_from(i64::from(p.start) - i64::from(base) + off) {
                p.start = start;
                out.prims.push(p);
            }
        }
    }
    // Prefer tight bounds from tessellated prims when we have them; keep extents as a floor.
    if !out.prims.is_empty() {
        let mut b = Bounds2::default();
        for p in &out.prims {
            for q in out.points(p) {
                b.add(*q);
            }
        }
        if !b.is_empty() {
            out.bounds = b.union(&scene.extents);
        }
    }
    out
}

/// Update `old` for `new_doc`: unchanged tiles keep their `Arc` lists; only dirty tiles retessellate.
/// `None` when the environment changed or too many tiles are dirty (caller should [`build_scene`]).
pub fn rebuild_scene(old: &Scene, old_doc: &Drawing, new_doc: &Drawing, opts: &Options) -> Option<Scene> {
    if old.expanded >= MAX_BLOCK_EXPANSION / 2 || !crate::same_environment(old_doc, new_doc) {
        return None;
    }
    if new_doc.header.f64("PDSIZE", 0.0) <= 0.0 && crate::point::sized(new_doc.header.i64("PDMODE", 0)) {
        return None;
    }
    let opts = with_point_view(new_doc, &Space::Model, opts);
    let edge = if old.edge.is_finite() && old.edge > 0.0 { old.edge } else { scene_tile_edge(opts.view_height) };
    if !scene_tiling_viable(new_doc, edge) {
        return None;
    }
    let cull = model_cull_window(&opts);
    let (new_map, assignment, extents) = partition_model(new_doc, edge, cull.as_ref());
    if new_map.len() > MAX_SCENE_TILES {
        return None;
    }

    let mut old_handle_tile: HashMap<Handle, TileId> = HashMap::new();
    for (h, id) in &old.assignment {
        old_handle_tile.insert(*h, *id);
    }

    let mut dirty: HashSet<TileId> = HashSet::new();
    // Removed or moved entities dirty their previous tile.
    for (h, id) in &old.assignment {
        let still = new_doc.model.get(*h);
        match still {
            None => {
                dirty.insert(*id);
            }
            Some(e) => {
                let same = old_doc.model.get(*h).is_some_and(|o| Arc::ptr_eq(o, e));
                let new_id = entity_tile(new_doc, e, edge).unwrap_or(GLOBAL_TILE);
                if !same || new_id != *id {
                    dirty.insert(*id);
                    dirty.insert(new_id);
                }
            }
        }
    }
    // New entities dirty their tile.
    for e in new_doc.model.iter() {
        if old_handle_tile.contains_key(&e.handle) {
            continue;
        }
        dirty.insert(entity_tile(new_doc, e, edge).unwrap_or(GLOBAL_TILE));
    }
    // Tiles that disappeared from the partition are dropped (not dirty rebuild).
    for id in old.tiles.keys() {
        if !new_map.contains_key(id) {
            dirty.insert(*id);
        }
    }

    let fresh_entities: usize = dirty.iter().filter_map(|id| new_map.get(id).map(Vec::len)).sum();
    if fresh_entities > new_doc.model.len() / 4 + 64 {
        return None;
    }

    let budget = AtomicUsize::new(old.expanded.min(MAX_BLOCK_EXPANSION / 2));
    let dirty_ids: Vec<TileId> = dirty.into_iter().filter(|id| new_map.contains_key(id)).collect();
    let rebuilt_parts: Vec<(TileId, DisplayList)> = dirty_ids
        .par_iter()
        .filter_map(|id| {
            let ents = new_map.get(id)?;
            Some((*id, build_entities_list(new_doc, ents, &opts, false, &budget)))
        })
        .collect();

    let mut tiles = BTreeMap::new();
    for (id, ents) in &new_map {
        if ents.is_empty() {
            continue;
        }
        if let Some((_, list)) = rebuilt_parts.iter().find(|(i, _)| i == id) {
            tiles.insert(*id, SceneTile { id: *id, list: Arc::new(list.clone()) });
        } else if let Some(prev) = old.tiles.get(id) {
            tiles.insert(*id, prev.clone());
        } else {
            // Should not happen: new tile must be dirty. Build serially as a fallback.
            let list = build_entities_list(new_doc, ents, &opts, false, &budget);
            tiles.insert(*id, SceneTile { id: *id, list: Arc::new(list) });
        }
    }
    let expanded = tiles.values().fold(0usize, |a, t| a.saturating_add(t.list.expanded));
    Some(Scene { edge, tiles, assignment, extents, expanded, tiles_rebuilt: rebuilt_parts.len() })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use aad_doc::{Common, EntityKind, Line};
    use aad_geom::Vec3;

    fn line_drawing(n: usize, spacing: f64) -> Drawing {
        let mut d = Drawing::new_metric();
        for i in 0..n {
            let x = i as f64 * spacing;
            d.add(&Space::Model, Common::default(), EntityKind::Line(Line { a: Vec3::new(x, 0.0, 0.0), b: Vec3::new(x + 1.0, 0.0, 0.0) })).unwrap();
        }
        d
    }

    #[test]
    fn flatten_matches_monolithic_build_without_frustum() {
        let d = line_drawing(8, 500.0);
        let opts = Options { view_height: 100.0, tolerance: 0.01, ..Default::default() };
        let scene = build_scene(&d, &opts);
        assert!(scene.tiles.len() >= 2, "spaced lines must tile");
        let flat = flatten_scene(&scene);
        let mono = crate::build(&d, &Space::Model, &opts);
        assert_eq!(flat.prims.len(), mono.prims.len());
        for (a, b) in flat.prims.iter().zip(&mono.prims) {
            assert_eq!(a.handle, b.handle);
            assert_eq!(a.kind, b.kind);
        }
    }

    #[test]
    fn frustum_skips_far_tiles() {
        let d = line_drawing(4, 2000.0);
        let full = Options { view_height: 100.0, tolerance: 0.01, ..Default::default() };
        let zoomed = Options { view_height: 100.0, view_center: Some(Vec2::new(0.5, 0.0)), tolerance: 0.01, ..Default::default() };
        let all = build_scene(&d, &full);
        let near = build_scene(&d, &zoomed);
        assert!(near.tiles.len() < all.tiles.len() || near.assignment.len() < all.assignment.len(), "frustum must drop far tiles");
        assert!(!near.extents.is_empty(), "extents still cover the model");
        // Extents should still span the far geometry.
        assert!(near.extents.width() > 1000.0, "extents width {}", near.extents.width());
    }

    #[test]
    fn rebuild_only_retessellates_dirty_tiles() {
        let old = line_drawing(6, 500.0);
        let opts = Options { view_height: 100.0, tolerance: 0.01, ..Default::default() };
        let scene = build_scene(&old, &opts);
        assert!(scene.tiles.len() >= 2);
        let total = scene.tiles.len();
        let cold = scene.tiles_rebuilt;
        assert_eq!(cold, total);

        let handles: Vec<_> = old.model.iter().map(|e| e.handle).collect();
        let mut new = old.clone();
        let edit = handles[handles.len() - 1];
        new.model.modify(edit, |e| {
            if let EntityKind::Line(l) = &mut e.kind {
                l.b.x += 5.0;
            }
        });
        let next = rebuild_scene(&scene, &old, &new, &opts).expect("incremental");
        assert_eq!(next.tiles_rebuilt, 1, "only the dirty tile rebuilds, got {}", next.tiles_rebuilt);
        assert_eq!(next.tiles.len(), scene.tiles.len());
        let flat = flatten_scene(&next);
        let mono = crate::build(&new, &Space::Model, &opts);
        assert_eq!(flat.prims.len(), mono.prims.len());
    }
}
