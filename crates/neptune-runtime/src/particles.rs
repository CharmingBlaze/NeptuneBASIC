//! Particle emitters (ext): `CreateEmitter` and the `Emitter*` commands.
//!
//! An emitter is an entity that sprays camera-facing particles, drawn by
//! Fyrox's particle renderer with soft edges where they meet geometry (soft
//! particles, `EmitterSoftness`). `EntityTexture` sets the particle image.
//! Speeds are in units per second, angles in degrees, and directions are in
//! Blitz space. By default particles are left behind in the world when the
//! emitter moves (smoke trails); `EmitterLocal` keeps them with it.

use fyrox::{
    asset::untyped::ResourceKind,
    core::{
        algebra::Vector3,
        color::Color,
        color_gradient::{ColorGradient, GradientPoint},
        uuid::Uuid,
    },
    material::{
        shader::{Shader, ShaderResource},
        Material, MaterialResource,
    },
    scene::{
        base::BaseBuilder,
        particle_system::{
            emitter::{base::BaseEmitterBuilder, cuboid::CuboidEmitterBuilder, sphere::SphereEmitterBuilder},
            CoordinateSystem, ParticleSystem, ParticleSystemBuilder,
        },
    },
};

use crate::world::{Host3D, Kind};

/// Fyrox moves a particle by its velocity once per update, whatever the
/// update's length, while sizes, spins and lifetimes use the length. Games
/// update 60 times a second; with `--uncapped` the length varies, so
/// emitters are re-tuned to it each frame (`retune_emitters`).
pub const UPDATE: f32 = 1.0 / 60.0;

/// The particle shader (see particle.shader), loaded once.
fn shader() -> ShaderResource {
    thread_local! {
        static SHADER: ShaderResource = ShaderResource::new_ok(
            Uuid::new_v4(),
            ResourceKind::Embedded,
            Shader::from_string(include_str!("particle.shader")).expect("built-in particle shader is valid"),
        );
    }
    SHADER.with(|s| s.clone())
}

#[derive(Debug, Clone)]
pub struct Settings {
    pub rate: u32,
    pub max: Option<u32>,
    pub life: (f32, f32),
    pub velocity: [f32; 3],
    pub spread: f32,
    pub size: f32,
    pub grow: f32,
    pub spin: (f32, f32),
    pub start: [u8; 4],
    pub end: [u8; 4],
    pub gravity: [f32; 3],
    /// 0 = sphere (size.0 = radius), 1 = box
    pub shape: i32,
    pub shape_size: [f32; 3],
    pub softness: f32,
    pub playing: bool,
    pub local: bool,
    /// The update length the velocities were set for.
    pub tuned_for: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            rate: 30,
            max: None,
            life: (1.0, 2.0),
            velocity: [0.0, 1.5, 0.0],
            spread: 0.5,
            size: 0.2,
            grow: 0.0,
            spin: (0.0, 0.0),
            start: [255, 255, 255, 255],
            end: [255, 255, 255, 0],
            gravity: [0.0, 0.0, 0.0],
            shape: 0,
            shape_size: [0.0; 3],
            softness: 1.0,
            playing: true,
            local: false,
            tuned_for: UPDATE,
        }
    }
}

fn range(a: f32, b: f32) -> std::ops::Range<f32> {
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    // Fyrox needs a non-empty range
    lo..hi.max(lo + 1e-4)
}

impl Settings {
    /// Launch velocity ranges (x, y, z) as distances per update of
    /// `tuned_for` seconds (sizes and spins are per second). Blitz space:
    /// X is mirrored.
    fn velocity_ranges(&self) -> [std::ops::Range<f32>; 3] {
        let per_update = |v: f32| v * self.tuned_for;
        let v = [-self.velocity[0], self.velocity[1], self.velocity[2]];
        let s = self.spread;
        [0, 1, 2].map(|i| range(per_update(v[i] - s), per_update(v[i] + s)))
    }

    fn emitter(&self) -> fyrox::scene::particle_system::emitter::Emitter {
        let [vx, vy, vz] = self.velocity_ranges();
        let spin = (self.spin.0.to_radians(), self.spin.1.to_radians());
        let base = BaseEmitterBuilder::new()
            .with_spawn_rate(self.rate)
            .with_max_particles(self.max.unwrap_or(u32::MAX))
            .with_lifetime_range(range(self.life.0, self.life.1))
            .with_size_range(range(self.size * 0.9, self.size * 1.1))
            .with_size_modifier_range(range(self.grow, self.grow))
            .with_x_velocity_range(vx)
            .with_y_velocity_range(vy)
            .with_z_velocity_range(vz)
            .with_rotation_speed_range(range(spin.0, spin.1))
            .with_rotation_range(range(0.0, std::f32::consts::TAU));
        match self.shape {
            1 => CuboidEmitterBuilder::new(base)
                .with_width(self.shape_size[0])
                .with_height(self.shape_size[1])
                .with_depth(self.shape_size[2])
                .build(),
            _ => SphereEmitterBuilder::new(base).with_radius(self.shape_size[0]).build(),
        }
    }

    fn gradient(&self) -> ColorGradient {
        let c = |p: [u8; 4]| Color::from_rgba(p[0], p[1], p[2], p[3]);
        let mut g = ColorGradient::new();
        g.add_point(GradientPoint::new(0.0, c(self.start)));
        g.add_point(GradientPoint::new(1.0, c(self.end)));
        g
    }
}

/// `CreateEmitter`: a new particle emitter entity.
pub fn create(h: &mut Host3D, texture: i32, parent: i32) -> Result<i32, String> {
    let node = ParticleSystemBuilder::new(BaseBuilder::new()).with_acceleration(Vector3::zeros()).build_node();
    let id = h.add_entity(node, Kind::Pivot, parent)?;
    let e = h.ent_mut(id)?;
    e.textures[0] = texture;
    e.emitter = Some(Box::default());
    h.refresh_material(id)?;
    Ok(id)
}

impl Host3D<'_> {
    pub fn emitter_mut(&mut self, e: i32) -> Result<&mut Settings, String> {
        self.ent_mut(e)?.emitter.as_deref_mut().ok_or_else(|| format!("Entity {e} is not an emitter"))
    }

    /// Changes an emitter's settings and rebuilds it.
    pub fn modify_emitter(&mut self, e: i32, f: impl FnOnce(&mut Settings)) -> Result<(), String> {
        f(self.emitter_mut(e)?);
        self.refresh_material(e)
    }

    /// Applies an emitter's settings and texture. False if the entity is not
    /// an emitter.
    pub fn refresh_emitter(&mut self, id: i32) -> Result<bool, String> {
        let e = self.ent(id)?;
        let Some(s) = e.emitter.as_deref().cloned() else { return Ok(false) };
        let (node, tex) = (e.node, e.textures[0]);
        let tex = if tex != 0 { Some(self.texture(tex)?.resource.clone()) } else { None };
        let mut m = Material::from_shader(shader());
        if let Some(t) = tex {
            m.bind("diffuseTexture", t);
        }
        // a higher factor gives harder edges; softness 0 turns soft edges off
        m.set_property("softBoundarySharpnessFactor", if s.softness <= 0.0 { 10000.0 } else { 100.0 / s.softness });
        let material = MaterialResource::new_ok(Uuid::new_v4(), ResourceKind::Embedded, m);
        let Some(ps) = self.graph()?[node].cast_mut::<ParticleSystem>() else { return Ok(false) };
        ps.set_material(material);
        ps.emitters.set_value_and_mark_modified(vec![s.emitter()]);
        ps.set_acceleration(Vector3::new(-s.gravity[0], s.gravity[1], s.gravity[2]));
        ps.set_color_over_lifetime_gradient(s.gradient());
        ps.set_coordinate_system(if s.local { CoordinateSystem::Local } else { CoordinateSystem::World });
        ps.play(s.playing);
        Ok(true)
    }

    /// Scales emitters' launch speeds to the length of the latest update
    /// (it only differs from 1/60 s with `--uncapped`).
    pub fn retune_emitters(&mut self, dt: f32) -> Result<(), String> {
        let dt = dt.clamp(1e-4, 0.25);
        let stale: Vec<i32> = self
            .entities()
            .filter(|(_, e)| e.emitter.as_ref().is_some_and(|s| (s.tuned_for - dt).abs() > 0.1 * dt))
            .map(|(i, _)| i)
            .collect();
        for e in stale {
            let s = self.emitter_mut(e)?;
            s.tuned_for = dt;
            let ranges = s.velocity_ranges();
            let node = self.ent(e)?.node;
            if let Some(ps) = self.graph()?[node].cast_mut::<ParticleSystem>() {
                for em in ps.emitters.get_value_mut_and_mark_modified().iter_mut() {
                    let [x, y, z] = ranges.clone();
                    em.set_x_velocity_range(x).set_y_velocity_range(y).set_z_velocity_range(z);
                }
            }
        }
        Ok(())
    }

    /// `EmitterClear`: removes the particles in flight.
    pub fn clear_particles(&mut self, e: i32) -> Result<(), String> {
        let node = self.ent(e)?.node;
        if let Some(ps) = self.graph()?[node].cast_mut::<ParticleSystem>() {
            ps.clear_particles();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_speeds_follow_the_update_length() {
        let mut s = Settings { velocity: [0.0, 6.0, 0.0], spread: 0.0, ..Default::default() };
        // 6 units a second is 0.1 per 1/60 s update
        assert!((s.velocity_ranges()[1].start - 0.1).abs() < 1e-5);
        // twice as long an update: twice as far per update
        s.tuned_for = 2.0 * UPDATE;
        assert!((s.velocity_ranges()[1].start - 0.2).abs() < 1e-5);
        // Blitz X is mirrored
        s.velocity = [3.0, 0.0, 0.0];
        assert!(s.velocity_ranges()[0].start < 0.0);
    }

    #[test]
    fn the_particle_shader_parses() {
        assert!(Shader::from_string(include_str!("particle.shader")).is_ok());
    }

    #[test]
    fn ranges_are_never_empty() {
        assert_eq!(range(2.0, 1.0).start, 1.0);
        let r = range(1.0, 1.0);
        assert!(r.end > r.start);
    }
}
