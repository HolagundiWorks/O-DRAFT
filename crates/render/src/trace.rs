//! Level traces: another level shown read-only, faded, under or over the level being edited.
//!
//! A trace is a display list of the traced level's visible objects whose colours are greyed and
//! blended towards the background, whose lines are drawn at the thinnest weight, and whose
//! primitives carry [`VIEWPORT_CONTENT`] as their handle so selection highlighting never matches
//! them. The traces are merged around the main list in draw order: traces marked Below, the edit
//! target, traces marked Above. The result is one ordinary [`DisplayList`], so the GPU mesh, the
//! raster and the egui canvas need no change. The fade bakes in the background colour, so a list
//! built here must be rebuilt when the theme changes.

use aad_color::Rgb;
use aad_doc::{Drawing, Space, Trace, TraceColour, TraceOrder};

use crate::{DPrim, DisplayList, Kind, Options, VIEWPORT_CONTENT, build, build_entities};

/// Grey and fade the primitives of a trace list in place. Wipeouts are dropped (they would hide
/// the level being edited), lineweights go to the thinnest, and colour 7 is resolved against
/// `background` first so the result no longer depends on it.
pub fn stylize_trace(list: &mut DisplayList, fade: u8, true_colour: bool, background: Rgb) {
    let f = f32::from(fade.min(90)) / 100.0;
    let mix = |a: u8, b: u8| (f32::from(a) * (1.0 - f) + f32::from(b) * f).round().clamp(0.0, 255.0) as u8;
    list.prims.retain(|p| p.kind != Kind::Mask);
    for p in &mut list.prims {
        let c = p.display_rgb(background);
        let c = if true_colour {
            c
        } else {
            let l = (0.299 * f32::from(c.0) + 0.587 * f32::from(c.1) + 0.114 * f32::from(c.2)).round().clamp(0.0, 255.0) as u8;
            Rgb(l, l, l)
        };
        p.color = Rgb(mix(c.0, background.0), mix(c.1, background.1), mix(c.2, background.2));
        p.aci7 = false;
        p.lw = 0.0;
        p.handle = VIEWPORT_CONTENT;
    }
}

/// Append `src` after `dst`'s primitives, moving its vertex references.
fn append(dst: &mut DisplayList, src: DisplayList) {
    let (vo, to) = (dst.verts.len() as u32, dst.tris.len() as u32);
    dst.prims.extend(src.prims.into_iter().map(|p: DPrim| match p.kind {
        Kind::Tris | Kind::Mask => DPrim { start: p.start + to, ..p },
        _ => DPrim { start: p.start + vo, ..p },
    }));
    dst.verts.extend(src.verts);
    dst.tris.extend(src.tris);
    dst.bounds = dst.bounds.union(&src.bounds);
    dst.expanded = dst.expanded.saturating_add(src.expanded);
}

/// The display list of one trace: the visible objects of the traced level, restricted to the
/// trace's layer list when it has one.
fn trace_list(d: &Drawing, t: &Trace, opts: &Options, background: Rgb) -> Option<DisplayList> {
    let store = d.level_store(t.level)?;
    let keep = |e: &aad_doc::Entity| d.is_visible(e) && t.layers.as_ref().is_none_or(|ls| ls.iter().any(|n| n.eq_ignore_ascii_case(&e.common.layer)));
    let mut list = build_entities(d, store.iter().map(|e| e.as_ref()).filter(|e| keep(e)), opts);
    stylize_trace(&mut list, t.fade, t.colour == TraceColour::TrueColour, background);
    Some(list)
}

/// How faded the level shown for reference in a sheet view is (percent).
pub const REFERENCE_FADE: u8 = 35;

/// The level shown for reference in a sheet view: its visible objects in their own colours,
/// lightly faded, so the annotation drawn over it stands out.
fn reference_list(d: &Drawing, opts: &Options, background: Rgb) -> Option<DisplayList> {
    let store = d.reference_store()?;
    let mut list = build_entities(d, store.iter().map(|e| e.as_ref()).filter(|e| d.is_visible(e)), opts);
    stylize_trace(&mut list, REFERENCE_FADE, true, background);
    Some(list)
}

/// Model space of the edit target with what is shown around it: Below traces, the level shown
/// for reference (in a sheet view), the edit target, Above traces. A drawing without levels, or
/// a level without traces or sheet view, gives exactly [`build`].
pub fn build_with_traces(d: &Drawing, opts: &Options, background: Rgb) -> DisplayList {
    let main = build(d, &Space::Model, opts);
    let reference = reference_list(d, opts, background);
    let traces = d.active_level_ref().map(|l| l.traces.as_slice()).unwrap_or(&[]);
    if reference.is_none() && traces.is_empty() {
        return main;
    }
    let (mut below, mut above) = (Vec::new(), Vec::new());
    for t in traces {
        if let Some(l) = trace_list(d, t, opts, background) {
            if t.order == TraceOrder::Below { below.push(l) } else { above.push(l) }
        }
    }
    let mut out = DisplayList::default();
    for l in below {
        append(&mut out, l);
    }
    if let Some(r) = reference {
        append(&mut out, r);
    }
    append(&mut out, main);
    for l in above {
        append(&mut out, l);
    }
    out
}
