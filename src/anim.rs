//! Keyframe animation curves.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tangent {
    /// Smooth spline through the neighbouring keys.
    Spline,
    Linear,
    /// Smooth, but horizontal at the key (ease in/out).
    Flat,
    /// Holds the value until the next key.
    Step,
}

impl Tangent {
    pub const ALL: [Tangent; 4] = [Tangent::Spline, Tangent::Linear, Tangent::Flat, Tangent::Step];
    pub fn label(self) -> &'static str {
        match self {
            Tangent::Spline => crate::tr!("Spline", "Spline"),
            Tangent::Linear => crate::tr!("Linear", "Linear"),
            Tangent::Flat => crate::tr!("Flach", "Flat"),
            Tangent::Step => crate::tr!("Stufe", "Stepped"),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Key {
    pub frame: f32,
    pub value: f32,
    pub tangent: Tangent,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Curve {
    pub keys: Vec<Key>,
}

impl Curve {
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    pub fn sort(&mut self) {
        self.keys.sort_by(|a, b| a.frame.total_cmp(&b.frame));
        // merge keys that landed on the same frame
        self.keys.dedup_by(|b, a| (a.frame - b.frame).abs() < 1e-3);
    }

    pub fn set_key(&mut self, frame: f32, value: f32, tangent: Tangent) {
        if let Some(k) = self.keys.iter_mut().find(|k| (k.frame - frame).abs() < 1e-3) {
            k.value = value;
            return;
        }
        self.keys.push(Key { frame, value, tangent });
        self.sort();
    }

    pub fn key_index_at(&self, frame: f32) -> Option<usize> {
        self.keys.iter().position(|k| (k.frame - frame).abs() < 1e-3)
    }

    pub fn remove_key_at(&mut self, frame: f32) -> bool {
        let n = self.keys.len();
        self.keys.retain(|k| (k.frame - frame).abs() >= 1e-3);
        n != self.keys.len()
    }

    /// Slope (value per frame) of the tangent at key i.
    fn slope(&self, i: usize) -> f32 {
        let k = &self.keys[i];
        let n = self.keys.len();
        match k.tangent {
            Tangent::Flat | Tangent::Step => 0.0,
            Tangent::Linear => {
                if i + 1 < n {
                    let b = &self.keys[i + 1];
                    (b.value - k.value) / (b.frame - k.frame).max(1e-6)
                } else if i > 0 {
                    let a = &self.keys[i - 1];
                    (k.value - a.value) / (k.frame - a.frame).max(1e-6)
                } else {
                    0.0
                }
            }
            Tangent::Spline => {
                if i == 0 || i + 1 >= n {
                    0.0
                } else {
                    let a = &self.keys[i - 1];
                    let b = &self.keys[i + 1];
                    (b.value - a.value) / (b.frame - a.frame).max(1e-6)
                }
            }
        }
    }

    pub fn eval(&self, frame: f32) -> Option<f32> {
        let n = self.keys.len();
        if n == 0 {
            return None;
        }
        let first = &self.keys[0];
        let last = &self.keys[n - 1];
        if frame <= first.frame {
            return Some(first.value);
        }
        if frame >= last.frame {
            return Some(last.value);
        }
        let i = self.keys.partition_point(|k| k.frame <= frame).saturating_sub(1);
        let a = &self.keys[i];
        let b = &self.keys[i + 1];
        let dt = (b.frame - a.frame).max(1e-6);
        let t = (frame - a.frame) / dt;
        match a.tangent {
            Tangent::Step => Some(a.value),
            Tangent::Linear if b.tangent == Tangent::Linear => Some(a.value + (b.value - a.value) * t),
            _ => {
                let m0 = if a.tangent == Tangent::Linear {
                    (b.value - a.value) / dt
                } else {
                    self.slope(i)
                } * dt;
                let m1 = if b.tangent == Tangent::Linear {
                    (b.value - a.value) / dt
                } else {
                    self.slope(i + 1)
                } * dt;
                let t2 = t * t;
                let t3 = t2 * t;
                let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
                let h10 = t3 - 2.0 * t2 + t;
                let h01 = -2.0 * t3 + 3.0 * t2;
                let h11 = t3 - t2;
                Some(h00 * a.value + h10 * m0 + h01 * b.value + h11 * m1)
            }
        }
    }

    pub fn value_range(&self) -> Option<(f32, f32)> {
        if self.keys.is_empty() {
            return None;
        }
        let mut lo = f32::MAX;
        let mut hi = f32::MIN;
        let a = self.keys[0].frame;
        let b = self.keys[self.keys.len() - 1].frame;
        let steps = 64;
        for s in 0..=steps {
            let f = a + (b - a) * s as f32 / steps as f32;
            let v = self.eval(f).unwrap_or(0.0);
            lo = lo.min(v);
            hi = hi.max(v);
        }
        Some((lo, hi))
    }
}

/// The ten keyable transform channels of every node.
pub const CHANNELS: usize = 10;
/// Display name of a keyable channel in the current language.
pub fn channel_name(c: usize) -> &'static str {
    const DE: [&str; CHANNELS] = [
        "Position X", "Position Y", "Position Z", "Drehung X", "Drehung Y", "Drehung Z",
        "Skalierung X", "Skalierung Y", "Skalierung Z", "Sichtbarkeit",
    ];
    const EN: [&str; CHANNELS] = [
        "Translate X", "Translate Y", "Translate Z", "Rotate X", "Rotate Y", "Rotate Z",
        "Scale X", "Scale Y", "Scale Z", "Visibility",
    ];
    let c = c.min(CHANNELS - 1);
    if crate::i18n::is_en() { EN[c] } else { DE[c] }
}
