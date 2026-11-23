//! Projection, shading and a software renderer (z-buffer) for final images.

use crate::math::*;
use crate::scene::*;

/// Camera projection for one view.
#[derive(Clone, Copy)]
pub struct Proj {
    pub cam_world: M4,
    pub view: M4,
    pub ortho: bool,
    /// perspective: focal factor 1/tan(fov/2); ortho: half height in world units
    pub f: f32,
    pub w: f32,
    pub h: f32,
    pub ox: f32,
    pub oy: f32,
    pub near: f32,
}

impl Proj {
    pub fn new(cam_world: M4, ortho: bool, fov_or_half: f32, x: f32, y: f32, w: f32, h: f32) -> Proj {
        let f = if ortho { fov_or_half } else { 1.0 / (fov_or_half.to_radians() * 0.5).tan() };
        Proj { cam_world, view: cam_world.inverse(), ortho, f, w, h, ox: x, oy: y, near: 0.05 }
    }

    pub fn eye(&self) -> V3 {
        self.cam_world.translation()
    }
    pub fn forward(&self) -> V3 {
        self.cam_world.vector(-V3::Z).norm()
    }

    /// View-space -> screen (x, y, depth). Depth grows away from the camera.
    pub fn view_to_screen(&self, p: V3) -> (f32, f32, f32) {
        let half = self.h * 0.5;
        if self.ortho {
            let s = half / self.f;
            (self.ox + self.w * 0.5 + p.x * s, self.oy + half - p.y * s, -p.z)
        } else {
            let d = -p.z;
            let s = self.f * half / d.max(1e-6);
            (self.ox + self.w * 0.5 + p.x * s, self.oy + half - p.y * s, d)
        }
    }

    /// World -> screen, None if behind the camera.
    pub fn project(&self, p: V3) -> Option<(f32, f32, f32)> {
        let v = self.view.point(p);
        if !self.ortho && -v.z < self.near {
            return None;
        }
        Some(self.view_to_screen(v))
    }

    /// World units per pixel at point p (for constant-size gizmos).
    pub fn world_per_pixel(&self, p: V3) -> f32 {
        let half = self.h * 0.5;
        if self.ortho {
            self.f / half
        } else {
            let d = (-self.view.point(p).z).max(self.near);
            d / (self.f * half)
        }
    }

    /// A world-space ray through a screen point.
    pub fn ray(&self, sx: f32, sy: f32) -> (V3, V3) {
        let half = self.h * 0.5;
        let nx = (sx - self.ox - self.w * 0.5) / half;
        let ny = -(sy - self.oy - half) / half;
        if self.ortho {
            let o = self.cam_world.point(v3(nx * self.f, ny * self.f, 0.0));
            (o, self.forward())
        } else {
            let d = self.cam_world.vector(v3(nx / self.f, ny / self.f, -1.0)).norm();
            (self.eye(), d)
        }
    }
}

/// Clips a view-space polygon against the near plane.
/// Polygon corner: (view-space position, world position, normal, uv as x/y).
pub type ClipVert = (V3, V3, V3, V3);

pub fn clip_near(poly: &[ClipVert], near: f32) -> Vec<ClipVert> {
    let inside = |p: &V3| -p.z >= near;
    let mut out = Vec::new();
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[(i + 1) % poly.len()];
        let ia = inside(&a.0);
        let ib = inside(&b.0);
        if ia {
            out.push(a);
        }
        if ia != ib {
            let t = (-near - a.0.z) / (b.0.z - a.0.z);
            out.push((a.0.lerp(b.0, t), a.1.lerp(b.1, t), a.2.lerp(b.2, t), a.3.lerp(b.3, t)));
        }
    }
    out
}

/// Blinn-Phong shading. Without lights a soft headlight is used, like a
/// typical 3D package's default lighting.
pub fn shade(p: V3, n: V3, eye: V3, color: [f32; 3], spec: f32, shin: f32, lights: &[LightInfo], default_light: bool) -> V3 {
    let base = v3(color[0], color[1], color[2]);
    let vdir = (eye - p).norm();
    let mut n = n;
    if n.dot(vdir) < 0.0 {
        n = -n; // two-sided
    }
    let mut c = V3::ZERO;
    let mut amb = V3::ZERO;
    let mut add = |ldir: V3, lc: V3| {
        let d = n.dot(ldir).max(0.0);
        c += base.mul_e(lc) * d;
        if spec > 0.0 && d > 0.0 {
            let hv = (ldir + vdir).norm();
            let s = n.dot(hv).max(0.0).powf(shin.max(1.0)) * spec;
            c += lc * s;
        }
    };
    if default_light || lights.is_empty() {
        add((vdir + v3(0.2, 0.35, 0.0)).norm(), v3(0.9, 0.9, 0.9));
        amb = base * 0.18;
    } else {
        for l in lights {
            match l.kind {
                LightKind::Directional => add(-l.dir, l.color),
                LightKind::Point => {
                    let d = l.pos - p;
                    let dist = d.len().max(0.1);
                    let att = 1.0 / (1.0 + 0.02 * dist * dist);
                    add(d / dist, l.color * att * 2.0)
                }
                LightKind::Ambient => amb += base.mul_e(l.color),
            }
        }
    }
    c + amb
}

pub fn to_rgb8(c: V3) -> [u8; 3] {
    // simple tone curve + gamma-ish lift
    let f = |x: f32| ((x / (1.0 + 0.15 * x)).clamp(0.0, 1.0).powf(1.0 / 1.15) * 255.0) as u8;
    [f(c.x), f(c.y), f(c.z)]
}

pub struct RenderSettings {
    pub width: u32,
    pub height: u32,
    pub antialias: bool,
}

/// Renders the scene with a z-buffer into an RGBA image.
pub type TexMap = std::collections::HashMap<u64, std::sync::Arc<image::RgbaImage>>;

/// Nearest-neighbour texture lookup with wrapping, returned as 0..1 colour.
pub fn sample_tex(img: &image::RgbaImage, u: f32, v: f32) -> V3 {
    let (w, h) = (img.width().max(1), img.height().max(1));
    let x = ((u.rem_euclid(1.0) * w as f32) as u32).min(w - 1);
    let y = ((v.rem_euclid(1.0) * h as f32) as u32).min(h - 1);
    let p = img.get_pixel(x, y).0;
    v3(p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0)
}

pub fn render_image(scene: &Scene, cam_world: M4, fov: f32, ortho: bool, set: &RenderSettings, textures: &TexMap) -> image::RgbaImage {
    let ss = if set.antialias { 2 } else { 1 };
    let w = (set.width * ss) as usize;
    let h = (set.height * ss) as usize;
    let proj = Proj::new(cam_world, ortho, fov, 0.0, 0.0, w as f32, h as f32);
    let worlds = scene.world_matrices();
    let tris = scene.triangles(&worlds);
    let lights = scene.lights(&worlds);
    let eye = proj.eye();
    let bg_top = v3(scene.background[0], scene.background[1], scene.background[2]);
    let bg_bottom = bg_top * 0.45;
    let mut color = vec![V3::ZERO; w * h];
    for y in 0..h {
        let t = y as f32 / h as f32;
        let c = bg_top.lerp(bg_bottom, t);
        for x in 0..w {
            color[y * w + x] = c;
        }
    }
    let mut depth = vec![f32::INFINITY; w * h];
    for tri in &tris {
        let poly: Vec<ClipVert> = (0..3).map(|i| (proj.view.point(tri.p[i]), tri.p[i], tri.n[i], v3(tri.uv[i][0], tri.uv[i][1], 0.0))).collect();
        let poly = if ortho { poly } else { clip_near(&poly, proj.near) };
        if poly.len() < 3 {
            continue;
        }
        let tex = if tri.tex != 0 { textures.get(&tri.tex) } else { None };
        let sp: Vec<(f32, f32, f32)> = poly.iter().map(|v| proj.view_to_screen(v.0)).collect();
        for k in 1..poly.len() - 1 {
            let idx = [0, k, k + 1];
            let s = idx.map(|i| sp[i]);
            let a = idx.map(|i| poly[i]);
            raster_tri(&s, w, h, |x, y, b| {
                // perspective-correct barycentrics
                let (b0, b1, b2) = if ortho {
                    (b[0], b[1], b[2])
                } else {
                    let i0 = b[0] / s[0].2;
                    let i1 = b[1] / s[1].2;
                    let i2 = b[2] / s[2].2;
                    let sum = i0 + i1 + i2;
                    (i0 / sum, i1 / sum, i2 / sum)
                };
                let z = s[0].2 * b0 + s[1].2 * b1 + s[2].2 * b2;
                let di = y * w + x;
                if z < depth[di] {
                    depth[di] = z;
                    let p = a[0].1 * b0 + a[1].1 * b1 + a[2].1 * b2;
                    let n = (a[0].2 * b0 + a[1].2 * b1 + a[2].2 * b2).norm();
                    let mut base = tri.color;
                    if let Some(img) = tex {
                        let uv = a[0].3 * b0 + a[1].3 * b1 + a[2].3 * b2;
                        let t = sample_tex(img, uv.x, uv.y);
                        base = [base[0] * t.x, base[1] * t.y, base[2] * t.z];
                    }
                    color[di] = shade(p, n, eye, base, tri.spec, tri.shin, &lights, false);
                }
            });
        }
    }
    let mut img = image::RgbaImage::new(set.width, set.height);
    for y in 0..set.height as usize {
        for x in 0..set.width as usize {
            let mut c = V3::ZERO;
            for dy in 0..ss as usize {
                for dx in 0..ss as usize {
                    c += color[(y * ss as usize + dy) * w + x * ss as usize + dx];
                }
            }
            c = c / (ss * ss) as f32;
            let rgb = to_rgb8(c);
            img.put_pixel(x as u32, y as u32, image::Rgba([rgb[0], rgb[1], rgb[2], 255]));
        }
    }
    img
}

fn raster_tri(s: &[(f32, f32, f32); 3], w: usize, h: usize, mut f: impl FnMut(usize, usize, [f32; 3])) {
    let minx = s.iter().map(|p| p.0).fold(f32::MAX, f32::min).floor().max(0.0) as i64;
    let maxx = s.iter().map(|p| p.0).fold(f32::MIN, f32::max).ceil().min(w as f32 - 1.0) as i64;
    let miny = s.iter().map(|p| p.1).fold(f32::MAX, f32::min).floor().max(0.0) as i64;
    let maxy = s.iter().map(|p| p.1).fold(f32::MIN, f32::max).ceil().min(h as f32 - 1.0) as i64;
    if minx > maxx || miny > maxy {
        return;
    }
    let area = (s[1].0 - s[0].0) * (s[2].1 - s[0].1) - (s[2].0 - s[0].0) * (s[1].1 - s[0].1);
    if area.abs() < 1e-9 {
        return;
    }
    for y in miny..=maxy {
        let py = y as f32 + 0.5;
        for x in minx..=maxx {
            let px = x as f32 + 0.5;
            let w0 = ((s[1].0 - px) * (s[2].1 - py) - (s[2].0 - px) * (s[1].1 - py)) / area;
            let w1 = ((s[2].0 - px) * (s[0].1 - py) - (s[0].0 - px) * (s[2].1 - py)) / area;
            let w2 = 1.0 - w0 - w1;
            if w0 >= -1e-4 && w1 >= -1e-4 && w2 >= -1e-4 {
                f(x as usize, y as usize, [w0, w1, w2]);
            }
        }
    }
}
