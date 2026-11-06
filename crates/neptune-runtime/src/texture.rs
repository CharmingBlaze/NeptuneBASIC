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
    pub path: String,
}

impl TextureInfo {
    /// Whether materials using this texture need alpha blending.
    pub fn needs_blending(&self) -> bool {
        self.flags & (flags::ALPHA | flags::MASKED) != 0
    }

    /// Whether this texture uses settings only the classic material supports.
    pub fn needs_classic(&self) -> bool {
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
            path,
        }
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
