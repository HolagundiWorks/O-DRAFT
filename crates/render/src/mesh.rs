//! Screen mesh for the drawing canvas.
//!
//! [`build_mesh`] turns a [`DisplayList`] into the vertex bytes the GPU canvas uploads. The egui
//! shell and the WinUI host both use this layout and [`CANVAS_SHADER`], so there is one mesh path.

use aad_color::Rgb;
use aad_geom::{Bounds2, Vec2};

use crate::{DisplayList, Kind};

/// Line/triangle vertex: position (2 × f32, relative to the origin) + sRGB colour (4 × u8).
pub const VERTEX_SIZE: usize = 12;
/// Wide-line / point instance: a, b (2 × f32 each) + colour (4 × u8) + width in points (f32).
pub const INSTANCE_SIZE: usize = 24;
/// Point markers are 1.5 × 1.5 points (as on the CPU path).
const POINT_SIZE: f32 = 1.5;

/// WGSL for fills and screen-space strokes. `view.a` is offset xy, pixels per world unit,
/// and a linear-output flag. `view.b` is the canvas centre and size in device pixels.
/// `view.c.x` is device pixels per point, so a 1-point hairline stays one logical pixel.
pub const CANVAS_SHADER: &str = r#"
struct View {
    a: vec4<f32>, // offset x, offset y (world), scale (device px per unit), 1 = linear output
    b: vec4<f32>, // canvas centre x, y (device px), screen w, h (device px)
    c: vec4<f32>, // pixels per point
};
@group(0) @binding(0) var<uniform> view: View;

struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) pa: vec2<f32>,
    @location(2) pb: vec2<f32>,
    @location(3) hw: f32,
};

fn to_px(p: vec2<f32>) -> vec2<f32> {
    let w = (p + view.a.xy) * view.a.z;
    return vec2<f32>(view.b.x + w.x, view.b.y - w.y);
}

fn px_to_clip(px: vec2<f32>) -> vec4<f32> {
    return vec4<f32>(px.x / view.b.z * 2.0 - 1.0, 1.0 - px.y / view.b.w * 2.0, 0.0, 1.0);
}

fn out_color(c: vec4<f32>) -> vec4<f32> {
    if (view.a.w > 0.5) {
        let lo = c.rgb / 12.92;
        let hi = pow((c.rgb + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
        return vec4<f32>(select(hi, lo, c.rgb <= vec3<f32>(0.04045)), c.a);
    }
    return c;
}

fn solid(pos: vec4<f32>, color: vec4<f32>) -> VOut {
    var o: VOut;
    o.pos = pos;
    o.color = color;
    o.pa = vec2<f32>(0.0, 0.0);
    o.pb = vec2<f32>(0.0, 0.0);
    o.hw = 0.0;
    return o;
}

@vertex
fn vs_line(@location(0) pos: vec2<f32>, @location(1) color: vec4<f32>) -> VOut {
    return solid(px_to_clip(to_px(pos)), out_color(color));
}

@vertex
fn vs_quad(
    @builtin(vertex_index) vi: u32,
    @location(0) a: vec2<f32>,
    @location(1) b: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) width: f32,
) -> VOut {
    var pa = to_px(a);
    var pb = to_px(b);
    // A hairline is one logical pixel. Snap a thin axis-aligned stroke onto a pixel centre
    // so it fills one row or column instead of splitting across two.
    let px_w = max(width * view.c.x, 1.0);
    let dlt = pb - pa;
    if (px_w <= 3.5 && abs(dlt.y) <= 0.75 && abs(dlt.x) >= abs(dlt.y)) {
        let y = floor((pa.y + pb.y) * 0.5) + 0.5;
        pa.y = y;
        pb.y = y;
    } else if (px_w <= 3.5 && abs(dlt.x) <= 0.75 && abs(dlt.y) > abs(dlt.x)) {
        let x = floor((pa.x + pb.x) * 0.5) + 0.5;
        pa.x = x;
        pb.x = x;
    }
    let delta = pb - pa;
    let len = length(delta);
    var dir = vec2<f32>(1.0, 0.0);
    if (len > 0.0001) {
        dir = delta / len;
    }
    let n = vec2<f32>(-dir.y, dir.x);
    let hw = 0.5 * px_w;
    let t = select(0.0, 1.0, vi == 1u || vi == 4u || vi == 5u);
    let s = select(-1.0, 1.0, vi == 2u || vi == 3u || vi == 5u);
    let along = mix(pa - dir * hw, pb + dir * hw, t);
    var o: VOut;
    o.pos = px_to_clip(along + n * hw * s);
    o.color = out_color(color);
    o.pa = pa;
    o.pb = pb;
    o.hw = hw;
    return o;
}

@fragment
fn fs_main(v: VOut) -> @location(0) vec4<f32> {
    let p = v.pos.xy;
    let ab = v.pb - v.pa;
    let lab = dot(ab, ab);
    let t = select(0.0, clamp(dot(p - v.pa, ab) / max(lab, 0.000001), 0.0, 1.0), lab > 0.0001);
    let d = length(p - (v.pa + ab * t));
    // Fills (hw = 0) stay solid. A stroke covers its width and fades across one pixel at the edge.
    var cover = select(1.0, clamp(v.hw + 0.5 - d, 0.0, 1.0), v.hw > 0.0);
    // Linear-light premultiplied blend thins dark-on-white edges; raise coverage toward display
    // gamma so a black hairline keeps its visual weight on a white sheet.
    if (v.hw > 0.0 && view.a.w > 0.5) {
        cover = pow(cover, 0.454545);
    }
    let a = v.color.a * cover;
    return vec4<f32>(v.color.rgb * a, a);
}
"#;

/// CPU-side vertex data for one display list, waiting to be uploaded.
#[derive(Default)]
pub struct MeshData {
    pub lines: Vec<u8>,
    pub tris: Vec<u8>,
    pub quads: Vec<u8>,
    /// Draw-order breaks, one before each wipeout mask: the fill vertices, line vertices and
    /// quad instances drawn before it. Each group is drawn fills, lines, quads in turn.
    pub breaks: Vec<[usize; 3]>,
}

impl MeshData {
    pub fn line_vertices(&self) -> usize {
        self.lines.len() / VERTEX_SIZE
    }
    pub fn tri_vertices(&self) -> usize {
        self.tris.len() / VERTEX_SIZE
    }
    pub fn quad_instances(&self) -> usize {
        self.quads.len() / INSTANCE_SIZE
    }
}

fn rel(p: Vec2, origin: Vec2) -> [f32; 2] {
    [(p.x - origin.x) as f32, (p.y - origin.y) as f32]
}

fn push_vertex(out: &mut Vec<u8>, p: [f32; 2], c: [u8; 4]) {
    out.extend_from_slice(&p[0].to_le_bytes());
    out.extend_from_slice(&p[1].to_le_bytes());
    out.extend_from_slice(&c);
}

/// Append a filled axis-aligned-ish quad (two triangles) with explicit RGBA.
///
/// Used for translucent selection-window fills on the overlay; [`build_mesh`] always writes opaque colours.
pub fn append_fill_quad(mesh: &mut MeshData, origin: Vec2, corners: [Vec2; 4], rgba: [u8; 4]) {
    if !corners.iter().all(|p| p.is_finite()) {
        return;
    }
    for p in [corners[0], corners[1], corners[2], corners[0], corners[2], corners[3]] {
        push_vertex(&mut mesh.tris, rel(p, origin), rgba);
    }
}

fn push_instance(out: &mut Vec<u8>, a: [f32; 2], b: [f32; 2], c: [u8; 4], width: f32) {
    for v in [a[0], a[1], b[0], b[1]] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&c);
    out.extend_from_slice(&width.to_le_bytes());
}

/// Lineweight in screen points for a primitive (as on the CPU path), or `None` for a hairline.
///
/// When LWDISPLAY is on, millimetre weights map to points (`lw × 3.78`) and are never thinner
/// than [`HAIRLINE_POINTS`]. That keeps 0.25 / 0.30 / 0.35 mm monotonic: previously 0.25 fell
/// through to the hairline expander at 1.5 pt while 0.30/0.35 drew as ~1.13/1.32 pt quads.
pub fn wide_width(lw: f32, lwdisplay: bool) -> Option<f32> {
    if !lwdisplay || lw <= 0.0 {
        return None;
    }
    let w = (lw * 3.78).clamp(HAIRLINE_POINTS, 12.0);
    Some(w)
}

/// Convert a display list into vertex data relative to `origin`, colours resolved against `bg`.
pub fn build_mesh(list: &DisplayList, origin: Vec2, bg: Rgb, lwdisplay: bool) -> MeshData {
    let mut m = MeshData {
        lines: Vec::with_capacity(list.segment_count().saturating_mul(2 * VERTEX_SIZE)),
        tris: Vec::with_capacity(list.tris.len().saturating_mul(VERTEX_SIZE)),
        quads: Vec::new(),
        breaks: Vec::new(),
    };
    for prim in &list.prims {
        let c = prim.display_rgb(bg);
        let col = [c.0, c.1, c.2, 255];
        let pts = list.points(prim);
        match prim.kind {
            Kind::Polyline => match wide_width(prim.lw, lwdisplay) {
                Some(w) => {
                    for s in pts.windows(2) {
                        if let [a, b] = s {
                            push_instance(&mut m.quads, rel(*a, origin), rel(*b, origin), col, w);
                        }
                    }
                }
                None => {
                    for s in pts.windows(2) {
                        if let [a, b] = s {
                            push_vertex(&mut m.lines, rel(*a, origin), col);
                            push_vertex(&mut m.lines, rel(*b, origin), col);
                        }
                    }
                }
            },
            Kind::Tris => {
                for t in pts.as_chunks::<3>().0 {
                    for q in t {
                        push_vertex(&mut m.tris, rel(*q, origin), col);
                    }
                }
            }
            Kind::Mask => {
                m.breaks.push([m.tri_vertices(), m.line_vertices(), m.quad_instances()]);
                for t in pts.as_chunks::<3>().0 {
                    for q in t {
                        push_vertex(&mut m.tris, rel(*q, origin), [bg.0, bg.1, bg.2, 255]);
                    }
                }
            }
            Kind::Point => {
                if let Some(q) = pts.first() {
                    let p = rel(*q, origin);
                    push_instance(&mut m.quads, p, p, col, POINT_SIZE);
                }
            }
            // Drawn by the shell each frame (clipped to the view).
            Kind::Infinite { .. } => {}
        }
    }
    m
}

/// Clip construction lines and rays to the view and append them as hairline segments.
///
/// [`build_mesh`] leaves `Kind::Infinite` out. The host calls this before [`expand_hairlines`].
pub fn append_infinite(mesh: &mut MeshData, list: &DisplayList, origin: Vec2, center: Vec2, width: f64, height: f64, bg: Rgb) {
    if !(width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0 && center.is_finite()) {
        return;
    }
    let vis = Bounds2::from_points([
        Vec2::new(center.x - width * 0.5, center.y - height * 0.5),
        Vec2::new(center.x + width * 0.5, center.y + height * 0.5),
    ]);
    for prim in &list.prims {
        let Kind::Infinite { ray } = prim.kind else { continue };
        let pts = list.points(prim);
        let (Some(base), Some(dir)) = (pts.first(), pts.get(1)) else { continue };
        if !dir.is_finite() || dir.len() < 1e-12 {
            continue;
        }
        let far = (vis.width() + vis.height()) * 2.0 + base.dist(vis.center());
        if !far.is_finite() {
            continue;
        }
        let a = if ray { *base } else { *base - *dir * far };
        let b = *base + *dir * far;
        if !a.is_finite() || !b.is_finite() {
            continue;
        }
        let rgb = prim.display_rgb(bg);
        let col = [rgb.0, rgb.1, rgb.2, 255];
        push_vertex(&mut mesh.lines, rel(a, origin), col);
        push_vertex(&mut mesh.lines, rel(b, origin), col);
    }
}

/// Default document hairline width in points (logical pixels ≈ CSS px at 96 DPI).
///
/// AutoCAD model-space hairlines are about one device pixel; on HiDPI that reads as frail.
/// 1.5 px is the default displayed weight after pixel-centre snap when LWDISPLAY is off.
pub const HAIRLINE_POINTS: f32 = 1.5;

/// Turn hairline segments into screen-space quads, in the same draw order.
///
/// Hardware line lists leave gaps on diagonals and draw a single device pixel, which is thinner
/// than one logical pixel on a scaled display. The quad shader strokes these in screen space.
pub fn expand_hairlines(mesh: &mut MeshData) {
    expand_hairlines_width(mesh, HAIRLINE_POINTS);
}

/// Like [`expand_hairlines`], with an explicit stroke width in points.
///
/// Overlay hairlines (crosshair, rubber band) use a half-point width so they stay near one device
/// pixel on common display scales, matching AutoCAD-style thin crosshairs.
pub fn expand_hairlines_width(mesh: &mut MeshData, width: f32) {
    if mesh.line_vertices() < 2 {
        return;
    }
    let width = if width.is_finite() && width > 0.0 { width } else { 1.0 };
    let line_n = mesh.line_vertices();
    let quad_n = mesh.quad_instances();
    let mut quads = Vec::with_capacity(mesh.quads.len() + (line_n / 2) * INSTANCE_SIZE);
    let mut breaks = Vec::with_capacity(mesh.breaks.len());
    let mut line_from = 0usize;
    let mut quad_from = 0usize;
    let end = [mesh.tri_vertices(), line_n, quad_n];
    for [tri, line_to, quad_to] in mesh.breaks.iter().copied().chain(std::iter::once(end)) {
        let mut i = line_from;
        while i + 1 < line_to {
            if let (Some((a, color)), Some((b, _))) = (read_vertex(&mesh.lines, i), read_vertex(&mesh.lines, i + 1)) {
                push_instance(&mut quads, a, b, color, width);
            }
            i += 2;
        }
        let lo = quad_from.saturating_mul(INSTANCE_SIZE);
        let hi = quad_to.saturating_mul(INSTANCE_SIZE).min(mesh.quads.len());
        if lo < hi {
            quads.extend_from_slice(&mesh.quads[lo..hi]);
        }
        breaks.push([tri, 0, quads.len() / INSTANCE_SIZE]);
        line_from = line_to;
        quad_from = quad_to;
    }
    breaks.pop();
    mesh.lines.clear();
    mesh.quads = quads;
    mesh.breaks = breaks;
}

fn read_vertex(bytes: &[u8], index: usize) -> Option<([f32; 2], [u8; 4])> {
    let at = index.checked_mul(VERTEX_SIZE)?;
    let b = bytes.get(at..at + VERTEX_SIZE)?;
    let x = f32::from_le_bytes([b[0], b[1], b[2], b[3]]);
    let y = f32::from_le_bytes([b[4], b[5], b[6], b[7]]);
    Some(([x, y], [b[8], b[9], b[10], b[11]]))
}

/// The vertex origin for a display list seen at `view_center` / `view_height`: the list's centre
/// normally; at deep zoom (the view much smaller than the drawing) a point near the view, snapped
/// to a coarse grid so panning re-uploads only every few dozen screens.
pub fn choose_origin(bounds: &Bounds2, view_center: Vec2, view_height: f64) -> Vec2 {
    let size = bounds.width().max(bounds.height());
    if bounds.is_empty()
        || !view_center.is_finite()
        || view_height.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater)
        || view_height * 2000.0 >= size
    {
        return if bounds.is_empty() { Vec2::ZERO } else { bounds.center() };
    }
    let step = view_height * 64.0;
    Vec2::new((view_center.x / step).round() * step, (view_center.y / step).round() * step)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn hairlines_become_quads_and_keep_wipeout_order() {
        let mut mesh = MeshData::default();
        push_vertex(&mut mesh.lines, [0.0, 0.0], [1, 2, 3, 255]);
        push_vertex(&mut mesh.lines, [2.0, 0.0], [1, 2, 3, 255]);
        push_instance(&mut mesh.quads, [0.0, 0.0], [1.0, 0.0], [9, 9, 9, 255], 2.0);
        push_vertex(&mut mesh.lines, [0.0, 1.0], [4, 5, 6, 255]);
        push_vertex(&mut mesh.lines, [0.0, 3.0], [4, 5, 6, 255]);
        mesh.breaks.push([0, 2, 1]);
        expand_hairlines(&mut mesh);
        assert_eq!(mesh.line_vertices(), 0);
        assert_eq!(mesh.quad_instances(), 3);
        assert_eq!(mesh.breaks, vec![[0, 0, 2]]);
        let width = f32::from_le_bytes(mesh.quads[20..24].try_into().expect("width bytes"));
        assert_eq!(width, HAIRLINE_POINTS);
    }

    #[test]
    fn displayed_lineweights_are_monotonic_and_at_least_hairline() {
        assert!(wide_width(0.25, false).is_none());
        let mut prev = 0.0_f32;
        for &hundredths in &aad_doc::Lineweight::STANDARD {
            let mm = f32::from(hundredths) / 100.0;
            let Some(w) = wide_width(mm, true) else {
                // 0.00 mm stays a hairline expander path.
                assert_eq!(hundredths, 0);
                continue;
            };
            assert!(w >= HAIRLINE_POINTS, "{mm} mm → {w} pt below hairline");
            assert!(w + 1e-5 >= prev, "{mm} mm ({w} pt) thinner than previous ({prev} pt)");
            prev = w;
        }
        assert!(prev > HAIRLINE_POINTS, "heavier standard weights must exceed the hairline floor");
    }
}
