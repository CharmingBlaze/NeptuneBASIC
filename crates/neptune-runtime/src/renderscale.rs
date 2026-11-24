//! (ext) `RenderScale`: the 3D view drawn at a lower resolution than the
//! window and stretched to fit, for slow graphics cards.
//!
//! Most of what a graphics card does for a frame grows with the number of
//! pixels, so drawing at 0.7 of the window's size each way (half the pixels)
//! is the largest single saving an old or built-in card can get. Text, 2D
//! drawing and the GUI are not affected: they stay sharp at the window's
//! own resolution.
//!
//! Fyrox draws a scene at the size of its render target when it has one,
//! and then leaves the picture in that texture instead of copying it to the
//! window. So the scene gets a smaller texture ([`Host3D::sync_render_scale`])
//! and the 2D layer starts each frame by drawing that texture over the whole
//! window (`DrawOp::Frame`, with a shader that ignores alpha, as Fyrox's own
//! copy does). This applies to the first scene; cameras that draw into
//! their own picture (`CameraClsMode`, mirrors, `CameraToTexture`) keep
//! their size.
//!
//! The commands are in `quality.rs` with the other quality commands.
//! The player's `neptune run --render-scale` (`NEPTUNE_RENDER_SCALE`) is a
//! ceiling, like `--max-quality`: the program gets no more than that.

use fyrox::material::shader::{Shader, ShaderResource};
use fyrox::material::{Material, MaterialResource};
use fyrox::resource::texture::{TextureResource, TextureResourceExtension};

use crate::world::Host3D;

/// The smallest scale accepted: a quarter each way is a sixteenth of the pixels.
pub const MIN_SCALE: f32 = 0.25;

/// Parses a scale given by the player ("0.5", "50%").
pub fn parse_scale(s: &str) -> Option<f32> {
    let s = s.trim();
    let v = match s.strip_suffix('%') {
        Some(p) => p.trim().parse::<f32>().ok()? / 100.0,
        None => s.parse::<f32>().ok()?,
    };
    (v.is_finite() && v > 0.0).then(|| v.clamp(MIN_SCALE, 1.0))
}

/// The player's ceiling (`NEPTUNE_RENDER_SCALE`), 1 when there is none.
pub fn max_scale() -> f32 {
    std::env::var("NEPTUNE_RENDER_SCALE").ok().and_then(|v| parse_scale(&v)).unwrap_or(1.0)
}

/// Size of the picture for a window of `w` x `h` logical pixels, or `None`
/// when the scene should be drawn straight to the window.
pub fn frame_size(w: u32, h: u32, dpi: f32, scale: f32) -> Option<(u32, u32)> {
    if scale >= 0.999 {
        return None;
    }
    let side = |n: u32| ((n as f32 * dpi * scale).round() as u32).max(1);
    Some((side(w), side(h)))
}

thread_local! {
    static MATERIAL: MaterialResource = {
        let shader = Shader::from_string(include_str!("frame.shader")).expect("built-in frame shader is valid");
        let shader = ShaderResource::new_embedded(shader);
        MaterialResource::new_embedded(Material::from_shader(shader))
    };
}

/// The material that draws the 3D picture over the window, opaque.
pub fn frame_material() -> MaterialResource {
    MATERIAL.with(|m| m.clone())
}

impl Host3D<'_> {
    /// The scale in use: what the program asked for, under the player's ceiling.
    pub fn render_scale(&self) -> f32 {
        self.world.render_scale.min(self.world.render_scale_max).clamp(MIN_SCALE, 1.0)
    }

    /// Gives the first scene a picture of the right size to draw into, or
    /// takes it away at scale 1 (called before each render: the window may
    /// have changed size).
    pub fn sync_render_scale(&mut self) -> Result<(), String> {
        if self.world.current_scene != 0 {
            return Ok(());
        }
        let want = frame_size(self.world.width, self.world.height, self.world.dpi, self.render_scale());
        if want == self.world.render_frame {
            return Ok(());
        }
        let picture = want.map(|(w, h)| TextureResource::new_render_target(w, h));
        self.scene()?.rendering_options.get_value_mut_silent().render_target = picture.clone();
        self.world.canvas.frame = picture;
        self.world.render_frame = want;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_from_the_player() {
        assert_eq!(parse_scale("0.5"), Some(0.5));
        assert_eq!(parse_scale(" 75% "), Some(0.75));
        assert_eq!(parse_scale("2"), Some(1.0));
        assert_eq!(parse_scale("0.01"), Some(MIN_SCALE));
        assert_eq!(parse_scale("0"), None);
        assert_eq!(parse_scale("half"), None);
    }

    #[test]
    fn picture_size_follows_window_dpi_and_scale() {
        assert_eq!(frame_size(1280, 720, 1.0, 1.0), None);
        assert_eq!(frame_size(1280, 720, 1.0, 0.5), Some((640, 360)));
        // a high-DPI window has more pixels than its logical size
        assert_eq!(frame_size(1280, 720, 1.25, 0.5), Some((800, 450)));
        assert_eq!(frame_size(3, 3, 1.0, 0.25), Some((1, 1)));
    }

    #[test]
    fn the_frame_shader_is_valid() {
        assert!(Shader::from_string(include_str!("frame.shader")).is_ok());
    }
}
