//! Animation editors: time slider, graph editor, dope sheet, script editor,
//! command line and floating windows.

use crate::anim::*;
use crate::app::*;
use crate::icons::*;
use crate::math::*;
use crate::mesh::*;
use crate::panels::*;
use crate::scene::*;
use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};
use std::collections::HashSet;

pub const CURVE_COLORS: [Color32; CHANNELS] = [
    Color32::from_rgb(235, 80, 80),
    Color32::from_rgb(110, 220, 90),
    Color32::from_rgb(90, 140, 255),
    Color32::from_rgb(255, 140, 140),
    Color32::from_rgb(170, 240, 150),
    Color32::from_rgb(150, 185, 255),
    Color32::from_rgb(190, 60, 60),
    Color32::from_rgb(80, 170, 70),
    Color32::from_rgb(70, 110, 210),
    Color32::from_rgb(240, 210, 90),
];

impl App {
    // ---------------- time slider ----------------
    pub fn time_slider(&mut self, ui: &mut Ui) {
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            let ctl_w = 330.0;
            let w = (ui.available_width() - ctl_w).max(100.0);
            let (rect, resp) = ui.allocate_exact_size(vec2(w, 34.0), Sense::click_and_drag());
            let p = ui.painter_at(rect);
            p.rect_filled(rect, 0.0, Color32::from_gray(56));
            let (s, e) = (self.scene.start, self.scene.end.max(self.scene.start + 1.0));
            let fx = |f: f32| rect.left() + 8.0 + (f - s) / (e - s) * (rect.width() - 16.0);
            let step = nice_step(e - s, rect.width(), 50.0).max(1.0);
            if (rect.width() - 16.0) / (e - s) > 4.0 {
                let mut g = s;
                while g <= e {
                    p.line_segment([pos2(fx(g), rect.bottom() - 5.0), pos2(fx(g), rect.bottom())], Stroke::new(1.0, Color32::from_gray(110)));
                    g += 1.0;
                }
            }
            let mut f = (s / step).ceil() * step;
            while f <= e {
                let x = fx(f);
                p.line_segment([pos2(x, rect.bottom() - 12.0), pos2(x, rect.bottom())], Stroke::new(1.0, Color32::from_gray(150)));
                p.text(pos2(x + 2.0, rect.top() + 3.0), egui::Align2::LEFT_TOP, format!("{}", f as i64), egui::FontId::proportional(11.0), Color32::from_gray(190));
                f += step;
            }
            for k in self.scene.key_frames(&self.sel) {
                if k >= s && k <= e {
                    let x = fx(k);
                    p.rect_filled(Rect::from_min_max(pos2(x - 1.5, rect.top() + 14.0), pos2(x + 1.5, rect.bottom())), 0.0, Color32::from_rgb(220, 50, 50));
                }
            }
            // notetrack events
            for n in &self.scene.notes {
                if n.frame >= s && n.frame <= e {
                    let x = fx(n.frame);
                    let col = Color32::from_rgb(90, 200, 255);
                    p.add(egui::Shape::convex_polygon(vec![pos2(x - 4.0, rect.bottom()), pos2(x + 4.0, rect.bottom()), pos2(x, rect.bottom() - 7.0)], col, Stroke::NONE));
                    p.text(pos2(x + 3.0, rect.bottom() - 8.0), egui::Align2::LEFT_BOTTOM, &n.name, egui::FontId::proportional(9.0), col);
                }
            }
            let cx = fx(self.scene.frame);
            p.rect_filled(Rect::from_min_max(pos2(cx - 4.0, rect.top()), pos2(cx + 4.0, rect.bottom())), 0.0, Color32::from_rgba_unmultiplied(210, 210, 210, 120));
            p.text(pos2(cx + 6.0, rect.top() + 15.0), egui::Align2::LEFT_TOP, format!("{}", self.scene.frame as i64), egui::FontId::proportional(12.0), Color32::WHITE);
            if resp.dragged() || resp.clicked() {
                if let Some(m) = resp.interact_pointer_pos() {
                    let f = s + (m.x - rect.left() - 8.0) / (rect.width() - 16.0) * (e - s);
                    let f = f.round().clamp(s, e);
                    if f != self.scene.frame {
                        self.set_frame(f);
                    }
                }
            }
            resp.context_menu(|ui| {
                if ui.button(crate::tr!("Schlüssel setzen", "Set key")).clicked() {
                    self.key_selection(0..9);
                    ui.close();
                }
                if ui.button(crate::tr!("Schlüssel hier löschen", "Delete keys here")).clicked() {
                    self.delete_keys_at_frame();
                    ui.close();
                }
            });

            let mut fr = self.scene.frame;
            if ui.add(egui::DragValue::new(&mut fr).speed(0.5).range(-10000.0..=100000.0).max_decimals(0)).changed() {
                self.set_frame(fr);
            }
            if icon_button(ui, Icon::ToStart, 24.0, false, crate::tr!("Zum Anfang", "Go to start")).clicked() {
                self.set_frame(self.scene.start);
            }
            if icon_button(ui, Icon::PrevKey, 24.0, false, crate::tr!("Vorheriger Schlüssel (,)", "Previous key (,)")).clicked() {
                self.step_key(-1.0);
            }
            if icon_button(ui, if self.playing { Icon::Pause } else { Icon::Play }, 24.0, self.playing, crate::tr!("Abspielen / Stopp (Alt+V)", "Play / stop (Alt+V)")).clicked() {
                self.playing = !self.playing;
            }
            if icon_button(ui, Icon::NextKey, 24.0, false, crate::tr!("Nächster Schlüssel (.)", "Next key (.)")).clicked() {
                self.step_key(1.0);
            }
            if icon_button(ui, Icon::ToEnd, 24.0, false, crate::tr!("Zum Ende", "Go to end")).clicked() {
                self.set_frame(self.scene.end);
            }
            let key_col = if self.auto_key { Color32::from_rgb(200, 50, 50) } else { Color32::from_gray(90) };
            if ui
                .add(egui::Button::new(egui::RichText::new("Auto-Key").color(Color32::WHITE)).fill(key_col))
                .on_hover_text(crate::tr!("Bewegte Kanäle automatisch verschlüsseln", "Key changed channels automatically"))
                .clicked()
            {
                self.auto_key = !self.auto_key;
            }
        });
        ui.horizontal(|ui| {
            ui.label(crate::tr!("Bereich", "Range"));
            let mut s = self.scene.start;
            let mut e = self.scene.end;
            ui.add(egui::DragValue::new(&mut s).speed(0.5).max_decimals(0));
            ui.add(egui::DragValue::new(&mut e).speed(0.5).max_decimals(0));
            if e < s + 1.0 {
                e = s + 1.0;
            }
            self.scene.start = s.round();
            self.scene.end = e.round();
            ui.separator();
            ui.label(crate::tr!("Bilder/s", "FPS"));
            egui::ComboBox::from_id_salt("fps").width(60.0).selected_text(format!("{}", self.scene.fps)).show_ui(ui, |ui| {
                for f in [12.0, 24.0, 25.0, 30.0, 48.0, 60.0] {
                    ui.selectable_value(&mut self.scene.fps, f, format!("{f}"));
                }
            });
            ui.checkbox(&mut self.loop_play, crate::tr!("Schleife", "Loop"));
            ui.separator();
            ui.label(crate::tr!("Tangente", "Tangent"));
            egui::ComboBox::from_id_salt("tan").width(80.0).selected_text(self.default_tangent.label()).show_ui(ui, |ui| {
                for t in Tangent::ALL {
                    ui.selectable_value(&mut self.default_tangent, t, t.label());
                }
            });
            ui.separator();
            let secs = (self.scene.frame - self.scene.start) / self.scene.fps;
            ui.label(egui::RichText::new(format!("{:02}:{:05.2}", (secs / 60.0) as i64, secs % 60.0)).monospace());
        });
    }

    // ---------------- bottom editors ----------------
    pub fn bottom_editors(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.bottom_tab, BottomTab::Graph, crate::tr!("Kurven-Editor", "Graph Editor"));
            ui.selectable_value(&mut self.bottom_tab, BottomTab::Dope, crate::tr!("Dope Sheet", "Dope Sheet"));
            ui.selectable_value(&mut self.bottom_tab, BottomTab::Script, crate::tr!("Skript-Editor", "Script Editor"));
            if self.bottom_tab != BottomTab::Script {
                ui.separator();
                ui.label(crate::tr!("Tangente:", "Tangent:"));
                for t in Tangent::ALL {
                    if ui.button(t.label()).on_hover_text(crate::tr!("Tangente der gewählten Schlüssel", "Tangent of the selected keys")).clicked() {
                        self.set_tangent_selected(t);
                    }
                }
                ui.separator();
                if ui.button(crate::tr!("Einpassen (F)", "Frame all (F)")).clicked() {
                    self.graph.fit_request = true;
                }
                if ui.button(crate::tr!("Schlüssel löschen", "Delete keys")).clicked() {
                    self.delete_selected_keys();
                }
            }
        });
        ui.separator();
        self.graph_hovered = false;
        match self.bottom_tab {
            BottomTab::Graph => self.graph_editor(ui),
            BottomTab::Dope => self.dope_sheet(ui),
            BottomTab::Script => self.script_editor(ui),
        }
    }

    fn set_tangent_selected(&mut self, t: Tangent) {
        if self.graph.sel.is_empty() {
            self.status = crate::tr!("Erst Schlüssel im Kurven-Editor auswählen.", "Select keys in the graph editor first.").into();
            return;
        }
        self.checkpoint();
        for k in self.graph.sel.clone() {
            if let Some(n) = self.scene.get_mut(k.node) {
                if let Some(key) = n.curves[k.ch].keys.get_mut(k.idx) {
                    key.tangent = t;
                }
            }
        }
        let f = self.scene.frame;
        self.scene.evaluate(f);
    }

    fn delete_selected_keys(&mut self) {
        if self.graph.sel.is_empty() {
            return;
        }
        self.checkpoint();
        let mut sel = self.graph.sel.clone();
        sel.sort_by(|a, b| b.idx.cmp(&a.idx));
        for k in sel {
            if let Some(n) = self.scene.get_mut(k.node) {
                if k.idx < n.curves[k.ch].keys.len() {
                    n.curves[k.ch].keys.remove(k.idx);
                }
            }
        }
        self.graph.sel.clear();
        let f = self.scene.frame;
        self.scene.evaluate(f);
    }

    fn key_value(&self, k: &KeyRef) -> Option<(f32, f32)> {
        self.scene.get(k.node).and_then(|n| n.curves[k.ch].keys.get(k.idx)).map(|key| (key.frame, key.value))
    }

    fn apply_key_drag(&mut self, kd: &KeyDrag, df: f32, dv: f32) {
        let selset: HashSet<KeyRef> = kd.starts.iter().map(|s| s.0).collect();
        for (k, f0, v0) in &kd.starts {
            let Some(n) = self.scene.get_mut(k.node) else { continue };
            let keys = &mut n.curves[k.ch].keys;
            if k.idx >= keys.len() {
                continue;
            }
            let mut nf = (f0 + df).round();
            if k.idx > 0 && !selset.contains(&KeyRef { idx: k.idx - 1, ..*k }) {
                nf = nf.max(keys[k.idx - 1].frame + 1.0);
            }
            if k.idx + 1 < keys.len() && !selset.contains(&KeyRef { idx: k.idx + 1, ..*k }) {
                nf = nf.min(keys[k.idx + 1].frame - 1.0);
            }
            keys[k.idx].frame = nf;
            keys[k.idx].value = v0 + dv;
        }
        let f = self.scene.frame;
        self.scene.evaluate(f);
    }

    fn finish_key_drag(&mut self) {
        let sel: Vec<(KeyRef, f32)> = self.graph.sel.iter().filter_map(|k| self.key_value(k).map(|(f, _)| (*k, f))).collect();
        let nodes: HashSet<Id> = sel.iter().map(|s| s.0.node).collect();
        for id in nodes {
            if let Some(n) = self.scene.get_mut(id) {
                for c in &mut n.curves {
                    c.sort();
                }
            }
        }
        self.graph.sel = sel
            .into_iter()
            .filter_map(|(k, f)| {
                let n = self.scene.get(k.node)?;
                let idx = n.curves[k.ch].keys.iter().position(|x| (x.frame - f).abs() < 1e-3)?;
                Some(KeyRef { idx, ..k })
            })
            .collect();
        let f = self.scene.frame;
        self.scene.evaluate(f);
    }

    fn graph_editor(&mut self, ui: &mut Ui) {
        let avail = ui.available_rect_before_wrap();
        let list_w = 170.0;
        let list_rect = Rect::from_min_size(avail.min, vec2(list_w, avail.height()));
        let rect = Rect::from_min_max(pos2(avail.left() + list_w + 4.0, avail.top()), avail.max);
        let nodes: Vec<Id> = self.sel.clone();
        ui.scope_builder(egui::UiBuilder::new().max_rect(list_rect), |ui| {
            egui::ScrollArea::vertical().id_salt("glist").auto_shrink([false, false]).show(ui, |ui| {
                if nodes.is_empty() {
                    ui.label(egui::RichText::new(crate::tr!("Objekt wählen, um seine Kurven zu sehen.", "Select an object to see its curves.")).color(Color32::from_gray(160)));
                }
                for id in &nodes {
                    let Some(n) = self.scene.get(*id) else { continue };
                    ui.label(egui::RichText::new(&n.name).strong());
                    let animated: Vec<usize> = (0..CHANNELS).filter(|c| !n.curves[*c].is_empty()).collect();
                    if animated.is_empty() {
                        ui.label(egui::RichText::new(crate::tr!("  (keine Animation)", "  (no animation)")).small().color(Color32::from_gray(150)));
                    }
                    for c in animated {
                        ui.horizontal(|ui| {
                            ui.add_space(6.0);
                            let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                            ui.painter().rect_filled(r, 2.0, CURVE_COLORS[c]);
                            ui.checkbox(&mut self.graph.show[c], channel_name(c));
                        });
                    }
                }
            });
        });

        let resp = ui.interact(rect, ui.scope_id().with("graph"), Sense::click_and_drag());
        self.graph_hovered = resp.hovered();
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 0.0, Color32::from_gray(50));

        let mut curves: Vec<(Id, usize)> = Vec::new();
        for id in &nodes {
            if let Some(n) = self.scene.get(*id) {
                for c in 0..CHANNELS {
                    if self.graph.show[c] && !n.curves[c].is_empty() {
                        curves.push((*id, c));
                    }
                }
            }
        }
        let fit_key = resp.hovered() && ui.input(|i| i.key_pressed(egui::Key::F)) && !ui.ctx().egui_wants_keyboard_input();
        if self.graph.fit_request || fit_key {
            self.graph.fit_request = false;
            let (mut f0, mut f1, mut v0, mut v1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
            for (id, c) in &curves {
                let cv = &self.scene.get(*id).unwrap().curves[*c];
                if let Some((lo, hi)) = cv.value_range() {
                    v0 = v0.min(lo);
                    v1 = v1.max(hi);
                }
                f0 = f0.min(cv.keys[0].frame);
                f1 = f1.max(cv.keys[cv.keys.len() - 1].frame);
            }
            if f0 > f1 {
                f0 = self.scene.start;
                f1 = self.scene.end;
                v0 = -1.0;
                v1 = 1.0;
            }
            let pf = ((f1 - f0) * 0.06).max(2.0);
            let pv = ((v1 - v0) * 0.12).max(0.5);
            self.graph.f0 = f0 - pf;
            self.graph.f1 = f1 + pf;
            self.graph.v0 = v0 - pv;
            self.graph.v1 = v1 + pv;
        }
        let (f0, f1, v0, v1) = (self.graph.f0, self.graph.f1, self.graph.v0, self.graph.v1);
        let to_x = |f: f32| rect.left() + (f - f0) / (f1 - f0) * rect.width();
        let to_y = |v: f32| rect.bottom() - (v - v0) / (v1 - v0) * rect.height();
        let from_x = |x: f32| f0 + (x - rect.left()) / rect.width() * (f1 - f0);
        let from_y = |y: f32| v0 + (rect.bottom() - y) / rect.height() * (v1 - v0);

        let fs = nice_step(f1 - f0, rect.width(), 70.0);
        let mut f = (f0 / fs).ceil() * fs;
        while f <= f1 {
            let x = to_x(f);
            p.line_segment([pos2(x, rect.top()), pos2(x, rect.bottom())], Stroke::new(1.0, Color32::from_gray(62)));
            p.text(pos2(x + 2.0, rect.bottom() - 2.0), egui::Align2::LEFT_BOTTOM, format!("{}", f as i64), egui::FontId::proportional(10.0), Color32::from_gray(150));
            f += fs;
        }
        let vs = nice_step(v1 - v0, rect.height(), 40.0);
        let mut v = (v0 / vs).ceil() * vs;
        while v <= v1 {
            let y = to_y(v);
            let col = if v.abs() < vs * 0.01 { Color32::from_gray(95) } else { Color32::from_gray(62) };
            p.line_segment([pos2(rect.left(), y), pos2(rect.right(), y)], Stroke::new(1.0, col));
            p.text(pos2(rect.left() + 2.0, y - 1.0), egui::Align2::LEFT_BOTTOM, format!("{v:.2}"), egui::FontId::proportional(10.0), Color32::from_gray(150));
            v += vs;
        }
        let cx = to_x(self.scene.frame);
        p.line_segment([pos2(cx, rect.top()), pos2(cx, rect.bottom())], Stroke::new(1.5, Color32::from_rgb(220, 180, 60)));

        let mut key_pts: Vec<(KeyRef, Pos2)> = Vec::new();
        for (id, c) in &curves {
            let cv = &self.scene.get(*id).unwrap().curves[*c];
            let col = CURVE_COLORS[*c];
            let steps = (rect.width() / 2.0) as usize;
            let pts: Vec<Pos2> = (0..=steps)
                .filter_map(|i| {
                    let x = rect.left() + i as f32 * 2.0;
                    cv.eval(from_x(x)).map(|val| pos2(x, to_y(val)))
                })
                .collect();
            p.add(egui::Shape::line(pts, Stroke::new(1.6, col)));
            for (i, k) in cv.keys.iter().enumerate() {
                let kr = KeyRef { node: *id, ch: *c, idx: i };
                let kp = pos2(to_x(k.frame), to_y(k.value));
                let sel = self.graph.sel.contains(&kr);
                let r = Rect::from_center_size(kp, vec2(6.0, 6.0));
                p.rect_filled(r, 0.0, if sel { Color32::YELLOW } else { Color32::BLACK });
                p.rect_stroke(r, 0.0, Stroke::new(1.0, col), egui::StrokeKind::Middle);
                key_pts.push((kr, kp));
            }
        }
        if curves.is_empty() {
            p.text(rect.center(), egui::Align2::CENTER_CENTER, crate::tr!("Keine animierten Kanäle – Objekt wählen und S drücken", "No animated channels – select an object and press S"), egui::FontId::proportional(14.0), Color32::from_gray(150));
        }

        let (alt, shift) = ui.input(|i| (i.modifiers.alt, i.modifiers.shift));
        let delta = ui.input(|i| i.pointer.delta());
        if resp.dragged_by(egui::PointerButton::Middle) || (alt && resp.dragged_by(egui::PointerButton::Primary)) {
            let df = -delta.x / rect.width() * (f1 - f0);
            let dv = delta.y / rect.height() * (v1 - v0);
            self.graph.f0 += df;
            self.graph.f1 += df;
            self.graph.v0 += dv;
            self.graph.v1 += dv;
        }
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                if let Some(m) = resp.hover_pos() {
                    let z = (-scroll * 0.002).exp();
                    let (mf, mv) = (from_x(m.x), from_y(m.y));
                    let ctrl = ui.input(|i| i.modifiers.command);
                    self.graph.f0 = mf + (self.graph.f0 - mf) * z;
                    self.graph.f1 = mf + (self.graph.f1 - mf) * z;
                    if !ctrl {
                        self.graph.v0 = mv + (self.graph.v0 - mv) * z;
                        self.graph.v1 = mv + (self.graph.v1 - mv) * z;
                    }
                }
            }
            if ui.input(|i| i.key_pressed(egui::Key::Delete)) && !ui.ctx().egui_wants_keyboard_input() {
                self.delete_selected_keys();
            }
        }
        let hit_key = |m: Pos2| key_pts.iter().find(|(_, kp)| (*kp - m).length() < 7.0).map(|(k, _)| *k);
        if !alt && resp.drag_started_by(egui::PointerButton::Primary) {
            let start = ui.input(|i| i.pointer.press_origin()).unwrap_or_default();
            if let Some(k) = hit_key(start) {
                if !self.graph.sel.contains(&k) {
                    if !shift {
                        self.graph.sel.clear();
                    }
                    self.graph.sel.push(k);
                }
                self.checkpoint();
                let starts = self.graph.sel.iter().filter_map(|k| self.key_value(k).map(|(f, v)| (*k, f, v))).collect();
                self.graph.drag = Some(KeyDrag { start, starts, boxsel: false });
            } else {
                self.graph.drag = Some(KeyDrag { start, starts: Vec::new(), boxsel: true });
            }
        }
        if let Some(kd) = self.graph.drag.clone() {
            let m = ui.input(|i| i.pointer.hover_pos()).unwrap_or(kd.start);
            if kd.boxsel {
                p.rect_stroke(Rect::from_two_pos(kd.start, m), 0.0, Stroke::new(1.0, Color32::WHITE), egui::StrokeKind::Inside);
            } else {
                let df = (m.x - kd.start.x) / rect.width() * (f1 - f0);
                let dv = -(m.y - kd.start.y) / rect.height() * (v1 - v0);
                self.apply_key_drag(&kd, df, dv);
            }
            if !ui.input(|i| i.pointer.primary_down()) {
                if kd.boxsel {
                    let r = Rect::from_two_pos(kd.start, m);
                    if !shift {
                        self.graph.sel.clear();
                    }
                    for (k, kp) in &key_pts {
                        if r.contains(*kp) && !self.graph.sel.contains(k) {
                            self.graph.sel.push(*k);
                        }
                    }
                } else {
                    self.finish_key_drag();
                }
                self.graph.drag = None;
            }
        }
        if resp.clicked() && !alt {
            if let Some(m) = resp.interact_pointer_pos() {
                match hit_key(m) {
                    Some(k) => {
                        if shift {
                            if self.graph.sel.contains(&k) {
                                self.graph.sel.retain(|x| *x != k);
                            } else {
                                self.graph.sel.push(k);
                            }
                        } else {
                            self.graph.sel = vec![k];
                        }
                    }
                    None => {
                        if !shift {
                            self.graph.sel.clear();
                        }
                    }
                }
            }
        }
        if let Some(k) = self.graph.sel.first().copied() {
            if let Some(key) = self.scene.get(k.node).and_then(|n| n.curves[k.ch].keys.get(k.idx)).copied() {
                let txt = crate::trf!("Bild {}  Wert {:.3}  Tangente {}  ({} gewählt)", "Frame {}  Value {:.3}  Tangent {}  ({} selected)", key.frame as i64, key.value, key.tangent.label(), self.graph.sel.len());
                p.text(rect.right_top() + vec2(-8.0, 6.0), egui::Align2::RIGHT_TOP, txt, egui::FontId::proportional(12.0), Color32::from_gray(220));
            }
        }
    }

    fn dope_sheet(&mut self, ui: &mut Ui) {
        let avail = ui.available_rect_before_wrap();
        let label_w = 170.0;
        let rect = Rect::from_min_max(pos2(avail.left() + label_w, avail.top()), avail.max);
        let resp = ui.interact(rect, ui.scope_id().with("dope"), Sense::click_and_drag());
        self.graph_hovered = resp.hovered();
        let p = ui.painter_at(avail);
        p.rect_filled(avail, 0.0, Color32::from_gray(50));
        let (s, e) = (self.scene.start, self.scene.end.max(self.scene.start + 1.0));
        let to_x = |f: f32| rect.left() + 6.0 + (f - s) / (e - s) * (rect.width() - 12.0);
        let px_per_frame = (rect.width() - 12.0) / (e - s);
        let mut rows: Vec<(String, Id, Option<usize>)> = Vec::new();
        for id in &self.sel {
            if let Some(n) = self.scene.get(*id) {
                rows.push((n.name.clone(), *id, None));
                for c in 0..CHANNELS {
                    if !n.curves[c].is_empty() {
                        rows.push((format!("   {}", channel_name(c)), *id, Some(c)));
                    }
                }
            }
        }
        let row_h = 18.0;
        let fs = nice_step(e - s, rect.width(), 50.0).max(1.0);
        let mut f = (s / fs).ceil() * fs;
        while f <= e {
            let x = to_x(f);
            p.line_segment([pos2(x, avail.top()), pos2(x, avail.bottom())], Stroke::new(1.0, Color32::from_gray(62)));
            p.text(pos2(x + 2.0, avail.top() + 1.0), egui::Align2::LEFT_TOP, format!("{}", f as i64), egui::FontId::proportional(10.0), Color32::from_gray(150));
            f += fs;
        }
        let cx = to_x(self.scene.frame);
        p.line_segment([pos2(cx, avail.top()), pos2(cx, avail.bottom())], Stroke::new(1.5, Color32::from_rgb(220, 180, 60)));
        let mut key_pts: Vec<(Vec<KeyRef>, Pos2)> = Vec::new();
        for (ri, (label, id, ch)) in rows.iter().enumerate() {
            let y = avail.top() + 16.0 + ri as f32 * row_h;
            if y > avail.bottom() {
                break;
            }
            let row_rect = Rect::from_min_size(pos2(avail.left(), y), vec2(avail.width(), row_h));
            if ri % 2 == 0 {
                p.rect_filled(row_rect, 0.0, Color32::from_gray(56));
            }
            let bold = ch.is_none();
            p.text(
                pos2(avail.left() + 6.0, y + row_h * 0.5),
                egui::Align2::LEFT_CENTER,
                label,
                egui::FontId::proportional(if bold { 13.0 } else { 12.0 }),
                if bold { Color32::WHITE } else { Color32::from_gray(200) },
            );
            let Some(n) = self.scene.get(*id) else { continue };
            match ch {
                Some(c) => {
                    for (i, k) in n.curves[*c].keys.iter().enumerate() {
                        let kr = KeyRef { node: *id, ch: *c, idx: i };
                        let kp = pos2(to_x(k.frame), y + row_h * 0.5);
                        let sel = self.graph.sel.contains(&kr);
                        let sz = (px_per_frame * 0.8).clamp(4.0, 9.0);
                        p.rect_filled(Rect::from_center_size(kp, vec2(sz, 11.0)), 1.0, if sel { Color32::YELLOW } else { CURVE_COLORS[*c] });
                        key_pts.push((vec![kr], kp));
                    }
                }
                None => {
                    for f in self.scene.key_frames(&[*id]) {
                        let refs: Vec<KeyRef> = (0..CHANNELS).filter_map(|c| n.curves[c].key_index_at(f).map(|idx| KeyRef { node: *id, ch: c, idx })).collect();
                        let kp = pos2(to_x(f), y + row_h * 0.5);
                        let sel = !refs.is_empty() && refs.iter().all(|r| self.graph.sel.contains(r));
                        p.rect_filled(Rect::from_center_size(kp, vec2(7.0, 11.0)), 1.0, if sel { Color32::YELLOW } else { Color32::from_gray(210) });
                        key_pts.push((refs, kp));
                    }
                }
            }
        }
        if rows.is_empty() {
            p.text(avail.center(), egui::Align2::CENTER_CENTER, crate::tr!("Objekte auswählen, um ihre Schlüssel zu sehen", "Select objects to see their keys"), egui::FontId::proportional(14.0), Color32::from_gray(150));
        }
        let shift = ui.input(|i| i.modifiers.shift);
        let hit = |m: Pos2| key_pts.iter().find(|(_, kp)| (kp.x - m.x).abs() < 6.0 && (kp.y - m.y).abs() < 7.0).map(|(k, _)| k.clone());
        if resp.drag_started_by(egui::PointerButton::Primary) {
            let start = ui.input(|i| i.pointer.press_origin()).unwrap_or_default();
            if let Some(ks) = hit(start) {
                if !ks.iter().all(|k| self.graph.sel.contains(k)) {
                    if !shift {
                        self.graph.sel.clear();
                    }
                    self.graph.sel.extend(ks);
                }
                self.checkpoint();
                let starts = self.graph.sel.iter().filter_map(|k| self.key_value(k).map(|(f, v)| (*k, f, v))).collect();
                self.dope.drag = Some(KeyDrag { start, starts, boxsel: false });
            } else {
                self.dope.drag = Some(KeyDrag { start, starts: Vec::new(), boxsel: true });
            }
        }
        if let Some(kd) = self.dope.drag.clone() {
            let m = ui.input(|i| i.pointer.hover_pos()).unwrap_or(kd.start);
            if kd.boxsel {
                p.rect_stroke(Rect::from_two_pos(kd.start, m), 0.0, Stroke::new(1.0, Color32::WHITE), egui::StrokeKind::Inside);
            } else {
                let df = ((m.x - kd.start.x) / px_per_frame).round();
                self.apply_key_drag(&kd, df, 0.0);
            }
            if !ui.input(|i| i.pointer.primary_down()) {
                if kd.boxsel {
                    let r = Rect::from_two_pos(kd.start, m);
                    if !shift {
                        self.graph.sel.clear();
                    }
                    for (ks, kp) in &key_pts {
                        if r.contains(*kp) {
                            for k in ks {
                                if !self.graph.sel.contains(k) {
                                    self.graph.sel.push(*k);
                                }
                            }
                        }
                    }
                } else {
                    self.finish_key_drag();
                }
                self.dope.drag = None;
            }
        }
        if resp.clicked() {
            if let Some(m) = resp.interact_pointer_pos() {
                match hit(m) {
                    Some(ks) => {
                        if !shift {
                            self.graph.sel.clear();
                        }
                        self.graph.sel.extend(ks);
                    }
                    None => {
                        if !shift {
                            self.graph.sel.clear();
                        }
                        let f = s + (m.x - rect.left() - 6.0) / (rect.width() - 12.0) * (e - s);
                        self.set_frame(f.clamp(s, e));
                    }
                }
            }
        }
        if resp.hovered() && ui.input(|i| i.key_pressed(egui::Key::Delete)) && !ui.ctx().egui_wants_keyboard_input() {
            self.delete_selected_keys();
        }
    }

    fn script_editor(&mut self, ui: &mut Ui) {
        let avail = ui.available_size();
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(avail.x * 0.48);
                ui.label(egui::RichText::new(crate::tr!("Ausgabe", "Output")).small());
                egui::ScrollArea::vertical().id_salt("log").stick_to_bottom(true).max_height((avail.y - 24.0).max(40.0)).show(ui, |ui| {
                    for l in &self.log {
                        ui.label(egui::RichText::new(l).monospace().size(12.0));
                    }
                });
            });
            ui.separator();
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(crate::tr!("Skript", "Script")).small());
                    if ui.button(crate::tr!("▶ Ausführen", "▶ Run")).clicked() {
                        let script = self.script.clone();
                        for line in script.lines() {
                            let line = line.trim();
                            if line.is_empty() || line.starts_with('#') {
                                continue;
                            }
                            self.exec(line);
                        }
                    }
                    if ui.button(crate::tr!("Ausgabe leeren", "Clear output")).clicked() {
                        self.log.clear();
                    }
                });
                ui.add_sized(vec2(ui.available_width(), (avail.y - 30.0).max(40.0)), egui::TextEdit::multiline(&mut self.script).code_editor());
            });
        });
    }

    pub fn command_line(&mut self, ui: &mut Ui) {
        ui.horizontal_centered(|ui| {
            ui.label(egui::RichText::new(crate::tr!("Befehl", "Command")).small());
            let r = ui.add(egui::TextEdit::singleline(&mut self.cmd).desired_width(240.0).hint_text(crate::tr!("z. B. kugel, bewegen 0 2 0, key, hilfe", "e.g. sphere, move 0 2 0, key, help")));
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                let c = std::mem::take(&mut self.cmd);
                if !c.trim().is_empty() {
                    self.exec(c.trim());
                }
                r.request_focus();
            }
            if let Some(last) = self.log.last() {
                ui.label(egui::RichText::new(last).monospace().size(11.0).color(Color32::from_gray(170)));
            }
            ui.separator();
            ui.label(egui::RichText::new(&self.status).size(12.0).color(Color32::from_gray(200)));
        });
    }

    /// Runs one command of the small built-in command language.
    pub fn exec(&mut self, line: &str) {
        self.log.push(format!("> {line}"));
        let mut parts = line.split_whitespace();
        let cmd = parts.next().unwrap_or("").to_lowercase();
        let args: Vec<String> = parts.map(|s| s.to_string()).collect();
        let nums: Vec<f32> = args.iter().filter_map(|a| a.replace(',', ".").parse().ok()).collect();
        let v3a = |d: f32| v3(*nums.first().unwrap_or(&d), *nums.get(1).unwrap_or(&d), *nums.get(2).unwrap_or(&d));
        let prim = match cmd.as_str() {
            "wuerfel" | "würfel" | "cube" => Some(Prim::Cube),
            "kugel" | "sphere" => Some(Prim::Sphere),
            "zylinder" | "cylinder" => Some(Prim::Cylinder),
            "kegel" | "cone" => Some(Prim::Cone),
            "ebene" | "plane" => Some(Prim::Plane),
            "torus" => Some(Prim::Torus),
            _ => None,
        };
        if let Some(p) = prim {
            self.create_prim(p);
            let msg = self.status.clone();
            self.log.push(msg);
            return;
        }
        let msg: String = match cmd.as_str() {
            "licht" | "light" => {
                self.create_light(LightKind::Directional);
                crate::tr!("Licht erzeugt", "Light created").into()
            }
            "kamera" | "camera" => {
                self.create_node(Kind::Camera);
                crate::tr!("Kamera erzeugt", "Camera created").into()
            }
            "gelenk" | "joint" => {
                self.checkpoint();
                let parent = self.lead().filter(|l| self.scene.get(*l).is_some_and(|n| n.kind == Kind::Joint));
                let name = self.scene.unique_name(crate::tr!("gelenk", "joint"));
                let mut n = Node::new(0, &name, Kind::Joint);
                n.parent = parent;
                n.t = v3a(0.0);
                let id = self.scene.add(n);
                self.sel = vec![id];
                crate::trf!("{name} erzeugt", "{name} created")
            }
            "bewegen" | "move" | "drehen" | "rotate" | "skalieren" | "scale" => {
                if self.sel.is_empty() {
                    crate::tr!("Nichts ausgewählt", "Nothing selected").into()
                } else {
                    self.checkpoint();
                    let v = v3a(if cmd.starts_with('s') { 1.0 } else { 0.0 });
                    for &id in &self.sel.clone() {
                        if let Some(n) = self.scene.get_mut(id) {
                            match cmd.as_str() {
                                "bewegen" | "move" => n.t = v,
                                "drehen" | "rotate" => n.r = v,
                                _ => n.s = v,
                            }
                        }
                    }
                    self.auto_key_selection();
                    format!("{cmd}: {} {} {}", v.x, v.y, v.z)
                }
            }
            "key" | "schluessel" | "schlüssel" => {
                self.key_selection(0..9);
                self.status.clone()
            }
            "bild" | "frame" => {
                let f = *nums.first().unwrap_or(&self.scene.frame);
                self.set_frame(f);
                crate::trf!("Bild {}", "Frame {}", f as i64)
            }
            "bereich" | "range" => {
                if nums.len() >= 2 {
                    self.scene.start = nums[0];
                    self.scene.end = nums[1].max(nums[0] + 1.0);
                }
                crate::trf!("Bereich {} – {}", "Range {} – {}", self.scene.start, self.scene.end)
            }
            "abspielen" | "play" => {
                self.playing = true;
                crate::tr!("Abspielen", "Play").into()
            }
            "stopp" | "stop" => {
                self.playing = false;
                crate::tr!("Gestoppt", "Stopped").into()
            }
            "auswahl" | "select" => {
                self.sel = args.iter().filter_map(|a| self.scene.nodes.iter().find(|n| &n.name == a).map(|n| n.id)).collect();
                crate::trf!("{} ausgewählt", "{} selected", self.sel.len())
            }
            "loeschen" | "löschen" | "delete" => {
                self.delete_selection();
                self.status.clone()
            }
            "duplizieren" | "duplicate" => {
                self.duplicate_selection();
                self.status.clone()
            }
            "verknuepfen" | "verknüpfen" | "parent" => {
                self.parent_selection();
                self.status.clone()
            }
            "ik" => {
                self.create_ik();
                self.status.clone()
            }
            "binden" | "bind" => {
                self.bind_skin();
                self.status.clone()
            }
            "glaetten" | "glätten" | "smooth" => {
                self.smooth_mesh();
                self.status.clone()
            }
            "farbe" | "color" => {
                let c = v3a(0.6);
                for &id in &self.sel.clone() {
                    if let Some(n) = self.scene.get_mut(id) {
                        n.material.color = [c.x, c.y, c.z];
                    }
                }
                crate::tr!("Farbe gesetzt", "Color set").into()
            }
            "rendern" | "render" => {
                self.pending_render = true;
                crate::tr!("Rendere …", "Rendering …").into()
            }
            "liste" | "list" | "ls" => self.scene.nodes.iter().map(|n| n.name.clone()).collect::<Vec<_>>().join(", "),
            "hilfe" | "help" => crate::tr!("Befehle: wuerfel kugel zylinder kegel ebene torus licht kamera gelenk [x y z] | bewegen/drehen/skalieren x y z | key | bild n | bereich a b | abspielen | stopp | auswahl name… | loeschen | duplizieren | verknuepfen | ik | binden | glaetten | farbe r g b | rendern | liste", "Commands: cube sphere cylinder cone plane torus light camera joint [x y z] | move/rotate/scale x y z | key | frame n | range a b | play | stop | select name… | delete | duplicate | parent | ik | bind | smooth | color r g b | render | list").into(),
            _ => crate::trf!("Unbekannter Befehl '{cmd}' – 'hilfe' zeigt alle Befehle", "Unknown command '{cmd}' – 'help' lists all commands"),
        };
        self.log.push(msg);
    }

    // ---------------- windows ----------------
    pub fn windows(&mut self, ctx: &egui::Context) {
        if self.pending_render {
            self.pending_render = false;
            self.render_frame(ctx);
        }
        let mut show = self.render.show;
        let mut do_render = false;
        egui::Window::new(crate::tr!("Render-Ansicht", "Render View")).open(&mut show).default_size([720.0, 480.0]).show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button(crate::tr!("Rendern", "Render")).clicked() {
                    do_render = true;
                }
                if ui.add_enabled(self.render.last.is_some(), egui::Button::new(crate::tr!("Bild speichern …", "Save image …"))).clicked() {
                    self.save_render();
                }
                if ui.add_enabled(self.render.seq.is_none(), egui::Button::new(crate::tr!("Sequenz rendern …", "Render sequence …"))).clicked() {
                    self.start_sequence();
                }
                if self.render.seq.is_some() && ui.button(crate::tr!("Abbrechen", "Cancel")).clicked() {
                    self.render.seq = None;
                    self.render.message = crate::tr!("Sequenz abgebrochen", "Sequence cancelled").into();
                }
            });
            ui.horizontal(|ui| {
                ui.label(crate::tr!("Größe", "Size"));
                ui.add(egui::DragValue::new(&mut self.render.width).range(16..=4096));
                ui.label("×");
                ui.add(egui::DragValue::new(&mut self.render.height).range(16..=4096));
                ui.checkbox(&mut self.render.antialias, crate::tr!("Kantenglättung", "Anti-aliasing"));
            });
            ui.horizontal(|ui| {
                ui.label(crate::tr!("Kamera", "Camera"));
                let cams: Vec<(Id, String)> = self.scene.nodes.iter().filter(|n| n.kind == Kind::Camera).map(|n| (n.id, n.name.clone())).collect();
                let cur = self.render.camera.and_then(|c| cams.iter().find(|x| x.0 == c)).map(|x| x.1.clone()).unwrap_or(crate::tr!("Perspektive-Ansicht", "Perspective view").into());
                egui::ComboBox::from_id_salt("rcam").selected_text(cur).show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.render.camera, None, crate::tr!("Perspektive-Ansicht", "Perspective view"));
                    for (id, name) in &cams {
                        ui.selectable_value(&mut self.render.camera, Some(*id), name);
                    }
                });
            });
            ui.label(egui::RichText::new(&self.render.message).small());
            if let Some(tex) = &self.render.tex {
                let size = tex.size_vec2();
                let avail = ui.available_size();
                let scale = (avail.x / size.x).min(avail.y.max(100.0) / size.y).min(1.0);
                ui.image((tex.id(), size * scale));
            } else {
                ui.label(crate::tr!("Noch nichts gerendert.", "Nothing rendered yet."));
            }
        });
        self.render.show = show;
        if do_render {
            self.render_frame(ctx);
        }

        let mut help = self.show_help;
        egui::Window::new(crate::tr!("Tastenkürzel", "Keyboard shortcuts")).open(&mut help).default_width(420.0).show(ctx, |ui| {
            egui::Grid::new("keys").striped(true).show(ui, |ui| {
                for (k, d) in [
                    (crate::tr!("Alt + linke Maus / rechte Maus", "Alt + left mouse / right mouse"), crate::tr!("Ansicht drehen", "Orbit view")),
                    (crate::tr!("Mittlere Maus", "Middle mouse"), crate::tr!("Ansicht verschieben", "Pan view")),
                    (crate::tr!("Mausrad / Alt + rechte Maus", "Wheel / Alt + right mouse"), crate::tr!("Zoomen", "Zoom")),
                    ("Q / W / E / R", crate::tr!("Auswählen / Bewegen / Drehen / Skalieren", "Select / move / rotate / scale")),
                    (crate::tr!("X gedrückt halten", "Hold X"), crate::tr!("Am Raster einrasten", "Snap to grid")),
                    ("S", crate::tr!("Schlüssel auf alle Kanäle", "Key all channels")),
                    (crate::tr!("Umschalt + W / E / R", "Shift + W / E / R"), crate::tr!("Schlüssel nur Position / Drehung / Skalierung", "Key only translate / rotate / scale")),
                    (", / .", crate::tr!("Vorheriger / nächster Schlüssel", "Previous / next key")),
                    ("Alt + , / Alt + .", crate::tr!("Ein Bild zurück / vor", "One frame back / forward")),
                    ("Alt + V", crate::tr!("Abspielen / Stopp", "Play / stop")),
                    ("F / A", crate::tr!("Auswahl / alles einpassen", "Frame selection / all")),
                    (crate::tr!("Leertaste", "Space"), crate::tr!("Eine / vier Ansichten", "One / four views")),
                    ("4 / 5 / 6", crate::tr!("Drahtgitter / schattiert / beides", "Wireframe / shaded / both")),
                    ("7", crate::tr!("Szenenlichter an/aus", "Scene lights on/off")),
                    ("1 / 3", crate::tr!("Glättungsvorschau aus / an", "Smooth preview off / on")),
                    ("F8 / F11", crate::tr!("Punkte / Flächen bearbeiten", "Edit vertices / faces")),
                    (crate::tr!("Strg + E", "Ctrl + E"), crate::tr!("Flächen extrudieren", "Extrude faces")),
                    (crate::tr!("P / Umschalt + P", "P / Shift + P"), crate::tr!("Verknüpfen / lösen", "Parent / unparent")),
                    (crate::tr!("Strg + G / Strg + D", "Ctrl + G / Ctrl + D"), crate::tr!("Gruppieren / duplizieren", "Group / duplicate")),
                    (crate::tr!("Strg + Z / Strg + Y", "Ctrl + Z / Ctrl + Y"), crate::tr!("Rückgängig / wiederholen", "Undo / redo")),
                    (crate::tr!("Strg + S / Strg + O", "Ctrl + S / Ctrl + O"), crate::tr!("Speichern / öffnen", "Save / open")),
                    (crate::tr!("Entf", "Del"), crate::tr!("Löschen", "Delete")),
                    ("Enter / Esc", crate::tr!("Gelenkkette beenden", "Finish joint chain")),
                ] {
                    ui.label(egui::RichText::new(k).monospace());
                    ui.label(d);
                    ui.end_row();
                }
            });
        });
        self.show_help = help;
    }
}
