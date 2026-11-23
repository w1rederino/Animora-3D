//! Application state, layout, menus, shortcuts and file handling.

use crate::anim::*;
use crate::math::*;
use crate::mesh::*;
use crate::panels::*;
use crate::render::*;
use crate::scene::*;
use crate::view::*;
use egui::{Color32, Key, Panel};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Select,
    Move,
    Rotate,
    Scale,
    Joint,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompMode {
    Object,
    Vertex,
    Face,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuSet {
    Modeling,
    Rigging,
    Animation,
    Rendering,
}

impl MenuSet {
    pub const ALL: [MenuSet; 4] = [MenuSet::Modeling, MenuSet::Rigging, MenuSet::Animation, MenuSet::Rendering];
    pub fn label(self) -> &'static str {
        match self {
            MenuSet::Modeling => crate::tr!("Modellieren", "Modeling"),
            MenuSet::Rigging => crate::tr!("Rigging", "Rigging"),
            MenuSet::Animation => crate::tr!("Animation", "Animation"),
            MenuSet::Rendering => crate::tr!("Rendern", "Render"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BottomTab {
    Graph,
    Dope,
    Script,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RightTab {
    Channels,
    Attributes,
}

pub struct Shot {
    pub path: PathBuf,
    pub requested: bool,
    pub done: Arc<AtomicBool>,
}

pub struct RenderState {
    pub width: u32,
    pub height: u32,
    pub antialias: bool,
    pub camera: Option<Id>,
    pub tex: Option<egui::TextureHandle>,
    pub last: Option<image::RgbaImage>,
    pub show: bool,
    pub seq: Option<(PathBuf, f32, f32)>,
    pub message: String,
}

pub struct App {
    pub scene: Scene,
    pub sel: Vec<Id>,
    pub tool: Tool,
    pub comp: CompMode,
    pub sel_verts: Vec<u32>,
    pub sel_faces: Vec<usize>,
    pub views: Vec<ViewCam>,
    pub quad_view: bool,
    pub active_view: usize,
    pub show_grid: bool,
    pub show_trail: bool,
    pub playing: bool,
    pub play_accum: f32,
    pub loop_play: bool,
    pub auto_key: bool,
    pub snap: bool,
    pub default_tangent: Tangent,
    pub undo: Vec<Scene>,
    pub redo: Vec<Scene>,
    pub drag: Option<DragState>,
    pub bottom_tab: BottomTab,
    pub show_bottom: bool,
    pub right_tab: RightTab,
    pub menu_set: MenuSet,
    pub graph: GraphState,
    pub dope: DopeState,
    pub outliner: OutlinerState,
    pub cmd: String,
    pub script: String,
    pub log: Vec<String>,
    pub status: String,
    pub file: Option<PathBuf>,
    pub render: RenderState,
    pub joint_chain_last: Option<Id>,
    pub shot: Option<Shot>,
    pub frames_drawn: u64,
    pub show_help: bool,
    pub extrude_dist: f32,
    pub snap_lock: bool,
    pub graph_hovered: bool,
    pub pending_render: bool,
    pub cod: crate::cod::CodState,
    pub textures: std::collections::HashMap<u64, TexEntry>,
    pub tex_failed: std::collections::HashSet<u64>,
    pub clip_ui: crate::clips::ClipUi,
}

pub struct TexEntry {
    pub image: Arc<image::RgbaImage>,
    pub handle: egui::TextureHandle,
}

impl App {
    /// Loads textures used by materials (once per path).
    pub fn sync_textures(&mut self, ctx: &egui::Context) {
        let paths: Vec<String> = self.scene.nodes.iter().filter_map(|n| n.material.texture.clone()).collect();
        for p in paths {
            let key = tex_key(&p);
            if self.textures.contains_key(&key) || self.tex_failed.contains(&key) {
                continue;
            }
            match image::open(&p) {
                Ok(img) => {
                    let rgba = img.to_rgba8();
                    let ci = egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw());
                    let handle = ctx.load_texture(format!("tex{key}"), ci, egui::TextureOptions::LINEAR_REPEAT);
                    self.textures.insert(key, TexEntry { image: Arc::new(rgba), handle });
                }
                Err(e) => {
                    self.tex_failed.insert(key);
                    self.log.push(crate::trf!("Textur nicht ladbar: {p} ({e})", "Texture could not be loaded: {p} ({e})"));
                }
            }
        }
    }

    pub fn tex_map(&self) -> TexMap {
        self.textures.iter().map(|(k, e)| (*k, e.image.clone())).collect()
    }

    /// Opens a scene file from a known path.
    pub fn open_path(&mut self, path: &std::path::Path) {
        match std::fs::read_to_string(path).map_err(|e| e.to_string()).and_then(|t| serde_json::from_str::<Scene>(&t).map_err(|e| e.to_string())) {
            Ok(s) => {
                self.checkpoint();
                self.scene = s;
                self.sel.clear();
                let f = self.scene.frame;
                self.scene.evaluate(f);
                self.status = crate::trf!("Geöffnet: {}", "Opened: {}", path.display());
                self.file = Some(path.to_path_buf());
            }
            Err(e) => self.status = crate::trf!("Öffnen fehlgeschlagen: {e}", "Opening failed: {e}"),
        }
    }
    pub fn new(ctx: &egui::Context, shot: Option<PathBuf>) -> App {
        setup_style(ctx);
        let mut views = vec![
            ViewCam::new(ViewKind::Persp),
            ViewCam::new(ViewKind::Top),
            ViewCam::new(ViewKind::Front),
            ViewCam::new(ViewKind::Side),
        ];
        views[0].target = v3(0.0, 1.5, -1.0);
        views[0].dist = 20.0;
        let mut scene = demo_scene();
        let ball = scene.nodes.iter().find(|n| n.name == "ball").map(|n| n.id);
        let start_frame = if shot.is_some() { 20.0 } else { 1.0 };
        scene.evaluate(start_frame);
        let mut app = App {
            scene,
            sel: ball.into_iter().collect(),
            tool: Tool::Move,
            comp: CompMode::Object,
            sel_verts: Vec::new(),
            sel_faces: Vec::new(),
            views,
            quad_view: false,
            active_view: 0,
            show_grid: true,
            show_trail: true,
            playing: false,
            play_accum: 0.0,
            loop_play: true,
            auto_key: false,
            snap: false,
            default_tangent: Tangent::Spline,
            undo: Vec::new(),
            redo: Vec::new(),
            drag: None,
            bottom_tab: BottomTab::Graph,
            show_bottom: true,
            right_tab: RightTab::Channels,
            menu_set: MenuSet::Animation,
            graph: GraphState::default(),
            dope: DopeState::default(),
            outliner: OutlinerState::default(),
            cmd: String::new(),
            script: crate::tr!("# Befehle, eine pro Zeile – z. B.:\nkugel\nbewegen 0 2 0\nkey\nbild 24\nbewegen 4 2 0\nkey", "# Commands, one per line – e.g.:\nsphere\nmove 0 2 0\nkey\nframe 24\nmove 4 2 0\nkey").into(),
            log: vec![crate::tr!("Willkommen in Animora 3D. Tippe 'hilfe' für alle Befehle.", "Welcome to Animora 3D. Type 'help' for all commands.").into()],
            status: crate::tr!("Alt+Linke Maus: drehen · Mittlere Maus: verschieben · Mausrad: zoomen · W/E/R: Werkzeuge · S: Schlüssel", "Alt+left mouse: orbit · Middle mouse: pan · Wheel: zoom · W/E/R: tools · S: set key").into(),
            file: None,
            render: RenderState {
                width: 960,
                height: 540,
                antialias: true,
                camera: None,
                tex: None,
                last: None,
                show: false,
                seq: None,
                message: String::new(),
            },
            joint_chain_last: None,
            shot: shot.map(|p| Shot { path: p, requested: false, done: Arc::new(AtomicBool::new(false)) }),
            frames_drawn: 0,
            show_help: false,
            extrude_dist: 0.5,
            snap_lock: false,
            graph_hovered: false,
            pending_render: false,
            cod: Default::default(),
            textures: Default::default(),
            tex_failed: Default::default(),
            clip_ui: Default::default(),
        };
        app.graph.fit_request = true;
        // the ball's roll (hundreds of degrees) would flatten the bounce curve
        app.graph.show[5] = false;
        // the example animation is the first entry of the animation list
        app.ensure_scene_clip();
        app
    }

    pub fn lead(&self) -> Option<Id> {
        self.sel.last().copied()
    }

    pub fn checkpoint(&mut self) {
        self.undo.push(self.scene.clone());
        if self.undo.len() > 150 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn do_undo(&mut self) {
        if let Some(s) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.scene, s));
            self.fix_selection();
            self.status = crate::tr!("Rückgängig", "Undo").into();
        }
    }

    pub fn do_redo(&mut self) {
        if let Some(s) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.scene, s));
            self.fix_selection();
            self.status = crate::tr!("Wiederholen", "Redo").into();
        }
    }

    pub fn fix_selection(&mut self) {
        let scene = &self.scene;
        self.sel.retain(|id| scene.get(*id).is_some());
        self.graph.sel.clear();
        if let Some(n) = self.lead().and_then(|l| self.scene.get(l)) {
            let nv = n.mesh.as_ref().map(|m| m.verts.len()).unwrap_or(0) as u32;
            let nf = n.mesh.as_ref().map(|m| m.faces.len()).unwrap_or(0);
            self.sel_verts.retain(|v| *v < nv);
            self.sel_faces.retain(|f| *f < nf);
        }
    }

    pub fn set_frame(&mut self, f: f32) {
        let f = f.round();
        self.scene.evaluate(f);
    }

    // ---------------- actions ----------------

    pub fn create_prim(&mut self, p: Prim) {
        self.checkpoint();
        let id = self.scene.add_prim(p);
        if p == Prim::Plane {
            if let Some(n) = self.scene.get_mut(id) {
                if let Some(pp) = &mut n.prim {
                    pp.size = 4.0;
                    n.mesh = Some(pp.build());
                }
            }
        } else if let Some(n) = self.scene.get_mut(id) {
            n.t.y = 1.0;
        }
        self.sel = vec![id];
        self.comp = CompMode::Object;
        if self.tool == Tool::Select || self.tool == Tool::Joint {
            self.tool = Tool::Move;
        }
        let name = self.scene.get(id).map(|n| n.name.clone()).unwrap_or_default();
        self.status = crate::trf!("{} erzeugt: {name}", "{} created: {name}", p.label());
    }

    pub fn create_node(&mut self, kind: Kind) {
        self.checkpoint();
        let base = match kind {
            Kind::Light => crate::tr!("licht", "light"),
            Kind::Camera => crate::tr!("kamera", "camera"),
            Kind::Group => crate::tr!("gruppe", "group"),
            Kind::Joint => crate::tr!("gelenk", "joint"),
            _ => crate::tr!("objekt", "object"),
        };
        let name = self.scene.unique_name(base);
        let mut n = Node::new(0, &name, kind);
        match kind {
            Kind::Light => {
                n.t = v3(0.0, 5.0, 3.0);
                n.r = v3(-45.0, 0.0, 0.0);
            }
            Kind::Camera => {
                // place the camera where the perspective view looks from
                let w = self.scene.world_matrices();
                let (m, _, _) = self.views[0].camera(&self.scene, &w);
                let (t, r, _) = m.decompose();
                n.t = t;
                n.r = r;
            }
            _ => {}
        }
        let id = self.scene.add(n);
        if kind == Kind::Group {
            for s in self.top_selection() {
                self.scene.set_parent(s, Some(id));
            }
        }
        self.sel = vec![id];
        self.status = crate::trf!("{} erzeugt: {name}", "{} created: {name}", kind.label());
    }

    pub fn delete_selection(&mut self) {
        if self.comp == CompMode::Face && !self.sel_faces.is_empty() {
            if let Some(l) = self.lead() {
                self.checkpoint();
                let faces = self.sel_faces.clone();
                if let Some(n) = self.scene.get_mut(l) {
                    if let Some(m) = &mut n.mesh {
                        m.delete_faces(&faces);
                        n.prim = None;
                        n.skin = None;
                    }
                }
                self.sel_faces.clear();
                self.status = crate::trf!("{} Flächen gelöscht", "{} faces deleted", faces.len());
            }
            return;
        }
        if self.sel.is_empty() {
            return;
        }
        self.checkpoint();
        let ids = self.sel.clone();
        self.scene.delete(&ids);
        self.sel.clear();
        self.comp = CompMode::Object;
        self.status = crate::trf!("{} Objekt(e) gelöscht", "{} object(s) deleted", ids.len());
    }

    pub fn duplicate_selection(&mut self) {
        if self.sel.is_empty() {
            return;
        }
        self.checkpoint();
        let mut new = Vec::new();
        for id in self.top_selection() {
            if let Some(n) = self.scene.duplicate(id) {
                if let Some(node) = self.scene.get_mut(n) {
                    node.t.x += 1.0;
                }
                new.push(n);
            }
        }
        self.sel = new;
        self.status = crate::tr!("Dupliziert", "Duplicated").into();
    }

    pub fn parent_selection(&mut self) {
        if self.sel.len() < 2 {
            self.status = crate::tr!("Zum Verknüpfen: Kinder auswählen, dann zuletzt das Elternobjekt (P).", "To parent: select the children, then the parent last (P).").into();
            return;
        }
        self.checkpoint();
        let parent = *self.sel.last().unwrap();
        for &c in &self.sel[..self.sel.len() - 1] {
            self.scene.set_parent(c, Some(parent));
        }
        self.status = crate::tr!("Hierarchie verknüpft", "Parented").into();
    }

    pub fn unparent_selection(&mut self) {
        if self.sel.is_empty() {
            return;
        }
        self.checkpoint();
        for &c in &self.sel.clone() {
            self.scene.set_parent(c, None);
        }
        self.status = crate::tr!("Aus Hierarchie gelöst", "Unparented").into();
    }

    /// Sets keys on the given channels for every selected node.
    pub fn key_selection(&mut self, channels: std::ops::Range<usize>) {
        if self.sel.is_empty() {
            self.status = crate::tr!("Nichts ausgewählt – kein Schlüssel gesetzt.", "Nothing selected – no key set.").into();
            return;
        }
        self.checkpoint();
        let f = self.scene.frame;
        let tan = self.default_tangent;
        for &id in &self.sel.clone() {
            if let Some(n) = self.scene.get_mut(id) {
                for c in channels.clone() {
                    n.set_key(c, f, tan);
                }
            }
        }
        self.status = crate::trf!("Schlüssel gesetzt bei Bild {}", "Key set at frame {}", f as i64);
    }

    /// Auto-key: re-key animated channels after an edit.
    pub fn auto_key_selection(&mut self) {
        if !self.auto_key {
            return;
        }
        let f = self.scene.frame;
        let tan = self.default_tangent;
        for &id in &self.sel.clone() {
            if let Some(n) = self.scene.get_mut(id) {
                let animated = n.is_animated();
                for c in 0..9 {
                    if !n.curves[c].is_empty() || !animated {
                        n.set_key(c, f, tan);
                    }
                }
            }
        }
    }

    pub fn delete_keys_at_frame(&mut self) {
        self.checkpoint();
        let f = self.scene.frame;
        for &id in &self.sel.clone() {
            if let Some(n) = self.scene.get_mut(id) {
                for c in &mut n.curves {
                    c.remove_key_at(f);
                }
            }
        }
        self.status = crate::trf!("Schlüssel bei Bild {} gelöscht", "Keys at frame {} deleted", f as i64);
    }

    pub fn step_key(&mut self, dir: f32) {
        let ids = if self.sel.is_empty() { self.scene.nodes.iter().map(|n| n.id).collect() } else { self.sel.clone() };
        let keys = self.scene.key_frames(&ids);
        let f = self.scene.frame;
        let next = if dir > 0.0 {
            keys.iter().copied().find(|k| *k > f + 0.01)
        } else {
            keys.iter().rev().copied().find(|k| *k < f - 0.01)
        };
        if let Some(k) = next {
            self.set_frame(k);
        }
    }

    pub fn extrude(&mut self) {
        let Some(l) = self.lead() else { return };
        if self.comp != CompMode::Face || self.sel_faces.is_empty() {
            self.status = crate::tr!("Extrudieren: erst in den Flächen-Modus (F11) und Flächen wählen.", "Extrude: switch to face mode (F11) and select faces first.").into();
            return;
        }
        self.checkpoint();
        let faces = self.sel_faces.clone();
        let d = self.extrude_dist;
        if let Some(n) = self.scene.get_mut(l) {
            if let Some(m) = &mut n.mesh {
                self.sel_faces = m.extrude_faces(&faces, d);
                n.prim = None;
                n.skin = None;
            }
        }
        self.tool = Tool::Move;
        self.status = crate::trf!("{} Fläche(n) extrudiert – mit dem Bewegen-Werkzeug weiterziehen.", "{} face(s) extruded – pull them further with the move tool.", faces.len());
    }

    pub fn smooth_mesh(&mut self) {
        self.checkpoint();
        for &id in &self.sel.clone() {
            if let Some(n) = self.scene.get_mut(id) {
                if let Some(m) = &mut n.mesh {
                    if m.faces.len() < 20000 {
                        *m = m.subdivide();
                        n.prim = None;
                        n.skin = None;
                    }
                }
            }
        }
        self.sel_verts.clear();
        self.sel_faces.clear();
        self.status = crate::tr!("Geglättet (Unterteilung)", "Smoothed (subdivided)").into();
    }

    pub fn relax_mesh(&mut self) {
        self.checkpoint();
        for &id in &self.sel.clone() {
            if let Some(n) = self.scene.get_mut(id) {
                if let Some(m) = &mut n.mesh {
                    m.relax(0.5);
                    n.prim = None;
                }
            }
        }
    }

    pub fn freeze_selection(&mut self) {
        self.checkpoint();
        for &id in &self.sel.clone() {
            self.scene.freeze(id);
        }
        self.status = crate::tr!("Transformationen eingefroren", "Transformations frozen").into();
    }

    pub fn create_ik(&mut self) {
        let joints: Vec<Id> = self.sel.iter().copied().filter(|id| self.scene.get(*id).is_some_and(|n| n.kind == Kind::Joint)).collect();
        if joints.len() < 2 {
            self.status = crate::tr!("IK-Griff: Startgelenk und dann Endgelenk auswählen.", "IK handle: select the start joint, then the end joint.").into();
            return;
        }
        let start = joints[0];
        let end = *joints.last().unwrap();
        if !self.scene.is_ancestor(start, end) {
            self.status = crate::tr!("IK-Griff: Das Endgelenk muss unterhalb des Startgelenks liegen.", "IK handle: the end joint must be below the start joint.").into();
            return;
        }
        let mut len = 0;
        let mut cur = self.scene.get(end).and_then(|n| n.parent);
        while let Some(c) = cur {
            len += 1;
            if c == start {
                break;
            }
            cur = self.scene.get(c).and_then(|n| n.parent);
        }
        self.checkpoint();
        let w = self.scene.world_matrices();
        let name = self.scene.unique_name(crate::tr!("ikGriff", "ikHandle"));
        let mut h = Node::new(0, &name, Kind::IkHandle);
        h.t = w[&end].translation();
        h.ik_end = Some(end);
        h.ik_chain = len;
        let id = self.scene.add(h);
        self.sel = vec![id];
        self.tool = Tool::Move;
        self.status = crate::trf!("{name} erzeugt (Kette mit {len} Gelenken) – bewege den Griff.", "{name} created (chain of {len} joints) – move the handle.");
    }

    pub fn bind_skin(&mut self) {
        let joints: Vec<Id> = self.sel.iter().copied().filter(|id| self.scene.get(*id).is_some_and(|n| n.kind == Kind::Joint)).collect();
        let meshes: Vec<Id> = self.sel.iter().copied().filter(|id| self.scene.get(*id).is_some_and(|n| n.kind == Kind::Mesh)).collect();
        if joints.is_empty() || meshes.is_empty() {
            self.status = crate::tr!("Haut binden: Gelenke (oder das Wurzelgelenk) und Objekte auswählen.", "Bind skin: select joints (or the root joint) and objects.").into();
            return;
        }
        // use whole joint hierarchies below the chosen joints
        let mut all = Vec::new();
        for j in joints {
            for d in self.scene.subtree(j) {
                if self.scene.get(d).is_some_and(|n| n.kind == Kind::Joint) && !all.contains(&d) {
                    all.push(d);
                }
            }
        }
        self.checkpoint();
        for m in &meshes {
            self.scene.bind_skin(*m, &all);
        }
        self.status = crate::trf!("{} Objekt(e) an {} Gelenke gebunden", "{} object(s) bound to {} joints", meshes.len(), all.len());
    }

    pub fn unbind_skin(&mut self) {
        self.checkpoint();
        for &id in &self.sel.clone() {
            if let Some(n) = self.scene.get_mut(id) {
                n.skin = None;
            }
        }
        self.status = crate::tr!("Bindung gelöst", "Skin unbound").into();
    }

    // ---------------- files ----------------

    pub fn new_scene(&mut self) {
        self.checkpoint();
        self.scene = Scene::default();
        self.sel.clear();
        self.file = None;
        self.status = crate::tr!("Neue Szene", "New scene").into();
    }

    pub fn save(&mut self, as_new: bool) {
        let path = if as_new || self.file.is_none() {
            rfd::FileDialog::new().add_filter(crate::tr!("Animora-Szene", "Animora scene"), &["anim3d"]).set_file_name(crate::tr!("szene.anim3d", "scene.anim3d")).save_file()
        } else {
            self.file.clone()
        };
        let Some(path) = path else { return };
        // keep edits of the active animation clip in the file
        self.clip_capture();
        match serde_json::to_string(&self.scene) {
            Ok(json) => match std::fs::write(&path, json) {
                Ok(_) => {
                    self.status = crate::trf!("Gespeichert: {}", "Saved: {}", path.display());
                    self.file = Some(path);
                }
                Err(e) => self.status = crate::trf!("Speichern fehlgeschlagen: {e}", "Saving failed: {e}"),
            },
            Err(e) => self.status = crate::trf!("Speichern fehlgeschlagen: {e}", "Saving failed: {e}"),
        }
    }

    pub fn open(&mut self) {
        let Some(path) = rfd::FileDialog::new().add_filter(crate::tr!("Animora-Szene", "Animora scene"), &["anim3d"]).pick_file() else { return };
        match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| serde_json::from_str::<Scene>(&t).map_err(|e| e.to_string())) {
            Ok(s) => {
                self.checkpoint();
                self.scene = s;
                self.sel.clear();
                let f = self.scene.frame;
                self.scene.evaluate(f);
                self.status = crate::trf!("Geöffnet: {}", "Opened: {}", path.display());
                self.file = Some(path);
            }
            Err(e) => self.status = crate::trf!("Öffnen fehlgeschlagen: {e}", "Opening failed: {e}"),
        }
    }

    pub fn export_obj(&mut self) {
        let Some(path) = rfd::FileDialog::new().add_filter("OBJ", &["obj"]).set_file_name(crate::tr!("szene.obj", "scene.obj")).save_file() else { return };
        match std::fs::write(&path, self.scene.to_obj()) {
            Ok(_) => self.status = crate::trf!("OBJ exportiert: {}", "OBJ exported: {}", path.display()),
            Err(e) => self.status = crate::trf!("Export fehlgeschlagen: {e}", "Export failed: {e}"),
        }
    }

    pub fn import_obj(&mut self) {
        let Some(path) = rfd::FileDialog::new().add_filter("OBJ", &["obj"]).pick_file() else { return };
        match std::fs::read_to_string(&path) {
            Ok(t) => {
                let mesh = Mesh::from_obj(&t);
                if mesh.faces.is_empty() {
                    self.status = crate::tr!("Keine Flächen in der OBJ-Datei gefunden.", "No faces found in the OBJ file.").into();
                    return;
                }
                self.checkpoint();
                let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or("import".into());
                let name = self.scene.unique_name(&stem);
                let mut n = Node::new(0, &name, Kind::Mesh);
                n.mesh = Some(mesh);
                let id = self.scene.add(n);
                self.sel = vec![id];
                self.status = crate::trf!("Importiert: {name}", "Imported: {name}");
            }
            Err(e) => self.status = crate::trf!("Import fehlgeschlagen: {e}", "Import failed: {e}"),
        }
    }

    // ---------------- rendering ----------------

    pub fn render_camera(&self) -> (M4, f32, bool) {
        let w = self.scene.world_matrices();
        if let Some(c) = self.render.camera.and_then(|c| self.scene.get(c)) {
            let m = w[&c.id];
            let mut r = m.rotation_only();
            r.set_col(3, m.translation());
            return (r, c.fov, false);
        }
        let (m, ortho, f) = self.views[0].camera(&self.scene, &w);
        (m, f, ortho)
    }

    pub fn render_frame(&mut self, ctx: &egui::Context) {
        let (m, fov, ortho) = self.render_camera();
        let set = RenderSettings { width: self.render.width, height: self.render.height, antialias: self.render.antialias };
        let t0 = std::time::Instant::now();
        let img = render_image(&self.scene, m, fov, ortho, &set, &self.tex_map());
        let ci = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
        self.render.tex = Some(ctx.load_texture("render", ci, egui::TextureOptions::LINEAR));
        self.render.last = Some(img);
        self.render.show = true;
        self.render.message = crate::trf!("Bild {} gerendert in {:.2} s", "Frame {} rendered in {:.2} s", self.scene.frame as i64, t0.elapsed().as_secs_f32());
    }

    pub fn save_render(&mut self) {
        let Some(img) = &self.render.last else { return };
        let Some(path) = rfd::FileDialog::new().add_filter("PNG", &["png"]).set_file_name("render.png").save_file() else { return };
        self.render.message = match img.save(&path) {
            Ok(_) => crate::trf!("Gespeichert: {}", "Saved: {}", path.display()),
            Err(e) => crate::trf!("Fehler: {e}", "Error: {e}"),
        };
    }

    pub fn start_sequence(&mut self) {
        let Some(dir) = rfd::FileDialog::new().pick_folder() else { return };
        self.render.seq = Some((dir, self.scene.start, self.scene.end));
        self.render.show = true;
        self.playing = false;
    }

    fn step_sequence(&mut self, ctx: &egui::Context) {
        let Some((dir, f, end)) = self.render.seq.clone() else { return };
        self.set_frame(f);
        self.render_frame(ctx);
        if let Some(img) = &self.render.last {
            let path = dir.join(crate::trf!("bild_{:04}.png", "frame_{:04}.png", f as i64));
            if let Err(e) = img.save(&path) {
                self.render.message = crate::trf!("Fehler: {e}", "Error: {e}");
                self.render.seq = None;
                return;
            }
        }
        if f + 1.0 > end {
            self.render.message = crate::trf!("Sequenz fertig: {} Bilder in {}", "Sequence done: {} frames in {}", (end - self.scene.start + 1.0) as i64, dir.display());
            self.render.seq = None;
        } else {
            self.render.message = crate::trf!("Rendere Bild {} von {} …", "Rendering frame {} of {} …", f as i64, end as i64);
            self.render.seq = Some((dir, f + 1.0, end));
            ctx.request_repaint();
        }
    }

    // ---------------- input ----------------

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let (mods, pressed): (egui::Modifiers, Vec<Key>) = ctx.input(|i| {
            let keys = i
                .events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Key { key, pressed: true, repeat: false, .. } => Some(*key),
                    _ => None,
                })
                .collect();
            (i.modifiers, keys)
        });
        self.snap = ctx.input(|i| i.key_down(Key::X)) || self.snap_lock;
        for k in pressed {
            match (k, mods.command, mods.shift, mods.alt) {
                (Key::Z, true, false, _) => self.do_undo(),
                (Key::Z, true, true, _) | (Key::Y, true, _, _) => self.do_redo(),
                (Key::S, true, _, _) => self.save(mods.shift),
                (Key::O, true, _, _) => self.open(),
                (Key::N, true, _, _) => self.new_scene(),
                (Key::D, true, _, _) => self.duplicate_selection(),
                (Key::G, true, _, _) => self.create_node(Kind::Group),
                (Key::E, true, _, _) => self.extrude(),
                (Key::A, true, _, _) => self.sel = self.scene.nodes.iter().map(|n| n.id).collect(),
                (Key::Q, false, _, false) => self.tool = Tool::Select,
                (Key::W, false, true, _) => self.key_selection(0..3),
                (Key::E, false, true, _) => self.key_selection(3..6),
                (Key::R, false, true, _) => self.key_selection(6..9),
                (Key::W, false, false, _) => self.tool = Tool::Move,
                (Key::E, false, false, _) => self.tool = Tool::Rotate,
                (Key::R, false, false, _) => self.tool = Tool::Scale,
                (Key::S, false, false, false) => self.key_selection(0..9),
                (Key::V, false, false, true) => self.playing = !self.playing,
                (Key::P, false, false, _) => self.parent_selection(),
                (Key::P, false, true, _) => self.unparent_selection(),
                (Key::F, false, false, _) => self.frame_view(false),
                (Key::A, false, false, false) => self.frame_view(true),
                (Key::Delete, _, _, _) | (Key::Backspace, _, _, _) => {
                    if !self.graph_hovered {
                        self.delete_selection()
                    }
                }
                (Key::Space, false, false, false) => self.quad_view = !self.quad_view,
                (Key::Num1, false, false, false) => self.set_smooth(false),
                (Key::Num3, false, false, false) => self.set_smooth(true),
                (Key::Num4, false, false, false) => self.views[self.active_view].display = Display::Wire,
                (Key::Num5, false, false, false) => self.views[self.active_view].display = Display::Shaded,
                (Key::Num6, false, false, false) => self.views[self.active_view].display = Display::ShadedWire,
                (Key::Num7, false, false, false) => {
                    let v = &mut self.views[self.active_view];
                    v.scene_lights = !v.scene_lights;
                }
                (Key::F8, _, _, _) => self.toggle_component(CompMode::Vertex),
                (Key::F9, _, _, _) => self.toggle_component(CompMode::Vertex),
                (Key::F11, _, _, _) => self.toggle_component(CompMode::Face),
                (Key::Period, false, false, false) => self.step_key(1.0),
                (Key::Comma, false, false, false) => self.step_key(-1.0),
                (Key::Period, false, false, true) => self.set_frame(self.scene.frame + 1.0),
                (Key::Comma, false, false, true) => self.set_frame(self.scene.frame - 1.0),
                (Key::Enter, _, _, _) | (Key::Escape, _, _, _) => {
                    if self.tool == Tool::Joint {
                        self.tool = Tool::Select;
                        self.joint_chain_last = None;
                        self.status = crate::tr!("Gelenkkette beendet", "Joint chain finished").into();
                    } else if k == Key::Escape {
                        self.comp = CompMode::Object;
                    }
                }
                (Key::F1, _, _, _) => self.show_help = !self.show_help,
                _ => {}
            }
        }
    }

    pub fn toggle_component(&mut self, m: CompMode) {
        let is_mesh = self.lead().and_then(|l| self.scene.get(l)).is_some_and(|n| n.kind == Kind::Mesh);
        if self.comp == m || !is_mesh {
            self.comp = CompMode::Object;
            if !is_mesh && m != CompMode::Object {
                self.status = crate::tr!("Komponenten-Modus: zuerst ein Polygon-Objekt wählen.", "Component mode: select a polygon object first.").into();
            }
        } else {
            self.comp = m;
            self.sel.truncate(self.sel.len());
            if let Some(l) = self.lead() {
                self.sel = vec![l];
            }
            if self.scene.lead_is_skinned(self.lead()) {
                self.status = crate::tr!("Hinweis: Gebundene Objekte zuerst lösen, bevor man Punkte verschiebt.", "Note: unbind skinned objects before moving their vertices.").into();
            }
        }
        self.sel_verts.clear();
        self.sel_faces.clear();
    }

    pub fn set_smooth(&mut self, on: bool) {
        for &id in &self.sel.clone() {
            if let Some(n) = self.scene.get_mut(id) {
                n.smooth_preview = on;
            }
        }
    }

    fn advance_playback(&mut self, ctx: &egui::Context) {
        if !self.playing {
            return;
        }
        let dt = ctx.input(|i| i.stable_dt).min(0.1);
        self.play_accum += dt * self.scene.fps;
        let mut f = self.scene.frame;
        while self.play_accum >= 1.0 {
            self.play_accum -= 1.0;
            f += 1.0;
            if f > self.scene.end {
                if self.loop_play {
                    f = self.scene.start;
                } else {
                    f = self.scene.end;
                    self.playing = false;
                }
            }
        }
        if f != self.scene.frame {
            self.scene.evaluate(f);
        }
        ctx.request_repaint();
    }
}

impl Scene {
    pub fn lead_is_skinned(&self, id: Option<Id>) -> bool {
        id.and_then(|i| self.get(i)).is_some_and(|n| n.skin.is_some())
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.frames_drawn += 1;
        if let Some(shot) = &mut self.shot {
            if self.frames_drawn == 1 {
                ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaintWhileHidden);
            }
            if shot.done.load(Ordering::SeqCst) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else if self.frames_drawn >= 120 && !shot.requested {
                shot.requested = true;
                let path = shot.path.clone();
                let done = shot.done.clone();
                let c2 = ctx.clone();
                ctx.request_screenshot(move |img| {
                    let [w, h] = img.size;
                    let mut buf = image::RgbaImage::new(w as u32, h as u32);
                    for (i, px) in img.pixels.iter().enumerate() {
                        let [r, g, b, a] = px.to_srgba_unmultiplied();
                        buf.put_pixel((i % w) as u32, (i / w) as u32, image::Rgba([r, g, b, a]));
                    }
                    if let Some(parent) = path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    if let Err(e) = buf.save(&path) {
                        eprintln!("{}", crate::trf!("Bildschirmfoto fehlgeschlagen: {e}", "Screenshot failed: {e}"));
                    }
                    done.store(true, Ordering::SeqCst);
                    c2.request_repaint();
                });
            }
            ctx.request_repaint();
        }

        self.handle_dropped_files(&ctx);
        self.sync_textures(&ctx);
        // animation in the scene always shows up in the animation list
        self.ensure_scene_clip();
        self.shortcuts(&ctx);
        self.advance_playback(&ctx);
        if self.render.seq.is_some() {
            self.step_sequence(&ctx);
        }
        // IK follows its handles continuously
        self.scene.solve_ik();

        Panel::top("menu").show(ui, |ui| self.menu_bar(ui));
        Panel::top("status").show(ui, |ui| self.status_line(ui));
        Panel::top("shelf").show(ui, |ui| self.shelf(ui));
        Panel::bottom("help").exact_size(24.0).show(ui, |ui| self.command_line(ui));
        Panel::bottom("time").show(ui, |ui| self.time_slider(ui));
        if self.show_bottom {
            Panel::bottom("editors").resizable(true).default_size(240.0).min_size(120.0).show(ui, |ui| self.bottom_editors(ui));
        }
        Panel::left("toolbox").exact_size(42.0).resizable(false).show(ui, |ui| self.toolbox(ui));
        Panel::left("outliner").resizable(true).default_size(235.0).min_size(120.0).show(ui, |ui| self.outliner_panel(ui));
        Panel::right("channels").resizable(true).default_size(300.0).min_size(220.0).show(ui, |ui| self.right_panel(ui));
        egui::CentralPanel::no_frame().show(ui, |ui| {
            let rect = ui.max_rect();
            if self.quad_view {
                let c = rect.center();
                let rects = [
                    egui::Rect::from_min_max(rect.min, c - egui::vec2(1.0, 1.0)),
                    egui::Rect::from_min_max(egui::pos2(c.x + 1.0, rect.min.y), egui::pos2(rect.max.x, c.y - 1.0)),
                    egui::Rect::from_min_max(egui::pos2(rect.min.x, c.y + 1.0), egui::pos2(c.x - 1.0, rect.max.y)),
                    egui::Rect::from_min_max(c + egui::vec2(1.0, 1.0), rect.max),
                ];
                ui.painter().rect_filled(rect, 0.0, Color32::from_gray(30));
                // order: top, persp / front, side
                let order = [1usize, 0, 2, 3];
                for (r, vi) in rects.iter().zip(order) {
                    self.viewport(ui, vi, *r);
                }
            } else {
                let vi = self.active_view;
                self.viewport(ui, vi, rect);
            }
        });
        self.windows(&ctx);
        self.cod_window(&ctx);
    }
}

pub fn setup_style(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = Color32::from_gray(68);
    v.window_fill = Color32::from_gray(60);
    v.extreme_bg_color = Color32::from_gray(43);
    v.faint_bg_color = Color32::from_gray(74);
    v.selection.bg_fill = Color32::from_rgb(82, 133, 166);
    v.widgets.noninteractive.bg_fill = Color32::from_gray(68);
    v.widgets.inactive.bg_fill = Color32::from_gray(93);
    v.widgets.inactive.weak_bg_fill = Color32::from_gray(88);
    v.widgets.hovered.bg_fill = Color32::from_gray(110);
    v.widgets.hovered.weak_bg_fill = Color32::from_gray(105);
    v.widgets.active.bg_fill = Color32::from_rgb(82, 133, 166);
    v.widgets.noninteractive.fg_stroke.color = Color32::from_gray(210);
    v.widgets.inactive.fg_stroke.color = Color32::from_gray(225);
    // Always dark, whatever the system theme is.
    ctx.set_theme(egui::Theme::Dark);
    ctx.set_visuals_of(egui::Theme::Dark, v.clone());
    ctx.set_visuals_of(egui::Theme::Light, v);
    for theme in [egui::Theme::Dark, egui::Theme::Light] {
        ctx.style_mut_of(theme, |s| {
            s.spacing.item_spacing = egui::vec2(6.0, 4.0);
            s.spacing.button_padding = egui::vec2(6.0, 2.0);
            s.spacing.interact_size.y = 20.0;
        });
    }
}
