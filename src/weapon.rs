//! "Attach Weapon to Rig" and automatic joint renaming, as in the CoD Maya
//! tools: the gun's base tag is parented under the viewhands' weapon tag
//! with zero offset, so the weapon sits exactly in the hand.

use crate::app::*;
use crate::math::*;
use crate::scene::*;

/// (gun base tag, viewhand tag) – tried in this order, later pairs win.
/// Namespaces are ignored (the Maya tool also lists "t7:" variants).
pub const ATTACH_PAIRS: [(&str, &str); 6] = [
    ("j_gun", "tag_weapon_right"),
    ("j_gun1", "tag_weapon_left"),
    ("j_gun", "tag_weapon"),
    ("j_gun1", "tag_weapon1"),
    ("tag_weapon", "tag_weapon_right"),
    ("tag_weapon1", "tag_weapon_left"),
];

/// Export renames: (joint, required parent or None) -> new name.
pub const RENAME_RULES: [(&str, Option<&str>, &str); 6] = [
    ("tag_weapon", Some("tag_torso"), "tag_weapon_right"),
    ("tag_weapon1", Some("tag_torso"), "tag_weapon_left"),
    ("j_gun", None, "tag_weapon"),
    ("j_gun1", None, "tag_weapon_le"),
    ("tag_flash1", Some("j_gun1"), "tag_flash_le"),
    ("tag_brass1", None, "tag_brass_le"),
];

fn base(n: &str) -> String {
    n.rsplit(':').next().unwrap_or(n).to_lowercase()
}

/// New export name for a joint (CoD tools' "Automatically rename joints").
/// `parent` may be the original or the already renamed parent name.
pub fn auto_rename(name: &str, parent: Option<&str>, parent_renamed: Option<&str>) -> String {
    let n = base(name);
    for (joint, want_parent, new_name) in RENAME_RULES {
        if n != joint {
            continue;
        }
        match want_parent {
            None => return new_name.to_string(),
            Some(p) => {
                if parent.is_some_and(|x| base(x) == p) || parent_renamed.is_some_and(|x| base(x) == p) {
                    return new_name.to_string();
                }
            }
        }
    }
    name.rsplit(':').next().unwrap_or(name).to_string()
}

impl App {
    fn joints_named(&self, name: &str) -> Vec<Id> {
        self.scene.nodes.iter().filter(|n| n.kind == Kind::Joint && base(&n.name) == name).map(|n| n.id).collect()
    }

    /// The model group a node belongs to (nearest model group above it).
    fn owner_model(&self, id: Id) -> Option<Id> {
        let groups = self.cod_model_groups();
        let mut cur = self.scene.get(id).and_then(|n| n.parent);
        while let Some(c) = cur {
            if groups.contains(&c) {
                return Some(c);
            }
            cur = self.scene.get(c).and_then(|n| n.parent);
        }
        None
    }

    /// Attaches the weapon to the viewhands. Returns a short report.
    pub fn attach_weapon_to_rig(&mut self) -> Result<String, String> {
        let mut done: Vec<String> = Vec::new();
        let mut checkpointed = false;
        for (gun_tag, hand_tag) in ATTACH_PAIRS {
            let guns = self.joints_named(gun_tag);
            let hands = self.joints_named(hand_tag);
            // a gun tag and a hand tag of different models, no loops
            let mut pick: Option<(Id, Id)> = None;
            'search: for &h in &hands {
                for &g in &guns {
                    if g == h || self.scene.is_ancestor(g, h) {
                        continue;
                    }
                    let (og, oh) = (self.owner_model(g), self.owner_model(h));
                    if og.is_some() && og == oh {
                        continue;
                    }
                    pick = Some((g, h));
                    break 'search;
                }
            }
            let Some((g, h)) = pick else { continue };
            if !checkpointed {
                self.checkpoint();
                checkpointed = true;
            }
            // the gun's base tag is the root of its own model: hang the whole
            // model on the hand tag so it stays a model of its own
            let g_parent = self.scene.get(g).and_then(|n| n.parent);
            let root_of_model = g_parent.is_some_and(|p| self.cod_model_groups().contains(&p) && self.owner_model(g) == Some(p));
            if root_of_model {
                let group = g_parent.unwrap();
                self.attach_model(group, Some(h));
            } else if let Some(n) = self.scene.get_mut(g) {
                n.parent = Some(h);
            }
            // reset the positions of both tags (gun sits exactly on the hand tag)
            if let Some(n) = self.scene.get_mut(g) {
                n.t = V3::ZERO;
                n.r = V3::ZERO;
                n.rest = Some([V3::ZERO, V3::ZERO, n.s]);
                for c in 0..6 {
                    n.curves[c] = Default::default();
                }
            }
            if let Some(n) = self.scene.get_mut(h) {
                n.r = V3::ZERO;
                let t = n.t;
                let s = n.s;
                n.rest = Some([n.rest.map(|r| r[0]).unwrap_or(t), V3::ZERO, n.rest.map(|r| r[2]).unwrap_or(s)]);
                for c in 3..6 {
                    n.curves[c] = Default::default();
                }
            }
            let gn = self.scene.get(g).map(|n| n.name.clone()).unwrap_or_default();
            let hn = self.scene.get(h).map(|n| n.name.clone()).unwrap_or_default();
            done.retain(|d| !d.starts_with(&format!("{gn} →")));
            done.push(format!("{gn} → {hn}"));
        }
        if done.is_empty() {
            return Err(crate::tr!(
                "Keine passenden Tags gefunden: die Waffe braucht j_gun / tag_weapon, die Arme tag_weapon_right / tag_weapon_left / tag_weapon.",
                "No matching tags found: the weapon needs j_gun / tag_weapon, the arms tag_weapon_right / tag_weapon_left / tag_weapon."
            )
            .into());
        }
        let f = self.scene.frame;
        self.scene.evaluate(f);
        Ok(crate::trf!("Waffe ans Rig gehängt: {}", "Weapon attached to rig: {}", done.join(", ")))
    }

    pub fn attach_weapon_dialog(&mut self) {
        let r = self.attach_weapon_to_rig();
        let msg = match r {
            Ok(m) => m,
            Err(e) => e,
        };
        self.status = msg.clone();
        self.cod.message = msg;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rename_rules() {
        assert_eq!(auto_rename("J_Gun", None, None), "tag_weapon");
        assert_eq!(auto_rename("tag_weapon", Some("tag_torso"), None), "tag_weapon_right");
        assert_eq!(auto_rename("tag_weapon", Some("j_wrist_ri"), None), "tag_weapon");
        assert_eq!(auto_rename("tag_flash1", Some("j_gun1"), Some("tag_weapon_le")), "tag_flash_le");
        assert_eq!(auto_rename("vm:j_elbow_le", None, None), "j_elbow_le");
    }

    #[test]
    fn weapon_binder_puts_gun_on_hand_tag() {
        let ctx = egui::Context::default();
        let mut a = App::new(&ctx, None);
        a.scene = Scene::default();
        // viewhands model: tag_torso -> tag_weapon_right (rotated, offset)
        let arms = a.scene.add(Node::new(0, "viewhands", Kind::Group));
        let mut torso = Node::new(0, "tag_torso", Kind::Joint);
        torso.parent = Some(arms);
        torso.t = v3(0.0, 1.0, 0.0);
        let torso = a.scene.add(torso);
        let mut hand = Node::new(0, "tag_weapon_right", Kind::Joint);
        hand.parent = Some(torso);
        hand.t = v3(2.0, 0.5, 1.0);
        hand.r = v3(10.0, 20.0, 30.0);
        let hand = a.scene.add(hand);
        // weapon model elsewhere: j_gun root with a child
        let gun_m = a.scene.add(Node::new(0, "weapon", Kind::Group));
        let mut gun = Node::new(0, "J_Gun", Kind::Joint);
        gun.parent = Some(gun_m);
        gun.t = v3(-5.0, 0.0, 3.0);
        let gun = a.scene.add(gun);
        let mut mag = Node::new(0, "j_mag1", Kind::Joint);
        mag.parent = Some(gun);
        mag.t = v3(0.0, -0.2, 0.1);
        let mag = a.scene.add(mag);
        let msg = a.attach_weapon_to_rig().unwrap();
        assert!(msg.contains("J_Gun → tag_weapon_right"), "{msg}");
        let w = a.scene.world_matrices();
        // gun tag sits exactly on the hand tag, the weapon follows
        assert!((w[&gun].translation() - w[&hand].translation()).len() < 1e-5);
        assert!((w[&mag].translation() - (w[&hand].translation() + v3(0.0, -0.2, 0.1))).len() < 1e-4);
        // the hand tag's rotation was reset, the weapon is still its own model
        assert_eq!(a.scene.get(hand).unwrap().r, V3::ZERO);
        assert_eq!(a.scene.get(gun_m).unwrap().parent, Some(hand));
        assert!(a.cod_model_groups().contains(&gun_m));
        // nothing to attach in an empty scene
        let mut b = App::new(&ctx, None);
        b.scene = Scene::default();
        assert!(b.attach_weapon_to_rig().is_err());
    }
}
