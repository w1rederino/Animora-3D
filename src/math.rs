//! Small 3D math library: vectors and 4x4 matrices (column vectors, row-major storage).

use serde::{Deserialize, Serialize};
use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct V3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

pub const fn v3(x: f32, y: f32, z: f32) -> V3 {
    V3 { x, y, z }
}

impl V3 {
    pub const ZERO: V3 = v3(0.0, 0.0, 0.0);
    pub const ONE: V3 = v3(1.0, 1.0, 1.0);
    pub const X: V3 = v3(1.0, 0.0, 0.0);
    pub const Y: V3 = v3(0.0, 1.0, 0.0);
    pub const Z: V3 = v3(0.0, 0.0, 1.0);

    pub fn dot(self, o: V3) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }
    pub fn cross(self, o: V3) -> V3 {
        v3(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }
    pub fn len(self) -> f32 {
        self.dot(self).sqrt()
    }
    pub fn norm(self) -> V3 {
        let l = self.len();
        if l > 1e-9 { self / l } else { V3::ZERO }
    }
    pub fn lerp(self, o: V3, t: f32) -> V3 {
        self + (o - self) * t
    }
    pub fn mul_e(self, o: V3) -> V3 {
        v3(self.x * o.x, self.y * o.y, self.z * o.z)
    }
    pub fn get(self, i: usize) -> f32 {
        match i {
            0 => self.x,
            1 => self.y,
            _ => self.z,
        }
    }
    pub fn set(&mut self, i: usize, v: f32) {
        match i {
            0 => self.x = v,
            1 => self.y = v,
            _ => self.z = v,
        }
    }
    pub fn axis(i: usize) -> V3 {
        match i {
            0 => V3::X,
            1 => V3::Y,
            _ => V3::Z,
        }
    }
    pub fn max_abs_axis(self) -> usize {
        let a = [self.x.abs(), self.y.abs(), self.z.abs()];
        if a[0] >= a[1] && a[0] >= a[2] {
            0
        } else if a[1] >= a[2] {
            1
        } else {
            2
        }
    }
}

impl Add for V3 {
    type Output = V3;
    fn add(self, o: V3) -> V3 {
        v3(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}
impl AddAssign for V3 {
    fn add_assign(&mut self, o: V3) {
        *self = *self + o;
    }
}
impl Sub for V3 {
    type Output = V3;
    fn sub(self, o: V3) -> V3 {
        v3(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}
impl SubAssign for V3 {
    fn sub_assign(&mut self, o: V3) {
        *self = *self - o;
    }
}
impl Mul<f32> for V3 {
    type Output = V3;
    fn mul(self, s: f32) -> V3 {
        v3(self.x * s, self.y * s, self.z * s)
    }
}
impl Div<f32> for V3 {
    type Output = V3;
    fn div(self, s: f32) -> V3 {
        v3(self.x / s, self.y / s, self.z / s)
    }
}
impl Neg for V3 {
    type Output = V3;
    fn neg(self) -> V3 {
        v3(-self.x, -self.y, -self.z)
    }
}

/// 4x4 matrix, `m[row][col]`, applied to column vectors: p' = M * p.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct M4 {
    pub m: [[f32; 4]; 4],
}

impl Default for M4 {
    fn default() -> Self {
        M4::IDENTITY
    }
}

impl M4 {
    pub const IDENTITY: M4 = M4 {
        m: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
    };

    pub fn translate(t: V3) -> M4 {
        let mut r = M4::IDENTITY;
        r.m[0][3] = t.x;
        r.m[1][3] = t.y;
        r.m[2][3] = t.z;
        r
    }
    pub fn scale(s: V3) -> M4 {
        let mut r = M4::IDENTITY;
        r.m[0][0] = s.x;
        r.m[1][1] = s.y;
        r.m[2][2] = s.z;
        r
    }
    pub fn rot_x(a: f32) -> M4 {
        let (s, c) = a.sin_cos();
        let mut r = M4::IDENTITY;
        r.m[1][1] = c;
        r.m[1][2] = -s;
        r.m[2][1] = s;
        r.m[2][2] = c;
        r
    }
    pub fn rot_y(a: f32) -> M4 {
        let (s, c) = a.sin_cos();
        let mut r = M4::IDENTITY;
        r.m[0][0] = c;
        r.m[0][2] = s;
        r.m[2][0] = -s;
        r.m[2][2] = c;
        r
    }
    pub fn rot_z(a: f32) -> M4 {
        let (s, c) = a.sin_cos();
        let mut r = M4::IDENTITY;
        r.m[0][0] = c;
        r.m[0][1] = -s;
        r.m[1][0] = s;
        r.m[1][1] = c;
        r
    }
    /// Rotation in degrees, order X then Y then Z (R = Rz * Ry * Rx).
    pub fn euler_deg(r: V3) -> M4 {
        M4::rot_z(r.z.to_radians()) * M4::rot_y(r.y.to_radians()) * M4::rot_x(r.x.to_radians())
    }
    pub fn trs(t: V3, r: V3, s: V3) -> M4 {
        M4::translate(t) * M4::euler_deg(r) * M4::scale(s)
    }
    /// Rotation about a unit axis (radians).
    pub fn axis_angle(axis: V3, a: f32) -> M4 {
        let (s, c) = a.sin_cos();
        let t = 1.0 - c;
        let (x, y, z) = (axis.x, axis.y, axis.z);
        M4 {
            m: [
                [t * x * x + c, t * x * y - s * z, t * x * z + s * y, 0.0],
                [t * x * y + s * z, t * y * y + c, t * y * z - s * x, 0.0],
                [t * x * z - s * y, t * y * z + s * x, t * z * z + c, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    pub fn point(&self, p: V3) -> V3 {
        let m = &self.m;
        v3(
            m[0][0] * p.x + m[0][1] * p.y + m[0][2] * p.z + m[0][3],
            m[1][0] * p.x + m[1][1] * p.y + m[1][2] * p.z + m[1][3],
            m[2][0] * p.x + m[2][1] * p.y + m[2][2] * p.z + m[2][3],
        )
    }
    pub fn vector(&self, p: V3) -> V3 {
        let m = &self.m;
        v3(
            m[0][0] * p.x + m[0][1] * p.y + m[0][2] * p.z,
            m[1][0] * p.x + m[1][1] * p.y + m[1][2] * p.z,
            m[2][0] * p.x + m[2][1] * p.y + m[2][2] * p.z,
        )
    }
    pub fn col(&self, c: usize) -> V3 {
        v3(self.m[0][c], self.m[1][c], self.m[2][c])
    }
    pub fn set_col(&mut self, c: usize, v: V3) {
        self.m[0][c] = v.x;
        self.m[1][c] = v.y;
        self.m[2][c] = v.z;
    }
    pub fn translation(&self) -> V3 {
        self.col(3)
    }

    /// Builds a camera-to-world matrix looking from `eye` to `target`.
    pub fn look_at(eye: V3, target: V3, up: V3) -> M4 {
        let z = (eye - target).norm();
        let mut x = up.cross(z).norm();
        if x.len() < 1e-6 {
            x = V3::X;
        }
        let y = z.cross(x);
        let mut r = M4::IDENTITY;
        r.set_col(0, x);
        r.set_col(1, y);
        r.set_col(2, z);
        r.set_col(3, eye);
        r
    }

    pub fn inverse(&self) -> M4 {
        let a = &self.m;
        let mut inv = [[0.0f32; 4]; 4];
        // Gauss-Jordan elimination
        let mut m = *a;
        for i in 0..4 {
            inv[i][i] = 1.0;
        }
        for c in 0..4 {
            let mut piv = c;
            for r in c + 1..4 {
                if m[r][c].abs() > m[piv][c].abs() {
                    piv = r;
                }
            }
            if m[piv][c].abs() < 1e-12 {
                return M4::IDENTITY;
            }
            m.swap(c, piv);
            inv.swap(c, piv);
            let d = m[c][c];
            for k in 0..4 {
                m[c][k] /= d;
                inv[c][k] /= d;
            }
            for r in 0..4 {
                if r != c {
                    let f = m[r][c];
                    if f != 0.0 {
                        for k in 0..4 {
                            m[r][k] -= f * m[c][k];
                            inv[r][k] -= f * inv[c][k];
                        }
                    }
                }
            }
        }
        M4 { m: inv }
    }

    /// Splits into translation, euler rotation (degrees, XYZ) and scale.
    pub fn decompose(&self) -> (V3, V3, V3) {
        let t = self.translation();
        let mut c0 = self.col(0);
        let mut c1 = self.col(1);
        let mut c2 = self.col(2);
        let mut s = v3(c0.len(), c1.len(), c2.len());
        if c0.cross(c1).dot(c2) < 0.0 {
            s.x = -s.x;
        }
        if s.x.abs() > 1e-9 {
            c0 = c0 / s.x;
        }
        if s.y.abs() > 1e-9 {
            c1 = c1 / s.y;
        }
        if s.z.abs() > 1e-9 {
            c2 = c2 / s.z;
        }
        let mut r = M4::IDENTITY;
        r.set_col(0, c0);
        r.set_col(1, c1);
        r.set_col(2, c2);
        (t, r.to_euler_deg(), s)
    }

    /// Extracts XYZ euler angles (degrees) from a pure rotation matrix.
    pub fn to_euler_deg(&self) -> V3 {
        let m = &self.m;
        let sy = (-m[2][0]).clamp(-1.0, 1.0);
        let b = sy.asin();
        let (a, c) = if sy.abs() < 0.9999 {
            (m[2][1].atan2(m[2][2]), m[1][0].atan2(m[0][0]))
        } else {
            ((-m[1][2]).atan2(m[1][1]), 0.0)
        };
        v3(a.to_degrees(), b.to_degrees(), c.to_degrees())
    }

    pub fn rotation_only(&self) -> M4 {
        let mut r = M4::IDENTITY;
        r.set_col(0, self.col(0).norm());
        r.set_col(1, self.col(1).norm());
        r.set_col(2, self.col(2).norm());
        r
    }
}

impl Mul for M4 {
    type Output = M4;
    fn mul(self, o: M4) -> M4 {
        let mut r = [[0.0f32; 4]; 4];
        for i in 0..4 {
            for j in 0..4 {
                r[i][j] = self.m[i][0] * o.m[0][j]
                    + self.m[i][1] * o.m[1][j]
                    + self.m[i][2] * o.m[2][j]
                    + self.m[i][3] * o.m[3][j];
            }
        }
        M4 { m: r }
    }
}

/// Rotation matrix from a unit quaternion given as (x, y, z, w).
pub fn quat_to_m4(q: [f32; 4]) -> M4 {
    let l = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    let (x, y, z, w) = if l > 1e-9 { (q[0] / l, q[1] / l, q[2] / l, q[3] / l) } else { (0.0, 0.0, 0.0, 1.0) };
    M4 {
        m: [
            [1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y - z * w), 2.0 * (x * z + y * w), 0.0],
            [2.0 * (x * y + z * w), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z - x * w), 0.0],
            [2.0 * (x * z - y * w), 2.0 * (y * z + x * w), 1.0 - 2.0 * (x * x + y * y), 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
    }
}

impl M4 {
    /// Quaternion (x, y, z, w) of the rotation part (scale is removed first).
    pub fn to_quat(&self) -> [f32; 4] {
        let r = self.rotation_only();
        let m = &r.m;
        let tr = m[0][0] + m[1][1] + m[2][2];
        let q = if tr > 0.0 {
            let s = (tr + 1.0).sqrt() * 2.0;
            [(m[2][1] - m[1][2]) / s, (m[0][2] - m[2][0]) / s, (m[1][0] - m[0][1]) / s, 0.25 * s]
        } else if m[0][0] > m[1][1] && m[0][0] > m[2][2] {
            let s = (1.0 + m[0][0] - m[1][1] - m[2][2]).sqrt() * 2.0;
            [0.25 * s, (m[0][1] + m[1][0]) / s, (m[0][2] + m[2][0]) / s, (m[2][1] - m[1][2]) / s]
        } else if m[1][1] > m[2][2] {
            let s = (1.0 + m[1][1] - m[0][0] - m[2][2]).sqrt() * 2.0;
            [(m[0][1] + m[1][0]) / s, 0.25 * s, (m[1][2] + m[2][1]) / s, (m[0][2] - m[2][0]) / s]
        } else {
            let s = (1.0 + m[2][2] - m[0][0] - m[1][1]).sqrt() * 2.0;
            [(m[0][2] + m[2][0]) / s, (m[1][2] + m[2][1]) / s, 0.25 * s, (m[1][0] - m[0][1]) / s]
        };
        // keep w positive for stable files
        if q[3] < 0.0 { [-q[0], -q[1], -q[2], -q[3]] } else { q }
    }
}

/// Distance from point p to segment ab.
pub fn dist_point_segment(p: V3, a: V3, b: V3) -> f32 {
    let ab = b - a;
    let l2 = ab.dot(ab);
    if l2 < 1e-12 {
        return (p - a).len();
    }
    let t = ((p - a).dot(ab) / l2).clamp(0.0, 1.0);
    (p - (a + ab * t)).len()
}

/// Ray / triangle intersection (Möller–Trumbore). Returns distance along the ray.
pub fn ray_tri(o: V3, d: V3, a: V3, b: V3, c: V3) -> Option<f32> {
    let e1 = b - a;
    let e2 = c - a;
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-9 {
        return None;
    }
    let inv = 1.0 / det;
    let s = o - a;
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = d.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = e2.dot(q) * inv;
    if t > 1e-5 { Some(t) } else { None }
}
