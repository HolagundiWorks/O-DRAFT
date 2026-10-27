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
/// Circle/arc ring instance: centre (2×f32) + (radius, start) + (sweep, width) + colour (4×u8) + pad.
pub const RING_INSTANCE_SIZE: usize = 32;
/// Point markers are 1.5 × 1.5 points (as on the CPU path).
const POINT_SIZE: f32 = 1.5;

/// WGSL for fills and screen-space strokes. `view.a` is offset xy, pixels per world unit,
/// and a linear-output flag. `view.b` is the canvas centre and size in device pixels.
/// `view.c.x` is device pixels per point; `view.c.y` selects true-scale lineweights (world mm).
pub const CANVAS_SHADER: &str = r#"
struct View {
    a: vec4<f32>, // offset x, offset y (world), scale (device px per unit), 1 = linear output
    b: vec4<f32>, // canvas centre x, y (device px), screen w, h (device px)
    c: vec4<f32>, // pixels per point, 1 = true-scale lineweights (width in world units)
};
@group(0) @binding(0) var<uniform> view: View;

struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) pa: vec2<f32>,
    @location(2) pb: vec2<f32>,
    @location(3) hw: f32,
    // 0 = segment stroke (pa/pb endpoints). >0 = ring: pa=centre_px, pb=(r_px, start), sweep=radians.
    @location(4) sweep: f32,
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

fn stroke_px_w(width: f32) -> f32 {
    let w_abs = abs(width);
    return max(select(1.5 * view.c.x, w_abs * select(view.c.x, view.a.z, view.c.y > 0.5), w_abs > 1e-6), 1.0);
}

fn solid(pos: vec4<f32>, color: vec4<f32>) -> VOut {
    var o: VOut;
    o.pos = pos;
    o.color = color;
    o.pa = vec2<f32>(0.0, 0.0);
    o.pb = vec2<f32>(0.0, 0.0);
    o.hw = 0.0;
    o.sweep = 0.0;
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
    // Negative width = butt caps (joined strip edges). Positive = round end extensions.
    let butt = width < 0.0;
    // Screen: width is points × pixels-per-point. True scale (layouts): width is world mm × scale.
    // Width ~0 is a hairline sentinel (always screen points) so default strokes stay thin on paper.
    let px_w = stroke_px_w(width);
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
    let extend = select(hw, 0.0, butt);
    let along = mix(pa - dir * extend, pb + dir * extend, t);
    var o: VOut;
    o.pos = px_to_clip(along + n * hw * s);
    o.color = out_color(color);
    o.pa = pa;
    o.pb = pb;
    o.hw = hw;
    o.sweep = 0.0;
    return o;
}

@vertex
fn vs_ring(
    @builtin(vertex_index) vi: u32,
    @location(0) center: vec2<f32>,
    @location(1) rs: vec2<f32>,
    @location(2) sw: vec2<f32>,
    @location(3) color: vec4<f32>,
) -> VOut {
    let radius = rs.x;
    let start = rs.y;
    let sweep = sw.x;
    let width = sw.y;
    let px_w = stroke_px_w(width);
    let hw = 0.5 * px_w;
    let c = to_px(center);
    let r_px = abs(radius) * view.a.z;
    let pad = hw + 1.5;
    let ext = r_px + pad;
    let t = select(0.0, 1.0, vi == 1u || vi == 4u || vi == 5u);
    let s = select(-1.0, 1.0, vi == 2u || vi == 3u || vi == 5u);
    let px = c + vec2<f32>((2.0 * t - 1.0) * ext, (2.0 * s - 1.0) * ext);
    var o: VOut;
    o.pos = px_to_clip(px);
    o.color = out_color(color);
    o.pa = c;
    o.pb = vec2<f32>(r_px, start);
    o.hw = hw;
    o.sweep = max(sweep, 1e-6);
    return o;
}

@fragment
fn fs_main(v: VOut) -> @location(0) vec4<f32> {
    let p = v.pos.xy;
    var d: f32;
    if (v.sweep > 0.0) {
        let c = v.pa;
        let r = v.pb.x;
        let start = v.pb.y;
        let dxy = p - c;
        let d_ring = abs(length(dxy) - r);
        // Screen Y is flipped vs world; world angle θ maps to atan2(-dy, dx) = -θ.
        let ang = -atan2(dxy.y, dxy.x);
        let tau = 6.28318530718;
        let full = v.sweep >= (tau - 1e-3);
        if (full) {
            d = d_ring;
        } else {
            var a = ang - start;
            a = a - floor(a / tau) * tau;
            if (a < 0.0) {
                a = a + tau;
            }
            if (a <= v.sweep) {
                d = d_ring;
            } else {
                let a0 = start;
                let a1 = start + v.sweep;
                // World (cos θ, sin θ) → screen offset (cos θ, -sin θ) × r.
                let p0 = c + vec2<f32>(cos(a0), -sin(a0)) * r;
                let p1 = c + vec2<f32>(cos(a1), -sin(a1)) * r;
                d = min(length(p - p0), length(p - p1));
            }
        }
    } else {
        let ab = v.pb - v.pa;
        let lab = dot(ab, ab);
        let t = select(0.0, clamp(dot(p - v.pa, ab) / max(lab, 0.000001), 0.0, 1.0), lab > 0.0001);
        d = length(p - (v.pa + ab * t));
    }
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
    /// Distance-field circle/arc stroke instances ([`RING_INSTANCE_SIZE`] each).
    pub rings: Vec<u8>,
    /// Draw-order breaks, one before each wipeout mask: fill / line / quad / ring counts drawn
    /// before it. Each group is drawn fills, lines, quads, rings in turn.
    pub breaks: Vec<[usize; 4]>,
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
    pub fn ring_instances(&self) -> usize {
        self.rings.len() / RING_INSTANCE_SIZE
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

fn push_ring(out: &mut Vec<u8>, center: [f32; 2], radius: f32, start: f32, sweep: f32, width: f32, c: [u8; 4]) {
    for v in [center[0], center[1], radius, start, sweep, width] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&c);
    out.extend_from_slice(&[0u8; 4]);
}

/// Append one distance-field circle/arc stroke (world coords relative to `origin`).
pub fn stroke_arc(out: &mut Vec<u8>, center: Vec2, radius: f64, start: f64, sweep: f64, origin: Vec2, col: [u8; 4], width: f32) {
    if !(center.is_finite() && radius.is_finite() && sweep.is_finite()) || radius <= 1e-12 || sweep <= 0.0 {
        return;
    }
    let width = if width.is_finite() && width > 0.0 { width } else { HAIRLINE_POINTS };
    push_ring(out, rel(center, origin), radius as f32, start as f32, sweep as f32, width, col);
}

/// Screen-space stroke for a polyline: butt-ended edges plus round joins/caps.
///
/// One round-capped quad per segment double-blends soft edges at every joint (arcs look beaded).
/// Joined strips use butt edges (`width` stored negative) and a round disc (a == b) at each join
/// and open end so coverage meets once.
pub fn stroke_polyline(out: &mut Vec<u8>, pts: &[Vec2], origin: Vec2, col: [u8; 4], width: f32) {
    let width = if width.is_finite() && width > 0.0 { width } else { HAIRLINE_POINTS };
    let mut clean: Vec<Vec2> = Vec::with_capacity(pts.len());
    for &p in pts {
        if !p.is_finite() {
            continue;
        }
        if clean.last().is_some_and(|q| q.dist(p) < 1e-12) {
            continue;
        }
        clean.push(p);
    }
    let n = clean.len();
    if n < 2 {
        return;
    }
    let closed = n >= 3 && clean[0].dist(clean[n - 1]) < 1e-9;
    if closed {
        clean.pop();
    }
    let n = clean.len();
    if n < 2 {
        return;
    }
    if n == 2 && !closed {
        // Lone segment: one round-capped quad (same as the old path).
        push_instance(out, rel(clean[0], origin), rel(clean[1], origin), col, width);
        return;
    }
    let last = if closed { n - 1 } else { n - 2 };
    for i in 0..=last {
        let j = if i + 1 < n { i + 1 } else { 0 };
        push_instance(out, rel(clean[i], origin), rel(clean[j], origin), col, -width);
    }
    if closed {
        for p in &clean {
            let r = rel(*p, origin);
            push_instance(out, r, r, col, width);
        }
    } else {
        let start = rel(clean[0], origin);
        let end = rel(clean[n - 1], origin);
        push_instance(out, start, start, col, width);
        push_instance(out, end, end, col, width);
        for p in &clean[1..n - 1] {
            let r = rel(*p, origin);
            push_instance(out, r, r, col, width);
        }
    }
}

/// How object lineweights map to stroke width when LWDISPLAY is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineweightMode {
    /// Fixed screen pixels (model space): `lw × 3.78` points, floored at [`HAIRLINE_POINTS`].
    Screen,
    /// Paper millimetres (layouts): instance width stays in world units; the shader × scale.
    TrueScale,
}

/// Lineweight width for a primitive, or `None` for a hairline.
///
/// Screen mode: millimetres → points (`lw × 3.78`), never thinner than [`HAIRLINE_POINTS`].
/// True scale: returns `lw` as world millimetres for the shader to scale with the view.
pub fn wide_width(lw: f32, mode: Option<LineweightMode>) -> Option<f32> {
    let Some(mode) = mode else {
        return None;
    };
    if lw <= 0.0 {
        return None;
    }
    match mode {
        LineweightMode::Screen => Some((lw * 3.78).clamp(HAIRLINE_POINTS, 12.0)),
        LineweightMode::TrueScale => Some(lw),
    }
}

/// Convert a display list into vertex data relative to `origin`, colours resolved against `bg`.
pub fn build_mesh(list: &DisplayList, origin: Vec2, bg: Rgb, lwdisplay: bool) -> MeshData {
    build_mesh_mode(list, origin, bg, lwdisplay.then_some(LineweightMode::Screen))
}

/// Like [`build_mesh`], with an explicit lineweight display mode (screen vs true scale).
pub fn build_mesh_mode(list: &DisplayList, origin: Vec2, bg: Rgb, lw: Option<LineweightMode>) -> MeshData {
    let mut m = MeshData {
        lines: Vec::with_capacity(list.segment_count().saturating_mul(2 * VERTEX_SIZE)),
        tris: Vec::with_capacity(list.tris.len().saturating_mul(VERTEX_SIZE)),
        quads: Vec::new(),
        rings: Vec::new(),
        breaks: Vec::new(),
    };
    for prim in &list.prims {
        let c = prim.display_rgb(bg);
        let col = [c.0, c.1, c.2, 255];
        let pts = list.points(prim);
        let hair = if matches!(lw, Some(LineweightMode::TrueScale)) { 0.0 } else { HAIRLINE_POINTS };
        let w = wide_width(prim.lw, lw).unwrap_or(hair);
        match prim.kind {
            Kind::Polyline => {
                stroke_polyline(&mut m.quads, pts, origin, col, w);
            }
            Kind::Arc { sweep } => {
                if let (Some(center), Some(rim)) = (pts.first(), pts.get(1)) {
                    let radius = center.dist(*rim);
                    let start = center.angle_to(*rim);
                    stroke_arc(&mut m.rings, *center, radius, start, f64::from(sweep), origin, col, w);
                }
            }
            Kind::Tris => {
                for t in pts.as_chunks::<3>().0 {
                    for q in t {
                        push_vertex(&mut m.tris, rel(*q, origin), col);
                    }
                }
            }
            Kind::Mask => {
                m.breaks.push([m.tri_vertices(), m.line_vertices(), m.quad_instances(), m.ring_instances()]);
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

/// Like [`expand_hairlines`] for a true-scale lineweight mesh: width `0` is the shader hairline sentinel.
pub fn expand_hairlines_true_scale(mesh: &mut MeshData) {
    expand_hairlines_width(mesh, 0.0);
}

/// Like [`expand_hairlines`], with an explicit stroke width in points.
///
/// Overlay hairlines (crosshair, rubber band) use a half-point width so they stay near one device
/// pixel on common display scales, matching AutoCAD-style thin crosshairs.
/// Width `0` is kept (true-scale hairline sentinel for the canvas shader).
pub fn expand_hairlines_width(mesh: &mut MeshData, width: f32) {
    if mesh.line_vertices() < 2 {
        return;
    }
    let width = if width.is_finite() && width >= 0.0 { width } else { 1.0 };
    let line_n = mesh.line_vertices();
    let quad_n = mesh.quad_instances();
    let ring_n = mesh.ring_instances();
    let mut quads = Vec::with_capacity(mesh.quads.len() + (line_n / 2) * INSTANCE_SIZE);
    let mut breaks = Vec::with_capacity(mesh.breaks.len());
    let mut line_from = 0usize;
    let mut quad_from = 0usize;
    let end = [mesh.tri_vertices(), line_n, quad_n, ring_n];
    for [tri, line_to, quad_to, ring_to] in mesh.breaks.iter().copied().chain(std::iter::once(end)) {
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
        breaks.push([tri, 0, quads.len() / INSTANCE_SIZE, ring_to]);
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
        mesh.breaks.push([0, 2, 1, 0]);
        expand_hairlines(&mut mesh);
        assert_eq!(mesh.line_vertices(), 0);
        assert_eq!(mesh.quad_instances(), 3);
        assert_eq!(mesh.breaks, vec![[0, 0, 2, 0]]);
        let width = f32::from_le_bytes(mesh.quads[20..24].try_into().expect("width bytes"));
        assert_eq!(width, HAIRLINE_POINTS);
    }

    #[test]
    fn gpu_arc_emits_one_ring_instance() {
        let mut list = DisplayList::default();
        list.verts.extend([Vec2::new(10.0, 20.0), Vec2::new(15.0, 20.0)]);
        list.prims.push(crate::DPrim {
            handle: aad_doc::Handle(1),
            color: Rgb(0, 0, 0),
            aci7: false,
            lw: 0.0,
            kind: Kind::Arc { sweep: std::f32::consts::TAU },
            start: 0,
            len: 2,
        });
        let m = build_mesh(&list, Vec2::ZERO, Rgb(255, 255, 255), false);
        assert_eq!(m.ring_instances(), 1);
        assert_eq!(m.quad_instances(), 0);
        let r = f32::from_le_bytes(m.rings[8..12].try_into().expect("radius"));
        assert!((r - 5.0).abs() < 1e-5, "radius {r}");
    }

    #[test]
    fn displayed_lineweights_are_monotonic_and_at_least_hairline() {
        assert!(wide_width(0.25, None).is_none());
        let mut prev = 0.0_f32;
        for &hundredths in &aad_doc::Lineweight::STANDARD {
            let mm = f32::from(hundredths) / 100.0;
            let Some(w) = wide_width(mm, Some(LineweightMode::Screen)) else {
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

    #[test]
    fn true_scale_lineweights_keep_world_millimetres() {
        assert_eq!(wide_width(0.5, Some(LineweightMode::TrueScale)), Some(0.5));
        assert_eq!(wide_width(0.25, Some(LineweightMode::TrueScale)), Some(0.25));
        assert!(wide_width(0.0, Some(LineweightMode::TrueScale)).is_none());
        let mut list = DisplayList::default();
        list.prims.push(crate::DPrim {
            handle: aad_doc::Handle(1),
            color: Rgb(0, 0, 0),
            aci7: false,
            lw: 0.5,
            kind: Kind::Polyline,
            start: 0,
            len: 2,
        });
        list.verts.extend([Vec2::ZERO, Vec2::new(10.0, 0.0)]);
        list.bounds.add(Vec2::ZERO);
        list.bounds.add(Vec2::new(10.0, 0.0));
        let screen = build_mesh_mode(&list, Vec2::ZERO, Rgb(255, 255, 255), Some(LineweightMode::Screen));
        let paper = build_mesh_mode(&list, Vec2::ZERO, Rgb(255, 255, 255), Some(LineweightMode::TrueScale));
        let sw = instance_width(&screen.quads, 0).abs();
        let pw = instance_width(&paper.quads, 0).abs();
        assert!((sw - (0.5_f32 * 3.78).clamp(HAIRLINE_POINTS, 12.0)).abs() < 1e-4);
        assert!((pw - 0.5).abs() < 1e-4, "true-scale keeps millimetres, got {pw}");
    }

    fn instance_width(quads: &[u8], index: usize) -> f32 {
        let at = index * INSTANCE_SIZE + 20;
        f32::from_le_bytes(quads[at..at + 4].try_into().expect("width"))
    }

    fn instance_is_point(quads: &[u8], index: usize) -> bool {
        let at = index * INSTANCE_SIZE;
        let a = f32::from_le_bytes(quads[at..at + 4].try_into().unwrap());
        let b = f32::from_le_bytes(quads[at + 4..at + 8].try_into().unwrap());
        let c = f32::from_le_bytes(quads[at + 8..at + 12].try_into().unwrap());
        let d = f32::from_le_bytes(quads[at + 12..at + 16].try_into().unwrap());
        (a - c).abs() < 1e-6 && (b - d).abs() < 1e-6
    }

    #[test]
    fn polyline_strip_uses_butt_edges_and_round_joins() {
        let pts = [Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0), Vec2::new(0.0, 10.0)];
        let mut quads = Vec::new();
        stroke_polyline(&mut quads, &pts, Vec2::ZERO, [0, 0, 0, 255], 2.0);
        // 3 butt edges + 2 end caps + 2 interior joins = 7
        assert_eq!(quads.len() / INSTANCE_SIZE, 7);
        let mut butts = 0;
        let mut discs = 0;
        for i in 0..7 {
            let w = instance_width(&quads, i);
            if instance_is_point(&quads, i) {
                assert!(w > 0.0, "join/cap discs use round (positive) width");
                discs += 1;
            } else {
                assert!(w < 0.0, "strip edges use butt (negative) width");
                butts += 1;
            }
        }
        assert_eq!(butts, 3);
        assert_eq!(discs, 4);
    }

    #[test]
    fn closed_polyline_strip_has_joins_at_every_vertex() {
        let pts = [Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0), Vec2::new(1.0, 1.0), Vec2::new(0.0, 0.0)];
        let mut quads = Vec::new();
        stroke_polyline(&mut quads, &pts, Vec2::ZERO, [0, 0, 0, 255], 1.5);
        // 3 edges + 3 vertex discs
        assert_eq!(quads.len() / INSTANCE_SIZE, 6);
        assert_eq!((0..6).filter(|&i| instance_is_point(&quads, i)).count(), 3);
    }

    #[test]
    fn two_point_polyline_stays_one_round_quad() {
        let pts = [Vec2::new(0.0, 0.0), Vec2::new(5.0, 0.0)];
        let mut quads = Vec::new();
        stroke_polyline(&mut quads, &pts, Vec2::ZERO, [0, 0, 0, 255], HAIRLINE_POINTS);
        assert_eq!(quads.len() / INSTANCE_SIZE, 1);
        assert!(instance_width(&quads, 0) > 0.0);
    }
}
