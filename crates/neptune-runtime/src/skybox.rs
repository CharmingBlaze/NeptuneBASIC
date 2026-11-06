//! Sky boxes (ext): `SkyBox`.
//!
//! The sky is a cube map texture (`LoadTexture file, 128`: six square faces
//! side by side in Blitz order left, front, right, back, up, down), drawn
//! behind everything by every camera that clears its colour.

use fyrox::scene::skybox::SkyBoxBuilder;

use crate::texture::flags;
use crate::world::Host3D;

/// The six faces of a cube map strip, in Blitz order.
fn faces(strip: &image::RgbaImage) -> Vec<image::RgbaImage> {
    let size = strip.height();
    (0..6).map(|i| image::imageops::crop_imm(strip, i * size, 0, size, size).to_image()).collect()
}

impl Host3D<'_> {
    /// `SkyBox`: shows a cube map texture as the sky (0 removes the sky).
    pub fn skybox(&mut self, texture: i32) -> Result<(), String> {
        if texture == 0 {
            self.scene()?.set_skybox(None);
            return Ok(());
        }
        let t = self.texture_mut(texture)?;
        if t.flags & flags::CUBE_MAP == 0 {
            return Err("SkyBox needs a cube map texture (LoadTexture file, 128)".into());
        }
        t.ensure_pixels()?;
        let strip = t.pixels.clone().ok_or("SkyBox: the texture has no pixels")?;
        if strip.width() != strip.height() * 6 {
            return Err("SkyBox: a cube map is six square faces side by side".into());
        }
        // Blitz space is Fyrox space mirrored in X, so every face is seen
        // mirrored (checked against Blitz's face order by rendering)
        let f: Vec<_> = faces(&strip).into_iter().map(|img| crate::texture::solid_from(&image::imageops::flip_horizontal(&img))).collect();
        let sky = SkyBoxBuilder::from_texture(&f[0])
            .with_left(f[0].clone())
            .with_front(f[1].clone())
            .with_right(f[2].clone())
            .with_back(f[3].clone())
            .with_top(f[4].clone())
            .with_bottom(f[5].clone())
            .build()
            .map_err(|e| format!("SkyBox: {e:?}"))?;
        self.scene()?.set_skybox(Some(sky));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_a_strip() {
        let mut strip = image::RgbaImage::new(12, 2);
        for i in 0..6u32 {
            strip.put_pixel(i * 2, 0, image::Rgba([i as u8, 0, 0, 255]));
        }
        let f = faces(&strip);
        assert_eq!(f.len(), 6);
        assert!(f.iter().enumerate().all(|(i, img)| img.dimensions() == (2, 2) && img.get_pixel(0, 0)[0] == i as u8));
    }
}
