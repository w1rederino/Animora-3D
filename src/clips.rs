//! Animation library: every imported animation becomes a clip. Clicking a
//! clip puts its keys on the models in the scene; edits are kept per clip.

use crate::anim::*;
use crate::app::*;
use crate::scene::*;
use egui::{Color32, Sense, Ui, vec2};
use std::path::PathBuf;

/// Model files are imported before animation files.
pub fn is_model_file(p: &std::path::Path) -> bool {
    matches!(p.extension().map(|e| e.to_string_lossy().to_lowercase()).as_deref(), Some("xmodel_export"))
}

pub fn is_anim_file(p: &std::path::Path) -> bool {
    matches!(p.extension().map(|e| e.to_string_lossy().to_lowercase()).as_deref(), Some("xanim_export"))
}

#[derive(Default)]
pub struct ClipUi {
    pub rename: Option<(usize, String)>,
}

impl App {
    /// Nodes whose keys belong to clips: every object in the scene
    /// (joints of imported models as well as animated objects).
    fn clip_nodes(&self) -> Vec<Id> {
        self.scene.nodes.iter().map(|n| n.id).collect()
    }

    /// Animation already in the scene (opened file, example scene, keys set
    /// by hand) that no clip holds yet becomes a clip of its own, so the
    /// list always shows what is on the models.
    pub fn ensure_scene_clip(&mut self) {
        if self.scene.active_clip.is_some_and(|i| i < self.scene.clips.len()) {
            return;
        }
        if !self.scene.nodes.iter().any(|n| n.is_animated()) {
            self.scene.active_clip = None;
            return;
        }
        let base = self
            .file
            .as_ref()
            .and_then(|f| f.file_stem())
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| crate::tr!("Szenen-Animation", "Scene animation").to_string());
        let source = self.file.as_ref().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
        self.clip_from_scene(&base, &source);
    }

    /// Stores the current keys (incl. edits) into the active clip.
    pub fn clip_capture(&mut self) {
        let Some(i) = self.scene.active_clip.filter(|i| *i < self.scene.clips.len()) else { return };
        let curves: Vec<(Id, Vec<Curve>)> = self
            .clip_nodes()
            .into_iter()
            .filter_map(|id| self.scene.get(id).filter(|n| n.is_animated()).map(|n| (id, n.curves.clone())))
            .collect();
        let c = &mut self.scene.clips[i];
        c.curves = curves;
        c.notes = self.scene.notes.clone();
        c.fps = self.scene.fps;
        c.start = self.scene.start;
        c.end = self.scene.end;
    }

    /// Takes all keys off the scene (joints go back to their bind pose).
    fn clip_clear_scene(&mut self) {
        for id in self.clip_nodes() {
            if let Some(n) = self.scene.get_mut(id) {
                if let Some(rest) = n.rest {
                    n.t = rest[0];
                    n.r = rest[1];
                    n.s = rest[2];
                }
                for c in 0..CHANNELS.min(n.curves.len()) {
                    n.curves[c] = Curve::default();
                }
            }
        }
        self.scene.notes.clear();
    }

    /// Puts a clip's keys on the models (joints go back to their bind pose first).
    fn clip_load(&mut self, i: usize) {
        let Some(clip) = self.scene.clips.get(i).cloned() else { return };
        self.clip_clear_scene();
        for (id, curves) in &clip.curves {
            if let Some(n) = self.scene.get_mut(*id) {
                let mut cs = curves.clone();
                cs.resize(CHANNELS, Curve::default());
                n.curves = cs;
            }
        }
        self.scene.notes = clip.notes.clone();
        if clip.fps > 0.0 {
            self.scene.fps = clip.fps;
        }
        self.scene.start = clip.start;
        self.scene.end = clip.end.max(clip.start + 1.0);
        self.scene.active_clip = Some(i);
        self.set_frame(clip.start);
        self.graph.fit_request = true;
    }

    /// Switches to another clip, keeping the edits of the current one.
    pub fn clip_activate(&mut self, i: usize) {
        if i >= self.scene.clips.len() {
            return;
        }
        if self.scene.active_clip == Some(i) {
            return;
        }
        self.clip_capture();
        self.checkpoint();
        self.clip_load(i);
        let name = self.scene.clips[i].name.clone();
        self.status = crate::trf!("Animation „{name}“ aktiv", "Animation \"{name}\" active");
    }

    fn unique_clip_name(&self, base: &str) -> String {
        if !self.scene.clips.iter().any(|c| c.name == base) {
            return base.to_string();
        }
        (2..).map(|i| format!("{base}_{i}")).find(|n| !self.scene.clips.iter().any(|c| &c.name == n)).unwrap()
    }

    /// Turns what is on the models now into a new clip and makes it active.
    fn clip_from_scene(&mut self, name: &str, source: &str) -> usize {
        let name = self.unique_clip_name(name);
        self.scene.clips.push(AnimClip { name, source: source.to_string(), ..Default::default() });
        let i = self.scene.clips.len() - 1;
        self.scene.active_clip = Some(i);
        self.clip_capture();
        i
    }

    /// Imports any number of model and animation files: models first, then
    /// each animation becomes a clip in the library.
    pub fn import_files(&mut self, paths: Vec<PathBuf>) -> String {
        // (phase, path, is animation part): models first, then animations.
        // A .cast file can hold both and then appears in both phases.
        let mut tasks: Vec<(u8, PathBuf, bool)> = Vec::new();
        for p in paths {
            let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            if ext == "cast" {
                let (models, anims) = crate::cast_app::cast_kind(&p);
                if models {
                    tasks.push((0, p.clone(), false));
                }
                if anims {
                    tasks.push((1, p.clone(), true));
                }
                if !models && !anims {
                    tasks.push((2, p, false));
                }
            } else if is_model_file(&p) {
                tasks.push((0, p, false));
            } else if is_anim_file(&p) {
                tasks.push((1, p, true));
            } else {
                tasks.push((2, p, false));
            }
        }
        tasks.sort_by_key(|t| t.0);
        let mut ok = 0;
        let mut errors = Vec::new();
        let mut last = String::new();
        for (_, p, anim_part) in tasks {
            let file = p.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
            let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            let r = if anim_part {
                // what is on the models now becomes / stays a clip (with edits),
                // then the scene is cleared so the new clip holds only the new anim
                self.ensure_scene_clip();
                self.clip_capture();
                if self.scene.active_clip.is_some() {
                    self.clip_clear_scene();
                }

                let r = match ext.as_str() {
                    "cast" => self.cast_import_anim(&p),
                    _ => self.import_xanim(&p),
                };
                if r.is_ok() {
                    let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or("anim".into());
                    self.clip_from_scene(&stem, &p.to_string_lossy());
                } else if let Some(a) = self.scene.active_clip {
                    // failed: put the previous animation back on the models
                    self.scene.active_clip = None;
                    self.clip_load(a);
                }
                r
            } else {
                match ext.as_str() {
                    "xmodel_export" => self.import_xmodel(&p),
                    "cast" => self.cast_import_models(&p),
                    "anim3d" => {
                        self.open_path(&p);
                        Ok(self.status.clone())
                    }
                    "png" | "jpg" | "jpeg" | "tga" | "dds" | "bmp" => self.assign_texture(&p),
                    _ => Err(crate::trf!("Dateityp .{ext} wird nicht unterstützt", "File type .{ext} is not supported")),
                }
            };
            match r {
                Ok(m) => {
                    ok += 1;
                    last = m;
                }
                Err(e) => errors.push(format!("{file}: {e}")),
            }
        }
        let mut msg = if ok == 1 && errors.is_empty() { last } else { crate::trf!("{ok} Datei(en) importiert", "{ok} file(s) imported") };
        if !errors.is_empty() {
            for e in &errors {
                self.log.push(e.clone());
            }
            msg.push_str(&crate::trf!(" · {} fehlgeschlagen: {}", " · {} failed: {}", errors.len(), errors.join(" | ")));
        }
        msg
    }

    /// File dialog with multi-selection for animations (CoD and SE).
    pub fn import_anims_dialog(&mut self) {
        let Some(paths) = rfd::FileDialog::new()
            .add_filter(crate::tr!("Animationen", "Animations"), &["cast", "CAST", "xanim_export", "XANIM_EXPORT"])
            .pick_files()
        else {
            return;
        };
        let msg = self.import_files(paths);
        self.status = msg.clone();
        self.cod.message = msg;
    }

    pub fn clip_remove(&mut self, i: usize) {
        if i >= self.scene.clips.len() {
            return;
        }
        self.checkpoint();
        let was_active = self.scene.active_clip == Some(i);
        self.scene.clips.remove(i);
        self.scene.active_clip = match self.scene.active_clip {
            Some(a) if a == i => None,
            Some(a) if a > i => Some(a - 1),
            other => other,
        };
        if was_active {
            // the removed animation also leaves the models
            self.playing = false;
            self.clip_clear_scene();
            let f = self.scene.start;
            self.set_frame(f);
        }
    }

    pub fn clip_duplicate(&mut self, i: usize) {
        if i >= self.scene.clips.len() {
            return;
        }
        self.clip_capture();
        self.checkpoint();
        let mut c = self.scene.clips[i].clone();
        c.name = self.unique_clip_name(&crate::trf!("{}_kopie", "{}_copy", c.name));
        c.source.clear();
        self.scene.clips.insert(i + 1, c);
        if let Some(a) = self.scene.active_clip {
            if a > i {
                self.scene.active_clip = Some(a + 1);
            }
        }
        self.scene.active_clip = None;
        self.clip_load(i + 1);
    }

    /// The animation list (left panel, under the outliner).
    pub fn clips_panel(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.heading(crate::tr!("Animationen", "Animations"));
            ui.label(egui::RichText::new(format!("({})", self.scene.clips.len())).color(Color32::from_gray(160)));
        });
        if ui
            .button(crate::tr!("Animationen importieren …", "Import animations …"))
            .on_hover_text(crate::tr!(
                "Mehrere .cast / .xanim_export auf einmal wählen (Strg/Umschalt im Explorer)",
                "Pick several .cast / .xanim_export files at once (Ctrl/Shift in Explorer)"
            ))
            .clicked()
        {
            self.import_anims_dialog();
        }
        ui.separator();
        if self.scene.clips.is_empty() {
            ui.label(
                egui::RichText::new(crate::tr!(
                    "Noch keine Animationen. Erst die Modelle importieren, dann eine oder mehrere Animationen.",
                    "No animations yet. Import the models first, then one or more animations."
                ))
                .small()
                .color(Color32::from_gray(160)),
            );
            return;
        }
        let mut action: Option<(&'static str, usize)> = None;
        egui::ScrollArea::vertical().id_salt("clips").auto_shrink([false, false]).show(ui, |ui| {
            for i in 0..self.scene.clips.len() {
                let active = self.scene.active_clip == Some(i);
                let (name, frames, fps) = {
                    let c = &self.scene.clips[i];
                    (c.name.clone(), (c.end - c.start + 1.0).max(1.0) as i64, c.fps)
                };
                ui.horizontal(|ui| {
                    let playing_this = active && self.playing;
                    let tip = crate::tr!("Abspielen / anhalten", "Play / pause");
                    if crate::icons::icon_button(ui, if playing_this { crate::icons::Icon::Pause } else { crate::icons::Icon::Play }, 20.0, playing_this, tip).clicked() {
                        action = Some(("play", i));
                    }
                    let renaming = self.clip_ui.rename.as_ref().is_some_and(|(r, _)| *r == i);
                    if renaming {
                        let mut done = false;
                        if let Some((_, buf)) = &mut self.clip_ui.rename {
                            let r = ui.add(egui::TextEdit::singleline(buf).desired_width(ui.available_width() - 30.0));
                            r.request_focus();
                            done = r.lost_focus();
                        }
                        if done {
                            action = Some(("rename_done", i));
                        }
                    } else {
                        let w = ui.available_width() - 26.0;
                        let (rect, resp) = ui.allocate_exact_size(vec2(w.max(40.0), 20.0), Sense::click());
                        let bg = if active { Color32::from_rgb(82, 133, 166) } else if resp.hovered() { Color32::from_gray(80) } else { Color32::TRANSPARENT };
                        ui.painter().rect_filled(rect, 3.0, bg);
                        let painter = ui.painter_at(rect);
                        let count = format!("{frames} {}", crate::tr!("B.", "fr"));
                        painter.text(rect.right_center() - vec2(6.0, 0.0), egui::Align2::RIGHT_CENTER, &count, egui::FontId::proportional(11.0), Color32::from_gray(190));
                        let name_rect = egui::Rect::from_min_max(rect.min, egui::pos2(rect.right() - 44.0, rect.bottom()));
                        ui.painter_at(name_rect).text(rect.left_center() + vec2(6.0, 0.0), egui::Align2::LEFT_CENTER, &name, egui::FontId::proportional(12.5), Color32::from_gray(235));
                        let resp = resp.on_hover_text(crate::trf!(
                            "{name} · {frames} Bilder bei {fps} B/s\nKlick: auf die Modelle legen · Doppelklick: umbenennen · Rechtsklick: mehr",
                            "{name} · {frames} frames at {fps} fps\nClick: put on the models · double-click: rename · right-click: more"
                        ));
                        if resp.clicked() {
                            action = Some(("activate", i));
                        }
                        if resp.double_clicked() {
                            action = Some(("rename", i));
                        }
                        resp.context_menu(|ui| {
                            if ui.button(crate::tr!("Abspielen", "Play")).clicked() {
                                action = Some(("play", i));
                                ui.close();
                            }
                            if ui.button(crate::tr!("Umbenennen", "Rename")).clicked() {
                                action = Some(("rename", i));
                                ui.close();
                            }
                            if ui.button(crate::tr!("Duplizieren", "Duplicate")).clicked() {
                                action = Some(("duplicate", i));
                                ui.close();
                            }
                            if ui.button(crate::tr!("Als XAnim exportieren …", "Export as XAnim …")).clicked() {
                                action = Some(("export_x", i));
                                ui.close();
                            }
                            if ui.button(crate::tr!("Als Cast exportieren …", "Export as Cast …")).clicked() {
                                action = Some(("export_cast", i));
                                ui.close();
                            }
                            ui.separator();
                            if ui.button(crate::tr!("Aus der Liste entfernen", "Remove from list")).clicked() {
                                action = Some(("remove", i));
                                ui.close();
                            }
                        });
                    }
                    if ui.small_button("✕").on_hover_text(crate::tr!("Aus der Liste entfernen", "Remove from list")).clicked() {
                        action = Some(("remove", i));
                    }
                });
            }
        });
        if let Some((what, i)) = action {
            match what {
                "activate" => self.clip_activate(i),
                "play" => {
                    if self.scene.active_clip == Some(i) {
                        self.playing = !self.playing;
                    } else {
                        self.clip_activate(i);
                        self.playing = true;
                    }
                }
                "rename" => self.clip_ui.rename = Some((i, self.scene.clips[i].name.clone())),
                "rename_done" => {
                    if let Some((_, buf)) = self.clip_ui.rename.take() {
                        let n = buf.trim().to_string();
                        if !n.is_empty() {
                            self.checkpoint();
                            self.scene.clips[i].name = n;
                        }
                    }
                }
                "duplicate" => self.clip_duplicate(i),
                "remove" => self.clip_remove(i),
                "export_cast" => {
                    self.clip_activate(i);
                    self.cast_export_anim_dialog();
                }
                "export_x" => {
                    self.clip_activate(i);
                    self.open_cod(0);
                    self.cod.start = self.scene.start;
                    self.cod.end = self.scene.end;
                    self.cod.fps = self.scene.fps;
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::*;
    use crate::mesh::Mesh;

    #[test]
    fn existing_scene_animation_is_listed() {
        let ctx = egui::Context::default();
        // the example scene is animated: it is the first entry right away
        let mut a = App::new(&ctx, None);
        assert_eq!(a.scene.clips.len(), 1);
        assert_eq!(a.scene.active_clip, Some(0));
        let ball = a.scene.nodes.iter().find(|n| n.name == "ball").unwrap().id;
        let ball_keys = a.scene.get(ball).unwrap().curves[1].keys.len();
        assert!(ball_keys > 0);

        // a scene saved without a library (older file) gets an entry too
        let mut old = a.scene.clone();
        old.clips.clear();
        old.active_clip = None;
        let mut b = App::new(&ctx, None);
        b.scene = old;
        b.file = Some(PathBuf::from("C:/szenen/walk_cycle.anim3d"));
        b.ensure_scene_clip();
        assert_eq!(b.scene.clips.len(), 1);
        assert_eq!(b.scene.clips[0].name, "walk_cycle");

        // keys set by hand in an empty scene show up as well
        let mut c = App::new(&ctx, None);
        c.scene = Scene::default();
        c.ensure_scene_clip();
        assert!(c.scene.clips.is_empty());
        let cube = c.scene.add_prim(crate::mesh::Prim::Cube);
        c.sel = vec![cube];
        c.key_selection(0..9);
        c.ensure_scene_clip();
        assert_eq!(c.scene.clips.len(), 1);

        // removing the active entry also takes its keys off the scene
        c.clip_remove(0);
        c.ensure_scene_clip();
        assert!(c.scene.clips.is_empty());
        assert!(!c.scene.get(cube).unwrap().is_animated());

        // switching away from the example animation and back restores it
        let dir = std::env::temp_dir().join("animora_clips_test2");
        let _ = std::fs::create_dir_all(&dir);
        let sh = a.scene.nodes.iter().find(|n| n.name == "shoulder").unwrap().id;
        let arm = a.scene.nodes.iter().find(|n| n.name == "arm").unwrap().id;
        let model = dir.join("arm.xmodel_export");
        let anim = dir.join("wave.xanim_export");
        a.sel = vec![sh, arm];
        a.export_xmodel(&model).unwrap();
        a.sel = vec![sh];
        a.cod.start = 1.0;
        a.cod.end = 20.0;
        a.export_xanim(&anim).unwrap();
        a.import_files(vec![model, anim]);
        assert_eq!(a.scene.clips.len(), 2);
        assert_eq!(a.scene.clips[1].name, "wave");
        assert!(!a.scene.get(ball).unwrap().is_animated(), "new clip holds only the new animation");
        a.clip_activate(0);
        assert_eq!(a.scene.get(ball).unwrap().curves[1].keys.len(), ball_keys);
    }

    #[test]
    fn clip_library_switches_and_keeps_edits() {
        let dir = std::env::temp_dir().join("animora_clips_test");
        let _ = std::fs::create_dir_all(&dir);
        let ctx = egui::Context::default();
        let mut a = App::new(&ctx, None);
        let sh = a.scene.nodes.iter().find(|n| n.name == "shoulder").unwrap().id;
        let arm = a.scene.nodes.iter().find(|n| n.name == "arm").unwrap().id;
        let wr = a.scene.nodes.iter().find(|n| n.name == "wrist").unwrap().id;
        // two models: the arm and a weapon held in the hand
        let arms_file = dir.join("arms.xmodel_export");
        a.sel = vec![sh, arm];
        a.export_xmodel(&arms_file).unwrap();
        let mut gun = Node::new(0, "j_gun", Kind::Joint);
        gun.parent = Some(wr);
        gun.t = v3(0.3, 0.2, 0.0);
        let gun = a.scene.add(gun);
        let mut body = Node::new(0, "weapon", Kind::Mesh);
        body.parent = Some(gun);
        body.mesh = Some(Mesh::cube(0.3));
        let body = a.scene.add(body);
        let weapon_file = dir.join("weapon.xmodel_export");
        a.sel = vec![gun, body];
        a.export_xmodel(&weapon_file).unwrap();
        // two different animations from the same rig: frames 1-30 and 31-60
        a.sel = vec![sh];
        let (fa, fb) = (dir.join("idle.xanim_export"), dir.join("reload.xanim_export"));
        a.cod.start = 1.0;
        a.cod.end = 30.0;
        a.export_xanim(&fa).unwrap();
        a.cod.start = 31.0;
        a.cod.end = 60.0;
        a.export_xanim(&fb).unwrap();
        // expected wrist path, evaluated like each export did (IK depends on
        // the frame it starts from)
        let mut wrist_at = Vec::new();
        for range in [1..=30, 31..=60] {
            let mut s = a.scene.clone();
            for f in range {
                s.evaluate(f as f32);
                wrist_at.push(s.world_matrices()[&wr].translation());
            }
        }

        // everything selected at once, anims listed first on purpose
        let mut b = App::new(&ctx, None);
        b.scene = Scene::default();
        b.sel.clear();
        let msg = b.import_files(vec![fa.clone(), fb.clone(), arms_file.clone(), weapon_file.clone()]);
        assert!(msg.contains('4'), "{msg}");
        assert_eq!(b.cod_model_groups().len(), 2, "both models imported first");
        assert_eq!(b.scene.clips.len(), 2);
        assert_eq!(b.scene.active_clip, Some(1));
        let wr2 = b.scene.nodes.iter().find(|n| n.name == "wrist").unwrap().id;
        let check = |b: &mut App, offset: usize| {
            for i in 0..30 {
                b.scene.evaluate(1.0 + i as f32);
                let got = b.scene.world_matrices()[&wr2].translation();
                assert!((got - wrist_at[offset + i]).len() < 2e-3, "frame {} offset {offset}", i + 1);
            }
        };
        check(&mut b, 30); // reload is on the models
        b.clip_activate(0);
        check(&mut b, 0); // idle
        b.clip_activate(1);
        check(&mut b, 30);

        // edit a key in idle, switch away and back: the edit is kept
        b.clip_activate(0);
        b.scene.get_mut(wr2).unwrap().curves[1].keys[5].value += 1.0;
        let edited = b.scene.get(wr2).unwrap().curves[1].keys[5].value;
        b.clip_activate(1);
        assert!((b.scene.get(wr2).unwrap().curves[1].keys[5].value - edited).abs() > 1e-4);
        b.clip_activate(0);
        assert!((b.scene.get(wr2).unwrap().curves[1].keys[5].value - edited).abs() < 1e-6);

        // the library is saved with the scene
        b.clip_capture();
        let json = serde_json::to_string(&b.scene).unwrap();
        let back: Scene = serde_json::from_str(&json).unwrap();
        assert_eq!(back.clips.len(), 2);
        assert_eq!(back.clips[0].name, "idle");
        assert_eq!(back.clips[1].name, "reload");
    }
}
