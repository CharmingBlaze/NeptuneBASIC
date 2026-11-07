//! Tweens (ext): `TweenPosition`, `TweenRotation`, `TweenScale`,
//! `TweenAlpha`, `TweenColor`, `Tweening`, `StopTweens`.
//!
//! A tween moves an entity's position, rotation, scale, alpha or colour from
//! where it is now to a target over some seconds, along an easing curve
//! (`EASE_*`, see `Ease`). Tweens run by real time and are applied when the
//! world is rendered; starting a tween of the same kind on the same entity
//! replaces the old one, and moving the entity yourself does not stop it.

use std::time::Instant;

use fyrox::core::algebra::{Matrix3, Rotation3, UnitQuaternion, Vector3};
use fyrox::core::color::Color;

use crate::transform::rotation;
use crate::world::Host3D;

type V3 = Vector3<f32>;

#[derive(Clone, Copy, Debug)]
pub enum Change {
    Position(V3, V3),
    Rotation(UnitQuaternion<f32>, UnitQuaternion<f32>),
    Scale(V3, V3),
    Alpha(f32, f32),
    Color(V3, V3),
}

impl Change {
    fn same_kind(&self, other: &Change) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

#[derive(Clone, Debug)]
pub struct Tween {
    pub entity: i32,
    pub change: Change,
    pub start: Instant,
    pub seconds: f32,
    pub ease: i32,
}

impl Tween {
    /// Progress along the curve at `now` (0-1, or beyond for "back"
    /// curves), and whether the tween is over.
    fn progress(&self, now: Instant) -> (f32, bool) {
        let t = if self.seconds <= 0.0 { 1.0 } else { now.duration_since(self.start).as_secs_f32() / self.seconds };
        (neptune_vm::ease::ease(t, self.ease), t >= 1.0)
    }
}

fn quat(m: &Matrix3<f32>) -> UnitQuaternion<f32> {
    UnitQuaternion::from_rotation_matrix(&Rotation3::from_matrix_unchecked(*m))
}

impl Host3D<'_> {
    fn start_tween(&mut self, entity: i32, change: Change, seconds: f32, ease: i32) -> Result<(), String> {
        self.ent(entity)?;
        self.world.tweens.retain(|t| !(t.entity == entity && t.change.same_kind(&change)));
        self.world.tweens.push(Tween { entity, change, start: Instant::now(), seconds: seconds.max(0.0), ease });
        Ok(())
    }

    pub fn tween_position(&mut self, e: i32, to: V3, seconds: f32, ease: i32) -> Result<(), String> {
        let from = self.ent(e)?.xf.pos;
        self.start_tween(e, Change::Position(from, to), seconds, ease)
    }

    pub fn tween_rotation(&mut self, e: i32, pitch: f32, yaw: f32, roll: f32, seconds: f32, ease: i32) -> Result<(), String> {
        let from = quat(&self.ent(e)?.xf.rot);
        let to = quat(&rotation(pitch, yaw, roll));
        self.start_tween(e, Change::Rotation(from, to), seconds, ease)
    }

    pub fn tween_scale(&mut self, e: i32, to: V3, seconds: f32, ease: i32) -> Result<(), String> {
        let from = self.ent(e)?.xf.scale;
        self.start_tween(e, Change::Scale(from, to), seconds, ease)
    }

    pub fn tween_alpha(&mut self, e: i32, to: f32, seconds: f32, ease: i32) -> Result<(), String> {
        let from = self.ent(e)?.color.a as f32 / 255.0;
        self.start_tween(e, Change::Alpha(from, to.clamp(0.0, 1.0)), seconds, ease)
    }

    pub fn tween_color(&mut self, e: i32, to: V3, seconds: f32, ease: i32) -> Result<(), String> {
        let c = self.ent(e)?.color;
        let from = V3::new(c.r as f32, c.g as f32, c.b as f32);
        self.start_tween(e, Change::Color(from, to.map(|v| v.clamp(0.0, 255.0))), seconds, ease)
    }

    /// `Tweening`: whether any tween of the entity (0: of any entity) is running.
    pub fn tweening(&self, e: i32) -> bool {
        self.world.tweens.iter().any(|t| e == 0 || t.entity == e)
    }

    /// `StopTweens`: stops an entity's tweens where they are (0: all).
    pub fn stop_tweens(&mut self, e: i32) {
        self.world.tweens.retain(|t| e != 0 && t.entity != e);
    }

    /// Applies the running tweens of entities in the current scene (before
    /// rendering) and drops the finished ones.
    pub fn apply_tweens(&mut self) -> Result<(), String> {
        if self.world.tweens.is_empty() {
            return Ok(());
        }
        let now = Instant::now();
        let tweens = std::mem::take(&mut self.world.tweens);
        let mut keep = Vec::with_capacity(tweens.len());
        for tw in tweens {
            if self.ent_any(tw.entity).is_none() {
                continue; // freed
            }
            if self.elsewhere(tw.entity) {
                keep.push(tw);
                continue;
            }
            let (k, done) = tw.progress(now);
            let e = tw.entity;
            match tw.change {
                Change::Position(a, b) => self.set_xf(e, |xf| xf.pos = a + (b - a) * k)?,
                Change::Scale(a, b) => self.set_xf(e, |xf| xf.scale = a + (b - a) * k)?,
                Change::Rotation(a, b) => {
                    // slerp cannot overshoot; "back" curves are clamped here
                    let q = a.slerp(&b, k.clamp(0.0, 1.0));
                    self.set_xf(e, |xf| xf.rot = *q.to_rotation_matrix().matrix())?;
                }
                Change::Alpha(a, b) => {
                    let v = ((a + (b - a) * k).clamp(0.0, 1.0) * 255.0).round() as u8;
                    let ent = self.ent_mut(e)?;
                    ent.fade_base = v;
                    ent.color = Color::from_rgba(ent.color.r, ent.color.g, ent.color.b, v);
                    self.refresh_material(e)?;
                }
                Change::Color(a, b) => {
                    let c = (a + (b - a) * k).map(|v| v.clamp(0.0, 255.0).round() as u8);
                    let ent = self.ent_mut(e)?;
                    ent.color = Color::from_rgba(c.x, c.y, c.z, ent.color.a);
                    self.refresh_material(e)?;
                }
            }
            if !done {
                keep.push(tw);
            }
        }
        // tweens started meanwhile (none can be, but keep them if so)
        keep.append(&mut self.world.tweens);
        self.world.tweens = keep;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_follows_time_and_curve() {
        let start = Instant::now();
        let tw = Tween { entity: 1, change: Change::Alpha(0.0, 1.0), start, seconds: 2.0, ease: 0 };
        let (k, done) = tw.progress(start + std::time::Duration::from_secs(1));
        assert!((k - 0.5).abs() < 1e-4 && !done);
        let (k, done) = tw.progress(start + std::time::Duration::from_secs(3));
        assert!((k - 1.0).abs() < 1e-6 && done);
        let instant = Tween { seconds: 0.0, ..tw };
        assert!(instant.progress(start).1);
        assert!(Change::Alpha(0.0, 1.0).same_kind(&Change::Alpha(1.0, 0.0)));
        assert!(!Change::Alpha(0.0, 1.0).same_kind(&Change::Scale(V3::zeros(), V3::zeros())));
    }
}
