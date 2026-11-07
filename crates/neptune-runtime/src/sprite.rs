//! Sprites (`CreateSprite`, `LoadSprite`, `SpriteViewMode`, ...): textured
//! quads that turn to face the camera.
//!
//! A sprite is a mesh entity with one 2x2 quad surface (built with the
//! custom-mesh code in `mesh.rs`). Before each render its node is turned to
//! face the first active camera according to its view mode; the entity's own
//! rotation is left alone, so `EntityYaw` and friends behave as usual.
//! Sprites are fullbright by default, the way they are mostly used
//! (particles, foliage, labels); `EntityFX` changes that.

use fyrox::core::algebra::{Matrix3, Vector3};

use crate::mesh::Vert;
use crate::transform::{angles, rotation, vector_pitch, vector_yaw};
use crate::world::{Host3D, Kind};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpriteState {
    /// 1 face camera, 2 free, 3 upright (camera yaw), 4 upright facing the camera position.
    pub mode: i32,
    /// `RotateSprite` roll in degrees.
    pub angle: f32,
    pub scale: (f32, f32),
    pub handle: (f32, f32),
    pub surface: i32,
}

/// Quad corners for a sprite's scale and handle, in Blitz space (front faces -Z).
pub fn quad(s: &SpriteState) -> Vec<Vert> {
    let (sx, sy) = s.scale;
    let (hx, hy) = s.handle;
    let v = |x: f32, y: f32, u: f32, t: f32| Vert {
        pos: Vector3::new((x - hx) * sx, (y - hy) * sy, 0.0),
        normal: Vector3::new(0.0, 0.0, -1.0),
        uv: [[u, t, 0.0], [u, t, 0.0]],
        ..Default::default()
    };
    vec![v(-1.0, 1.0, 0.0, 0.0), v(1.0, 1.0, 1.0, 0.0), v(1.0, -1.0, 1.0, 1.0), v(-1.0, -1.0, 0.0, 1.0)]
}

/// World rotation of a sprite for a camera, or `None` for free sprites.
pub fn facing(mode: i32, angle: f32, cam_rot: &Matrix3<f32>, cam_pos: Vector3<f32>, pos: Vector3<f32>) -> Option<Matrix3<f32>> {
    match mode {
        2 => None,
        3 => {
            let (_, yaw, _) = angles(cam_rot);
            Some(rotation(0.0, yaw, angle))
        }
        4 => {
            let d = pos - cam_pos;
            Some(rotation(0.0, vector_yaw(&d), angle))
        }
        _ => Some(cam_rot * rotation(0.0, 0.0, angle)),
    }
}

impl Host3D<'_> {
    pub fn create_sprite(&mut self, parent: i32) -> Result<i32, String> {
        self.scene()?;
        let id = self.add_entity(crate::mesh::empty_mesh_node(), Kind::Mesh, parent)?;
        let surface = self.create_surface(id, 0)?;
        let state = SpriteState { mode: 1, angle: 0.0, scale: (1.0, 1.0), handle: (0.0, 0.0), surface };
        self.edit_surface(surface, |s| {
            s.verts = quad(&state);
            s.tris = vec![[0, 1, 2], [0, 2, 3]];
        })?;
        let e = self.ent_mut(id)?;
        e.sprite = Some(state);
        e.fx = crate::classic::fx::FULLBRIGHT;
        self.refresh_material(id)?;
        Ok(id)
    }

    pub fn sprite_mut(&mut self, e: i32) -> Result<&mut SpriteState, String> {
        self.ent_mut(e)?.sprite.as_mut().ok_or_else(|| format!("Entity {e} is not a sprite"))
    }

    /// Rebuilds a sprite's quad after a scale or handle change.
    pub fn update_sprite(&mut self, e: i32) -> Result<(), String> {
        let s = *self.sprite_mut(e)?;
        self.edit_surface(s.surface, |info| info.verts = quad(&s))
    }

    /// Turns every sprite towards the first active camera (before rendering).
    pub fn face_sprites(&mut self) -> Result<(), String> {
        let sprites: Vec<(i32, SpriteState)> = self.entities().filter_map(|(i, e)| e.sprite.map(|s| (i, s))).collect();
        if sprites.is_empty() {
            return Ok(());
        }
        let cam = self.entities().find(|(_, e)| matches!(e.kind, Kind::Camera { mode, .. } if mode != 0)).map(|(i, _)| i);
        let Some(cam) = cam else { return Ok(()) };
        let cw = self.world_of(cam)?;
        for (e, s) in sprites {
            let pos = self.world_of(e)?.position();
            let Some(world_rot) = facing(s.mode, s.angle, &cw.rot, cw.position(), pos) else { continue };
            let local = self.parent_world(e)?.rot.transpose() * world_rot;
            let node = self.ent(e)?.node;
            let sm = Matrix3::from_diagonal(&Vector3::new(-1.0, 1.0, 1.0));
            let q = crate::transform::quat(&(sm * local * sm));
            self.graph()?[node].local_transform_mut().set_rotation(q);
        }
        Ok(())
    }
}

/// Unused helper kept close to `facing` for tests: pitch of a direction.
#[allow(dead_code)]
fn pitch_of(v: &Vector3<f32>) -> f32 {
    vector_pitch(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quad_respects_scale_and_handle() {
        let s = SpriteState { mode: 1, angle: 0.0, scale: (2.0, 0.5), handle: (1.0, 0.0), surface: 0 };
        let q = quad(&s);
        assert_eq!(q[0].pos, Vector3::new(-4.0, 0.5, 0.0));
        assert_eq!(q[2].pos, Vector3::new(0.0, -0.5, 0.0));
    }

    #[test]
    fn modes_face_the_camera() {
        let cam_rot = rotation(20.0, 30.0, 0.0);
        // mode 1 copies the camera rotation, so the quad's -Z normal points back at it
        let r = facing(1, 0.0, &cam_rot, Vector3::zeros(), Vector3::new(0.0, 0.0, 5.0)).unwrap();
        assert!((r - cam_rot).norm() < 1e-5);
        // mode 3 keeps the sprite upright
        let r = facing(3, 0.0, &cam_rot, Vector3::zeros(), Vector3::zeros()).unwrap();
        assert!((r * Vector3::y() - Vector3::y()).norm() < 1e-5);
        // mode 4 faces the camera position: forward points away from the camera
        let r = facing(4, 0.0, &Matrix3::identity(), Vector3::new(5.0, 0.0, 0.0), Vector3::zeros()).unwrap();
        let f = r * Vector3::z();
        assert!((f - Vector3::new(-1.0, 0.0, 0.0)).norm() < 1e-5, "{f:?}");
        assert!(facing(2, 0.0, &cam_rot, Vector3::zeros(), Vector3::zeros()).is_none());
    }
}
