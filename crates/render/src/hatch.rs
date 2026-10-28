//! Hatch drawing: island detection picks the boundary loops that take part, then a solid,
//! gradient or background fill (even-odd triangles) and the pattern lines (each pattern line
//! family swept across the boundary, even-odd clipped, then dashed).
//!
//! Phase D: pattern expansion is retained per `(boundary hash, pattern, scale, band)` so tile /
//! zoom-band rebuilds reuse the swept dashes when the hatch content is unchanged.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use super::{Builder, Ctx, Ink, fill, ink};
use aad_color::{Color, Rgb};
use aad_doc::library::pattern;
use aad_doc::{Entity, Gradient, Hatch};
use aad_geom::{Bounds2, Polyline, Vec2, point_in_polygon};

/// Tessellation band for hatch pattern cache: inverse of [`crate::tessellation_tolerance`].
fn hatch_band(tol: f64) -> i32 {
    if !(tol.is_finite() && tol > 0.0) {
        return i32::MIN;
    }
    (tol / 0.2).max(1e-300).log2().floor() as i32
}

/// Cap on retained hatch pattern expansions (hostile / never-crash). Clears when exceeded.
pub const MAX_HATCH_PATTERN_ENTRIES: usize = 2_048;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct HatchPatternKey {
    boundary: u64,
    pattern: String,
    /// Hash of embedded `pattern_lines` (unknown names); 0 when empty / library-only.
    pattern_lines: u64,
    scale_bits: u64,
    angle_bits: u64,
    origin_x_bits: u64,
    origin_y_bits: u64,
    band: i32,
}

type HatchCache = HashMap<HatchPatternKey, Arc<Vec<Vec<Vec2>>>>;

fn hatch_cache() -> &'static Mutex<HatchCache> {
    static C: OnceLock<Mutex<HatchCache>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Test / inspect helper: retained hatch pattern expansions.
pub fn hatch_pattern_cache_len() -> usize {
    hatch_cache().lock().unwrap_or_else(PoisonError::into_inner).len()
}

/// Drop all retained hatch pattern expansions (tests).
pub fn hatch_pattern_cache_clear() {
    hatch_cache().lock().unwrap_or_else(PoisonError::into_inner).clear();
}

fn f64_key(v: f64) -> u64 {
    if v.is_finite() { v.to_bits() } else { 0 }
}

/// Stable hash of tessellated boundary loops (Phase D hatch key component).
fn boundary_hash(loops: &[Vec<Vec2>]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    loops.len().hash(&mut h);
    for l in loops {
        l.len().hash(&mut h);
        for p in l {
            f64_key(p.x).hash(&mut h);
            f64_key(p.y).hash(&mut h);
        }
    }
    h.finish()
}

fn pattern_lines_hash(h: &Hatch) -> u64 {
    if h.pattern_lines.is_empty() {
        return 0;
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    h.pattern_lines.len().hash(&mut hasher);
    for fam in h.pattern_lines.iter().take(MAX_PATTERN_LINES) {
        f64_key(fam.angle).hash(&mut hasher);
        f64_key(fam.origin.0).hash(&mut hasher);
        f64_key(fam.origin.1).hash(&mut hasher);
        f64_key(fam.delta.0).hash(&mut hasher);
        f64_key(fam.delta.1).hash(&mut hasher);
        fam.dashes.len().hash(&mut hasher);
        for d in &fam.dashes {
            f64_key(*d).hash(&mut hasher);
        }
    }
    hasher.finish()
}

fn pattern_key(h: &Hatch, loops: &[Vec<Vec2>], band: i32) -> HatchPatternKey {
    let scale = if h.scale.is_finite() && h.scale > 1e-9 { h.scale } else { 1.0 };
    HatchPatternKey {
        boundary: boundary_hash(loops),
        pattern: h.pattern.to_ascii_uppercase(),
        pattern_lines: pattern_lines_hash(h),
        scale_bits: f64_key(scale),
        angle_bits: f64_key(h.angle),
        origin_x_bits: f64_key(h.origin.x),
        origin_y_bits: f64_key(h.origin.y),
        band,
    }
}

fn pattern_lines_cached(h: &Hatch, loops: &[Vec<Vec2>], band: i32) -> Arc<Vec<Vec<Vec2>>> {
    let key = pattern_key(h, loops, band);
    if let Some(v) = hatch_cache().lock().unwrap_or_else(PoisonError::into_inner).get(&key) {
        return Arc::clone(v);
    }
    let segs = pattern_lines(h, loops);
    let arc = Arc::new(segs);
    let mut cache = hatch_cache().lock().unwrap_or_else(PoisonError::into_inner);
    if cache.len() >= MAX_HATCH_PATTERN_ENTRIES {
        cache.clear();
    }
    cache.insert(key, Arc::clone(&arc));
    arc
}

/// Draw a hatch entity: `rgb` is its resolved colour, `tol` the chord tolerance.
pub(super) fn draw(b: &mut Builder, ctx: &Ctx, e: &Entity, h: &Hatch, rgb: Ink, lw: f32, tol: f64) {
    let loops = fill_loops(h, tol);
    // Gradient and background colours given ByLayer follow the hatch's own layer.
    let layer = if e.common.layer == "0" { ctx.block_layer.as_deref().unwrap_or("0") } else { e.common.layer.as_str() };
    let layer_color = ctx.layer_color(layer).unwrap_or(Color::Index(7));
    if h.solid || h.pattern.eq_ignore_ascii_case("SOLID") || h.gradient.is_some() {
        if b.opts.fill {
            let tris = fill::triangulate_evenodd(&loops);
            match &h.gradient {
                Some(g) => gradient(b, ctx, g, layer_color, &tris),
                None => b.tris(ctx, rgb, &tris),
            }
        }
        return;
    }
    if let Some(bg) = h.background
        && b.opts.fill
    {
        b.tris(ctx, ink(bg, layer_color, ctx.block_color), &fill::triangulate_evenodd(&loops));
    }
    // Pattern lines are the expensive part of a hatch; `fill: false` (far-zoom LOD) skips them.
    if !b.opts.fill {
        return;
    }
    // Far zoom: a light solid tint avoids moiré from sub-pixel pattern lines.
    if b.opts.fill_tint {
        let tint = Ink {
            rgb: Rgb(
                ((u16::from(rgb.rgb.0) * 2 + 255) / 3) as u8,
                ((u16::from(rgb.rgb.1) * 2 + 255) / 3) as u8,
                ((u16::from(rgb.rgb.2) * 2 + 255) / 3) as u8,
            ),
            aci7: rgb.aci7,
            aci: rgb.aci,
        };
        b.tris(ctx, tint, &fill::triangulate_evenodd(&loops));
        return;
    }
    let band = hatch_band(tol);
    for seg in pattern_lines_cached(h, &loops, band).iter() {
        if seg.len() == 1 {
            if let Some(p) = seg.first() {
                b.point(ctx, rgb, *p);
            }
        } else {
            b.polyline(ctx, rgb, lw, seg);
        }
    }
}

/// Upper bound on loop count × vertex count for island detection (the nesting test compares
/// every loop with every other); past it all loops are kept (odd parity).
const MAX_ISLAND_WORK: usize = 20_000_000;

/// The tessellated boundary loops that take part in the fill under the hatch's island
/// detection style (DXF group 75), filled with odd parity: 0 normal keeps every loop (islands
/// within islands alternate), 1 outer keeps the outermost loops and their first islands, 2
/// ignore keeps only the outermost loops. Nesting depth = how many other loops enclose a loop.
pub(crate) fn fill_loops(h: &Hatch, tol: f64) -> Vec<Vec<Vec2>> {
    let loops: Vec<Vec<Vec2>> = h.loops.iter().map(|l| Polyline { vertices: l.vertices.clone(), closed: true }.tessellate(tol)).collect();
    let max_depth = match h.style {
        1 => 1,
        2 => 0,
        _ => return loops,
    };
    let verts: usize = loops.iter().map(Vec::len).sum();
    if loops.len().saturating_mul(verts) > MAX_ISLAND_WORK {
        return loops;
    }
    let depth = |i: usize, l: &[Vec2]| {
        let Some(p) = probe(l) else { return 0 };
        loops.iter().enumerate().filter(|(j, o)| *j != i && point_in_polygon(o, p)).count()
    };
    loops.iter().enumerate().filter(|(i, l)| depth(*i, l) <= max_depth).map(|(_, l)| l.clone()).collect()
}

/// A point of a loop to test its nesting with: the middle of its first non-degenerate edge
/// (vertices may touch an enclosing loop; edge midpoints rarely do).
fn probe(l: &[Vec2]) -> Option<Vec2> {
    let n = l.len();
    (0..n).find_map(|i| {
        let (a, b) = (*l.get(i)?, *l.get((i + 1) % n)?);
        (a.dist(b) > 1e-12).then(|| (a + b) * 0.5).filter(|m| m.is_finite())
    })
}

/// Colour steps a gradient is drawn with (one display-list primitive each).
const GRADIENT_LEVELS: usize = 64;
/// Gradient triangles are split until no edge is longer than the larger extent over this.
const GRADIENT_CELLS: f64 = 64.0;
/// Upper bound on the triangles of one gradient fill.
const MAX_GRADIENT_TRIS: usize = 200_000;

/// A two-colour gradient over the fill triangles `tris`: a base fill in the middle colour (it
/// hides seams between steps), then the triangles split into small pieces, each drawn in the
/// colour step of the gradient value at its centroid.
fn gradient(b: &mut Builder, ctx: &Ctx, g: &Gradient, layer: Color, tris: &[Vec2]) {
    let c1 = ink(g.color1, layer, ctx.block_color);
    let c2 = ink(g.color2, layer, ctx.block_color);
    b.tris(ctx, mix(c1, c2, 0.5), tris);
    let Some(field) = Field::new(g, tris) else { return };
    let mut steps: Vec<Vec<Vec2>> = vec![Vec::new(); GRADIENT_LEVELS];
    let max2 = field.cell * field.cell;
    let mut stack: Vec<[Vec2; 3]> = tris.as_chunks::<3>().0.to_vec();
    let mut drawn = 0usize;
    while let Some([a, bb, c]) = stack.pop() {
        // Split the longest edge while the budget allows.
        let edges = [(a, bb, c), (bb, c, a), (c, a, bb)];
        let (p, q, r) = edges.into_iter().max_by(|x, y| (x.0 - x.1).len2().total_cmp(&(y.0 - y.1).len2())).unwrap_or((a, bb, c));
        if (p - q).len2() > max2 && drawn + stack.len() + 2 <= MAX_GRADIENT_TRIS {
            let m = (p + q) * 0.5;
            stack.push([p, m, r]);
            stack.push([m, q, r]);
            continue;
        }
        let t = field.value((a + bb + c) / 3.0);
        let k = (t * (GRADIENT_LEVELS - 1) as f64).round() as usize;
        if let Some(s) = steps.get_mut(k.min(GRADIENT_LEVELS - 1)) {
            s.extend_from_slice(&[a, bb, c]);
        }
        drawn += 1;
    }
    for (k, s) in steps.iter().enumerate() {
        b.tris(ctx, mix(c1, c2, k as f64 / (GRADIENT_LEVELS - 1) as f64), s);
    }
}

/// `a` blended towards `b` by `t`; colour 7 only when both ends are.
fn mix(a: Ink, b: Ink, t: f64) -> Ink {
    let l = |x: u8, y: u8| (f64::from(x) + (f64::from(y) - f64::from(x)) * t).round().clamp(0.0, 255.0) as u8;
    // The plot style follows the nearer end's colour.
    let aci = if t < 0.5 { a.aci } else { b.aci };
    Ink { rgb: Rgb(l(a.rgb.0, b.rgb.0), l(a.rgb.1, b.rgb.1), l(a.rgb.2, b.rgb.2)), aci7: a.aci7 && b.aci7, aci }
}

/// Gradient shapes by name (the `INV` forms swap the two colours).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Shape {
    /// Colour 1 to colour 2 along the gradient direction.
    Linear,
    /// Colour 1 at both sides, colour 2 along the middle.
    Cylinder,
    /// Like linear but bending quickly towards colour 2.
    Curved,
    /// Colour 2 at the centre, colour 1 from the farthest side outwards.
    Spherical,
    /// Like spherical, centred on the bottom edge (in the gradient direction's frame).
    Hemispherical,
}

/// The gradient value (0 = colour 1, 1 = colour 2) over the fill, in a frame whose x axis is
/// the gradient direction.
struct Field {
    shape: Shape,
    inverted: bool,
    dir: Vec2,
    /// Fill bounds in the gradient frame.
    bb: Bounds2,
    /// Centre in the gradient frame (moved up and back when the gradient isn't centred).
    centre: Vec2,
    /// Radius at which a radial gradient reaches colour 1: the centre's distance to the farthest
    /// side of `bb` (the corners beyond it are colour 1).
    reach: f64,
    /// Largest edge length of the pieces drawn.
    cell: f64,
}

impl Field {
    fn new(g: &Gradient, tris: &[Vec2]) -> Option<Field> {
        let name = g.name.trim().to_ascii_uppercase();
        let (inverted, base) = match name.strip_prefix("INV") {
            Some(rest) => (true, rest),
            None => (false, name.as_str()),
        };
        let shape = match base {
            "CYLINDER" => Shape::Cylinder,
            "CURVED" => Shape::Curved,
            "SPHERICAL" => Shape::Spherical,
            "HEMISPHERICAL" => Shape::Hemispherical,
            _ => Shape::Linear,
        };
        let angle = if g.angle.is_finite() { g.angle } else { 0.0 };
        let dir = Vec2::new(angle.cos(), angle.sin());
        let bb = Bounds2::from_points(tris.iter().filter(|p| p.is_finite()).map(|p| Vec2::new(p.dot(dir), p.dot(dir.perp()))));
        let size = bb.width().max(bb.height());
        if bb.is_empty() || !(size.is_finite() && size > 1e-12) {
            return None;
        }
        let (fx, fy) = if g.centered { (0.5, 0.5) } else { (0.3, 0.7) };
        let mut centre = Vec2::new(bb.min.x + bb.width() * fx, bb.min.y + bb.height() * fy);
        if shape == Shape::Hemispherical {
            centre.y = bb.min.y;
        }
        let reach = [centre.x - bb.min.x, bb.max.x - centre.x, centre.y - bb.min.y, bb.max.y - centre.y].into_iter().fold(0.0, f64::max).max(1e-12);
        Some(Field { shape, inverted, dir, bb, centre, reach, cell: size / GRADIENT_CELLS })
    }

    /// The value at world point `p`, in 0..=1.
    fn value(&self, p: Vec2) -> f64 {
        let q = Vec2::new(p.dot(self.dir), p.dot(self.dir.perp()));
        // Position along the direction, 0..1, with the centre mapped to 0.5.
        let along = || {
            let (lo, c, hi) = (self.bb.min.x, self.centre.x, self.bb.max.x);
            if q.x < c { 0.5 * (q.x - lo) / (c - lo).max(1e-12) } else { 0.5 + 0.5 * (q.x - c) / (hi - c).max(1e-12) }
        };
        let t = match self.shape {
            Shape::Linear => along(),
            Shape::Cylinder => 1.0 - (along() - 0.5).abs() * 2.0,
            Shape::Curved => 1.0 - (1.0 - along().clamp(0.0, 1.0)).powi(2),
            Shape::Spherical | Shape::Hemispherical => 1.0 - q.dist(self.centre) / self.reach,
        };
        let t = if t.is_finite() { t.clamp(0.0, 1.0) } else { 0.0 };
        if self.inverted { 1.0 - t } else { t }
    }
}

/// Upper bound on the line families drawn from a pattern definition read from a file.
const MAX_PATTERN_LINES: usize = 1024;

/// Hatch line runs in world coordinates (single-point runs are dots) for the fill `loops`.
pub fn pattern_lines(h: &Hatch, loops: &[Vec<Vec2>]) -> Vec<Vec<Vec2>> {
    // A pattern the library doesn't define draws with the lines its file defined, else as ANSI31.
    let lines = match pattern(&h.pattern) {
        Some(p) => p.lines,
        None if !h.pattern_lines.is_empty() => h.pattern_lines.iter().take(MAX_PATTERN_LINES).cloned().collect(),
        None => pattern("ANSI31").map(|p| p.lines).unwrap_or_default(),
    };
    let scale = if h.scale.is_finite() && h.scale > 1e-9 { h.scale } else { 1.0 };
    let mut out = Vec::new();
    for fam in &lines {
        let ang = fam.angle.to_radians() + h.angle;
        let origin = h.origin + Vec2::new(fam.origin.0, fam.origin.1).rotate(h.angle) * scale;
        let delta = Vec2::new(fam.delta.0, fam.delta.1).rotate(ang) * scale;
        let spacing = (Vec2::new(fam.delta.0, fam.delta.1).y * scale).abs();
        if !spacing.is_finite() || spacing < 1e-9 {
            continue;
        }
        let dashes: Vec<f64> = fam.dashes.iter().map(|d| d * scale).collect();
        family(loops, origin, ang, delta, spacing, &dashes, &mut out);
        if out.len() > 500_000 {
            break;
        }
    }
    out
}

fn family(loops: &[Vec<Vec2>], origin: Vec2, ang: f64, delta: Vec2, spacing: f64, dashes: &[f64], out: &mut Vec<Vec<Vec2>>) {
    // Work in a frame where the family's lines are horizontal.
    let to_local = |p: Vec2| (p - origin).rotate(-ang);
    let to_world = |p: Vec2| origin + p.rotate(ang);
    let local: Vec<Vec<Vec2>> = loops.iter().map(|l| l.iter().map(|p| to_local(*p)).collect()).collect();
    let bb = Bounds2::from_points(local.iter().flatten().copied());
    if bb.is_empty() {
        return;
    }
    let shift_per_line = delta.rotate(-ang).x;
    let k0 = (bb.min.y / spacing).floor() as i64;
    let k1 = (bb.max.y / spacing).ceil() as i64;
    if k1.saturating_sub(k0) > 20_000 {
        return;
    }
    let pat_len: f64 = dashes.iter().map(|d| d.abs()).sum();
    for k in k0..=k1 {
        let y = k as f64 * spacing;
        let mut xs: Vec<f64> = Vec::new();
        for l in &local {
            let n = l.len();
            for i in 0..n {
                let (Some(a), Some(b)) = (l.get(i), l.get((i + 1) % n)) else { continue };
                if (a.y > y) != (b.y > y) {
                    xs.push(a.x + (b.x - a.x) * (y - a.y) / (b.y - a.y));
                }
            }
        }
        xs.sort_by(f64::total_cmp);
        let offset = shift_per_line * k as f64;
        for span in xs.chunks(2) {
            let [x0, x1] = span else { continue };
            if dashes.is_empty() || pat_len < 1e-12 {
                out.push(vec![to_world(Vec2::new(*x0, y)), to_world(Vec2::new(*x1, y))]);
                continue;
            }
            // Dash phase anchored at x = offset.
            let start_rep = ((x0 - offset) / pat_len).floor();
            let mut x = offset + start_rep * pat_len;
            let mut guard = 0;
            'outer: while x < *x1 {
                for d in dashes {
                    guard += 1;
                    if guard > 100_000 {
                        break 'outer;
                    }
                    let len = d.abs();
                    if *d > 0.0 {
                        let a = x.max(*x0);
                        let b = (x + len).min(*x1);
                        if b > a {
                            out.push(vec![to_world(Vec2::new(a, y)), to_world(Vec2::new(b, y))]);
                        }
                    } else if *d == 0.0 && x >= *x0 && x <= *x1 {
                        out.push(vec![to_world(Vec2::new(x, y))]);
                    }
                    x += len;
                    if x >= *x1 {
                        break 'outer;
                    }
                }
            }
        }
    }
}
