//! Keyframe animation of any entity (`AddAnimSeq`, `SetAnimKey`), played by
//! the same `Animate`/`SetAnimTime` commands as loaded models.
//!
//! `AddAnimSeq entity, length` starts a sequence of `length` frames;
//! `SetAnimKey entity, frame` records the entity's current position,
//! rotation and/or scale at that frame. Playback interpolates linearly
//! between keys (rotations with spherical interpolation).

use std::collections::BTreeMap;

use fyrox::core::algebra::{Matrix3, Rotation3, UnitQuaternion, Vector3};

use crate::transform::Xform;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Key {
    pub pos: Option<Vector3<f32>>,
    pub rot: Option<UnitQuaternion<f32>>,
    pub scale: Option<Vector3<f32>>,
}

#[derive(Clone, Debug, Default)]
pub struct Seq {
    pub length: f32,
    pub keys: BTreeMap<u32, Key>,
}

#[derive(Clone, Debug, Default)]
pub struct KeyAnim {
    pub seqs: Vec<Seq>,
}

fn rot_q(m: &Matrix3<f32>) -> UnitQuaternion<f32> {
    UnitQuaternion::from_rotation_matrix(&Rotation3::from_matrix_unchecked(*m))
}

impl Seq {
    /// Interpolated value of one channel at `frame`.
    fn channel<T: Copy>(&self, frame: f32, get: impl Fn(&Key) -> Option<T>, mix: impl Fn(T, T, f32) -> T) -> Option<T> {
        let keys: Vec<(f32, T)> = self.keys.iter().filter_map(|(f, k)| get(k).map(|v| (*f as f32, v))).collect();
        let first = keys.first()?;
        if frame <= first.0 {
            return Some(first.1);
        }
        for w in keys.windows(2) {
            let ((f0, a), (f1, b)) = (w[0], w[1]);
            if frame <= f1 {
                let t = if f1 > f0 { (frame - f0) / (f1 - f0) } else { 1.0 };
                return Some(mix(a, b, t));
            }
        }
        keys.last().map(|k| k.1)
    }

    /// Applies the sequence at `frame` to a transform (channels without keys are kept).
    pub fn sample(&self, frame: f32, xf: &mut Xform) {
        if let Some(p) = self.channel(frame, |k| k.pos, |a, b, t| a.lerp(&b, t)) {
            xf.pos = p;
        }
        if let Some(q) = self.channel(frame, |k| k.rot, |a, b, t| a.slerp(&b, t)) {
            xf.rot = q.to_rotation_matrix().into_inner();
        }
        if let Some(s) = self.channel(frame, |k| k.scale, |a, b, t| a.lerp(&b, t)) {
            xf.scale = s;
        }
    }
}

/// A key from a transform with the chosen channels.
pub fn key_from(xf: &Xform, pos: bool, rot: bool, scale: bool) -> Key {
    Key { pos: pos.then_some(xf.pos), rot: rot.then(|| rot_q(&xf.rot)), scale: scale.then_some(xf.scale) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transform::rotation;

    #[test]
    fn interpolates_between_keys() {
        let mut seq = Seq { length: 10.0, keys: BTreeMap::new() };
        let mut a = Xform::default();
        seq.keys.insert(0, key_from(&a, true, true, false));
        a.pos = Vector3::new(10.0, 0.0, 0.0);
        a.rot = rotation(0.0, 90.0, 0.0);
        seq.keys.insert(10, key_from(&a, true, true, false));
        let mut x = Xform::default();
        seq.sample(5.0, &mut x);
        assert!((x.pos.x - 5.0).abs() < 1e-5);
        let (_, yaw, _) = crate::transform::angles(&x.rot);
        assert!((yaw - 45.0).abs() < 0.1, "{yaw}");
        // past the last key it holds the last value; scale was never keyed
        seq.sample(20.0, &mut x);
        assert!((x.pos.x - 10.0).abs() < 1e-5);
        assert_eq!(x.scale, Vector3::new(1.0, 1.0, 1.0));
    }
}
