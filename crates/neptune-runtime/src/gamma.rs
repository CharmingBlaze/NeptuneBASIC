//! Display gamma (`SetGamma`, `UpdateGamma`, `GammaRed/Green/Blue`).
//!
//! Blitz3D keeps a 256-entry ramp per colour channel. `SetGamma` changes
//! entries, `UpdateGamma` applies the ramps to the display. Here the ramps
//! become a colour-grading look-up table that every camera applies to its
//! final image, so gamma, brightness and colour tints work on any GPU.

use fyrox::{
    asset::untyped::ResourceKind,
    core::{futures::executor::block_on, uuid::Uuid},
    resource::texture::{Texture, TextureKind, TexturePixelKind, TextureResource},
    scene::camera::ColorGradingLut,
};

#[derive(Clone, Debug, PartialEq)]
pub struct Gamma {
    /// Output value for each input value, per channel.
    pub ramp: [[u8; 256]; 3],
}

impl Default for Gamma {
    fn default() -> Self {
        let id: [u8; 256] = std::array::from_fn(|i| i as u8);
        Gamma { ramp: [id; 3] }
    }
}

impl Gamma {
    pub fn is_identity(&self) -> bool {
        *self == Gamma::default()
    }

    /// `SetGamma`: maps one input level of each channel to an output level.
    pub fn set(&mut self, src: [i32; 3], dst: [f32; 3]) {
        for c in 0..3 {
            if let Ok(i) = usize::try_from(src[c]) {
                if i < 256 {
                    self.ramp[c][i] = dst[c].clamp(0.0, 255.0).round() as u8;
                }
            }
        }
    }

    /// The colour-grading strip Fyrox expects: 16 slices of 16x16 side by
    /// side (256x16; Fyrox checks for 16*16*16 pixels), red along x inside a
    /// slice, green down, blue across slices.
    pub fn lut_strip(&self) -> Vec<u8> {
        let level = |c: usize, v: usize| self.ramp[c][(v * 17).min(255)];
        let mut bytes = Vec::with_capacity(256 * 16 * 3);
        for g in 0..16 {
            for b in 0..16 {
                for r in 0..16 {
                    bytes.extend_from_slice(&[level(0, r), level(1, g), level(2, b)]);
                }
            }
        }
        bytes
    }

    pub fn lut(&self) -> Result<ColorGradingLut, String> {
        let tex = Texture::from_bytes(TextureKind::Rectangle { width: 256, height: 16 }, TexturePixelKind::RGB8, self.lut_strip())
            .ok_or("bad gamma table size")?;
        let res = TextureResource::new_ok(Uuid::new_v4(), ResourceKind::Embedded, tex);
        block_on(ColorGradingLut::new(res)).map_err(|e| format!("cannot build the gamma table: {e:?}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ramps_and_strip() {
        let mut g = Gamma::default();
        assert!(g.is_identity());
        g.set([255, 255, 255], [128.0, 255.0, 255.0]);
        assert_eq!(g.ramp[0][255], 128);
        assert!(!g.is_identity());
        let strip = g.lut_strip();
        assert_eq!(strip.len(), 256 * 16 * 3);
        // the brightest red cell (r = 15) now maps to 128; green and blue untouched
        let cell = |r: usize, gg: usize, b: usize| {
            let i = (gg * 256 + b * 16 + r) * 3;
            [strip[i], strip[i + 1], strip[i + 2]]
        };
        assert_eq!(cell(15, 15, 15), [128, 255, 255]);
        assert_eq!(cell(0, 0, 0), [0, 0, 0]);
    }
}
