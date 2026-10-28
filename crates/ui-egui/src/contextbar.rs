//! The context bar: a thin row above the drawing, in both Classic and Pro, with the three things
//! that say what the next object will be and what can be changed: the current layer, the current
//! floor and the active drawing space. Everything here is a view of engine state and acts through
//! commands (`layer.current`, `floor.current`, `space.*`), so the rules (a locked or frozen layer is
//! not offered as current, a space never moves an object) live in the engine, not here.
//! See docs/ACTIVE_DRAWING_SPACE.md.

use aad_color::Color;
use egui::{Color32, RichText, Sense, vec2};
use serde_json::{Value, json};

use crate::CadApp;
use crate::theme::Tokens;

/// One drawing space as the bar shows it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SpaceRow {
    pub id: String,
    pub name: String,
    pub floor: Option<String>,
    pub layers: usize,
}

#[derive(Default)]
pub struct State {
    /// (document uid, revision) the cached lists below were read for.
    key: Option<(u64, u64)>,
    pub active: Option<String>,
    pub dirty: bool,
    pub spaces: Vec<SpaceRow>,
    pub floors: Vec<String>,
    pub floor: Option<String>,
    pub search: String,
    pub all_floors: bool,
}

enum Action {
    SetLayer(String),
    SetFloor(String),
    TogglePin,
}

fn rgb32(c: Color) -> Color32 {
    let rgb = c.resolve(Color::Index(7), Color::Index(7));
    Color32::from_rgb(rgb.0, rgb.1, rgb.2)
}

fn swatch(ui: &mut egui::Ui, c: Color32) {
    let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
    ui.painter().rect_filled(r, 1.0, c);
    ui.painter().rect_stroke(r, 1.0, egui::Stroke::new(1.0, Color32::from_gray(90)), egui::StrokeKind::Inside);
}

/// Re-read the lists when the document or its revision changed (the lists live in the drawing).
pub fn refresh(app: &mut CadApp, force: bool) {
    let Ok(st) = app.session.state() else { return };
    let key = (st.uid, st.revision);
    if !force && app.ctx_bar.key == Some(key) {
        return;
    }
    app.ctx_bar.key = Some(key);
    let list = app.run("space.list", json!({})).unwrap_or(Value::Null);
    let status = app.run("space.status", json!({})).unwrap_or(Value::Null);
    let spaces: Vec<SpaceRow> = list["spaces"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|s| SpaceRow {
                    id: s["id"].as_str().unwrap_or("").to_string(),
                    name: s["name"].as_str().unwrap_or("").to_string(),
                    floor: s["floor"].as_str().map(str::to_string),
                    layers: s["layers"].as_u64().unwrap_or(0) as usize,
                })
                .collect()
        })
        .unwrap_or_default();
    let s = &mut app.ctx_bar;
    s.active = status["active"]["id"].as_str().map(str::to_string);
    s.dirty = status["dirty"].as_bool().unwrap_or(false);
    s.spaces = spaces;
    refresh_floors(app);
}

fn refresh_floors(app: &mut CadApp) {
    let v = app.run("floor.current", json!({})).unwrap_or(Value::Null);
    let names: Vec<String> = match v["mode"].as_str() {
        Some("project") => {
            v["floors"].as_array().map(|a| a.iter().filter_map(|f| f["name"].as_str().map(str::to_string)).collect()).unwrap_or_default()
        }
        _ => v["floors"].as_array().map(|a| a.iter().filter_map(|f| f.as_str().map(str::to_string)).collect()).unwrap_or_default(),
    };
    app.ctx_bar.floors = names;
    app.ctx_bar.floor = v["floor"].as_str().filter(|f| !f.is_empty()).map(str::to_string);
}

struct LayerRow {
    name: String,
    color: Color,
    on: bool,
    frozen: bool,
    locked: bool,
}

pub fn show(app: &mut CadApp, ui: &mut egui::Ui) {
    let t = Tokens::get();
    refresh(app, false);
    let mut actions: Vec<Action> = Vec::new();
    egui::Panel::top("aad_context_bar").exact_size(30.0).frame(egui::Frame::NONE.fill(t.chrome)).show(ui, |ui| {
        ui.horizontal_centered(|ui| {
            ui.add_space(8.0);
            layer_selector(app, ui, &mut actions);
            ui.separator();
            floor_selector(app, ui, &mut actions);
            // Panels: pinned (always shown) or auto-hide (collapse when the pointer is away).
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(8.0);
                let pinned = app.ui.pin_panels;
                let label = if pinned { crate::tl!("Panels: pinned") } else { crate::tl!("Panels: auto-hide") };
                let tip = if pinned {
                    crate::tl!("The side panels stay open. Click to let them collapse when the pointer is away")
                } else {
                    crate::tl!("The side panels collapse when the pointer is away; the edge strip brings them back. Click to pin them open")
                };
                let pin = ui.add(egui::Button::new(RichText::new(label).small()).selected(!pinned)).on_hover_text(tip);
                crate::control::record_widget(ui, &pin, "Panels pin", "button", Some(pinned));
                if pin.clicked() {
                    actions.push(Action::TogglePin);
                }
            });
        });
    });
    for a in actions {
        run(app, a);
    }
}

fn layer_selector(app: &mut CadApp, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
    let Ok(d) = app.session.doc() else { return };
    let cur = d.header.str("CLAYER", "0");
    let rows: Vec<LayerRow> =
        d.layers.iter().map(|l| LayerRow { name: l.name.clone(), color: l.color, on: l.on, frozen: l.frozen, locked: l.locked }).collect();
    let current = rows.iter().find(|r| r.name.eq_ignore_ascii_case(&cur));
    ui.label(RichText::new(crate::tl!("Layer")).weak().small());
    swatch(ui, current.map_or(Color32::GRAY, |r| rgb32(r.color)));
    let resp =
        ui.add(egui::Button::new(cur.clone()).min_size(vec2(150.0, 0.0))).on_hover_text(crate::tl!("Current layer: new objects are drawn on it"));
    if let Some(r) = current {
        let badge = if r.frozen {
            Some(crate::tl!("frozen"))
        } else if r.locked {
            Some(crate::tl!("locked"))
        } else if !r.on {
            Some(crate::tl!("hidden"))
        } else {
            None
        };
        if let Some(b) = badge {
            ui.label(RichText::new(b).small().color(Color32::from_rgb(220, 120, 60)));
        }
    }
    crate::control::record_widget(ui, &resp, "Layer selector", "button", None);
    let opened = resp.clicked();
    egui::Popup::from_toggle_button_response(&resp).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
        ui.set_min_width(240.0);
        let edit = ui.add(egui::TextEdit::singleline(&mut app.ctx_bar.search).hint_text(crate::tl!("Search layers")).desired_width(220.0));
        if opened {
            edit.request_focus();
        }
        let q = app.ctx_bar.search.to_lowercase();
        egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
            for r in rows.iter().filter(|r| q.is_empty() || r.name.to_lowercase().contains(&q)) {
                let usable = !r.frozen && !r.locked;
                ui.horizontal(|ui| {
                    swatch(ui, rgb32(r.color));
                    let resp = ui.add_enabled_ui(usable, |ui| ui.selectable_label(r.name.eq_ignore_ascii_case(&cur), r.name.as_str())).inner;
                    if usable && resp.clicked() {
                        actions.push(Action::SetLayer(r.name.clone()));
                        ui.close();
                    } else if !usable {
                        resp.on_disabled_hover_text(if r.frozen {
                            crate::tl!("Frozen: thaw it first")
                        } else {
                            crate::tl!("Locked: unlock it first")
                        });
                    }
                    for (on, text) in [(r.frozen, crate::tl!("frozen")), (r.locked, crate::tl!("locked")), (!r.on, crate::tl!("hidden"))] {
                        if on {
                            ui.label(RichText::new(text).small().weak());
                        }
                    }
                });
            }
        });
    });
}

fn floor_selector(app: &mut CadApp, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
    ui.label(RichText::new(crate::tl!("Floor")).weak().small());
    let floors = app.ctx_bar.floors.clone();
    let shown = app.ctx_bar.floor.clone().unwrap_or_else(|| "—".to_string());
    ui.add_enabled_ui(!floors.is_empty(), |ui| {
        egui::ComboBox::from_id_salt("ctx_floor").width(110.0).selected_text(shown.clone()).show_ui(ui, |ui| {
            for f in &floors {
                if ui.selectable_label(*f == shown, f.as_str()).clicked() {
                    actions.push(Action::SetFloor(f.clone()));
                }
            }
        });
    })
    .response
    .on_hover_text(if floors.is_empty() {
        crate::tl!("No floors defined: Architecture > Project > Floors")
    } else {
        crate::tl!("Current floor: choosing it moves no object")
    });
}

fn say(app: &mut CadApp, r: Result<Value, String>) -> Option<Value> {
    match r {
        Ok(v) => {
            let mut msg = v["message"].as_str().unwrap_or("").to_string();
            if let Some(w) = v["warnings"].as_array() {
                for w in w.iter().filter_map(Value::as_str) {
                    msg.push_str(" — ");
                    msg.push_str(w);
                }
            }
            if !msg.is_empty() {
                app.set_status(msg);
            }
            Some(v)
        }
        Err(e) => {
            app.set_status(e);
            None
        }
    }
}

fn run(app: &mut CadApp, a: Action) {
    match a {
        Action::SetLayer(name) => {
            let r = app.run("layer.current", json!({"name": name}));
            say(app, r);
        }
        Action::SetFloor(name) => {
            let r = app.run("floor.current", json!({"name": name}));
            say(app, r);
            refresh(app, true);
        }
        Action::TogglePin => {
            let _ = app.run("ui.toggle.pinpanels", json!({}));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb32_resolves_index_colours() {
        assert_eq!(rgb32(Color::Index(1)), Color32::from_rgb(255, 0, 0));
    }
}
