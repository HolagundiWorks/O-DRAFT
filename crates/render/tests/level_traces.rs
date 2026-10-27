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

/// Cost of three traces of 25 000 objects each (plan budget). Run with
/// `cargo test --release -p aad-render --test level_traces -- --ignored --nocapture`.
#[test]
#[ignore = "timing probe"]
fn probe_three_traces_of_25000_objects() {
    use std::time::Instant;
    let mut d = Drawing::new_metric();
    let mut ids = Vec::new();
    for lv in 0..4 {
        let id = if lv == 0 { d.materialize_levels("L0", 0.0, 0.0).unwrap() } else { d.add_level(&format!("L{lv}"), 0.0, 0.0, None).unwrap() };
        d.switch_level(id).unwrap();
        for i in 0..25_000 {
            line(&mut d, f64::from(i) * 0.01 + f64::from(lv), "WALL", 7);
        }
        ids.push(id);
    }
    d.switch_level(ids[3]).unwrap();
    for id in &ids[..3] {
        d.set_trace(ids[3], Trace::new(*id)).unwrap();
    }
    let opts = Options::default();
    let t = Instant::now();
    let plain = build(&d, &Space::Model, &opts);
    let t_plain = t.elapsed();
    let t = Instant::now();
    let with = build_with_traces(&d, &opts, BG);
    let t_with = t.elapsed();
    eprintln!("plain {:?} ({} prims); with 3 traces {:?} ({} prims)", t_plain, plain.prims.len(), t_with, with.prims.len());
}

#[test]
fn a_sheet_view_draws_the_level_faded_under_its_annotation() {
    let (mut d, _g, f) = two_levels();
    let set = d.add_sheet_set("Electrical", "E-").unwrap();
    d.switch_level(f).unwrap();
    d.enter_sheet_view(set).unwrap();
    line(&mut d, 300.0, "E-ANNO", 1);
    let l = build_with_traces(&d, &Options::default(), BG);
    assert_eq!(l.prims.len(), 2, "the level's one line for reference and the annotation");
    assert_eq!(l.prims[0].handle, VIEWPORT_CONTENT, "the reference is drawn first and is not selectable");
    assert_ne!(l.prims[1].handle, VIEWPORT_CONTENT);
    assert_eq!(l.prims[1].display_rgb(BG), Rgb(255, 0, 0), "annotation keeps its colour");
    let c = l.prims[0].display_rgb(BG);
    assert!(c.2 > c.0 && c.2 < 255, "the level keeps its hue (blue) but is lightly faded: {c:?}");
}

#[test]
fn design_mode_after_a_sheet_view_is_the_plain_list_again() {
    let (mut d, _g, f) = two_levels();
    let set = d.add_sheet_set("Electrical", "E-").unwrap();
    d.switch_level(f).unwrap();
    d.enter_sheet_view(set).unwrap();
    d.leave_sheet_view().unwrap();
    let plain = build(&d, &Space::Model, &Options::default());
    assert_eq!(build_with_traces(&d, &Options::default(), BG).prims.len(), plain.prims.len());
}

// ---------------- layout viewports bound to a level and a sheet set ----------------

fn add_viewport(d: &mut Drawing, centre_x: f64, view_y: f64, bind: impl FnOnce(&mut aad_doc::Viewport)) {
    // Paper-space viewport 50 x 50 at `centre_x`, showing a 10-unit-high window around (5, view_y): scale 5.
    let mut vp = aad_doc::Viewport {
        center: Vec3::new(centre_x, 25.0, 0.0),
        width: 50.0,
        height: 50.0,
        view_center: aad_geom::Vec2::new(5.0, view_y),
        view_height: 10.0,
        id: 2,
        ..Default::default()
    };
    bind(&mut vp);
    let h = d.new_handle();
    d.layouts[0].entities.push(aad_doc::Entity { handle: h, common: Common::default(), kind: EntityKind::Viewport(vp) });
}

/// Content primitives (model geometry seen through a viewport) whose first point is in `[x0, x1]`.
fn content_in(l: &aad_render::DisplayList, x0: f64, x1: f64) -> Vec<aad_render::DPrim> {
    l.prims.iter().filter(|p| p.handle == VIEWPORT_CONTENT && l.points(p).first().is_some_and(|q| q.x >= x0 && q.x <= x1)).copied().collect()
}

/// Ground has a red wall at y=0, First a blue wall at y=5, and Electrical an annotation line (green) on First.
fn sheet_layout() -> (Drawing, aad_doc::LevelId, aad_doc::LevelId, aad_doc::SheetSetId) {
    let (mut d, g, f) = two_levels(); // Ground: red at y=0 and a green one on HIDE at y=1; First: blue at y=100
    let set = d.add_sheet_set("Electrical", "E-").unwrap();
    d.enter_sheet_view(set).unwrap();
    line(&mut d, 100.5, "E-ANNO", 3);
    d.leave_sheet_view().unwrap();
    (d, g, f, set)
}

#[test]
fn viewports_show_their_own_level_and_sheet_set_side_by_side() {
    let (mut d, _g, f, set) = sheet_layout();
    d.layers.iter_mut().find(|l| l.name == "HIDE").unwrap().on = false; // keeps Ground to its one red wall
    add_viewport(&mut d, 25.0, 0.0, |_| {}); // unbound: the first level (Ground)
    add_viewport(&mut d, 125.0, 100.0, |v| {
        v.level = Some(f);
        v.sheet_set = Some(set);
    });
    add_viewport(&mut d, 225.0, 100.0, |v| v.level = Some(f)); // First, plain geometry
    let l = build(&d, &Space::Paper("Layout1".into()), &Options::default());
    let a = content_in(&l, 0.0, 50.0);
    let b = content_in(&l, 100.0, 150.0);
    let c = content_in(&l, 200.0, 250.0);
    assert_eq!(a.len(), 1, "an unbound viewport shows the first level");
    assert_eq!(a[0].display_rgb(BG), Rgb(255, 0, 0));
    assert_eq!(b.len(), 2, "First plus its Electrical annotation");
    assert_eq!(c.len(), 1, "First without a sheet set is plain geometry");
    assert_eq!(c[0].display_rgb(BG), Rgb(0, 0, 255));
    assert!(b.iter().any(|p| p.display_rgb(BG) == Rgb(0, 255, 0)), "the annotation line is drawn");
}

#[test]
fn an_unbound_viewport_keeps_showing_the_first_level_whatever_tab_is_active() {
    let (mut d, _g, f, _set) = sheet_layout();
    d.layers.iter_mut().find(|l| l.name == "HIDE").unwrap().on = false;
    add_viewport(&mut d, 25.0, 0.0, |_| {});
    d.switch_level(f).unwrap();
    let on_first = build(&d, &Space::Paper("Layout1".into()), &Options::default());
    assert_eq!(content_in(&on_first, 0.0, 50.0).len(), 1);
    assert_eq!(content_in(&on_first, 0.0, 50.0)[0].display_rgb(BG), Rgb(255, 0, 0), "still Ground's red wall");
}

#[test]
fn a_viewport_trace_is_drawn_faded_towards_white_paper_and_follows_its_order() {
    let (mut d, g, f, set) = sheet_layout();
    d.layers.iter_mut().find(|l| l.name == "HIDE").unwrap().on = false;
    let mut t = Trace::new(g);
    t.fade = 50;
    add_viewport(&mut d, 125.0, 100.0, |v| {
        v.level = Some(f);
        v.sheet_set = Some(set);
        v.traces = vec![t];
    });
    // A tall window so both levels are in view.
    let mut wide = d.clone();
    for e in wide.layouts[0].entities.iter().map(|e| e.handle).collect::<Vec<_>>() {
        wide.layouts[0].entities.modify(e, |x| {
            if let EntityKind::Viewport(v) = &mut x.kind {
                v.view_height = 400.0;
                v.view_center = aad_geom::Vec2::new(5.0, 50.0);
                v.height = 50.0;
            }
        });
    }
    let l = build(&wide, &Space::Paper("Layout1".into()), &Options::default());
    let all = content_in(&l, 100.0, 150.0);
    assert_eq!(all.len(), 3, "the trace's wall, First's wall and the annotation");
    let traced = all[0];
    let col = traced.display_rgb(BG);
    assert!(col.0 == col.1 && col.1 == col.2, "grey: {col:?}");
    assert!(col.0 > 100 && col.0 < 255, "faded towards white paper, not black: {col:?}");
    assert_eq!(traced.lw, 0.0);
    assert_eq!(all[1].display_rgb(BG), Rgb(0, 0, 255), "a Below trace is drawn first, the level over it");
}

#[test]
fn changing_a_viewport_binding_changes_what_the_cached_tessellation_shows() {
    let (mut d, _g, f, set) = sheet_layout();
    d.layers.iter_mut().find(|l| l.name == "HIDE").unwrap().on = false;
    add_viewport(&mut d, 125.0, 100.0, |v| v.level = Some(f));
    let before = content_in(&build(&d, &Space::Paper("Layout1".into()), &Options::default()), 100.0, 150.0).len();
    let h = d.layouts[0].entities.iter().next().unwrap().handle;
    d.layouts[0].entities.modify(h, |x| {
        if let EntityKind::Viewport(v) = &mut x.kind {
            v.sheet_set = Some(set);
        }
    });
    let after = content_in(&build(&d, &Space::Paper("Layout1".into()), &Options::default()), 100.0, 150.0).len();
    assert_eq!((before, after), (1, 2), "binding the sheet set adds its annotation");
}
