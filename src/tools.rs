//! General helpers: status reports, opening / dropping files, textures and
//! skeleton utilities (rest pose, namespaces, joint selection, curves).

use crate::anim::*;
use crate::app::*;
use crate::math::*;
use crate::scene::*;
use std::path::{Path, PathBuf};

/// Name without a "namespace:" prefix.
pub fn strip_ns(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

impl App {
    /// Shows the outcome of an action in the status line and the tool windows.
    pub fn report(&mut self, r: Result<String, String>, what: &str) {
        let msg = match r {
            Ok(m) => m,
            Err(e) => crate::trf!("{what} fehlgeschlagen: {e}", "{what} failed: {e}"),
        };
        self.status = msg.clone();
        self.cod.message = msg;
    }

    /// Returns the rest pose of a joint, saving the current pose the first time.
    fn obtain_rest(&mut self, id: Id, reset: bool) -> Option<[V3; 3]> {
        let n = self.scene.get_mut(id)?;
        let rest = *n.rest.get_or_insert([n.t, n.r, n.s]);
        if reset {
            n.t = rest[0];
            n.r = rest[1];
            n.s = rest[2];
        }
        Some(rest)
    }

    /// Puts every joint back to its rest pose and removes animation + notes.
    pub fn reset_scene_inner(&mut self) {
        let joints: Vec<Id> = self.scene.nodes.iter().filter(|n| n.kind == Kind::Joint).map(|n| n.id).collect();
        for j in joints {
            self.obtain_rest(j, true);
            if let Some(n) = self.scene.get_mut(j) {
                for c in 0..9 {
                    n.curves[c] = Curve::default();
                }
            }
        }
        self.scene.notes.clear();
    }

    pub fn reset_scene(&mut self) {
        self.checkpoint();
        self.reset_scene_inner();
        self.status = crate::tr!("Szene auf Ruhepose zurückgesetzt", "Scene reset to the rest pose").into();
    }

    pub fn clean_namespaces(&mut self) {
        self.checkpoint();
        let mut changed = 0;
        let ids: Vec<Id> = self.scene.nodes.iter().filter(|n| n.name.contains(':')).map(|n| n.id).collect();
        for id in ids {
            let Some(old) = self.scene.get(id).map(|n| n.name.clone()) else { continue };
            let base = strip_ns(&old).to_string();
            let new = if self.scene.nodes.iter().any(|n| n.name == base) { self.scene.unique_name(&base) } else { base };
            if let Some(n) = self.scene.get_mut(id) {
                n.name = new;
                changed += 1;
            }
        }
        self.status = crate::trf!("Namespaces entfernt: {changed} Namen bereinigt", "Namespaces removed: {changed} names cleaned");
    }

    pub fn select_joints(&mut self, keyed_only: bool) {
        self.sel = self.scene.nodes.iter().filter(|n| n.kind == Kind::Joint && (!keyed_only || n.is_animated())).map(|n| n.id).collect();
        self.comp = CompMode::Object;
        self.status = crate::trf!("{} Gelenke ausgewählt", "{} joints selected", self.sel.len());
    }

    pub fn clear_curves(&mut self) {
        self.checkpoint();
        for n in &mut self.scene.nodes {
            for c in 0..9 {
                n.curves[c] = Curve::default();
            }
        }
        self.status = crate::tr!("Alle Animationskurven gelöscht", "All animation curves deleted").into();
    }

    /// Files dropped onto the window are imported by their type
    /// (models first, every animation becomes a clip).
    pub fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let files: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty()).collect());
        if !files.is_empty() {
            let msg = self.import_files(files);
            self.report(Ok(msg), crate::tr!("Import", "Import"));
        }
    }

    /// Opens or imports any supported file by its extension.
    pub fn open_any(&mut self, p: &Path) {
        if !p.exists() {
            self.report(Err(crate::trf!("Datei nicht gefunden: {}", "File not found: {}", p.display())), crate::tr!("Öffnen", "Open"));
            return;
        }
        let msg = self.import_files(vec![p.to_path_buf()]);
        self.report(Ok(msg), crate::tr!("Import", "Import"));
    }

    /// Puts a texture on the selected meshes (creating box UVs if needed).
    pub fn assign_texture(&mut self, p: &Path) -> Result<String, String> {
        let meshes: Vec<Id> = self.sel.iter().copied().filter(|id| self.scene.get(*id).is_some_and(|n| n.kind == Kind::Mesh)).collect();
        if meshes.is_empty() {
            return Err(crate::tr!("Für eine Textur zuerst ein Objekt auswählen.", "Select an object first to apply a texture.").into());
        }
        self.checkpoint();
        let path = p.to_string_lossy().to_string();
        self.tex_failed.remove(&tex_key(&path));
        for id in &meshes {
            if let Some(n) = self.scene.get_mut(*id) {
                if let Some(m) = &mut n.mesh {
                    if !m.has_uvs() {
                        m.box_uvs();
                    }
                }
                n.material.texture = Some(path.clone());
                n.material.color = [1.0, 1.0, 1.0];
            }
        }
        Ok(crate::trf!("Textur auf {} Objekt(e) gelegt", "Texture applied to {} object(s)", meshes.len()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A texture put on the box shows up in the rendered picture.
    #[test]
    fn texture_shows_in_render() {
        let dir = std::env::temp_dir().join("animora_tools_test");
        let _ = std::fs::create_dir_all(&dir);
        let png = dir.join("rot.png");
        image::RgbaImage::from_pixel(4, 4, image::Rgba([255, 0, 0, 255])).save(&png).unwrap();
        let ctx = egui::Context::default();
        let mut a = App::new(&ctx, None);
        let kiste = a.scene.nodes.iter().find(|n| n.name == "box").unwrap().id;
        a.sel = vec![kiste];
        a.assign_texture(&png).unwrap();
        a.sync_textures(&ctx);
        assert_eq!(a.textures.len(), 1);
        let w = a.scene.world_matrices();
        let target = w[&kiste].translation();
        let cam = M4::look_at(target + v3(0.0, 1.0, 5.0), target, V3::Y);
        let set = crate::render::RenderSettings { width: 64, height: 64, antialias: false };
        let img = crate::render::render_image(&a.scene, cam, 40.0, false, &set, &a.tex_map());
        let c = img.get_pixel(32, 32).0;
        assert!(c[0] > 100 && c[1] < 60 && c[2] < 60, "centre pixel {c:?}");
    }

    #[test]
    fn reset_scene_restores_rest_pose() {
        let ctx = egui::Context::default();
        let mut a = App::new(&ctx, None);
        let el = a.scene.nodes.iter().find(|n| n.name == "elbow").unwrap().id;
        a.scene.get_mut(el).unwrap().rest = Some([v3(0.0, 1.6, 0.0), V3::ZERO, V3::ONE]);
        a.scene.get_mut(el).unwrap().r = v3(40.0, 0.0, 0.0);
        a.scene.get_mut(el).unwrap().curves[3].set_key(1.0, 40.0, Tangent::Linear);
        a.reset_scene();
        let n = a.scene.get(el).unwrap();
        assert_eq!(n.r, V3::ZERO);
        assert!(!n.is_animated());
    }
}
