//! Graphics quality presets (ext), so one program runs on old and new PCs.
//!
//! `GraphicsQuality 0..3` picks Fyrox's low, medium, high or ultra renderer
//! settings; `PostEffect` turns single effects on or off on top of that.
//! The starting level comes from `neptune run --quality <level>` (or the
//! `NEPTUNE_QUALITY` environment variable) and defaults to high. Low also
//! turns off multisampling and is meant for older integrated GPUs.
//!
//! Shadow controls (ext): `SetShadowMode`, `SetShadowQuality`,
//! `SetShadowResolution`, `SetShadowSoftness`, `SetShadowDistance`,
//! `SetShadowCascadeCount`, `SetShadowBias`, `EnableShadows` and
//! `DisableShadows`. Fyrox 1.0 has hard or PCF-filtered (soft) shadow maps,
//! three cascades for directional lights, cube maps for point lights and
//! no contact shadows; soft shadows are on or off, not a radius.

use fyrox::renderer::{QualitySettings, ShadowMapPrecision};
use fyrox::scene::light::{directional::{DirectionalLight, FrustumSplitOptions}, point::PointLight, spot::SpotLight};
use neptune_vm::value::Value;

use fyrox::graph::SceneGraph;

use crate::commands::Cmd;
use crate::world::Host3D;

pub const SHADOW_OFF: i32 = 0;
pub const SHADOW_HARD: i32 = 1;
pub const SHADOW_SOFT: i32 = 2;

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
    /// PCF filtering of every shadow map.
    pub soft_shadows: Option<bool>,
    /// Shadow map size of directional and spot lights (point lights half).
    pub shadow_size: Option<usize>,
    pub shadow_precision_full: Option<bool>,
    /// Distance shadows reach (all light kinds).
    pub shadow_distance: Option<f32>,
    /// Applied to lights: bias (0 = Fyrox default) and cascade count.
    pub shadow_bias: Option<f32>,
    pub cascades: i32,
}

impl Default for Quality {
    fn default() -> Self {
        Quality {
            level: initial_level(),
            bloom: None,
            ssao: None,
            fxaa: None,
            shadows: None,
            scatter: None,
            soft_shadows: None,
            shadow_size: None,
            shadow_precision_full: None,
            shadow_distance: None,
            shadow_bias: None,
            cascades: 3,
        }
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
        if let Some(s) = self.soft_shadows {
            q.csm_settings.pcf = s;
            q.point_soft_shadows = s;
            q.spot_soft_shadows = s;
        }
        if let Some(n) = self.shadow_size {
            q.csm_settings.size = n;
            q.spot_shadow_map_size = n;
            q.point_shadow_map_size = (n / 2).max(64);
        }
        if let Some(full) = self.shadow_precision_full {
            let p = if full { ShadowMapPrecision::Full } else { ShadowMapPrecision::Half };
            q.csm_settings.precision = p;
            q.spot_shadow_map_precision = p;
            q.point_shadow_map_precision = p;
        }
        if let Some(d) = self.shadow_distance {
            q.point_shadows_distance = d;
            q.spot_shadows_distance = d;
        }
        q
    }

    /// Far planes of the three cascades for the cascade count and distance.
    pub fn cascade_planes(&self) -> [f32; 3] {
        let d = self.shadow_distance.unwrap_or(64.0).max(0.1);
        match self.cascades {
            1 => [d, d, d],
            2 => [d * 0.3, d, d],
            _ => [d * 0.08, d * 0.4, d],
        }
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

impl Host3D<'_> {
    /// Gives every light in the scene the program's shadow settings.
    pub fn apply_light_shadows(&mut self) -> Result<(), String> {
        let q = self.world.quality;
        let custom_split = q.cascades != 3 || q.shadow_distance.is_some();
        let planes = q.cascade_planes();
        let graph = self.graph()?;
        let handles: Vec<_> = graph.pair_iter().map(|(h, _)| h).collect();
        for h in handles {
            let n = &mut graph[h];
            if let Some(l) = n.cast_mut::<DirectionalLight>() {
                let mut o = (*l.csm_options).clone();
                if custom_split {
                    o.split_options = FrustumSplitOptions::Absolute { far_planes: planes };
                }
                if let Some(b) = q.shadow_bias {
                    o.set_shadow_bias(b);
                }
                l.csm_options.set_value_and_mark_modified(o);
            } else if let Some(l) = n.cast_mut::<PointLight>() {
                if let Some(b) = q.shadow_bias {
                    l.set_shadow_bias(b);
                }
            } else if let Some(l) = n.cast_mut::<SpotLight>() {
                if let Some(b) = q.shadow_bias {
                    l.set_shadow_bias(b);
                }
            }
        }
        Ok(())
    }

    fn shadows_changed(&mut self) -> Result<Value, String> {
        self.world.quality_dirty = true;
        if self.scene().is_ok() {
            self.apply_light_shadows()?;
        }
        Ok(Value::Null)
    }
}

/// The shadow commands; see the module comment.
pub static COMMANDS: &[(&str, Cmd)] = &[
    ("SetShadowMode%mode", |h, a, _| {
        let q = &mut h.world.quality;
        match a[0].int() {
            SHADOW_OFF => q.shadows = Some(false),
            SHADOW_HARD => {
                q.shadows = Some(true);
                q.soft_shadows = Some(false);
            }
            SHADOW_SOFT => {
                q.shadows = Some(true);
                q.soft_shadows = Some(true);
            }
            m => return Err(format!("SetShadowMode: mode must be SHADOW_OFF, SHADOW_HARD or SHADOW_SOFT, not {m}")),
        }
        h.shadows_changed()
    }),
    ("SetShadowQuality%quality", |h, a, _| {
        let (size, full) = match a[0].int() {
            0 => (512, false),
            1 => (1024, false),
            2 => (2048, true),
            3 => (4096, true),
            l => return Err(format!("SetShadowQuality: quality must be 0-3, not {l}")),
        };
        h.world.quality.shadow_size = Some(size);
        h.world.quality.shadow_precision_full = Some(full);
        h.shadows_changed()
    }),
    ("SetShadowResolution%size", |h, a, _| {
        let n = a[0].int();
        if !(64..=8192).contains(&n) {
            return Err(format!("SetShadowResolution: size must be 64-8192, not {n}"));
        }
        h.world.quality.shadow_size = Some(n as usize);
        h.shadows_changed()
    }),
    ("SetShadowBias#bias", |h, a, _| {
        h.world.quality.shadow_bias = Some(a[0].float().max(0.0));
        h.shadows_changed()
    }),
    ("SetShadowSoftness#softness", |h, a, _| {
        h.world.quality.soft_shadows = Some(a[0].float() > 0.0);
        h.shadows_changed()
    }),
    ("SetShadowDistance#distance", |h, a, _| {
        h.world.quality.shadow_distance = Some(a[0].float().max(0.1));
        h.shadows_changed()
    }),
    ("SetShadowCascadeCount%count", |h, a, _| {
        let n = a[0].int();
        if !(1..=3).contains(&n) {
            return Err(format!("SetShadowCascadeCount: Fyrox has 1 to 3 cascades, not {n}"));
        }
        h.world.quality.cascades = n;
        h.shadows_changed()
    }),
    ("EnableShadows", |h, _, _| {
        h.world.quality.shadows = Some(true);
        h.shadows_changed()
    }),
    ("DisableShadows", |h, _, _| {
        h.world.quality.shadows = Some(false);
        h.shadows_changed()
    }),
];

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

    #[test]
    fn shadow_settings() {
        let mut q = Quality { level: 2, ..Quality::default() };
        q.soft_shadows = Some(false);
        q.shadow_size = Some(1024);
        let s = q.settings();
        assert!(!s.csm_settings.pcf && !s.point_soft_shadows);
        assert_eq!((s.csm_settings.size, s.point_shadow_map_size), (1024, 512));
        q.cascades = 1;
        q.shadow_distance = Some(40.0);
        assert_eq!(q.cascade_planes(), [40.0, 40.0, 40.0]);
    }
}
