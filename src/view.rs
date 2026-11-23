//! The 3D viewports: camera navigation, drawing, picking and manipulators.

use crate::app::*;
use crate::math::*;
use crate::render::*;
use crate::scene::*;
use egui::{Color32, Pos2, Rect, Sense, Stroke, pos2, vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewKind {
    Persp,
    Top,
    Front,
    Side,
}

impl ViewKind {
    pub fn label(self) -> &'static str {
        match self {
            ViewKind::Persp => crate::tr!("Perspektive", "Perspective"),
            ViewKind::Top => crate::tr!("Oben", "Top"),
            ViewKind::Front => crate::tr!("Vorne", "Front"),
            ViewKind::Side => crate::tr!("Seite", "Side"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Display {
    Shaded,
    Wire,
    ShadedWire,
}

#[derive(Clone, Debug)]
pub struct ViewCam {
    pub kind: ViewKind,
    pub target: V3,
    pub yaw: f32,
    pub pitch: f32,
    pub dist: f32,
    pub half: f32,
    pub fov: f32,
    pub look_through: Option<Id>,
    pub display: Display,
    pub scene_lights: bool,
}

impl ViewCam {
    pub fn new(kind: ViewKind) -> ViewCam {
        ViewCam {
            kind,
            target: v3(0.0, 1.0, 0.0),
            yaw: 38f32.to_radians(),
            pitch: 24f32.to_radians(),
            dist: 18.0,
            half: 7.0,
            fov: 45.0,
            look_through: None,
            display: Display::Shaded,
            scene_lights: true,
        }
    }

    /// Camera-to-world matrix, orthographic flag and fov (or ortho half size).
    pub fn camera(&self, scene: &Scene, w: &Worlds) -> (M4, bool, f32) {
        if let Some(c) = self.look_through {
            if let (Some(m), Some(n)) = (w.get(&c), scene.get(c)) {
                let mut r = m.rotation_only();
                r.set_col(3, m.translation());
                return (r, false, n.fov);
            }
        }
        match self.kind {
            ViewKind::Persp => {
                let dir = v3(self.pitch.cos() * self.yaw.sin(), self.pitch.sin(), self.pitch.cos() * self.yaw.cos());
                (M4::look_at(self.target + dir * self.dist, self.target, V3::Y), false, self.fov)
            }
            ViewKind::Top => (M4::look_at(self.target + V3::Y * 200.0, self.target, -V3::Z), true, self.half),
            ViewKind::Front => (M4::look_at(self.target + V3::Z * 200.0, self.target, V3::Y), true, self.half),
            ViewKind::Side => (M4::look_at(self.target + V3::X * 200.0, self.target, V3::Y), true, self.half),
        }
    }

    pub fn label(&self, scene: &Scene) -> String {
        if let Some(c) = self.look_through.and_then(|c| scene.get(c)) {
            return crate::trf!("Kamera: {}", "Camera: {}", c.name);
        }
        self.kind.label().to_string()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DragKind {
    Gizmo(usize),
    Box,
}

#[derive(Clone)]
pub struct NodeStart {
    pub id: Id,
    pub world_pos: V3,
    pub parent_inv: M4,
    pub r: V3,
    pub s: V3,
}

#[derive(Clone)]
pub struct DragState {
    pub kind: DragKind,
    pub view: usize,
    pub start_mouse: Pos2,
    pub center: V3,
    pub nodes: Vec<NodeStart>,
    pub verts: Vec<V3>,
    pub mesh_world: M4,
    pub proj: Proj,
}

const AXIS_COLORS: [Color32; 3] = [
    Color32::from_rgb(230, 70, 70),
    Color32::from_rgb(90, 210, 90),
    Color32::from_rgb(80, 130, 255),
];
const SEL_COLOR: Color32 = Color32::from_rgb(120, 255, 170);
const LEAD_COLOR: Color32 = Color32::from_rgb(255, 255, 255);
const HANDLE_HOVER: Color32 = Color32::from_rgb(255, 230, 60);

fn p2(s: (f32, f32, f32)) -> Pos2 {
    pos2(s.0, s.1)
}

fn dist_seg(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let l2 = ab.length_sq();
    if l2 < 1e-6 {
        return (p - a).length();
    }
    let t = ((p - a).dot(ab) / l2).clamp(0.0, 1.0);
    (p - (a + ab * t)).length()
}

impl App {
    pub fn proj_for(&self, vi: usize, rect: Rect, w: &Worlds) -> Proj {
        let (m, ortho, f) = self.views[vi].camera(&self.scene, w);
        let mut p = Proj::new(m, ortho, f, rect.min.x, rect.min.y, rect.width(), rect.height());
        if ortho {
            p.near = -1e6;
        }
        p
    }

    /// Centre of the current selection in world space (objects or components).
    pub fn selection_center(&self, w: &Worlds) -> Option<V3> {
        if self.comp != CompMode::Object {
            let lead = self.lead()?;
            let n = self.scene.get(lead)?;
            let mesh = n.mesh.as_ref()?;
            let verts = self.scene.deformed_verts(n, mesh, w);
            let ids = self.component_verts(mesh);
            if ids.is_empty() {
                return None;
            }
            let mut c = V3::ZERO;
            for &i in &ids {
                c += verts[i as usize];
            }
            return Some(c / ids.len() as f32);
        }
        let lead = self.lead()?;
        w.get(&lead).map(|m| m.translation())
    }

    /// Vertex indices touched by the component selection.
    pub fn component_verts(&self, mesh: &crate::mesh::Mesh) -> Vec<u32> {
        let mut ids: Vec<u32> = match self.comp {
            CompMode::Vertex => self.sel_verts.clone(),
            CompMode::Face => self
                .sel_faces
                .iter()
                .filter_map(|&f| mesh.faces.get(f))
                .flat_map(|f| f.iter().copied())
                .collect(),
            CompMode::Object => Vec::new(),
        };
        ids.sort_unstable();
        ids.dedup();
        ids.retain(|&i| (i as usize) < mesh.verts.len());
        ids
    }

    fn gizmo_handles(&self, proj: &Proj, c: V3) -> Vec<(usize, Vec<Pos2>)> {
        let mut out = Vec::new();
        let Some(sc) = proj.project(c) else { return out };
        let wpp = proj.world_per_pixel(c);
        match self.tool {
            Tool::Move | Tool::Scale => {
                for a in 0..3 {
                    if let Some(e) = proj.project(c + V3::axis(a) * wpp * 85.0) {
                        out.push((a, vec![p2(sc), p2(e)]));
                    }
                }
                out.push((3, vec![p2(sc)]));
            }
            Tool::Rotate => {
                for a in 0..3 {
                    let (u, v) = match a {
                        0 => (V3::Y, V3::Z),
                        1 => (V3::Z, V3::X),
                        _ => (V3::X, V3::Y),
                    };
                    let r = wpp * 75.0;
                    let pts: Vec<Pos2> = (0..=48)
                        .filter_map(|i| {
                            let t = std::f32::consts::TAU * i as f32 / 48.0;
                            proj.project(c + (u * t.cos() + v * t.sin()) * r).map(p2)
                        })
                        .collect();
                    out.push((a, pts));
                }
            }
            _ => {}
        }
        out
    }

    fn hit_gizmo(&self, proj: &Proj, c: V3, m: Pos2) -> Option<usize> {
        let mut best: Option<(usize, f32)> = None;
        for (a, pts) in self.gizmo_handles(proj, c) {
            let d = if a == 3 {
                (pts[0] - m).length() - 4.0
            } else {
                pts.windows(2).map(|w| dist_seg(m, w[0], w[1])).fold(f32::MAX, f32::min)
            };
            if d < 8.0 && best.is_none_or(|b| d < b.1) {
                best = Some((a, d));
            }
        }
        best.map(|b| b.0)
    }

    /// Picks the object under the mouse.
    fn pick_object(&self, proj: &Proj, w: &Worlds, m: Pos2) -> Option<Id> {
        // small helpers first (joints, lights, cameras, handles)
        let mut best: Option<(Id, f32)> = None;
        for n in &self.scene.nodes {
            if n.kind == Kind::Mesh || n.kind == Kind::Group || !self.scene.is_visible(n.id) {
                continue;
            }
            if let Some(s) = w.get(&n.id).and_then(|mm| proj.project(mm.translation())) {
                let d = (p2(s) - m).length();
                if d < 12.0 && best.is_none_or(|b| d < b.1) {
                    best = Some((n.id, d));
                }
            }
        }
        if let Some(b) = best {
            return Some(b.0);
        }
        let (o, d) = proj.ray(m.x, m.y);
        let mut hit: Option<(Id, f32)> = None;
        for t in self.scene.triangles(w) {
            if let Some(dist) = ray_tri(o, d, t.p[0], t.p[1], t.p[2]) {
                if hit.is_none_or(|h| dist < h.1) {
                    hit = Some((t.node, dist));
                }
            }
        }
        hit.map(|h| h.0)
    }

    fn select_click(&mut self, id: Option<Id>, shift: bool, ctrl: bool) {
        match id {
            Some(id) => {
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
                }
            }
            None => {
                if !shift && !ctrl {
                    self.sel.clear();
                }
            }
        }
    }

    fn pick_component(&mut self, proj: &Proj, w: &Worlds, m: Pos2, shift: bool, rect_sel: Option<Rect>) {
        let Some(lead) = self.lead() else { return };
        let Some(n) = self.scene.get(lead) else { return };
        let Some(mesh) = n.mesh.clone() else { return };
        let verts = self.scene.deformed_verts(n, &mesh, w);
        match self.comp {
            CompMode::Vertex => {
                let mut picked: Vec<u32> = Vec::new();
                if let Some(r) = rect_sel {
                    for (i, v) in verts.iter().enumerate() {
                        if let Some(s) = proj.project(*v) {
                            if r.contains(p2(s)) {
                                picked.push(i as u32);
                            }
                        }
                    }
                } else {
                    let mut best: Option<(u32, f32)> = None;
                    for (i, v) in verts.iter().enumerate() {
                        if let Some(s) = proj.project(*v) {
                            let d = (p2(s) - m).length();
                            if d < 10.0 && best.is_none_or(|b| d < b.1 - 0.01) {
                                best = Some((i as u32, d));
                            }
                        }
                    }
                    if let Some(b) = best {
                        picked.push(b.0);
                    }
                }
                if !shift {
                    self.sel_verts.clear();
                }
                for p in picked {
                    if shift && rect_sel.is_none() && self.sel_verts.contains(&p) {
                        self.sel_verts.retain(|v| *v != p);
                    } else if !self.sel_verts.contains(&p) {
                        self.sel_verts.push(p);
                    }
                }
            }
            CompMode::Face => {
                let mut picked: Vec<usize> = Vec::new();
                if let Some(r) = rect_sel {
                    for (fi, f) in mesh.faces.iter().enumerate() {
                        let c = crate::mesh::Mesh::face_center(&verts, f);
                        if let Some(s) = proj.project(c) {
                            if r.contains(p2(s)) {
                                picked.push(fi);
                            }
                        }
                    }
                } else {
                    let (o, d) = proj.ray(m.x, m.y);
                    let mut best: Option<(usize, f32)> = None;
                    for (fi, f) in mesh.faces.iter().enumerate() {
                        for k in 1..f.len().saturating_sub(1) {
                            let (a, b, c) = (verts[f[0] as usize], verts[f[k] as usize], verts[f[k + 1] as usize]);
                            if let Some(t) = ray_tri(o, d, a, b, c) {
                                if best.is_none_or(|bb| t < bb.1) {
                                    best = Some((fi, t));
                                }
                            }
                        }
                    }
                    if let Some(b) = best {
                        picked.push(b.0);
                    }
                }
                if !shift {
                    self.sel_faces.clear();
                }
                for p in picked {
                    if shift && rect_sel.is_none() && self.sel_faces.contains(&p) {
                        self.sel_faces.retain(|v| *v != p);
                    } else if !self.sel_faces.contains(&p) {
                        self.sel_faces.push(p);
                    }
                }
            }
            CompMode::Object => {}
        }
    }

    /// Places a joint where the user clicked (joint tool).
    fn place_joint(&mut self, proj: &Proj, w: &Worlds, m: Pos2) {
        let (o, d) = proj.ray(m.x, m.y);
        let fwd = proj.forward();
        let axis = fwd.max_abs_axis();
        let normal = V3::axis(axis);
        let plane_d = if proj.ortho { 0.0 } else { 0.0 };
        let denom = d.dot(normal);
        if denom.abs() < 1e-6 {
            return;
        }
        let t = (plane_d - o.dot(normal)) / denom;
        let p = o + d * t;
        self.checkpoint();
        let parent = self.joint_chain_last.filter(|j| self.scene.get(*j).is_some());
        let name = self.scene.unique_name(crate::tr!("gelenk", "joint"));
        let mut n = Node::new(0, &name, Kind::Joint);
        n.parent = parent;
        let pinv = parent.and_then(|pid| w.get(&pid)).map(|m| m.inverse()).unwrap_or(M4::IDENTITY);
        n.t = pinv.point(p);
        let id = self.scene.add(n);
        self.joint_chain_last = Some(id);
        self.sel = vec![id];
        self.status = crate::trf!("Gelenk {name} gesetzt – weiter klicken für die Kette, Enter beendet.", "Joint {name} placed – keep clicking for the chain, Enter finishes.");
    }

    pub fn viewport(&mut self, ui: &mut egui::Ui, vi: usize, rect: Rect) {
        let id = ui.scope_id().with(("viewport", vi));
        let resp = ui.interact(rect, id, Sense::click_and_drag());
        let (alt, shift, ctrl) = ui.input(|i| (i.modifiers.alt, i.modifiers.shift, i.modifiers.command));
        let pointer = ui.input(|i| i.pointer.hover_pos());
        if resp.hovered() && ui.input(|i| i.pointer.any_pressed()) {
            self.active_view = vi;
        }
        let w = self.scene.world_matrices();
        let proj = self.proj_for(vi, rect, &w);

        // ---------- navigation ----------
        let delta = ui.input(|i| i.pointer.delta());
        let orbit = (alt && resp.dragged_by(egui::PointerButton::Primary))
            || (!alt && resp.dragged_by(egui::PointerButton::Secondary));
        let pan = resp.dragged_by(egui::PointerButton::Middle);
        let dolly = alt && resp.dragged_by(egui::PointerButton::Secondary);
        {
            let cam = &mut self.views[vi];
            if cam.look_through.is_none() {
                if orbit && !dolly {
                    if cam.kind == ViewKind::Persp {
                        cam.yaw -= delta.x * 0.006;
                        cam.pitch = (cam.pitch + delta.y * 0.006).clamp(-1.55, 1.55);
                    } else {
                        let wpp = proj.world_per_pixel(cam.target);
                        let right = proj.cam_world.col(0);
                        let up = proj.cam_world.col(1);
                        cam.target = cam.target - right * delta.x * wpp + up * delta.y * wpp;
                    }
                }
                if pan {
                    let wpp = proj.world_per_pixel(cam.target);
                    let right = proj.cam_world.col(0);
                    let up = proj.cam_world.col(1);
                    cam.target = cam.target - right * delta.x * wpp + up * delta.y * wpp;
                }
                if dolly {
                    let f = (delta.x * 0.01 - delta.y * 0.01).exp();
                    cam.dist = (cam.dist / f).clamp(0.2, 2000.0);
                    cam.half = (cam.half / f).clamp(0.05, 2000.0);
                }
                if resp.hovered() {
                    let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                    if scroll != 0.0 {
                        let f = (-scroll * 0.002).exp();
                        cam.dist = (cam.dist * f).clamp(0.2, 2000.0);
                        cam.half = (cam.half * f).clamp(0.05, 2000.0);
                    }
                }
            }
        }

        // ---------- tools ----------
        let center = self.selection_center(&w);
        let hover_handle = match (center, pointer) {
            (Some(c), Some(m)) if !alt && rect.contains(m) && self.drag.is_none() => self.hit_gizmo(&proj, c, m),
            _ => None,
        };
        if !alt && resp.drag_started_by(egui::PointerButton::Primary) {
            let m = resp.interact_pointer_pos().unwrap_or_default();
            let start = ui.input(|i| i.pointer.press_origin()).unwrap_or(m);
            let handle = center.and_then(|c| self.hit_gizmo(&proj, c, start));
            if let (Some(h), Some(c)) = (handle, center) {
                self.checkpoint();
                let mut ds = DragState {
                    kind: DragKind::Gizmo(h),
                    view: vi,
                    start_mouse: start,
                    center: c,
                    nodes: Vec::new(),
                    verts: Vec::new(),
                    mesh_world: M4::IDENTITY,
                    proj,
                };
                if self.comp == CompMode::Object {
                    for &sid in &self.top_selection() {
                        let Some(n) = self.scene.get(sid) else { continue };
                        ds.nodes.push(NodeStart {
                            id: sid,
                            world_pos: w[&sid].translation(),
                            parent_inv: self.scene.parent_world(sid, &w).inverse(),
                            r: n.r,
                            s: n.s,
                        });
                    }
                } else if let Some(n) = self.lead().and_then(|l| self.scene.get(l)) {
                    if let Some(mesh) = &n.mesh {
                        ds.verts = mesh.verts.clone();
                        ds.mesh_world = w[&n.id];
                    }
                }
                self.drag = Some(ds);
            } else if self.tool != Tool::Joint {
                self.drag = Some(DragState {
                    kind: DragKind::Box,
                    view: vi,
                    start_mouse: start,
                    center: V3::ZERO,
                    nodes: Vec::new(),
                    verts: Vec::new(),
                    mesh_world: M4::IDENTITY,
                    proj,
                });
            }
        }
        if let Some(ds) = self.drag.clone() {
            if ds.view == vi {
                if let Some(m) = pointer.or(resp.interact_pointer_pos()) {
                    if let DragKind::Gizmo(axis) = ds.kind {
                        self.apply_gizmo_drag(&ds, axis, m);
                    }
                }
                let released = ui.input(|i| i.pointer.primary_released()) || !ui.input(|i| i.pointer.primary_down());
                if released {
                    if ds.kind == DragKind::Box {
                        let end = pointer.unwrap_or(ds.start_mouse);
                        let r = Rect::from_two_pos(ds.start_mouse, end);
                        if r.width() > 3.0 || r.height() > 3.0 {
                            self.box_select(&proj, &w, r, shift);
                        }
                    }
                    if matches!(ds.kind, DragKind::Gizmo(_)) {
                        self.auto_key_selection();
                    }
                    self.drag = None;
                }
            }
        }
        if resp.clicked_by(egui::PointerButton::Primary) && !alt {
            let m = resp.interact_pointer_pos().unwrap_or_default();
            if self.tool == Tool::Joint {
                self.place_joint(&proj, &w, m);
            } else if self.comp != CompMode::Object {
                self.pick_component(&proj, &w, m, shift, None);
            } else {
                let hit = self.pick_object(&proj, &w, m);
                self.select_click(hit, shift, ctrl);
            }
        }

        // ---------- drawing ----------
        let w = self.scene.world_matrices();
        let proj = self.proj_for(vi, rect, &w);
        let painter = ui.painter_at(rect);
        self.draw_view(&painter, vi, rect, &proj, &w, hover_handle);
        if let Some(ds) = &self.drag {
            if ds.kind == DragKind::Box && ds.view == vi {
                if let Some(m) = pointer {
                    let r = Rect::from_two_pos(ds.start_mouse, m);
                    painter.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(120, 170, 255, 30));
                    painter.rect_stroke(r, 0.0, Stroke::new(1.0, Color32::from_gray(230)), egui::StrokeKind::Inside);
                }
            }
        }
        // active view outline
        if self.quad_view && self.active_view == vi {
            painter.rect_stroke(rect.shrink(1.0), 0.0, Stroke::new(2.0, Color32::from_rgb(90, 160, 230)), egui::StrokeKind::Inside);
        }
    }

    /// Selected nodes without those whose ancestor is selected too.
    pub fn top_selection(&self) -> Vec<Id> {
        self.sel
            .iter()
            .copied()
            .filter(|&id| !self.sel.iter().any(|&o| o != id && self.scene.is_ancestor(o, id)))
            .collect()
    }

    fn box_select(&mut self, proj: &Proj, w: &Worlds, r: Rect, shift: bool) {
        if self.comp != CompMode::Object {
            self.pick_component(proj, w, r.center(), shift, Some(r));
            return;
        }
        if !shift {
            self.sel.clear();
        }
        for n in &self.scene.nodes {
            if !self.scene.is_visible(n.id) {
                continue;
            }
            if let Some(s) = w.get(&n.id).and_then(|m| proj.project(m.translation())) {
                if r.contains(p2(s)) && !self.sel.contains(&n.id) {
                    self.sel.push(n.id);
                }
            }
        }
    }

    fn apply_gizmo_drag(&mut self, ds: &DragState, axis: usize, m: Pos2) {
        let proj = &ds.proj;
        let c = ds.center;
        let md = m - ds.start_mouse;
        let wpp = proj.world_per_pixel(c);
        let snap = self.snap;
        match self.tool {
            Tool::Move => {
                let delta = if axis < 3 {
                    let d = V3::axis(axis);
                    let (Some(s0), Some(s1)) = (proj.project(c), proj.project(c + d * wpp * 85.0)) else { return };
                    let sv = vec2(s1.0 - s0.0, s1.1 - s0.1);
                    let l = sv.length().max(1e-3);
                    let amount = md.dot(sv / l) / l * wpp * 85.0;
                    d * amount
                } else {
                    let right = proj.cam_world.col(0);
                    let up = proj.cam_world.col(1);
                    right * md.x * wpp - up * md.y * wpp
                };
                if self.comp == CompMode::Object {
                    for ns in &ds.nodes {
                        let mut wp = ns.world_pos + delta;
                        if snap {
                            for a in 0..3 {
                                if axis == 3 || a == axis {
                                    wp.set(a, (wp.get(a) * 2.0).round() / 2.0);
                                }
                            }
                        }
                        let lp = ns.parent_inv.point(wp);
                        if let Some(n) = self.scene.get_mut(ns.id) {
                            n.t = lp;
                        }
                    }
                } else {
                    let local = ds.mesh_world.inverse().vector(delta);
                    self.transform_components(ds, |p| p + local);
                }
            }
            Tool::Rotate if axis < 3 => {
                let Some(sc) = proj.project(c) else { return };
                let a0 = (ds.start_mouse.y - sc.1).atan2(ds.start_mouse.x - sc.0);
                let a1 = (m.y - sc.1).atan2(m.x - sc.0);
                let mut da = (a1 - a0).to_degrees();
                if da > 180.0 {
                    da -= 360.0;
                }
                if da < -180.0 {
                    da += 360.0;
                }
                let toward = V3::axis(axis).dot(proj.eye() - c) > 0.0 || proj.ortho && V3::axis(axis).dot(-proj.forward()) > 0.0;
                let mut ang = if toward { -da } else { da };
                if snap {
                    ang = (ang / 15.0).round() * 15.0;
                }
                if self.comp == CompMode::Object {
                    for ns in &ds.nodes {
                        if let Some(n) = self.scene.get_mut(ns.id) {
                            let mut r = ns.r;
                            r.set(axis, ns.r.get(axis) + ang);
                            n.r = r;
                        }
                    }
                } else {
                    let wm = ds.mesh_world;
                    let rot = M4::translate(c) * M4::axis_angle(V3::axis(axis), ang.to_radians()) * M4::translate(-c);
                    let mtx = wm.inverse() * rot * wm;
                    self.transform_components(ds, |p| mtx.point(p));
                }
            }
            Tool::Scale => {
                let f = if axis < 3 {
                    let d = V3::axis(axis);
                    let (Some(s0), Some(s1)) = (proj.project(c), proj.project(c + d * wpp * 85.0)) else { return };
                    let sv = vec2(s1.0 - s0.0, s1.1 - s0.1);
                    let l = sv.length().max(1e-3);
                    1.0 + md.dot(sv / l) / 85.0
                } else {
                    1.0 + (md.x - md.y) / 120.0
                };
                let f = f.max(0.01);
                if self.comp == CompMode::Object {
                    for ns in &ds.nodes {
                        if let Some(n) = self.scene.get_mut(ns.id) {
                            if axis < 3 {
                                let mut s = ns.s;
                                s.set(axis, ns.s.get(axis) * f);
                                n.s = s;
                            } else {
                                n.s = ns.s * f;
                            }
                        }
                    }
                } else {
                    let wm = ds.mesh_world;
                    let sv = if axis < 3 {
                        let mut v = V3::ONE;
                        v.set(axis, f);
                        v
                    } else {
                        V3::ONE * f
                    };
                    let mtx = wm.inverse() * M4::translate(c) * M4::scale(sv) * M4::translate(-c) * wm;
                    self.transform_components(ds, |p| mtx.point(p));
                }
            }
            _ => {}
        }
    }

    fn transform_components(&mut self, ds: &DragState, f: impl Fn(V3) -> V3) {
        let Some(lead) = self.lead() else { return };
        let ids = {
            let Some(n) = self.scene.get(lead) else { return };
            let Some(mesh) = &n.mesh else { return };
            self.component_verts(mesh)
        };
        if let Some(n) = self.scene.get_mut(lead) {
            if let Some(mesh) = &mut n.mesh {
                for i in ids {
                    let i = i as usize;
                    if i < ds.verts.len() && i < mesh.verts.len() {
                        mesh.verts[i] = f(ds.verts[i]);
                    }
                }
            }
            n.prim = None;
        }
    }

    fn draw_line3(&self, painter: &egui::Painter, proj: &Proj, a: V3, b: V3, stroke: Stroke) {
        let va = proj.view.point(a);
        let vb = proj.view.point(b);
        let (mut va, mut vb) = (va, vb);
        if !proj.ortho {
            let n = proj.near;
            let ia = -va.z >= n;
            let ib = -vb.z >= n;
            if !ia && !ib {
                return;
            }
            if !ia || !ib {
                let t = (-n - va.z) / (vb.z - va.z);
                let pm = va.lerp(vb, t);
                if !ia {
                    va = pm;
                } else {
                    vb = pm;
                }
            }
        }
        painter.line_segment([p2(proj.view_to_screen(va)), p2(proj.view_to_screen(vb))], stroke);
    }

    fn draw_view(&self, painter: &egui::Painter, vi: usize, rect: Rect, proj: &Proj, w: &Worlds, hover: Option<usize>) {
        let cam = &self.views[vi];
        // background gradient
        let bg = self.scene.background;
        let top = Color32::from_rgb((bg[0] * 255.0) as u8, (bg[1] * 255.0) as u8, (bg[2] * 255.0) as u8);
        let bottom = Color32::from_rgb((bg[0] * 90.0) as u8, (bg[1] * 90.0) as u8, (bg[2] * 95.0) as u8);
        let mut bgm = egui::Mesh::default();
        bgm.colored_vertex(rect.left_top(), top);
        bgm.colored_vertex(rect.right_top(), top);
        bgm.colored_vertex(rect.right_bottom(), bottom);
        bgm.colored_vertex(rect.left_bottom(), bottom);
        bgm.add_triangle(0, 1, 2);
        bgm.add_triangle(0, 2, 3);
        painter.add(egui::Shape::mesh(bgm));

        // grid
        if self.show_grid {
            let (ax_u, ax_v) = match (cam.kind, cam.look_through.is_some()) {
                (ViewKind::Front, false) => (0usize, 1usize),
                (ViewKind::Side, false) => (2, 1),
                _ => (0, 2),
            };
            let ext = 12;
            for i in -ext..=ext {
                let f = i as f32;
                let col = if i == 0 {
                    Color32::from_gray(25)
                } else if i % 5 == 0 {
                    Color32::from_gray(95)
                } else {
                    Color32::from_gray(120).gamma_multiply(0.6)
                };
                let mut a = V3::ZERO;
                let mut b = V3::ZERO;
                a.set(ax_u, f);
                a.set(ax_v, -ext as f32);
                b.set(ax_u, f);
                b.set(ax_v, ext as f32);
                self.draw_line3(painter, proj, a, b, Stroke::new(1.0, col));
                let mut a = V3::ZERO;
                let mut b = V3::ZERO;
                a.set(ax_v, f);
                a.set(ax_u, -ext as f32);
                b.set(ax_v, f);
                b.set(ax_u, ext as f32);
                self.draw_line3(painter, proj, a, b, Stroke::new(1.0, col));
            }
        }

        // shaded triangles (painter's algorithm)
        let lights = self.scene.lights(w);
        let eye = proj.eye();
        let lead = self.lead();
        let show_shaded = cam.display != Display::Wire;
        if show_shaded {
            let tris = self.scene.triangles(w);
            // (depth, screen points, colours, uvs, texture id)
            type ScreenTri = (f32, [Pos2; 3], [Color32; 3], [Pos2; 3], Option<egui::TextureId>);
            let mut screen: Vec<ScreenTri> = Vec::with_capacity(tris.len());
            for t in &tris {
                let tex_id = if t.tex != 0 { self.textures.get(&t.tex).map(|e| e.handle.id()) } else { None };
                let poly: Vec<ClipVert> = (0..3).map(|i| (proj.view.point(t.p[i]), t.p[i], t.n[i], v3(t.uv[i][0], t.uv[i][1], 0.0))).collect();
                let poly = if proj.ortho { poly } else { clip_near(&poly, proj.near) };
                if poly.len() < 3 {
                    continue;
                }
                let face_sel = self.comp == CompMode::Face && Some(t.node) == lead && self.sel_faces.contains(&t.face);
                let mut color = t.color;
                if face_sel {
                    color = [1.0, 0.55, 0.15];
                }
                let cols: Vec<Color32> = poly
                    .iter()
                    .map(|v| {
                        let c = shade(v.1, v.2, eye, color, t.spec, t.shin, &lights, !cam.scene_lights);
                        let rgb = to_rgb8(c);
                        Color32::from_rgb(rgb[0], rgb[1], rgb[2])
                    })
                    .collect();
                let sp: Vec<(f32, f32, f32)> = poly.iter().map(|v| proj.view_to_screen(v.0)).collect();
                for k in 1..poly.len() - 1 {
                    let idx = [0, k, k + 1];
                    let depth = (sp[0].2 + sp[k].2 + sp[k + 1].2) / 3.0;
                    let tex = if face_sel { None } else { tex_id };
                    screen.push((depth, idx.map(|i| p2(sp[i])), idx.map(|i| cols[i]), idx.map(|i| pos2(poly[i].3.x, poly[i].3.y)), tex));
                }
            }
            screen.sort_by(|a, b| b.0.total_cmp(&a.0));
            // one egui mesh per run of triangles sharing a texture (keeps depth order)
            let mut mesh = egui::Mesh::default();
            let mut cur: Option<egui::TextureId> = None;
            for (_, p, c, uv, tex) in &screen {
                if *tex != cur {
                    if !mesh.indices.is_empty() {
                        painter.add(egui::Shape::mesh(std::mem::take(&mut mesh)));
                    }
                    mesh = match tex {
                        Some(id) => egui::Mesh::with_texture(*id),
                        None => egui::Mesh::default(),
                    };
                    cur = *tex;
                }
                let base = mesh.vertices.len() as u32;
                for i in 0..3 {
                    if tex.is_some() {
                        mesh.vertices.push(egui::epaint::Vertex { pos: p[i], uv: uv[i], color: c[i] });
                    } else {
                        mesh.colored_vertex(p[i], c[i]);
                    }
                }
                mesh.add_triangle(base, base + 1, base + 2);
            }
            if !mesh.indices.is_empty() {
                painter.add(egui::Shape::mesh(mesh));
            }
        }

        // wireframes
        for n in &self.scene.nodes {
            if n.kind != Kind::Mesh || !self.scene.is_visible(n.id) {
                continue;
            }
            let selected = self.sel.contains(&n.id);
            let draw = selected || cam.display != Display::Shaded;
            if !draw {
                continue;
            }
            let Some(mesh) = Scene::display_mesh(n) else { continue };
            if mesh.faces.len() > 6000 {
                continue;
            }
            let verts = self.scene.deformed_verts(n, &mesh, w);
            let col = if Some(n.id) == lead && selected {
                SEL_COLOR
            } else if selected {
                Color32::from_rgb(70, 200, 230)
            } else if cam.display == Display::Wire {
                Color32::from_gray(220)
            } else {
                Color32::from_gray(30)
            };
            let alpha = if selected && cam.display == Display::Shaded { 0.75 } else { 1.0 };
            let stroke = Stroke::new(1.0, col.gamma_multiply(alpha));
            let sp: Vec<Option<Pos2>> = verts.iter().map(|v| proj.project(*v).map(p2)).collect();
            for f in &mesh.faces {
                for i in 0..f.len() {
                    let a = f[i] as usize;
                    let b = f[(i + 1) % f.len()] as usize;
                    if let (Some(pa), Some(pb)) = (sp[a], sp[b]) {
                        painter.line_segment([pa, pb], stroke);
                    }
                }
            }
            // component points
            if self.comp == CompMode::Vertex && Some(n.id) == lead && !n.smooth_preview {
                for (i, p) in sp.iter().enumerate() {
                    if let Some(p) = p {
                        let s = self.sel_verts.contains(&(i as u32));
                        painter.circle_filled(*p, if s { 3.5 } else { 2.5 }, if s { Color32::YELLOW } else { Color32::from_rgb(200, 60, 200) });
                    }
                }
            }
            if self.comp == CompMode::Face && Some(n.id) == lead && !n.smooth_preview {
                for (fi, f) in mesh.faces.iter().enumerate() {
                    let c = crate::mesh::Mesh::face_center(&verts, f);
                    if let Some(s) = proj.project(c) {
                        let sel = self.sel_faces.contains(&fi);
                        painter.rect_filled(Rect::from_center_size(p2(s), vec2(4.0, 4.0)), 0.0, if sel { Color32::from_rgb(255, 150, 40) } else { Color32::from_rgb(60, 120, 220) });
                    }
                }
            }
        }

        // motion trail of the lead object
        if self.show_trail {
            if let Some(l) = lead {
                if self.scene.get(l).is_some_and(|n| n.is_animated()) {
                    let mut pts = Vec::new();
                    let mut f = self.scene.start;
                    while f <= self.scene.end {
                        if let Some(p) = self.world_pos_at(l, f) {
                            if let Some(s) = proj.project(p) {
                                pts.push((f, p2(s)));
                            }
                        }
                        f += 1.0;
                    }
                    for wpair in pts.windows(2) {
                        painter.line_segment([wpair[0].1, wpair[1].1], Stroke::new(1.5, Color32::from_rgb(255, 180, 90)));
                    }
                    let keys = self.scene.key_frames(&[l]);
                    for (f, p) in &pts {
                        if keys.iter().any(|k| (k - f).abs() < 0.01) {
                            painter.circle_filled(*p, 3.0, Color32::from_rgb(255, 90, 60));
                        }
                    }
                }
            }
        }

        // helpers: joints, lights, cameras, IK handles
        for n in &self.scene.nodes {
            if !self.scene.is_visible(n.id) {
                continue;
            }
            let Some(m) = w.get(&n.id) else { continue };
            let pos = m.translation();
            let selected = self.sel.contains(&n.id);
            let col = if Some(n.id) == lead && selected {
                LEAD_COLOR
            } else if selected {
                SEL_COLOR
            } else {
                Color32::from_rgb(70, 130, 255)
            };
            match n.kind {
                Kind::Joint => {
                    let Some(s) = proj.project(pos) else { continue };
                    let r = (n.joint_radius / proj.world_per_pixel(pos)).clamp(3.0, 40.0);
                    let jc = if selected { col } else { Color32::from_rgb(80, 150, 255) };
                    painter.circle_stroke(p2(s), r, Stroke::new(1.5, jc));
                    for c in self.scene.nodes.iter().filter(|c| c.parent == Some(n.id) && c.kind == Kind::Joint) {
                        let Some(cs) = w.get(&c.id).and_then(|cm| proj.project(cm.translation())) else { continue };
                        let dir = p2(cs) - p2(s);
                        if dir.length() < 1.0 {
                            continue;
                        }
                        let perp = vec2(-dir.y, dir.x).normalized() * r;
                        let bc = if self.sel.contains(&c.id) || selected { col } else { Color32::from_rgb(110, 170, 255) };
                        painter.line_segment([p2(s) + perp, p2(cs)], Stroke::new(1.3, bc));
                        painter.line_segment([p2(s) - perp, p2(cs)], Stroke::new(1.3, bc));
                    }
                }
                Kind::Light => {
                    let Some(s) = proj.project(pos) else { continue };
                    let lc = if selected { col } else { Color32::from_rgb(255, 230, 120) };
                    painter.circle_stroke(p2(s), 7.0, Stroke::new(1.5, lc));
                    match n.light_kind {
                        LightKind::Directional => {
                            let d = m.vector(-V3::Z).norm();
                            self.draw_line3(painter, proj, pos, pos + d * 2.0, Stroke::new(1.5, lc));
                        }
                        LightKind::Point => {
                            for k in 0..8 {
                                let a = k as f32 * std::f32::consts::TAU / 8.0;
                                let dv = vec2(a.cos(), a.sin());
                                painter.line_segment([p2(s) + dv * 9.0, p2(s) + dv * 14.0], Stroke::new(1.2, lc));
                            }
                        }
                        LightKind::Ambient => {
                            painter.text(p2(s), egui::Align2::CENTER_CENTER, "A", egui::FontId::proportional(10.0), lc);
                        }
                    }
                }
                Kind::Camera => {
                    if cam.look_through == Some(n.id) {
                        continue;
                    }
                    let cc = if selected { col } else { Color32::from_rgb(200, 200, 200) };
                    let st = Stroke::new(1.3, cc);
                    let z = -1.2;
                    let hy = (n.fov.to_radians() * 0.5).tan() * 1.2;
                    let hx = hy * 16.0 / 9.0;
                    let corners = [v3(-hx, -hy, z), v3(hx, -hy, z), v3(hx, hy, z), v3(-hx, hy, z)].map(|c| m.point(c));
                    for i in 0..4 {
                        self.draw_line3(painter, proj, pos, corners[i], st);
                        self.draw_line3(painter, proj, corners[i], corners[(i + 1) % 4], st);
                    }
                    let up = m.point(v3(0.0, hy * 1.5, z));
                    self.draw_line3(painter, proj, corners[3], up, st);
                    self.draw_line3(painter, proj, corners[2], up, st);
                }
                Kind::IkHandle => {
                    let Some(s) = proj.project(pos) else { continue };
                    let ic = if selected { col } else { Color32::from_rgb(230, 140, 255) };
                    painter.line_segment([p2(s) - vec2(8.0, 0.0), p2(s) + vec2(8.0, 0.0)], Stroke::new(1.5, ic));
                    painter.line_segment([p2(s) - vec2(0.0, 8.0), p2(s) + vec2(0.0, 8.0)], Stroke::new(1.5, ic));
                    // line to the chain root
                    let mut root = n.ik_end;
                    for _ in 0..n.ik_chain {
                        root = root.and_then(|r| self.scene.get(r)).and_then(|r| r.parent);
                    }
                    if let Some(rp) = root.and_then(|r| w.get(&r)) {
                        self.draw_line3(painter, proj, rp.translation(), pos, Stroke::new(1.0, ic.gamma_multiply(0.7)));
                    }
                }
                Kind::Group => {
                    if selected {
                        if let Some(s) = proj.project(pos) {
                            painter.rect_stroke(Rect::from_center_size(p2(s), vec2(10.0, 10.0)), 0.0, Stroke::new(1.5, col), egui::StrokeKind::Middle);
                        }
                    }
                }
                Kind::Mesh => {}
            }
        }

        // manipulator
        if let Some(c) = self.selection_center(w) {
            let active = self.drag.as_ref().and_then(|d| if let DragKind::Gizmo(a) = d.kind { Some(a) } else { None });
            for (a, pts) in self.gizmo_handles(proj, c) {
                let hot = hover == Some(a) || active == Some(a);
                let col = if hot { HANDLE_HOVER } else if a < 3 { AXIS_COLORS[a] } else { Color32::from_rgb(240, 240, 120) };
                if a == 3 {
                    let size = if self.tool == Tool::Scale { 9.0 } else { 11.0 };
                    painter.rect_stroke(Rect::from_center_size(pts[0], vec2(size, size)), 0.0, Stroke::new(2.0, col), egui::StrokeKind::Middle);
                } else if self.tool == Tool::Rotate {
                    painter.add(egui::Shape::line(pts, Stroke::new(if hot { 3.0 } else { 2.0 }, col)));
                } else {
                    painter.line_segment([pts[0], pts[1]], Stroke::new(if hot { 3.5 } else { 2.5 }, col));
                    if self.tool == Tool::Move {
                        let dir = (pts[1] - pts[0]).normalized();
                        let perp = vec2(-dir.y, dir.x);
                        let tip = pts[1] + dir * 12.0;
                        painter.add(egui::Shape::convex_polygon(vec![tip, pts[1] + perp * 5.0, pts[1] - perp * 5.0], col, Stroke::NONE));
                    } else {
                        painter.rect_filled(Rect::from_center_size(pts[1], vec2(9.0, 9.0)), 0.0, col);
                    }
                }
            }
        }

        // HUD
        let font = egui::FontId::proportional(13.0);
        painter.text(rect.left_top() + vec2(10.0, 8.0), egui::Align2::LEFT_TOP, cam.label(&self.scene), font.clone(), Color32::from_gray(235));
        let polys: usize = self.scene.nodes.iter().filter_map(|n| n.mesh.as_ref()).map(|m| m.faces.len()).sum();
        let hud = crate::trf!("Bild {:>4}   Objekte {}   Polygone {}", "Frame {:>4}   Objects {}   Polygons {}", self.scene.frame.round() as i64, self.scene.nodes.len(), polys);
        painter.text(rect.right_top() + vec2(-10.0, 8.0), egui::Align2::RIGHT_TOP, hud, egui::FontId::monospace(11.0), Color32::from_gray(215));
        if let Some(clip) = self.scene.active_clip.and_then(|i| self.scene.clips.get(i)) {
            let t = crate::trf!("Animation: {}", "Animation: {}", clip.name);
            painter.text(rect.right_top() + vec2(-10.0, 24.0), egui::Align2::RIGHT_TOP, t, egui::FontId::monospace(11.0), Color32::from_rgb(140, 200, 255));
        }
        if self.auto_key {
            painter.text(rect.right_top() + vec2(-10.0, 40.0), egui::Align2::RIGHT_TOP, "AUTO-KEY", egui::FontId::monospace(11.0), Color32::from_rgb(255, 90, 80));
        }
        // axis triad
        let o = rect.left_bottom() + vec2(36.0, -36.0);
        let mut axes: Vec<(usize, V3)> = (0..3).map(|a| (a, proj.view.vector(V3::axis(a)))).collect();
        axes.sort_by(|a, b| a.1.z.total_cmp(&b.1.z));
        for (a, v) in axes {
            let e = o + vec2(v.x, -v.y) * 22.0;
            painter.line_segment([o, e], Stroke::new(2.0, AXIS_COLORS[a]));
            painter.text(e + vec2(v.x, -v.y) * 6.0, egui::Align2::CENTER_CENTER, ["X", "Y", "Z"][a], egui::FontId::proportional(11.0), AXIS_COLORS[a]);
        }
    }

    /// World position of a node at an arbitrary frame (no IK), for motion trails.
    pub fn world_pos_at(&self, id: Id, frame: f32) -> Option<V3> {
        let mut m = M4::IDENTITY;
        let mut chain = Vec::new();
        let mut cur = Some(id);
        while let Some(c) = cur {
            let n = self.scene.get(c)?;
            chain.push(n);
            cur = n.parent;
            if chain.len() > 64 {
                break;
            }
        }
        for n in chain.iter().rev() {
            let mut t = n.t;
            let mut r = n.r;
            let mut s = n.s;
            for c in 0..9 {
                if let Some(v) = n.curves[c].eval(frame) {
                    match c {
                        0..=2 => t.set(c, v),
                        3..=5 => r.set(c - 3, v),
                        _ => s.set(c - 6, v),
                    }
                }
            }
            m = m * M4::trs(t, r, s);
        }
        Some(m.translation())
    }

    /// Frames the camera of the active view on the selection (or everything).
    pub fn frame_view(&mut self, all: bool) {
        let w = self.scene.world_matrices();
        // a selected group frames everything inside it
        let ids: Vec<Id> = if all || self.sel.is_empty() {
            self.scene.nodes.iter().map(|n| n.id).collect()
        } else {
            self.sel.iter().flat_map(|s| self.scene.subtree(*s)).collect()
        };
        let mut lo = v3(f32::MAX, f32::MAX, f32::MAX);
        let mut hi = v3(f32::MIN, f32::MIN, f32::MIN);
        let mut any = false;
        for id in ids {
            let Some(n) = self.scene.get(id) else { continue };
            let pts: Vec<V3> = match &n.mesh {
                Some(m) => self.scene.deformed_verts(n, m, &w),
                None => vec![w[&id].translation()],
            };
            for p in pts {
                any = true;
                lo = v3(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
                hi = v3(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
            }
        }
        if !any {
            return;
        }
        let c = (lo + hi) * 0.5;
        let r = ((hi - lo).len() * 0.5).max(0.5);
        let v = self.active_view;
        let cam = &mut self.views[v];
        cam.target = c;
        cam.dist = r / (cam.fov.to_radians() * 0.5).sin() * 1.1;
        cam.half = r * 1.2;
    }
}
