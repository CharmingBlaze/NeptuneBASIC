//! Decals (ext): `CreateDecal`.
//!
//! A decal projects its texture onto the scene inside its box, which is one
//! unit across and is sized with `ScaleEntity` (Y is the projection depth,
//! so a decal facing down onto the floor needs no rotation). `EntityColor`,
//! `EntityAlpha`, `EntityTexture` and `EntityNormalMap` work on decals.
//! Decals are drawn onto entities that use the standard (deferred)
//! material.

use fyrox::scene::{base::BaseBuilder, decal::{Decal, DecalBuilder}};

use crate::world::{Host3D, Kind};

/// `CreateDecal`: a new decal entity showing `texture`.
pub fn create(h: &mut Host3D, texture: i32, parent: i32) -> Result<i32, String> {
    let mut b = DecalBuilder::new(BaseBuilder::new());
    if texture != 0 {
        b = b.with_diffuse_texture(h.texture(texture)?.resource.clone());
    }
    let id = h.add_entity(b.build_node(), Kind::Pivot, parent)?;
    h.ent_mut(id)?.textures[0] = texture;
    Ok(id)
}

impl Host3D<'_> {
    /// Applies colour, alpha and textures to a decal. False if the entity is
    /// not one.
    pub fn refresh_decal(&mut self, id: i32) -> Result<bool, String> {
        let e = self.ent(id)?;
        let (node, color, tex, normal) = (e.node, e.color, e.textures[0], e.normal_map);
        if self.graph()?[node].cast::<Decal>().is_none() {
            return Ok(false);
        }
        let tex = if tex != 0 { Some(self.texture(tex)?.resource.clone()) } else { None };
        let normal = if normal != 0 { Some(self.texture(normal)?.resource.clone()) } else { None };
        let d = self.graph()?[node].cast_mut::<Decal>().unwrap();
        d.set_color(color);
        d.set_diffuse_texture(tex);
        d.set_normal_texture(normal);
        Ok(true)
    }
}
