//! Asset caching, reloading and watching (ext): `ReloadAssets`,
//! `WatchAssets`, `AssetsReloaded`, `ImageName`, `SoundName`, `ModelName`.
//!
//! Loading the same file again shares what was decoded the first time:
//! textures share their GPU texture, images their decoded pixels (each image
//! still gets its own textures, as images can be drawn into), sounds their
//! decoded samples. Every load still returns its own handle with its own
//! settings, as in Blitz3D. The cache key includes the file's modification
//! time, so a changed file is read again. Streamed music is not shared.
//!
//! `ReloadAssets` reads every loaded texture, image, sound and model again
//! from its file, keeping each handle's settings; `WatchAssets` does that
//! by itself for files that change while the game runs (quick art changes).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Instant, SystemTime};

use fyrox::scene::sound::SoundBufferResource;

use crate::texture::TextureInfo;
use crate::world::Host3D;

type Key = (PathBuf, Option<SystemTime>);

fn key(path: &str) -> Key {
    let p = std::fs::canonicalize(path).unwrap_or_else(|_| PathBuf::from(path));
    let time = std::fs::metadata(&p).and_then(|m| m.modified()).ok();
    (p, time)
}

fn modified(path: &str) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

static TEXTURES: OnceLock<Mutex<HashMap<(Key, i32), TextureInfo>>> = OnceLock::new();
static PIXELS: OnceLock<Mutex<HashMap<Key, Arc<image::RgbaImage>>>> = OnceLock::new();
static SOUNDS: OnceLock<Mutex<HashMap<Key, (SoundBufferResource, f64)>>> = OnceLock::new();

/// A texture file, decoded once per file version and flags.
pub fn texture(path: &str, flags: i32, load: impl FnOnce() -> Result<TextureInfo, String>) -> Result<TextureInfo, String> {
    let k = (key(path), flags);
    let cache = TEXTURES.get_or_init(Default::default);
    if let Some(t) = cache.lock().unwrap().get(&k) {
        return Ok(t.clone());
    }
    let t = load()?;
    cache.lock().unwrap().insert(k, t.clone());
    Ok(t)
}

/// An image file's pixels, decoded once per file version.
pub fn pixels(path: &str) -> Result<image::RgbaImage, String> {
    let k = key(path);
    let cache = PIXELS.get_or_init(Default::default);
    if let Some(p) = cache.lock().unwrap().get(&k) {
        return Ok((**p).clone());
    }
    let img = image::open(path).map_err(|e| format!("cannot load image '{path}': {e}"))?.to_rgba8();
    cache.lock().unwrap().insert(k, Arc::new(img.clone()));
    Ok(img)
}

/// A sound file's samples, decoded once per file version.
pub fn sound(path: &str, load: impl FnOnce() -> Result<(SoundBufferResource, f64), String>) -> Result<(SoundBufferResource, f64), String> {
    let k = key(path);
    let cache = SOUNDS.get_or_init(Default::default);
    if let Some(s) = cache.lock().unwrap().get(&k) {
        return Ok(s.clone());
    }
    let s = load()?;
    cache.lock().unwrap().insert(k, s.clone());
    Ok(s)
}

/// Forgets everything cached (`ReloadAssets`).
pub fn clear() {
    if let Some(c) = TEXTURES.get() {
        c.lock().unwrap().clear();
    }
    if let Some(c) = PIXELS.get() {
        c.lock().unwrap().clear();
    }
    if let Some(c) = SOUNDS.get() {
        c.lock().unwrap().clear();
    }
}

/// `WatchAssets` state: when files were last checked and their times then.
#[derive(Debug, Default)]
pub struct Watch {
    pub on: bool,
    pub last: Option<Instant>,
    pub times: HashMap<String, Option<SystemTime>>,
    /// Assets reloaded since `AssetsReloaded` last asked.
    pub reloaded: i32,
}

impl Host3D<'_> {
    /// Paths of every loaded asset.
    fn asset_paths(&mut self) -> Vec<String> {
        let mut paths: Vec<String> = Vec::new();
        paths.extend(self.world.textures.iter().flatten().filter(|t| !t.path.is_empty() && !t.render_target).map(|t| t.path.clone()));
        paths.extend(self.world.images.iter().flatten().filter(|i| !i.path.is_empty()).map(|i| i.path.clone()));
        paths.extend(self.world.sounds.iter().flatten().filter(|s| !s.path.is_empty()).map(|s| s.path.clone()));
        paths.extend(self.model_paths().into_iter().map(|(_, p)| p));
        paths.sort();
        paths.dedup();
        paths
    }

    /// (entity, file) of every loaded model.
    fn model_paths(&mut self) -> Vec<(i32, String)> {
        let Some(rm) = self.resources() else { return Vec::new() };
        self.entities()
            .filter_map(|(i, e)| e.model_res.as_ref().filter(|_| !e.adopted).and_then(|r| rm.resource_path(r.as_ref())).map(|p| (i, p.to_string_lossy().to_string())))
            .collect()
    }

    /// Reads assets from their files again (all of them, or those of `only`).
    pub fn reload_assets(&mut self, only: Option<&[String]>) -> Result<i32, String> {
        clear();
        let wanted = |p: &str| only.is_none_or(|o| o.iter().any(|w| w == p));
        let mut count = 0;
        // textures keep their settings; entities using them are refreshed
        let tex: Vec<(i32, String, i32)> = self
            .world
            .textures
            .iter()
            .enumerate()
            .filter_map(|(i, t)| t.as_ref().filter(|t| !t.path.is_empty() && !t.render_target && wanted(&t.path)).map(|t| (i as i32 + 1, t.path.clone(), t.flags)))
            .collect();
        for (id, path, flags) in tex {
            match crate::texture::load(&path, flags) {
                Ok(mut fresh) => {
                    let cur = self.texture(id)?;
                    fresh.scale_u = cur.scale_u;
                    fresh.scale_v = cur.scale_v;
                    fresh.pos_u = cur.pos_u;
                    fresh.pos_v = cur.pos_v;
                    fresh.rotation = cur.rotation;
                    fresh.blend = cur.blend;
                    fresh.coords = cur.coords;
                    fresh.frame_w = cur.frame_w;
                    fresh.frame_h = cur.frame_h;
                    fresh.first_frame = cur.first_frame;
                    fresh.frames = cur.frames;
                    fresh.cube_face = cur.cube_face;
                    fresh.cube_mode = cur.cube_mode;
                    self.world.textures[id as usize - 1] = Some(fresh);
                    self.refresh_texture_users(id)?;
                    count += 1;
                }
                Err(e) => eprintln!("ReloadAssets: {e}"),
            }
        }
        // images keep their frames, handle and scale
        for img in self.world.images.iter_mut().flatten() {
            if !img.path.is_empty() && wanted(&img.path) {
                let path = img.path.clone();
                match img.reload(&path) {
                    Ok(()) => count += 1,
                    Err(e) => eprintln!("ReloadAssets: {e}"),
                }
            }
        }
        // sounds: new channels play the new samples
        for s in self.world.sounds.iter_mut().flatten() {
            if !s.path.is_empty() && wanted(&s.path) {
                match crate::sound::load(&s.path, false) {
                    Ok((buffer, rate)) => {
                        s.buffer = buffer;
                        s.rate = rate;
                        count += 1;
                    }
                    Err(e) => eprintln!("ReloadAssets: {e}"),
                }
            }
        }
        // models: Fyrox reloads the file and updates every instance of it
        if let Some(rm) = self.resources() {
            let mut done: Vec<String> = Vec::new();
            let models: Vec<(i32, String)> = self.model_paths();
            for (e, path) in models {
                if done.contains(&path) || !wanted(&path) {
                    continue;
                }
                if let Some(res) = self.ent(e)?.model_res.clone() {
                    rm.state().reload_resource(res.into_untyped());
                    done.push(path);
                    count += 1;
                }
            }
        }
        self.world.watch.reloaded += count;
        Ok(count)
    }

    /// `WatchAssets`: checks the files of loaded assets about once a second
    /// (called every frame) and reloads those that changed.
    pub fn watch_assets(&mut self) -> Result<(), String> {
        if !self.world.watch.on {
            return Ok(());
        }
        let now = Instant::now();
        if self.world.watch.last.is_some_and(|t| now.duration_since(t).as_millis() < 1000) {
            return Ok(());
        }
        self.world.watch.last = Some(now);
        let mut changed = Vec::new();
        for p in self.asset_paths() {
            let t = modified(&p);
            match self.world.watch.times.insert(p.clone(), t) {
                Some(old) if old != t => changed.push(p),
                _ => {}
            }
        }
        if !changed.is_empty() {
            self.reload_assets(Some(&changed))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixels_are_cached_until_the_file_changes() {
        let dir = std::env::temp_dir().join(format!("neptune-assets-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.png");
        let p = path.to_str().unwrap();
        image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255])).save(&path).unwrap();
        assert_eq!(pixels(p).unwrap().get_pixel(0, 0)[0], 255);
        assert_eq!(pixels(p).unwrap().dimensions(), (2, 2));
        // a newer file is read again (set its time explicitly: some file
        // systems keep whole seconds)
        image::RgbaImage::from_pixel(3, 1, image::Rgba([0, 255, 0, 255])).save(&path).unwrap();
        let later = std::time::SystemTime::now() + std::time::Duration::from_secs(5);
        std::fs::File::options().write(true).open(&path).unwrap().set_modified(later).unwrap();
        assert_eq!(pixels(p).unwrap().dimensions(), (3, 1));
        std::fs::remove_dir_all(dir).ok();
    }
}
