//! 2D animation (ext): `CreateAnim2D`, `Anim2DPlay`, `DrawAnim2D`, ...
//!
//! An animation plays a range of frames of an animation image
//! (`LoadAnimImage`) by the clock: `mode` 1 loops, 2 goes back and forth
//! (ping-pong), 3 plays once and stops on the last frame. Each animation
//! keeps its own time, so several characters can share one image.

use std::time::Instant;

#[derive(Debug, Clone)]
pub struct Anim2D {
    pub image: i32,
    pub first: i32,
    pub last: i32,
    pub fps: f32,
    pub mode: i32,
    start: Instant,
    /// Time already played when paused.
    paused: Option<f32>,
}

impl Anim2D {
    pub fn new(image: i32, first: i32, last: i32, fps: f32, mode: i32) -> Anim2D {
        Anim2D { image, first, last, fps, mode, start: Instant::now(), paused: None }
    }

    /// Starts a new sequence from its first frame.
    pub fn play(&mut self, first: i32, last: i32, fps: f32, mode: i32) {
        *self = Anim2D { image: self.image, ..Anim2D::new(self.image, first, last, fps, mode) };
    }

    fn elapsed(&self) -> f32 {
        self.paused.unwrap_or_else(|| self.start.elapsed().as_secs_f32())
    }

    pub fn pause(&mut self, pause: bool) {
        match (pause, self.paused) {
            (true, None) => self.paused = Some(self.elapsed()),
            (false, Some(t)) => {
                self.start = Instant::now() - std::time::Duration::from_secs_f32(t);
                self.paused = None;
            }
            _ => {}
        }
    }

    /// Frame shown after `t` seconds, and whether the animation still plays.
    pub fn frame_at(&self, t: f32) -> (i32, bool) {
        let count = (self.last - self.first).abs() + 1;
        let dir = if self.last >= self.first { 1 } else { -1 };
        if count <= 1 || self.fps <= 0.0 {
            return (self.first, count > 1 && self.fps > 0.0);
        }
        let step = (t * self.fps).floor() as i64;
        let n = count as i64;
        let (i, playing) = match self.mode {
            2 => {
                // 0 1 2 .. n-1 n-2 .. 1, repeating
                let period = 2 * (n - 1);
                let k = step.rem_euclid(period);
                (if k < n { k } else { period - k }, true)
            }
            3 => (step.min(n - 1), step < n - 1),
            _ => (step.rem_euclid(n), true),
        };
        (self.first + dir * i as i32, playing)
    }

    pub fn frame(&self) -> i32 {
        self.frame_at(self.elapsed()).0
    }

    pub fn playing(&self) -> bool {
        self.paused.is_none() && self.frame_at(self.elapsed()).1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes() {
        let a = Anim2D::new(1, 4, 7, 10.0, 1);
        assert_eq!(a.frame_at(0.0), (4, true));
        assert_eq!(a.frame_at(0.35).0, 7);
        assert_eq!(a.frame_at(0.4).0, 4);
        let p = Anim2D::new(1, 0, 3, 10.0, 2);
        let frames: Vec<i32> = (0..8).map(|i| p.frame_at(i as f32 / 10.0 + 0.01).0).collect();
        assert_eq!(frames, vec![0, 1, 2, 3, 2, 1, 0, 1]);
        let o = Anim2D::new(1, 0, 2, 10.0, 3);
        assert_eq!(o.frame_at(0.15), (1, true));
        assert_eq!(o.frame_at(5.0), (2, false));
        let back = Anim2D::new(1, 5, 3, 10.0, 1);
        assert_eq!(back.frame_at(0.15).0, 4);
    }
}
