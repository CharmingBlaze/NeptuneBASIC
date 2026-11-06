//! Drawing buffers (`SetBuffer`, `ImageBuffer`, `TextureBuffer`,
//! `ReadPixel`/`WritePixel`, `CopyRect`, ...).
//!
//! The back buffer is the screen layer drawn by the 2D overlay. Image and
//! texture buffers are CPU pixel copies: 2D commands aimed at them
//! (`Plot`, `Line`, `Rect`, `Oval`, `Cls`, `DrawImage`, `WritePixel`, ...)
//! are rasterised in software, and the changed pixels are uploaded to the
//! GPU at `UnlockBuffer`, `RenderWorld` or `Flip`, whichever comes first.
//! `Text` is rasterised with the current font. Reading pixels back from the
//! screen is not supported.
//!
//! Buffer handles: 1 = back buffer, 2 = front buffer,
//! `IMAGE_TAG | frame << 20 | image`, `TEXTURE_TAG | frame << 20 | texture`.

use image::{Rgba, RgbaImage};

use crate::world::Host3D;

pub const BACK: i32 = 1;
pub const FRONT: i32 = 2;
const IMAGE_TAG: i32 = 0x1000_0000;
const TEXTURE_TAG: i32 = 0x2000_0000;
const TAG_MASK: i32 = 0x7000_0000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Buffer {
    Screen,
    Image { id: i32, frame: u32 },
    Texture { id: i32 },
}

pub fn image_buffer(id: i32, frame: i32) -> i32 {
    IMAGE_TAG | ((frame.clamp(0, 255)) << 20) | id
}

pub fn texture_buffer(id: i32) -> i32 {
    TEXTURE_TAG | id
}

pub fn decode(b: i32) -> Result<Buffer, String> {
    match b & TAG_MASK {
        0 if b == BACK || b == FRONT => Ok(Buffer::Screen),
        IMAGE_TAG => Ok(Buffer::Image { id: b & 0xF_FFFF, frame: ((b >> 20) & 0xFF) as u32 }),
        TEXTURE_TAG => Ok(Buffer::Texture { id: b & 0xF_FFFF }),
        _ => Err(format!("{b} is not a buffer")),
    }
}

/// Blitz colours are `$AARRGGBB` ints.
pub fn argb_to_rgba(c: i32) -> Rgba<u8> {
    let c = c as u32;
    Rgba([(c >> 16) as u8, (c >> 8) as u8, c as u8, (c >> 24) as u8])
}

pub fn rgba_to_argb(p: Rgba<u8>) -> i32 {
    ((p[3] as u32) << 24 | (p[0] as u32) << 16 | (p[1] as u32) << 8 | p[2] as u32) as i32
}

/// A clipped drawing window into a pixel buffer (one frame of an image).
pub struct Target<'a> {
    pub pixels: &'a mut RgbaImage,
    pub x0: i32,
    pub y0: i32,
    pub w: i32,
    pub h: i32,
}

impl Target<'_> {
    pub fn plot(&mut self, x: i32, y: i32, c: Rgba<u8>) {
        if x >= 0 && y >= 0 && x < self.w && y < self.h {
            self.pixels.put_pixel((self.x0 + x) as u32, (self.y0 + y) as u32, c);
        }
    }

    pub fn get(&self, x: i32, y: i32) -> Rgba<u8> {
        if x >= 0 && y >= 0 && x < self.w && y < self.h {
            *self.pixels.get_pixel((self.x0 + x) as u32, (self.y0 + y) as u32)
        } else {
            Rgba([0, 0, 0, 0])
        }
    }

    pub fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, solid: bool, c: Rgba<u8>) {
        for yy in y..y + h {
            for xx in x..x + w {
                if solid || yy == y || yy == y + h - 1 || xx == x || xx == x + w - 1 {
                    self.plot(xx, yy, c);
                }
            }
        }
    }

    pub fn line(&mut self, x1: i32, y1: i32, x2: i32, y2: i32, c: Rgba<u8>) {
        let (dx, dy) = ((x2 - x1).abs(), -(y2 - y1).abs());
        let (sx, sy) = (if x1 < x2 { 1 } else { -1 }, if y1 < y2 { 1 } else { -1 });
        let (mut x, mut y, mut err) = (x1, y1, dx + dy);
        loop {
            self.plot(x, y, c);
            if x == x2 && y == y2 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    pub fn oval(&mut self, x: i32, y: i32, w: i32, h: i32, solid: bool, c: Rgba<u8>) {
        if w <= 0 || h <= 0 {
            return;
        }
        let (rx, ry) = (w as f32 / 2.0, h as f32 / 2.0);
        let (cx, cy) = (x as f32 + rx, y as f32 + ry);
        let inside = |px: i32, py: i32| {
            let (dx, dy) = ((px as f32 + 0.5 - cx) / rx, (py as f32 + 0.5 - cy) / ry);
            dx * dx + dy * dy <= 1.0
        };
        for py in y..y + h {
            for px in x..x + w {
                if inside(px, py) && (solid || !inside(px - 1, py) || !inside(px + 1, py) || !inside(px, py - 1) || !inside(px, py + 1)) {
                    self.plot(px, py, c);
                }
            }
        }
    }

    /// Blends `color` over a pixel with coverage `a` (0..255).
    pub fn blend(&mut self, x: i32, y: i32, color: Rgba<u8>, a: u8) {
        if a == 0 {
            return;
        }
        let d = self.get(x, y);
        let k = a as u32;
        let mix = |s: u8, d: u8| ((s as u32 * k + d as u32 * (255 - k)) / 255) as u8;
        let alpha = (k + d[3] as u32 * (255 - k) / 255).min(255) as u8;
        self.plot(x, y, Rgba([mix(color[0], d[0]), mix(color[1], d[1]), mix(color[2], d[2]), alpha]));
    }

    /// Draws `text` with its top-left corner at `x, y` (Blitz `Text` placement).
    pub fn text(&mut self, font: &crate::draw2d::FontInfo, x: i32, y: i32, text: &str, color: Rgba<u8>) {
        let f = font.resource.data_ref();
        let Some(inner) = f.inner.as_ref() else { return };
        let baseline = y as f32 + f.ascender(font.size);
        let mut pen = x as f32;
        let mut prev: Option<char> = None;
        for c in text.chars() {
            if let Some(p) = prev {
                pen += f.horizontal_kerning(font.size, p, c).unwrap_or(0.0);
            }
            let (m, coverage) = inner.rasterize(c, font.size);
            let left = (pen + m.xmin as f32).round() as i32;
            let top = (baseline - (m.ymin as f32 + m.height as f32)).round() as i32;
            for gy in 0..m.height {
                for gx in 0..m.width {
                    self.blend(left + gx as i32, top + gy as i32, color, coverage[gy * m.width + gx]);
                }
            }
            pen += m.advance_width;
            prev = Some(c);
        }
    }

    /// Copies a `w`×`h` block from `src` (at `sx, sy`) to `dx, dy`, skipping
    /// pixels equal to `mask` when given.
    #[allow(clippy::too_many_arguments)]
    pub fn blit(&mut self, src: &RgbaImage, sx: i32, sy: i32, w: i32, h: i32, dx: i32, dy: i32, mask: Option<[u8; 3]>) {
        for y in 0..h {
            for x in 0..w {
                let (px, py) = (sx + x, sy + y);
                if px < 0 || py < 0 || px >= src.width() as i32 || py >= src.height() as i32 {
                    continue;
                }
                let p = *src.get_pixel(px as u32, py as u32);
                if mask.is_some_and(|m| [p[0], p[1], p[2]] == m) || (mask.is_some() && p[3] == 0) {
                    continue;
                }
                self.plot(dx + x, dy + y, p);
            }
        }
    }
}

impl Host3D<'_> {
    pub fn current_buffer(&self) -> Result<Buffer, String> {
        decode(self.world.buffer)
    }

    /// Runs `f` on the pixels of an image or texture buffer.
    pub fn with_target<R>(&mut self, b: Buffer, f: impl FnOnce(&mut Target) -> R) -> Result<R, String> {
        match b {
            Buffer::Screen => Err("this operation needs an image or texture buffer".into()),
            Buffer::Image { id, frame } => {
                let img = self.image_mut(id)?;
                let (x0, y0, w, h) = img.frame_rect(frame)?;
                img.dirty = true;
                let mut t = Target { pixels: img.pixels_mut(), x0, y0, w, h };
                Ok(f(&mut t))
            }
            Buffer::Texture { id } => {
                let tex = self.texture_mut(id)?;
                tex.ensure_pixels()?;
                tex.dirty = true;
                let cube = tex.flags & crate::texture::flags::CUBE_MAP != 0;
                let face = tex.cube_face.clamp(0, 5);
                let pixels = tex.pixels.as_mut().expect("pixels just loaded");
                let (w, h) = (pixels.width() as i32, pixels.height() as i32);
                // a cube map's buffer is the face chosen with SetCubeFace
                let mut t = if cube { Target { pixels, x0: face * h, y0: 0, w: h, h } } else { Target { pixels, x0: 0, y0: 0, w, h } };
                Ok(f(&mut t))
            }
        }
    }

    /// Uploads every changed image and texture buffer to the GPU.
    pub fn commit_buffers(&mut self) -> Result<(), String> {
        for img in self.world.images.iter_mut().flatten() {
            if img.dirty {
                img.commit();
            }
        }
        let changed: Vec<i32> =
            self.world.textures.iter().enumerate().filter(|(_, t)| t.as_ref().is_some_and(|t| t.dirty)).map(|(i, _)| i as i32 + 1).collect();
        for t in changed {
            let replaced = self.texture_mut(t)?.commit();
            if replaced {
                // a loaded texture became editable: its users need the new resource
                self.refresh_texture_users(t)?;
            }
        }
        Ok(())
    }

    /// Gives every entity using texture `t` (directly or through a brush) its
    /// current GPU resource.
    pub fn refresh_texture_users(&mut self, t: i32) -> Result<(), String> {
        let users: Vec<i32> = self.entities().filter(|(_, e)| e.textures.contains(&t) || e.normal_map == t).map(|(i, _)| i).collect();
        for e in users {
            self.refresh_material(e)?;
        }
        let brush_users: Vec<i32> = self
            .world
            .brushes
            .iter()
            .enumerate()
            .filter(|(_, b)| b.as_ref().is_some_and(|b| b.textures.contains(&t)))
            .flat_map(|(i, _)| self.brush_users(i as i32 + 1))
            .collect();
        for e in brush_users {
            self.refresh_material(e)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffer_handles_round_trip() {
        assert_eq!(decode(BACK), Ok(Buffer::Screen));
        assert_eq!(decode(image_buffer(7, 3)), Ok(Buffer::Image { id: 7, frame: 3 }));
        assert_eq!(decode(texture_buffer(12)), Ok(Buffer::Texture { id: 12 }));
        assert!(decode(5).is_err());
    }

    #[test]
    fn argb_conversion() {
        let c = 0x80FF2010u32 as i32;
        assert_eq!(argb_to_rgba(c), Rgba([0xFF, 0x20, 0x10, 0x80]));
        assert_eq!(rgba_to_argb(argb_to_rgba(c)), c);
    }

    #[test]
    fn raster_shapes_clip_to_the_frame() {
        let mut px = RgbaImage::new(8, 4);
        let red = Rgba([255, 0, 0, 255]);
        {
            // a 4x4 window starting at x=4 (second frame of a 2-frame sheet)
            let mut t = Target { pixels: &mut px, x0: 4, y0: 0, w: 4, h: 4 };
            t.rect(-2, -2, 10, 10, true, red);
        }
        assert_eq!(*px.get_pixel(3, 0), Rgba([0, 0, 0, 0]), "first frame untouched");
        assert_eq!(*px.get_pixel(4, 0), red);
        let mut t = Target { pixels: &mut px, x0: 0, y0: 0, w: 4, h: 4 };
        t.line(0, 0, 3, 3, Rgba([0, 255, 0, 255]));
        assert_eq!(t.get(2, 2), Rgba([0, 255, 0, 255]));
        let mut o = RgbaImage::new(9, 9);
        Target { pixels: &mut o, x0: 0, y0: 0, w: 9, h: 9 }.oval(0, 0, 9, 9, false, red);
        assert_eq!(*o.get_pixel(4, 4), Rgba([0, 0, 0, 0]), "outline only");
        assert_eq!(*o.get_pixel(4, 0), red);
    }

    #[test]
    fn text_draws_glyph_pixels() {
        let font = crate::draw2d::FontInfo::default_font();
        let mut px = RgbaImage::from_pixel(64, 32, Rgba([0, 0, 0, 255]));
        Target { pixels: &mut px, x0: 0, y0: 0, w: 64, h: 32 }.text(&font, 2, 2, "Hi", Rgba([255, 255, 255, 255]));
        let lit = px.pixels().filter(|p| p[0] > 128).count();
        assert!(lit > 20, "only {lit} pixels lit");
        // nothing is drawn above the text's top edge
        assert!((0..64).all(|x| px.get_pixel(x, 0)[0] == 0));
    }

    #[test]
    fn masked_blit_skips_mask_colour() {
        let mut src = RgbaImage::new(2, 1);
        src.put_pixel(0, 0, Rgba([0, 0, 0, 255]));
        src.put_pixel(1, 0, Rgba([9, 9, 9, 255]));
        let mut dst = RgbaImage::from_pixel(2, 1, Rgba([1, 1, 1, 255]));
        Target { pixels: &mut dst, x0: 0, y0: 0, w: 2, h: 1 }.blit(&src, 0, 0, 2, 1, 0, 0, Some([0, 0, 0]));
        assert_eq!(*dst.get_pixel(0, 0), Rgba([1, 1, 1, 255]));
        assert_eq!(*dst.get_pixel(1, 0), Rgba([9, 9, 9, 255]));
    }
}
