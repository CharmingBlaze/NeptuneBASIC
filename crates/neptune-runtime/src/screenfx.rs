//! Camera shake and screen fades (ext): `CameraShake`, `ScreenFade`,
//! `ScreenFading`.
//!
//! A shake jolts the camera's picture (a small turn and offset that die
//! away) without moving the camera itself, so the program's camera
//! position and picks are unaffected. A fade lays a colour over the whole
//! screen, 3D and 2D, changing its opacity over time: fade to black
//! between levels, flash white when hit.

use std::time::Instant;

use fyrox::core::algebra::Vector3;
use fyrox::core::color::Color;

use crate::world::Host3D;

#[derive(Clone, Debug)]
pub struct Shake {
    pub camera: i32,
    pub strength: f32,
    pub seconds: f32,
    pub start: Instant,
}

impl Shake {
    /// Turn (pitch, yaw, roll in degrees) and offset (camera-space units)
    /// at `now`, or `None` when the shake is over.
    pub fn offset(&self, now: Instant) -> Option<(Vector3<f32>, Vector3<f32>)> {
        let t = now.duration_since(self.start).as_secs_f32();
        if t >= self.seconds || self.seconds <= 0.0 {
            return None;
        }
        let fade = (1.0 - t / self.seconds).powi(2) * self.strength;
        // a few unrelated frequencies make an irregular jolt
        let w = |a: f32, b: f32| (t * a).sin() * 0.6 + (t * b + 1.3).sin() * 0.4;
        let turn = Vector3::new(w(41.0, 67.0), w(37.0, 59.0), w(29.0, 73.0) * 0.5) * (1.5 * fade);
        let shift = Vector3::new(w(53.0, 31.0), w(47.0, 71.0), 0.0) * (0.08 * fade);
        Some((turn, shift))
    }
}

/// A screen fade in progress (or finished, holding its colour).
#[derive(Clone, Debug)]
pub struct Fade {
    pub color: [u8; 3],
    pub from: f32,
    pub to: f32,
    pub seconds: f32,
    pub start: Instant,
}

impl Fade {
    pub fn alpha(&self, now: Instant) -> f32 {
        let t = if self.seconds <= 0.0 { 1.0 } else { (now.duration_since(self.start).as_secs_f32() / self.seconds).min(1.0) };
        self.from + (self.to - self.from) * t
    }

    pub fn running(&self, now: Instant) -> bool {
        self.seconds > 0.0 && now.duration_since(self.start).as_secs_f32() < self.seconds
    }

    /// The colour to lay over the screen now (alpha 0 = nothing).
    pub fn overlay(&self, now: Instant) -> Color {
        let a = (self.alpha(now).clamp(0.0, 1.0) * 255.0).round() as u8;
        Color::from_rgba(self.color[0], self.color[1], self.color[2], a)
    }
}

impl Host3D<'_> {
    /// `CameraShake`: strength 1 is a solid hit; it dies away over `seconds`.
    pub fn camera_shake(&mut self, cam: i32, strength: f32, seconds: f32) -> Result<(), String> {
        if !matches!(self.ent(cam)?.kind, crate::world::Kind::Camera { .. }) {
            return Err(format!("Entity {cam} is not a camera"));
        }
        self.world.shakes.retain(|s| s.camera != cam);
        if strength > 0.0 && seconds > 0.0 {
            self.world.shakes.push(Shake { camera: cam, strength, seconds, start: Instant::now() });
        }
        Ok(())
    }

    /// Shows the shaking cameras' pictures jolted (before rendering; the
    /// real transform comes back on the next render).
    pub fn apply_shakes(&mut self) -> Result<(), String> {
        if self.world.shakes.is_empty() {
            return Ok(());
        }
        let now = Instant::now();
        let shakes = std::mem::take(&mut self.world.shakes);
        let mut keep = Vec::new();
        for s in shakes {
            let Some(ent) = self.ent_any(s.camera) else { continue };
            let Some((turn, shift)) = s.offset(now) else { continue };
            if self.elsewhere(s.camera) {
                keep.push(s);
                continue;
            }
            let ent = ent.clone_xf_node();
            let mut xf = ent.0;
            xf.pos += xf.rot * shift;
            xf.rot *= crate::transform::rotation(turn.x, turn.y, turn.z);
            let tr = self.graph()?[ent.1].local_transform_mut();
            tr.set_position(xf.fyrox_position());
            tr.set_rotation(xf.fyrox_rotation());
            self.world.tweened.push(s.camera);
            keep.push(s);
        }
        self.world.shakes = keep;
        Ok(())
    }

    /// `ScreenFade`: from the current opacity to `alpha` (0 clear, 1 solid).
    pub fn screen_fade(&mut self, r: f32, g: f32, b: f32, alpha: f32, seconds: f32) {
        let now = Instant::now();
        let from = self.world.canvas.fade.as_ref().map_or(0.0, |f| f.alpha(now));
        let c = |v: f32| v.clamp(0.0, 255.0) as u8;
        self.world.canvas.fade = Some(Fade { color: [c(r), c(g), c(b)], from, to: alpha.clamp(0.0, 1.0), seconds: seconds.max(0.0), start: now });
    }
}

impl crate::world::Entity {
    /// The live local transform and node (for showing a jolted picture).
    fn clone_xf_node(&self) -> (crate::transform::Xform, fyrox::core::pool::Handle<fyrox::scene::node::Node>) {
        (self.xf, self.node)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn shakes_die_away() {
        let start = Instant::now();
        let s = Shake { camera: 1, strength: 1.0, seconds: 0.5, start };
        let (a, _) = s.offset(start + Duration::from_millis(30)).unwrap();
        let (b, _) = s.offset(start + Duration::from_millis(450)).unwrap();
        assert!(a.norm() <= 1.5 * 1.3 && b.norm() < a.norm().max(0.05));
        assert!(s.offset(start + Duration::from_millis(600)).is_none());
    }

    #[test]
    fn fades_reach_their_target() {
        let start = Instant::now();
        let f = Fade { color: [0, 0, 0], from: 0.0, to: 1.0, seconds: 2.0, start };
        assert!((f.alpha(start + Duration::from_secs(1)) - 0.5).abs() < 0.01);
        assert_eq!(f.overlay(start + Duration::from_secs(5)).a, 255);
        assert!(!f.running(start + Duration::from_secs(5)));
    }
}
