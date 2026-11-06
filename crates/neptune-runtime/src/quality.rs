//! Graphics quality presets (ext), so one program runs on old and new PCs.
//!
//! `GraphicsQuality 0..3` picks Fyrox's low, medium, high or ultra renderer
//! settings; `PostEffect` turns single effects on or off on top of that.
//! The starting level comes from `neptune run --quality <level>` (or the
//! `NEPTUNE_QUALITY` environment variable) and defaults to high. Low also
//! turns off multisampling and is meant for older integrated GPUs.

use fyrox::renderer::QualitySettings;

pub const EFFECT_BLOOM: i32 = 1;
pub const EFFECT_SSAO: i32 = 2;
pub const EFFECT_FXAA: i32 = 3;
pub const EFFECT_SHADOWS: i32 = 4;
pub const EFFECT_LIGHT_SCATTER: i32 = 5;

/// Requested quality: a preset plus per-effect overrides.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quality {
    pub level: i32,
    pub bloom: Option<bool>,
    pub ssao: Option<bool>,
    pub fxaa: Option<bool>,
    pub shadows: Option<bool>,
    pub scatter: Option<bool>,
}

impl Default for Quality {
    fn default() -> Self {
        Quality { level: initial_level(), bloom: None, ssao: None, fxaa: None, shadows: None, scatter: None }
    }
}

/// Parses "low", "medium", "high", "ultra" or 0-3.
pub fn parse_level(s: &str) -> Option<i32> {
    match s.trim().to_ascii_lowercase().as_str() {
        "0" | "low" => Some(0),
        "1" | "medium" | "med" => Some(1),
        "2" | "high" => Some(2),
        "3" | "ultra" => Some(3),
        _ => None,
    }
}

fn initial_level() -> i32 {
    std::env::var("NEPTUNE_QUALITY").ok().and_then(|v| parse_level(&v)).unwrap_or(2)
}

impl Quality {
    /// Multisample count for the window (set when it opens).
    pub fn msaa(&self) -> Option<u8> {
        if self.level == 0 {
            None
        } else {
            Some(4)
        }
    }

    pub fn settings(&self) -> QualitySettings {
        let mut q = match self.level {
            0 => QualitySettings::low(),
            1 => QualitySettings::medium(),
            3 => QualitySettings::ultra(),
            _ => QualitySettings::high(),
        };
        if self.level == 0 {
            // Fyrox's low preset keeps tiny half-precision sun shadows that
            // speckle surfaces; dropping them is both cleaner and faster
            q.csm_settings.enabled = false;
        }
        if let Some(b) = self.bloom {
            q.hdr_settings.bloom_settings.use_bloom = b;
        }
        if let Some(b) = self.ssao {
            q.use_ssao = b;
        }
        if let Some(b) = self.fxaa {
            q.fxaa = b;
        }
        if let Some(b) = self.shadows {
            q.point_shadows_enabled = b;
            q.spot_shadows_enabled = b;
            q.csm_settings.enabled = b;
            if b && q.spot_shadow_map_size < 256 {
                q.csm_settings.size = q.csm_settings.size.max(1024);
                q.csm_settings.pcf = true;
                // the low preset shrinks maps to nothing; give enabled shadows a usable size
                q.point_shadow_map_size = 256;
                q.spot_shadow_map_size = 512;
                q.point_shadows_distance = 15.0;
                q.spot_shadows_distance = 15.0;
            }
        }
        if let Some(b) = self.scatter {
            q.light_scatter_enabled = b;
        }
        q
    }

    pub fn set_effect(&mut self, effect: i32, on: bool) -> Result<(), String> {
        match effect {
            EFFECT_BLOOM => self.bloom = Some(on),
            EFFECT_SSAO => self.ssao = Some(on),
            EFFECT_FXAA => self.fxaa = Some(on),
            EFFECT_SHADOWS => self.shadows = Some(on),
            EFFECT_LIGHT_SCATTER => self.scatter = Some(on),
            e => return Err(format!("PostEffect: unknown effect {e} (use EFFECT_BLOOM, EFFECT_SSAO, EFFECT_FXAA, EFFECT_SHADOWS or EFFECT_LIGHT_SCATTER)")),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_and_overrides() {
        assert_eq!(parse_level("Low"), Some(0));
        assert_eq!(parse_level("3"), Some(3));
        assert_eq!(parse_level("max"), None);
        let mut q = Quality { level: 0, ..Quality::default() };
        assert!(!q.settings().use_ssao);
        assert_eq!(q.msaa(), None);
        q.set_effect(EFFECT_SSAO, true).unwrap();
        assert!(q.settings().use_ssao);
        q.set_effect(EFFECT_SHADOWS, true).unwrap();
        assert!(q.settings().spot_shadows_enabled && q.settings().spot_shadow_map_size >= 256);
        assert!(q.set_effect(99, true).is_err());
    }
}
