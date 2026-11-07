//! Multiple scenes (ext): `CreateScene`, `SetScene`, `ShowScene`,
//! `HideScene`, `FreeScene`, `EntityScene`.
//!
//! Each scene is its own Fyrox scene with its own entities, cameras,
//! lights, ambient light, fog, physics and sound, so a menu, a level and an
//! interior can all stay loaded. Every visible scene is drawn each frame, in
//! the order they were made: the cameras of scenes after the first draw
//! with a see-through background (as with `CameraClsMode camera, 0`), so
//! they lie over the scenes below, like a HUD or a picture-in-picture view. Commands work in
//! the current scene (`SetScene`); using an entity of another scene is an
//! error. `UpdateWorld` and `RenderWorld` update every visible scene.
//! Scene 1 is the scene `Graphics3D` makes.

use fyrox::{
    core::pool::Handle,
    scene::{node::Node, Scene},
};

use fyrox::resource::texture::{TextureResource, TextureResourceExtension};

use crate::world::Host3D;

#[derive(Debug)]
pub struct SceneSlot {
    pub handle: Handle<Scene>,
    /// The mirror and texture cameras of the scene while it is not current
    /// (the current scene's are in `World::mirror_pool`).
    pub mirror_pool: Vec<Handle<Node>>,
}

impl Host3D<'_> {
    fn scene_slot(&self, s: i32) -> Result<usize, String> {
        usize::try_from(s - 1).ok().filter(|&i| self.world.scenes.get(i).is_some_and(Option::is_some)).ok_or_else(|| format!("Scene {s} does not exist"))
    }

    /// `CreateScene`: a new empty scene (not made current).
    pub fn create_scene(&mut self) -> Result<i32, String> {
        let mut scene = Scene::new();
        let pool = crate::world::setup_scene(&mut scene);
        // Fyrox would copy the scene's frame over the whole window: its
        // cameras draw into their own pictures instead (`CameraClsMode`),
        // and the scene's frame goes to a tiny unused texture
        let opts = scene.rendering_options.get_value_mut_silent();
        opts.render_target = Some(TextureResource::new_render_target(1, 1));
        opts.clear_color = Some(fyrox::core::color::Color::from_rgba(0, 0, 0, 0));
        let handle = self.scene_container()?.add(scene);
        self.world.scenes.push(Some(SceneSlot { handle, mirror_pool: pool }));
        Ok(self.world.scenes.len() as i32)
    }

    /// Makes slot `i` current (no checks).
    fn switch_to(&mut self, i: usize) {
        let cur = self.world.current_scene;
        if cur == i {
            return;
        }
        let pool = std::mem::take(&mut self.world.mirror_pool);
        if let Some(Some(slot)) = self.world.scenes.get_mut(cur) {
            slot.mirror_pool = pool;
        }
        if let Some(Some(slot)) = self.world.scenes.get_mut(i) {
            self.world.mirror_pool = std::mem::take(&mut slot.mirror_pool);
        }
        self.world.current_scene = i;
    }

    /// `SetScene`.
    pub fn set_scene(&mut self, s: i32) -> Result<(), String> {
        let i = self.scene_slot(s)?;
        self.switch_to(i);
        Ok(())
    }

    /// `ShowScene` / `HideScene`: a hidden scene is neither drawn nor updated.
    pub fn show_scene(&mut self, s: i32, on: bool) -> Result<(), String> {
        let handle = self.world.scenes[self.scene_slot(s)?].as_ref().map(|x| x.handle).unwrap_or_default();
        let scene = self.scene_container()?.try_get_mut(handle).map_err(|_| format!("Scene {s} does not exist"))?;
        scene.enabled.set_value_and_mark_modified(on);
        Ok(())
    }

    pub fn scene_visible(&mut self, s: i32) -> Result<bool, String> {
        let handle = self.world.scenes[self.scene_slot(s)?].as_ref().map(|x| x.handle).unwrap_or_default();
        Ok(self.scene_container()?.try_get(handle).is_ok_and(|sc| *sc.enabled))
    }

    /// `FreeScene`: the scene and all its entities (not scene 1).
    pub fn free_scene(&mut self, s: i32) -> Result<(), String> {
        let i = self.scene_slot(s)?;
        if i == 0 {
            return Err("scene 1 cannot be freed".into());
        }
        if self.world.current_scene == i {
            self.switch_to(0);
        }
        let gone: Vec<i32> = self.world.entities.iter().enumerate().filter(|(_, e)| e.as_ref().is_some_and(|e| e.scene == i)).map(|(n, _)| n as i32 + 1).collect();
        for &e in &gone {
            self.world.entities[e as usize - 1] = None;
        }
        for s in self.world.surfaces.iter_mut() {
            if s.as_ref().is_some_and(|x| gone.contains(&x.entity)) {
                *s = None;
            }
        }
        self.world.agents.retain(|a| a.as_ref().is_none_or(|a| !gone.contains(&a.entity)));
        self.world.prefab_of.retain(|e, _| !gone.contains(e));
        if let Some(slot) = self.world.scenes[i].take() {
            self.scene_container()?.remove(slot.handle);
        }
        Ok(())
    }

    /// Indexes of the visible scenes (the current one always counts).
    pub fn visible_scenes(&self) -> Vec<usize> {
        let current = self.world.current_scene;
        (0..self.world.scenes.len())
            .filter(|&i| {
                i == current
                    || self.world.scenes[i].as_ref().is_some_and(|slot| self.scenes_ref().and_then(|c| c.try_get(slot.handle).ok()).is_some_and(|sc| *sc.enabled))
            })
            .collect()
    }

    /// `EntityScene`: the scene an entity is in.
    pub fn entity_scene(&self, e: i32) -> Result<i32, String> {
        self.ent_any(e).map(|x| x.scene as i32 + 1).ok_or_else(|| format!("Entity {e} does not exist"))
    }

    /// Runs `f` in every visible scene in turn, then goes back to the current one.
    pub fn in_each_scene(&mut self, mut f: impl FnMut(&mut Self) -> Result<(), String>) -> Result<(), String> {
        let current = self.world.current_scene;
        let visible = self.visible_scenes();
        let mut result = Ok(());
        for i in visible {
            self.switch_to(i);
            if let Err(e) = f(self) {
                result = Err(e);
                break;
            }
        }
        self.switch_to(current);
        result
    }
}
