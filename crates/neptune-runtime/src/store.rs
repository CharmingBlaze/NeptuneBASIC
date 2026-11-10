//! (ext) `StoreEntity` and `RestoreEntity`: taking parts of a large world
//! out of the scene without freeing them.
//!
//! A hidden entity is still a scene node, and the engine visits every node
//! every frame (about half a microsecond each), so a world of tens of
//! thousands of hidden objects is slow even with nothing on screen. A stored
//! entity and its children are lifted out of the scene graph altogether
//! (Fyrox's `SubGraph`): they cost nothing until they are restored, and
//! restoring is far cheaper than building them again.
//!
//! While stored, an entity behaves like one in another scene: every command
//! on it fails with a message (`Host3D::ent`), and the work done each frame
//! passes over it (`Host3D::entities`, `Host3D::elsewhere`). Its node handle
//! must not be used: the graph panics on a node that is out.

use crate::world::{Host3D, Kind};

impl Host3D<'_> {
    /// `StoreEntity`: takes an entity and its children out of the scene.
    pub fn store_entity(&mut self, id: i32) -> Result<(), String> {
        let node = self.ent(id)?.node;
        // the entities that leave: the whole subtree, except what is inside
        // a child that was stored before (it is out already)
        let mut leaving = Vec::new();
        let mut stack = vec![id as u32];
        while let Some(i) = stack.pop() {
            let Some(e) = self.ent_any(i as i32) else { continue };
            if e.stored {
                continue;
            }
            // these have a part that lives outside their own nodes
            let why = if e.phys.is_some() {
                Some("has physics")
            } else if matches!(e.kind, Kind::Camera { .. }) {
                Some("is a camera")
            } else if e.mirror.is_some() {
                Some("is a mirror")
            } else {
                None
            };
            if let Some(why) = why {
                return Err(format!("Entity {i} {why} and cannot be stored (use HideEntity)"));
            }
            leaving.push(i);
            stack.extend(&e.children);
        }
        let sub = self.graph()?.take_reserve_sub_graph(node);
        for i in leaving {
            if let Some(e) = self.world.entities[i as usize - 1].as_mut() {
                e.stored = true;
            }
        }
        self.world.stored.insert(id as u32, sub);
        self.world.pick_gen += 1;
        Ok(())
    }

    /// `RestoreEntity`: puts a stored entity and its children back, as they
    /// were. Does nothing for an entity that is not stored.
    pub fn restore_entity(&mut self, id: i32) -> Result<(), String> {
        let scene = self.world.current_scene;
        let (parent, stored) = match self.ent_any(id) {
            Some(e) if e.scene == scene => (e.parent, e.stored),
            Some(e) => return Err(format!("Entity {id} is in scene {} (use SetScene)", e.scene + 1)),
            None => return Err(format!("Entity {id} does not exist")),
        };
        if !stored {
            return Ok(());
        }
        if !self.world.stored.contains_key(&(id as u32)) {
            return Err(format!("Entity {id} was stored along with its parent: restore the parent"));
        }
        if self.ent_any(parent as i32).is_some_and(|p| p.stored) {
            return Err(format!("Entity {id}'s parent is stored: restore the parent first"));
        }
        let Some(sub) = self.world.stored.remove(&(id as u32)) else { return Ok(()) };
        self.graph()?.put_sub_graph_back(sub);
        let mut stack = vec![id as u32];
        while let Some(i) = stack.pop() {
            // a child that was stored by itself stays out
            if i != id as u32 && self.world.stored.contains_key(&i) {
                continue;
            }
            if let Some(e) = self.world.entities[i as usize - 1].as_mut() {
                e.stored = false;
                stack.extend(&e.children);
            }
        }
        self.world.pick_gen += 1;
        Ok(())
    }

    /// `EntityStored`
    pub fn entity_stored(&self, id: i32) -> Result<bool, String> {
        self.ent_any(id).map(|e| e.stored).ok_or_else(|| format!("Entity {id} does not exist"))
    }

    /// Restores `id` and everything stored inside it, so that the whole
    /// subtree is in the scene (before freeing it).
    pub(crate) fn restore_tree(&mut self, id: i32) -> Result<(), String> {
        if self.world.stored.is_empty() {
            return Ok(());
        }
        // outer ones first: a stored child goes back under its parent's node
        let mut order = Vec::new();
        let mut stack = vec![id as u32];
        while let Some(i) = stack.pop() {
            let Some(e) = self.ent_any(i as i32) else { continue };
            if self.world.stored.contains_key(&i) {
                order.push(i);
            }
            stack.extend(&e.children);
        }
        for i in order {
            self.restore_entity(i as i32)?;
        }
        Ok(())
    }

    /// Restores every stored entity of the current scene (`ClearWorld`).
    pub(crate) fn restore_all(&mut self) -> Result<(), String> {
        let scene = self.world.current_scene;
        let mut roots: Vec<u32> = self.world.stored.keys().copied().filter(|&i| self.ent_any(i as i32).is_some_and(|e| e.scene == scene)).collect();
        // parents before their children
        roots.sort_by_key(|&i| self.depth(i));
        for i in roots {
            self.restore_entity(i as i32)?;
        }
        Ok(())
    }

    fn depth(&self, id: u32) -> usize {
        let mut depth = 0;
        let mut cur = id;
        while let Some(e) = self.ent_any(cur as i32).filter(|e| e.parent != 0) {
            cur = e.parent;
            depth += 1;
        }
        depth
    }
}
