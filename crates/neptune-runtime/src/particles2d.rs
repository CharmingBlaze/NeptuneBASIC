//! 2D particles (ext): `CreateParticles2D`, `DrawParticles2D` and the
//! settings commands.
//!
//! A 2D particle system throws out copies of an image (a soft white dot when
//! none is given) that fly, fall, spin, grow or shrink and fade, for sparks,
//! smoke, dust, rain or explosions in 2D games. It emits steadily
//! (`Particles2DRate`) or in bursts (`EmitParticles2D`) from its position,
//! and moves and draws its particles each time `DrawParticles2D` is called,
//! by the real time since the last call, so draw it once a frame. Positions
//! are 2D drawing coordinates, so `Camera2D` and `Origin` apply.

use std::time::Instant;

use fyrox::asset::untyped::ResourceKind;
use fyrox::core::{color::Color, math::Rect, uuid::Uuid};
use fyrox::resource::texture::{Texture, TextureKind, TexturePixelKind, TextureResource};

use crate::world::Host3D;

#[derive(Clone, Debug)]
struct Particle {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    age: f32,
    life: f32,
    angle: f32,
    spin: f32,
}

#[derive(Clone, Debug)]
pub struct System {
    /// 2D image (0 = the built-in dot).
    pub image: i32,
    pub x: f32,
    pub y: f32,
    pub rate: f32,
    pub life: (f32, f32),
    pub speed: (f32, f32),
    /// Direction in degrees (0 right, 90 down) and spread around it.
    pub angle: f32,
    pub spread: f32,
    pub gravity: (f32, f32),
    pub size: (f32, f32),
    pub spin: (f32, f32),
    /// Start and end colour, RGBA 0-1.
    pub start: [f32; 4],
    pub end: [f32; 4],
    pub max: usize,
    particles: Vec<Particle>,
    owed: f32,
    last: Option<Instant>,
    rng: u64,
}

impl System {
    pub fn new(image: i32, seed: u64) -> System {
        System {
            image,
            x: 0.0,
            y: 0.0,
            rate: 0.0,
            life: (0.6, 1.2),
            speed: (60.0, 140.0),
            angle: -90.0,
            spread: 360.0,
            gravity: (0.0, 0.0),
            size: (1.0, 1.0),
            spin: (0.0, 0.0),
            start: [1.0, 1.0, 1.0, 1.0],
            end: [1.0, 1.0, 1.0, 0.0],
            max: 5000,
            particles: Vec::new(),
            owed: 0.0,
            last: None,
            rng: seed | 1,
        }
    }

    fn rand(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng = x;
        (x >> 40) as f32 / (1u64 << 24) as f32
    }

    fn between(&mut self, r: (f32, f32)) -> f32 {
        r.0 + (r.1 - r.0) * self.rand()
    }

    /// Adds `n` particles at the system's position.
    pub fn emit(&mut self, n: usize) {
        for _ in 0..n {
            if self.particles.len() >= self.max {
                break;
            }
            let dir = (self.angle + (self.rand() - 0.5) * self.spread).to_radians();
            let speed = self.between(self.speed);
            let life = self.between(self.life).max(0.01);
            let spin = self.between(self.spin);
            let angle = self.rand() * 360.0;
            self.particles.push(Particle { x: self.x, y: self.y, vx: dir.cos() * speed, vy: dir.sin() * speed, age: 0.0, life, angle, spin });
        }
    }

    /// Moves everything on by `dt` seconds and emits at the steady rate.
    pub fn step(&mut self, dt: f32) {
        self.owed += self.rate * dt;
        let n = self.owed.floor();
        self.owed -= n;
        self.emit(n as usize);
        let (gx, gy) = self.gravity;
        for p in &mut self.particles {
            p.vx += gx * dt;
            p.vy += gy * dt;
            p.x += p.vx * dt;
            p.y += p.vy * dt;
            p.angle += p.spin * dt;
            p.age += dt;
        }
        self.particles.retain(|p| p.age < p.life);
    }

    pub fn count(&self) -> usize {
        self.particles.len()
    }
}

/// The built-in soft round dot (16 x 16, white, alpha fading to the edge).
fn dot() -> TextureResource {
    const N: u32 = 16;
    let mut bytes = Vec::with_capacity((N * N * 4) as usize);
    for y in 0..N {
        for x in 0..N {
            let (dx, dy) = (x as f32 + 0.5 - N as f32 / 2.0, y as f32 + 0.5 - N as f32 / 2.0);
            let d = (dx * dx + dy * dy).sqrt() / (N as f32 / 2.0);
            let a = (1.0 - d).clamp(0.0, 1.0);
            bytes.extend_from_slice(&[255, 255, 255, (a * a * 255.0) as u8]);
        }
    }
    let tex = Texture::from_bytes(TextureKind::Rectangle { width: N, height: N }, TexturePixelKind::RGBA8, bytes).expect("dot texture");
    TextureResource::new_ok(Uuid::new_v4(), ResourceKind::Embedded, tex)
}

impl Host3D<'_> {
    pub fn particles2d(&mut self, p: i32) -> Result<&mut System, String> {
        usize::try_from(p - 1)
            .ok()
            .and_then(|i| self.world.particles2d.get_mut(i))
            .and_then(Option::as_mut)
            .ok_or_else(|| format!("2D particles {p} do not exist"))
    }

    pub fn create_particles2d(&mut self, image: i32) -> Result<i32, String> {
        if image != 0 {
            self.image(image)?;
        }
        let seed = 0x9e37_79b9_7f4a_7c15u64 ^ (self.world.particles2d.len() as u64).wrapping_mul(0x2545_f491);
        self.world.particles2d.push(Some(System::new(image, seed)));
        Ok(self.world.particles2d.len() as i32)
    }

    /// `DrawParticles2D`: moves the particles on by the time since the last
    /// call and draws them.
    pub fn draw_particles2d(&mut self, p: i32) -> Result<(), String> {
        let now = Instant::now();
        let image = {
            let s = self.particles2d(p)?;
            let dt = s.last.map_or(0.0, |l| now.duration_since(l).as_secs_f32()).min(0.1);
            s.last = Some(now);
            s.step(dt);
            s.image
        };
        // texture, frame size and the part of the texture to use
        let (tex, w, h, uv) = if image != 0 {
            if self.image(image)?.dirty {
                self.image_mut(image)?.commit();
            }
            let img = self.image(image)?;
            let (w, h) = img.size();
            (img.masked.clone(), w, h, img.uv(0, None)?)
        } else {
            let dot = self.world.particle_dot.get_or_insert_with(dot).clone();
            (dot, 8.0, 8.0, Rect::new(0.0, 0.0, 1.0, 1.0))
        };
        let s = self.particles2d(p)?.clone();
        let canvas = &mut self.world.canvas;
        for q in &s.particles {
            let t = (q.age / q.life).clamp(0.0, 1.0);
            let k = s.size.0 + (s.size.1 - s.size.0) * t;
            let c: [f32; 4] = std::array::from_fn(|i| s.start[i] + (s.end[i] - s.start[i]) * t);
            let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            let color = Color::from_rgba(b(c[0]), b(c[1]), b(c[2]), b(c[3]));
            let (pw, ph) = (w * k, h * k);
            canvas.image_ex(q.x, q.y, pw, ph, (pw / 2.0, ph / 2.0), q.angle, uv, tex.clone(), color);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn particles_live_move_and_die() {
        let mut s = System::new(0, 7);
        s.speed = (100.0, 100.0);
        s.angle = 0.0;
        s.spread = 0.0;
        s.life = (1.0, 1.0);
        s.emit(10);
        assert_eq!(s.count(), 10);
        s.step(0.5);
        assert!((s.particles[0].x - 50.0).abs() < 1e-3 && s.particles[0].y.abs() < 1e-3);
        s.step(0.6);
        assert_eq!(s.count(), 0);
        // a steady rate of 100 a second makes 50 in half a second
        s.rate = 100.0;
        s.step(0.5);
        assert_eq!(s.count(), 50);
        s.max = 60;
        s.emit(100);
        assert_eq!(s.count(), 60);
    }
}
