//! Camera rendering settings (ext): `CameraExposure`, `CameraColorGrading`
//! and `CameraToTexture`.
//!
//! Cameras render in HDR and tone-map to the screen. `CameraExposure` picks
//! a fixed exposure or automatic exposure (eye adaptation).
//! `CameraColorGrading` grades a camera's image through a colour lookup
//! table: a 256x16 image of 16 slices of 16x16 (red across a slice, green
//! down, blue from slice to slice), the common layout of LUT files; it
//! replaces `SetGamma` for that camera. `CameraToTexture` makes a camera
//! draw into a texture (security monitors, mini-maps, portals).

use fyrox::{
    asset::untyped::ResourceKind,
    core::{futures::executor::block_on, uuid::Uuid},
    resource::texture::{Texture, TextureKind, TexturePixelKind, TextureResource, TextureResourceExtension},
    core::algebra::{UnitQuaternion, Vector3},
    scene::{
        base::BaseBuilder,
        camera::{Camera, CameraBuilder, ColorGradingLut, Exposure},
    },
};

use fyrox::graph::SceneGraph;

use crate::world::{Host3D, Kind};

/// A camera drawing into a texture (`CameraToTexture`).
#[derive(Debug, Clone)]
pub struct TextureCamera {
    /// The program's camera entity.
    pub camera: i32,
    /// The start-up camera that takes the picture.
    pub pool: fyrox::core::pool::Handle<fyrox::scene::node::Node>,
    pub texture: i32,
    pub rt: TextureResource,
    pub frames: u32,
}

impl Host3D<'_> {
    fn camera_node(&mut self, cam: i32) -> Result<&mut Camera, String> {
        let e = self.ent(cam)?;
        if !matches!(e.kind, Kind::Camera { .. }) {
            return Err(format!("Entity {cam} is not a camera"));
        }
        let node = e.node;
        self.graph()?[node].cast_mut::<Camera>().ok_or_else(|| "camera node missing".to_string())
    }

    /// `CameraExposure`: mode 0 = fixed `exposure`, 1 = automatic between
    /// the given scene brightness limits.
    pub fn camera_exposure(&mut self, cam: i32, mode: i32, exposure: f32, min: f32, max: f32) -> Result<(), String> {
        let e = match mode {
            0 => Exposure::Manual(exposure.max(0.0)),
            1 => Exposure::Auto { min_luminance: min.max(0.0), max_luminance: max.max(min) },
            _ => return Err(format!("CameraExposure: mode must be 0 (fixed) or 1 (automatic), not {mode}")),
        };
        self.camera_node(cam)?.set_exposure(e);
        Ok(())
    }

    /// `CameraColorGrading`: grades the camera through a LUT image ("" turns
    /// it off and gives the camera back to `SetGamma`).
    pub fn camera_color_grading(&mut self, cam: i32, file: &str) -> Result<(), String> {
        let node = self.ent(cam)?.node;
        if file.is_empty() {
            self.world.graded_cameras.remove(&node);
            self.camera_node(cam)?.set_color_grading_enabled(false);
            // SetGamma's table, if any, applies again
            self.world.gamma_dirty = true;
            return Ok(());
        }
        let img = image::open(file).map_err(|e| format!("CameraColorGrading: cannot read '{file}': {e}"))?.to_rgb8();
        if img.dimensions() != (256, 16) {
            return Err(format!("CameraColorGrading: '{file}' is {}x{}; a colour table is 256x16", img.width(), img.height()));
        }
        let tex = Texture::from_bytes(TextureKind::Rectangle { width: 256, height: 16 }, TexturePixelKind::RGB8, img.into_raw())
            .ok_or("bad colour table size")?;
        let lut = block_on(ColorGradingLut::new(TextureResource::new_ok(Uuid::new_v4(), ResourceKind::Embedded, tex)))
            .map_err(|e| format!("CameraColorGrading: {e:?}"))?;
        let c = self.camera_node(cam)?;
        let _ = c.set_color_grading_lut(Some(lut));
        c.set_color_grading_enabled(true);
        self.world.graded_cameras.insert(node);
        Ok(())
    }

    /// `CameraToTexture`: the camera draws into `texture` (0 = the screen
    /// again). The texture keeps its size.
    ///
    /// Fyrox draws cameras in creation order, and a texture must be drawn
    /// before a camera shows it. So the picture is taken by one of the
    /// cameras made at start-up (shared with mirrors), which follows the
    /// program's camera every frame; the program's camera stops drawing to
    /// the screen.
    pub fn camera_to_texture(&mut self, cam: i32, texture: i32) -> Result<(), String> {
        self.camera_node(cam)?;
        // undo an earlier CameraToTexture of this camera
        if let Some(i) = self.world.texture_cameras.iter().position(|t| t.camera == cam) {
            let old = self.world.texture_cameras.remove(i);
            if let Some(c) = self.graph()?[old.pool].cast_mut::<Camera>() {
                c.set_render_target(None);
                c.set_enabled(false);
            }
            self.world.mirror_pool.push(old.pool);
        }
        let on = matches!(self.ent(cam)?.kind, Kind::Camera { mode, .. } if mode != 0);
        if texture == 0 {
            self.camera_node(cam)?.set_enabled(on);
            return Ok(());
        }
        let (w, h) = {
            let t = self.texture(texture)?;
            (t.width.max(1), t.height.max(1))
        };
        let rt = TextureResource::new_render_target(w, h);
        let pool = match self.world.mirror_pool.pop() {
            Some(c) => c,
            // more than the start-up cameras: this one draws a frame late
            None => self.graph()?.add_node(CameraBuilder::new(BaseBuilder::new().with_name("TextureCamera")).build_node()),
        };
        if let Some(c) = self.graph()?[pool].cast_mut::<Camera>() {
            c.set_render_target(Some(rt.clone()));
            c.set_enabled(on);
        }
        self.camera_node(cam)?.set_enabled(false);
        self.world.texture_cameras.push(TextureCamera { camera: cam, pool, texture, rt, frames: 0 });
        Ok(())
    }

    /// Moves the texture cameras to their program cameras and gives
    /// textures their pictures once drawn.
    pub fn update_render_targets(&mut self) -> Result<(), String> {
        let list = std::mem::take(&mut self.world.texture_cameras);
        let mut keep = Vec::new();
        for mut t in list {
            let Ok(ent) = self.ent(t.camera) else {
                // the camera was freed
                if let Some(c) = self.graph()?[t.pool].cast_mut::<Camera>() {
                    c.set_render_target(None);
                    c.set_enabled(false);
                }
                self.world.mirror_pool.push(t.pool);
                continue;
            };
            let (node, on) = (ent.node, matches!(ent.kind, Kind::Camera { mode, .. } if mode != 0) && !ent.hidden);
            let cw = self.world_of(t.camera)?;
            let mirror = |v: Vector3<f32>| Vector3::new(-v.x, v.y, v.z);
            let (pos, look, up) = (mirror(cw.position()), mirror(cw.rot * Vector3::z()), mirror(cw.rot * Vector3::y()));
            let graph = self.graph()?;
            let (projection, exposure) = match graph[node].cast::<Camera>() {
                Some(c) => (c.projection().clone(), c.exposure()),
                None => continue,
            };
            let pool = &mut graph[t.pool];
            let tr = pool.local_transform_mut();
            tr.set_position(pos);
            tr.set_rotation(UnitQuaternion::face_towards(&look, &up));
            if let Some(c) = pool.cast_mut::<Camera>() {
                c.set_projection(projection);
                c.set_exposure(exposure);
                c.set_enabled(on);
            }
            t.frames = t.frames.saturating_add(1);
            if t.frames == 2 {
                // the camera has drawn into it: show the picture
                if let Ok(tex) = self.texture_mut(t.texture) {
                    tex.resource = t.rt.clone();
                    // the GPU draws into it now; pixel commands no longer see it
                    tex.pixels = None;
                    tex.owned = false;
                    tex.render_target = true;
                    self.refresh_texture_users(t.texture)?;
                }
            }
            keep.push(t);
        }
        self.world.texture_cameras = keep;
        Ok(())
    }
}
