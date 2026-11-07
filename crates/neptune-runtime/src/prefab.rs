//! Prefabs (ext): reusable scenes placed many times.
//!
//! `LoadPrefab` reads a model or saved scene file once (`.rgs` from
//! `SaveScene`/`SaveEntity` or the Fyrox editor, glTF, FBX); every
//! `InstancePrefab` places a new copy that shares its meshes, materials and
//! textures (Fyrox's model instancing), so a forest of one tree file costs
//! the tree once. `ReloadAssets` updates every copy. `CreatePrefab` turns an
//! entity built in the program into a prefab: it is hidden and kept as the
//! original, and instances are copies of it (`CopyEntity`).

use fyrox::resource::model::{Model, ModelResource};

use crate::world::Host3D;

#[derive(Debug, Clone)]
pub enum Prefab {
    File { model: ModelResource, loader: Option<fyrox::core::algebra::Matrix3<f32>>, path: String },
    /// A hidden entity copied for each instance.
    Entity(i32),
}

impl Host3D<'_> {
    fn prefab(&self, p: i32) -> Result<&Prefab, String> {
        usize::try_from(p - 1).ok().and_then(|i| self.world.prefabs.get(i)).and_then(Option::as_ref).ok_or_else(|| format!("Prefab {p} does not exist"))
    }

    fn add_prefab(&mut self, p: Prefab) -> i32 {
        self.world.prefabs.push(Some(p));
        self.world.prefabs.len() as i32
    }

    fn set_hidden(&mut self, e: i32, hidden: bool) -> Result<(), String> {
        self.ent_mut(e)?.hidden = hidden;
        let n = self.ent(e)?.node;
        self.graph()?[n].set_visibility(!hidden);
        Ok(())
    }

    /// `LoadPrefab`: 0 if the file cannot be loaded (like `LoadMesh`).
    pub fn load_prefab(&mut self, path: &str) -> Result<i32, String> {
        let rm = self.resources().ok_or("Graphics3D must be called before LoadPrefab")?;
        let Some(file) = crate::model::resource_path("LoadPrefab", path) else { return Ok(0) };
        let model = match fyrox::core::futures::executor::block_on(rm.request::<Model>(file)) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("LoadPrefab: cannot load {path}: {e:?}");
                return Ok(0);
            }
        };
        let loader = crate::model::loader_matrix(self, path);
        Ok(self.add_prefab(Prefab::File { model, loader, path: path.replace('\\', "/") }))
    }

    /// `CreatePrefab`: the entity becomes a hidden original for copies.
    pub fn create_prefab(&mut self, e: i32) -> Result<i32, String> {
        self.ent(e)?;
        self.set_hidden(e, true)?;
        Ok(self.add_prefab(Prefab::Entity(e)))
    }

    /// `InstancePrefab`: a new entity placed from the prefab.
    pub fn instance_prefab(&mut self, p: i32, parent: i32) -> Result<i32, String> {
        let e = match self.prefab(p)?.clone() {
            Prefab::File { model, loader, .. } => crate::model::instantiate(self, &model, parent, loader)?,
            Prefab::Entity(original) => {
                let copy = self.copy_entity(original, parent)?;
                self.set_hidden(copy, false)?;
                copy
            }
        };
        self.world.prefab_of.insert(e, p);
        Ok(e)
    }

    pub fn prefab_name(&self, p: i32) -> Result<String, String> {
        Ok(match self.prefab(p)? {
            Prefab::File { path, .. } => path.clone(),
            Prefab::Entity(e) => self.ent(*e).map(|ent| ent.name.clone()).unwrap_or_default(),
        })
    }

    /// `FreePrefab`: placed copies stay; an entity prefab's original is freed.
    pub fn free_prefab(&mut self, p: i32) -> Result<(), String> {
        if let Prefab::Entity(e) = self.prefab(p)?.clone() {
            if self.ent(e).is_ok() {
                self.free_entity(e)?;
            }
        }
        self.world.prefabs[p as usize - 1] = None;
        self.world.prefab_of.retain(|_, q| *q != p);
        Ok(())
    }
}
