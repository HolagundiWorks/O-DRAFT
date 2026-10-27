//! Level traces in the display list: order, fade, grey, visibility and selection safety.

use aad_color::Rgb;
use aad_doc::{Common, Drawing, EntityKind, Line, Space, Trace, TraceColour, TraceOrder};
use aad_geom::Vec3;
use aad_render::{Kind, Options, VIEWPORT_CONTENT, build, build_with_traces};

const BG: Rgb = Rgb(0, 0, 0);

fn line(d: &mut Drawing, y: f64, layer: &str, colour: i16) -> aad_doc::Handle {
    d.ensure_layer(layer);
    let common = Common { layer: layer.into(), color: aad_color::Color::Index(colour as u8), ..Common::default() };
    d.add(&Space::Model, common, EntityKind::Line(Line { a: Vec3::new(0.0, y, 0.0), b: Vec3::new(10.0, y, 0.0) })).unwrap()
}

/// Ground has a red line at y=0 and a green one on layer HIDE; First (active) has a blue line at y=100.
fn two_levels() -> (Drawing, aad_doc::LevelId, aad_doc::LevelId) {
    let mut d = Drawing::new_metric();
    line(&mut d, 0.0, "WALL", 1);
    line(&mut d, 1.0, "HIDE", 3);
    let g = d.materialize_levels("Ground", 0.0, 3000.0).unwrap();
    let f = d.add_level("First", 3000.0, 3000.0, None).unwrap();
    d.switch_level(f).unwrap();
    line(&mut d, 100.0, "WALL", 5);
    (d, g, f)
}

fn colours(l: &aad_render::DisplayList) -> Vec<Rgb> {
    l.prims.iter().map(|p| p.display_rgb(BG)).collect()
}

#[test]
fn without_traces_the_list_is_exactly_the_plain_build() {
    let (d, _, _) = two_levels();
    let plain = build(&d, &Space::Model, &Options::default());
    let with = build_with_traces(&d, &Options::default(), BG);
    assert_eq!((plain.prims.len(), plain.verts.len()), (with.prims.len(), with.verts.len()));

    let single = Drawing::new_metric();
    assert_eq!(build_with_traces(&single, &Options::default(), BG).prims.len(), 0, "a drawing without levels is unchanged");
}

#[test]
fn a_trace_is_grey_faded_thin_and_never_selectable() {
    let (mut d, g, f) = two_levels();
    let mut t = Trace::new(g);
    t.fade = 50;
    d.set_trace(f, t).unwrap();
    let l = build_with_traces(&d, &Options::default(), BG);
    assert_eq!(l.prims.len(), 3, "two traced lines and the active one");
    let traced: Vec<_> = l.prims.iter().filter(|p| p.handle == VIEWPORT_CONTENT).collect();
    assert_eq!(traced.len(), 2, "trace primitives carry the non-selectable handle");
    for p in &traced {
        let c = p.display_rgb(BG);
        assert!(c.0 == c.1 && c.1 == c.2, "grey: {c:?}");
        assert!(c.0 < 255 && c.0 > 0, "faded towards the black background: {c:?}");
        assert_eq!(p.lw, 0.0);
        assert!(!p.aci7);
    }
    let active = l.prims.iter().find(|p| p.handle != VIEWPORT_CONTENT).unwrap();
    assert_eq!(active.display_rgb(BG), Rgb(0, 0, 255), "the level being edited keeps its colour");
}

#[test]
fn true_colour_keeps_the_hue_and_more_fade_is_closer_to_the_background() {
    let (mut d, g, f) = two_levels();
    let mut t = Trace::new(g);
    t.colour = TraceColour::TrueColour;
    t.fade = 0;
    d.set_trace(f, t.clone()).unwrap();
    let l = build_with_traces(&d, &Options::default(), BG);
    assert!(colours(&l).contains(&Rgb(255, 0, 0)), "fade 0 keeps the true colour");
    t.fade = 90;
    d.set_trace(f, t).unwrap();
    let l = build_with_traces(&d, &Options::default(), BG);
    let red = l.prims.iter().find(|p| p.handle == VIEWPORT_CONTENT && p.display_rgb(BG).0 > p.display_rgb(BG).1).unwrap();
    assert!(red.display_rgb(BG).0 < 40, "90 percent fade is nearly the background: {:?}", red.display_rgb(BG));
}

#[test]
fn order_decides_what_is_drawn_first_and_hidden_layers_stay_hidden() {
    let (mut d, g, f) = two_levels();
    d.layers.iter_mut().find(|l| l.name == "HIDE").unwrap().on = false;
    let mut below = Trace::new(g);
    below.order = TraceOrder::Below;
    d.set_trace(f, below.clone()).unwrap();
    let l = build_with_traces(&d, &Options::default(), BG);
    assert_eq!(l.prims.len(), 2, "the object on the hidden layer is not traced");
    assert_eq!(l.prims[0].handle, VIEWPORT_CONTENT);
    assert_ne!(l.prims[1].handle, VIEWPORT_CONTENT);

    below.order = TraceOrder::Above;
    d.set_trace(f, below).unwrap();
    let l = build_with_traces(&d, &Options::default(), BG);
    assert_ne!(l.prims[0].handle, VIEWPORT_CONTENT);
    assert_eq!(l.prims[1].handle, VIEWPORT_CONTENT);
}

#[test]
fn a_trace_layer_list_limits_what_is_shown_and_the_bounds_include_the_trace() {
    let (mut d, g, f) = two_levels();
    let mut t = Trace::new(g);
    t.layers = Some(vec!["wall".into()]);
    d.set_trace(f, t).unwrap();
    let l = build_with_traces(&d, &Options::default(), BG);
    assert_eq!(l.prims.len(), 2, "only the WALL line of the traced level (names match ignoring case)");
    let plain = build(&d, &Space::Model, &Options::default());
    assert!(l.bounds.min.y < plain.bounds.min.y, "zoom extents include the visible trace");
    // Vertex references stay valid after merging.
    for p in &l.prims {
        assert!(!l.points(p).is_empty());
    }
}

#[test]
fn several_traces_merge_in_order_with_valid_vertex_references() {
    let (mut d, g, f) = two_levels();
    let r = d.add_level("Roof", 6000.0, 0.0, None).unwrap();
    d.switch_level(r).unwrap();
    line(&mut d, 200.0, "WALL", 2);
    d.switch_level(f).unwrap();
    let mut a = Trace::new(g);
    a.order = TraceOrder::Below;
    let mut b = Trace::new(r);
    b.order = TraceOrder::Above;
    d.set_trace(f, a).unwrap();
    d.set_trace(f, b).unwrap();
    let l = build_with_traces(&d, &Options::default(), BG);
    assert_eq!(l.prims.len(), 4);
    assert!(l.prims.iter().all(|p| p.kind == Kind::Polyline && l.points(p).len() == 2));
    let ys: Vec<f64> = l.prims.iter().map(|p| l.points(p)[0].y).collect();
    assert_eq!(ys, [0.0, 1.0, 100.0, 200.0], "below traces, the level, then above traces");
}
