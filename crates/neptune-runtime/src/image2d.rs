//! 2D images (`LoadImage`, `DrawImage`, `ImagesCollide`, ...).
//!
//! An image keeps its pixels in memory (for masking and pixel-perfect
//! collisions) and two GPU textures: one with the mask colour made
//! transparent (`DrawImage`) and one without (`DrawBlock`). Like Blitz3D,
//! the default mask colour is black.

use fyrox::{
    asset::untyped::ResourceKind,
    core::{math::Rect, uuid::Uuid},
    resource::texture::{
        Texture, TextureKind, TextureMagnificationFilter, TextureMinificationFilter, TexturePixelKind, TextureResource,
    },
};
use image::RgbaImage;

#[derive(Debug, Clone)]
pub struct Image2D {
    pixels: RgbaImage,
    /// Size of one frame.
    pub frame_w: u32,
    pub frame_h: u32,
    /// First frame index within the source image, and frame count.
    first: u32,
    pub frames: u32,
    /// Hot spot subtracted from draw positions.
    pub handle: (f32, f32),
    /// Drawing scale (`ScaleImage` / `ResizeImage`).
    pub scale: (f32, f32),
    mask: [u8; 3],
    pub masked: TextureResource,
    pub block: TextureResource,
    /// Pixels changed through an image buffer and not yet uploaded.
    pub dirty: bool,
    /// File the image was loaded from ("" if made in the program).
    pub path: String,
}

fn make_texture(img: &RgbaImage) -> TextureResource {
    let mut tex = Texture::from_bytes(
        TextureKind::Rectangle { width: img.width(), height: img.height() },
        TexturePixelKind::RGBA8,
        img.as_raw().clone(),
    )
    .expect("RGBA buffer matches its size");
    // no mipmaps: plain linear filtering keeps 2D images sharp and visible
    tex.set_minification_filter(TextureMinificationFilter::Linear);
    tex.set_magnification_filter(TextureMagnificationFilter::Linear);
    TextureResource::new_ok(Uuid::new_v4(), ResourceKind::Embedded, tex)
}

fn apply_mask(img: &RgbaImage, mask: [u8; 3]) -> RgbaImage {
    let mut out = img.clone();
    for p in out.pixels_mut() {
        if p[0] == mask[0] && p[1] == mask[1] && p[2] == mask[2] {
            p[3] = 0;
        }
    }
    out
}

impl Image2D {
    /// Builds an image from pixels, split into `frames` frames of `fw`×`fh`
    /// starting at frame `first` (all zero = one frame covering the image).
    pub fn new(pixels: RgbaImage, fw: u32, fh: u32, first: u32, frames: u32) -> Result<Image2D, String> {
        let (w, h) = pixels.dimensions();
        let (fw, fh) = if fw == 0 || fh == 0 { (w, h) } else { (fw, fh) };
        if fw > w || fh > h || w == 0 || h == 0 {
            return Err(format!("frame size {fw}x{fh} does not fit image {w}x{h}"));
        }
        let available = (w / fw) * (h / fh);
        let frames = if frames == 0 { available - first.min(available) } else { frames };
        if first + frames > available {
            return Err(format!("image has {available} frames of {fw}x{fh}, asked for {first}..{}", first + frames));
        }
        let mask = [0, 0, 0];
        Ok(Image2D {
            masked: make_texture(&apply_mask(&pixels, mask)),
            block: make_texture(&pixels),
            pixels,
            frame_w: fw,
            frame_h: fh,
            first,
            frames: frames.max(1),
            handle: (0.0, 0.0),
            scale: (1.0, 1.0),
            mask,
            dirty: false,
            path: String::new(),
        })
    }

    /// `CreateImage`: a blank (black) image with `frames` frames side by side.
    pub fn blank(w: u32, h: u32, frames: u32) -> Result<Image2D, String> {
        if w == 0 || h == 0 || w > 16384 || h > 16384 {
            return Err(format!("image size {w}x{h} is not allowed"));
        }
        let frames = frames.max(1);
        let px = RgbaImage::from_pixel(w * frames, h, image::Rgba([0, 0, 0, 255]));
        Image2D::new(px, w, h, 0, frames)
    }

    /// An independent copy with its own textures (`CopyImage`).
    pub fn duplicate(&self) -> Image2D {
        let mut c = Image2D::new(self.pixels.clone(), self.frame_w, self.frame_h, self.first, self.frames).expect("same layout");
        c.handle = self.handle;
        c.scale = self.scale;
        c.set_mask(self.mask);
        c
    }

    pub fn pixels(&self) -> &RgbaImage {
        &self.pixels
    }

    pub fn pixels_mut(&mut self) -> &mut RgbaImage {
        &mut self.pixels
    }

    pub fn mask(&self) -> [u8; 3] {
        self.mask
    }

    /// Pixel rectangle `(x, y, w, h)` of a frame within the image.
    pub fn frame_rect(&self, frame: u32) -> Result<(i32, i32, i32, i32), String> {
        let (x, y) = self.frame_origin(frame)?;
        Ok((x as i32, y as i32, self.frame_w as i32, self.frame_h as i32))
    }

    /// Uploads changed pixels to both GPU textures.
    pub fn commit(&mut self) {
        self.dirty = false;
        let masked = apply_mask(&self.pixels, self.mask);
        for (tex, px) in [(&self.block, &self.pixels), (&self.masked, &masked)] {
            let mut t = tex.data_ref();
            let mut m = t.modify();
            let bytes = m.data_mut();
            if bytes.len() == px.as_raw().len() {
                bytes.copy_from_slice(px.as_raw());
            }
        }
    }

    /// `RotateImage`: rotates a single-frame image (degrees, clockwise on screen).
    pub fn rotate(&mut self, degrees: f32, smooth: bool) -> Result<(), String> {
        let (s, c) = degrees.to_radians().sin_cos();
        self.transform([[c, -s], [s, c]], smooth)
    }

    /// `TFormImage`: applies a 2x2 matrix to a single-frame image about its
    /// centre (`x' = a*x + b*y`, `y' = c*x + d*y`, y pointing down), into a
    /// canvas big enough to hold the result. The handle stays on the same
    /// pixel. `smooth` samples bilinearly (`TFormFilter`).
    pub fn transform(&mut self, m: [[f32; 2]; 2], smooth: bool) -> Result<(), String> {
        if self.frames != 1 || self.pixels.width() != self.frame_w || self.pixels.height() != self.frame_h {
            return Err("this command works on single-frame images".into());
        }
        let det = m[0][0] * m[1][1] - m[0][1] * m[1][0];
        if det.abs() < 1e-6 {
            return Err("the transform squashes the image flat".into());
        }
        let inv = [[m[1][1] / det, -m[0][1] / det], [-m[1][0] / det, m[0][0] / det]];
        let (w, h) = (self.frame_w as f32, self.frame_h as f32);
        let (cx, cy) = (w / 2.0, h / 2.0);
        let fwd = |x: f32, y: f32| (m[0][0] * x + m[0][1] * y, m[1][0] * x + m[1][1] * y);
        let corners = [fwd(-cx, -cy), fwd(cx, -cy), fwd(cx, cy), fwd(-cx, cy)];
        let span = |f: fn(&(f32, f32)) -> f32| {
            let v: Vec<f32> = corners.iter().map(f).collect();
            v.iter().cloned().fold(f32::MIN, f32::max) - v.iter().cloned().fold(f32::MAX, f32::min)
        };
        // a small tolerance keeps e.g. a 90 degree turn of a 4x2 image at 2x4, not 3x4
        let size = |v: f32| (v - 1e-3).ceil().clamp(1.0, 16384.0);
        let (nw, nh) = (size(span(|p| p.0)), size(span(|p| p.1)));
        let fill = image::Rgba([self.mask[0], self.mask[1], self.mask[2], 255]);
        let mut out = RgbaImage::from_pixel(nw as u32, nh as u32, fill);
        let (ncx, ncy) = (nw / 2.0, nh / 2.0);
        let src = &self.pixels;
        let get = |x: i32, y: i32| -> Option<[f32; 4]> {
            (x >= 0 && y >= 0 && (x as f32) < w && (y as f32) < h).then(|| src.get_pixel(x as u32, y as u32).0.map(|c| c as f32))
        };
        for y in 0..nh as u32 {
            for x in 0..nw as u32 {
                // map each destination pixel centre back into the source
                let (dx, dy) = (x as f32 + 0.5 - ncx, y as f32 + 0.5 - ncy);
                let (sx, sy) = (inv[0][0] * dx + inv[0][1] * dy + cx, inv[1][0] * dx + inv[1][1] * dy + cy);
                if sx < 0.0 || sy < 0.0 || sx >= w || sy >= h {
                    continue;
                }
                let p = if smooth {
                    let (fx, fy) = (sx - 0.5, sy - 0.5);
                    let (x0, y0) = (fx.floor() as i32, fy.floor() as i32);
                    let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
                    let c = |xx: i32, yy: i32| get(xx.clamp(0, w as i32 - 1), yy.clamp(0, h as i32 - 1)).unwrap_or([0.0; 4]);
                    let (a, b, cc, d) = (c(x0, y0), c(x0 + 1, y0), c(x0, y0 + 1), c(x0 + 1, y0 + 1));
                    let mut r = [0u8; 4];
                    for i in 0..4 {
                        let top = a[i] + (b[i] - a[i]) * tx;
                        let bottom = cc[i] + (d[i] - cc[i]) * tx;
                        r[i] = (top + (bottom - top) * ty).round() as u8;
                    }
                    image::Rgba(r)
                } else {
                    *src.get_pixel(sx as u32, sy as u32)
                };
                out.put_pixel(x, y, p);
            }
        }
        let (hx, hy) = fwd(self.handle.0 - cx, self.handle.1 - cy);
        let mut r = Image2D::new(out, 0, 0, 0, 1)?;
        r.handle = (hx + ncx, hy + ncy);
        r.scale = self.scale;
        r.set_mask(self.mask);
        *self = r;
        Ok(())
    }

    pub fn load(path: &str, fw: u32, fh: u32, first: u32, frames: u32) -> Result<Image2D, String> {
        let mut img = Image2D::new(crate::assets::pixels(path)?, fw, fh, first, frames)?;
        img.path = path.to_string();
        Ok(img)
    }

    /// Reads the image's file again, keeping its frames, handle, scale and mask.
    pub fn reload(&mut self, path: &str) -> Result<(), String> {
        let mut fresh = Image2D::new(crate::assets::pixels(path)?, self.frame_w, self.frame_h, self.first, self.frames)?;
        fresh.handle = self.handle;
        fresh.scale = self.scale;
        fresh.set_mask(self.mask);
        fresh.path = path.to_string();
        *self = fresh;
        Ok(())
    }

    /// An image whose pixels arrive later (`LoadImageAsync`): a single
    /// transparent pixel until then.
    pub fn placeholder() -> Image2D {
        Image2D::new(RgbaImage::from_pixel(1, 1, image::Rgba([0, 0, 0, 0])), 1, 1, 0, 1).expect("1x1 image")
    }

    /// Fills a placeholder with loaded pixels, cut into frames.
    pub fn fill(&mut self, pixels: RgbaImage, frames: (u32, u32, u32, u32), path: &str) -> Result<(), String> {
        let mut fresh = Image2D::new(pixels, frames.0, frames.1, frames.2, frames.3)?;
        fresh.handle = self.handle;
        fresh.scale = self.scale;
        fresh.path = path.to_string();
        *self = fresh;
        Ok(())
    }

    pub fn set_mask(&mut self, mask: [u8; 3]) {
        self.mask = mask;
        self.masked = make_texture(&apply_mask(&self.pixels, mask));
    }

    /// Displayed size (frame size times scale).
    pub fn size(&self) -> (f32, f32) {
        (self.frame_w as f32 * self.scale.0, self.frame_h as f32 * self.scale.1)
    }

    fn frame_origin(&self, frame: u32) -> Result<(u32, u32), String> {
        if frame >= self.frames {
            return Err(format!("image frame {frame} out of range (image has {} frames)", self.frames));
        }
        let cols = self.pixels.width() / self.frame_w;
        let idx = self.first + frame;
        Ok(((idx % cols) * self.frame_w, (idx / cols) * self.frame_h))
    }

    /// Texture coordinates of a sub-rectangle of a frame.
    pub fn uv(&self, frame: u32, sub: Option<Rect<f32>>) -> Result<Rect<f32>, String> {
        let (ox, oy) = self.frame_origin(frame)?;
        let (w, h) = (self.pixels.width() as f32, self.pixels.height() as f32);
        let r = sub.unwrap_or_else(|| Rect::new(0.0, 0.0, self.frame_w as f32, self.frame_h as f32));
        Ok(Rect::new((ox as f32 + r.x()) / w, (oy as f32 + r.y()) / h, r.w() / w, r.h() / h))
    }

    /// Is the pixel at frame-local `(x, y)` solid (inside and not the mask colour)?
    pub fn solid(&self, frame: u32, x: i32, y: i32) -> bool {
        if x < 0 || y < 0 || x >= self.frame_w as i32 || y >= self.frame_h as i32 {
            return false;
        }
        let Ok((ox, oy)) = self.frame_origin(frame) else { return false };
        let p = self.pixels.get_pixel(ox + x as u32, oy + y as u32);
        p[3] != 0 && [p[0], p[1], p[2]] != self.mask
    }

    /// Top-left corner when drawn at `(x, y)`.
    pub fn top_left(&self, x: f32, y: f32) -> (f32, f32) {
        (x - self.handle.0 * self.scale.0, y - self.handle.1 * self.scale.1)
    }
}

/// Axis-aligned rectangle overlap (Blitz `RectsOverlap`).
pub fn rects_overlap(a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)) -> bool {
    a.0 < b.0 + b.2 && b.0 < a.0 + a.2 && a.1 < b.1 + b.3 && b.1 < a.1 + a.3
}

/// Pixel-perfect collision of two image frames drawn at the given positions
/// (scale is ignored, as in Blitz).
pub fn images_collide(a: &Image2D, ax: f32, ay: f32, af: u32, b: &Image2D, bx: f32, by: f32, bf: u32) -> bool {
    let (ax, ay) = (ax - a.handle.0, ay - a.handle.1);
    let (bx, by) = (bx - b.handle.0, by - b.handle.1);
    let ra = (ax, ay, a.frame_w as f32, a.frame_h as f32);
    let rb = (bx, by, b.frame_w as f32, b.frame_h as f32);
    if !rects_overlap(ra, rb) {
        return false;
    }
    let (x0, y0) = (ax.max(bx).floor() as i32, ay.max(by).floor() as i32);
    let (x1, y1) = ((ax + ra.2).min(bx + rb.2).ceil() as i32, (ay + ra.3).min(by + rb.3).ceil() as i32);
    for y in y0..y1 {
        for x in x0..x1 {
            if a.solid(af, x - ax as i32, y - ay as i32) && b.solid(bf, x - bx as i32, y - by as i32) {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 4x2 image: two 2x2 frames; frame 0 has one solid pixel at (1,1), frame 1 is solid.
    fn sheet() -> Image2D {
        let mut px = RgbaImage::new(4, 2);
        for p in px.pixels_mut() {
            *p = image::Rgba([0, 0, 0, 255]);
        }
        px.put_pixel(1, 1, image::Rgba([255, 0, 0, 255]));
        for y in 0..2 {
            for x in 2..4 {
                px.put_pixel(x, y, image::Rgba([0, 255, 0, 255]));
            }
        }
        Image2D::new(px, 2, 2, 0, 0).unwrap()
    }

    #[test]
    fn frames_and_uvs() {
        let s = sheet();
        assert_eq!(s.frames, 2);
        let uv = s.uv(1, None).unwrap();
        assert_eq!((uv.x(), uv.y(), uv.w(), uv.h()), (0.5, 0.0, 0.5, 1.0));
        assert!(s.uv(2, None).is_err());
        assert!(Image2D::new(RgbaImage::new(4, 2), 3, 3, 0, 0).is_err());
    }

    #[test]
    fn mask_and_pixel_collisions() {
        let s = sheet();
        assert!(!s.solid(0, 0, 0)); // black is masked by default
        assert!(s.solid(0, 1, 1));
        assert!(s.solid(1, 0, 0));
        // frame 0's only solid pixel is at (1,1)
        assert!(images_collide(&s, 0.0, 0.0, 0, &s, 1.0, 1.0, 1));
        assert!(!images_collide(&s, 0.0, 0.0, 0, &s, -1.0, -1.0, 1)); // overlaps only masked pixels
        assert!(!images_collide(&s, 0.0, 0.0, 1, &s, 5.0, 0.0, 1));
        let mut m = sheet();
        m.set_mask([255, 0, 0]);
        assert!(m.solid(0, 0, 0) && !m.solid(0, 1, 1));
    }

    #[test]
    fn transforms_keep_the_handle_pixel() {
        let mut px = RgbaImage::new(4, 2);
        px.put_pixel(3, 0, image::Rgba([255, 0, 0, 255]));
        let mut img = Image2D::new(px, 0, 0, 0, 1).unwrap();
        img.handle = (3.5, 0.5); // centre of the red pixel
        img.rotate(90.0, false).unwrap();
        assert_eq!((img.frame_w, img.frame_h), (2, 4));
        let (hx, hy) = img.handle;
        assert_eq!(img.pixels().get_pixel(hx as u32, hy as u32).0, [255, 0, 0, 255]);
        // scaling by 2 doubles the size
        let mut img2 = Image2D::new(RgbaImage::new(4, 2), 0, 0, 0, 1).unwrap();
        img2.transform([[2.0, 0.0], [0.0, 2.0]], true).unwrap();
        assert_eq!((img2.frame_w, img2.frame_h), (8, 4));
        assert!(img2.transform([[1.0, 2.0], [2.0, 4.0]], false).is_err());
    }

    #[test]
    fn rect_overlap() {
        assert!(rects_overlap((0.0, 0.0, 10.0, 10.0), (5.0, 5.0, 10.0, 10.0)));
        assert!(!rects_overlap((0.0, 0.0, 10.0, 10.0), (10.0, 0.0, 5.0, 5.0)));
    }
}
