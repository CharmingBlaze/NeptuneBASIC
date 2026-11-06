//! PBR material settings (ext): `EntityMetallic`, `EntityRoughness`,
//! `EntityEmissive`, and the texture maps `EntityMetallicMap`,
//! `EntityRoughnessMap`, `EntityEmissiveMap`, `EntityAOMap`,
//! `EntityHeightMap` (parallax mapping) and `EntityLightmap`.
//!
//! They apply to Fyrox's standard material, which entities use unless they
//! need the classic shader (blend modes, `EntityFX`, shininess, several
//! texture layers or fog). Fyrox's shader reads metallic, roughness and
//! emission from textures, so plain values become 1x1 textures.

use fyrox::{
    core::algebra::Vector3,
    material::Material,
    resource::texture::TextureResource,
};

use crate::world::Host3D;

#[derive(Debug, Clone, Default)]
pub struct Pbr {
    pub metallic: Option<f32>,
    pub roughness: Option<f32>,
    /// sRGB colour 0-255 and strength
    pub emissive: Option<([f32; 3], f32)>,
    pub metallic_map: i32,
    pub roughness_map: i32,
    pub emissive_map: i32,
    pub ao_map: i32,
    pub height_map: i32,
    pub parallax: f32,
    pub lightmap: i32,
}

impl Pbr {
    pub fn uses_texture(&self, t: i32) -> bool {
        [self.metallic_map, self.roughness_map, self.emissive_map, self.ao_map, self.height_map, self.lightmap].contains(&t)
    }
}

/// What to bind on a material, resolved from the texture table.
pub struct Resolved {
    binds: Vec<(&'static str, TextureResource)>,
    emission_strength: Option<Vector3<f32>>,
    parallax: Option<f32>,
}

fn value_texture(v: f32) -> TextureResource {
    let b = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    crate::texture::solid_texture([b, b, b, 255])
}

impl Host3D<'_> {
    pub fn pbr_mut(&mut self, e: i32) -> Result<&mut Pbr, String> {
        Ok(self.ent_mut(e)?.pbr.get_or_insert_with(Default::default))
    }

    fn map(&self, t: i32) -> Option<TextureResource> {
        (t != 0).then(|| self.texture(t).ok().map(|t| t.resource.clone())).flatten()
    }

    /// The bindings for an entity's PBR settings (`None` if it has none).
    pub fn resolve_pbr(&self, e: i32) -> Result<Option<Resolved>, String> {
        let Some(p) = self.ent(e)?.pbr.as_deref() else { return Ok(None) };
        let mut binds = Vec::new();
        if let Some(t) = self.map(p.metallic_map).or_else(|| p.metallic.map(value_texture)) {
            binds.push(("metallicTexture", t));
        }
        if let Some(t) = self.map(p.roughness_map).or_else(|| p.roughness.map(value_texture)) {
            binds.push(("roughnessTexture", t));
        }
        let mut emission_strength = None;
        let emissive_map = self.map(p.emissive_map);
        if emissive_map.is_some() || p.emissive.is_some() {
            // the colour scales the map (or a white texel); strength makes it glow
            let (c, s) = p.emissive.unwrap_or(([255.0; 3], 1.0));
            let lin = |v: f32| {
                let v = v / 255.0;
                let l = if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) };
                l * s
            };
            emission_strength = Some(Vector3::new(lin(c[0]), lin(c[1]), lin(c[2])));
            binds.push(("emissionTexture", emissive_map.unwrap_or_else(|| crate::texture::solid_texture([255; 4]))));
        }
        if let Some(t) = self.map(p.ao_map) {
            binds.push(("aoTexture", t));
        }
        let mut parallax = None;
        if let Some(t) = self.map(p.height_map) {
            binds.push(("heightTexture", t));
            parallax = Some(p.parallax);
        }
        if let Some(t) = self.map(p.lightmap) {
            binds.push(("lightmapTexture", t));
        }
        Ok(Some(Resolved { binds, emission_strength, parallax }))
    }
}

impl Resolved {
    pub fn apply(&self, m: &mut Material) {
        for (name, t) in &self.binds {
            m.bind(*name, t.clone());
        }
        if let Some(s) = self.emission_strength {
            m.set_property("emissionStrength", s);
        }
        if let Some(p) = self.parallax {
            m.set_property("parallaxScale", p);
        }
    }
}
