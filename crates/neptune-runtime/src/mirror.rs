//! Mirrors (`CreateMirror`): a flat reflective plane, like a polished floor
//! or still water.
//!
//! Blitz3D draws the world upside down below a mirror's plane. Here a
//! hidden camera, the main camera reflected across the plane, renders the
//! scene into a texture each frame, and the mirror's large quad (its local
//! XZ plane, facing +Y) shows that texture projected through the reflected
//! camera's own view-projection matrix, so the reflection lines up exactly.
//! Semi-transparent ground drawn on top (as in Blitz3D) blends with it.
//! The reflection is one frame behind the main view. Fyrox logs one
//! "Failed to create GPU texture" line the first time a material samples a
//! camera's render target; it is harmless (the texture is registered by the
//! renderer right after) and appears once per mirror.

use fyrox::{
    core::algebra::{Matrix4, Point3, UnitQuaternion, Vector2, Vector3},
    graph::SceneGraph,
    resource::texture::{TextureResource, TextureResourceExtension},
    scene::{base::BaseBuilder, camera::Camera, camera::CameraBuilder, node::Node},
};
use fyrox::core::pool::Handle;

use crate::world::{Host3D, Kind};

/// Size of the reflection texture's longer side.
const RT_SIZE: f32 = 1024.0;

#[derive(Clone, Debug)]
pub struct MirrorState {
    pub camera: Handle<Node>,
    pub texture: TextureResource,
    /// Render target size (pixels).
    pub size: (u32, u32),
    /// Frames since the render target was (re)made; the mirror is shown once
    /// its camera has drawn into it.
    pub frames: u32,
}

/// Reflects a point and a direction across the plane through `p0` with unit normal `n`.
pub fn reflect_point(p: Vector3<f32>, p0: Vector3<f32>, n: Vector3<f32>) -> Vector3<f32> {
    p - n * (2.0 * (p - p0).dot(&n))
}

pub fn reflect_dir(d: Vector3<f32>, n: Vector3<f32>) -> Vector3<f32> {
    d - n * (2.0 * d.dot(&n))
}

fn mirrored(v: Vector3<f32>) -> Vector3<f32> {
    Vector3::new(-v.x, v.y, v.z)
}

fn rt_size(w: u32, h: u32) -> (u32, u32) {
    let aspect = w.max(1) as f32 / h.max(1) as f32;
    if aspect >= 1.0 {
        (RT_SIZE as u32, (RT_SIZE / aspect).round().max(1.0) as u32)
    } else {
        ((RT_SIZE * aspect).round().max(1.0) as u32, RT_SIZE as u32)
    }
}

impl Host3D<'_> {
    /// `CreateMirror`.
    pub fn create_mirror(&mut self, parent: i32) -> Result<i32, String> {
        self.scene()?;
        let b = crate::primitive::plane(10_000.0);
        let id = self.add_entity(crate::world::mesh_node(crate::mesh::surface_data(&b.verts, &b.tris), "Mirror"), Kind::Mesh, parent)?;
        let size = rt_size(self.world.width, self.world.height);
        let texture = TextureResource::new_render_target(size.0, size.1);
        let camera = self.graph()?.add_node(
            CameraBuilder::new(BaseBuilder::new().with_name("MirrorCamera")).with_render_target(Some(texture.clone())).build_node(),
        );
        let e = self.ent_mut(id)?;
        e.mirror = Some(MirrorState { camera, texture, size, frames: 0 });
        e.fx = crate::classic::fx::FULLBRIGHT;
        let node = e.node;
        // hidden until the reflection camera has rendered once
        self.graph()?[node].set_visibility(false);
        Ok(id)
    }

    /// Moves every mirror camera to the reflection of the first active
    /// camera and hands the projection to the mirror's material.
    pub fn update_mirrors(&mut self) -> Result<(), String> {
        let mirrors: Vec<i32> = self.entities().filter(|(_, e)| e.mirror.is_some()).map(|(i, _)| i).collect();
        if mirrors.is_empty() {
            return Ok(());
        }
        let cam = self.entities().find(|(_, e)| matches!(e.kind, Kind::Camera { mode, .. } if mode != 0)).map(|(i, _)| i);
        let Some(cam) = cam else { return Ok(()) };
        let cw = self.world_of(cam)?;
        let main_node = self.ent(cam)?.node;
        let projection = self.graph()?[main_node].cast::<Camera>().map(|c| c.projection().clone()).ok_or("camera missing")?;
        for m in mirrors {
            // keep the render target's shape matching the window
            let want = rt_size(self.world.width, self.world.height);
            if self.ent(m)?.mirror.as_ref().is_some_and(|s| s.size != want) {
                let tex = TextureResource::new_render_target(want.0, want.1);
                let cam_node = self.ent(m)?.mirror.as_ref().unwrap().camera;
                if let Some(c) = self.graph()?[cam_node].cast_mut::<Camera>() {
                    c.set_render_target(Some(tex.clone()));
                }
                if let Some(s) = self.ent_mut(m)?.mirror.as_mut() {
                    s.texture = tex;
                    s.size = want;
                    s.frames = 0;
                }
            }
            let frames = {
                let s = self.ent_mut(m)?.mirror.as_mut().unwrap();
                s.frames = s.frames.saturating_add(1);
                s.frames
            };
            if frames == 2 {
                // the camera has now drawn into the texture: show it
                self.refresh_material(m)?;
                let (node, hidden) = (self.ent(m)?.node, self.ent(m)?.hidden);
                self.graph()?[node].set_visibility(!hidden);
            }
            let mw = self.world_of(m)?;
            let n = (mw.rot * Vector3::y()).normalize();
            let p0 = mw.position();
            // reflected camera in Blitz space, then into Fyrox space
            let pos = mirrored(reflect_point(cw.position(), p0, n));
            let look = mirrored(reflect_dir(cw.rot * Vector3::z(), n));
            let up = mirrored(reflect_dir(cw.rot * Vector3::y(), n));
            let state = self.ent(m)?.mirror.clone().unwrap();
            {
                let graph = self.graph()?;
                let node = &mut graph[state.camera];
                let t = node.local_transform_mut();
                t.set_position(pos);
                t.set_rotation(UnitQuaternion::face_towards(&look, &up));
                if let Some(c) = node.cast_mut::<Camera>() {
                    c.set_projection(projection.clone());
                }
            }
            // the same view-projection Fyrox will use for that camera
            let view = Matrix4::look_at_rh(&Point3::from(pos), &Point3::from(pos + look), &up);
            let proj = projection.matrix(Vector2::new(state.size.0 as f32, state.size.1 as f32));
            let vp = proj * view;
            self.with_materials(m, |mat| mat.set_property("mirrorMatrix", vp))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reflection_across_a_floor() {
        let n = Vector3::y();
        let p = reflect_point(Vector3::new(1.0, 5.0, -2.0), Vector3::new(0.0, 1.0, 0.0), n);
        assert_eq!(p, Vector3::new(1.0, -3.0, -2.0));
        let d = reflect_dir(Vector3::new(0.0, -1.0, 1.0), n);
        assert_eq!(d, Vector3::new(0.0, 1.0, 1.0));
        assert_eq!(rt_size(1920, 1080), (1024, 576));
    }
}
