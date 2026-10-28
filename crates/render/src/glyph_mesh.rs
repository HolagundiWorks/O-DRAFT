//! Phase D: retained triangulated glyph meshes keyed by `(font, glyph_id, band, fill)`.
//!
//! TrueType outlines are flattened per tessellation band in `aad-fonts`. Triangulation
//! (ear-clip / even-odd) is the expensive follow-on; this cache stores **font-unit**
//! triangles so zoom-band rebuilds and repeated glyphs reuse the mesh. Placement and any
//! post [`aad_geom::Mat3`] on [`aad_fonts::Shaped`] transform the cached verts.
//!
//! **Prototype decision (this PR):** ship the CPU triangulated glyph×band cache. A GPU
//! glyph atlas (R8 coverage) and Loop–Blinn filled TTF remain **proposed** — measure a
//! 705k-prim class drawing before picking one.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use aad_fonts::{GlyphMeshKey, GlyphPlace, glyph_font_unit_contours};
use aad_geom::{Mat3, Vec2};

use crate::fill;

/// Cap on retained glyph meshes (hostile / never-crash). Clears when exceeded.
pub const MAX_GLYPH_MESH_ENTRIES: usize = 8_192;

/// Full cache key: font/glyph/band identity plus whether the mesh is a filled triangulation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct GlyphFillKey {
    pub glyph: GlyphMeshKey,
    pub fill: bool,
}

type MeshCache = HashMap<GlyphFillKey, Arc<Vec<Vec2>>>;

fn mesh_cache() -> &'static Mutex<MeshCache> {
    static C: OnceLock<Mutex<MeshCache>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Test / inspect helper: number of retained glyph meshes.
pub fn glyph_mesh_cache_len() -> usize {
    mesh_cache().lock().unwrap_or_else(PoisonError::into_inner).len()
}

/// Drop all retained glyph meshes (tests).
pub fn glyph_mesh_cache_clear() {
    mesh_cache().lock().unwrap_or_else(PoisonError::into_inner).clear();
}

fn font_unit_filled(key: GlyphMeshKey) -> Arc<Vec<Vec2>> {
    let fill_key = GlyphFillKey { glyph: key, fill: true };
    if let Some(v) = mesh_cache().lock().unwrap_or_else(PoisonError::into_inner).get(&fill_key) {
        return Arc::clone(v);
    }
    let loops = glyph_font_unit_contours(key).unwrap_or_default();
    let tris = if loops.is_empty() { Vec::new() } else { fill::triangulate_outline(&loops) };
    let arc = Arc::new(tris);
    let mut cache = mesh_cache().lock().unwrap_or_else(PoisonError::into_inner);
    if cache.len() >= MAX_GLYPH_MESH_ENTRIES {
        cache.clear();
    }
    cache.insert(fill_key, Arc::clone(&arc));
    arc
}

/// Place a font-unit point the same way [`aad_fonts::ttf`] places outline contours.
fn place_pt(p: Vec2, place: GlyphPlace) -> Vec2 {
    let scale = place.height / place.cap;
    let y = p.y * scale;
    Vec2::new(place.x + p.x * scale * place.width_factor + y * place.shear, y)
}

/// Filled triangles for a shaped glyph, preferring the font-unit cache when identity is known.
pub fn filled_tris(key: Option<GlyphMeshKey>, place: Option<GlyphPlace>, post_xf: Mat3, placed_contours: &[Vec<Vec2>]) -> Vec<Vec2> {
    match (key, place) {
        (Some(k), Some(pl)) if pl.is_finite() => {
            let font_tris = font_unit_filled(k);
            if font_tris.is_empty() {
                return fill::triangulate_outline(placed_contours);
            }
            font_tris.iter().map(|p| post_xf.apply(place_pt(*p, pl))).collect()
        }
        _ => fill::triangulate_outline(placed_contours),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aad_fonts::{TextFont, shape_line, with_world_chord_tol};
    use aad_geom::Vec2;

    fn outline_font() -> Option<TextFont> {
        for name in ["DejaVuSans", "LiberationSans-Regular", "Arial", "Segoe UI"] {
            if let Some(face) = aad_fonts::ttf::find(name) {
                return Some(TextFont::Outline(face));
            }
        }
        None
    }

    #[test]
    fn glyph_mesh_cache_reuses_same_key() {
        glyph_mesh_cache_clear();
        let Some(font) = outline_font() else { return };
        let band_tol = 0.01;
        // Same world chord + same height ⇒ same flatten band; different pen x is fine.
        let (a, b) = with_world_chord_tol(band_tol, || {
            let s1 = shape_line(&font, "Hi", 2.5, 1.0, 0.0);
            let s2 = shape_line(&font, "HiHi", 2.5, 1.0, 0.0);
            (s1, s2)
        });
        assert_eq!(a.glyph_keys.len(), a.glyphs.len());
        assert!(!a.glyph_keys.is_empty());
        assert!(a.glyph_keys.iter().all(|k| k.is_some()), "TrueType glyphs carry mesh keys");
        let before = glyph_mesh_cache_len();
        for (i, g) in a.glyphs.iter().enumerate() {
            let _ = filled_tris(a.glyph_keys.get(i).copied().flatten(), a.glyph_places.get(i).copied().flatten(), a.post_xf, g);
        }
        let mid = glyph_mesh_cache_len();
        assert!(mid > before, "first triangulation inserts cache entries");
        for (i, g) in b.glyphs.iter().enumerate() {
            let _ = filled_tris(b.glyph_keys.get(i).copied().flatten(), b.glyph_places.get(i).copied().flatten(), b.post_xf, g);
        }
        assert_eq!(glyph_mesh_cache_len(), mid, "repeated glyphs at the same band reuse mesh entries");
    }

    #[test]
    fn filled_tris_match_direct_triangulation_area() {
        glyph_mesh_cache_clear();
        let Some(font) = outline_font() else { return };
        let sh = with_world_chord_tol(0.02, || shape_line(&font, "O", 3.0, 1.0, 0.0));
        let Some(g) = sh.glyphs.first() else { return };
        assert!(sh.glyph_keys.first().is_some_and(|k| k.is_some()));
        let direct = fill::triangulate_outline(g);
        let cached = filled_tris(sh.glyph_keys.first().copied().flatten(), sh.glyph_places.first().copied().flatten(), sh.post_xf, g);
        let area = |t: &[Vec2]| -> f64 { t.as_chunks::<3>().0.iter().map(|c| (c[1] - c[0]).cross(c[2] - c[0]).abs() * 0.5).sum() };
        let (ad, ac) = (area(&direct), area(&cached));
        assert!(ad > 0.0 && ac > 0.0);
        assert!((ad - ac).abs() <= 1e-4 * ad.max(ac), "cached {ac} vs direct {ad}");
    }

    #[test]
    fn cache_respects_entry_cap() {
        glyph_mesh_cache_clear();
        let mut cache = mesh_cache().lock().unwrap_or_else(PoisonError::into_inner);
        for i in 0..MAX_GLYPH_MESH_ENTRIES {
            let key = GlyphFillKey { glyph: GlyphMeshKey { font_ptr: i, font_len: 1, face: 0, glyph_id: i as u32, band: 0 }, fill: true };
            cache.insert(key, Arc::new(vec![Vec2::ZERO]));
        }
        assert_eq!(cache.len(), MAX_GLYPH_MESH_ENTRIES);
        drop(cache);
        // One more insert via font_unit_filled path clears when at cap.
        let k = GlyphMeshKey { font_ptr: 99_999, font_len: 1, face: 0, glyph_id: 1, band: 0 };
        let _ = font_unit_filled(k);
        assert!(glyph_mesh_cache_len() <= MAX_GLYPH_MESH_ENTRIES);
    }
}
