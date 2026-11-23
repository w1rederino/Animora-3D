//! Polygon meshes: primitives, modeling operations and normals.

use crate::math::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::f32::consts::{PI, TAU};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Mesh {
    pub verts: Vec<V3>,
    /// Polygons as counter-clockwise vertex index lists (seen from outside).
    pub faces: Vec<Vec<u32>>,
    /// Texture coordinates per face corner (image space: v grows downwards).
    /// Either empty (no UVs) or one entry per face.
    #[serde(default)]
    pub uvs: Vec<Vec<[f32; 2]>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Prim {
    Cube,
    Sphere,
    Cylinder,
    Cone,
    Plane,
    Torus,
}

impl Prim {
    pub const ALL: [Prim; 6] = [Prim::Cube, Prim::Sphere, Prim::Cylinder, Prim::Cone, Prim::Plane, Prim::Torus];
    pub fn label(self) -> &'static str {
        match self {
            Prim::Cube => crate::tr!("Würfel", "Cube"),
            Prim::Sphere => crate::tr!("Kugel", "Sphere"),
            Prim::Cylinder => crate::tr!("Zylinder", "Cylinder"),
            Prim::Cone => crate::tr!("Kegel", "Cone"),
            Prim::Plane => crate::tr!("Ebene", "Plane"),
            Prim::Torus => crate::tr!("Torus", "Torus"),
        }
    }
}

/// Parameters that rebuild a primitive (attribute editor "Form" tab).
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct PrimParams {
    pub kind: Prim,
    pub size: f32,
    pub height: f32,
    pub radius2: f32,
    pub seg_u: u32,
    pub seg_v: u32,
}

impl PrimParams {
    pub fn new(kind: Prim) -> Self {
        let (size, height, radius2, seg_u, seg_v) = match kind {
            Prim::Cube => (1.0, 1.0, 0.0, 1, 1),
            Prim::Sphere => (1.0, 0.0, 0.0, 20, 14),
            Prim::Cylinder => (0.5, 2.0, 0.0, 20, 1),
            Prim::Cone => (0.6, 1.5, 0.0, 20, 1),
            Prim::Plane => (10.0, 0.0, 0.0, 4, 4),
            Prim::Torus => (1.0, 0.0, 0.3, 24, 12),
        };
        PrimParams { kind, size, height, radius2, seg_u, seg_v }
    }
    pub fn build(&self) -> Mesh {
        let u = self.seg_u.max(3);
        let v = self.seg_v.max(2);
        match self.kind {
            Prim::Cube => Mesh::cube(self.size),
            Prim::Sphere => Mesh::sphere(self.size, u, v),
            Prim::Cylinder => Mesh::cylinder(self.size, self.height, u, false),
            Prim::Cone => Mesh::cylinder(self.size, self.height, u, true),
            Prim::Plane => Mesh::plane(self.size, self.seg_u.max(1)),
            Prim::Torus => Mesh::torus(self.size, self.radius2, u, v),
        }
    }
}

impl Mesh {
    pub fn cube(s: f32) -> Mesh {
        let h = s * 0.5;
        let verts = vec![
            v3(-h, -h, -h),
            v3(h, -h, -h),
            v3(h, h, -h),
            v3(-h, h, -h),
            v3(-h, -h, h),
            v3(h, -h, h),
            v3(h, h, h),
            v3(-h, h, h),
        ];
        let faces = vec![
            vec![4, 5, 6, 7], // +z
            vec![1, 0, 3, 2], // -z
            vec![5, 1, 2, 6], // +x
            vec![0, 4, 7, 3], // -x
            vec![7, 6, 2, 3], // +y
            vec![0, 1, 5, 4], // -y
        ];
        Mesh { verts, faces, uvs: Vec::new() }
    }

    pub fn sphere(r: f32, su: u32, sv: u32) -> Mesh {
        let mut verts = vec![v3(0.0, r, 0.0)];
        for j in 1..sv {
            let phi = PI * j as f32 / sv as f32;
            for i in 0..su {
                let th = TAU * i as f32 / su as f32;
                verts.push(v3(r * phi.sin() * th.sin(), r * phi.cos(), r * phi.sin() * th.cos()));
            }
        }
        verts.push(v3(0.0, -r, 0.0));
        let bottom = verts.len() as u32 - 1;
        let ring = |j: u32, i: u32| 1 + (j - 1) * su + (i % su);
        let mut faces = Vec::new();
        for i in 0..su {
            faces.push(vec![0, ring(1, i), ring(1, i + 1)]);
        }
        for j in 1..sv - 1 {
            for i in 0..su {
                faces.push(vec![ring(j, i), ring(j + 1, i), ring(j + 1, i + 1), ring(j, i + 1)]);
            }
        }
        for i in 0..su {
            faces.push(vec![ring(sv - 1, i + 1), ring(sv - 1, i), bottom]);
        }
        Mesh { verts, faces, uvs: Vec::new() }
    }

    pub fn cylinder(r: f32, h: f32, seg: u32, cone: bool) -> Mesh {
        let mut verts = Vec::new();
        let y0 = -h * 0.5;
        let y1 = h * 0.5;
        for i in 0..seg {
            let th = TAU * i as f32 / seg as f32;
            verts.push(v3(r * th.sin(), y0, r * th.cos()));
        }
        let mut faces = Vec::new();
        if cone {
            verts.push(v3(0.0, y1, 0.0));
            let apex = seg;
            for i in 0..seg {
                faces.push(vec![i, (i + 1) % seg, apex]);
            }
        } else {
            for i in 0..seg {
                let th = TAU * i as f32 / seg as f32;
                verts.push(v3(r * th.sin(), y1, r * th.cos()));
            }
            for i in 0..seg {
                let n = (i + 1) % seg;
                faces.push(vec![i, n, seg + n, seg + i]);
            }
            faces.push((0..seg).map(|i| seg + i).collect());
        }
        faces.push((0..seg).rev().collect());
        Mesh { verts, faces, uvs: Vec::new() }
    }

    pub fn plane(size: f32, div: u32) -> Mesh {
        let mut verts = Vec::new();
        let n = div + 1;
        for j in 0..n {
            for i in 0..n {
                let x = -size * 0.5 + size * i as f32 / div as f32;
                let z = -size * 0.5 + size * j as f32 / div as f32;
                verts.push(v3(x, 0.0, z));
            }
        }
        let mut faces = Vec::new();
        for j in 0..div {
            for i in 0..div {
                let a = j * n + i;
                faces.push(vec![a, a + n, a + n + 1, a + 1]);
            }
        }
        Mesh { verts, faces, uvs: Vec::new() }
    }

    pub fn torus(big: f32, small: f32, su: u32, sv: u32) -> Mesh {
        let mut verts = Vec::new();
        for i in 0..su {
            let th = TAU * i as f32 / su as f32;
            for j in 0..sv {
                let ph = TAU * j as f32 / sv as f32;
                let rr = big + small * ph.cos();
                verts.push(v3(rr * th.sin(), small * ph.sin(), rr * th.cos()));
            }
        }
        let idx = |i: u32, j: u32| (i % su) * sv + (j % sv);
        let mut faces = Vec::new();
        for i in 0..su {
            for j in 0..sv {
                faces.push(vec![idx(i, j), idx(i + 1, j), idx(i + 1, j + 1), idx(i, j + 1)]);
            }
        }
        Mesh { verts, faces, uvs: Vec::new() }
    }

    /// Newell normal of a polygon given the (possibly deformed) vertex positions.
    pub fn face_normal(verts: &[V3], f: &[u32]) -> V3 {
        let mut n = V3::ZERO;
        for i in 0..f.len() {
            let a = verts[f[i] as usize];
            let b = verts[f[(i + 1) % f.len()] as usize];
            n.x += (a.y - b.y) * (a.z + b.z);
            n.y += (a.z - b.z) * (a.x + b.x);
            n.z += (a.x - b.x) * (a.y + b.y);
        }
        n.norm()
    }

    pub fn face_center(verts: &[V3], f: &[u32]) -> V3 {
        let mut c = V3::ZERO;
        for &i in f {
            c += verts[i as usize];
        }
        c / f.len().max(1) as f32
    }

    /// Per-face-corner normals, smoothed across edges flatter than `angle_deg`.
    pub fn corner_normals(&self, verts: &[V3], angle_deg: f32) -> Vec<Vec<V3>> {
        let fnorm: Vec<V3> = self.faces.iter().map(|f| Mesh::face_normal(verts, f)).collect();
        let mut vfaces: Vec<Vec<usize>> = vec![Vec::new(); verts.len()];
        for (fi, f) in self.faces.iter().enumerate() {
            for &v in f {
                vfaces[v as usize].push(fi);
            }
        }
        let cos_t = angle_deg.to_radians().cos();
        self.faces
            .iter()
            .enumerate()
            .map(|(fi, f)| {
                f.iter()
                    .map(|&v| {
                        let mut n = V3::ZERO;
                        for &of in &vfaces[v as usize] {
                            if fnorm[of].dot(fnorm[fi]) >= cos_t {
                                n += fnorm[of];
                            }
                        }
                        let n = n.norm();
                        if n.len() < 0.5 { fnorm[fi] } else { n }
                    })
                    .collect()
            })
            .collect()
    }

    /// Extrudes each listed face on its own along its normal. Returns the new cap faces.
    pub fn extrude_faces(&mut self, faces: &[usize], dist: f32) -> Vec<usize> {
        let mut caps = Vec::new();
        for &fi in faces {
            if fi >= self.faces.len() {
                continue;
            }
            let f = self.faces[fi].clone();
            let n = Mesh::face_normal(&self.verts, &f);
            let mut new_idx = Vec::new();
            for &v in &f {
                let p = self.verts[v as usize] + n * dist;
                self.verts.push(p);
                new_idx.push(self.verts.len() as u32 - 1);
            }
            let has_uv = self.has_uvs();
            for i in 0..f.len() {
                let j = (i + 1) % f.len();
                self.faces.push(vec![f[i], f[j], new_idx[j], new_idx[i]]);
                if has_uv {
                    let c = &self.uvs[fi];
                    let (a, b) = (c[i], c[j]);
                    self.uvs.push(vec![a, b, b, a]);
                }
            }
            self.faces[fi] = new_idx;
            caps.push(fi);
        }
        caps
    }

    pub fn has_uvs(&self) -> bool {
        !self.uvs.is_empty() && self.uvs.len() == self.faces.len()
    }

    /// Simple box projection: every face is mapped along its main axis.
    pub fn box_uvs(&mut self) {
        let mut lo = v3(f32::MAX, f32::MAX, f32::MAX);
        let mut hi = v3(f32::MIN, f32::MIN, f32::MIN);
        for p in &self.verts {
            lo = v3(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
            hi = v3(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
        }
        let size = (hi - lo).x.max((hi - lo).y).max((hi - lo).z).max(1e-6);
        self.uvs = self
            .faces
            .iter()
            .map(|f| {
                let axis = Mesh::face_normal(&self.verts, f).max_abs_axis();
                f.iter()
                    .map(|&v| {
                        let p = (self.verts[v as usize] - lo) / size;
                        match axis {
                            0 => [p.z, 1.0 - p.y],
                            1 => [p.x, p.z],
                            _ => [p.x, 1.0 - p.y],
                        }
                    })
                    .collect()
            })
            .collect();
    }

    pub fn delete_faces(&mut self, faces: &[usize]) {
        let mut del = faces.to_vec();
        del.sort_unstable();
        del.dedup();
        let has_uv = self.has_uvs();
        for &fi in del.iter().rev() {
            if fi < self.faces.len() {
                self.faces.remove(fi);
                if has_uv {
                    self.uvs.remove(fi);
                }
            }
        }
        self.remove_unused_verts();
    }

    pub fn remove_unused_verts(&mut self) {
        let mut used = vec![false; self.verts.len()];
        for f in &self.faces {
            for &v in f {
                used[v as usize] = true;
            }
        }
        let mut remap = vec![u32::MAX; self.verts.len()];
        let mut nv = Vec::new();
        for (i, p) in self.verts.iter().enumerate() {
            if used[i] {
                remap[i] = nv.len() as u32;
                nv.push(*p);
            }
        }
        for f in &mut self.faces {
            for v in f.iter_mut() {
                *v = remap[*v as usize];
            }
        }
        self.verts = nv;
    }

    /// One level of Catmull-Clark subdivision.
    pub fn subdivide(&self) -> Mesh {
        let nv = self.verts.len();
        let face_pts: Vec<V3> = self.faces.iter().map(|f| Mesh::face_center(&self.verts, f)).collect();
        // edges
        let mut edge_map: HashMap<(u32, u32), usize> = HashMap::new();
        let mut edges: Vec<(u32, u32, Vec<usize>)> = Vec::new();
        for (fi, f) in self.faces.iter().enumerate() {
            for i in 0..f.len() {
                let a = f[i];
                let b = f[(i + 1) % f.len()];
                let key = (a.min(b), a.max(b));
                let e = *edge_map.entry(key).or_insert_with(|| {
                    edges.push((key.0, key.1, Vec::new()));
                    edges.len() - 1
                });
                edges[e].2.push(fi);
            }
        }
        let edge_pts: Vec<V3> = edges
            .iter()
            .map(|(a, b, fs)| {
                let mid = self.verts[*a as usize] + self.verts[*b as usize];
                if fs.len() == 2 {
                    (mid + face_pts[fs[0]] + face_pts[fs[1]]) / 4.0
                } else {
                    mid / 2.0
                }
            })
            .collect();
        // vertex points
        let mut v_faces: Vec<Vec<usize>> = vec![Vec::new(); nv];
        for (fi, f) in self.faces.iter().enumerate() {
            for &v in f {
                v_faces[v as usize].push(fi);
            }
        }
        let mut v_edges: Vec<Vec<usize>> = vec![Vec::new(); nv];
        for (ei, (a, b, _)) in edges.iter().enumerate() {
            v_edges[*a as usize].push(ei);
            v_edges[*b as usize].push(ei);
        }
        let mut new_verts: Vec<V3> = Vec::with_capacity(nv + edges.len() + self.faces.len());
        for v in 0..nv {
            let p = self.verts[v];
            let boundary: Vec<usize> = v_edges[v].iter().copied().filter(|&e| edges[e].2.len() < 2).collect();
            let n = v_faces[v].len() as f32;
            if !boundary.is_empty() || n < 3.0 {
                if boundary.len() == 2 {
                    let mut s = p * 6.0;
                    for &e in &boundary {
                        let (a, b, _) = &edges[e];
                        let o = if *a as usize == v { *b } else { *a };
                        s += self.verts[o as usize];
                    }
                    new_verts.push(s / 8.0);
                } else {
                    new_verts.push(p);
                }
                continue;
            }
            let mut f = V3::ZERO;
            for &fi in &v_faces[v] {
                f += face_pts[fi];
            }
            f = f / n;
            let mut r = V3::ZERO;
            for &e in &v_edges[v] {
                let (a, b, _) = &edges[e];
                r += (self.verts[*a as usize] + self.verts[*b as usize]) * 0.5;
            }
            r = r / v_edges[v].len() as f32;
            new_verts.push((f + r * 2.0 + p * (n - 3.0)) / n);
        }
        let e_base = new_verts.len() as u32;
        new_verts.extend(edge_pts);
        let f_base = new_verts.len() as u32;
        new_verts.extend(face_pts.iter().copied());
        let mut faces = Vec::new();
        for (fi, f) in self.faces.iter().enumerate() {
            let k = f.len();
            for i in 0..k {
                let prev = f[(i + k - 1) % k];
                let cur = f[i];
                let next = f[(i + 1) % k];
                let e_prev = edge_map[&(prev.min(cur), prev.max(cur))] as u32;
                let e_next = edge_map[&(cur.min(next), cur.max(next))] as u32;
                faces.push(vec![cur, e_base + e_next, f_base + fi as u32, e_base + e_prev]);
            }
        }
        Mesh { verts: new_verts, faces, uvs: Vec::new() }
    }

    /// Smooths vertex positions (Laplacian relax) for the listed vertices or all.
    pub fn relax(&mut self, amount: f32) {
        let mut nb: Vec<Vec<u32>> = vec![Vec::new(); self.verts.len()];
        for f in &self.faces {
            for i in 0..f.len() {
                let a = f[i];
                let b = f[(i + 1) % f.len()];
                nb[a as usize].push(b);
                nb[b as usize].push(a);
            }
        }
        let old = self.verts.clone();
        for (i, p) in self.verts.iter_mut().enumerate() {
            if nb[i].is_empty() {
                continue;
            }
            let mut avg = V3::ZERO;
            for &o in &nb[i] {
                avg += old[o as usize];
            }
            avg = avg / nb[i].len() as f32;
            *p = p.lerp(avg, amount);
        }
    }

    pub fn tri_count(&self) -> usize {
        self.faces.iter().map(|f| f.len().saturating_sub(2)).sum()
    }

    /// Parses a Wavefront OBJ file (positions and faces only).
    pub fn from_obj(text: &str) -> Mesh {
        let mut m = Mesh::default();
        for line in text.lines() {
            let mut it = line.split_whitespace();
            match it.next() {
                Some("v") => {
                    let c: Vec<f32> = it.take(3).filter_map(|s| s.parse().ok()).collect();
                    if c.len() == 3 {
                        m.verts.push(v3(c[0], c[1], c[2]));
                    }
                }
                Some("f") => {
                    let n = m.verts.len() as i64;
                    let f: Vec<u32> = it
                        .filter_map(|s| s.split('/').next()?.parse::<i64>().ok())
                        .map(|i| if i < 0 { n + i } else { i - 1 })
                        .filter(|&i| i >= 0 && i < n)
                        .map(|i| i as u32)
                        .collect();
                    if f.len() >= 3 {
                        m.faces.push(f);
                    }
                }
                _ => {}
            }
        }
        m
    }
}
