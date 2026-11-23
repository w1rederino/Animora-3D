//! Scene graph: nodes, hierarchy, animation evaluation, IK and skinning.

use crate::anim::*;
use crate::math::*;
use crate::mesh::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub type Id = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LightKind {
    Directional,
    Point,
    Ambient,
}

impl LightKind {
    pub fn label(self) -> &'static str {
        match self {
            LightKind::Directional => crate::tr!("Richtungslicht", "Directional light"),
            LightKind::Point => crate::tr!("Punktlicht", "Point light"),
            LightKind::Ambient => crate::tr!("Umgebungslicht", "Ambient light"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Mesh,
    Joint,
    Light,
    Camera,
    Group,
    IkHandle,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Mesh => crate::tr!("Polygon-Objekt", "Polygon object"),
            Kind::Joint => crate::tr!("Gelenk", "Joint"),
            Kind::Light => crate::tr!("Licht", "Light"),
            Kind::Camera => crate::tr!("Kamera", "Camera"),
            Kind::Group => crate::tr!("Gruppe", "Group"),
            Kind::IkHandle => crate::tr!("IK-Griff", "IK handle"),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Material {
    pub color: [f32; 3],
    pub specular: f32,
    pub shininess: f32,
    pub ambient: f32,
    /// Diffuse (colour) texture file, used when the mesh has UVs.
    #[serde(default)]
    pub texture: Option<String>,
    /// Material name from imported files (Cast / XModel).
    #[serde(default)]
    pub name: String,
}

impl Default for Material {
    fn default() -> Self {
        Material { color: [0.62, 0.62, 0.62], specular: 0.15, shininess: 20.0, ambient: 0.0, texture: None, name: String::new() }
    }
}

/// Stable key for a texture path (0 means "no texture").
pub fn tex_key(path: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut h);
    h.finish().max(1)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Skin {
    pub joints: Vec<Id>,
    pub bind_inv: Vec<M4>,
    /// (joint index, weight) influences per vertex, weights sum to 1.
    /// (Older scene files stored exactly two – they load unchanged.)
    pub weights: Vec<Vec<(usize, f32)>>,
    pub bind_mesh_world: M4,
}

/// Keeps the 8 strongest valid influences and makes them sum to 1.
pub fn normalize_weights(ws: Vec<(usize, f32)>, joint_count: usize) -> Vec<(usize, f32)> {
    let mut ws: Vec<(usize, f32)> = ws.into_iter().filter(|(j, w)| *j < joint_count && *w > 1e-6).collect();
    ws.sort_by(|a, b| b.1.total_cmp(&a.1));
    ws.truncate(8);
    let sum: f32 = ws.iter().map(|w| w.1).sum();
    if ws.is_empty() || sum <= 1e-6 {
        return vec![(0, 1.0)];
    }
    ws.into_iter().map(|(j, w)| (j, w / sum)).collect()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Node {
    pub id: Id,
    pub name: String,
    pub kind: Kind,
    pub parent: Option<Id>,
    pub t: V3,
    pub r: V3,
    pub s: V3,
    pub visible: bool,
    pub curves: Vec<Curve>,
    pub mesh: Option<Mesh>,
    pub prim: Option<PrimParams>,
    pub material: Material,
    pub smooth_preview: bool,
    pub light_kind: LightKind,
    pub light_color: [f32; 3],
    pub intensity: f32,
    pub fov: f32,
    pub joint_radius: f32,
    pub ik_end: Option<Id>,
    pub ik_chain: usize,
    pub skin: Option<Skin>,
    /// Rest (bind) pose of a joint: translation, rotation, scale. Saved the
    /// first time an animation import touches the joint, used by "Reset scene".
    #[serde(default)]
    pub rest: Option<[V3; 3]>,
}

impl Node {
    pub fn new(id: Id, name: &str, kind: Kind) -> Node {
        Node {
            id,
            name: name.to_string(),
            kind,
            parent: None,
            t: V3::ZERO,
            r: V3::ZERO,
            s: V3::ONE,
            visible: true,
            curves: vec![Curve::default(); CHANNELS],
            mesh: None,
            prim: None,
            material: Material::default(),
            smooth_preview: false,
            light_kind: LightKind::Directional,
            light_color: [1.0, 1.0, 1.0],
            intensity: 1.0,
            fov: 45.0,
            joint_radius: 0.12,
            ik_end: None,
            ik_chain: 2,
            skin: None,
            rest: None,
        }
    }

    pub fn channel(&self, c: usize) -> f32 {
        match c {
            0..=2 => self.t.get(c),
            3..=5 => self.r.get(c - 3),
            6..=8 => self.s.get(c - 6),
            _ => {
                if self.visible {
                    1.0
                } else {
                    0.0
                }
            }
        }
    }

    pub fn set_channel(&mut self, c: usize, v: f32) {
        match c {
            0..=2 => self.t.set(c, v),
            3..=5 => self.r.set(c - 3, v),
            6..=8 => self.s.set(c - 6, v),
            _ => self.visible = v >= 0.5,
        }
    }

    pub fn local(&self) -> M4 {
        M4::trs(self.t, self.r, self.s)
    }

    pub fn is_animated(&self) -> bool {
        self.curves.iter().any(|c| !c.is_empty())
    }

    pub fn set_key(&mut self, c: usize, frame: f32, tangent: Tangent) {
        let v = self.channel(c);
        let tan = if c == 9 { Tangent::Step } else { tangent };
        self.curves[c].set_key(frame, v, tan);
    }
}

/// One animation of the library: the joint keys it puts on the models,
/// plus its notetracks, frame rate and frame range.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AnimClip {
    pub name: String,
    /// File it was imported from (empty for clips made in Animora).
    pub source: String,
    /// Keyframe curves per joint (all 10 channels).
    pub curves: Vec<(Id, Vec<Curve>)>,
    pub notes: Vec<Note>,
    pub fps: f32,
    pub start: f32,
    pub end: f32,
}

/// A named event on the timeline (CoD "notetrack" note).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Note {
    pub frame: f32,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Scene {
    pub nodes: Vec<Node>,
    pub next_id: Id,
    pub start: f32,
    pub end: f32,
    pub fps: f32,
    pub frame: f32,
    pub background: [f32; 3],
    #[serde(default)]
    pub notes: Vec<Note>,
    /// Imported animations (clips) that can be switched on the same models.
    #[serde(default)]
    pub clips: Vec<AnimClip>,
    /// Index of the clip currently on the models.
    #[serde(default)]
    pub active_clip: Option<usize>,
}

impl Default for Scene {
    fn default() -> Self {
        Scene {
            nodes: Vec::new(),
            next_id: 1,
            start: 1.0,
            end: 120.0,
            fps: 24.0,
            frame: 1.0,
            background: [0.36, 0.40, 0.46],
            notes: Vec::new(),
            clips: Vec::new(),
            active_clip: None,
        }
    }
}

/// A world-space triangle ready for drawing or rendering.
#[derive(Clone, Copy)]
pub struct Tri {
    pub p: [V3; 3],
    pub n: [V3; 3],
    pub color: [f32; 3],
    pub spec: f32,
    pub shin: f32,
    pub node: Id,
    pub face: usize,
    pub uv: [[f32; 2]; 3],
    /// Texture key (see [`tex_key`]); 0 = untextured.
    pub tex: u64,
}

#[derive(Clone, Copy)]
pub struct LightInfo {
    pub kind: LightKind,
    pub pos: V3,
    pub dir: V3,
    pub color: V3,
}

pub type Worlds = HashMap<Id, M4>;

impl Scene {
    pub fn new_id(&mut self) -> Id {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn get(&self, id: Id) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn get_mut(&mut self, id: Id) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }

    pub fn children(&self, id: Option<Id>) -> Vec<Id> {
        self.nodes.iter().filter(|n| n.parent == id).map(|n| n.id).collect()
    }

    pub fn unique_name(&self, base: &str) -> String {
        let base = base.trim_end_matches(|c: char| c.is_ascii_digit());
        for i in 1.. {
            let name = format!("{base}{i}");
            if !self.nodes.iter().any(|n| n.name == name) {
                return name;
            }
        }
        unreachable!()
    }

    pub fn add(&mut self, mut node: Node) -> Id {
        node.id = self.new_id();
        let id = node.id;
        self.nodes.push(node);
        id
    }

    pub fn add_prim(&mut self, p: Prim) -> Id {
        let name = self.unique_name(match p {
            Prim::Cube => crate::tr!("wuerfel", "cube"),
            Prim::Sphere => crate::tr!("kugel", "sphere"),
            Prim::Cylinder => crate::tr!("zylinder", "cylinder"),
            Prim::Cone => crate::tr!("kegel", "cone"),
            Prim::Plane => crate::tr!("ebene", "plane"),
            Prim::Torus => "torus",
        });
        let mut n = Node::new(0, &name, Kind::Mesh);
        let params = PrimParams::new(p);
        n.mesh = Some(params.build());
        n.prim = Some(params);
        self.add(n)
    }

    pub fn is_ancestor(&self, anc: Id, mut id: Id) -> bool {
        while let Some(p) = self.get(id).and_then(|n| n.parent) {
            if p == anc {
                return true;
            }
            id = p;
        }
        false
    }

    /// Ids of a node and all its descendants.
    pub fn subtree(&self, id: Id) -> Vec<Id> {
        let mut out = vec![id];
        let mut i = 0;
        while i < out.len() {
            let cur = out[i];
            out.extend(self.children(Some(cur)));
            i += 1;
        }
        out
    }

    pub fn delete(&mut self, ids: &[Id]) {
        let mut all = Vec::new();
        for &id in ids {
            all.extend(self.subtree(id));
        }
        self.nodes.retain(|n| !all.contains(&n.id));
        for n in &mut self.nodes {
            if n.ik_end.is_some_and(|e| all.contains(&e)) {
                n.ik_end = None;
            }
            if let Some(s) = &n.skin {
                if s.joints.iter().any(|j| all.contains(j)) {
                    n.skin = None;
                }
            }
        }
    }

    pub fn world_matrices(&self) -> Worlds {
        let mut w: Worlds = HashMap::new();
        fn rec(s: &Scene, id: Id, w: &mut Worlds) -> M4 {
            if let Some(m) = w.get(&id) {
                return *m;
            }
            let Some(n) = s.get(id) else { return M4::IDENTITY };
            let local = n.local();
            let m = match n.parent {
                Some(p) if p != id => rec(s, p, w) * local,
                _ => local,
            };
            w.insert(id, m);
            m
        }
        for n in &self.nodes {
            rec(self, n.id, &mut w);
        }
        w
    }

    pub fn parent_world(&self, id: Id, w: &Worlds) -> M4 {
        self.get(id).and_then(|n| n.parent).and_then(|p| w.get(&p).copied()).unwrap_or(M4::IDENTITY)
    }

    /// Re-parents keeping the world transform.
    pub fn set_parent(&mut self, id: Id, parent: Option<Id>) {
        if let Some(p) = parent {
            if p == id || self.is_ancestor(id, p) {
                return;
            }
        }
        let w = self.world_matrices();
        let world = w.get(&id).copied().unwrap_or_default();
        let pw = parent.and_then(|p| w.get(&p).copied()).unwrap_or(M4::IDENTITY);
        let local = pw.inverse() * world;
        let (t, r, s) = local.decompose();
        if let Some(n) = self.get_mut(id) {
            n.parent = parent;
            n.t = t;
            n.r = r;
            n.s = s;
        }
    }

    /// Applies animation curves for the given frame.
    pub fn evaluate(&mut self, frame: f32) {
        self.frame = frame;
        for n in &mut self.nodes {
            for c in 0..CHANNELS {
                if let Some(v) = n.curves[c].eval(frame) {
                    n.set_channel(c, v);
                }
            }
        }
        self.solve_ik();
    }

    /// Cyclic-coordinate-descent IK for every IK handle.
    pub fn solve_ik(&mut self) {
        let handles: Vec<(Id, Id, usize)> = self
            .nodes
            .iter()
            .filter(|n| n.kind == Kind::IkHandle && n.visible)
            .filter_map(|n| n.ik_end.map(|e| (n.id, e, n.ik_chain.max(1))))
            .collect();
        for (h, end, chain_len) in handles {
            // chain: joints from end's parent upwards
            let mut chain = Vec::new();
            let mut cur = self.get(end).and_then(|n| n.parent);
            while let Some(c) = cur {
                if chain.len() >= chain_len {
                    break;
                }
                if self.get(c).map(|n| n.kind) != Some(Kind::Joint) {
                    break;
                }
                chain.push(c);
                cur = self.get(c).and_then(|n| n.parent);
            }
            if chain.is_empty() {
                continue;
            }
            for _iter in 0..12 {
                let w = self.world_matrices();
                let target = w.get(&h).map(|m| m.translation()).unwrap_or_default();
                let eff = w.get(&end).map(|m| m.translation()).unwrap_or_default();
                if (eff - target).len() < 1e-3 {
                    break;
                }
                for &j in &chain {
                    let w = self.world_matrices();
                    let jw = w[&j];
                    let jp = jw.translation();
                    let eff = w[&end].translation();
                    let a = (eff - jp).norm();
                    let b = (target - jp).norm();
                    let axis = a.cross(b);
                    if axis.len() < 1e-6 {
                        continue;
                    }
                    let ang = a.dot(b).clamp(-1.0, 1.0).acos();
                    let rot = M4::axis_angle(axis.norm(), ang);
                    // new world rotation of the joint
                    let rw = rot * jw.rotation_only();
                    let pw = self.parent_world(j, &w).rotation_only();
                    let local_rot = pw.inverse() * rw;
                    let e = local_rot.to_euler_deg();
                    if let Some(n) = self.get_mut(j) {
                        n.r = e;
                    }
                }
            }
        }
    }

    /// World-space vertex positions of a mesh node (with skinning).
    pub fn deformed_verts(&self, n: &Node, mesh: &Mesh, w: &Worlds) -> Vec<V3> {
        let world = w.get(&n.id).copied().unwrap_or_default();
        if let Some(skin) = &n.skin {
            if skin.weights.len() == mesh.verts.len() {
                let mats: Vec<M4> = skin
                    .joints
                    .iter()
                    .zip(&skin.bind_inv)
                    .map(|(j, bi)| w.get(j).copied().unwrap_or_default() * *bi * skin.bind_mesh_world)
                    .collect();
                return mesh
                    .verts
                    .iter()
                    .zip(&skin.weights)
                    .map(|(v, ws)| {
                        let mut p = V3::ZERO;
                        for &(j, wt) in ws {
                            if wt > 0.0 && j < mats.len() {
                                p += mats[j].point(*v) * wt;
                            }
                        }
                        p
                    })
                    .collect();
            }
        }
        mesh.verts.iter().map(|v| world.point(*v)).collect()
    }

    /// The mesh as displayed (optionally smooth-previewed).
    pub fn display_mesh(n: &Node) -> Option<std::borrow::Cow<'_, Mesh>> {
        let m = n.mesh.as_ref()?;
        if n.smooth_preview && n.skin.is_none() && m.faces.len() < 4000 {
            Some(std::borrow::Cow::Owned(m.subdivide().subdivide()))
        } else {
            Some(std::borrow::Cow::Borrowed(m))
        }
    }

    pub fn is_visible(&self, id: Id) -> bool {
        let mut cur = Some(id);
        while let Some(c) = cur {
            match self.get(c) {
                Some(n) if n.visible => cur = n.parent,
                Some(_) => return false,
                None => return true,
            }
        }
        true
    }

    pub fn triangles(&self, w: &Worlds) -> Vec<Tri> {
        let mut out = Vec::new();
        for n in &self.nodes {
            if n.kind != Kind::Mesh || !self.is_visible(n.id) {
                continue;
            }
            let Some(mesh) = Scene::display_mesh(n) else { continue };
            let verts = self.deformed_verts(n, &mesh, w);
            let normals = mesh.corner_normals(&verts, 50.0);
            let has_uv = mesh.has_uvs();
            let tex = match (&n.material.texture, has_uv) {
                (Some(p), true) => tex_key(p),
                _ => 0,
            };
            for (fi, f) in mesh.faces.iter().enumerate() {
                for k in 1..f.len().saturating_sub(1) {
                    let ids = [0, k, k + 1];
                    out.push(Tri {
                        uv: if has_uv { ids.map(|i| mesh.uvs[fi].get(i).copied().unwrap_or([0.0, 0.0])) } else { [[0.0; 2]; 3] },
                        tex,
                        p: ids.map(|i| verts[f[i] as usize]),
                        n: ids.map(|i| normals[fi][i]),
                        color: n.material.color,
                        spec: n.material.specular,
                        shin: n.material.shininess,
                        node: n.id,
                        face: if n.smooth_preview { usize::MAX } else { fi },
                    });
                }
            }
        }
        out
    }

    pub fn lights(&self, w: &Worlds) -> Vec<LightInfo> {
        self.nodes
            .iter()
            .filter(|n| n.kind == Kind::Light && self.is_visible(n.id))
            .map(|n| {
                let m = w.get(&n.id).copied().unwrap_or_default();
                LightInfo {
                    kind: n.light_kind,
                    pos: m.translation(),
                    dir: m.vector(-V3::Z).norm(),
                    color: v3(n.light_color[0], n.light_color[1], n.light_color[2]) * n.intensity,
                }
            })
            .collect()
    }

    /// Smooth-binds a mesh to joints: each vertex follows its two nearest bones.
    pub fn bind_skin(&mut self, mesh_id: Id, joints: &[Id]) -> bool {
        if joints.is_empty() {
            return false;
        }
        let w = self.world_matrices();
        let Some(n) = self.get(mesh_id) else { return false };
        let Some(mesh) = &n.mesh else { return false };
        let mw = w[&mesh_id];
        // bones: segment from joint to its first joint child (or a point)
        let bones: Vec<(V3, V3)> = joints
            .iter()
            .map(|j| {
                let a = w[j].translation();
                let child = self
                    .nodes
                    .iter()
                    .find(|c| c.parent == Some(*j) && c.kind == Kind::Joint)
                    .map(|c| w[&c.id].translation());
                (a, child.unwrap_or(a))
            })
            .collect();
        let weights: Vec<Vec<(usize, f32)>> = mesh
            .verts
            .iter()
            .map(|v| {
                let p = mw.point(*v);
                let mut d: Vec<(usize, f32)> =
                    bones.iter().enumerate().map(|(i, (a, b))| (i, dist_point_segment(p, *a, *b))).collect();
                d.sort_by(|a, b| a.1.total_cmp(&b.1));
                if d.len() == 1 {
                    return vec![(d[0].0, 1.0)];
                }
                let w0 = 1.0 / (d[0].1 + 1e-3).powi(4);
                let w1 = 1.0 / (d[1].1 + 1e-3).powi(4);
                let s = w0 + w1;
                vec![(d[0].0, w0 / s), (d[1].0, w1 / s)]
            })
            .collect();
        let bind_inv = joints.iter().map(|j| w[j].inverse()).collect();
        let skin = Skin { joints: joints.to_vec(), bind_inv, weights, bind_mesh_world: mw };
        if let Some(n) = self.get_mut(mesh_id) {
            n.skin = Some(skin);
        }
        true
    }

    pub fn duplicate(&mut self, id: Id) -> Option<Id> {
        let ids = self.subtree(id);
        let mut map: HashMap<Id, Id> = HashMap::new();
        let mut new_nodes = Vec::new();
        for &old in &ids {
            let mut n = self.get(old)?.clone();
            let nid = self.new_id();
            map.insert(old, nid);
            n.id = nid;
            n.name = self.unique_name(&n.name);
            new_nodes.push(n);
        }
        for n in &mut new_nodes {
            if let Some(p) = n.parent {
                if let Some(np) = map.get(&p) {
                    n.parent = Some(*np);
                }
            }
            if let Some(e) = n.ik_end {
                n.ik_end = Some(*map.get(&e).unwrap_or(&e));
            }
            if let Some(s) = &mut n.skin {
                for j in &mut s.joints {
                    *j = *map.get(j).unwrap_or(j);
                }
            }
        }
        let first = map[&id];
        self.nodes.extend(new_nodes);
        Some(first)
    }

    /// Bakes the transform into the mesh vertices.
    pub fn freeze(&mut self, id: Id) {
        if !self.children(Some(id)).is_empty() {
            return;
        }
        let Some(n) = self.get_mut(id) else { return };
        let m = n.local();
        if let Some(mesh) = &mut n.mesh {
            for v in &mut mesh.verts {
                *v = m.point(*v);
            }
            n.t = V3::ZERO;
            n.r = V3::ZERO;
            n.s = V3::ONE;
            n.prim = None;
        }
    }

    pub fn to_obj(&self) -> String {
        let w = self.world_matrices();
        let mut s = String::from(crate::tr!("# exportiert aus Animora 3D\n", "# exported from Animora 3D\n"));
        let mut base = 1usize;
        for n in &self.nodes {
            if n.kind != Kind::Mesh || !self.is_visible(n.id) {
                continue;
            }
            let Some(mesh) = &n.mesh else { continue };
            let verts = self.deformed_verts(n, mesh, &w);
            s.push_str(&format!("o {}\n", n.name));
            for v in &verts {
                s.push_str(&format!("v {} {} {}\n", v.x, v.y, v.z));
            }
            for f in &mesh.faces {
                s.push('f');
                for &i in f {
                    s.push_str(&format!(" {}", i as usize + base));
                }
                s.push('\n');
            }
            base += verts.len();
        }
        s
    }

    /// All key frames of the given nodes (for the time slider).
    pub fn key_frames(&self, ids: &[Id]) -> Vec<f32> {
        let mut f: Vec<f32> = Vec::new();
        for id in ids {
            if let Some(n) = self.get(*id) {
                for c in &n.curves {
                    for k in &c.keys {
                        f.push(k.frame);
                    }
                }
            }
        }
        f.sort_by(|a, b| a.total_cmp(b));
        f.dedup_by(|a, b| (*a - *b).abs() < 1e-3);
        f
    }
}

/// Builds the example scene shown on start: a bouncing ball, a spinning
/// box, an arm rig with IK and lights.
pub fn demo_scene() -> Scene {
    let mut s = Scene::default();
    let ground = s.add_prim(Prim::Plane);
    if let Some(n) = s.get_mut(ground) {
        n.name = crate::tr!("boden", "ground").into();
        n.s = v3(1.6, 1.0, 1.6);
        n.material.color = [0.45, 0.47, 0.5];
        n.material.specular = 0.0;
    }

    let ball = s.add_prim(Prim::Sphere);
    if let Some(n) = s.get_mut(ball) {
        n.name = "ball".into();
        n.material.color = [0.86, 0.32, 0.22];
        n.material.specular = 0.5;
        n.material.shininess = 40.0;
        let keys: [(f32, f32, f32, f32); 6] = [
            (1.0, -5.0, 6.0, 0.0),
            (13.0, -3.0, 1.0, 0.0),
            (25.0, -1.0, 4.0, 0.0),
            (37.0, 1.0, 1.0, 0.0),
            (49.0, 3.0, 2.5, 0.0),
            (61.0, 5.0, 1.0, 0.0),
        ];
        for (f, x, y, z) in keys {
            n.t = v3(x, y, z);
            n.curves[0].set_key(f, x, Tangent::Linear);
            let tan = if y <= 1.01 { Tangent::Linear } else { Tangent::Flat };
            n.curves[1].set_key(f, y, tan);
            n.curves[2].set_key(f, z, Tangent::Spline);
        }
        n.curves[5].set_key(1.0, 0.0, Tangent::Linear);
        n.curves[5].set_key(61.0, -540.0, Tangent::Linear);
        for (f, sy) in [(12.0, 1.0), (13.0, 0.7), (15.0, 1.0), (36.0, 1.0), (37.0, 0.75), (39.0, 1.0)] {
            n.curves[7].set_key(f, sy, Tangent::Spline);
        }
    }

    let boxn = s.add_prim(Prim::Cube);
    if let Some(n) = s.get_mut(boxn) {
        n.name = crate::tr!("kiste", "box").into();
        n.t = v3(3.5, 0.75, -3.0);
        n.s = v3(1.5, 1.5, 1.5);
        n.material.color = [0.32, 0.55, 0.85];
        n.curves[4].set_key(1.0, 0.0, Tangent::Flat);
        n.curves[4].set_key(60.0, 180.0, Tangent::Flat);
        n.curves[4].set_key(120.0, 360.0, Tangent::Flat);
    }

    let torus = s.add_prim(Prim::Torus);
    if let Some(n) = s.get_mut(torus) {
        n.name = "ring".into();
        n.t = v3(-4.0, 0.3, -3.0);
        n.material.color = [0.95, 0.75, 0.25];
        n.material.specular = 0.7;
        n.material.shininess = 60.0;
    }

    // Arm rig: shoulder -> elbow -> wrist, with an IK handle and a skinned cylinder.
    let mut sh = Node::new(0, crate::tr!("schulter", "shoulder"), Kind::Joint);
    sh.t = v3(0.0, 0.2, -4.0);
    let sh = s.add(sh);
    let mut el = Node::new(0, crate::tr!("ellbogen", "elbow"), Kind::Joint);
    el.parent = Some(sh);
    el.t = v3(0.0, 1.6, 0.0);
    let el = s.add(el);
    let mut wr = Node::new(0, crate::tr!("handgelenk", "wrist"), Kind::Joint);
    wr.parent = Some(el);
    wr.t = v3(0.0, 1.6, 0.0);
    let wr = s.add(wr);

    let arm = s.add_prim(Prim::Cylinder);
    if let Some(n) = s.get_mut(arm) {
        n.name = "arm".into();
        // many rings so the arm bends smoothly
        n.mesh = Some(segmented_tube(0.3, 3.2, 14, 10));
        n.prim = None;
        n.t = v3(0.0, 1.8, -4.0);
        n.material.color = [0.55, 0.8, 0.5];
    }
    s.bind_skin(arm, &[sh, el, wr]);

    let mut ik = Node::new(0, crate::tr!("ikGriff1", "ikHandle1"), Kind::IkHandle);
    ik.ik_end = Some(wr);
    ik.ik_chain = 2;
    ik.t = v3(1.6, 2.6, -4.0);
    ik.curves[0].set_key(1.0, 1.6, Tangent::Spline);
    ik.curves[0].set_key(40.0, -1.4, Tangent::Spline);
    ik.curves[0].set_key(80.0, 1.2, Tangent::Spline);
    ik.curves[0].set_key(120.0, 1.6, Tangent::Spline);
    ik.curves[1].set_key(1.0, 2.6, Tangent::Spline);
    ik.curves[1].set_key(40.0, 2.2, Tangent::Spline);
    ik.curves[1].set_key(80.0, 1.4, Tangent::Spline);
    ik.curves[1].set_key(120.0, 2.6, Tangent::Spline);
    s.add(ik);

    let mut sun = Node::new(0, crate::tr!("sonne", "sun"), Kind::Light);
    sun.t = v3(4.0, 8.0, 6.0);
    sun.r = v3(-50.0, 30.0, 0.0);
    sun.intensity = 1.0;
    sun.light_color = [1.0, 0.96, 0.9];
    s.add(sun);
    let mut amb = Node::new(0, crate::tr!("umgebung", "ambient"), Kind::Light);
    amb.light_kind = LightKind::Ambient;
    amb.intensity = 0.25;
    amb.light_color = [0.7, 0.8, 1.0];
    amb.t = v3(-6.0, 6.0, 4.0);
    s.add(amb);

    let mut cam = Node::new(0, crate::tr!("kamera1", "camera1"), Kind::Camera);
    cam.t = v3(9.0, 6.0, 12.0);
    cam.r = v3(-20.0, 36.0, 0.0);
    s.add(cam);

    s.notes = vec![
        Note { frame: 13.0, name: "sndnt#ball_bounce".into() },
        Note { frame: 37.0, name: "sndnt#ball_bounce".into() },
        Note { frame: 61.0, name: "end".into() },
    ];

    s.evaluate(1.0);
    s
}

/// A capped tube with several rings along its length (good for skinning).
pub fn segmented_tube(r: f32, h: f32, seg: u32, rings: u32) -> Mesh {
    let mut verts = Vec::new();
    for j in 0..=rings {
        let y = -h * 0.5 + h * j as f32 / rings as f32;
        for i in 0..seg {
            let th = std::f32::consts::TAU * i as f32 / seg as f32;
            verts.push(v3(r * th.sin(), y, r * th.cos()));
        }
    }
    let mut faces = Vec::new();
    for j in 0..rings {
        for i in 0..seg {
            let n = (i + 1) % seg;
            let a = j * seg;
            let b = (j + 1) * seg;
            faces.push(vec![a + i, a + n, b + n, b + i]);
        }
    }
    faces.push((0..seg).rev().collect());
    faces.push((0..seg).map(|i| rings * seg + i).collect());
    Mesh { verts, faces, uvs: Vec::new() }
}
