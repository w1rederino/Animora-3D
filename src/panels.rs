//! UI panels: menus, status line, shelf, toolbox, outliner, channel box and
//! attribute editor.

use crate::anim::*;
use crate::app::*;
use crate::icons::*;
use crate::math::*;
use crate::mesh::*;
use crate::scene::*;
use crate::view::*;
use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KeyRef {
    pub node: Id,
    pub ch: usize,
    pub idx: usize,
}

#[derive(Clone)]
pub struct KeyDrag {
    pub start: Pos2,
    pub starts: Vec<(KeyRef, f32, f32)>,
    pub boxsel: bool,
}

pub struct GraphState {
    pub f0: f32,
    pub f1: f32,
    pub v0: f32,
    pub v1: f32,
    pub sel: Vec<KeyRef>,
    pub drag: Option<KeyDrag>,
    pub show: [bool; CHANNELS],
    pub fit_request: bool,
}

impl Default for GraphState {
    fn default() -> Self {
        GraphState { f0: 0.0, f1: 120.0, v0: -5.0, v1: 10.0, sel: Vec::new(), drag: None, show: [true; CHANNELS], fit_request: true }
    }
}

#[derive(Default)]
pub struct DopeState {
    pub drag: Option<KeyDrag>,
}

#[derive(Default)]
pub struct OutlinerState {
    pub collapsed: HashSet<Id>,
    pub rename: Option<(Id, String)>,
    pub drag: Option<Id>,
    pub shelf_tab: usize,
}

impl App {
    // ---------------- menu bar ----------------
    pub fn menu_bar(&mut self, ui: &mut Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button(crate::tr!("Datei", "File"), |ui| {
                if ui.button(crate::tr!("Neue Szene      Strg+N", "New scene      Ctrl+N")).clicked() {
                    self.new_scene();
                }
                if ui.button(crate::tr!("Öffnen …        Strg+O", "Open …         Ctrl+O")).clicked() {
                    self.open();
                }
                if ui.button(crate::tr!("Speichern       Strg+S", "Save           Ctrl+S")).clicked() {
                    self.save(false);
                }
                if ui.button(crate::tr!("Speichern unter …", "Save as …")).clicked() {
                    self.save(true);
                }
                ui.separator();
                if ui.button(crate::tr!("OBJ importieren …", "Import OBJ …")).clicked() {
                    self.import_obj();
                }
                if ui.button(crate::tr!("OBJ exportieren …", "Export OBJ …")).clicked() {
                    self.export_obj();
                }
                ui.separator();
                if ui.button(crate::tr!("Beenden", "Quit")).clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button(crate::tr!("Bearbeiten", "Edit"), |ui| {
                if ui.button(crate::tr!("Rückgängig      Strg+Z", "Undo           Ctrl+Z")).clicked() {
                    self.do_undo();
                }
                if ui.button(crate::tr!("Wiederholen     Strg+Y", "Redo           Ctrl+Y")).clicked() {
                    self.do_redo();
                }
                ui.separator();
                if ui.button(crate::tr!("Duplizieren     Strg+D", "Duplicate      Ctrl+D")).clicked() {
                    self.duplicate_selection();
                }
                if ui.button(crate::tr!("Löschen         Entf", "Delete         Del")).clicked() {
                    self.delete_selection();
                }
                if ui.button(crate::tr!("Alles auswählen Strg+A", "Select all     Ctrl+A")).clicked() {
                    self.sel = self.scene.nodes.iter().map(|n| n.id).collect();
                }
                if ui.button(crate::tr!("Auswahl aufheben", "Deselect")).clicked() {
                    self.sel.clear();
                }
                ui.separator();
                if ui.button(crate::tr!("Gruppieren      Strg+G", "Group          Ctrl+G")).clicked() {
                    self.create_node(Kind::Group);
                }
                if ui.button(crate::tr!("Verknüpfen      P", "Parent         P")).clicked() {
                    self.parent_selection();
                }
                if ui.button(crate::tr!("Lösen           Umschalt+P", "Unparent       Shift+P")).clicked() {
                    self.unparent_selection();
                }
            });
            ui.menu_button(crate::tr!("Erstellen", "Create"), |ui| {
                ui.menu_button(crate::tr!("Polygon-Grundkörper", "Polygon primitives"), |ui| {
                    for p in Prim::ALL {
                        if ui.button(p.label()).clicked() {
                            self.create_prim(p);
                        }
                    }
                });
                ui.menu_button(crate::tr!("Lichter", "Lights"), |ui| {
                    for k in [LightKind::Directional, LightKind::Point, LightKind::Ambient] {
                        if ui.button(k.label()).clicked() {
                            self.create_light(k);
                        }
                    }
                });
                if ui.button(crate::tr!("Kamera", "Camera")).clicked() {
                    self.create_node(Kind::Camera);
                }
                if ui.button(crate::tr!("Leere Gruppe", "Empty group")).clicked() {
                    self.sel.clear();
                    self.create_node(Kind::Group);
                }
                if ui.button(crate::tr!("Gelenk-Werkzeug", "Joint tool")).clicked() {
                    self.start_joint_tool();
                }
            });
            ui.menu_button(crate::tr!("Modellieren", "Modeling"), |ui| {
                if ui.button(crate::tr!("Punkte bearbeiten   F8", "Edit vertices   F8")).clicked() {
                    self.toggle_component(CompMode::Vertex);
                }
                if ui.button(crate::tr!("Flächen bearbeiten  F11", "Edit faces      F11")).clicked() {
                    self.toggle_component(CompMode::Face);
                }
                ui.separator();
                if ui.button(crate::tr!("Extrudieren         Strg+E", "Extrude         Ctrl+E")).clicked() {
                    self.extrude();
                }
                if ui.button(crate::tr!("Flächen löschen", "Delete faces")).clicked() {
                    self.delete_selection();
                }
                if ui.button(crate::tr!("Glätten (Unterteilen)", "Smooth (subdivide)")).clicked() {
                    self.smooth_mesh();
                }
                if ui.button(crate::tr!("Entspannen", "Relax")).clicked() {
                    self.relax_mesh();
                }
                if ui.button(crate::tr!("Transformationen einfrieren", "Freeze transformations")).clicked() {
                    self.freeze_selection();
                }
                ui.separator();
                if ui.button(crate::tr!("Glättungsvorschau an   3", "Smooth preview on   3")).clicked() {
                    self.set_smooth(true);
                }
                if ui.button(crate::tr!("Glättungsvorschau aus  1", "Smooth preview off  1")).clicked() {
                    self.set_smooth(false);
                }
            });
            ui.menu_button(crate::tr!("Skelett", "Skeleton"), |ui| {
                if ui.button(crate::tr!("Gelenk-Werkzeug", "Joint tool")).clicked() {
                    self.start_joint_tool();
                }
                if ui.button(crate::tr!("IK-Griff erstellen", "Create IK handle")).clicked() {
                    self.create_ik();
                }
                ui.separator();
                if ui.button(crate::tr!("Haut binden", "Bind skin")).clicked() {
                    self.bind_skin();
                }
                if ui.button(crate::tr!("Bindung lösen", "Unbind skin")).clicked() {
                    self.unbind_skin();
                }
                ui.separator();
                if ui.button(crate::tr!("Alle Gelenke auswählen", "Select all joints")).clicked() {
                    self.select_joints(false);
                }
                if ui.button(crate::tr!("Animierte Gelenke auswählen", "Select keyed joints")).clicked() {
                    self.select_joints(true);
                }
                if ui.button(crate::tr!("Namespaces bereinigen", "Clean namespaces")).clicked() {
                    self.clean_namespaces();
                }
                ui.separator();
                if ui.button(crate::tr!("Szene zurücksetzen (Ruhepose)", "Reset scene (rest pose)")).clicked() {
                    self.reset_scene();
                }
                if ui.button(crate::tr!("Kurven löschen", "Clear curves")).clicked() {
                    self.clear_curves();
                }
            });
            ui.menu_button(crate::tr!("Animation", "Animation"), |ui| {
                if ui.button(crate::tr!("Schlüssel setzen           S", "Set key                S")).clicked() {
                    self.key_selection(0..9);
                }
                if ui.button(crate::tr!("Schlüssel Position         Umschalt+W", "Key translate          Shift+W")).clicked() {
                    self.key_selection(0..3);
                }
                if ui.button(crate::tr!("Schlüssel Drehung          Umschalt+E", "Key rotate             Shift+E")).clicked() {
                    self.key_selection(3..6);
                }
                if ui.button(crate::tr!("Schlüssel Skalierung       Umschalt+R", "Key scale              Shift+R")).clicked() {
                    self.key_selection(6..9);
                }
                if ui.button(crate::tr!("Schlüssel Sichtbarkeit", "Key visibility")).clicked() {
                    self.key_selection(9..10);
                }
                if ui.button(crate::tr!("Schlüssel hier löschen", "Delete keys here")).clicked() {
                    self.delete_keys_at_frame();
                }
                ui.separator();
                if ui.button(crate::tr!("Vorheriger Schlüssel       ,", "Previous key           ,")).clicked() {
                    self.step_key(-1.0);
                }
                if ui.button(crate::tr!("Nächster Schlüssel         .", "Next key               .")).clicked() {
                    self.step_key(1.0);
                }
                if ui.button(crate::tr!("Abspielen / Stopp          Alt+V", "Play / stop            Alt+V")).clicked() {
                    self.playing = !self.playing;
                }
                ui.separator();
                ui.checkbox(&mut self.auto_key, "Auto-Key");
                ui.checkbox(&mut self.show_trail, crate::tr!("Bewegungspfad zeigen", "Show motion trail"));
                ui.checkbox(&mut self.loop_play, crate::tr!("Endlos abspielen", "Loop playback"));
                ui.menu_button(crate::tr!("Standard-Tangente", "Default tangent"), |ui| {
                    for t in Tangent::ALL {
                        ui.radio_value(&mut self.default_tangent, t, t.label());
                    }
                });
            });
            ui.menu_button(crate::tr!("Rendern", "Render"), |ui| {
                if ui.button(crate::tr!("Aktuelles Bild rendern", "Render current frame")).clicked() {
                    self.pending_render = true;
                }
                if ui.button(crate::tr!("Sequenz rendern (PNG) …", "Render sequence (PNG) …")).clicked() {
                    self.start_sequence();
                }
                if ui.button(crate::tr!("Render-Fenster", "Render window")).clicked() {
                    self.render.show = true;
                }
            });
            ui.menu_button(crate::tr!("Ansicht", "View"), |ui| {
                ui.checkbox(&mut self.quad_view, crate::tr!("Vier Ansichten (Leertaste)", "Four views (Space)"));
                ui.separator();
                for (i, k) in [ViewKind::Persp, ViewKind::Top, ViewKind::Front, ViewKind::Side].iter().enumerate() {
                    if ui.button(crate::trf!("Ansicht: {}", "View: {}", k.label())).clicked() {
                        self.active_view = i;
                        self.quad_view = false;
                    }
                }
                ui.menu_button(crate::tr!("Durch Kamera schauen", "Look through camera"), |ui| {
                    let cams: Vec<(Id, String)> = self.scene.nodes.iter().filter(|n| n.kind == Kind::Camera).map(|n| (n.id, n.name.clone())).collect();
                    let v = self.active_view;
                    if ui.button(crate::tr!("Keine (frei)", "None (free)")).clicked() {
                        self.views[v].look_through = None;
                    }
                    for (id, name) in cams {
                        if ui.button(name).clicked() {
                            self.views[v].look_through = Some(id);
                        }
                    }
                });
                if ui.button(crate::tr!("Kamera an Ansicht ausrichten", "Align camera to view")).clicked() {
                    self.align_camera_to_view();
                }
                ui.separator();
                let v = self.active_view;
                ui.radio_value(&mut self.views[v].display, Display::Shaded, crate::tr!("Schattiert (5)", "Shaded (5)"));
                ui.radio_value(&mut self.views[v].display, Display::Wire, crate::tr!("Drahtgitter (4)", "Wireframe (4)"));
                ui.radio_value(&mut self.views[v].display, Display::ShadedWire, crate::tr!("Schattiert + Gitter (6)", "Shaded + wireframe (6)"));
                ui.checkbox(&mut self.views[v].scene_lights, crate::tr!("Szenenlichter verwenden (7)", "Use scene lights (7)"));
                ui.checkbox(&mut self.show_grid, crate::tr!("Raster", "Grid"));
                ui.separator();
                if ui.button(crate::tr!("Auswahl einpassen   F", "Frame selection   F")).clicked() {
                    self.frame_view(false);
                }
                if ui.button(crate::tr!("Alles einpassen     A", "Frame all         A")).clicked() {
                    self.frame_view(true);
                }
            });
            ui.menu_button(crate::tr!("Fenster", "Windows"), |ui| {
                ui.checkbox(&mut self.show_bottom, crate::tr!("Editoren unten", "Bottom editors"));
                if ui.button(crate::tr!("Kurven-Editor", "Graph Editor")).clicked() {
                    self.show_bottom = true;
                    self.bottom_tab = BottomTab::Graph;
                }
                if ui.button(crate::tr!("Dope Sheet", "Dope Sheet")).clicked() {
                    self.show_bottom = true;
                    self.bottom_tab = BottomTab::Dope;
                }
                if ui.button(crate::tr!("Skript-Editor", "Script Editor")).clicked() {
                    self.show_bottom = true;
                    self.bottom_tab = BottomTab::Script;
                }
                if ui.button(crate::tr!("Attribut-Editor", "Attribute Editor")).clicked() {
                    self.right_tab = RightTab::Attributes;
                }
                if ui.button(crate::tr!("Kanal-Box", "Channel Box")).clicked() {
                    self.right_tab = RightTab::Channels;
                }
            });
            self.cod_menu(ui);
            self.cast_menu(ui);
            ui.menu_button(crate::tr!("Hilfe", "Help"), |ui| {
                if ui.button(crate::tr!("Tastenkürzel   F1", "Keyboard shortcuts   F1")).clicked() {
                    self.show_help = true;
                }
                ui.separator();
                ui.menu_button("Language / Sprache", |ui| {
                    for l in [crate::i18n::Lang::English, crate::i18n::Lang::German] {
                        if ui.radio(crate::i18n::lang() == l, l.label()).clicked() {
                            self.set_language(l);
                            ui.close();
                        }
                    }
                });
            });
        });
    }

    /// Small "English / Deutsch" selector (always shown in both languages).
    pub fn language_picker(&mut self, ui: &mut Ui) {
        let cur = crate::i18n::lang();
        egui::ComboBox::from_id_salt("language").width(90.0).selected_text(cur.label()).show_ui(ui, |ui| {
            for l in [crate::i18n::Lang::English, crate::i18n::Lang::German] {
                if ui.selectable_label(cur == l, l.label()).clicked() && cur != l {
                    self.set_language(l);
                }
            }
        });
        ui.label(egui::RichText::new("Language / Sprache").small().color(Color32::from_gray(160)));
    }

    /// Switches the interface language and remembers the choice.
    pub fn set_language(&mut self, l: crate::i18n::Lang) {
        // keep the example script in step with the language if it was not edited
        let old_script = crate::tr!(
            "# Befehle, eine pro Zeile – z. B.:\nkugel\nbewegen 0 2 0\nkey\nbild 24\nbewegen 4 2 0\nkey",
            "# Commands, one per line – e.g.:\nsphere\nmove 0 2 0\nkey\nframe 24\nmove 4 2 0\nkey"
        );
        let untouched = self.script == old_script;
        crate::i18n::save(l);
        if untouched {
            self.script = crate::tr!(
                "# Befehle, eine pro Zeile – z. B.:\nkugel\nbewegen 0 2 0\nkey\nbild 24\nbewegen 4 2 0\nkey",
                "# Commands, one per line – e.g.:\nsphere\nmove 0 2 0\nkey\nframe 24\nmove 4 2 0\nkey"
            )
            .into();
        }
        self.status = crate::tr!("Sprache: Deutsch", "Language: English").into();
    }

    pub fn create_light(&mut self, k: LightKind) {
        self.create_node(Kind::Light);
        if let Some(n) = self.lead().and_then(|l| self.scene.get_mut(l)) {
            n.light_kind = k;
            if k == LightKind::Point {
                n.t = v3(0.0, 4.0, 0.0);
            }
            if k == LightKind::Ambient {
                n.intensity = 0.3;
            }
        }
    }

    pub fn start_joint_tool(&mut self) {
        self.tool = Tool::Joint;
        self.comp = CompMode::Object;
        self.joint_chain_last = None;
        self.status = crate::tr!("Gelenk-Werkzeug: in die Ansicht klicken (am besten 'Vorne'), Enter beendet die Kette.", "Joint tool: click into the view (best in 'Front'), Enter finishes the chain.").into();
    }

    pub fn align_camera_to_view(&mut self) {
        let cam = self.sel.iter().copied().find(|id| self.scene.get(*id).is_some_and(|n| n.kind == Kind::Camera));
        let Some(cam) = cam else {
            self.status = crate::tr!("Erst eine Kamera auswählen.", "Select a camera first.").into();
            return;
        };
        self.checkpoint();
        let w = self.scene.world_matrices();
        let mut vc = self.views[0].clone();
        vc.look_through = None;
        let (m, _, _) = vc.camera(&self.scene, &w);
        let pinv = self.scene.parent_world(cam, &w).inverse();
        let (t, r, _) = (pinv * m).decompose();
        if let Some(n) = self.scene.get_mut(cam) {
            n.t = t;
            n.r = r;
        }
        self.auto_key_selection();
    }

    // ---------------- status line & shelf ----------------
    pub fn status_line(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let before = self.menu_set;
            egui::ComboBox::from_id_salt("menuset").width(110.0).selected_text(self.menu_set.label()).show_ui(ui, |ui| {
                for m in MenuSet::ALL {
                    ui.selectable_value(&mut self.menu_set, m, m.label());
                }
            });
            if before != self.menu_set {
                self.outliner.shelf_tab = match self.menu_set {
                    MenuSet::Modeling => 1,
                    MenuSet::Rigging => 2,
                    MenuSet::Animation => 3,
                    MenuSet::Rendering => 4,
                };
            }
            ui.separator();
            ui.label(crate::tr!("Auswahl:", "Select:"));
            let is_mesh = self.lead().and_then(|l| self.scene.get(l)).is_some_and(|n| n.kind == Kind::Mesh);
            if ui.selectable_label(self.comp == CompMode::Object, crate::tr!("Objekte", "Objects")).clicked() {
                self.comp = CompMode::Object;
            }
            if ui.add_enabled(is_mesh, egui::Button::selectable(self.comp == CompMode::Vertex, crate::tr!("Punkte", "Vertices"))).clicked() {
                self.toggle_component(CompMode::Vertex);
            }
            if ui.add_enabled(is_mesh, egui::Button::selectable(self.comp == CompMode::Face, crate::tr!("Flächen", "Faces"))).clicked() {
                self.toggle_component(CompMode::Face);
            }
            ui.separator();
            ui.toggle_value(&mut self.snap_lock, crate::tr!("Raster-Fang", "Grid snap")).on_hover_text(crate::tr!("Am Raster einrasten (oder X gedrückt halten)", "Snap to the grid (or hold X)"));
            ui.separator();
            if ui.button(crate::tr!("↶ Rückgängig", "↶ Undo")).on_hover_text(crate::tr!("Strg+Z", "Ctrl+Z")).clicked() {
                self.do_undo();
            }
            if ui.button("↷").on_hover_text(crate::tr!("Wiederholen (Strg+Y)", "Redo (Ctrl+Y)")).clicked() {
                self.do_redo();
            }
            ui.separator();
            if ui.button(crate::tr!("Rendern", "Render")).on_hover_text(crate::tr!("Aktuelles Bild rendern", "Render current frame")).clicked() {
                self.pending_render = true;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.language_picker(ui);
                ui.separator();
                let name = self.file.as_ref().and_then(|f| f.file_name()).map(|f| f.to_string_lossy().to_string()).unwrap_or(crate::tr!("unbenannt", "untitled").into());
                ui.label(egui::RichText::new(format!("Animora 3D – {name}")).color(Color32::from_gray(180)));
            });
        });
    }

    pub fn shelf(&mut self, ui: &mut Ui) {
        let tabs: [&str; 7] = [crate::tr!("Grundkörper", "Primitives"), crate::tr!("Modellieren", "Modeling"), crate::tr!("Rigging", "Rigging"), crate::tr!("Animation", "Animation"), crate::tr!("Rendern", "Render"), "CoD", "Cast"];
        ui.horizontal(|ui| {
            for (i, t) in tabs.iter().enumerate() {
                if ui.selectable_label(self.outliner.shelf_tab == i, *t).clicked() {
                    self.outliner.shelf_tab = i;
                }
            }
        });
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            match self.outliner.shelf_tab {
                0 => {
                    for p in Prim::ALL {
                        if shelf_button(ui, prim_icon(p), p.label(), &crate::trf!("Polygon-{} erzeugen", "Create polygon {}", p.label())) {
                            self.create_prim(p);
                        }
                    }
                    ui.separator();
                    if shelf_button(ui, Icon::Light, crate::tr!("Licht", "Light"), crate::tr!("Richtungslicht erzeugen", "Create directional light")) {
                        self.create_light(LightKind::Directional);
                    }
                    if shelf_button(ui, Icon::PointLight, crate::tr!("Punktlicht", "Point light"), crate::tr!("Punktlicht erzeugen", "Create point light")) {
                        self.create_light(LightKind::Point);
                    }
                    if shelf_button(ui, Icon::Camera, crate::tr!("Kamera", "Camera"), crate::tr!("Kamera an der aktuellen Ansicht erzeugen", "Create a camera at the current view")) {
                        self.create_node(Kind::Camera);
                    }
                    if shelf_button(ui, Icon::Group, crate::tr!("Gruppe", "Group"), crate::tr!("Auswahl gruppieren (Strg+G)", "Group selection (Ctrl+G)")) {
                        self.create_node(Kind::Group);
                    }
                }
                1 => {
                    if shelf_button(ui, Icon::Extrude, crate::tr!("Extrudieren", "Extrude"), crate::tr!("Gewählte Flächen extrudieren (Strg+E)", "Extrude selected faces (Ctrl+E)")) {
                        self.extrude();
                    }
                    if shelf_button(ui, Icon::Smooth, crate::tr!("Glätten", "Smooth"), crate::tr!("Objekt unterteilen und glätten", "Subdivide and smooth the object")) {
                        self.smooth_mesh();
                    }
                    if shelf_button(ui, Icon::Sphere, crate::tr!("Entspannen", "Relax"), crate::tr!("Punkte entspannen", "Relax vertices")) {
                        self.relax_mesh();
                    }
                    if shelf_button(ui, Icon::Delete, crate::tr!("Löschen", "Delete"), crate::tr!("Auswahl / Flächen löschen", "Delete selection / faces")) {
                        self.delete_selection();
                    }
                    if shelf_button(ui, Icon::Freeze, crate::tr!("Einfrieren", "Freeze"), crate::tr!("Transformationen einfrieren", "Freeze transformations")) {
                        self.freeze_selection();
                    }
                    ui.separator();
                    ui.vertical(|ui| {
                        ui.label(crate::tr!("Extrusion:", "Extrusion:"));
                        ui.add(egui::DragValue::new(&mut self.extrude_dist).speed(0.05).range(-10.0..=10.0));
                    });
                }
                2 => {
                    if shelf_button(ui, Icon::Joint, crate::tr!("Gelenke", "Joints"), crate::tr!("Gelenk-Werkzeug: Klicks setzen eine Gelenkkette", "Joint tool: clicks place a joint chain")) {
                        self.start_joint_tool();
                    }
                    if shelf_button(ui, Icon::Ik, crate::tr!("IK-Griff", "IK handle"), crate::tr!("Startgelenk und Endgelenk wählen, dann klicken", "Select the start and end joint, then click")) {
                        self.create_ik();
                    }
                    if shelf_button(ui, Icon::Bind, crate::tr!("Binden", "Bind"), crate::tr!("Gelenke + Objekt wählen: Haut weich binden", "Select joints + object: smooth bind skin")) {
                        self.bind_skin();
                    }
                    if shelf_button(ui, Icon::Unbind, crate::tr!("Lösen", "Unbind"), crate::tr!("Hautbindung lösen", "Unbind skin")) {
                        self.unbind_skin();
                    }
                    if shelf_button(ui, Icon::Parent, crate::tr!("Verknüpfen", "Parent"), crate::tr!("Kinder, dann Eltern wählen (P)", "Select children, then the parent (P)")) {
                        self.parent_selection();
                    }
                }
                3 => {
                    if shelf_button(ui, Icon::Key, crate::tr!("Schlüssel", "Key"), crate::tr!("Schlüssel auf alle Kanäle (S)", "Key all channels (S)")) {
                        self.key_selection(0..9);
                    }
                    if shelf_button(ui, Icon::Move, crate::tr!("Pos.-Key", "Move key"), crate::tr!("Schlüssel nur auf Position (Umschalt+W)", "Key translate only (Shift+W)")) {
                        self.key_selection(0..3);
                    }
                    if shelf_button(ui, Icon::Rotate, crate::tr!("Dreh-Key", "Rot. key"), crate::tr!("Schlüssel nur auf Drehung (Umschalt+E)", "Key rotate only (Shift+E)")) {
                        self.key_selection(3..6);
                    }
                    if shelf_button(ui, Icon::Scale, crate::tr!("Skal.-Key", "Scale key"), crate::tr!("Schlüssel nur auf Skalierung (Umschalt+R)", "Key scale only (Shift+R)")) {
                        self.key_selection(6..9);
                    }
                    if shelf_button(ui, Icon::Delete, crate::tr!("Key weg", "Del key"), crate::tr!("Schlüssel am aktuellen Bild löschen", "Delete keys at the current frame")) {
                        self.delete_keys_at_frame();
                    }
                    if shelf_button(ui, if self.playing { Icon::Pause } else { Icon::Play }, crate::tr!("Abspielen", "Play"), crate::tr!("Abspielen / Stopp (Alt+V)", "Play / stop (Alt+V)")) {
                        self.playing = !self.playing;
                    }
                    if shelf_button(ui, Icon::Trail, crate::tr!("Pfad", "Trail"), crate::tr!("Bewegungspfad an/aus", "Motion trail on/off")) {
                        self.show_trail = !self.show_trail;
                    }
                }
                5 => {
                    if shelf_button(ui, Icon::Joint, crate::tr!("1 XModel ↓", "1 XModel ↓"), crate::tr!("Schritt 1: Modelle importieren – Arme, Waffe, Visier … (.xmodel_export)", "Step 1: import models – arms, weapon, scope … (.xmodel_export)")) {
                        self.cod_import_xmodel_dialog();
                    }
                    if shelf_button(ui, Icon::Sequence, crate::tr!("2 Anhängen", "2 Attach"), crate::tr!("Schritt 2: eine Animation an alle importierten Modelle anhängen (.xanim_export)", "Step 2: attach one animation to all imported models (.xanim_export)")) {
                        self.cod_import_xanim_dialog();
                    }
                    ui.separator();
                    if shelf_button(ui, Icon::Joint, "XModel ↑", crate::tr!("Call-of-Duty-Modell exportieren (.xmodel_export)", "Export Call of Duty model (.xmodel_export)")) {
                        self.open_cod(1);
                    }
                    if shelf_button(ui, Icon::Sequence, "XAnim ↑", crate::tr!("Call-of-Duty-Animation exportieren (.xanim_export)", "Export Call of Duty animation (.xanim_export)")) {
                        self.open_cod(0);
                    }
                    if shelf_button(ui, Icon::Parent, crate::tr!("Waffe → Rig", "Weapon → rig"), crate::tr!("Waffe ans Rig hängen (wie CoD Maya Tools): j_gun / tag_weapon an tag_weapon_right / tag_weapon", "Attach weapon to rig (like CoD Maya tools): j_gun / tag_weapon onto tag_weapon_right / tag_weapon")) {
                        self.attach_weapon_dialog();
                    }
                    if shelf_button(ui, Icon::Key, "Notetracks", crate::tr!("Notetrack-Ereignisse bearbeiten", "Edit notetrack events")) {
                        self.open_cod(2);
                    }
                }
                6 => {
                    if shelf_button(ui, Icon::Joint, crate::tr!("Import ↓", "Import ↓"), crate::tr!("Cast-Dateien importieren – Modelle und Animationen, mehrere auf einmal", "Import Cast files – models and animations, several at once")) {
                        self.cast_import_dialog();
                    }
                    if shelf_button(ui, Icon::Joint, crate::tr!("Modell ↑", "Model ↑"), crate::tr!("Modell als Cast exportieren", "Export model as Cast")) {
                        self.cast_export_model_dialog();
                    }
                    if shelf_button(ui, Icon::Sequence, crate::tr!("Anim ↑", "Anim ↑"), crate::tr!("Animation als Cast exportieren", "Export animation as Cast")) {
                        self.cast_export_anim_dialog();
                    }
                    ui.separator();
                    if shelf_button(ui, Icon::Key, "Notetracks", crate::tr!("Notetracks bearbeiten", "Edit notetracks")) {
                        self.open_cod(2);
                    }
                    if shelf_button(ui, Icon::Freeze, crate::tr!("Ruhepose", "Rest pose"), crate::tr!("Szene zurücksetzen: Gelenke in Ruhepose, Animation entfernen", "Reset scene: joints to rest pose, remove animation")) {
                        self.reset_scene();
                    }
                }
                _ => {
                    if shelf_button(ui, Icon::Render, crate::tr!("Bild", "Image"), crate::tr!("Aktuelles Bild rendern", "Render current frame")) {
                        self.pending_render = true;
                    }
                    if shelf_button(ui, Icon::Sequence, crate::tr!("Sequenz", "Sequence"), crate::tr!("Alle Bilder als PNG rendern", "Render all frames as PNG")) {
                        self.start_sequence();
                    }
                    if shelf_button(ui, Icon::Light, crate::tr!("Licht", "Light"), crate::tr!("Richtungslicht erzeugen", "Create directional light")) {
                        self.create_light(LightKind::Directional);
                    }
                    if shelf_button(ui, Icon::Camera, crate::tr!("Kamera", "Camera"), crate::tr!("Kamera erzeugen", "Create camera")) {
                        self.create_node(Kind::Camera);
                    }
                }
            }
        });
    }

    // ---------------- toolbox ----------------
    pub fn toolbox(&mut self, ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(4.0);
            for (t, icon, tip) in [
                (Tool::Select, Icon::Select, crate::tr!("Auswählen (Q)", "Select (Q)")),
                (Tool::Move, Icon::Move, crate::tr!("Bewegen (W)", "Move (W)")),
                (Tool::Rotate, Icon::Rotate, crate::tr!("Drehen (E)", "Rotate (E)")),
                (Tool::Scale, Icon::Scale, crate::tr!("Skalieren (R)", "Scale (R)")),
                (Tool::Joint, Icon::Joint, crate::tr!("Gelenk-Werkzeug", "Joint tool")),
            ] {
                if icon_button(ui, icon, 32.0, self.tool == t, tip).clicked() {
                    if t == Tool::Joint {
                        self.start_joint_tool();
                    } else {
                        self.tool = t;
                    }
                }
            }
            ui.separator();
            if icon_button(ui, Icon::Single, 32.0, !self.quad_view, crate::tr!("Eine Ansicht", "Single view")).clicked() {
                self.quad_view = false;
            }
            if icon_button(ui, Icon::Quad, 32.0, self.quad_view, crate::tr!("Vier Ansichten (Leertaste)", "Four views (Space)")).clicked() {
                self.quad_view = true;
            }
            ui.separator();
            for (i, l) in ["P", "O", "V", "S"].iter().enumerate() {
                let label = [crate::tr!("Perspektive", "Perspective"), crate::tr!("Oben", "Top"), crate::tr!("Vorne", "Front"), crate::tr!("Seite", "Side")][i];
                if ui.add_sized([32.0, 22.0], egui::Button::selectable(self.active_view == i, *l)).on_hover_text(label).clicked() {
                    self.active_view = i;
                }
            }
        });
    }

    // ---------------- outliner ----------------
    pub fn outliner_panel(&mut self, ui: &mut Ui) {
        // animation library under the outliner
        egui::Panel::bottom("clips_panel").resizable(true).default_size(220.0).min_size(90.0).show(ui, |ui| self.clips_panel(ui));
        ui.heading(crate::tr!("Gliederung", "Outliner"));
        ui.separator();
        let mut rows: Vec<(Id, Rect)> = Vec::new();
        let area = egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for r in self.scene.children(None) {
                self.outliner_row(ui, r, 0, &mut rows);
            }
            ui.allocate_space(vec2(ui.available_width(), 40.0));
        });
        let area_rect = area.inner_rect;
        // drag & drop parenting
        if let Some(dragged) = self.outliner.drag {
            let ptr = ui.input(|i| i.pointer.hover_pos());
            if let (Some(p), Some(n)) = (ptr, self.scene.get(dragged)) {
                ui.painter().text(p + vec2(12.0, 0.0), egui::Align2::LEFT_CENTER, &n.name, egui::FontId::proportional(12.0), Color32::YELLOW);
            }
            if ui.input(|i| i.pointer.any_released()) {
                if let Some(p) = ptr {
                    let target = rows.iter().find(|(_, r)| r.contains(p)).map(|(id, _)| *id);
                    match target {
                        Some(t) if t != dragged => {
                            self.checkpoint();
                            self.scene.set_parent(dragged, Some(t));
                            self.status = crate::tr!("Per Ziehen verknüpft", "Parented by dragging").into();
                        }
                        None if area_rect.contains(p) => {
                            if self.scene.get(dragged).is_some_and(|n| n.parent.is_some()) {
                                self.checkpoint();
                                self.scene.set_parent(dragged, None);
                            }
                        }
                        _ => {}
                    }
                }
                self.outliner.drag = None;
            }
        }
    }

    fn outliner_row(&mut self, ui: &mut Ui, id: Id, depth: usize, rows: &mut Vec<(Id, Rect)>) {
        let Some(n) = self.scene.get(id) else { return };
        let name = n.name.clone();
        let kind = n.kind;
        let visible = n.visible;
        let animated = n.is_animated();
        let children = self.scene.children(Some(id));
        let collapsed = self.outliner.collapsed.contains(&id);
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 19.0), Sense::click_and_drag());
        rows.push((id, rect));
        let selected = self.sel.contains(&id);
        let p = ui.painter().clone();
        if selected {
            p.rect_filled(rect, 0.0, if self.lead() == Some(id) { Color32::from_rgb(82, 133, 166) } else { Color32::from_rgb(64, 100, 125) });
        } else if resp.hovered() {
            p.rect_filled(rect, 0.0, Color32::from_gray(80));
        }
        if self.outliner.drag.is_some() && resp.contains_pointer() && self.outliner.drag != Some(id) {
            p.rect_stroke(rect, 0.0, Stroke::new(1.0, Color32::YELLOW), egui::StrokeKind::Inside);
        }
        let x0 = rect.left() + 4.0 + depth as f32 * 14.0;
        let arrow_rect = Rect::from_min_size(pos2(x0, rect.top()), vec2(12.0, rect.height()));
        if !children.is_empty() {
            let c = arrow_rect.center();
            let tri = if collapsed {
                vec![c + vec2(-3.0, -4.0), c + vec2(3.0, 0.0), c + vec2(-3.0, 4.0)]
            } else {
                vec![c + vec2(-4.0, -2.0), c + vec2(4.0, -2.0), c + vec2(0.0, 3.0)]
            };
            p.add(egui::Shape::convex_polygon(tri, Color32::from_gray(200), Stroke::NONE));
        }
        draw_icon(&p, Rect::from_min_size(pos2(x0 + 14.0, rect.top() + 2.0), vec2(15.0, 15.0)), kind_icon(kind), Color32::from_gray(220));
        let tc = if !visible {
            Color32::from_gray(130)
        } else if animated {
            Color32::from_rgb(255, 200, 190)
        } else {
            Color32::from_gray(230)
        };
        let renaming = self.outliner.rename.as_ref().is_some_and(|(r, _)| *r == id);
        if renaming {
            let text_rect = Rect::from_min_max(pos2(x0 + 32.0, rect.top()), rect.max);
            let mut done = false;
            if let Some((_, buf)) = &mut self.outliner.rename {
                let r = ui.put(text_rect, egui::TextEdit::singleline(buf));
                r.request_focus();
                done = r.lost_focus();
            }
            if done {
                if let Some((rid, buf)) = self.outliner.rename.take() {
                    let new = buf.trim().to_string();
                    if !new.is_empty() && !self.scene.nodes.iter().any(|o| o.name == new && o.id != rid) {
                        self.checkpoint();
                        if let Some(n) = self.scene.get_mut(rid) {
                            n.name = new;
                        }
                    }
                }
            }
        } else {
            p.text(pos2(x0 + 32.0, rect.center().y), egui::Align2::LEFT_CENTER, &name, egui::FontId::proportional(13.0), tc);
        }
        if resp.clicked() {
            let ptr = resp.interact_pointer_pos().unwrap_or_default();
            if !children.is_empty() && arrow_rect.contains(ptr) {
                if collapsed {
                    self.outliner.collapsed.remove(&id);
                } else {
                    self.outliner.collapsed.insert(id);
                }
            } else {
                let (shift, ctrl) = ui.input(|i| (i.modifiers.shift, i.modifiers.command));
                if ctrl {
                    self.sel.retain(|s| *s != id);
                } else if shift {
                    if self.sel.contains(&id) {
                        self.sel.retain(|s| *s != id);
                    } else {
                        self.sel.push(id);
                    }
                } else {
                    self.sel = vec![id];
                    if self.comp != CompMode::Object && kind != Kind::Mesh {
                        self.comp = CompMode::Object;
                    }
                }
            }
        }
        if resp.double_clicked() {
            self.outliner.rename = Some((id, name.clone()));
        }
        if resp.drag_started() {
            self.outliner.drag = Some(id);
        }
        resp.context_menu(|ui| {
            if ui.button(crate::tr!("Umbenennen", "Rename")).clicked() {
                self.outliner.rename = Some((id, name.clone()));
                ui.close();
            }
            if ui.button(if visible { crate::tr!("Ausblenden", "Hide") } else { crate::tr!("Einblenden", "Show") }).clicked() {
                self.checkpoint();
                if let Some(n) = self.scene.get_mut(id) {
                    n.visible = !n.visible;
                }
                ui.close();
            }
            if ui.button(crate::tr!("Duplizieren", "Duplicate")).clicked() {
                self.sel = vec![id];
                self.duplicate_selection();
                ui.close();
            }
            if ui.button(crate::tr!("Aus Hierarchie lösen", "Unparent")).clicked() {
                self.checkpoint();
                self.scene.set_parent(id, None);
                ui.close();
            }
            if ui.button(crate::tr!("Löschen", "Delete")).clicked() {
                self.sel = vec![id];
                self.comp = CompMode::Object;
                self.delete_selection();
                ui.close();
            }
        });
        if !collapsed {
            for c in children {
                self.outliner_row(ui, c, depth + 1, rows);
            }
        }
    }

    // ---------------- right panel ----------------
    pub fn right_panel(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.right_tab, RightTab::Channels, crate::tr!("Kanal-Box", "Channel Box"));
            ui.selectable_value(&mut self.right_tab, RightTab::Attributes, crate::tr!("Attribut-Editor", "Attribute Editor"));
        });
        ui.separator();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match self.right_tab {
            RightTab::Channels => self.channel_box(ui),
            RightTab::Attributes => self.attribute_editor(ui),
        });
    }

    fn channel_box(&mut self, ui: &mut Ui) {
        let Some(lead) = self.lead() else {
            ui.label(egui::RichText::new(crate::tr!("Nichts ausgewählt.", "Nothing selected.")).color(Color32::from_gray(160)));
            ui.add_space(8.0);
            ui.label(crate::tr!("Klicke ein Objekt in der Ansicht oder in der Gliederung an.", "Click an object in the view or in the outliner."));
            return;
        };
        let Some(node) = self.scene.get(lead).cloned() else { return };
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
            draw_icon(ui.painter(), r, kind_icon(node.kind), Color32::WHITE);
            ui.label(egui::RichText::new(&node.name).strong().size(14.0));
            if self.sel.len() > 1 {
                ui.label(egui::RichText::new(format!("(+{})", self.sel.len() - 1)).color(Color32::from_gray(160)));
            }
        });
        ui.add_space(4.0);
        let frame = self.scene.frame;
        let w = ui.available_width();
        for c in 0..CHANNELS {
            let curve = &node.curves[c];
            let on_key = curve.key_index_at(frame).is_some();
            let animated = !curve.is_empty();
            let (rect, lresp) = ui.allocate_exact_size(vec2(w, 21.0), Sense::click());
            let field_rect = Rect::from_min_max(pos2(rect.left() + w * 0.54, rect.top() + 1.0), pos2(rect.right() - 2.0, rect.bottom() - 1.0));
            ui.painter().text(pos2(rect.left() + w * 0.52 - 4.0, rect.center().y), egui::Align2::RIGHT_CENTER, channel_name(c), egui::FontId::proportional(13.0), Color32::from_gray(215));
            let mut v = node.channel(c);
            let before = v;
            let col = if on_key {
                Color32::from_rgb(205, 39, 41)
            } else if animated {
                Color32::from_rgb(221, 114, 122)
            } else {
                Color32::from_gray(43)
            };
            let resp = ui
                .scope_builder(egui::UiBuilder::new().max_rect(field_rect), |ui| {
                    let vis = ui.visuals_mut();
                    for wv in [&mut vis.widgets.inactive, &mut vis.widgets.hovered, &mut vis.widgets.active] {
                        wv.bg_fill = col;
                        wv.weak_bg_fill = col;
                    }
                    if c == 9 {
                        let on = v >= 0.5;
                        let r = ui.add_sized(field_rect.size(), egui::Button::new(if on { crate::tr!("an", "on") } else { crate::tr!("aus", "off") }));
                        if r.clicked() {
                            v = if on { 0.0 } else { 1.0 };
                        }
                        r
                    } else {
                        let speed = if c >= 6 { 0.01 } else if c >= 3 { 0.5 } else { 0.02 };
                        ui.add_sized(field_rect.size(), egui::DragValue::new(&mut v).speed(speed).max_decimals(3))
                    }
                })
                .inner;
            if resp.drag_started() || resp.gained_focus() || (c == 9 && resp.clicked()) {
                self.checkpoint();
            }
            if v != before {
                let tan = self.default_tangent;
                for &id in &self.sel.clone() {
                    if let Some(n) = self.scene.get_mut(id) {
                        n.set_channel(c, v);
                        if self.auto_key {
                            n.set_key(c, frame, tan);
                        }
                    }
                }
            }
            lresp.context_menu(|ui| {
                ui.label(egui::RichText::new(channel_name(c)).strong());
                if ui.button(crate::tr!("Schlüssel für diesen Kanal setzen", "Key this channel")).clicked() {
                    self.key_selection(c..c + 1);
                    ui.close();
                }
                if ui.button(crate::tr!("Schlüssel hier löschen", "Delete keys here")).clicked() {
                    self.checkpoint();
                    for &id in &self.sel.clone() {
                        if let Some(n) = self.scene.get_mut(id) {
                            n.curves[c].remove_key_at(frame);
                        }
                    }
                    ui.close();
                }
                if ui.button(crate::tr!("Animation entfernen", "Remove animation")).clicked() {
                    self.checkpoint();
                    for &id in &self.sel.clone() {
                        if let Some(n) = self.scene.get_mut(id) {
                            n.curves[c] = Curve::default();
                        }
                    }
                    ui.close();
                }
            });
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button(crate::tr!("Schlüssel (S)", "Key (S)")).clicked() {
                self.key_selection(0..9);
            }
            if ui.button(crate::tr!("Löschen", "Delete")).on_hover_text(crate::tr!("Schlüssel am aktuellen Bild löschen", "Delete keys at the current frame")).clicked() {
                self.delete_keys_at_frame();
            }
        });
        ui.label(egui::RichText::new(crate::tr!("Rechtsklick auf einen Kanal: Schlüssel-Menü", "Right-click a channel: key menu")).small().color(Color32::from_gray(150)));
        ui.separator();
        if let Some(pp) = node.prim {
            ui.label(egui::RichText::new(crate::tr!("EINGÄNGE", "INPUTS")).small().color(Color32::from_gray(170)));
            ui.label(egui::RichText::new(format!("poly{}", pp.kind.label())).strong());
            self.prim_params_ui(ui, lead);
        } else if let Some(m) = &node.mesh {
            ui.label(egui::RichText::new("FORM").small().color(Color32::from_gray(170)));
            ui.label(crate::trf!("{} Punkte · {} Flächen · {} Dreiecke", "{} vertices · {} faces · {} triangles", m.verts.len(), m.faces.len(), m.tri_count()));
        }
    }

    fn prim_params_ui(&mut self, ui: &mut Ui, id: Id) {
        let Some(mut pp) = self.scene.get(id).and_then(|n| n.prim) else { return };
        let before = format!("{pp:?}");
        let mut start = false;
        egui::Grid::new(("prim", id)).num_columns(2).show(ui, |ui| {
            let size_label = match pp.kind {
                Prim::Cube => crate::tr!("Größe", "Size"),
                Prim::Plane => crate::tr!("Breite", "Width"),
                _ => crate::tr!("Radius", "Radius"),
            };
            ui.label(size_label);
            let r = ui.add(egui::DragValue::new(&mut pp.size).speed(0.02).range(0.01..=1000.0));
            start |= r.drag_started() || r.gained_focus();
            ui.end_row();
            if matches!(pp.kind, Prim::Cylinder | Prim::Cone) {
                ui.label(crate::tr!("Höhe", "Height"));
                let r = ui.add(egui::DragValue::new(&mut pp.height).speed(0.02).range(0.01..=1000.0));
                start |= r.drag_started() || r.gained_focus();
                ui.end_row();
            }
            if pp.kind == Prim::Torus {
                ui.label(crate::tr!("Rohr-Radius", "Tube radius"));
                let r = ui.add(egui::DragValue::new(&mut pp.radius2).speed(0.01).range(0.01..=100.0));
                start |= r.drag_started() || r.gained_focus();
                ui.end_row();
            }
            if pp.kind != Prim::Cube {
                ui.label(if pp.kind == Prim::Plane { crate::tr!("Unterteilungen", "Subdivisions") } else { crate::tr!("Segmente", "Segments") });
                let r = ui.add(egui::DragValue::new(&mut pp.seg_u).speed(0.2).range(1..=96));
                start |= r.drag_started() || r.gained_focus();
                ui.end_row();
                if matches!(pp.kind, Prim::Sphere | Prim::Torus) {
                    ui.label(crate::tr!("Ringe", "Rings"));
                    let r = ui.add(egui::DragValue::new(&mut pp.seg_v).speed(0.2).range(2..=96));
                    start |= r.drag_started() || r.gained_focus();
                    ui.end_row();
                }
            }
        });
        if start {
            self.checkpoint();
        }
        if format!("{pp:?}") != before {
            if let Some(n) = self.scene.get_mut(id) {
                n.prim = Some(pp);
                n.mesh = Some(pp.build());
                n.skin = None;
            }
            self.sel_verts.clear();
            self.sel_faces.clear();
        }
    }

    fn attribute_editor(&mut self, ui: &mut Ui) {
        let Some(lead) = self.lead() else {
            ui.label(crate::tr!("Nichts ausgewählt.", "Nothing selected."));
            return;
        };
        let Some(node) = self.scene.get(lead).cloned() else { return };
        ui.horizontal(|ui| {
            ui.label(format!("{}:", node.kind.label()));
            let mut name = node.name.clone();
            if ui.text_edit_singleline(&mut name).changed() && !name.trim().is_empty() {
                if let Some(n) = self.scene.get_mut(lead) {
                    n.name = name;
                }
            }
        });
        ui.separator();
        egui::CollapsingHeader::new(crate::tr!("Transformation", "Transform")).default_open(true).show(ui, |ui| {
            let mut n = node.clone();
            let mut changed = false;
            let mut start = false;
            egui::Grid::new("xform").num_columns(4).show(ui, |ui| {
                for (label, v, sp) in [(crate::tr!("Position", "Translate"), &mut n.t, 0.02), (crate::tr!("Drehung", "Rotate"), &mut n.r, 0.5), (crate::tr!("Skalierung", "Scale"), &mut n.s, 0.01)] {
                    ui.label(label);
                    for a in 0..3 {
                        let mut x = v.get(a);
                        let r = ui.add(egui::DragValue::new(&mut x).speed(sp).max_decimals(3));
                        start |= r.drag_started() || r.gained_focus();
                        if x != v.get(a) {
                            v.set(a, x);
                            changed = true;
                        }
                    }
                    ui.end_row();
                }
            });
            changed |= ui.checkbox(&mut n.visible, crate::tr!("Sichtbar", "Visible")).changed();
            if start {
                self.checkpoint();
            }
            if changed {
                if let Some(m) = self.scene.get_mut(lead) {
                    m.t = n.t;
                    m.r = n.r;
                    m.s = n.s;
                    m.visible = n.visible;
                }
                self.auto_key_selection();
            }
            if let Some(p) = node.parent.and_then(|p| self.scene.get(p)) {
                ui.label(crate::trf!("Eltern: {}", "Parent: {}", p.name));
            }
        });
        match node.kind {
            Kind::Mesh => {
                egui::CollapsingHeader::new(crate::tr!("Form", "Shape")).default_open(true).show(ui, |ui| {
                    if let Some(m) = &node.mesh {
                        ui.label(crate::trf!("{} Punkte · {} Flächen", "{} vertices · {} faces", m.verts.len(), m.faces.len()));
                    }
                    if node.prim.is_some() {
                        self.prim_params_ui(ui, lead);
                    } else {
                        ui.label(egui::RichText::new(crate::tr!("(bearbeitetes Netz – keine Grundkörper-Parameter)", "(edited mesh – no primitive parameters)")).small().color(Color32::from_gray(160)));
                    }
                    let mut sp = node.smooth_preview;
                    if ui.checkbox(&mut sp, crate::tr!("Glättungsvorschau (1 / 3)", "Smooth preview (1 / 3)")).changed() {
                        if let Some(n) = self.scene.get_mut(lead) {
                            n.smooth_preview = sp;
                        }
                    }
                    ui.horizontal_wrapped(|ui| {
                        if ui.button(crate::tr!("Glätten", "Smooth")).clicked() {
                            self.smooth_mesh();
                        }
                        if ui.button(crate::tr!("Entspannen", "Relax")).clicked() {
                            self.relax_mesh();
                        }
                        if ui.button(crate::tr!("Einfrieren", "Freeze")).clicked() {
                            self.freeze_selection();
                        }
                    });
                });
                egui::CollapsingHeader::new(crate::tr!("Material", "Material")).default_open(true).show(ui, |ui| {
                    let mut mat = node.material.clone();
                    let mut changed = false;
                    egui::Grid::new("mat").num_columns(2).show(ui, |ui| {
                        ui.label(crate::tr!("Farbe", "Color"));
                        changed |= ui.color_edit_button_rgb(&mut mat.color).changed();
                        ui.end_row();
                        ui.label(crate::tr!("Glanz", "Specular"));
                        changed |= ui.add(egui::Slider::new(&mut mat.specular, 0.0..=1.0)).changed();
                        ui.end_row();
                        ui.label(crate::tr!("Glanzschärfe", "Glossiness"));
                        changed |= ui.add(egui::Slider::new(&mut mat.shininess, 1.0..=200.0).logarithmic(true)).changed();
                        ui.end_row();
                    });
                    ui.horizontal(|ui| {
                        ui.label(crate::tr!("Vorlagen:", "Presets:"));
                        if ui.button(crate::tr!("Matt", "Matte")).clicked() {
                            mat.specular = 0.0;
                            changed = true;
                        }
                        if ui.button(crate::tr!("Glänzend", "Glossy")).clicked() {
                            mat.specular = 0.6;
                            mat.shininess = 50.0;
                            changed = true;
                        }
                        if ui.button(crate::tr!("Metall", "Metal")).clicked() {
                            mat.specular = 1.0;
                            mat.shininess = 120.0;
                            changed = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(crate::tr!("Textur", "Texture"));
                        let label = mat
                            .texture
                            .as_ref()
                            .map(|t| std::path::Path::new(t).file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or(t.clone()))
                            .unwrap_or(crate::tr!("keine", "none").into());
                        let failed = mat.texture.as_ref().is_some_and(|t| self.tex_failed.contains(&tex_key(t)));
                        ui.label(egui::RichText::new(label).color(if failed { Color32::from_rgb(255, 120, 100) } else { Color32::from_gray(200) }));
                        if ui.small_button(crate::tr!("Wählen …", "Choose …")).clicked() {
                            if let Some(p) = rfd::FileDialog::new().add_filter(crate::tr!("Bild", "Image"), &["png", "jpg", "jpeg", "tga", "dds", "bmp"]).pick_file() {
                                let r = self.assign_texture(&p);
                                self.report(r, crate::tr!("Textur", "Texture"));
                            }
                        }
                        if mat.texture.is_some() && ui.small_button("✕").on_hover_text(crate::tr!("Textur entfernen", "Remove texture")).clicked() {
                            mat.texture = None;
                            changed = true;
                        }
                    });
                    if !mat.name.is_empty() {
                        ui.label(egui::RichText::new(crate::trf!("Materialname: {}", "Material name: {}", mat.name)).small().color(Color32::from_gray(160)));
                    }
                    if changed {
                        for &id in &self.sel.clone() {
                            if let Some(n) = self.scene.get_mut(id) {
                                if n.kind == Kind::Mesh {
                                    n.material = mat.clone();
                                }
                            }
                        }
                    }
                });
                if let Some(skin) = &node.skin {
                    egui::CollapsingHeader::new(crate::tr!("Hautbindung", "Skin binding")).default_open(true).show(ui, |ui| {
                        ui.label(crate::trf!("Gebunden an {} Gelenke:", "Bound to {} joints:", skin.joints.len()));
                        for j in &skin.joints {
                            if let Some(jn) = self.scene.get(*j) {
                                ui.label(format!("  • {}", jn.name));
                            }
                        }
                        if ui.button(crate::tr!("Bindung lösen", "Unbind skin")).clicked() {
                            self.unbind_skin();
                        }
                    });
                }
            }
            Kind::Light => {
                egui::CollapsingHeader::new(crate::tr!("Licht", "Light")).default_open(true).show(ui, |ui| {
                    let mut n = node.clone();
                    let mut changed = false;
                    egui::ComboBox::from_id_salt("lk").selected_text(n.light_kind.label()).show_ui(ui, |ui| {
                        for k in [LightKind::Directional, LightKind::Point, LightKind::Ambient] {
                            changed |= ui.selectable_value(&mut n.light_kind, k, k.label()).changed();
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(crate::tr!("Farbe", "Color"));
                        changed |= ui.color_edit_button_rgb(&mut n.light_color).changed();
                    });
                    changed |= ui.add(egui::Slider::new(&mut n.intensity, 0.0..=5.0).text(crate::tr!("Stärke", "Intensity"))).changed();
                    if changed {
                        if let Some(m) = self.scene.get_mut(lead) {
                            m.light_kind = n.light_kind;
                            m.light_color = n.light_color;
                            m.intensity = n.intensity;
                        }
                    }
                });
            }
            Kind::Camera => {
                egui::CollapsingHeader::new(crate::tr!("Kamera", "Camera")).default_open(true).show(ui, |ui| {
                    let mut fov = node.fov;
                    if ui.add(egui::Slider::new(&mut fov, 5.0..=120.0).text(crate::tr!("Blickwinkel °", "Field of view °"))).changed() {
                        if let Some(m) = self.scene.get_mut(lead) {
                            m.fov = fov;
                        }
                    }
                    let v = self.active_view;
                    if ui.button(crate::tr!("Durch diese Kamera schauen", "Look through this camera")).clicked() {
                        self.views[v].look_through = Some(lead);
                    }
                    if ui.button(crate::tr!("An aktuelle Ansicht ausrichten", "Align to current view")).clicked() {
                        self.align_camera_to_view();
                    }
                    if ui.button(crate::tr!("Als Render-Kamera verwenden", "Use as render camera")).clicked() {
                        self.render.camera = Some(lead);
                    }
                });
            }
            Kind::Joint => {
                egui::CollapsingHeader::new(crate::tr!("Gelenk", "Joint")).default_open(true).show(ui, |ui| {
                    let mut r = node.joint_radius;
                    if ui.add(egui::Slider::new(&mut r, 0.02..=1.0).text(crate::tr!("Anzeigeradius", "Display radius"))).changed() {
                        if let Some(m) = self.scene.get_mut(lead) {
                            m.joint_radius = r;
                        }
                    }
                });
            }
            Kind::IkHandle => {
                egui::CollapsingHeader::new(crate::tr!("IK-Griff", "IK handle")).default_open(true).show(ui, |ui| {
                    if let Some(e) = node.ik_end.and_then(|e| self.scene.get(e)) {
                        ui.label(crate::trf!("Endgelenk: {}", "End joint: {}", e.name));
                    }
                    let mut len = node.ik_chain;
                    if ui.add(egui::Slider::new(&mut len, 1..=10).text(crate::tr!("Kettenlänge", "Chain length"))).changed() {
                        if let Some(m) = self.scene.get_mut(lead) {
                            m.ik_chain = len;
                        }
                    }
                    let mut on = node.visible;
                    if ui.checkbox(&mut on, crate::tr!("IK aktiv", "IK active")).changed() {
                        if let Some(m) = self.scene.get_mut(lead) {
                            m.visible = on;
                        }
                    }
                });
            }
            Kind::Group => {}
        }
        egui::CollapsingHeader::new(crate::tr!("Szene", "Scene")).default_open(false).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(crate::tr!("Hintergrund", "Background"));
                ui.color_edit_button_rgb(&mut self.scene.background);
            });
        });
    }
}
