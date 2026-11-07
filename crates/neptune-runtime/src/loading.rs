//! Background loading (ext): `LoadMeshAsync`, `LoadTextureAsync`,
//! `EntityLoaded`, `TextureLoaded`, `PendingLoads`.
//!
//! The handle is returned at once, so a program can show a loading screen or
//! keep playing. A mesh entity starts empty (it can already be positioned,
//! parented and so on) and the model appears in it when Fyrox has loaded it
//! on its worker threads. A texture starts as a single white pixel and its
//! image is decoded on a thread of its own. Loads are checked every frame.

use std::sync::mpsc::{channel, Receiver, TryRecvError};

use fyrox::{
    core::{algebra::Matrix3, pool::Handle},
    resource::model::{Model, ModelResource},
    scene::node::Node,
};

use crate::texture::TextureInfo;
use crate::world::{pivot_node, Host3D, Kind};

pub struct PendingMesh {
    entity: i32,
    node: Handle<Node>,
    model: ModelResource,
    loader: Option<Matrix3<f32>>,
}

pub struct PendingTexture {
    texture: i32,
    path: String,
    rx: Receiver<Result<TextureInfo, String>>,
}

pub struct PendingImage {
    image: i32,
    path: String,
    frames: (u32, u32, u32, u32),
    rx: Receiver<Result<image::RgbaImage, String>>,
}

pub struct PendingSound {
    sound: i32,
    path: String,
    rx: Receiver<Result<(fyrox::scene::sound::SoundBufferResource, f64), String>>,
}

impl std::fmt::Debug for PendingImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PendingImage({}, {})", self.image, self.path)
    }
}

impl std::fmt::Debug for PendingSound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PendingSound({}, {})", self.sound, self.path)
    }
}

/// `LoadImageAsync`: an empty image now, its pixels (cut into frames like
/// `LoadAnimImage` when a frame size is given) when decoded.
pub fn load_image(h: &mut Host3D, path: &str, frames: (u32, u32, u32, u32)) -> Result<i32, String> {
    if !std::path::Path::new(path).is_file() {
        eprintln!("LoadImageAsync: file not found: {path}");
        return Ok(0);
    }
    h.world.images.push(Some(crate::image2d::Image2D::placeholder()));
    let image = h.world.images.len() as i32;
    let (tx, rx) = channel();
    let file = path.to_string();
    std::thread::spawn(move || {
        let _ = tx.send(crate::assets::pixels(&file));
    });
    h.world.loading_images.push(PendingImage { image, path: path.to_string(), frames, rx });
    Ok(image)
}

/// `LoadSoundAsync`: a silent sound now, the real one when decoded.
pub fn load_sound(h: &mut Host3D, path: &str, spatial: bool) -> Result<i32, String> {
    if !std::path::Path::new(path).is_file() {
        eprintln!("LoadSoundAsync: file not found: {path}");
        return Ok(0);
    }
    let (buffer, rate) = crate::sound::silence();
    let info = crate::sound::SoundInfo { buffer, looping: false, volume: 1.0, pan: 0.0, pitch: 1.0, rate, spatial, path: path.to_string() };
    let sound = h.add_sound(info);
    let (tx, rx) = channel();
    let file = path.to_string();
    std::thread::spawn(move || {
        let _ = tx.send(crate::sound::load(&file, false));
    });
    h.world.loading_sounds.push(PendingSound { sound, path: path.to_string(), rx });
    Ok(sound)
}

impl std::fmt::Debug for PendingMesh {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PendingMesh({})", self.entity)
    }
}

impl std::fmt::Debug for PendingTexture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PendingTexture({}, {})", self.texture, self.path)
    }
}

/// `LoadMeshAsync`: an empty entity now, the model when it has loaded.
pub fn load_mesh(h: &mut Host3D, path: &str, parent: i32) -> Result<i32, String> {
    let rm = h.resources().ok_or("Graphics3D must be called before LoadMeshAsync")?;
    let Some(file) = crate::model::resource_path("LoadMeshAsync", path) else { return Ok(0) };
    let model = rm.request::<Model>(file);
    let loader = crate::model::loader_matrix(h, path);
    let id = h.add_entity(pivot_node(), Kind::Mesh, parent)?;
    let node = h.ent(id)?.node;
    h.world.loading_meshes.push(PendingMesh { entity: id, node, model, loader });
    Ok(id)
}

/// `LoadTextureAsync`: a white texture now, the image when it is decoded.
pub fn load_texture(h: &mut Host3D, path: &str, flags: i32) -> Result<i32, String> {
    if !std::path::Path::new(path).is_file() {
        eprintln!("LoadTextureAsync: file not found: {path}");
        return Ok(0);
    }
    let mut placeholder = TextureInfo::blank(1, 1, flags & !crate::texture::flags::CUBE_MAP)?;
    placeholder.pixels = Some(image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 255, 255, 255])));
    placeholder.commit();
    placeholder.path = path.to_string();
    h.world.textures.push(Some(placeholder));
    let texture = h.world.textures.len() as i32;
    let (tx, rx) = channel();
    let file = path.to_string();
    std::thread::spawn(move || {
        let _ = tx.send(crate::texture::load(&file, flags));
    });
    h.world.loading_textures.push(PendingTexture { texture, path: path.to_string(), rx });
    Ok(texture)
}

/// Finishes the loads that are ready. Called every frame and by the status
/// commands.
pub fn poll(h: &mut Host3D) -> Result<(), String> {
    poll_images_and_sounds(h);
    let mut i = 0;
    while i < h.world.loading_meshes.len() {
        // models for another scene are attached when it is current
        if h.world.loading_meshes[i].model.is_loading() || h.elsewhere(h.world.loading_meshes[i].entity) {
            i += 1;
            continue;
        }
        let p = h.world.loading_meshes.swap_remove(i);
        // the entity may have been freed while loading
        if !h.ent(p.entity).is_ok_and(|e| e.node == p.node) {
            continue;
        }
        if p.model.is_ok() {
            crate::model::attach_model(h, p.entity, &p.model, p.loader)?;
        } else {
            eprintln!("LoadMeshAsync: cannot load {}", p.model.kind());
            h.world.failed_meshes.insert(p.node);
        }
    }
    let mut i = 0;
    while i < h.world.loading_textures.len() {
        let result = match h.world.loading_textures[i].rx.try_recv() {
            Err(TryRecvError::Empty) => {
                i += 1;
                continue;
            }
            Ok(r) => r,
            Err(TryRecvError::Disconnected) => Err("the loading thread stopped".into()),
        };
        let p = h.world.loading_textures.swap_remove(i);
        let Some(Some(cur)) = h.world.textures.get(p.texture as usize - 1) else { continue };
        match result {
            Ok(mut t) => {
                // keep what the program set while the texture was loading
                t.scale_u = cur.scale_u;
                t.scale_v = cur.scale_v;
                t.pos_u = cur.pos_u;
                t.pos_v = cur.pos_v;
                t.rotation = cur.rotation;
                t.blend = cur.blend;
                t.coords = cur.coords;
                h.world.textures[p.texture as usize - 1] = Some(t);
                h.refresh_texture_users(p.texture)?;
            }
            Err(e) => {
                eprintln!("LoadTextureAsync: {e}");
                h.world.failed_textures.insert(p.texture);
            }
        }
    }
    Ok(())
}

/// Finishes image and sound loads that are ready.
fn poll_images_and_sounds(h: &mut Host3D) {
    let mut i = 0;
    while i < h.world.loading_images.len() {
        let result = match h.world.loading_images[i].rx.try_recv() {
            Err(TryRecvError::Empty) => {
                i += 1;
                continue;
            }
            Ok(r) => r,
            Err(TryRecvError::Disconnected) => Err("the loading thread stopped".into()),
        };
        let p = h.world.loading_images.swap_remove(i);
        let filled = result.and_then(|px| match h.world.images.get_mut(p.image as usize - 1).and_then(Option::as_mut) {
            Some(img) => img.fill(px, p.frames, &p.path),
            None => Ok(()),
        });
        if let Err(e) = filled {
            eprintln!("LoadImageAsync: {e}");
            h.world.failed_images.insert(p.image);
        }
    }
    let mut i = 0;
    while i < h.world.loading_sounds.len() {
        let result = match h.world.loading_sounds[i].rx.try_recv() {
            Err(TryRecvError::Empty) => {
                i += 1;
                continue;
            }
            Ok(r) => r,
            Err(TryRecvError::Disconnected) => Err("the loading thread stopped".into()),
        };
        let p = h.world.loading_sounds.swap_remove(i);
        match result {
            Ok((buffer, rate)) => {
                if let Ok(s) = h.sound_mut(p.sound) {
                    s.buffer = buffer;
                    s.rate = rate;
                }
            }
            Err(e) => {
                eprintln!("LoadSoundAsync: {e}");
                h.world.failed_sounds.insert(p.sound);
            }
        }
    }
}

/// `ImageLoaded`: 1 loaded, 0 loading, -1 failed.
pub fn image_state(h: &mut Host3D, img: i32) -> Result<i32, String> {
    poll(h)?;
    h.image(img)?;
    Ok(if h.world.loading_images.iter().any(|p| p.image == img) {
        0
    } else if h.world.failed_images.contains(&img) {
        -1
    } else {
        1
    })
}

/// `SoundLoaded`: 1 loaded, 0 loading, -1 failed.
pub fn sound_state(h: &mut Host3D, s: i32) -> Result<i32, String> {
    poll(h)?;
    h.sound(s)?;
    Ok(if h.world.loading_sounds.iter().any(|p| p.sound == s) {
        0
    } else if h.world.failed_sounds.contains(&s) {
        -1
    } else {
        1
    })
}

/// 1 when the entity's model has loaded (always for other entities), 0 while
/// it is loading, -1 if loading failed.
pub fn entity_state(h: &mut Host3D, e: i32) -> Result<i32, String> {
    poll(h)?;
    let node = h.ent(e)?.node;
    Ok(if h.world.loading_meshes.iter().any(|p| p.node == node) {
        0
    } else if h.world.failed_meshes.contains(&node) {
        -1
    } else {
        1
    })
}

/// Like [`entity_state`] for textures.
pub fn texture_state(h: &mut Host3D, t: i32) -> Result<i32, String> {
    poll(h)?;
    h.texture(t)?;
    Ok(if h.world.loading_textures.iter().any(|p| p.texture == t) {
        0
    } else if h.world.failed_textures.contains(&t) {
        -1
    } else {
        1
    })
}

/// Number of loads still in progress.
pub fn pending(h: &mut Host3D) -> Result<i32, String> {
    poll(h)?;
    let w = &h.world;
    Ok((w.loading_meshes.len() + w.loading_textures.len() + w.loading_images.len() + w.loading_sounds.len()) as i32)
}
