//! Keeps joints with the same name in different models together, like the
//! Cast plugin's "merge model": a model joint named like a joint of another
//! (earlier or animated) model is placed exactly where that joint is, on
//! every frame. A weapon whose root is called like the hand joint then sits
//! in the hand, its own joints and skinned mesh follow.

use crate::anim::*;
use crate::app::*;
use crate::math::*;
use crate::scene::*;
use std::collections::HashMap;

fn base_name(n: &str) -> String {
    n.rsplit(':').next().unwrap_or(n).to_lowercase()
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

impl App {
    /// Pairs (follower, leader) of joints that share a name across models.
    pub fn shared_joint_pairs(&self, targets: &[Id]) -> Vec<(Id, Id)> {
        let mut by_name: Vec<(String, Vec<Id>)> = Vec::new();
        for t in targets {
            for id in self.model_nodes(*t) {
                let Some(n) = self.scene.get(id) else { continue };
                if n.kind != Kind::Joint {
                    continue;
                }
                let key = base_name(&n.name);
                match by_name.iter_mut().find(|(k, _)| *k == key) {
                    Some((_, v)) => v.push(id),
                    None => by_name.push((key, vec![id])),
                }
            }
        }
        let mut pairs = Vec::new();
        for (_, ids) in by_name {
            if ids.len() < 2 {
                continue;
            }
            // leader: the first animated one (models in import order), else the first
            let leader = ids.iter().copied().find(|i| self.scene.get(*i).is_some_and(|n| n.is_animated())).unwrap_or(ids[0]);
            for f in ids {
                if f != leader && !self.scene.is_ancestor(f, leader) && !self.scene.is_ancestor(leader, f) {
                    pairs.push((f, leader));
                }
            }
        }
        // parents first, so a follower's parent is placed before the follower
        let depth = |s: &Scene, id: Id| {
            let mut d = 0;
            let mut cur = s.get(id).and_then(|n| n.parent);
            while let Some(c) = cur {
                d += 1;
                cur = s.get(c).and_then(|n| n.parent);
            }
            d
        };
        pairs.sort_by_key(|(f, _)| depth(&self.scene, *f));
        pairs
    }

    /// Keys every follower joint onto its leader for the whole frame range.
    /// Returns how many joints were aligned.
    pub fn align_shared_joints(&mut self, targets: &[Id]) -> usize {
        let pairs = self.shared_joint_pairs(targets);
        if pairs.is_empty() {
            return 0;
        }
        for (f, _) in &pairs {
            if let Some(n) = self.scene.get_mut(*f) {
                for c in 0..9 {
                    n.curves[c] = Curve::default();
                }
            }
        }
        let start = self.scene.start.round();
        let end = self.scene.end.round().max(start);
        let mut s = self.scene.clone();
        let mut keys: HashMap<Id, Vec<Vec<Key>>> = HashMap::new();
        let mut prev_r: HashMap<Id, V3> = HashMap::new();
        let mut fr = start;
        while fr <= end {
            s.evaluate(fr);
            // local matrices this frame; followers get overridden as we go
            let mut local: HashMap<Id, M4> = s.nodes.iter().map(|n| (n.id, n.local())).collect();
            let world_of = |local: &HashMap<Id, M4>, id: Id| -> M4 {
                let mut chain = Vec::new();
                let mut cur = Some(id);
                while let Some(c) = cur {
                    chain.push(c);
                    cur = s.get(c).and_then(|n| n.parent);
                    if chain.len() > 256 {
                        break;
                    }
                }
                chain.iter().rev().fold(M4::IDENTITY, |m, c| m * local.get(c).copied().unwrap_or(M4::IDENTITY))
            };
            for (f, leader) in &pairs {
                let target = world_of(&local, *leader);
                let pw = s.get(*f).and_then(|n| n.parent).map(|p| world_of(&local, p)).unwrap_or(M4::IDENTITY);
                let l = pw.inverse() * target;
                local.insert(*f, l);
                let (t, mut r, sc) = l.decompose();
                if let Some(pr) = prev_r.get(f) {
                    r = v3(unwrap(pr.x, r.x), unwrap(pr.y, r.y), unwrap(pr.z, r.z));
                }
                prev_r.insert(*f, r);
                let ch = keys.entry(*f).or_insert_with(|| vec![Vec::new(); 9]);
                for a in 0..3 {
                    ch[a].push(Key { frame: fr, value: t.get(a), tangent: Tangent::Linear });
                    ch[3 + a].push(Key { frame: fr, value: r.get(a), tangent: Tangent::Linear });
                    ch[6 + a].push(Key { frame: fr, value: sc.get(a), tangent: Tangent::Linear });
                }
            }
            fr += 1.0;
        }
        for (id, chans) in keys {
            if let Some(n) = self.scene.get_mut(id) {
                let scaled = chans[6..9].iter().flatten().any(|k| (k.value - 1.0).abs() > 1e-3);
                for (c, ks) in chans.into_iter().enumerate() {
                    if c >= 6 && !scaled {
                        continue;
                    }
                    n.curves[c].keys = ks;
                    n.curves[c].sort();
                }
            }
        }
        pairs.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cast::*;
    use crate::mesh::Mesh;

    #[test]
    fn weapon_sits_on_same_named_hand_joint() {
        let dir = std::env::temp_dir().join("animora_rig_test");
        let _ = std::fs::create_dir_all(&dir);
        let ctx = egui::Context::default();
        // the "game rig": arms with a gun in the hand
        let mut a = App::new(&ctx, None);
        let sh = a.scene.nodes.iter().find(|n| n.name == "shoulder").unwrap().id;
        let el = a.scene.nodes.iter().find(|n| n.name == "elbow").unwrap().id;
        let wr = a.scene.nodes.iter().find(|n| n.name == "wrist").unwrap().id;
        let arm = a.scene.nodes.iter().find(|n| n.name == "arm").unwrap().id;
        a.scene.end = 30.0;
        let mut gun = Node::new(0, "j_gun", Kind::Joint);
        gun.parent = Some(wr);
        gun.t = v3(0.3, 0.2, 0.0);
        gun.r = v3(0.0, 0.0, 30.0);
        let gun = a.scene.add(gun);
        let mut body = Node::new(0, "weapon", Kind::Mesh);
        body.parent = Some(gun);
        body.mesh = Some(Mesh::cube(0.3));
        let body = a.scene.add(body);
        a.scene.bind_skin(body, &[gun]);
        let arms_file = dir.join("arms.cast");
        a.sel = vec![sh, el, wr, arm];
        a.cast_export_model(&arms_file).unwrap();
        let anim_file = dir.join("fire.cast");
        a.sel = vec![sh, el, wr, gun];
        a.cast_export_anim(&anim_file).unwrap();
        // like many third-party files: no hierarchy inside the animation
        let mut roots = parse_cast(&std::fs::read(&anim_file).unwrap()).unwrap();
        for r in &mut roots {
            for an in r.children.iter_mut().filter(|c| c.id == ID_ANIMATION) {
                an.children.retain(|c| c.id != ID_SKELETON);
            }
        }
        std::fs::write(&anim_file, write_cast(&roots)).unwrap();
        let centroid = |s: &Scene, id: Id| {
            let w = s.world_matrices();
            let n = s.get(id).unwrap();
            let v = s.deformed_verts(n, n.mesh.as_ref().unwrap(), &w);
            v.iter().fold(V3::ZERO, |acc, p| acc + *p) / v.len() as f32
        };
        let mut s = a.scene.clone();
        let expect: Vec<V3> = (1..=30)
            .map(|f| {
                s.evaluate(f as f32);
                centroid(&s, body)
            })
            .collect();

        // the weapon as its own model: root joint named like the hand, far away
        let mut c = App::new(&ctx, None);
        c.scene = Scene::default();
        let mut cw = Node::new(0, "wrist", Kind::Joint);
        cw.t = v3(5.0, 0.0, 0.0);
        let cw = c.scene.add(cw);
        let mut cg = Node::new(0, "j_gun", Kind::Joint);
        cg.parent = Some(cw);
        cg.t = v3(0.3, 0.2, 0.0);
        cg.r = v3(0.0, 0.0, 30.0);
        let cg = c.scene.add(cg);
        let mut cb = Node::new(0, "weapon", Kind::Mesh);
        cb.parent = Some(cg);
        cb.mesh = Some(Mesh::cube(0.3));
        let cb = c.scene.add(cb);
        c.scene.bind_skin(cb, &[cg]);
        let weapon_file = dir.join("weapon.cast");
        c.sel = vec![cw, cg, cb];
        c.cast_export_model(&weapon_file).unwrap();

        let mut b = App::new(&ctx, None);
        b.scene = Scene::default();
        b.sel.clear();
        b.import_files(vec![arms_file, weapon_file, anim_file]);
        let groups = b.cod_model_groups();
        assert_eq!(groups.len(), 2);
        let weapon_group = groups.iter().copied().find(|g| b.scene.get(*g).unwrap().name == "weapon").unwrap();
        let weapon_mesh = b.model_nodes(weapon_group).into_iter().find(|id| b.scene.get(*id).is_some_and(|n| n.kind == Kind::Mesh)).unwrap();
        for (i, e) in expect.iter().enumerate() {
            b.scene.evaluate(1.0 + i as f32);
            let got = centroid(&b.scene, weapon_mesh);
            assert!((got - *e).len() < 5e-3, "frame {} weapon at {:?}, the hand wants {:?}", i + 1, got, e);
        }
    }
}
