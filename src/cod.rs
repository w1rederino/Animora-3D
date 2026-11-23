//! Call of Duty intermediate files, as written by the CoD Maya tools:
//! `.xanim_export` (animation), `.NT_EXPORT` (notetracks) and
//! `.xmodel_export` (skeleton + skinned meshes). All are text formats.
//!
//! Conventions (same as the Maya plugin): positions are world space in
//! inches (Maya centimetres × 0.3937), each joint's world X/Y/Z axes are
//! written as three rows, frames are numbered from 0, triangles are written
//! in reversed (clockwise) order.

use crate::anim::*;
use crate::app::*;
use crate::math::*;
use crate::mesh::*;
use crate::scene::*;
use egui::{Color32, Ui};
use std::collections::HashMap;
use std::path::Path;

pub const CM_TO_INCH: f32 = 0.393_700_8;

#[derive(Clone, Copy, Debug)]
pub struct XPose {
    pub offset: V3,
    pub axes: [V3; 3],
}

impl Default for XPose {
    fn default() -> Self {
        XPose { offset: V3::ZERO, axes: [V3::X, V3::Y, V3::Z] }
    }
}

impl XPose {
    /// World matrix in scene units (`k` = scene units per inch).
    pub fn matrix(&self, k: f32) -> M4 {
        let mut m = M4::IDENTITY;
        // re-orthonormalise (files are written with 6 decimals)
        let x = self.axes[0].norm();
        let z = x.cross(self.axes[1]).norm();
        let y = z.cross(x).norm();
        let z = if z.dot(self.axes[2]) < 0.0 { -z } else { z };
        m.set_col(0, x);
        m.set_col(1, y);
        m.set_col(2, z);
        m.set_col(3, self.offset * k);
        m
    }

    /// From a world matrix in scene units (`k` = inches per scene unit).
    pub fn from_matrix(m: &M4, k: f32) -> XPose {
        let r = m.rotation_only();
        XPose { offset: m.translation() * k, axes: [r.col(0), r.col(1), r.col(2)] }
    }
}

pub struct XAnim {
    pub framerate: f32,
    pub parts: Vec<String>,
    pub frames: Vec<Vec<XPose>>,
    pub notes: Vec<Note>,
}

pub struct XBone {
    pub name: String,
    pub parent: i32,
    pub pose: XPose,
}

pub struct XVert {
    pub pos: V3,
    pub weights: Vec<(usize, f32)>,
}

pub struct XTri {
    pub object: usize,
    pub material: usize,
    pub v: [usize; 3],
    pub n: [V3; 3],
}

pub struct XMat {
    pub name: String,
    pub color: [f32; 3],
}

pub struct XModel {
    pub version: u32,
    pub bones: Vec<XBone>,
    pub verts: Vec<XVert>,
    pub tris: Vec<XTri>,
    pub objects: Vec<String>,
    pub materials: Vec<XMat>,
}

/// Splits a line into tokens; quoted strings stay together, trailing commas go.
fn tokens(line: &str) -> Vec<String> {
    let line = match line.find("//") {
        Some(i) if !line[..i].contains('"') => &line[..i],
        _ => line,
    };
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for ch in line.chars() {
        match ch {
            '"' => {
                if quoted {
                    out.push(std::mem::take(&mut cur));
                }
                quoted = !quoted;
            }
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    for t in &mut out {
        while t.ends_with(',') {
            t.pop();
        }
    }
    out.retain(|t| !t.is_empty());
    out
}

fn num(t: &[String], i: usize) -> f32 {
    t.get(i).and_then(|s| s.parse::<f32>().ok()).unwrap_or(0.0)
}

fn int(t: &[String], i: usize) -> i64 {
    t.get(i).and_then(|s| s.parse::<f64>().ok()).map(|v| v as i64).unwrap_or(0)
}

fn vec3(t: &[String]) -> V3 {
    v3(num(t, 1), num(t, 2), num(t, 3))
}

fn f6(v: f32) -> String {
    let v = if v.abs() < 5e-7 { 0.0 } else { v };
    format!("{v:.6}")
}

fn v6(v: V3) -> String {
    format!("{} {} {}", f6(v.x), f6(v.y), f6(v.z))
}

fn axis6(v: V3) -> String {
    v6(v3(v.x.clamp(-1.0, 1.0), v.y.clamp(-1.0, 1.0), v.z.clamp(-1.0, 1.0)))
}

/// Keeps only the characters note names may have.
pub fn clean_note(name: &str) -> String {
    name.chars().filter(|c| c.is_alphanumeric() || *c == '_' || *c == '#').collect()
}

pub fn export_time() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    // civil-from-days (proleptic Gregorian)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02} UTC", rem / 3600, rem % 3600 / 60, rem % 60)
}

// ---------------------------------------------------------------- XANIM

pub fn parse_xanim(text: &str) -> Result<XAnim, String> {
    let mut a = XAnim { framerate: 30.0, parts: Vec::new(), frames: Vec::new(), notes: Vec::new() };
    let mut is_anim = false;
    let mut in_frames = false;
    let mut in_notes = false;
    let mut part: usize = 0;
    for line in text.lines() {
        let t = tokens(line);
        let Some(head) = t.first() else { continue };
        match head.as_str() {
            "ANIMATION" => is_anim = true,
            "NUMPARTS" => a.parts = vec![String::new(); int(&t, 1).max(0) as usize],
            "FRAMERATE" => {
                a.framerate = num(&t, 1);
                in_frames = true;
            }
            "NUMFRAMES" => {
                in_frames = true;
            }
            "NOTETRACKS" | "NUMKEYS" => {
                in_notes = true;
            }
            "PART" if !in_frames => {
                let i = int(&t, 1).max(0) as usize;
                if i < a.parts.len() {
                    a.parts[i] = t.get(2).cloned().unwrap_or_default();
                }
            }
            "FRAME" if in_notes || t.len() >= 3 => {
                if t.len() >= 3 {
                    a.notes.push(Note { frame: num(&t, 1), name: t[2].clone() });
                }
            }
            "FRAME" => {
                a.frames.push(vec![XPose::default(); a.parts.len()]);
            }
            "PART" if !in_notes => {
                part = int(&t, 1).max(0) as usize;
            }
            "OFFSET" | "X" | "Y" | "Z" if !in_notes => {
                if let Some(f) = a.frames.last_mut() {
                    if let Some(p) = f.get_mut(part) {
                        match head.as_str() {
                            "OFFSET" => p.offset = vec3(&t),
                            "X" => p.axes[0] = vec3(&t),
                            "Y" => p.axes[1] = vec3(&t),
                            _ => p.axes[2] = vec3(&t),
                        }
                    }
                }
            }
            _ => {}
        }
    }
    if !is_anim {
        return Err(crate::tr!("Keine XANIM_EXPORT-Datei (Kopfzeile ANIMATION fehlt).", "Not an XANIM_EXPORT file (ANIMATION header missing).").into());
    }
    if a.parts.is_empty() || a.frames.is_empty() {
        return Err(crate::tr!("Die Datei enthält keine Gelenke oder keine Bilder.", "The file contains no joints or no frames.").into());
    }
    if a.framerate <= 0.0 {
        a.framerate = 30.0;
    }
    Ok(a)
}

pub fn write_xanim(a: &XAnim, path_label: &str, embed_notes: bool) -> String {
    let mut s = String::new();
    s.push_str(&format!("// Export filename: {path_label}\n// Source: Animora 3D\n// Export time: {}\n\n", export_time()));
    s.push_str("ANIMATION\nVERSION 3\n\n");
    s.push_str(&format!("NUMPARTS {}\n", a.parts.len()));
    for (i, p) in a.parts.iter().enumerate() {
        s.push_str(&format!("PART {i} \"{p}\"\n"));
    }
    s.push('\n');
    let fr = format!("{}", a.framerate);
    s.push_str(&format!("FRAMERATE {fr}\nNUMFRAMES {}\n", a.frames.len()));
    for (fi, f) in a.frames.iter().enumerate() {
        s.push_str(&format!("FRAME {fi}\n"));
        for (pi, p) in f.iter().enumerate() {
            s.push_str(&format!("PART {pi}\nOFFSET {}\nSCALE 1.000000 1.000000 1.000000\n", v6(p.offset)));
            s.push_str(&format!("X {}\nY {}\nZ {}\n\n", axis6(p.axes[0]), axis6(p.axes[1]), axis6(p.axes[2])));
        }
    }
    s.push_str("NOTETRACKS\n\n");
    if embed_notes {
        for pi in 0..a.parts.len() {
            let tracks = if pi == 0 && !a.notes.is_empty() { 1 } else { 0 };
            s.push_str(&format!("PART {pi}\nNUMTRACKS {tracks}\n\n"));
            if tracks == 1 {
                s.push_str(&format!("NOTETRACK 0\nNUMKEYS {}\n", a.notes.len()));
                for n in &a.notes {
                    s.push_str(&format!("FRAME {} \"{}\"\n", n.frame.round() as i64, n.name));
                }
                s.push('\n');
            }
        }
    }
    s
}

pub fn parse_nt(text: &str) -> Vec<Note> {
    text.lines()
        .map(tokens)
        .filter(|t| t.first().map(|h| h == "FRAME").unwrap_or(false) && t.len() >= 3)
        .map(|t| Note { frame: num(&t, 1), name: t[2].clone() })
        .collect()
}

pub fn write_nt(notes: &[Note], first: i64, count: usize) -> String {
    let mut s = format!("FIRSTFRAME {first}\nNUMFRAMES {count}\nNUMKEYS {}\n", notes.len());
    for n in notes {
        s.push_str(&format!("FRAME {} \"{}\"\n", n.frame.round() as i64, n.name));
    }
    s
}

// ---------------------------------------------------------------- XMODEL

pub fn parse_xmodel(text: &str) -> Result<XModel, String> {
    #[derive(PartialEq)]
    enum Sec {
        Head,
        Bones,
        Verts,
        Faces,
        Objects,
        Materials,
    }
    let mut m = XModel { version: 6, bones: Vec::new(), verts: Vec::new(), tris: Vec::new(), objects: Vec::new(), materials: Vec::new() };
    let mut is_model = false;
    let mut sec = Sec::Head;
    let mut bone_cur: usize = 0;
    let mut corner: usize = 0;
    for line in text.lines() {
        let t = tokens(line);
        let Some(head) = t.first() else { continue };
        match head.as_str() {
            "MODEL" => is_model = true,
            "VERSION" if sec == Sec::Head => m.version = int(&t, 1) as u32,
            "NUMBONES" => {
                sec = Sec::Bones;
                let n = int(&t, 1).max(0) as usize;
                m.bones = (0..n).map(|_| XBone { name: String::new(), parent: -1, pose: XPose::default() }).collect();
            }
            "NUMCOSMETICS" => {}
            "BONE" if sec == Sec::Bones => {
                let i = int(&t, 1).max(0) as usize;
                if t.len() >= 4 {
                    if let Some(b) = m.bones.get_mut(i) {
                        b.parent = int(&t, 2) as i32;
                        b.name = t[3].clone();
                    }
                } else {
                    bone_cur = i;
                }
            }
            "OFFSET" if sec == Sec::Bones => {
                if let Some(b) = m.bones.get_mut(bone_cur) {
                    b.pose.offset = vec3(&t);
                }
            }
            "X" | "Y" | "Z" if sec == Sec::Bones => {
                if let Some(b) = m.bones.get_mut(bone_cur) {
                    let a = match head.as_str() {
                        "X" => 0,
                        "Y" => 1,
                        _ => 2,
                    };
                    b.pose.axes[a] = vec3(&t);
                }
            }
            "NUMVERTS" | "NUMVERTS32" => sec = Sec::Verts,
            "VERT" | "VERT32" if sec == Sec::Verts => m.verts.push(XVert { pos: V3::ZERO, weights: Vec::new() }),
            "OFFSET" if sec == Sec::Verts => {
                if let Some(v) = m.verts.last_mut() {
                    v.pos = vec3(&t);
                }
            }
            "BONE" if sec == Sec::Verts => {
                if let Some(v) = m.verts.last_mut() {
                    v.weights.push((int(&t, 1).max(0) as usize, num(&t, 2)));
                }
            }
            "NUMFACES" => sec = Sec::Faces,
            h if sec == Sec::Faces && h.starts_with("TRI") => {
                m.tris.push(XTri { object: int(&t, 1).max(0) as usize, material: int(&t, 2).max(0) as usize, v: [0; 3], n: [V3::Y; 3] });
                corner = 0;
            }
            "VERT" | "VERT32" if sec == Sec::Faces => {
                if let Some(tri) = m.tris.last_mut() {
                    if corner < 3 {
                        tri.v[corner] = int(&t, 1).max(0) as usize;
                        if t.len() >= 5 {
                            // version 5: VERT i nx ny nz u v
                            tri.n[corner] = v3(num(&t, 2), num(&t, 3), num(&t, 4));
                        }
                        corner += 1;
                    }
                }
            }
            "NORMAL" if sec == Sec::Faces => {
                if let Some(tri) = m.tris.last_mut() {
                    if corner >= 1 && corner <= 3 {
                        tri.n[corner - 1] = vec3(&t);
                    }
                }
            }
            "NUMOBJECTS" => sec = Sec::Objects,
            "OBJECT" if sec == Sec::Objects => m.objects.push(t.get(2).cloned().unwrap_or_else(|| crate::trf!("objekt{}", "object{}", m.objects.len()))),
            "NUMMATERIALS" => sec = Sec::Materials,
            "MATERIAL" if sec == Sec::Materials => m.materials.push(XMat { name: t.get(2).cloned().unwrap_or_default(), color: [0.62, 0.62, 0.62] }),
            "COLOR" if sec == Sec::Materials => {
                if let Some(mat) = m.materials.last_mut() {
                    let c = vec3(&t);
                    if c.x + c.y + c.z > 0.01 {
                        mat.color = [c.x.clamp(0.0, 1.0), c.y.clamp(0.0, 1.0), c.z.clamp(0.0, 1.0)];
                    }
                }
            }
            _ => {}
        }
    }
    if !is_model {
        return Err(crate::tr!("Keine XMODEL_EXPORT-Datei (Kopfzeile MODEL fehlt).", "Not an XMODEL_EXPORT file (MODEL header missing).").into());
    }
    if m.bones.is_empty() {
        return Err(crate::tr!("Das Modell enthält keine Gelenke.", "The model contains no joints.").into());
    }
    Ok(m)
}

pub fn write_xmodel(m: &XModel, path_label: &str) -> String {
    let v = m.version.clamp(6, 7);
    let mut s = format!("// Export filename: {path_label}\n// Source: Animora 3D\n// Export time: {}\n\nMODEL\nVERSION {v}\n\n", export_time());
    s.push_str(&format!("NUMBONES {}\n", m.bones.len()));
    for (i, b) in m.bones.iter().enumerate() {
        s.push_str(&format!("BONE {i} {} \"{}\"\n", b.parent, b.name));
    }
    s.push('\n');
    for (i, b) in m.bones.iter().enumerate() {
        s.push_str(&format!("BONE {i}\nOFFSET {}\nSCALE 1.000000 1.000000 1.000000\n", v6(b.pose.offset)));
        s.push_str(&format!("X {}\nY {}\nZ {}\n\n", axis6(b.pose.axes[0]), axis6(b.pose.axes[1]), axis6(b.pose.axes[2])));
    }
    s.push('\n');
    let big = v == 7 && m.verts.len() > 0xFFFF;
    let vt = if big { "VERT32" } else { "VERT" };
    s.push_str(&format!("NUMVERTS{} {}\n", if big { "32" } else { "" }, m.verts.len()));
    for (i, vert) in m.verts.iter().enumerate() {
        s.push_str(&format!("{vt} {i}\nOFFSET {}\nBONES {}\n", v6(vert.pos), vert.weights.len()));
        for (b, w) in &vert.weights {
            s.push_str(&format!("BONE {b} {}\n", f6(*w)));
        }
        s.push('\n');
    }
    s.push_str(&format!("NUMFACES {}\n", m.tris.len()));
    for tri in &m.tris {
        let tok = if v >= 7 && (tri.object > 255 || tri.material > 255) { "TRI16" } else { "TRI" };
        s.push_str(&format!("{tok} {} {} 0 0\n", tri.object, tri.material));
        for c in 0..3 {
            let n = tri.n[c].norm();
            s.push_str(&format!("{vt} {}\nNORMAL {}\nCOLOR 1.000000 1.000000 1.000000 1.000000\nUV 1 0.000000 0.000000\n\n", tri.v[c], axis6(n)));
        }
    }
    s.push_str(&format!("NUMOBJECTS {}\n", m.objects.len()));
    for (i, o) in m.objects.iter().enumerate() {
        s.push_str(&format!("OBJECT {i} \"{o}\"\n"));
    }
    s.push('\n');
    s.push_str(&format!("NUMMATERIALS {}\n", m.materials.len()));
    for (i, mat) in m.materials.iter().enumerate() {
        let c = mat.color;
        s.push_str(&format!("MATERIAL {i} \"{}\" \"Lambert\" \"color:$none.tga\"\n", mat.name));
        s.push_str(&format!("COLOR {} {} {} 1.000000\n", f6(c[0]), f6(c[1]), f6(c[2])));
        s.push_str("TRANSPARENCY 0.000000 0.000000 0.000000 1.000000\nAMBIENTCOLOR 0.000000 0.000000 0.000000 1.000000\n");
        s.push_str("INCANDESCENCE 0.000000 0.000000 0.000000 1.000000\nCOEFFS 0.800000 0.000000\nGLOW 0.000000 0\n");
        s.push_str("REFRACTIVE 6 1.000000\nSPECULARCOLOR -1.000000 -1.000000 -1.000000 1.000000\n");
        s.push_str("REFLECTIVECOLOR -1.000000 -1.000000 -1.000000 1.000000\nREFLECTIVE -1 1.000000\nBLINN -1.000000 -1.000000\nPHONG -1.000000\n\n");
    }
    s
}

// ---------------------------------------------------------------- app side

pub struct CodState {
    pub show: bool,
    pub tab: usize,
    pub start: f32,
    pub end: f32,
    pub fps: f32,
    pub reverse: bool,
    pub tag_align: bool,
    pub nt_file: bool,
    pub unit_cm: bool,
    pub version: u32,
    pub create_missing: bool,
    pub replace_anim: bool,
    pub fit_range: bool,
    pub new_note: String,
    pub message: String,
    pub range_init: bool,
    /// Convert between the game's Z-up axes and Animora's Y-up axes.
    pub z_up: bool,
    /// Model groups created by XModel imports (newest last).
    pub models: Vec<Id>,
    /// Models NOT moved by animations (unticked in the list).
    pub excluded: std::collections::HashSet<Id>,
    /// The last attached animation file (for "attach again").
    pub last_anim: Option<std::path::PathBuf>,
    /// Export option of the CoD tools: rename J_GUN to tag_weapon etc.
    pub auto_rename: bool,
}

impl Default for CodState {
    fn default() -> Self {
        CodState {
            show: false,
            tab: 0,
            start: 1.0,
            end: 120.0,
            fps: 30.0,
            reverse: false,
            tag_align: false,
            nt_file: false,
            unit_cm: true,
            version: 6,
            create_missing: true,
            replace_anim: true,
            fit_range: true,
            new_note: "end".into(),
            message: String::new(),
            range_init: false,
            z_up: true,
            models: Vec::new(),
            excluded: Default::default(),
            last_anim: None,
            auto_rename: false,
        }
    }
}

fn unwrap_angle(prev: f32, v: f32) -> f32 {
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
    /// Inches per scene unit.
    fn cod_k(&self) -> f32 {
        if self.cod.unit_cm { CM_TO_INCH } else { 1.0 }
    }

    /// Nodes to write as CoD parts/bones: joints (and groups) of the selected
    /// hierarchies, or every joint when nothing usable is selected. Parents
    /// always come before their children.
    pub fn cod_joints(&self) -> Vec<Id> {
        let mut chosen: Vec<Id> = Vec::new();
        for &s in &self.sel {
            for id in self.scene.subtree(s) {
                if self.scene.get(id).is_some_and(|n| matches!(n.kind, Kind::Joint)) && !chosen.contains(&id) {
                    chosen.push(id);
                }
            }
            // a selected group/locator such as tag_origin counts too
            if self.scene.get(s).is_some_and(|n| n.kind == Kind::Group) && !chosen.contains(&s) {
                chosen.push(s);
            }
        }
        if chosen.is_empty() {
            chosen = self.scene.nodes.iter().filter(|n| n.kind == Kind::Joint).map(|n| n.id).collect();
        }
        // hierarchy order
        let mut order = Vec::new();
        fn walk(s: &Scene, id: Id, chosen: &[Id], out: &mut Vec<Id>) {
            if chosen.contains(&id) {
                out.push(id);
            }
            for c in s.children(Some(id)) {
                walk(s, c, chosen, out);
            }
        }
        for r in self.scene.children(None) {
            walk(&self.scene, r, &chosen, &mut order);
        }
        order
    }

    fn cod_parent_index(&self, list: &[Id], id: Id) -> i32 {
        let mut cur = self.scene.get(id).and_then(|n| n.parent);
        while let Some(c) = cur {
            if let Some(i) = list.iter().position(|x| *x == c) {
                return i as i32;
            }
            cur = self.scene.get(c).and_then(|n| n.parent);
        }
        -1
    }

    pub fn export_xanim(&mut self, path: &Path) -> Result<String, String> {
        let joints = self.cod_joints();
        if joints.is_empty() {
            return Err(crate::tr!("Keine Gelenke zum Exportieren – Gelenke auswählen oder anlegen.", "No joints to export – select or create joints.").into());
        }
        let (start, end) = (self.cod.start.round(), self.cod.end.round());
        if end < start {
            return Err(crate::tr!("Ungültiger Bildbereich (Ende vor Anfang).", "Invalid frame range (end before start).").into());
        }
        let k = self.cod_k();
        let mut parts: Vec<String> = joints
            .iter()
            .filter_map(|id| self.scene.get(*id))
            .map(|n| {
                if self.cod.auto_rename {
                    let pn = n.parent.and_then(|p| self.scene.get(p)).map(|p| p.name.clone());
                    crate::weapon::auto_rename(&n.name, pn.as_deref(), pn.as_deref().map(|x| crate::weapon::auto_rename(x, None, None)).as_deref())
                } else {
                    n.name.clone()
                }
            })
            .collect();
        if self.cod.tag_align {
            parts.push("TAG_ALIGN".into());
        }
        let mut frames_order: Vec<f32> = Vec::new();
        let mut f = start;
        while f <= end {
            frames_order.push(f);
            f += 1.0;
        }
        if self.cod.reverse {
            frames_order.reverse();
        }
        let to_file = self.cod_to_file();
        let mut s = self.scene.clone();
        let mut frames = Vec::with_capacity(frames_order.len());
        for &fr in &frames_order {
            s.evaluate(fr);
            let w = s.world_matrices();
            let mut poses: Vec<XPose> = joints.iter().map(|id| XPose::from_matrix(&(to_file * w[id]), k)).collect();
            if self.cod.tag_align {
                poses.push(XPose::default());
            }
            frames.push(poses);
        }
        let n = frames_order.len() as f32;
        let mut notes: Vec<Note> = self
            .scene
            .notes
            .iter()
            .filter(|nt| nt.frame >= start && nt.frame <= end)
            .map(|nt| {
                let rel = nt.frame - start;
                Note { frame: if self.cod.reverse { n - 1.0 - rel } else { rel }, name: clean_note(&nt.name) }
            })
            .filter(|nt| !nt.name.is_empty())
            .collect();
        notes.sort_by(|a, b| a.frame.total_cmp(&b.frame));
        let anim = XAnim { framerate: self.cod.fps, parts, frames, notes };
        let label = path.display().to_string();
        std::fs::write(path, write_xanim(&anim, &label, !self.cod.nt_file)).map_err(|e| e.to_string())?;
        if self.cod.nt_file {
            let nt = path.with_extension("NT_EXPORT");
            std::fs::write(&nt, write_nt(&anim.notes, 0, anim.frames.len())).map_err(|e| e.to_string())?;
        }
        Ok(crate::trf!(
            "XAnim exportiert: {} Gelenke, {} Bilder, {} Notizen → {}", "XAnim exported: {} joints, {} frames, {} notes → {}",
            joints.len(),
            anim.frames.len(),
            anim.notes.len(),
            path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default()
        ))
    }

    /// Attaches one XAnim to all chosen models (arms, weapon, scope …):
    /// every model joint whose name is in the anim is moved by it.
    pub fn import_xanim(&mut self, path: &Path) -> Result<String, String> {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let mut anim = parse_xanim(&text)?;
        // a separate notetrack file next to the anim
        for ext in ["NT_EXPORT", "nt_export"] {
            let p = path.with_extension(ext);
            if p.exists() {
                if let Ok(t) = std::fs::read_to_string(&p) {
                    anim.notes.extend(parse_nt(&t));
                }
                break;
            }
        }
        let k = 1.0 / self.cod_k();
        let targets = self.cod_targets();
        if targets.is_empty() {
            return Err(crate::tr!("Bitte zuerst die XModels importieren (z. B. Arme und Waffe) – die Animation wird dann an alle angehängt.", "Please import the XModels first (e.g. arms and weapon) – the animation is then attached to all of them.").into());
        }
        // match the anim's parts in every model
        let mut maps: Vec<(Id, Vec<Option<Id>>)> = Vec::new();
        for &t in &targets {
            let pool: Vec<Id> = self.model_nodes(t).into_iter().filter(|id| self.scene.get(*id).is_some_and(|n| n.kind != Kind::Mesh && n.kind != Kind::Group)).collect();
            let map: Vec<Option<Id>> = anim
                .parts
                .iter()
                .map(|name| {
                    let base = name.rsplit(':').next().unwrap_or(name);
                    pool.iter().copied().find(|id| {
                        self.scene.get(*id).is_some_and(|n| n.name.eq_ignore_ascii_case(name) || n.name.rsplit(':').next().unwrap_or(&n.name).eq_ignore_ascii_case(base))
                    })
                })
                .collect();
            maps.push((t, map));
        }
        let used_part = |pi: usize, maps: &[(Id, Vec<Option<Id>>)]| maps.iter().any(|(_, m)| m[pi].is_some());
        let matched_parts = (0..anim.parts.len()).filter(|pi| used_part(*pi, &maps)).count();
        if matched_parts == 0 {
            let names: Vec<String> = targets.iter().filter_map(|t| self.scene.get(*t).map(|n| n.name.clone())).collect();
            return Err(crate::trf!(
                "Kein Gelenk der Animation passt zu den Modellen ({}). Sind es die richtigen XModels? (Gelenke in der Datei z. B.: {})", "No joint of the animation matches the models ({}). Are these the right XModels? (joints in the file e.g.: {})",
                names.join(", "),
                anim.parts.iter().take(4).cloned().collect::<Vec<_>>().join(", ")
            ));
        }
        self.checkpoint();
        // joints only the anim has (in no model) are added to the first model
        let mut created = 0;
        if self.cod.create_missing {
            let first = targets[0];
            let radius = self.scene.subtree(first).iter().filter_map(|id| self.scene.get(*id)).find(|n| n.kind == Kind::Joint).map(|n| n.joint_radius).unwrap_or(0.12);
            for pi in 0..anim.parts.len() {
                let name = anim.parts[pi].clone();
                if used_part(pi, &maps) || name.is_empty() || name == "TAG_ALIGN" {
                    continue;
                }
                let mut n = Node::new(0, &name, Kind::Joint);
                n.parent = Some(first);
                n.joint_radius = radius;
                let id = self.scene.add(n);
                maps[0].1[pi] = Some(id);
                created += 1;
            }
        }
        let start = self.scene.start;
        let mut report = Vec::new();
        let mut following = 0;
        for (target, map) in &maps {
            let (moved, follow) = self.apply_xanim_to_model(*target, map, &anim, k, start);
            following += follow;
            let name = self.scene.get(*target).map(|n| n.name.clone()).unwrap_or_default();
            report.push(format!("{name} ({moved})"));
        }
        let note_count = anim.notes.len();
        if self.cod.replace_anim {
            self.scene.notes.clear();
        }
        for n in anim.notes {
            self.scene.notes.push(Note { frame: start + n.frame, name: n.name });
        }
        self.scene.notes.sort_by(|a, b| a.frame.total_cmp(&b.frame));
        if self.cod.fit_range {
            self.scene.fps = anim.framerate;
            self.scene.end = start + (anim.frames.len() as f32 - 1.0).max(1.0);
        }
        // IK handles would fight the imported motion
        let driven: Vec<Id> = maps.iter().flat_map(|(_, m)| m.iter().flatten().copied()).collect();
        for n in &mut self.scene.nodes {
            if n.kind == Kind::IkHandle && driven.iter().any(|j| Some(*j) == n.ik_end) {
                n.visible = false;
            }
        }
        self.cod.last_anim = Some(path.to_path_buf());
        // joints shared by name across models sit where the animated one is
        let aligned = self.align_shared_joints(&targets);
        self.set_frame(start);
        self.sel = targets.clone();
        self.graph.fit_request = true;
        let unused = anim.parts.len() - matched_parts - created;
        let mut msg = crate::trf!(
            "Animation an {} Modell(e) angehängt – bewegte Gelenke: {} · {} Bilder bei {} B/s · {} neu angelegt · {} Notizen", "Animation attached to {} model(s) – moved joints: {} · {} frames at {} fps · {} created · {} notes",
            maps.len(),
            report.join(", "),
            anim.frames.len(),
            anim.framerate,
            created,
            note_count
        );
        if aligned > 0 {
            msg.push_str(&crate::trf!(" · {aligned} gleichnamige Gelenke ausgerichtet", " · {aligned} same-named joints aligned"));
        }
        if following > 0 {
            msg.push_str(&crate::trf!(" · {following} Modell-Gelenke folgen ihren Eltern", " · {following} model joints follow their parents"));
        }
        if unused > 0 {
            msg.push_str(&crate::trf!(" · {unused} Gelenke der Animation in keinem Modell", " · {unused} animation joints are in no model"));
        }
        Ok(msg)
    }

    /// Keys one model's skeleton from the anim. Returns (joints moved by the
    /// anim, model joints that only follow their parents).
    fn apply_xanim_to_model(&mut self, target: Id, map: &[Option<Id>], anim: &XAnim, k: f32, start: f32) -> (usize, usize) {
        // reset the skeleton to its bind pose first
        let mut skeleton: Vec<Id> = vec![target];
        skeleton.extend(self.model_nodes(target));
        for id in &skeleton {
            if let Some(n) = self.scene.get_mut(*id) {
                if n.kind == Kind::Mesh || *id == target {
                    continue;
                }
                if let Some(rest) = n.rest {
                    n.t = rest[0];
                    n.r = rest[1];
                    n.s = rest[2];
                }
                if self.cod.replace_anim {
                    for c in 0..9 {
                        n.curves[c] = Curve::default();
                    }
                }
            }
        }
        let w0 = self.scene.world_matrices();
        // game space of this model = world of its top-level model (up-axis fix);
        // an attached model follows the joint it hangs on
        let mut top = target;
        while let Some(p) = self.scene.get(top).and_then(|n| n.parent) {
            top = p;
        }
        let space = w0.get(&top).copied().unwrap_or_default();
        let model_world = if top == target { w0.get(&target).copied().unwrap_or_default() } else { space };
        let attach_part: Option<usize> = self
            .scene
            .get(target)
            .and_then(|n| n.parent)
            .and_then(|p| self.scene.get(p))
            .and_then(|pn| anim.parts.iter().position(|q| q.eq_ignore_ascii_case(&pn.name)));
        let part_of: HashMap<Id, usize> = map.iter().enumerate().filter_map(|(i, id)| id.map(|id| (id, i))).collect();
        let mut prev_r: HashMap<Id, V3> = HashMap::new();
        let mut new_keys: HashMap<Id, Vec<Vec<Key>>> = HashMap::new();
        for (fi, poses) in anim.frames.iter().enumerate() {
            let frame = start + fi as f32;
            // walk the hierarchy (parents first): animated joints take the file
            // pose, the others follow their parent with their bind pose
            let mut world: HashMap<Id, M4> = HashMap::new();
            let target_world = match attach_part.and_then(|pi| poses.get(pi)) {
                Some(p) => space * p.matrix(k),
                None => w0.get(&target).copied().unwrap_or_default(),
            };
            world.insert(target, target_world);
            for &id in &skeleton {
                if id == target {
                    continue;
                }
                let Some(n) = self.scene.get(id) else { continue };
                let pw = n.parent.and_then(|p| world.get(&p).copied().or_else(|| w0.get(&p).copied())).unwrap_or(M4::IDENTITY);
                match part_of.get(&id).and_then(|pi| poses.get(*pi)) {
                    Some(pose) => {
                        let wn = model_world * pose.matrix(k);
                        let (t, mut r, s) = (pw.inverse() * wn).decompose();
                        if let Some(pr) = prev_r.get(&id) {
                            r = v3(unwrap_angle(pr.x, r.x), unwrap_angle(pr.y, r.y), unwrap_angle(pr.z, r.z));
                        }
                        prev_r.insert(id, r);
                        let ch = new_keys.entry(id).or_insert_with(|| vec![Vec::new(); 9]);
                        for c in 0..3 {
                            ch[c].push(Key { frame, value: t.get(c), tangent: Tangent::Linear });
                            ch[3 + c].push(Key { frame, value: r.get(c), tangent: Tangent::Linear });
                            ch[6 + c].push(Key { frame, value: s.get(c), tangent: Tangent::Linear });
                        }
                        world.insert(id, wn);
                    }
                    None => {
                        world.insert(id, pw * n.local());
                    }
                }
            }
        }
        let moved = new_keys.len();
        for (id, chans) in new_keys {
            if let Some(n) = self.scene.get_mut(id) {
                // drop scale curves that never leave 1.0
                let scaled = chans[6..9].iter().flatten().any(|k| (k.value - 1.0).abs() > 1e-3);
                for (c, keys) in chans.into_iter().enumerate() {
                    if c >= 6 && !scaled {
                        continue;
                    }
                    let frames: std::collections::HashSet<i64> = keys.iter().map(|k| (k.frame * 1000.0).round() as i64).collect();
                    n.curves[c].keys.retain(|k| !frames.contains(&((k.frame * 1000.0).round() as i64)));
                    n.curves[c].keys.extend(keys);
                    n.curves[c].sort();
                }
            }
        }
        let following = skeleton.iter().filter(|id| self.scene.get(**id).is_some_and(|n| n.kind == Kind::Joint) && !part_of.contains_key(id)).count();
        (moved, following)
    }

    /// Matrix from Animora (Y-up) to game space (Z-up) for exports.
    pub fn cod_to_file(&self) -> M4 {
        if self.cod.z_up { M4::rot_x(90f32.to_radians()) } else { M4::IDENTITY }
    }

    /// Imported models in the scene: top-level groups that contain joints.
    pub fn cod_model_groups(&self) -> Vec<Id> {
        self.scene
            .nodes
            .iter()
            .filter(|n| n.kind == Kind::Group)
            // top-level models, or models attached to a joint of another model
            .filter(|n| n.parent.is_none_or(|p| self.scene.get(p).is_some_and(|pn| pn.kind == Kind::Joint)))
            .filter(|n| self.scene.subtree(n.id).iter().any(|c| self.scene.get(*c).is_some_and(|m| m.kind == Kind::Joint)))
            .map(|n| n.id)
            .collect()
    }

    /// Nodes that belong to model `t` itself, not to models attached inside
    /// it (parents come before children).
    pub fn model_nodes(&self, t: Id) -> Vec<Id> {
        let groups = self.cod_model_groups();
        self.scene
            .subtree(t)
            .into_iter()
            .filter(|id| *id != t)
            .filter(|id| {
                let mut cur = self.scene.get(*id).and_then(|n| n.parent);
                while let Some(c) = cur {
                    if groups.contains(&c) {
                        return c == t;
                    }
                    cur = self.scene.get(c).and_then(|n| n.parent);
                }
                false
            })
            .collect()
    }

    /// Hangs a model on a joint of another model (e.g. the weapon in the
    /// hand), or frees it again keeping its place (`None`).
    pub fn attach_model(&mut self, group: Id, joint: Option<Id>) {
        if let Some(j) = joint {
            if j == group || self.scene.is_ancestor(group, j) {
                return;
            }
        }
        let world = self.scene.world_matrices().get(&group).copied().unwrap_or_default();
        if let Some(g) = self.scene.get_mut(group) {
            match joint {
                Some(j) => {
                    g.parent = Some(j);
                    g.t = V3::ZERO;
                    g.r = V3::ZERO;
                    g.s = V3::ONE;
                }
                None => {
                    let (t, r, s) = world.decompose();
                    g.parent = None;
                    g.t = t;
                    g.r = r;
                    g.s = s;
                }
            }
        }
        if !self.cod.models.contains(&group) {
            self.cod.models.push(group);
        }
    }

    /// The models an animation is attached to (all ticked models).
    pub fn cod_targets(&self) -> Vec<Id> {
        self.cod_model_groups().into_iter().filter(|id| !self.cod.excluded.contains(id)).collect()
    }

    pub fn export_xmodel(&mut self, path: &Path) -> Result<String, String> {
        let k = self.cod_k();
        let to_file = self.cod_to_file();
        let w: Worlds = self.scene.world_matrices().into_iter().map(|(id, m)| (id, to_file * m)).collect();
        let mut joints = self.cod_joints();
        let meshes: Vec<Id> = {
            let sel: Vec<Id> = self.sel.iter().copied().filter(|id| self.scene.get(*id).is_some_and(|n| n.kind == Kind::Mesh)).collect();
            if sel.is_empty() {
                self.scene.nodes.iter().filter(|n| n.kind == Kind::Mesh && self.scene.is_visible(n.id)).map(|n| n.id).collect()
            } else {
                sel
            }
        };
        // joints used by skins must be part of the model
        for m in &meshes {
            if let Some(skin) = self.scene.get(*m).and_then(|n| n.skin.as_ref()) {
                for j in &skin.joints {
                    if !joints.contains(j) {
                        joints.push(*j);
                    }
                }
            }
        }
        let mut bones: Vec<XBone> = Vec::new();
        let mut list: Vec<Id> = Vec::new();
        if joints.is_empty() {
            bones.push(XBone { name: "tag_origin".into(), parent: -1, pose: XPose::default() });
        } else {
            // keep parent-before-child order
            let all = joints.clone();
            let mut order = Vec::new();
            fn walk(s: &Scene, id: Id, chosen: &[Id], out: &mut Vec<Id>) {
                if chosen.contains(&id) {
                    out.push(id);
                }
                for c in s.children(Some(id)) {
                    walk(s, c, chosen, out);
                }
            }
            for r in self.scene.children(None) {
                walk(&self.scene, r, &all, &mut order);
            }
            list = order;
            for &id in &list {
                let n = self.scene.get(id).unwrap();
                let bone_name = if self.cod.auto_rename { let pn = n.parent.and_then(|p| self.scene.get(p)).map(|p| p.name.clone()); crate::weapon::auto_rename(&n.name, pn.as_deref(), pn.as_deref().map(|x| crate::weapon::auto_rename(x, None, None)).as_deref()) } else { n.name.clone() };
                bones.push(XBone { name: bone_name, parent: self.cod_parent_index(&list, id), pose: XPose::from_matrix(&w[&id], k) });
            }
        }
        let mut model = XModel { version: self.cod.version, bones, verts: Vec::new(), tris: Vec::new(), objects: Vec::new(), materials: Vec::new() };
        for (oi, mid) in meshes.iter().enumerate() {
            let n = self.scene.get(*mid).unwrap();
            let Some(mesh) = &n.mesh else { continue };
            let base = model.verts.len();
            let verts = self.scene.deformed_verts(n, mesh, &w);
            // which bone does an unskinned mesh follow? its nearest joint ancestor
            let rigid_bone = {
                let mut cur = n.parent;
                let mut b = 0usize;
                while let Some(c) = cur {
                    if let Some(i) = list.iter().position(|x| *x == c) {
                        b = i;
                        break;
                    }
                    cur = self.scene.get(c).and_then(|p| p.parent);
                }
                b
            };
            for (vi, p) in verts.iter().enumerate() {
                let weights: Vec<(usize, f32)> = match &n.skin {
                    Some(skin) if skin.weights.len() == verts.len() => skin.weights[vi]
                        .iter()
                        .filter(|(_, wt)| *wt > 1e-4)
                        .map(|(j, wt)| (skin.joints.get(*j).and_then(|jid| list.iter().position(|x| x == jid)).unwrap_or(0), *wt))
                        .collect(),
                    _ => vec![(rigid_bone, 1.0)],
                };
                model.verts.push(XVert { pos: *p * k, weights });
            }
            let normals = mesh.corner_normals(&verts, 50.0);
            for (fi, f) in mesh.faces.iter().enumerate() {
                for c in 1..f.len().saturating_sub(1) {
                    // the game wants clockwise triangles: write a, c, b
                    let ids = [0, c + 1, c];
                    model.tris.push(XTri {
                        object: oi,
                        material: oi,
                        v: ids.map(|i| base + f[i] as usize),
                        n: ids.map(|i| normals[fi][i]),
                    });
                }
            }
            model.objects.push(n.name.clone());
            model.materials.push(XMat { name: format!("{}_mtl", n.name), color: n.material.color });
        }
        let label = path.display().to_string();
        std::fs::write(path, write_xmodel(&model, &label)).map_err(|e| e.to_string())?;
        Ok(crate::trf!(
            "XModel (Version {}) exportiert: {} Gelenke, {} Objekte, {} Punkte, {} Dreiecke", "XModel (version {}) exported: {} joints, {} objects, {} vertices, {} triangles",
            model.version.clamp(6, 7),
            model.bones.len(),
            model.objects.len(),
            model.verts.len(),
            model.tris.len()
        ))
    }

    pub fn import_xmodel(&mut self, path: &Path) -> Result<String, String> {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let model = parse_xmodel(&text)?;
        self.checkpoint();
        let k = 1.0 / self.cod_k();
        let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or("xmodel".into());
        let gname = self.scene.unique_name(&format!("{stem}_"));
        let mut gnode = Node::new(0, &gname, Kind::Group);
        if self.cod.z_up {
            // game files are Z-up: stand the model upright in Animora's Y-up world
            gnode.r = v3(-90.0, 0.0, 0.0);
        }
        let group = self.scene.add(gnode);
        let worlds: Vec<M4> = model.bones.iter().map(|b| b.pose.matrix(k)).collect();
        // joint display size from the typical bone length
        let mut lens: Vec<f32> = model
            .bones
            .iter()
            .enumerate()
            .filter(|(_, b)| b.parent >= 0 && (b.parent as usize) < worlds.len())
            .map(|(i, b)| (worlds[i].translation() - worlds[b.parent as usize].translation()).len())
            .filter(|l| *l > 1e-4)
            .collect();
        lens.sort_by(|a, b| a.total_cmp(b));
        let radius = lens.get(lens.len() / 2).map(|l| (l * 0.15).max(0.01)).unwrap_or(0.12);
        let mut ids: Vec<Id> = Vec::new();
        for (i, b) in model.bones.iter().enumerate() {
            let parent_ok = b.parent >= 0 && (b.parent as usize) < i;
            let (pid, pw) = if parent_ok { (ids[b.parent as usize], worlds[b.parent as usize]) } else { (group, M4::IDENTITY) };
            let (t, r, _) = (pw.inverse() * worlds[i]).decompose();
            let mut n = Node::new(0, &b.name, Kind::Joint);
            n.parent = Some(pid);
            n.t = t;
            n.r = r;
            n.joint_radius = radius;
            // bind pose, restored before every XAnim import
            n.rest = Some([t, r, V3::ONE]);
            ids.push(self.scene.add(n));
        }
        // meshes, one per object
        let obj_count = model.objects.len().max(model.tris.iter().map(|t| t.object + 1).max().unwrap_or(0));
        let mut mesh_count = 0;
        for o in 0..obj_count {
            let tris: Vec<&XTri> = model.tris.iter().filter(|t| t.object == o).collect();
            if tris.is_empty() {
                continue;
            }
            let mut remap: HashMap<usize, u32> = HashMap::new();
            let mut mesh = Mesh::default();
            let mut src: Vec<usize> = Vec::new();
            for t in &tris {
                let mut f = Vec::new();
                for &vi in &t.v {
                    let idx = *remap.entry(vi).or_insert_with(|| {
                        let p = model.verts.get(vi).map(|v| v.pos * k).unwrap_or_default();
                        mesh.verts.push(p);
                        src.push(vi);
                        mesh.verts.len() as u32 - 1
                    });
                    f.push(idx);
                }
                // orient each triangle by its stored normals
                let a = mesh.verts[f[0] as usize];
                let b = mesh.verts[f[1] as usize];
                let c = mesh.verts[f[2] as usize];
                let geo = (b - a).cross(c - a);
                let given = t.n[0] + t.n[1] + t.n[2];
                if geo.dot(given) < 0.0 {
                    f.swap(1, 2);
                }
                mesh.faces.push(f);
            }
            let name = model.objects.get(o).cloned().unwrap_or_else(|| format!("{stem}_mesh{o}"));
            let name = if self.scene.nodes.iter().any(|n| n.name == name) { self.scene.unique_name(&name) } else { name };
            let mut n = Node::new(0, &name, Kind::Mesh);
            n.parent = Some(group);
            if let Some(m) = model.materials.get(tris[0].material) {
                n.material.color = m.color;
            }
            let weights: Vec<Vec<(usize, f32)>> = src
                .iter()
                .map(|vi| {
                    let ws: Vec<(usize, f32)> = model.verts.get(*vi).map(|v| v.weights.clone()).unwrap_or_default();
                    crate::scene::normalize_weights(ws, ids.len())
                })
                .collect();
            n.skin = Some(Skin { joints: ids.clone(), bind_inv: worlds.iter().map(|m| m.inverse()).collect(), weights, bind_mesh_world: M4::IDENTITY });
            n.mesh = Some(mesh);
            self.scene.add(n);
            mesh_count += 1;
        }
        self.cod.models.push(group);
        self.cod.excluded.remove(&group);
        self.sel = vec![group];
        self.frame_view(false);
        Ok(crate::trf!(
            "XModel „{gname}“ importiert: {} Gelenke, {} Objekte, {} Punkte – weitere Modelle importieren oder die Animation anhängen (Schritt 2)", "XModel \"{gname}\" imported: {} joints, {} objects, {} vertices – import more models or attach the animation (step 2)",
            ids.len(),
            mesh_count,
            model.verts.len()
        ))
    }

    fn cod_pick_save(&self, filter: &str, ext: &str, default: &str) -> Option<std::path::PathBuf> {
        rfd::FileDialog::new().add_filter(filter, &[ext]).set_file_name(default).save_file()
    }

    pub fn cod_menu(&mut self, ui: &mut Ui) {
        ui.menu_button(crate::tr!("CoD-Werkzeuge", "CoD Tools"), |ui| {
            if ui.button(crate::tr!("1. XModels importieren (Arme, Waffe …) …", "1. Import XModels (arms, weapon …) …")).clicked() {
                self.cod_import_xmodel_dialog();
                ui.close();
            }
            let has_model = !self.cod_targets().is_empty();
            if ui
                .add_enabled(has_model, egui::Button::new(crate::tr!("2. Animation an Modelle anhängen …", "2. Attach animation to models …")))
                .on_disabled_hover_text(crate::tr!("Zuerst ein XModel importieren", "Import an XModel first"))
                .clicked()
            {
                self.cod_import_xanim_dialog();
                ui.close();
            }
            if ui.button(crate::tr!("Waffe ans Rig hängen", "Attach weapon to rig")).clicked() {
                self.attach_weapon_dialog();
                ui.close();
            }
            if ui.button(crate::tr!("Import-Fenster …", "Import window …")).clicked() {
                self.open_cod(0);
                ui.close();
            }
            ui.separator();
            if ui.button(crate::tr!("XModel exportieren …", "Export XModel …")).clicked() {
                self.open_cod(1);
                ui.close();
            }
            if ui.button(crate::tr!("XAnim exportieren …", "Export XAnim …")).clicked() {
                self.open_cod(0);
                ui.close();
            }
            ui.separator();
            if ui.button(crate::tr!("Notetracks bearbeiten …", "Edit notetracks …")).clicked() {
                self.open_cod(2);
                ui.close();
            }
        });
    }

    pub fn open_cod(&mut self, tab: usize) {
        if !self.cod.range_init {
            self.cod.start = self.scene.start;
            self.cod.end = self.scene.end;
            self.cod.fps = self.scene.fps;
            self.cod.range_init = true;
        }
        self.cod.tab = tab;
        self.cod.show = true;
    }

    pub fn cod_import_xanim_dialog(&mut self) {
        if self.cod_targets().is_empty() {
            self.cod.message = crate::tr!("Schritt 1 fehlt: Bitte zuerst die XModels importieren (z. B. Arme und Waffe), dann die Animation anhängen.", "Step 1 missing: please import the XModels first (e.g. arms and weapon), then attach the animation.").into();
            self.status = self.cod.message.clone();
            self.open_cod(0);
            return;
        }
        let Some(paths) = rfd::FileDialog::new().add_filter(crate::tr!("Animation (XAnim / Cast)", "Animation (XAnim / Cast)"), &["xanim_export", "XANIM_EXPORT", "cast", "CAST"]).pick_files() else { return };
        let msg = self.import_files(paths);
        self.status = msg.clone();
        self.cod.message = msg;
    }

    pub fn cod_import_xmodel_dialog(&mut self) {
        let Some(paths) = rfd::FileDialog::new().add_filter(crate::tr!("Modell (XModel / Cast)", "Model (XModel / Cast)"), &["xmodel_export", "XMODEL_EXPORT", "cast", "CAST"]).pick_files() else { return };
        let msg = self.import_files(paths);
        self.status = msg.clone();
        self.cod.message = msg;
    }

    pub fn cod_window(&mut self, ctx: &egui::Context) {
        if !self.cod.show {
            return;
        }
        let mut show = true;
        egui::Window::new(crate::tr!("Call-of-Duty-Werkzeuge", "Call of Duty Tools")).open(&mut show).default_width(430.0).resizable(true).show(ctx, |ui| {
            ui.horizontal(|ui| {
                for (i, t) in ["XAnim", "XModel", "Notetracks"].iter().enumerate() {
                    if ui.selectable_label(self.cod.tab == i, *t).clicked() {
                        self.cod.tab = i;
                    }
                }
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(crate::tr!("Szenen-Einheit:", "Scene unit:"));
                ui.radio_value(&mut self.cod.unit_cm, true, crate::tr!("1 = 1 cm (wie Maya)", "1 = 1 cm (like Maya)"));
                ui.radio_value(&mut self.cod.unit_cm, false, crate::tr!("1 = 1 Zoll", "1 = 1 inch"));
            });
            ui.checkbox(&mut self.cod.z_up, crate::tr!("Achsen umrechnen: Spiel (Z oben) ↔ Animora (Y oben)", "Convert axes: game (Z up) ↔ Animora (Y up)"))
                .on_hover_text(crate::tr!("Spieldateien sind Z-oben. Mit Haken stehen importierte Modelle aufrecht und Exporte passen ins Spiel.", "Game files are Z-up. When ticked, imported models stand upright and exports fit the game."));
            ui.add_space(4.0);
            match self.cod.tab {
                0 => self.cod_xanim_tab(ui),
                1 => self.cod_xmodel_tab(ui),
                _ => self.cod_notes_tab(ui),
            }
            if !self.cod.message.is_empty() {
                ui.separator();
                ui.label(egui::RichText::new(&self.cod.message).color(Color32::from_rgb(170, 220, 255)));
            }
        });
        self.cod.show = show;
    }

    fn cod_selection_info(&self, ui: &mut Ui) {
        let joints = self.cod_joints();
        let names: Vec<String> = joints.iter().take(6).filter_map(|id| self.scene.get(*id).map(|n| n.name.clone())).collect();
        let more = if joints.len() > 6 { format!(" … (+{})", joints.len() - 6) } else { String::new() };
        ui.label(crate::trf!("Gelenke ({}): {}{}", "Joints ({}): {}{}", joints.len(), names.join(", "), more));
        ui.label(egui::RichText::new(crate::tr!("Ausgewählte Gelenk-Hierarchien – oder alle Gelenke, wenn nichts gewählt ist.", "Selected joint hierarchies – or all joints when nothing is selected.")).small().color(Color32::from_gray(160)));
    }

    fn cod_xanim_tab(&mut self, ui: &mut Ui) {
        ui.heading(crate::tr!("Animation exportieren (.xanim_export)", "Export animation (.xanim_export)"));
        self.cod_selection_info(ui);
        egui::Grid::new("xanim_opts").num_columns(2).show(ui, |ui| {
            ui.label(crate::tr!("Bilder", "Frames"));
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut self.cod.start).speed(0.5).max_decimals(0));
                ui.label(crate::tr!("bis", "to"));
                ui.add(egui::DragValue::new(&mut self.cod.end).speed(0.5).max_decimals(0));
                if ui.button(crate::tr!("Aus Szene", "From scene")).on_hover_text(crate::tr!("Bildbereich der Zeitleiste übernehmen", "Use the time slider's frame range")).clicked() {
                    self.cod.start = self.scene.start;
                    self.cod.end = self.scene.end;
                    self.cod.fps = self.scene.fps;
                }
            });
            ui.end_row();
            ui.label(crate::tr!("Bilder/s", "FPS"));
            ui.add(egui::DragValue::new(&mut self.cod.fps).speed(0.5).range(1.0..=240.0));
            ui.end_row();
        });
        ui.checkbox(&mut self.cod.reverse, crate::tr!("Rückwärts exportieren", "Export reversed"));
        ui.checkbox(&mut self.cod.auto_rename, crate::tr!("Gelenke automatisch umbenennen (J_GUN → tag_weapon usw.)", "Automatically rename joints (J_GUN → tag_weapon etc.)"));
        ui.checkbox(&mut self.cod.tag_align, crate::tr!("TAG_ALIGN anhängen (für manche Spiel-Animationen nötig)", "Append TAG_ALIGN (needed by some game animations)"));
        ui.checkbox(&mut self.cod.nt_file, crate::tr!("Notetracks als eigene .NT_EXPORT-Datei", "Notetracks as a separate .NT_EXPORT file"));
        ui.horizontal(|ui| {
            if ui.button(crate::tr!("Exportieren …", "Export …")).clicked() {
                if let Some(p) = self.cod_pick_save(crate::tr!("CoD-Animation", "CoD animation"), "xanim_export", crate::tr!("animation.xanim_export", "animation.xanim_export")) {
                    self.cod.message = match self.export_xanim(&p) {
                        Ok(m) => m,
                        Err(e) => crate::trf!("Export fehlgeschlagen: {e}", "Export failed: {e}"),
                    };
                    self.status = self.cod.message.clone();
                }
            }
        });
        ui.separator();
        self.cod_import_steps(ui);
    }

    /// The import workflow: 1. all models (arms, weapon, scope …),
    /// 2. one animation attached to all ticked models.
    fn cod_import_steps(&mut self, ui: &mut Ui) {
        ui.heading(crate::tr!("Modelle + eine Animation", "Models + one animation"));
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(crate::tr!("Schritt 1", "Step 1")).strong());
            if ui.button(crate::tr!("XModel importieren …", "Import XModel …")).on_hover_text(crate::tr!("Mehrmals möglich: Arme, Waffe, Visier …", "Repeat as needed: arms, weapon, scope …")).clicked() {
                self.cod_import_xmodel_dialog();
            }
        });
        let models = self.cod_model_groups();
        if models.is_empty() {
            ui.label(egui::RichText::new(crate::tr!("Noch keine Modelle – importiere z. B. die Arme und die Waffe.", "No models yet – import e.g. the arms and the weapon.")).small().color(Color32::from_rgb(255, 200, 120)));
        } else {
            ui.label(egui::RichText::new(crate::tr!("Modelle in der Szene (Haken = wird von der Animation bewegt):", "Models in the scene (ticked = moved by the animation):")).small());
            egui::Frame::group(ui.style()).show(ui, |ui| {
                for id in &models {
                    let Some(n) = self.scene.get(*id) else { continue };
                    let name = n.name.clone();
                    let own = self.model_nodes(*id);
                    let joints = own.iter().filter(|c| self.scene.get(**c).is_some_and(|m| m.kind == Kind::Joint)).count();
                    let animated = own.iter().any(|c| self.scene.get(*c).is_some_and(|m| m.kind == Kind::Joint && m.is_animated()));
                    let attached_to = n.parent;
                    let mut on = !self.cod.excluded.contains(id);
                    // joints of the other models this one can hang on
                    let candidates: Vec<(Id, String)> = models
                        .iter()
                        .filter(|m| *m != id)
                        .flat_map(|m| {
                            let mname = self.scene.get(*m).map(|x| x.name.clone()).unwrap_or_default();
                            self.model_nodes(*m)
                                .into_iter()
                                .filter(|j| self.scene.get(*j).is_some_and(|x| x.kind == Kind::Joint))
                                .filter(|j| !self.scene.is_ancestor(*id, *j))
                                .map(move |j| (j, mname.clone()))
                                .collect::<Vec<_>>()
                        })
                        .map(|(j, mname)| (j, format!("{mname}: {}", self.scene.get(j).map(|x| x.name.clone()).unwrap_or_default())))
                        .collect();
                    let mut attach: Option<Option<Id>> = None;
                    ui.horizontal(|ui| {
                        let cur = attached_to.and_then(|p| candidates.iter().find(|c| c.0 == p)).map(|c| c.1.clone()).unwrap_or("–".into());
                        egui::ComboBox::from_id_salt(("attach", *id))
                            .width(150.0)
                            .selected_text(cur)
                            .show_ui(ui, |ui| {
                                if ui.selectable_label(attached_to.is_none(), crate::tr!("– (frei)", "– (free)")).clicked() {
                                    attach = Some(None);
                                }
                                for (j, label) in &candidates {
                                    if ui.selectable_label(attached_to == Some(*j), label).clicked() {
                                        attach = Some(Some(*j));
                                    }
                                }
                            })
                            .response
                            .on_hover_text(crate::tr!(
                                "An ein Gelenk eines anderen Modells hängen (z. B. Waffe an die Hand), damit lokale Animationen (Cast) passen",
                                "Attach to a joint of another model (e.g. weapon to the hand) so local animations (Cast) line up"
                            ));
                        if ui.checkbox(&mut on, crate::trf!("{name}  ({joints} Gelenke)", "{name}  ({joints} joints)")).changed() {
                            if on {
                                self.cod.excluded.remove(id);
                            } else {
                                self.cod.excluded.insert(*id);
                            }
                        }
                        if animated {
                            ui.label(egui::RichText::new(crate::tr!("animiert", "animated")).small().color(Color32::from_rgb(255, 160, 150)));
                        }
                    });
                    if let Some(j) = attach {
                        self.checkpoint();
                        self.attach_model(*id, j);
                    }
                }
            });
        }
        let targets = self.cod_targets();
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(crate::tr!("Schritt 2", "Step 2")).strong());
            let label = crate::trf!("Animation an {} Modell(e) anhängen …", "Attach animation to {} model(s) …", targets.len());
            if ui
                .add_enabled(!targets.is_empty(), egui::Button::new(egui::RichText::new(label).strong()))
                .on_hover_text(crate::tr!("Eine .xanim_export-Datei wählen – sie bewegt alle Gelenke aller angehakten Modelle", "Choose an .xanim_export file – it moves all joints of all ticked models"))
                .clicked()
            {
                self.cod_import_xanim_dialog();
            }
        });
        if let Some(last) = self.cod.last_anim.clone() {
            let name = last.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
            if ui
                .add_enabled(!targets.is_empty(), egui::Button::new(crate::trf!("„{name}“ erneut anhängen", "Attach \"{name}\" again")))
                .on_hover_text(crate::tr!("Nützlich, wenn du danach noch ein Modell (z. B. das Visier) importiert hast", "Useful when you imported another model (e.g. the scope) afterwards"))
                .clicked()
            {
                let r = self.import_xanim(&last);
                self.cod.message = match r {
                    Ok(m) => m,
                    Err(e) => crate::trf!("Anhängen fehlgeschlagen: {e}", "Attaching failed: {e}"),
                };
                self.status = self.cod.message.clone();
            }
        }
        if ui
            .button(crate::tr!("Waffe ans Rig hängen", "Attach weapon to rig"))
            .on_hover_text(crate::tr!("Wie in den CoD Maya Tools: j_gun / tag_weapon der Waffe kommt ohne Versatz an tag_weapon_right / tag_weapon der Arme", "Like the CoD Maya tools: the weapon's j_gun / tag_weapon goes onto the arms' tag_weapon_right / tag_weapon with no offset"))
            .clicked()
        {
            self.attach_weapon_dialog();
        }
        ui.checkbox(&mut self.cod.create_missing, crate::tr!("Gelenke, die nur die Animation hat, dem ersten Modell hinzufügen", "Add joints that only the animation has to the first model"));
        ui.checkbox(&mut self.cod.replace_anim, crate::tr!("Vorherige Animation ersetzen (Grundpose wiederherstellen)", "Replace the previous animation (restore the bind pose)"));
        ui.checkbox(&mut self.cod.fit_range, crate::tr!("Zeitleiste und Bilder/s an die Datei anpassen", "Match the time slider and FPS to the file"));
    }

    fn cod_xmodel_tab(&mut self, ui: &mut Ui) {
        ui.heading(crate::tr!("Modell exportieren (.xmodel_export)", "Export model (.xmodel_export)"));
        self.cod_selection_info(ui);
        let meshes = self.sel.iter().filter(|id| self.scene.get(**id).is_some_and(|n| n.kind == Kind::Mesh)).count();
        ui.label(if meshes > 0 { crate::trf!("Objekte: {meshes} ausgewählt", "Objects: {meshes} selected") } else { crate::tr!("Objekte: alle sichtbaren", "Objects: all visible").into() });
        ui.horizontal(|ui| {
            ui.label(crate::tr!("Version", "Version"));
            ui.radio_value(&mut self.cod.version, 6, crate::tr!("6 (ältere Spiele)", "6 (older games)"));
            ui.radio_value(&mut self.cod.version, 7, crate::tr!("7 (Black Ops 3)", "7 (Black Ops 3)"));
        });
        ui.checkbox(&mut self.cod.auto_rename, crate::tr!("Gelenke automatisch umbenennen (J_GUN → tag_weapon usw.)", "Automatically rename joints (J_GUN → tag_weapon etc.)"));
        ui.label(
            egui::RichText::new(crate::tr!("Gebundene Objekte behalten ihre Gewichte; ungebundene hängen am nächsten Gelenk darüber. Am besten in der Grundpose exportieren.", "Skinned objects keep their weights; unskinned ones follow the nearest joint above them. Best exported in the bind pose."))
                .small()
                .color(Color32::from_gray(160)),
        );
        if ui.button(crate::tr!("Exportieren …", "Export …")).clicked() {
            if let Some(p) = self.cod_pick_save(crate::tr!("CoD-Modell", "CoD model"), "xmodel_export", crate::tr!("modell.xmodel_export", "model.xmodel_export")) {
                self.cod.message = match self.export_xmodel(&p) {
                    Ok(m) => m,
                    Err(e) => crate::trf!("Export fehlgeschlagen: {e}", "Export failed: {e}"),
                };
                self.status = self.cod.message.clone();
            }
        }
        ui.separator();
        self.cod_import_steps(ui);
    }

    fn cod_notes_tab(&mut self, ui: &mut Ui) {
        ui.heading("Notetracks");
        ui.label(egui::RichText::new(crate::tr!("Ereignisse wie „end“, „fire“ oder „sndnt#…“ an bestimmten Bildern.", "Events such as \"end\", \"fire\" or \"sndnt#…\" at certain frames.")).small().color(Color32::from_gray(160)));
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.cod.new_note).desired_width(140.0));
            if ui.button(crate::trf!("Hinzufügen bei Bild {}", "Add at frame {}", self.scene.frame as i64)).clicked() {
                let name = clean_note(&self.cod.new_note);
                if !name.is_empty() {
                    self.checkpoint();
                    self.scene.notes.push(Note { frame: self.scene.frame, name });
                    self.scene.notes.sort_by(|a, b| a.frame.total_cmp(&b.frame));
                }
            }
        });
        let mut remove: Option<usize> = None;
        let mut jump: Option<f32> = None;
        egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
            if self.scene.notes.is_empty() {
                ui.label(egui::RichText::new(crate::tr!("Noch keine Notizen.", "No notes yet.")).color(Color32::from_gray(150)));
            }
            egui::Grid::new("notes").num_columns(4).striped(true).show(ui, |ui| {
                for (i, n) in self.scene.notes.iter_mut().enumerate() {
                    ui.add(egui::DragValue::new(&mut n.frame).speed(0.5).max_decimals(0));
                    ui.add(egui::TextEdit::singleline(&mut n.name).desired_width(150.0));
                    if ui.small_button(crate::tr!("Hin", "Go")).on_hover_text(crate::tr!("Zu diesem Bild springen", "Jump to this frame")).clicked() {
                        jump = Some(n.frame);
                    }
                    if ui.small_button("✕").on_hover_text(crate::tr!("Entfernen", "Remove")).clicked() {
                        remove = Some(i);
                    }
                    ui.end_row();
                }
            });
        });
        if let Some(i) = remove {
            self.checkpoint();
            self.scene.notes.remove(i);
        }
        if let Some(f) = jump {
            self.set_frame(f);
        }
        ui.horizontal(|ui| {
            if ui.button(crate::tr!("Alle löschen", "Delete all")).clicked() && !self.scene.notes.is_empty() {
                self.checkpoint();
                self.scene.notes.clear();
            }
            if ui.button(crate::tr!("NT_EXPORT laden …", "Load NT_EXPORT …")).clicked() {
                if let Some(p) = rfd::FileDialog::new().add_filter(crate::tr!("Notetrack", "Notetrack"), &["NT_EXPORT", "nt_export"]).pick_file() {
                    match std::fs::read_to_string(&p) {
                        Ok(t) => {
                            self.checkpoint();
                            let start = self.scene.start;
                            let notes = parse_nt(&t);
                            self.cod.message = crate::trf!("{} Notizen geladen", "{} notes loaded", notes.len());
                            for n in notes {
                                self.scene.notes.push(Note { frame: start + n.frame, name: n.name });
                            }
                            self.scene.notes.sort_by(|a, b| a.frame.total_cmp(&b.frame));
                        }
                        Err(e) => self.cod.message = crate::trf!("Fehler: {e}", "Error: {e}"),
                    }
                }
            }
            if ui.button(crate::tr!("NT_EXPORT speichern …", "Save NT_EXPORT …")).clicked() {
                if let Some(p) = self.cod_pick_save(crate::tr!("Notetrack", "Notetrack"), "NT_EXPORT", "animation.NT_EXPORT") {
                    let start = self.scene.start;
                    let notes: Vec<Note> = self.scene.notes.iter().map(|n| Note { frame: n.frame - start, name: clean_note(&n.name) }).collect();
                    let count = (self.scene.end - start + 1.0).max(1.0) as usize;
                    self.cod.message = match std::fs::write(&p, write_nt(&notes, 0, count)) {
                        Ok(_) => crate::trf!("{} Notizen gespeichert", "{} notes saved", notes.len()),
                        Err(e) => crate::trf!("Fehler: {e}", "Error: {e}"),
                    };
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xanim_roundtrip() {
        let pose = XPose { offset: v3(1.0, 2.0, 3.0), axes: [V3::X, V3::Y, V3::Z] };
        let a = XAnim {
            framerate: 30.0,
            parts: vec!["tag_origin".into(), "j_mainroot".into()],
            frames: vec![vec![pose, pose], vec![pose, pose]],
            notes: vec![Note { frame: 1.0, name: "end".into() }],
        };
        let text = write_xanim(&a, "test", true);
        let b = parse_xanim(&text).unwrap();
        assert_eq!(b.parts, a.parts);
        assert_eq!(b.frames.len(), 2);
        assert!((b.frames[1][1].offset - pose.offset).len() < 1e-5);
        assert_eq!(b.notes.len(), 1);
        assert_eq!(b.notes[0].name, "end");
    }

    /// Full path through the app: export the demo rig's model and animation,
    /// import both into a fresh scene and compare joint positions per frame.
    #[test]
    fn app_export_then_import() {
        let dir = std::env::temp_dir().join("animora_cod_test");
        let _ = std::fs::create_dir_all(&dir);
        let ctx = egui::Context::default();
        let mut a = App::new(&ctx, None);
        let sh = a.scene.nodes.iter().find(|n| n.name == "shoulder").unwrap().id;
        let arm = a.scene.nodes.iter().find(|n| n.name == "arm").unwrap().id;
        a.sel = vec![sh, arm];
        a.cod.start = 1.0;
        a.cod.end = 40.0;
        let xm = dir.join("arm.xmodel_export");
        let xa = dir.join("arm.xanim_export");
        a.export_xmodel(&xm).unwrap();
        a.sel = vec![sh];
        a.export_xanim(&xa).unwrap();
        // expected world positions of the wrist and the arm mesh per frame
        let wr = a.scene.nodes.iter().find(|n| n.name == "wrist").unwrap().id;
        let centroid = |s: &Scene, id: Id| {
            let w = s.world_matrices();
            let n = s.get(id).unwrap();
            let v = s.deformed_verts(n, n.mesh.as_ref().unwrap(), &w);
            v.iter().fold(V3::ZERO, |acc, p| acc + *p) / v.len() as f32
        };
        let mut expect = Vec::new();
        let mut s = a.scene.clone();
        for f in 1..=40 {
            s.evaluate(f as f32);
            expect.push((s.world_matrices()[&wr].translation(), centroid(&s, arm)));
        }

        let mut b = App::new(&ctx, None);
        b.scene = Scene::default();
        b.sel.clear();
        // step 2 without step 1 is refused
        assert!(b.import_xanim(&xa).is_err());
        b.import_xmodel(&xm).unwrap();
        let group = b.cod.models[0];
        assert_eq!(b.scene.get(group).unwrap().r.x, -90.0, "Z-up model stands upright");
        let msg = b.import_xanim(&xa).unwrap();
        assert!(msg.contains("arm_1 (3)"), "{msg}");
        let wr2 = b.scene.nodes.iter().find(|n| n.name == "wrist").unwrap().id;
        let arm2 = b.scene.nodes.iter().find(|n| n.kind == Kind::Mesh).unwrap().id;
        for (i, (e, c)) in expect.iter().enumerate() {
            b.scene.evaluate(1.0 + i as f32);
            let got = b.scene.world_matrices()[&wr2].translation();
            assert!((got - *e).len() < 2e-3, "frame {} got {:?} want {:?}", i + 1, got, e);
            // the model's mesh follows the animated joints
            let gc = centroid(&b.scene, arm2);
            assert!((gc - *c).len() < 5e-3, "mesh frame {} got {:?} want {:?}", i + 1, gc, c);
        }
        assert_eq!(b.scene.notes.len(), 2, "notes inside frames 1-40");

        // an anim with a joint the model lacks: it is added inside the model;
        // a model joint the anim lacks follows its animated parent
        let mut extra = Node::new(0, "j_extra", Kind::Joint);
        extra.parent = Some(wr);
        extra.t = v3(0.0, 0.5, 0.0);
        let ex = a.scene.add(extra);
        let el = a.scene.nodes.iter().find(|n| n.name == "elbow").unwrap().id;
        let xa2 = dir.join("arm2.xanim_export");
        a.sel = vec![sh];
        // leave the elbow out of this anim by renaming it for the export
        a.scene.get_mut(el).unwrap().name = "x_elbow".into();
        a.export_xanim(&xa2).unwrap();
        let msg = b.import_xanim(&xa2).unwrap();
        assert!(msg.contains("1 model joints follow"), "{msg}");
        let ex2 = b.scene.nodes.iter().find(|n| n.name == "j_extra").unwrap();
        assert_eq!(ex2.parent, Some(group));
        let ex2 = ex2.id;
        // IK depends on the previous frame: step through like the export did
        let mut s = a.scene.clone();
        for f in 1..=20 {
            s.evaluate(f as f32);
        }
        b.scene.evaluate(20.0);
        let (wa, wb) = (s.world_matrices(), b.scene.world_matrices());
        assert!((wa[&ex].translation() - wb[&ex2].translation()).len() < 2e-3, "extra a {:?} b {:?} msg {msg}", wa[&ex].translation(), wb[&ex2].translation());
        assert!((wa[&wr].translation() - wb[&wr2].translation()).len() < 2e-3);
    }

    /// Viewmodel workflow: arms model + weapon model imported, then one anim
    /// moves the joints of both; the weapon mesh follows its gun joint.
    #[test]
    fn one_anim_drives_arms_and_weapon() {
        let dir = std::env::temp_dir().join("animora_cod_multi");
        let _ = std::fs::create_dir_all(&dir);
        let ctx = egui::Context::default();
        let mut a = App::new(&ctx, None);
        let sh = a.scene.nodes.iter().find(|n| n.name == "shoulder").unwrap().id;
        let arm = a.scene.nodes.iter().find(|n| n.name == "arm").unwrap().id;
        let wr = a.scene.nodes.iter().find(|n| n.name == "wrist").unwrap().id;
        a.cod.start = 1.0;
        a.cod.end = 30.0;
        let arms_file = dir.join("arms.xmodel_export");
        a.sel = vec![sh, arm];
        a.export_xmodel(&arms_file).unwrap();
        // the weapon: a gun joint held in the hand, with a box on it
        let mut gun = Node::new(0, "j_gun", Kind::Joint);
        gun.parent = Some(wr);
        gun.t = v3(0.3, 0.2, 0.0);
        gun.r = v3(0.0, 0.0, 30.0);
        let gun = a.scene.add(gun);
        let mut body = Node::new(0, "waffe", Kind::Mesh);
        body.parent = Some(gun);
        body.t = v3(0.4, 0.0, 0.0);
        body.mesh = Some(Mesh::cube(0.3));
        let body = a.scene.add(body);
        let weapon_file = dir.join("weapon.xmodel_export");
        a.sel = vec![gun, body];
        a.export_xmodel(&weapon_file).unwrap();
        // one animation for all joints
        let anim_file = dir.join("reload.xanim_export");
        a.sel = vec![sh];
        a.export_xanim(&anim_file).unwrap();
        let centroid = |s: &Scene, id: Id| {
            let w = s.world_matrices();
            let n = s.get(id).unwrap();
            let v = s.deformed_verts(n, n.mesh.as_ref().unwrap(), &w);
            v.iter().fold(V3::ZERO, |acc, p| acc + *p) / v.len() as f32
        };
        let mut expect = Vec::new();
        let mut s = a.scene.clone();
        for f in 1..=30 {
            s.evaluate(f as f32);
            expect.push((centroid(&s, arm), centroid(&s, body)));
        }

        let mut b = App::new(&ctx, None);
        b.scene = Scene::default();
        b.sel.clear();
        b.import_xmodel(&arms_file).unwrap();
        b.import_xmodel(&weapon_file).unwrap();
        assert_eq!(b.cod_targets().len(), 2);
        let msg = b.import_xanim(&anim_file).unwrap();
        assert!(msg.contains("to 2 model(s)"), "{msg}");
        assert!(msg.contains("arms_1 (3)") && msg.contains("weapon_1 (1)"), "{msg}");
        let meshes: Vec<Id> = b.scene.nodes.iter().filter(|n| n.kind == Kind::Mesh).map(|n| n.id).collect();
        assert_eq!(meshes.len(), 2);
        for (i, (ea, eb)) in expect.iter().enumerate() {
            b.scene.evaluate(1.0 + i as f32);
            let ga = centroid(&b.scene, meshes[0]);
            let gb = centroid(&b.scene, meshes[1]);
            assert!((ga - *ea).len() < 5e-3, "arms frame {} got {:?} want {:?}", i + 1, ga, ea);
            assert!((gb - *eb).len() < 5e-3, "weapon frame {} got {:?} want {:?}", i + 1, gb, eb);
        }
        // a weapon attached to the hand still gets the same world motion
        let weapon_group = b.cod_model_groups()[1];
        let arms_group = b.cod_model_groups()[0];
        let wrist_in_arms = b.model_nodes(arms_group).into_iter().find(|id| b.scene.get(*id).is_some_and(|n| n.name == "wrist")).unwrap();
        b.attach_model(weapon_group, Some(wrist_in_arms));
        assert_eq!(b.cod_model_groups().len(), 2, "attached model is still listed");
        let msg = b.import_xanim(&anim_file).unwrap();
        assert!(msg.contains("weapon_1 (1)"), "{msg}");
        for (i, (_, eb)) in expect.iter().enumerate() {
            b.scene.evaluate(1.0 + i as f32);
            let gb = centroid(&b.scene, meshes[1]);
            assert!((gb - *eb).len() < 5e-3, "attached weapon frame {} got {:?} want {:?}", i + 1, gb, eb);
        }
        // unticking a model keeps it out of the next attachment
        b.cod.excluded.insert(weapon_group);
        let msg = b.import_xanim(&anim_file).unwrap();
        assert!(msg.contains("to 1 model(s)"), "{msg}");
    }

    /// Joint poses of the demo rig survive world→file→world→local.
    #[test]
    fn pose_roundtrip_through_file_space() {
        let mut s = crate::scene::demo_scene();
        s.evaluate(30.0);
        let w = s.world_matrices();
        for n in s.nodes.iter().filter(|n| n.kind == Kind::Joint) {
            let pose = XPose::from_matrix(&w[&n.id], CM_TO_INCH);
            let back = pose.matrix(1.0 / CM_TO_INCH);
            let pw = s.parent_world(n.id, &w);
            let (t, r, _) = (pw.inverse() * back).decompose();
            let rebuilt = pw * M4::trs(t, r, V3::ONE);
            for c in 0..4 {
                assert!((rebuilt.col(c) - w[&n.id].col(c)).len() < 1e-3, "{} col {c}", n.name);
            }
        }
    }

    #[test]
    fn xmodel_roundtrip() {
        let m = XModel {
            version: 6,
            bones: vec![
                XBone { name: "tag_origin".into(), parent: -1, pose: XPose::default() },
                XBone { name: "j_a".into(), parent: 0, pose: XPose { offset: v3(0.0, 5.0, 0.0), axes: [V3::X, V3::Y, V3::Z] } },
            ],
            verts: vec![
                XVert { pos: v3(0.0, 0.0, 0.0), weights: vec![(0, 1.0)] },
                XVert { pos: v3(1.0, 0.0, 0.0), weights: vec![(1, 0.5), (0, 0.5)] },
                XVert { pos: v3(0.0, 1.0, 0.0), weights: vec![(1, 1.0)] },
            ],
            tris: vec![XTri { object: 0, material: 0, v: [0, 2, 1], n: [V3::Z; 3] }],
            objects: vec!["mesh".into()],
            materials: vec![XMat { name: "mtl".into(), color: [0.5, 0.2, 0.1] }],
        };
        let text = write_xmodel(&m, "test");
        let b = parse_xmodel(&text).unwrap();
        assert_eq!(b.bones.len(), 2);
        assert_eq!(b.bones[1].parent, 0);
        assert_eq!(b.bones[1].name, "j_a");
        assert_eq!(b.verts.len(), 3);
        assert_eq!(b.verts[1].weights.len(), 2);
        assert_eq!(b.tris.len(), 1);
        assert_eq!(b.tris[0].v, [0, 2, 1]);
        assert_eq!(b.objects, vec!["mesh".to_string()]);
        assert!((b.materials[0].color[0] - 0.5).abs() < 1e-5);
    }
}
