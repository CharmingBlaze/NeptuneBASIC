//! Level of detail (ext): `EntityLOD`.
//!
//! An entity lists child entities with the distance range (from the camera)
//! in which each is shown, for example a detailed model up close and a
//! simple one far away. Each `RenderWorld` shows the levels whose range
//! holds the distance to the first active camera. Hidden entities
//! (`HideEntity`) stay hidden.

use crate::world::{Host3D, Kind};

#[derive(Debug, Clone, Copy)]
pub struct Level {
    pub entity: i32,
    pub near: f32,
    pub far: f32,
}

impl Host3D<'_> {
    /// `EntityLOD`: shows `level` only between `near` and `far` units from
    /// the camera. A far value of 0 means no limit; near = far = -1 removes
    /// the level.
    pub fn entity_lod(&mut self, e: i32, level: i32, near: f32, far: f32) -> Result<(), String> {
        self.ent(level)?;
        let lod = &mut self.ent_mut(e)?.lod;
        lod.retain(|l| l.entity != level);
        if !(near < 0.0 && far < 0.0) {
            lod.push(Level { entity: level, near, far: if far <= 0.0 { f32::MAX } else { far } });
        }
        Ok(())
    }

    /// Shows and hides levels of detail for this frame.
    pub fn update_lods(&mut self) -> Result<(), String> {
        let groups: Vec<(i32, Vec<Level>)> = self.entities().filter(|(_, e)| !e.lod.is_empty()).map(|(i, e)| (i, e.lod.clone())).collect();
        if groups.is_empty() {
            return Ok(());
        }
        let Some(cam) = self.entities().find(|(_, e)| matches!(e.kind, Kind::Camera { mode, .. } if mode != 0) && !e.hidden).map(|(i, _)| i) else {
            return Ok(());
        };
        let cam_pos = self.world_of(cam)?.position();
        for (owner, levels) in groups {
            let d = (self.world_of(owner)?.position() - cam_pos).norm();
            for l in levels {
                let Ok(ent) = self.ent(l.entity) else { continue };
                let (node, hidden) = (ent.node, ent.hidden);
                let show = !hidden && d >= l.near && d < l.far;
                self.graph()?[node].set_visibility(show);
            }
        }
        Ok(())
    }
}
