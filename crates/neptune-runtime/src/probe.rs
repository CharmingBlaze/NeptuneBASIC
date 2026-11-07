//! Reflection probes (ext): `CreateReflectionProbe`, `ProbeRange`,
//! `ProbeMode`, `UpdateProbe`, `ProbeOffset`, `ProbeTexture`.
//!
//! A probe takes a picture of the scene in all six directions (a cube map)
//! from its centre. While the camera is inside the probe's box, Fyrox uses
//! that picture for the reflections and the ambient light of every surface
//! with the standard (PBR) material, so shiny things reflect the room or
//! the landscape around them instead of only the sky. The box is one unit
//! across and is sized with `ScaleEntity`, like a decal.
//!
//! By default a probe takes its picture once, on the next frame
//! (`UpdateProbe` takes it again); `ProbeMode probe, 1` retakes it every
//! frame, which draws the scene six more times. `ProbeTexture` gives the
//! picture as a cube map texture for custom shaders (`ShaderTexture`,
//! `ShaderSetCubeMap`); an entity given one with `ShaderTexture` is left
//! out of the probes' pictures, so it never draws into the picture it reads.
//!
//! Reflections need the scene's lighting to come from the environment
//! (`EnvironmentLighting 1`, the sky box or the probe) instead of the flat
//! `AmbientLight` colour; creating a probe switches that on.

use fyrox::core::algebra::Vector3;
use fyrox::scene::base::BaseBuilder;
use fyrox::scene::collider::BitMask;
use fyrox::scene::probe::{ReflectionProbe, ReflectionProbeBuilder, UpdateMode};
use fyrox::scene::EnvironmentLightingSource;

use crate::texture::{flags, TextureInfo};
use crate::world::{Host3D, Kind};

/// Render mask bit of entities that probes do not see.
const HIDDEN_FROM_PROBES: u32 = 1 << 31;

/// `CreateReflectionProbe`: a probe whose picture is `resolution` pixels
/// square per face.
pub fn create(h: &mut Host3D, resolution: i32, parent: i32) -> Result<i32, String> {
    let node = ReflectionProbeBuilder::new(BaseBuilder::new())
        .with_resolution(resolution.clamp(16, 2048) as usize)
        .with_z_near(0.1)
        .with_z_far(1000.0)
        .with_update_mode(UpdateMode::Once)
        .build_node();
    let id = h.add_entity(node, Kind::Pivot, parent)?;
    // the probe sees everything except what was hidden from probes
    let node = h.ent(id)?.node;
    h.graph()?[node].render_mask.set_value_and_mark_modified(BitMask(!HIDDEN_FROM_PROBES));
    // reflections need lighting from the environment (EnvironmentLighting 1)
    let opts = h.scene()?.rendering_options.get_value_mut_silent();
    if opts.environment_lighting_source == EnvironmentLightingSource::AmbientColor {
        opts.environment_lighting_source = EnvironmentLightingSource::SkyBox;
    }
    Ok(id)
}

/// Texture path that marks a probe's picture.
fn marker(probe: i32) -> String {
    format!("<reflection probe {probe}>")
}

impl Host3D<'_> {
    /// `EnvironmentLighting`: 0 lights everything with the `AmbientLight`
    /// colour (Blitz3D, no reflections), 1 takes ambient light and
    /// reflections from the sky box or the reflection probe around the
    /// camera, scaled by `brightness`.
    pub fn environment_lighting(&mut self, mode: i32, brightness: f32) -> Result<(), String> {
        let source = match mode {
            0 => EnvironmentLightingSource::AmbientColor,
            1 => EnvironmentLightingSource::SkyBox,
            _ => return Err(format!("EnvironmentLighting: mode must be 0 (ambient colour) or 1 (sky box and probes), not {mode}")),
        };
        let opts = self.scene()?.rendering_options.get_value_mut_silent();
        opts.environment_lighting_source = source;
        opts.environment_lighting_brightness = brightness.max(0.0);
        Ok(())
    }

    fn probe_mut(&mut self, e: i32) -> Result<&mut ReflectionProbe, String> {
        let node = self.ent(e)?.node;
        self.graph()?[node].cast_mut::<ReflectionProbe>().ok_or_else(|| format!("Entity {e} is not a reflection probe"))
    }

    /// `ProbeRange`: near and far distances of what the probe sees.
    pub fn probe_range(&mut self, e: i32, near: f32, far: f32) -> Result<(), String> {
        let p = self.probe_mut(e)?;
        p.z_near.set_value_and_mark_modified(near.max(0.001));
        p.z_far.set_value_and_mark_modified(far.max(near + 0.01));
        p.force_update();
        Ok(())
    }

    /// `ProbeMode`: 0 takes the picture once, 1 every frame.
    pub fn probe_mode(&mut self, e: i32, mode: i32) -> Result<(), String> {
        let mode = match mode {
            0 => UpdateMode::Once,
            1 => UpdateMode::EachFrame,
            _ => return Err(format!("ProbeMode: mode must be 0 (once) or 1 (every frame), not {mode}")),
        };
        let p = self.probe_mut(e)?;
        p.update_mode.set_value_and_mark_modified(mode);
        p.force_update();
        Ok(())
    }

    /// `UpdateProbe`: takes the picture again on the next frame.
    pub fn update_probe(&mut self, e: i32) -> Result<(), String> {
        self.probe_mut(e)?.force_update();
        Ok(())
    }

    /// `ProbeOffset`: where the picture is taken, relative to the probe's
    /// position (world directions; the probe's rotation is ignored).
    pub fn probe_offset(&mut self, e: i32, x: f32, y: f32, z: f32) -> Result<(), String> {
        let p = self.probe_mut(e)?;
        p.rendering_position.set_value_and_mark_modified(Vector3::new(-x, y, z));
        p.force_update();
        Ok(())
    }

    /// `ProbeTexture`: the probe's picture as a cube map texture (the same
    /// texture every time).
    pub fn probe_texture(&mut self, e: i32) -> Result<i32, String> {
        let (target, size) = {
            let p = self.probe_mut(e)?;
            (p.render_target().clone(), p.resolution() as u32)
        };
        let name = marker(e);
        if let Some(i) = self.world.textures.iter().position(|t| t.as_ref().is_some_and(|t| t.path == name)) {
            return Ok(i as i32 + 1);
        }
        let mut t = TextureInfo::from_resource(target, size, size, flags::CUBE_MAP, name);
        t.render_target = true;
        self.world.textures.push(Some(t));
        Ok(self.world.textures.len() as i32)
    }

    /// Fyrox works out the lighting from a probe's picture only on frames
    /// when some probe takes one, so when a camera moves into or out of a
    /// probe's box the probes take their pictures again. Called before
    /// rendering.
    pub fn watch_probes(&mut self) -> Result<(), String> {
        use fyrox::graph::SceneGraph;
        let scene = self.world.current_scene;
        let before = self.world.probes_around_cameras.get(&scene).cloned();
        let graph = self.graph()?;
        let probes: Vec<_> = graph.pair_iter().filter(|(_, n)| n.cast::<ReflectionProbe>().is_some()).map(|(h, _)| h).collect();
        if probes.is_empty() && before.is_none() {
            return Ok(());
        }
        let around: Vec<Option<_>> = graph
            .linear_iter()
            .filter(|n| n.cast::<fyrox::scene::camera::Camera>().is_some_and(|c| c.is_enabled()) && n.is_globally_enabled())
            .map(|cam| {
                let p = cam.global_position();
                probes.iter().copied().find(|h| graph[*h].world_bounding_box().is_contains_point(p))
            })
            .collect();
        if before.as_ref() != Some(&around) {
            for h in &probes {
                if let Some(p) = graph[*h].cast_mut::<ReflectionProbe>() {
                    p.force_update();
                }
            }
            self.world.probes_around_cameras.insert(scene, around);
        }
        Ok(())
    }

    /// Whether texture `t` is a probe's picture.
    pub fn is_probe_texture(&self, t: i32) -> bool {
        self.texture(t).is_ok_and(|t| t.path.starts_with("<reflection probe "))
    }

    /// Leaves an entity's meshes out of the probes' pictures.
    pub fn hide_from_probes(&mut self, e: i32) -> Result<(), String> {
        let nodes = self.entity_mesh_nodes(e)?;
        let graph = self.graph()?;
        for n in nodes {
            // cameras see every bit, probes every bit but this one
            graph[n].render_mask.set_value_and_mark_modified(BitMask(HIDDEN_FROM_PROBES));
        }
        Ok(())
    }
}
