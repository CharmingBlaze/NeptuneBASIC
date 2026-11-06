//! Textures (`LoadTexture`, `EntityTexture`, ...), backed by Fyrox textures.

use std::io::Cursor;

use fyrox::{
    asset::untyped::ResourceKind,
    core::uuid::Uuid,
    resource::texture::{Texture, TextureImportOptions, TextureKind, TextureResource, TextureWrapMode},
};

/// Blitz texture flags.
pub mod flags {
    pub const COLOR: i32 = 1;
    pub const ALPHA: i32 = 2;
    pub const MASKED: i32 = 4;
    pub const MIPMAPPED: i32 = 8;
    pub const CLAMP_U: i32 = 16;
    pub const CLAMP_V: i32 = 32;
    pub const SPHERE_MAP: i32 = 64;
    pub const CUBE_MAP: i32 = 128;
    /// (ext) stored compressed (DXT5) in video memory
    pub const COMPRESSED: i32 = 4096;
}

/// A loaded texture and the Blitz-side state attached to it.
#[derive(Debug, Clone)]
pub struct TextureInfo {
    pub resource: TextureResource,
    pub width: u32,
    pub height: u32,
    pub flags: i32,
    pub scale_u: f32,
    pub scale_v: f32,
    /// `PositionTexture` offset and `RotateTexture` angle (degrees).
    pub pos_u: f32,
    pub pos_v: f32,
    pub rotation: f32,
    /// `TextureBlend` mode (Blitz default 2, multiply).
    pub blend: i32,
    /// `TextureCoords`: UV set 0 or 1.
    pub coords: i32,
    /// `LoadAnimTexture` frame layout: frame size, first frame, frame count
    /// (`frames == 0` for ordinary textures).
    pub frame_w: u32,
    pub frame_h: u32,
    pub first_frame: u32,
    pub frames: u32,
    /// Cube maps (flag 128) are a horizontal strip of six square faces in
    /// Blitz order: left, front, right, back, up, down. `cube_face` is the
    /// face `TextureBuffer` draws to; `cube_mode` 1 reflection, 2 normal, 3 refraction.
    pub cube_face: i32,
    pub cube_mode: i32,
    pub path: String,
    /// CPU copy of the pixels while the texture is drawn to (`TextureBuffer`).
    pub pixels: Option<image::RgbaImage>,
    /// Pixels changed and not yet uploaded.
    pub dirty: bool,
    /// The GPU texture is a plain RGBA8 texture made here, updated in place.
    pub owned: bool,
    /// (ext) a camera draws into it (`CameraToTexture`)
    pub render_target: bool,
}

impl TextureInfo {
    /// Whether materials using this texture need alpha blending.
    pub fn needs_blending(&self) -> bool {
        self.flags & (flags::ALPHA | flags::MASKED) != 0
    }

    /// UV scale and offset that show `frame` of an animated texture.
    pub fn frame_window(&self, frame: i32) -> ((f32, f32), (f32, f32)) {
        if self.frames == 0 || self.frame_w == 0 || self.frame_h == 0 {
            return ((1.0, 1.0), (0.0, 0.0));
        }
        let cols = (self.width / self.frame_w).max(1);
        let idx = self.first_frame + (frame.max(0) as u32).min(self.frames - 1);
        let (sx, sy) = (self.frame_w as f32 / self.width as f32, self.frame_h as f32 / self.height as f32);
        ((sx, sy), ((idx % cols) as f32 * sx, (idx / cols) as f32 * sy))
    }

    /// Whether this texture uses settings only the classic material supports.
    pub fn needs_classic(&self) -> bool {
        if self.frames > 0 || self.flags & flags::CUBE_MAP != 0 {
            return true;
        }
        self.pos_u != 0.0
            || self.pos_v != 0.0
            || self.rotation != 0.0
            || self.blend != 2
            || self.coords != 0
            || self.flags & (flags::SPHERE_MAP | flags::ALPHA | flags::MASKED) != 0
    }

    /// Wraps an existing texture resource with default Blitz settings.
    pub fn from_resource(resource: TextureResource, width: u32, height: u32, flag_bits: i32, path: String) -> Self {
        TextureInfo {
            resource,
            width,
            height,
            flags: flag_bits,
            scale_u: 1.0,
            scale_v: 1.0,
            pos_u: 0.0,
            pos_v: 0.0,
            rotation: 0.0,
            blend: 2,
            coords: 0,
            frame_w: 0,
            frame_h: 0,
            first_frame: 0,
            frames: 0,
            cube_face: 0,
            cube_mode: 1,
            path,
            pixels: None,
            dirty: false,
            owned: false,
            render_target: false,
        }
    }

    /// Face size of a cube map (the strip's height), else the width.
    pub fn face_size(&self) -> u32 {
        if self.flags & flags::CUBE_MAP != 0 {
            self.height
        } else {
            self.width
        }
    }

    /// `CreateTexture`: a blank texture that programs draw into. A cube map
    /// (flag 128) gets six `w`-sized faces.
    pub fn blank(w: u32, h: u32, flag_bits: i32) -> Result<Self, String> {
        if w == 0 || h == 0 || w > 8192 || h > 8192 {
            return Err(format!("texture size {w}x{h} is not allowed"));
        }
        let (w, h) = if flag_bits & flags::CUBE_MAP != 0 { (w * 6, w) } else { (w, h) };
        let px = image::RgbaImage::from_pixel(w, h, image::Rgba([0, 0, 0, 255]));
        let mut t = TextureInfo::from_resource(rgba_texture(&px, flag_bits), w, h, flag_bits, String::new());
        t.pixels = Some(px);
        t.owned = true;
        Ok(t)
    }

    /// Makes sure the CPU pixel copy exists (read from the file the first time).
    pub fn ensure_pixels(&mut self) -> Result<(), String> {
        if self.pixels.is_none() {
            let px = if self.path.is_empty() {
                image::RgbaImage::new(self.width.max(1), self.height.max(1))
            } else {
                image::open(&self.path).map_err(|e| format!("cannot read texture '{}': {e}", self.path))?.to_rgba8()
            };
            self.pixels = Some(px);
        }
        Ok(())
    }

    /// Uploads changed pixels. Returns true if the GPU resource was replaced
    /// (materials using it must be refreshed).
    pub fn commit(&mut self) -> bool {
        self.dirty = false;
        let Some(px) = &self.pixels else { return false };
        let px = if self.flags & flags::MASKED != 0 { masked(px) } else { px.clone() };
        if self.owned {
            let mut t = self.resource.data_ref();
            let mut m = t.modify();
            let bytes = m.data_mut();
            if bytes.len() == px.as_raw().len() {
                bytes.copy_from_slice(px.as_raw());
                return false;
            }
        }
        self.resource = rgba_texture(&px, self.flags);
        self.width = px.width();
        self.height = px.height();
        self.owned = true;
        true
    }
}

/// Loads an image file as a texture, applying Blitz flags:
/// masked (4) makes pure black transparent, clamp U/V (16/32) disable wrapping.
pub fn load(path: &str, flag_bits: i32) -> Result<TextureInfo, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read '{path}': {e}"))?;
    let bytes = if flag_bits & flags::MASKED != 0 { mask_black(&bytes)? } else { bytes };
    let mut opts = TextureImportOptions::default();
    if flag_bits & flags::CLAMP_U != 0 {
        opts.set_s_wrap_mode(TextureWrapMode::ClampToEdge);
    }
    if flag_bits & flags::CLAMP_V != 0 {
        opts.set_t_wrap_mode(TextureWrapMode::ClampToEdge);
    }
    // (ext) compressed in video memory (DXT); DDS files arrive compressed already
    if flag_bits & flags::COMPRESSED != 0 {
        opts.set_compression(fyrox::resource::texture::CompressionOptions::Quality);
    }
    let tex = Texture::load_from_memory(&bytes, opts).map_err(|e| format!("cannot load texture '{path}': {e:?}"))?;
    let (width, height) = match tex.kind() {
        TextureKind::Rectangle { width, height } => (width, height),
        _ => (0, 0),
    };
    Ok(TextureInfo::from_resource(
        TextureResource::new_ok(Uuid::new_v4(), ResourceKind::Embedded, tex),
        width,
        height,
        flag_bits,
        path.to_string(),
    ))
}

/// A 1x1 texture of one colour (RGBA, 0-255).
pub fn solid_texture(px: [u8; 4]) -> TextureResource {
    rgba_texture(&image::RgbaImage::from_pixel(1, 1, image::Rgba(px)), 0)
}

/// A texture of an image, clamped at the edges (sky box faces).
pub fn solid_from(img: &image::RgbaImage) -> TextureResource {
    rgba_texture(img, flags::CLAMP_U | flags::CLAMP_V)
}

/// A plain RGBA8 texture (no mipmaps, so it can be updated in place).
fn rgba_texture(px: &image::RgbaImage, flag_bits: i32) -> TextureResource {
    use fyrox::resource::texture::{TextureMagnificationFilter, TextureMinificationFilter, TexturePixelKind};
    let mut tex = Texture::from_bytes(TextureKind::Rectangle { width: px.width(), height: px.height() }, TexturePixelKind::RGBA8, px.as_raw().clone())
        .expect("RGBA buffer matches its size");
    tex.set_minification_filter(TextureMinificationFilter::Linear);
    tex.set_magnification_filter(TextureMagnificationFilter::Linear);
    if flag_bits & flags::CLAMP_U != 0 {
        tex.set_s_wrap_mode(TextureWrapMode::ClampToEdge);
    }
    if flag_bits & flags::CLAMP_V != 0 {
        tex.set_t_wrap_mode(TextureWrapMode::ClampToEdge);
    }
    TextureResource::new_ok(Uuid::new_v4(), ResourceKind::Embedded, tex)
}

fn masked(px: &image::RgbaImage) -> image::RgbaImage {
    let mut out = px.clone();
    for p in out.pixels_mut() {
        if p[0] == 0 && p[1] == 0 && p[2] == 0 {
            p[3] = 0;
        }
    }
    out
}

/// Re-encodes an image with black pixels made fully transparent.
fn mask_black(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let img = image::load_from_memory(bytes).map_err(|e| format!("cannot decode image: {e}"))?;
    let mut rgba = img.to_rgba8();
    for p in rgba.pixels_mut() {
        if p[0] == 0 && p[1] == 0 && p[2] == 0 {
            p[3] = 0;
        }
    }
    let mut out = Vec::new();
    rgba.write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png).map_err(|e| e.to_string())?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(pixels: &[[u8; 4]], w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbaImage::from_fn(w, h, |x, y| image::Rgba(pixels[(y * w + x) as usize]));
        let mut out = Vec::new();
        img.write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
        out
    }

    #[test]
    fn masking_clears_black_only() {
        let src = png(&[[0, 0, 0, 255], [255, 0, 0, 255]], 2, 1);
        let out = image::load_from_memory(&mask_black(&src).unwrap()).unwrap().to_rgba8();
        assert_eq!(out.get_pixel(0, 0)[3], 0);
        assert_eq!(out.get_pixel(1, 0).0, [255, 0, 0, 255]);
    }

    #[test]
    fn load_reports_size_and_flags() {
        let dir = std::env::temp_dir().join(format!("neptune-tex-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.png");
        std::fs::write(&path, png(&[[255, 255, 255, 255]; 8], 4, 2)).unwrap();
        let t = load(path.to_str().unwrap(), flags::COLOR | flags::MASKED).unwrap();
        assert_eq!((t.width, t.height), (4, 2));
        assert!(t.needs_blending());
        assert!(load("does-not-exist.png", 1).is_err());
        std::fs::remove_dir_all(dir).ok();
    }
}
