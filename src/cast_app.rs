//! Cast inside Animora: import models (skeleton, skinned meshes, materials)
//! and animations (curves per joint in absolute / relative / additive mode,
//! mode overrides, notification tracks), export both. Behaviour follows
//! the Cast plugins: joints are matched by name (namespaces ignored), keys
//! are local, rotations are quaternions, the metadata up axis is honoured.

use crate::anim::*;
use crate::app::*;
use crate::cast::*;
use crate::cod::CM_TO_INCH;
use crate::math::*;
use crate::mesh::Mesh;
use crate::scene::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn strip_ns(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

fn unwrap(prev: f32, v: f32) -> f32 {
    let mut v = v;
    while v - prev > 180.0 {
        v -= 360.0;
    }
    while v - prev < -180.0 {
        v += 360.0;
    }
    v
}

fn qmul(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    // Hamilton product a*b, (x, y, z, w)
    let [ax, ay, az, aw] = a;
    let [bx, by, bz, bw] = b;
    [
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    ]
}

fn slerp(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    let mut d = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
    let mut b = b;
    if d < 0.0 {
        d = -d;
        b = [-b[0], -b[1], -b[2], -b[3]];
    }
    let (s0, s1) = if d > 0.9995 {
        (1.0 - t, t)
    } else {
        let th = d.clamp(-1.0, 1.0).acos();
        let s = th.sin();
        (((1.0 - t) * th).sin() / s, (t * th).sin() / s)
    };
    [a[0] * s0 + b[0] * s1, a[1] * s0 + b[1] * s1, a[2] * s0 + b[2] * s1, a[3] * s0 + b[3] * s1]
}

/// What a cast file contains: (models, animations).
pub fn cast_kind(path: &Path) -> (bool, bool) {
    let Ok(data) = std::fs::read(path) else { return (false, false) };
    let Ok(roots) = parse_cast(&data) else { return (false, false) };
    let models = roots.iter().any(|r| r.kids(ID_MODEL).next().is_some());
    let anims = roots.iter().any(|r| r.kids(ID_ANIMATION).next().is_some());
    (models, anims)
}

fn up_axis(roots: &[CNode]) -> String {
    roots.iter().flat_map(|r| r.kids(ID_METADATA)).find_map(|m| m.str("up").map(|s| s.to_lowercase())).unwrap_or_else(|| "y".into())
}

fn v3_at(v: &[f32], i: usize) -> V3 {
    v3(v.get(i * 3).copied().unwrap_or(0.0), v.get(i * 3 + 1).copied().unwrap_or(0.0), v.get(i * 3 + 2).copied().unwrap_or(0.0))
}

fn q_of(v: &[f32]) -> [f32; 4] {
    if v.len() >= 4 { [v[0], v[1], v[2], v[3]] } else { [0.0, 0.0, 0.0, 1.0] }
}

impl App {
    /// Scene units per Cast unit (Cast files from Maya are in centimetres).
    fn cast_k(&self) -> f32 {
        if self.cod.unit_cm { 1.0 } else { CM_TO_INCH }
    }

    // ---------------------------------------------------------------- import models

    pub fn cast_import_models(&mut self, path: &Path) -> Result<String, String> {
        let data = std::fs::read(path).map_err(|e| e.to_string())?;
        let roots = parse_cast(&data)?;
        let k = self.cast_k();
        let up = up_axis(&roots);
        let up_fix = match up.as_str() {
            "z" => M4::rot_x((-90f32).to_radians()),
            "x" => M4::rot_z(90f32.to_radians()),
            _ => M4::IDENTITY,
        };
        let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or("cast".into());
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let models: Vec<&CNode> = roots.iter().flat_map(|r| r.kids(ID_MODEL)).collect();
        if models.is_empty() {
            return Err(crate::tr!("Die Datei enthält kein Modell.", "The file contains no model.").into());
        }
        self.checkpoint();
        let mut report = Vec::new();
        let mut groups = Vec::new();
        for (mi, model) in models.iter().enumerate() {
            let base = model.str("n").map(|s| s.to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| if models.len() > 1 { format!("{stem}_{mi}") } else { stem.clone() });
            let gname = if self.scene.nodes.iter().any(|n| n.name == base) { self.scene.unique_name(&format!("{base}_")) } else { base };
            // model placement (optional) combined with the up-axis fix
            let mp = model.floats("p").map(|v| v3_at(&v, 0) * k).unwrap_or(V3::ZERO);
            let mr = model.floats("r").map(|v| quat_to_m4(q_of(&v))).unwrap_or(M4::IDENTITY);
            let ms = model.floats("s").map(|v| v3_at(&v, 0)).unwrap_or(V3::ONE);
            let gm = up_fix * M4::translate(mp) * mr * M4::scale(ms);
            let (gt, gr, gs) = gm.decompose();
            let mut g = Node::new(0, &gname, Kind::Group);
            g.t = gt;
            g.r = gr;
            g.s = gs;
            let group = self.scene.add(g);
            groups.push(group);
            // skeleton
            let bones: Vec<&CNode> = model.kids(ID_SKELETON).flat_map(|s| s.kids(ID_BONE)).collect();
            let mut ids: Vec<Id> = Vec::new();
            let mut by_hash: HashMap<u64, usize> = HashMap::new();
            for (i, b) in bones.iter().enumerate() {
                let name = b.str("n").unwrap_or("bone").to_string();
                let mut n = Node::new(0, &name, Kind::Joint);
                n.parent = Some(group);
                ids.push(self.scene.add(n));
                by_hash.insert(b.hash, i);
            }
            let parent_of = |b: &CNode| -> i64 { b.int("p").map(|p| p as u32 as i32 as i64).unwrap_or(-1) };
            // world matrices (model space) for bones that only have world data
            let worlds: Vec<M4> = bones
                .iter()
                .map(|b| {
                    let p = b.floats("wp").map(|v| v3_at(&v, 0) * k).unwrap_or(V3::ZERO);
                    let r = b.floats("wr").map(|v| quat_to_m4(q_of(&v))).unwrap_or(M4::IDENTITY);
                    M4::translate(p) * r
                })
                .collect();
            for (i, b) in bones.iter().enumerate() {
                let p = parent_of(b);
                let parent_ok = p >= 0 && (p as usize) < bones.len() && p as usize != i;
                let (t, r) = match (b.floats("lp"), b.floats("lr")) {
                    (Some(lp), lr) => (v3_at(&lp, 0) * k, quat_to_m4(lr.map(|v| q_of(&v)).unwrap_or([0.0, 0.0, 0.0, 1.0])).to_euler_deg()),
                    (None, _) => {
                        let pw = if parent_ok { worlds[p as usize] } else { M4::IDENTITY };
                        let (t, r, _) = (pw.inverse() * worlds[i]).decompose();
                        (t, r)
                    }
                };
                let s = b.floats("s").map(|v| v3_at(&v, 0)).unwrap_or(V3::ONE);
                let pid = if parent_ok { ids[p as usize] } else { group };
                if let Some(n) = self.scene.get_mut(ids[i]) {
                    n.parent = Some(pid);
                    n.t = t;
                    n.r = r;
                    n.s = s;
                    n.rest = Some([t, r, s]);
                }
            }
            // joint display size from the typical bone length
            let w = self.scene.world_matrices();
            let mut lens: Vec<f32> = ids
                .iter()
                .filter_map(|id| {
                    let p = self.scene.get(*id)?.parent?;
                    Some((w[id].translation() - w.get(&p)?.translation()).len())
                })
                .filter(|l| *l > 1e-4)
                .collect();
            lens.sort_by(|a, b| a.total_cmp(b));
            let radius = lens.get(lens.len() / 2).map(|l| (l * 0.15).max(0.01)).unwrap_or(0.12);
            for id in &ids {
                if let Some(n) = self.scene.get_mut(*id) {
                    n.joint_radius = radius;
                }
            }
            // materials: hash -> (name, texture path, colour)
            let mut mats: HashMap<u64, (String, Option<String>, Option<[f32; 3]>)> = HashMap::new();
            for m in model.kids(ID_MATERIAL) {
                let name = m.str("n").unwrap_or("material").to_string();
                let mut tex = None;
                let mut color = None;
                for slot in ["albedo", "diffuse"] {
                    let Some(h) = m.int(slot) else { continue };
                    if let Some(f) = m.kids(ID_FILE).find(|f| f.hash == h) {
                        if let Some(p) = f.str("p") {
                            let pb = PathBuf::from(p);
                            let full = if pb.is_absolute() { pb } else { dir.join(pb) };
                            tex = Some(full);
                        }
                    }
                    if let Some(c) = m.kids(ID_COLOR).find(|c| c.hash == h) {
                        if let Some(v) = c.floats("rgba") {
                            color = Some([v.first().copied().unwrap_or(0.6), v.get(1).copied().unwrap_or(0.6), v.get(2).copied().unwrap_or(0.6)]);
                        }
                    }
                    if tex.is_some() || color.is_some() {
                        break;
                    }
                }
                let tex = tex.and_then(|t| {
                    if t.exists() {
                        Some(t.to_string_lossy().to_string())
                    } else {
                        self.log.push(crate::trf!("Textur fehlt: {}", "Texture missing: {}", t.display()));
                        None
                    }
                });
                mats.insert(m.hash, (name, tex, color));
            }
            // meshes
            let mut mesh_count = 0;
            for (ci, cm) in model.kids(ID_MESH).enumerate() {
                let Some(vp) = cm.floats("vp") else { continue };
                let faces = cm.ints("f").map(|f| f.to_vec()).unwrap_or_default();
                let nv = vp.len() / 3;
                let mut mesh = Mesh::default();
                mesh.verts = (0..nv).map(|i| v3_at(&vp, i) * k).collect();
                let uv0 = if cm.int("ul").unwrap_or(1) > 0 { cm.floats("u0") } else { None };
                for f in faces.chunks_exact(3) {
                    let (a, b, c) = (f[0] as u32, f[1] as u32, f[2] as u32);
                    if a == b || a == c || b == c || [a, b, c].iter().any(|i| *i as usize >= nv) {
                        continue;
                    }
                    if let Some(uv) = &uv0 {
                        mesh.uvs.push([a, b, c].iter().map(|i| [uv.get(*i as usize * 2).copied().unwrap_or(0.0), uv.get(*i as usize * 2 + 1).copied().unwrap_or(0.0)]).collect());
                    }
                    mesh.faces.push(vec![a, b, c]);
                }
                if mesh.faces.is_empty() {
                    continue;
                }
                let name = cm.str("n").filter(|s| !s.is_empty()).map(|s| s.to_string()).unwrap_or_else(|| format!("{gname}_mesh{ci}"));
                let name = if self.scene.nodes.iter().any(|n| n.name == name) { self.scene.unique_name(&name) } else { name };
                let mut n = Node::new(0, &name, Kind::Mesh);
                n.parent = Some(group);
                if let Some((mname, tex, color)) = cm.int("m").and_then(|h| mats.get(&h)) {
                    n.material.name = mname.clone();
                    n.material.specular = 0.1;
                    if let Some(c) = color {
                        n.material.color = *c;
                    }
                    if let Some(t) = tex {
                        n.material.texture = Some(t.clone());
                        n.material.color = [1.0, 1.0, 1.0];
                    }
                }
                n.mesh = Some(mesh);
                let mid = self.scene.add(n);
                // skin weights
                let mi = cm.int("mi").unwrap_or(0) as usize;
                if let (Some(wb), Some(wv)) = (cm.ints("wb"), cm.floats("wv")) {
                    if mi > 0 && !ids.is_empty() && wb.len() >= nv * mi && wv.len() >= nv * mi {
                        let w = self.scene.world_matrices();
                        let weights = (0..nv)
                            .map(|v| {
                                let ws: Vec<(usize, f32)> = (0..mi).map(|j| (wb[v * mi + j] as usize, wv[v * mi + j])).collect();
                                normalize_weights(ws, ids.len())
                            })
                            .collect();
                        let bind_inv = ids.iter().map(|id| w[id].inverse()).collect();
                        let mw = w[&mid];
                        if let Some(n) = self.scene.get_mut(mid) {
                            n.skin = Some(Skin { joints: ids.clone(), bind_inv, weights, bind_mesh_world: mw });
                        }
                    }
                }
                mesh_count += 1;
            }
            let _ = by_hash;
            self.cod.models.push(group);
            report.push(crate::trf!("„{gname}“ ({} Gelenke, {mesh_count} Objekte)", "\"{gname}\" ({} joints, {mesh_count} objects)", ids.len()));
        }
        self.sel = groups;
        self.frame_view(false);
        Ok(crate::trf!("Cast-Modell importiert: {}", "Cast model imported: {}", report.join(", ")))
    }

    // ---------------------------------------------------------------- import animation

    /// Puts the first animation of a cast file on the scene's joints.
    pub fn cast_import_anim(&mut self, path: &Path) -> Result<String, String> {
        let data = std::fs::read(path).map_err(|e| e.to_string())?;
        let roots = parse_cast(&data)?;
        let Some(anim) = roots.iter().flat_map(|r| r.kids(ID_ANIMATION)).next() else {
            return Err(crate::tr!("Die Datei enthält keine Animation.", "The file contains no animation.").into());
        };
        let k = self.cast_k();
        let fps = anim.floats("fr").and_then(|v| v.first().copied()).unwrap_or(30.0);
        // Like the XAnim attach: the animation drives the joints of all
        // ticked models (arms, weapon, scope …) – not other scene objects.
        let targets = self.cod_targets();
        if targets.is_empty() {
            return Err(crate::tr!(
                "Bitte zuerst die Modelle importieren (z. B. Arme und Waffe) – die Animation wird dann an alle angehängt.",
                "Please import the models first (e.g. arms and weapon) – the animation is then attached to all of them."
            )
            .into());
        }
        // each model's own joints (attached models are listed separately)
        let mut pool: Vec<Id> = Vec::new();
        for t in &targets {
            for id in self.model_nodes(*t) {
                if !pool.contains(&id) && self.scene.get(id).is_some_and(|n| n.kind != Kind::Mesh && n.kind != Kind::Group) {
                    pool.push(id);
                }
            }
        }
        let find_all = |s: &Scene, name: &str| -> Vec<Id> { pool.iter().copied().filter(|id| s.get(*id).is_some_and(|n| strip_ns(&n.name).eq_ignore_ascii_case(name))).collect() };
        let mut curve_names: Vec<String> = Vec::new();
        for c in anim.kids(ID_CURVE) {
            let nn = strip_ns(c.str("nn").unwrap_or("")).to_string();
            if !nn.is_empty() && !curve_names.iter().any(|n| n.eq_ignore_ascii_case(&nn)) {
                curve_names.push(nn);
            }
        }
        let unmatched: Vec<String> = curve_names.iter().filter(|n| find_all(&self.scene, n).is_empty()).cloned().collect();
        if unmatched.len() == curve_names.len() && !curve_names.is_empty() && !self.cod.create_missing {
            let names: Vec<String> = targets.iter().filter_map(|t| self.scene.get(*t).map(|n| n.name.clone())).collect();
            return Err(crate::trf!(
                "Kein Gelenk der Animation passt zu den Modellen ({}).",
                "No joint of the animation matches the models ({}).",
                names.join(", ")
            ));
        }
        self.checkpoint();
        // joints only the animation has are added to the first model; an
        // optional skeleton in the animation tells their parent and rest pose
        let anim_bones: Vec<&CNode> = anim.kids(ID_SKELETON).flat_map(|s| s.kids(ID_BONE)).collect();
        let mut created: Vec<Id> = Vec::new();
        if self.cod.create_missing {
            let first = targets[0];
            let radius = pool.iter().filter_map(|id| self.scene.get(*id)).find(|n| n.kind == Kind::Joint).map(|n| n.joint_radius).unwrap_or(0.12);
            for name in &unmatched {
                let mut n = Node::new(0, name, Kind::Joint);
                n.parent = Some(first);
                n.joint_radius = radius;
                if let Some(b) = anim_bones.iter().find(|b| b.str("n").is_some_and(|bn| strip_ns(bn).eq_ignore_ascii_case(name))) {
                    let p = b.int("p").map(|p| p as u32 as i32).unwrap_or(-1);
                    if p >= 0 {
                        if let Some(pn) = anim_bones.get(p as usize).and_then(|pb| pb.str("n")) {
                            if let Some(pid) = find_all(&self.scene, strip_ns(pn)).first() {
                                n.parent = Some(*pid);
                            }
                        }
                    }
                    if let Some(lp) = b.floats("lp") {
                        n.t = v3_at(&lp, 0) * k;
                    }
                    if let Some(lr) = b.floats("lr") {
                        n.r = quat_to_m4(q_of(&lr)).to_euler_deg();
                    }
                }
                n.rest = Some([n.t, n.r, n.s]);
                created.push(self.scene.add(n));
            }
        }
        // reset the chosen models to their bind pose (other objects stay as they are)
        let mut skeleton: Vec<Id> = pool.clone();
        skeleton.extend(created.iter().copied());
        for id in &skeleton {
            if let Some(n) = self.scene.get_mut(*id) {
                if let Some(rest) = n.rest {
                    n.t = rest[0];
                    n.r = rest[1];
                    n.s = rest[2];
                }
                if self.cod.replace_anim {
                    for c in 0..CHANNELS {
                        n.curves[c] = Curve::default();
                    }
                }
            }
        }
        if self.cod.replace_anim {
            self.scene.notes.clear();
        }
        // Cast keys are local to the parent joint: a model whose root joint
        // hangs under a joint of another model in the animation (weapon in
        // the hand) is attached to that joint, like binding a gun to the arms.
        let mut attached: Vec<String> = Vec::new();
        for b in &anim_bones {
            let p = b.int("p").map(|p| p as u32 as i32).unwrap_or(-1);
            let (Some(name), Some(pname)) = (b.str("n"), (p >= 0).then(|| anim_bones.get(p as usize).and_then(|pb| pb.str("n"))).flatten()) else { continue };
            for &j in &skeleton {
                let Some(jn) = self.scene.get(j) else { continue };
                if !strip_ns(&jn.name).eq_ignore_ascii_case(strip_ns(name)) {
                    continue;
                }
                // only a model's root joint can be attached
                let Some(g) = jn.parent.filter(|g| targets.contains(g)) else { continue };
                if self.scene.get(g).and_then(|gn| gn.parent).is_some() {
                    continue; // model already attached somewhere
                }
                let parent_joint = skeleton
                    .iter()
                    .copied()
                    .find(|pj| !self.scene.is_ancestor(g, *pj) && self.scene.get(*pj).is_some_and(|n| n.kind == Kind::Joint && strip_ns(&n.name).eq_ignore_ascii_case(strip_ns(pname))));
                if let Some(pj) = parent_joint {
                    self.attach_model(g, Some(pj));
                    let gname = self.scene.get(g).map(|n| n.name.clone()).unwrap_or_default();
                    attached.push(format!("{gname} → {pname}"));
                }
            }
        }
        // a joint whose parent in the animation differs from its parent in the
        // model is moved under the right parent of the same model (keeping its
        // place), so the local keys put it where the animation wants it
        let mut reparented = 0;
        for b in &anim_bones {
            let p = b.int("p").map(|p| p as u32 as i32).unwrap_or(-1);
            let (Some(name), Some(pname)) = (b.str("n"), (p >= 0).then(|| anim_bones.get(p as usize).and_then(|pb| pb.str("n"))).flatten()) else { continue };
            for &j in &skeleton {
                let Some(jn) = self.scene.get(j) else { continue };
                if !strip_ns(&jn.name).eq_ignore_ascii_case(strip_ns(name)) {
                    continue;
                }
                let Some(cur_parent) = jn.parent else { continue };
                let cur_is_joint = self.scene.get(cur_parent).is_some_and(|pn| pn.kind == Kind::Joint);
                let cur_name = self.scene.get(cur_parent).map(|pn| strip_ns(&pn.name).to_lowercase()).unwrap_or_default();
                if !cur_is_joint || cur_name == strip_ns(pname).to_lowercase() {
                    continue;
                }
                // owner model of j, and the wanted parent inside it
                let owner = targets.iter().copied().find(|t| self.model_nodes(*t).contains(&j));
                let Some(owner) = owner else { continue };
                let want = self
                    .model_nodes(owner)
                    .into_iter()
                    .find(|q| *q != j && !self.scene.is_ancestor(j, *q) && self.scene.get(*q).is_some_and(|n| n.kind == Kind::Joint && strip_ns(&n.name).eq_ignore_ascii_case(strip_ns(pname))));
                if let Some(q) = want {
                    let w = self.scene.world_matrices();
                    let local = w[&q].inverse() * w[&j];
                    let (t, r, s) = local.decompose();
                    if let Some(n) = self.scene.get_mut(j) {
                        n.parent = Some(q);
                        n.t = t;
                        n.r = r;
                        n.s = s;
                        n.rest = Some([t, r, s]);
                    }
                    reparented += 1;
                }
            }
        }
        let start = self.scene.start;
        let overrides: Vec<(String, String, bool, bool, bool)> = anim
            .kids(ID_CURVE_OVERRIDE)
            .map(|o| {
                (
                    strip_ns(o.str("nn").unwrap_or("")).to_lowercase(),
                    o.str("m").unwrap_or("absolute").to_lowercase(),
                    o.int("ot").unwrap_or(0) != 0,
                    o.int("or").unwrap_or(0) != 0,
                    o.int("os").unwrap_or(0) != 0,
                )
            })
            .collect();
        let mut animated: Vec<Id> = Vec::new();
        let mut missing: Vec<String> = Vec::new();
        let mut max_frame = 0u64;
        let mut new_keys: HashMap<Id, Vec<Vec<Key>>> = HashMap::new();
        for c in anim.kids(ID_CURVE) {
            let nn = strip_ns(c.str("nn").unwrap_or("")).to_string();
            let kp = c.str("kp").unwrap_or("").to_lowercase();
            let mode_in = c.str("m").unwrap_or("absolute").to_lowercase();
            let blend = c.floats("ab").and_then(|v| v.first().copied()).unwrap_or(1.0);
            let frames: Vec<u64> = c.ints("kb").map(|v| v.to_vec()).unwrap_or_default();
            let values = c.floats("kv").unwrap_or_default();
            if frames.is_empty() {
                continue;
            }
            max_frame = max_frame.max(frames.iter().copied().max().unwrap_or(0));
            // every model joint with this name is driven (e.g. tag_origin in arms and weapon)
            let targets: Vec<Id> = skeleton.iter().copied().filter(|id| self.scene.get(*id).is_some_and(|n| strip_ns(&n.name).eq_ignore_ascii_case(&nn))).collect();
            if targets.is_empty() {
                if !missing.contains(&nn) {
                    missing.push(nn);
                }
                continue;
            }
            for id in targets {
                // mode override from an ancestor (root first)
                let mut chain = Vec::new();
                let mut cur = self.scene.get(id).and_then(|n| n.parent);
                while let Some(p) = cur {
                    chain.push(p);
                    cur = self.scene.get(p).and_then(|n| n.parent);
                }
                let mut mode = mode_in.clone();
                'outer: for a in chain.iter().rev() {
                    let an = self.scene.get(*a).map(|n| strip_ns(&n.name).to_lowercase()).unwrap_or_default();
                    for (on, om, ot, or, os) in &overrides {
                        let applies = match kp.as_str() {
                            "tx" | "ty" | "tz" => *ot,
                            "rq" => *or,
                            "sx" | "sy" | "sz" => *os,
                            _ => false,
                        };
                        if applies && *on == an {
                            mode = om.clone();
                            break 'outer;
                        }
                    }
                }
                let Some(n) = self.scene.get_mut(id) else { continue };
                let rest = *n.rest.get_or_insert([n.t, n.r, n.s]);
                let ch = new_keys.entry(id).or_insert_with(|| vec![Vec::new(); CHANNELS]);
                let lin = Tangent::Linear;
                match kp.as_str() {
                    "tx" | "ty" | "tz" => {
                        let axis = (kp.as_bytes()[1] - b'x') as usize;
                        for (i, f) in frames.iter().enumerate() {
                            let v = values.get(i).copied().unwrap_or(0.0) * k;
                            let r = rest[0].get(axis);
                            let val = match mode.as_str() {
                                "relative" => r + v,
                                "additive" => r + v * blend,
                                _ => v,
                            };
                            ch[axis].push(Key { frame: start + *f as f32, value: val, tangent: lin });
                        }
                    }
                    "sx" | "sy" | "sz" => {
                        let axis = (kp.as_bytes()[1] - b'x') as usize;
                        for (i, f) in frames.iter().enumerate() {
                            let v = values.get(i).copied().unwrap_or(1.0);
                            let r = rest[2].get(axis);
                            let val = match mode.as_str() {
                                "relative" | "additive" => r * v,
                                _ => v,
                            };
                            ch[6 + axis].push(Key { frame: start + *f as f32, value: val, tangent: lin });
                        }
                    }
                    "rq" => {
                        let rest_q = M4::euler_deg(rest[1]).to_quat();
                        let mut order: Vec<usize> = (0..frames.len()).collect();
                        order.sort_by_key(|i| frames[*i]);
                        let mut prev = rest[1];
                        for i in order {
                            let q = q_of(values.get(i * 4..i * 4 + 4).unwrap_or(&[]));
                            let fq = match mode.as_str() {
                                // rest rotation, then the key (Maya: frame * rest)
                                "relative" => qmul(rest_q, q),
                                "additive" => slerp(rest_q, qmul(rest_q, q), blend),
                                _ => q,
                            };
                            let mut e = quat_to_m4(fq).to_euler_deg();
                            e = v3(unwrap(prev.x, e.x), unwrap(prev.y, e.y), unwrap(prev.z, e.z));
                            prev = e;
                            for a in 0..3 {
                                ch[3 + a].push(Key { frame: start + frames[i] as f32, value: e.get(a), tangent: lin });
                            }
                        }
                    }
                    "vb" => {
                        for (i, f) in frames.iter().enumerate() {
                            let v = if values.get(i).copied().unwrap_or(1.0) >= 1.0 { 1.0 } else { 0.0 };
                            ch[9].push(Key { frame: start + *f as f32, value: v, tangent: Tangent::Step });
                        }
                    }
                    _ => {}
                }
                if !animated.contains(&id) {
                    animated.push(id);
                }
            }
        }
        for (id, chans) in new_keys {
            if let Some(n) = self.scene.get_mut(id) {
                for (c, keys) in chans.into_iter().enumerate() {
                    if keys.is_empty() {
                        continue;
                    }
                    n.curves[c].keys = keys;
                    n.curves[c].sort();
                }
            }
        }
        // notification tracks
        let mut note_count = 0;
        for t in anim.kids(ID_NOTIFICATION) {
            let name = t.str("n").unwrap_or("").to_string();
            if name.trim().is_empty() {
                continue;
            }
            for f in t.ints("kb").unwrap_or(&[]) {
                max_frame = max_frame.max(*f);
                self.scene.notes.push(Note { frame: start + *f as f32, name: name.clone() });
                note_count += 1;
            }
        }
        self.scene.notes.sort_by(|a, b| a.frame.total_cmp(&b.frame));
        if fps > 0.0 {
            self.scene.fps = fps;
        }
        self.scene.end = start + (max_frame.max(1)) as f32;
        if anim.int("lo").unwrap_or(0) != 0 {
            self.loop_play = true;
        }
        for n in &mut self.scene.nodes {
            if n.kind == Kind::IkHandle && animated.iter().any(|j| Some(*j) == n.ik_end) {
                n.visible = false;
            }
        }
        // joints shared by name across models sit where the animated one is
        let aligned = self.align_shared_joints(&targets);
        self.set_frame(start);
        self.graph.fit_request = true;
        let report: Vec<String> = targets
            .iter()
            .map(|t| {
                let sub = self.model_nodes(*t);
                let moved = animated.iter().filter(|id| sub.contains(id)).count();
                format!("{} ({moved})", self.scene.get(*t).map(|n| n.name.clone()).unwrap_or_default())
            })
            .collect();
        let mut msg = crate::trf!(
            "Cast-Animation an {} Modell(e) angehängt – bewegte Gelenke: {} · {} Bilder bei {} B/s · {} neu angelegt · {} Notizen",
            "Cast animation attached to {} model(s) – moved joints: {} · {} frames at {} fps · {} created · {} notes",
            targets.len(),
            report.join(", "),
            max_frame + 1,
            fps,
            created.len(),
            note_count
        );
        if aligned > 0 {
            msg.push_str(&crate::trf!(" · {aligned} gleichnamige Gelenke ausgerichtet", " · {aligned} same-named joints aligned"));
        }
        if reparented > 0 {
            msg.push_str(&crate::trf!(" · {reparented} Gelenke wie in der Animation umgehängt", " · {reparented} joints re-parented like in the animation"));
        }
        if !attached.is_empty() {
            msg.push_str(&crate::trf!(" · angehängt: {}", " · attached: {}", attached.join(", ")));
        }
        if !missing.is_empty() {
            msg.push_str(&crate::trf!(" · {} Gelenke der Animation in keinem Modell", " · {} animation joints are in no model", missing.len()));
        }
        Ok(msg)
    }

    // ---------------------------------------------------------------- export

    fn cast_export_joints(&self) -> Vec<Id> {
        let sel: Vec<Id> = self.sel.iter().copied().filter(|id| self.scene.get(*id).is_some_and(|n| n.kind == Kind::Joint)).collect();
        if !sel.is_empty() {
            return sel;
        }
        // all joints, parents before children
        let mut out = Vec::new();
        for r in self.scene.children(None) {
            for id in self.scene.subtree(r) {
                if self.scene.get(id).is_some_and(|n| n.kind == Kind::Joint) {
                    out.push(id);
                }
            }
        }
        out
    }

    /// Local matrix of a bone relative to its nearest exported ancestor.
    fn cast_local(&self, w: &Worlds, list: &[Id], id: Id) -> (M4, i64) {
        let mut cur = self.scene.get(id).and_then(|n| n.parent);
        while let Some(p) = cur {
            if let Some(i) = list.iter().position(|x| *x == p) {
                return (w[&p].inverse() * w[&id], i as i64);
            }
            cur = self.scene.get(p).and_then(|n| n.parent);
        }
        (w[&id], -1)
    }

    fn cast_meta() -> CNode {
        let mut m = CNode::new(ID_METADATA);
        m.set_str("s", "Animora 3D").set_str("up", "y");
        m
    }

    pub fn cast_export_anim(&mut self, path: &Path) -> Result<String, String> {
        let joints = self.cast_export_joints();
        if joints.is_empty() && self.scene.notes.is_empty() {
            return Err(crate::tr!("Keine Gelenke oder Notizen zum Exportieren.", "No joints or notes to export.").into());
        }
        let k = 1.0 / self.cast_k();
        let (start, end) = (self.scene.start.round(), self.scene.end.round().max(self.scene.start));
        let n_frames = (end - start) as usize + 1;
        let mut tr: Vec<[Vec<f32>; 3]> = vec![[Vec::new(), Vec::new(), Vec::new()]; joints.len()];
        let mut sc: Vec<[Vec<f32>; 3]> = vec![[Vec::new(), Vec::new(), Vec::new()]; joints.len()];
        let mut rq: Vec<Vec<f32>> = vec![Vec::new(); joints.len()];
        let mut s = self.scene.clone();
        for fi in 0..n_frames {
            s.evaluate(start + fi as f32);
            let w = s.world_matrices();
            let me = App::cast_local_in(&s, &w, &joints);
            for (bi, (m, _)) in me.iter().enumerate() {
                let (t, _, sv) = m.decompose();
                for a in 0..3 {
                    tr[bi][a].push(t.get(a) * k);
                    sc[bi][a].push(sv.get(a));
                }
                let mut q = m.to_quat();
                // keep neighbouring quaternions on the same hemisphere
                if let Some(p) = rq[bi].len().checked_sub(4).map(|i| &rq[bi][i..]) {
                    if p[0] * q[0] + p[1] * q[1] + p[2] * q[2] + p[3] * q[3] < 0.0 {
                        q = [-q[0], -q[1], -q[2], -q[3]];
                    }
                }
                rq[bi].extend_from_slice(&q);
            }
        }
        let kb: Vec<u64> = (0..n_frames as u64).collect();
        let kb_ty = int_type_for(&kb);
        let mut node = CNode::new(ID_ANIMATION);
        node.set_str("n", &path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default());
        node.set_floats("fr", *b"f\0", vec![self.scene.fps]);
        node.set_ints("lo", *b"b\0", vec![self.loop_play as u64]);
        for (bi, id) in joints.iter().enumerate() {
            let name = strip_ns(&self.scene.get(*id).unwrap().name).to_string();
            let mut add = |kp: &str, ty: [u8; 2], vals: Vec<f32>| {
                let mut c = CNode::new(ID_CURVE);
                c.set_str("nn", &name).set_str("kp", kp).set_str("m", "absolute");
                c.set_ints("kb", kb_ty, kb.clone());
                c.set_floats("kv", ty, vals);
                node.children.push(c);
            };
            add("rq", *b"4v", rq[bi].clone());
            for (a, kp) in ["tx", "ty", "tz"].iter().enumerate() {
                add(kp, *b"f\0", tr[bi][a].clone());
            }
            if sc[bi].iter().flatten().any(|v| (v - 1.0).abs() > 1e-4) {
                for (a, kp) in ["sx", "sy", "sz"].iter().enumerate() {
                    add(kp, *b"f\0", sc[bi][a].clone());
                }
            }
        }
        // the joint hierarchy of the animation (parents + first-frame pose), so
        // importers can attach separate models (weapon to hand) correctly
        {
            let mut s0 = self.scene.clone();
            s0.evaluate(start);
            let w0 = s0.world_matrices();
            let locals = App::cast_local_in(&s0, &w0, &joints);
            let mut skel = CNode::new(ID_SKELETON);
            for (id, (m, parent)) in joints.iter().zip(locals) {
                let (t, _, sv) = m.decompose();
                let mut b = CNode::new(ID_BONE);
                b.set_str("n", strip_ns(&self.scene.get(*id).unwrap().name));
                b.set_ints("p", *b"i\0", vec![(parent as i32) as u32 as u64]);
                b.set_floats("lp", *b"3v", vec![t.x * k, t.y * k, t.z * k]);
                b.set_floats("lr", *b"4v", m.to_quat().to_vec());
                b.set_floats("s", *b"3v", vec![sv.x, sv.y, sv.z]);
                skel.children.push(b);
            }
            node.children.push(skel);
        }
        // notetracks: one track per name
        let mut by_name: Vec<(String, Vec<u64>)> = Vec::new();
        for nt in self.scene.notes.iter().filter(|n| n.frame >= start && n.frame <= end && !n.name.trim().is_empty()) {
            let f = (nt.frame - start).round() as u64;
            match by_name.iter_mut().find(|(n, _)| *n == nt.name) {
                Some((_, v)) => v.push(f),
                None => by_name.push((nt.name.clone(), vec![f])),
            }
        }
        for (name, frames) in &by_name {
            let mut t = CNode::new(ID_NOTIFICATION);
            t.set_str("n", name);
            t.set_ints("kb", int_type_for(frames), frames.clone());
            node.children.push(t);
        }
        let mut root = CNode::new(ID_ROOT);
        root.children.push(node);
        root.children.push(App::cast_meta());
        std::fs::write(path, write_cast(&[root])).map_err(|e| e.to_string())?;
        Ok(crate::trf!(
            "Cast-Animation exportiert: {} Gelenke, {} Bilder, {} Notizen",
            "Cast animation exported: {} joints, {} frames, {} notes",
            joints.len(),
            n_frames,
            by_name.iter().map(|b| b.1.len()).sum::<usize>()
        ))
    }

    /// Local matrices (relative to the nearest exported ancestor) for a list of joints.
    fn cast_local_in(s: &Scene, w: &Worlds, list: &[Id]) -> Vec<(M4, i64)> {
        list.iter()
            .map(|id| {
                let mut cur = s.get(*id).and_then(|n| n.parent);
                while let Some(p) = cur {
                    if let Some(i) = list.iter().position(|x| *x == p) {
                        return (w[&p].inverse() * w[id], i as i64);
                    }
                    cur = s.get(p).and_then(|n| n.parent);
                }
                (w[id], -1)
            })
            .collect()
    }

    pub fn cast_export_model(&mut self, path: &Path) -> Result<String, String> {
        let k = 1.0 / self.cast_k();
        let w = self.scene.world_matrices();
        let meshes: Vec<Id> = {
            let sel: Vec<Id> = self.sel.iter().copied().filter(|id| self.scene.get(*id).is_some_and(|n| n.kind == Kind::Mesh)).collect();
            if sel.is_empty() && self.sel.iter().all(|id| self.scene.get(*id).is_none_or(|n| n.kind != Kind::Joint)) {
                self.scene.nodes.iter().filter(|n| n.kind == Kind::Mesh && self.scene.is_visible(n.id)).map(|n| n.id).collect()
            } else {
                sel
            }
        };
        let mut joints = self.cast_export_joints();
        for m in &meshes {
            if let Some(skin) = self.scene.get(*m).and_then(|n| n.skin.as_ref()) {
                for j in &skin.joints {
                    if !joints.contains(j) {
                        joints.push(*j);
                    }
                }
            }
        }
        if joints.is_empty() && meshes.is_empty() {
            return Err(crate::tr!("Nichts zum Exportieren gefunden.", "Nothing found to export.").into());
        }
        let mut model = CNode::new(ID_MODEL);
        model.set_str("n", &path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or("model".into()));
        let mut skel = CNode::new(ID_SKELETON);
        for id in &joints {
            let (local, parent) = self.cast_local(&w, &joints, *id);
            let n = self.scene.get(*id).unwrap();
            let (lt, _, ls) = local.decompose();
            let wm = w[id];
            let mut b = CNode::new(ID_BONE);
            b.set_str("n", strip_ns(&n.name));
            b.set_ints("p", *b"i\0", vec![(parent as i32) as u32 as u64]);
            b.set_floats("lp", *b"3v", vec![lt.x * k, lt.y * k, lt.z * k]);
            b.set_floats("lr", *b"4v", local.to_quat().to_vec());
            let wt = wm.translation();
            b.set_floats("wp", *b"3v", vec![wt.x * k, wt.y * k, wt.z * k]);
            b.set_floats("wr", *b"4v", wm.to_quat().to_vec());
            b.set_floats("s", *b"3v", vec![ls.x, ls.y, ls.z]);
            skel.children.push(b);
        }
        if !joints.is_empty() {
            model.children.push(skel);
        }
        let mut mat_hash: HashMap<String, u64> = HashMap::new();
        let mut tris = 0;
        for &mid in &meshes {
            let n = self.scene.get(mid).unwrap();
            let Some(mesh) = &n.mesh else { continue };
            let verts = self.scene.deformed_verts(n, mesh, &w);
            let corner_n = mesh.corner_normals(&verts, 50.0);
            let has_uv = mesh.has_uvs();
            // one UV per vertex in Cast: split vertices where UVs differ
            let mut map: HashMap<(u32, u32, u32), u32> = HashMap::new();
            let (mut vp, mut vn, mut uv, mut src) = (Vec::new(), Vec::<V3>::new(), Vec::new(), Vec::new());
            let mut faces: Vec<u64> = Vec::new();
            for (fi, f) in mesh.faces.iter().enumerate() {
                let mut idx = Vec::new();
                for (c, &v) in f.iter().enumerate() {
                    let t = if has_uv { mesh.uvs[fi].get(c).copied().unwrap_or([0.0, 0.0]) } else { [0.0, 0.0] };
                    let i = *map.entry((v, t[0].to_bits(), t[1].to_bits())).or_insert_with(|| {
                        let p = verts[v as usize] * k;
                        vp.extend_from_slice(&[p.x, p.y, p.z]);
                        vn.push(V3::ZERO);
                        uv.extend_from_slice(&t);
                        src.push(v);
                        (src.len() - 1) as u32
                    });
                    vn[i as usize] += corner_n[fi][c];
                    idx.push(i as u64);
                }
                // Cast faces are counter-clockwise like ours
                for c in 1..idx.len().saturating_sub(1) {
                    faces.extend_from_slice(&[idx[0], idx[c], idx[c + 1]]);
                    tris += 1;
                }
            }
            let mut cm = CNode::new(ID_MESH);
            cm.set_str("n", strip_ns(&n.name));
            cm.set_floats("vp", *b"3v", vp);
            cm.set_floats("vn", *b"3v", vn.iter().flat_map(|v| {
                let v = v.norm();
                [v.x, v.y, v.z]
            }).collect());
            cm.set_ints("ul", *b"b\0", vec![1]);
            cm.set_floats("u0", *b"2v", uv);
            let fty = int_type_for(&[src.len() as u64]);
            cm.set_ints("f", fty, faces);
            if let Some(skin) = &n.skin {
                if skin.weights.len() == mesh.verts.len() {
                    let per: Vec<Vec<(u64, f32)>> = src
                        .iter()
                        .map(|v| {
                            skin.weights[*v as usize]
                                .iter()
                                .filter(|(_, wt)| *wt > 1e-6)
                                .filter_map(|(j, wt)| skin.joints.get(*j).and_then(|jid| joints.iter().position(|x| x == jid)).map(|b| (b as u64, *wt)))
                                .collect()
                        })
                        .collect();
                    let mi = per.iter().map(|p| p.len()).max().unwrap_or(0).max(1);
                    let mut wb = Vec::new();
                    let mut wv = Vec::new();
                    for p in &per {
                        for i in 0..mi {
                            let (b, wt) = p.get(i).copied().unwrap_or((0, 0.0));
                            wb.push(b);
                            wv.push(wt);
                        }
                    }
                    cm.set_ints("mi", *b"b\0", vec![mi as u64]);
                    cm.set_ints("wb", int_type_for(&[joints.len() as u64]), wb);
                    cm.set_floats("wv", *b"f\0", wv);
                }
            }
            // material (one per name)
            let mname = if n.material.name.is_empty() { format!("{}_mat", strip_ns(&n.name)) } else { n.material.name.clone() };
            let hash = match mat_hash.get(&mname) {
                Some(h) => *h,
                None => {
                    let mut m = CNode::new(ID_MATERIAL);
                    m.set_str("n", &mname).set_str("t", "pbr");
                    if let Some(t) = &n.material.texture {
                        let mut f = CNode::new(ID_FILE);
                        f.set_str("p", t);
                        m.set_ints("diffuse", *b"l\0", vec![f.hash]);
                        m.children.push(f);
                    } else {
                        let c = n.material.color;
                        let mut col = CNode::new(ID_COLOR);
                        col.set_floats("rgba", *b"4v", vec![c[0], c[1], c[2], 1.0]);
                        m.set_ints("diffuse", *b"l\0", vec![col.hash]);
                        m.children.push(col);
                    }
                    let h = m.hash;
                    model.children.push(m);
                    mat_hash.insert(mname, h);
                    h
                }
            };
            cm.set_ints("m", *b"l\0", vec![hash]);
            model.children.push(cm);
        }
        let mut root = CNode::new(ID_ROOT);
        root.children.push(model);
        root.children.push(App::cast_meta());
        std::fs::write(path, write_cast(&[root])).map_err(|e| e.to_string())?;
        Ok(crate::trf!(
            "Cast-Modell exportiert: {} Gelenke, {} Objekte, {} Dreiecke",
            "Cast model exported: {} joints, {} objects, {} triangles",
            joints.len(),
            meshes.len(),
            tris
        ))
    }

    // ---------------------------------------------------------------- dialogs & menu

    pub fn cast_import_dialog(&mut self) {
        let Some(paths) = rfd::FileDialog::new().add_filter("Cast", &["cast", "CAST"]).pick_files() else { return };
        let msg = self.import_files(paths);
        self.report(Ok(msg), crate::tr!("Import", "Import"));
    }

    pub fn cast_export_anim_dialog(&mut self) {
        let Some(p) = rfd::FileDialog::new().add_filter("Cast", &["cast"]).set_file_name("animation.cast").save_file() else { return };
        let r = self.cast_export_anim(&p);
        self.report(r, crate::tr!("Export", "Export"));
    }

    pub fn cast_export_model_dialog(&mut self) {
        let Some(p) = rfd::FileDialog::new().add_filter("Cast", &["cast"]).set_file_name(crate::tr!("modell.cast", "model.cast")).save_file() else { return };
        let r = self.cast_export_model(&p);
        self.report(r, crate::tr!("Export", "Export"));
    }

    pub fn cast_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("Cast", |ui| {
            if ui
                .button(crate::tr!("Cast importieren (Modelle + Animationen) …", "Import Cast (models + animations) …"))
                .on_hover_text(crate::tr!("Mehrere Dateien möglich – Modelle zuerst, jede Animation wird ein Eintrag der Animationsliste", "Several files at once – models first, every animation becomes an entry of the animation list"))
                .clicked()
            {
                self.cast_import_dialog();
                ui.close();
            }
            ui.separator();
            if ui.button(crate::tr!("Modell als Cast exportieren …", "Export model as Cast …")).clicked() {
                self.cast_export_model_dialog();
                ui.close();
            }
            if ui.button(crate::tr!("Animation als Cast exportieren …", "Export animation as Cast …")).clicked() {
                self.cast_export_anim_dialog();
                ui.close();
            }
            ui.separator();
            ui.label(egui::RichText::new(crate::tr!("Cast: Modelle und Animationen in einem offenen Format.", "Cast: models and animations in one open format.")).small().color(egui::Color32::from_gray(160)));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wrist_path(a: &App, wr: Id, n: usize) -> Vec<V3> {
        // evaluated like the export (IK depends on the frame it starts from)
        let mut s = a.scene.clone();
        (1..=n)
            .map(|f| {
                s.evaluate(f as f32);
                s.world_matrices()[&wr].translation()
            })
            .collect()
    }

    #[test]
    fn cast_model_and_anim_roundtrip() {
        let dir = std::env::temp_dir().join("animora_cast_test");
        let _ = std::fs::create_dir_all(&dir);
        let ctx = egui::Context::default();
        let mut a = App::new(&ctx, None);
        let sh = a.scene.nodes.iter().find(|n| n.name == "shoulder").unwrap().id;
        let el = a.scene.nodes.iter().find(|n| n.name == "elbow").unwrap().id;
        let wr = a.scene.nodes.iter().find(|n| n.name == "wrist").unwrap().id;
        let arm = a.scene.nodes.iter().find(|n| n.name == "arm").unwrap().id;
        a.scene.end = 30.0;
        let model = dir.join("arm_model.cast");
        let anim = dir.join("arm_wave.cast");
        a.sel = vec![sh, el, wr, arm];
        a.cast_export_model(&model).unwrap();
        a.sel = vec![sh, el, wr];
        a.cast_export_anim(&anim).unwrap();
        let expect = wrist_path(&a, wr, 30);
        let mesh_c = |s: &Scene, id: Id| {
            let w = s.world_matrices();
            let n = s.get(id).unwrap();
            let v = s.deformed_verts(n, n.mesh.as_ref().unwrap(), &w);
            v.iter().fold(V3::ZERO, |acc, p| acc + *p) / v.len() as f32
        };
        let mut s = a.scene.clone();
        let expect_mesh: Vec<V3> = (1..=30)
            .map(|f| {
                s.evaluate(f as f32);
                mesh_c(&s, arm)
            })
            .collect();

        // separate model + animation file
        let mut b = App::new(&ctx, None);
        b.scene = Scene::default();
        b.sel.clear();
        let msg = b.import_files(vec![anim.clone(), model.clone()]);
        assert!(msg.contains('2'), "{msg}");
        assert_eq!(b.scene.clips.len(), 1);
        assert_eq!(b.scene.clips[0].name, "arm_wave");
        let wr2 = b.scene.nodes.iter().find(|n| n.name == "wrist").unwrap().id;
        let m2 = b.scene.nodes.iter().find(|n| n.kind == Kind::Mesh).unwrap().id;
        assert!(b.scene.get(m2).unwrap().skin.is_some());
        assert!(b.scene.get(m2).unwrap().mesh.as_ref().unwrap().has_uvs());
        for i in 0..30 {
            b.scene.evaluate(1.0 + i as f32);
            let got = b.scene.world_matrices()[&wr2].translation();
            assert!((got - expect[i]).len() < 2e-3, "frame {} got {:?} want {:?}", i + 1, got, expect[i]);
            let gm = mesh_c(&b.scene, m2);
            assert!((gm - expect_mesh[i]).len() < 5e-3, "mesh frame {}", i + 1);
        }

        // one file holding both model and animation
        let mut roots = parse_cast(&std::fs::read(&model).unwrap()).unwrap();
        let anim_roots = parse_cast(&std::fs::read(&anim).unwrap()).unwrap();
        for n in anim_roots[0].kids(ID_ANIMATION) {
            roots[0].children.push(n.clone());
        }
        let both = dir.join("arm_both.cast");
        std::fs::write(&both, write_cast(&roots)).unwrap();
        assert_eq!(cast_kind(&both), (true, true));
        let mut c = App::new(&ctx, None);
        c.scene = Scene::default();
        c.sel.clear();
        c.import_files(vec![both]);
        assert_eq!(c.cod_model_groups().len(), 1);
        assert_eq!(c.scene.clips.len(), 1);
        let wr3 = c.scene.nodes.iter().find(|n| n.name == "wrist").unwrap().id;
        c.scene.evaluate(15.0);
        assert!((c.scene.world_matrices()[&wr3].translation() - expect[14]).len() < 2e-3);
    }

    /// Viewmodel workflow with Cast: arms + weapon models, one animation
    /// moves the joints of both models and nothing else in the scene.
    #[test]
    fn cast_anim_drives_all_models() {
        let dir = std::env::temp_dir().join("animora_cast_multi");
        let _ = std::fs::create_dir_all(&dir);
        let ctx = egui::Context::default();
        let mut a = App::new(&ctx, None);
        let sh = a.scene.nodes.iter().find(|n| n.name == "shoulder").unwrap().id;
        let el = a.scene.nodes.iter().find(|n| n.name == "elbow").unwrap().id;
        let wr = a.scene.nodes.iter().find(|n| n.name == "wrist").unwrap().id;
        let arm = a.scene.nodes.iter().find(|n| n.name == "arm").unwrap().id;
        a.scene.end = 30.0;
        let arms_file = dir.join("arms.cast");
        a.sel = vec![sh, el, wr, arm];
        a.cast_export_model(&arms_file).unwrap();
        // weapon: a gun joint in the hand with a box on it
        let mut gun = Node::new(0, "j_gun", Kind::Joint);
        gun.parent = Some(wr);
        gun.t = v3(0.3, 0.2, 0.0);
        let gun = a.scene.add(gun);
        let mut body = Node::new(0, "weapon", Kind::Mesh);
        body.parent = Some(gun);
        body.mesh = Some(Mesh::cube(0.3));
        let body = a.scene.add(body);
        // bind the box to the gun joint so the Cast model carries weights
        a.scene.bind_skin(body, &[gun]);
        let weapon_file = dir.join("weapon.cast");
        a.sel = vec![gun, body];
        a.cast_export_model(&weapon_file).unwrap();
        // one animation with all joints plus one the models do not have
        let mut extra = Node::new(0, "j_extra", Kind::Joint);
        extra.parent = Some(gun);
        extra.t = v3(0.0, 0.5, 0.0);
        let extra = a.scene.add(extra);
        let anim_file = dir.join("reload.cast");
        a.sel = vec![sh, el, wr, gun, extra];
        a.cast_export_anim(&anim_file).unwrap();
        let centroid = |s: &Scene, id: Id| {
            let w = s.world_matrices();
            let n = s.get(id).unwrap();
            let v = s.deformed_verts(n, n.mesh.as_ref().unwrap(), &w);
            v.iter().fold(V3::ZERO, |acc, p| acc + *p) / v.len() as f32
        };
        let mut s = a.scene.clone();
        let expect: Vec<(V3, V3)> = (1..=30)
            .map(|f| {
                s.evaluate(f as f32);
                (centroid(&s, arm), centroid(&s, body))
            })
            .collect();

        // import into the example scene: its own shoulder/elbow/wrist share
        // the names but are not an imported model and must stay as they are
        let mut b = App::new(&ctx, None);
        let demo_wrist = b.scene.nodes.iter().find(|n| n.name == "wrist").unwrap().id;
        let demo_keys_before = b.scene.get(demo_wrist).unwrap().is_animated();
        let msg = b.import_files(vec![anim_file.clone(), weapon_file.clone(), arms_file.clone()]);
        assert!(msg.contains('3'), "{msg}");
        assert_eq!(b.cod_targets().len(), 2);
        assert_eq!(b.scene.get(demo_wrist).unwrap().is_animated(), demo_keys_before, "example scene joint untouched");
        let groups = b.cod_model_groups();
        let in_model = |b: &App, g: Id, kind: Kind| b.scene.subtree(g).into_iter().find(|id| b.scene.get(*id).is_some_and(|n| n.kind == kind)).unwrap();
        let (arms_g, weapon_g) = if b.scene.get(groups[0]).unwrap().name == "arms" { (groups[0], groups[1]) } else { (groups[1], groups[0]) };
        let arms_mesh = in_model(&b, arms_g, Kind::Mesh);
        let weapon_mesh = in_model(&b, weapon_g, Kind::Mesh);
        // the anim-only joint was added inside the first model
        let ex = b.scene.nodes.iter().find(|n| n.name == "j_extra").unwrap();
        assert!(groups.iter().any(|g| b.scene.is_ancestor(*g, ex.id)));
        for (i, (ea, eb)) in expect.iter().enumerate() {
            b.scene.evaluate(1.0 + i as f32);
            let ga = centroid(&b.scene, arms_mesh);
            let gb = centroid(&b.scene, weapon_mesh);
            assert!((ga - *ea).len() < 5e-3, "arms frame {} got {:?} want {:?}", i + 1, ga, ea);
            assert!((gb - *eb).len() < 5e-3, "weapon frame {} got {:?} want {:?}", i + 1, gb, eb);
        }
        // an unticked model is left out of the next attachment
        b.cod.excluded.insert(weapon_g);
        let msg = b.cast_import_anim(&anim_file).unwrap();
        assert!(msg.contains("to 1 model(s)"), "{msg}");
        assert!(msg.contains("arms (") && !msg.contains("weapon ("), "{msg}");
        // the weapon was attached to the arms' wrist from the animation's hierarchy
        let wpar = b.scene.get(weapon_g).unwrap().parent.and_then(|p| b.scene.get(p)).map(|p| p.name.clone());
        assert_eq!(wpar.as_deref(), Some("wrist"));
    }

    /// Hand-made file: Z-up metadata, relative translation keys, a mode
    /// override and a notification track.
    #[test]
    fn cast_up_axis_modes_and_notes() {
        let dir = std::env::temp_dir().join("animora_cast_test");
        let _ = std::fs::create_dir_all(&dir);
        let mut root = CNode::new(ID_ROOT);
        let mut model = CNode::new(ID_MODEL);
        model.set_str("n", "rig");
        let mut skel = CNode::new(ID_SKELETON);
        for (name, parent, lp) in [("tag_origin", -1i32, [0.0, 0.0, 0.0]), ("j_hand", 0, [0.0, 0.0, 10.0])] {
            let mut b = CNode::new(ID_BONE);
            b.set_str("n", name);
            b.set_ints("p", *b"i\0", vec![parent as u32 as u64]);
            b.set_floats("lp", *b"3v", lp.to_vec());
            b.set_floats("lr", *b"4v", vec![0.0, 0.0, 0.0, 1.0]);
            skel.children.push(b);
        }
        model.children.push(skel);
        root.children.push(model);
        let mut anim = CNode::new(ID_ANIMATION);
        anim.set_floats("fr", *b"f\0", vec![60.0]);
        let mut c = CNode::new(ID_CURVE);
        c.set_str("nn", "j_hand").set_str("kp", "tx").set_str("m", "relative");
        c.set_ints("kb", *b"b\0", vec![0, 10]);
        c.set_floats("kv", *b"f\0", vec![0.0, 5.0]);
        anim.children.push(c);
        // absolute curve turned relative by an override on the parent
        let mut c = CNode::new(ID_CURVE);
        c.set_str("nn", "j_hand").set_str("kp", "tz").set_str("m", "absolute");
        c.set_ints("kb", *b"b\0", vec![0, 10]);
        c.set_floats("kv", *b"f\0", vec![0.0, 2.0]);
        anim.children.push(c);
        let mut o = CNode::new(ID_CURVE_OVERRIDE);
        o.set_str("nn", "tag_origin").set_str("m", "relative");
        o.set_ints("ot", *b"b\0", vec![1]);
        anim.children.push(o);
        let mut nt = CNode::new(ID_NOTIFICATION);
        nt.set_str("n", "fire");
        nt.set_ints("kb", *b"b\0", vec![4]);
        anim.children.push(nt);
        root.children.push(anim);
        let mut meta = CNode::new(ID_METADATA);
        meta.set_str("up", "z");
        root.children.push(meta);
        let file = dir.join("hand.cast");
        std::fs::write(&file, write_cast(&[root])).unwrap();

        let ctx = egui::Context::default();
        let mut b = App::new(&ctx, None);
        b.scene = Scene::default();
        b.sel.clear();
        b.import_files(vec![file]);
        let group = b.cod_model_groups()[0];
        assert!((b.scene.get(group).unwrap().r.x + 90.0).abs() < 1e-3, "Z-up stands upright");
        assert_eq!(b.scene.fps, 60.0);
        assert_eq!(b.scene.end, b.scene.start + 10.0);
        assert!(b.scene.notes.iter().any(|n| n.name == "fire" && n.frame == b.scene.start + 4.0));
        let hand = b.scene.nodes.iter().find(|n| n.name == "j_hand").unwrap().id;
        b.scene.evaluate(b.scene.start + 10.0);
        let t = b.scene.get(hand).unwrap().t;
        assert!((t.x - 5.0).abs() < 1e-4, "relative tx: rest 0 + 5, got {t:?}");
        assert!((t.z - 12.0).abs() < 1e-4, "override makes tz relative: rest 10 + 2, got {t:?}");
    }
}
