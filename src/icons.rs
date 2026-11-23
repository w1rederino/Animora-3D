//! Small vector icons drawn with the painter (no image files needed).

use crate::mesh::Prim;
use crate::scene::Kind;
use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Select,
    Move,
    Rotate,
    Scale,
    Cube,
    Sphere,
    Cylinder,
    Cone,
    Plane,
    Torus,
    Joint,
    Ik,
    Bind,
    Unbind,
    Light,
    PointLight,
    Camera,
    Key,
    Play,
    Pause,
    Render,
    Sequence,
    Extrude,
    Smooth,
    Delete,
    Freeze,
    Group,
    Single,
    Quad,
    Mesh,
    ToStart,
    ToEnd,
    PrevKey,
    NextKey,
    Parent,
    Trail,
}

pub fn prim_icon(p: Prim) -> Icon {
    match p {
        Prim::Cube => Icon::Cube,
        Prim::Sphere => Icon::Sphere,
        Prim::Cylinder => Icon::Cylinder,
        Prim::Cone => Icon::Cone,
        Prim::Plane => Icon::Plane,
        Prim::Torus => Icon::Torus,
    }
}

pub fn kind_icon(k: Kind) -> Icon {
    match k {
        Kind::Mesh => Icon::Mesh,
        Kind::Joint => Icon::Joint,
        Kind::Light => Icon::Light,
        Kind::Camera => Icon::Camera,
        Kind::Group => Icon::Group,
        Kind::IkHandle => Icon::Ik,
    }
}

fn poly(p: &egui::Painter, pts: Vec<Pos2>, col: Color32) {
    p.add(egui::Shape::convex_polygon(pts, col, Stroke::NONE));
}

/// Draws a small vector icon inside `r`.
pub fn draw_icon(p: &egui::Painter, r: Rect, icon: Icon, col: Color32) {
    let c = r.center();
    let s = r.width().min(r.height()) * 0.5;
    let q = |x: f32, y: f32| c + vec2(x, y) * s;
    let st = Stroke::new((s * 0.13).max(1.2), col);
    let blue = Color32::from_rgb(110, 170, 235);
    let fill = Color32::from_rgb(150, 160, 175);
    let red = Color32::from_rgb(225, 60, 60);
    match icon {
        Icon::Select => poly(p, vec![q(-0.45, -0.7), q(0.45, 0.15), q(0.0, 0.2), q(-0.2, 0.75)], col),
        Icon::Move => {
            for d in [vec2(1.0, 0.0), vec2(-1.0, 0.0), vec2(0.0, 1.0), vec2(0.0, -1.0)] {
                p.line_segment([c, c + d * s * 0.75], st);
                let perp = vec2(-d.y, d.x);
                poly(p, vec![c + d * s * 0.95, c + d * s * 0.6 + perp * s * 0.22, c + d * s * 0.6 - perp * s * 0.22], col);
            }
        }
        Icon::Rotate => {
            let pts: Vec<Pos2> = (0..=20)
                .map(|i| {
                    let a = 0.4 + i as f32 / 20.0 * 5.0;
                    c + vec2(a.cos(), a.sin()) * s * 0.7
                })
                .collect();
            let end = *pts.last().unwrap();
            p.add(egui::Shape::line(pts, st));
            p.circle_filled(end, s * 0.18, col);
        }
        Icon::Scale => {
            p.rect_stroke(Rect::from_center_size(c, vec2(s * 1.2, s * 1.2)), 1.0, st, egui::StrokeKind::Middle);
            p.rect_filled(Rect::from_center_size(q(0.6, -0.6), vec2(s * 0.5, s * 0.5)), 1.0, col);
        }
        Icon::Cube | Icon::Mesh => {
            let k = if icon == Icon::Mesh { 0.75 } else { 1.0 };
            let qq = |x: f32, y: f32| c + vec2(x, y) * s * k;
            poly(p, vec![qq(0.0, -0.8), qq(0.7, -0.4), qq(0.0, 0.0), qq(-0.7, -0.4)], Color32::from_gray(215));
            poly(p, vec![qq(-0.7, -0.4), qq(0.0, 0.0), qq(0.0, 0.8), qq(-0.7, 0.4)], Color32::from_gray(150));
            poly(p, vec![qq(0.0, 0.0), qq(0.7, -0.4), qq(0.7, 0.4), qq(0.0, 0.8)], Color32::from_gray(110));
        }
        Icon::Sphere => {
            p.circle_filled(c, s * 0.75, Color32::from_gray(130));
            p.circle_filled(q(-0.2, -0.2), s * 0.45, Color32::from_gray(175));
            p.circle_filled(q(-0.3, -0.3), s * 0.18, Color32::from_gray(230));
        }
        Icon::Cylinder => {
            p.rect_filled(Rect::from_center_size(c, vec2(s * 1.1, s * 1.1)), 0.0, Color32::from_gray(140));
            p.add(egui::Shape::ellipse_filled(q(0.0, 0.55), vec2(0.55, 0.22) * s, Color32::from_gray(140)));
            p.add(egui::Shape::ellipse_filled(q(0.0, -0.55), vec2(0.55, 0.22) * s, Color32::from_gray(210)));
        }
        Icon::Cone => {
            p.add(egui::Shape::ellipse_filled(q(0.0, 0.55), vec2(0.6, 0.22) * s, Color32::from_gray(120)));
            poly(p, vec![q(0.0, -0.8), q(0.6, 0.55), q(-0.6, 0.55)], Color32::from_gray(165));
        }
        Icon::Plane => {
            p.add(egui::Shape::convex_polygon(
                vec![q(-0.35, -0.4), q(0.85, -0.4), q(0.35, 0.4), q(-0.85, 0.4)],
                Color32::from_gray(170),
                Stroke::new(1.0, Color32::from_gray(90)),
            ));
        }
        Icon::Torus => {
            p.add(egui::Shape::ellipse_stroke(c, vec2(0.6, 0.38) * s, Stroke::new(s * 0.32, Color32::from_gray(165))));
        }
        Icon::Joint => {
            p.circle_stroke(q(-0.35, 0.35), s * 0.3, Stroke::new(1.5, blue));
            p.line_segment([q(-0.15, 0.15), q(0.6, -0.6)], Stroke::new(1.5, blue));
            p.circle_stroke(q(0.6, -0.6), s * 0.15, Stroke::new(1.5, blue));
        }
        Icon::Ik => {
            let pc = Color32::from_rgb(220, 140, 255);
            p.line_segment([q(-0.7, 0.6), q(0.4, -0.4)], Stroke::new(1.3, pc));
            p.line_segment([q(0.4, -0.75), q(0.4, -0.05)], Stroke::new(1.6, pc));
            p.line_segment([q(0.05, -0.4), q(0.75, -0.4)], Stroke::new(1.6, pc));
            p.circle_stroke(q(-0.7, 0.6), s * 0.18, Stroke::new(1.3, blue));
        }
        Icon::Bind | Icon::Unbind => {
            p.circle_filled(c, s * 0.65, Color32::from_rgb(200, 170, 140));
            p.line_segment([q(0.0, -0.8), q(0.0, 0.8)], Stroke::new(1.5, blue));
            p.circle_stroke(c, s * 0.15, Stroke::new(1.5, blue));
            if icon == Icon::Unbind {
                p.line_segment([q(-0.7, -0.7), q(0.7, 0.7)], Stroke::new(2.0, red));
            }
        }
        Icon::Light | Icon::PointLight => {
            let y = Color32::from_rgb(255, 220, 100);
            p.circle_filled(c, s * 0.35, y);
            let n = if icon == Icon::Light { 8 } else { 4 };
            for i in 0..n {
                let a = i as f32 * std::f32::consts::TAU / n as f32;
                let d = vec2(a.cos(), a.sin());
                p.line_segment([c + d * s * 0.5, c + d * s * 0.85], Stroke::new(1.5, y));
            }
        }
        Icon::Camera => {
            p.rect_filled(Rect::from_center_size(q(-0.15, 0.0), vec2(s * 1.0, s * 0.8)), 2.0, Color32::from_gray(170));
            poly(p, vec![q(0.35, 0.0), q(0.85, -0.4), q(0.85, 0.4)], Color32::from_gray(170));
        }
        Icon::Key => poly(p, vec![q(0.0, -0.7), q(0.6, 0.0), q(0.0, 0.7), q(-0.6, 0.0)], red),
        Icon::Play => poly(p, vec![q(-0.45, -0.6), q(0.6, 0.0), q(-0.45, 0.6)], col),
        Icon::Pause => {
            p.rect_filled(Rect::from_center_size(q(-0.25, 0.0), vec2(s * 0.3, s * 1.1)), 0.0, col);
            p.rect_filled(Rect::from_center_size(q(0.25, 0.0), vec2(s * 0.3, s * 1.1)), 0.0, col);
        }
        Icon::ToStart | Icon::ToEnd | Icon::PrevKey | Icon::NextKey => {
            let dir = if matches!(icon, Icon::ToEnd | Icon::NextKey) { 1.0 } else { -1.0 };
            let tc = if matches!(icon, Icon::PrevKey | Icon::NextKey) { Color32::from_rgb(230, 90, 90) } else { col };
            poly(p, vec![q(-0.4 * dir, -0.55), q(0.4 * dir, 0.0), q(-0.4 * dir, 0.55)], tc);
            p.line_segment([q(0.55 * dir, -0.6), q(0.55 * dir, 0.6)], Stroke::new(2.0, col));
        }
        Icon::Render | Icon::Sequence => {
            let rr = Rect::from_center_size(q(0.0, 0.15), vec2(s * 1.4, s * 0.9));
            p.rect_filled(rr, 2.0, Color32::from_gray(60));
            p.rect_stroke(rr, 2.0, Stroke::new(1.0, col), egui::StrokeKind::Middle);
            p.circle_filled(q(0.0, 0.15), s * 0.25, Color32::from_rgb(240, 170, 60));
            if icon == Icon::Sequence {
                for i in 0..4 {
                    let x = -0.6 + i as f32 * 0.4;
                    p.rect_filled(Rect::from_center_size(q(x, -0.55), vec2(s * 0.2, s * 0.15)), 0.0, col);
                }
            } else {
                p.line_segment([q(-0.7, -0.5), q(0.7, -0.5)], Stroke::new(2.0, col));
            }
        }
        Icon::Extrude => {
            let o = Color32::from_rgb(255, 170, 60);
            p.rect_filled(Rect::from_center_size(q(0.0, 0.4), vec2(s * 1.3, s * 0.5)), 0.0, fill);
            p.line_segment([q(0.0, 0.2), q(0.0, -0.6)], Stroke::new(2.0, o));
            poly(p, vec![q(0.0, -0.9), q(0.3, -0.5), q(-0.3, -0.5)], o);
        }
        Icon::Smooth => {
            p.rect_stroke(Rect::from_center_size(c, vec2(s * 1.4, s * 1.4)), 0.0, Stroke::new(1.0, Color32::from_gray(140)), egui::StrokeKind::Middle);
            p.circle_filled(c, s * 0.6, fill);
        }
        Icon::Delete => {
            let r = Color32::from_rgb(230, 80, 70);
            p.line_segment([q(-0.6, -0.6), q(0.6, 0.6)], Stroke::new(2.5, r));
            p.line_segment([q(0.6, -0.6), q(-0.6, 0.6)], Stroke::new(2.5, r));
        }
        Icon::Freeze => {
            let b = Color32::from_rgb(160, 210, 255);
            for i in 0..3 {
                let a = i as f32 * std::f32::consts::PI / 3.0;
                let d = vec2(a.cos(), a.sin()) * s * 0.8;
                p.line_segment([c - d, c + d], Stroke::new(1.6, b));
            }
        }
        Icon::Group => {
            p.rect_stroke(Rect::from_center_size(c, vec2(s * 1.4, s * 1.2)), 2.0, Stroke::new(1.3, col), egui::StrokeKind::Middle);
            p.rect_filled(Rect::from_center_size(q(-0.3, 0.0), vec2(s * 0.4, s * 0.4)), 0.0, fill);
            p.rect_filled(Rect::from_center_size(q(0.3, 0.0), vec2(s * 0.4, s * 0.4)), 0.0, fill);
        }
        Icon::Single => {
            p.rect_stroke(Rect::from_center_size(c, vec2(s * 1.5, s * 1.2)), 1.0, st, egui::StrokeKind::Middle);
        }
        Icon::Quad => {
            let rr = Rect::from_center_size(c, vec2(s * 1.5, s * 1.2));
            p.rect_stroke(rr, 1.0, st, egui::StrokeKind::Middle);
            p.line_segment([pos2(c.x, rr.top()), pos2(c.x, rr.bottom())], st);
            p.line_segment([pos2(rr.left(), c.y), pos2(rr.right(), c.y)], st);
        }
        Icon::Parent => {
            p.circle_filled(q(0.0, -0.5), s * 0.25, col);
            p.circle_filled(q(-0.5, 0.5), s * 0.22, fill);
            p.circle_filled(q(0.5, 0.5), s * 0.22, fill);
            p.line_segment([q(0.0, -0.5), q(-0.5, 0.5)], Stroke::new(1.2, col));
            p.line_segment([q(0.0, -0.5), q(0.5, 0.5)], Stroke::new(1.2, col));
        }
        Icon::Trail => {
            let pts: Vec<Pos2> = (0..=12)
                .map(|i| {
                    let t = i as f32 / 12.0;
                    q(-0.8 + 1.6 * t, 0.5 - (t * std::f32::consts::PI).sin())
                })
                .collect();
            for d in pts.iter().step_by(3) {
                p.circle_filled(*d, s * 0.12, Color32::from_rgb(255, 90, 60));
            }
            p.add(egui::Shape::line(pts, Stroke::new(1.5, Color32::from_rgb(255, 180, 90))));
        }
    }
}

/// A square button showing an icon.
pub fn icon_button(ui: &mut Ui, icon: Icon, size: f32, active: bool, tip: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click());
    let bg = if active {
        Color32::from_rgb(82, 133, 166)
    } else if resp.hovered() {
        Color32::from_gray(105)
    } else {
        Color32::from_gray(78)
    };
    ui.painter().rect_filled(rect, 3.0, bg);
    draw_icon(ui.painter(), rect.shrink(size * 0.18), icon, Color32::from_gray(235));
    resp.on_hover_text(tip)
}

/// A shelf tile: icon with a short label.
pub fn shelf_button(ui: &mut Ui, icon: Icon, label: &str, tip: &str) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(58.0, 46.0), Sense::click());
    let bg = if resp.is_pointer_button_down_on() {
        Color32::from_rgb(82, 133, 166)
    } else if resp.hovered() {
        Color32::from_gray(98)
    } else {
        Color32::from_gray(76)
    };
    ui.painter().rect_filled(rect, 4.0, bg);
    draw_icon(ui.painter(), Rect::from_center_size(rect.center() - vec2(0.0, 7.0), vec2(26.0, 26.0)), icon, Color32::from_gray(235));
    ui.painter().text(pos2(rect.center().x, rect.bottom() - 7.0), egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(10.0), Color32::from_gray(225));
    resp.on_hover_text(tip).clicked()
}

pub fn nice_step(range: f32, px: f32, target_px: f32) -> f32 {
    let raw = (range * target_px / px.max(1.0)).max(1e-6);
    let mag = 10f32.powf(raw.log10().floor());
    let n = raw / mag;
    let k = if n < 1.5 {
        1.0
    } else if n < 3.5 {
        2.0
    } else if n < 7.5 {
        5.0
    } else {
        10.0
    };
    k * mag
}
