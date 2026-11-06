//! Property animation tracks (ext): `CreateTrack`, `TrackKey`, `PlayTrack`,
//! `SetTrackTime`, `TrackTime`, `TrackPlaying`, `FreeTrack`.
//!
//! A track animates one property of an entity through keys (frame, value),
//! interpolated in a straight line between keys and played like `Animate`
//! (loop, ping-pong or once, in frames of 1/60 s advanced by `UpdateWorld`).
//! Any entity can have tracks, models included, alongside its animation.
//!
//! Properties (values per key): position, rotation, scale (x, y, z); color
//! (red, green, blue); alpha; shininess; emissive (red, green, blue);
//! lightcolor (red, green, blue); lightrange; camerazoom; fogrange (near,
//! far); texture (u, v offset of the entity's first texture);
//! texturerotation (degrees).

use neptune_lang::commands::{parse_sig, ConstVal};
use neptune_vm::{NativeCtx, Value};

use crate::world::Host3D;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Prop {
    Position,
    Rotation,
    Scale,
    Color,
    Alpha,
    Shininess,
    Emissive,
    LightColor,
    LightRange,
    CameraZoom,
    FogRange,
    Texture,
    TextureRotation,
}

impl Prop {
    pub fn parse(s: &str) -> Result<Prop, String> {
        Ok(match s.trim().to_lowercase().as_str() {
            "position" => Prop::Position,
            "rotation" => Prop::Rotation,
            "scale" => Prop::Scale,
            "color" | "colour" => Prop::Color,
            "alpha" => Prop::Alpha,
            "shininess" => Prop::Shininess,
            "emissive" => Prop::Emissive,
            "lightcolor" | "lightcolour" => Prop::LightColor,
            "lightrange" => Prop::LightRange,
            "camerazoom" => Prop::CameraZoom,
            "fogrange" => Prop::FogRange,
            "texture" => Prop::Texture,
            "texturerotation" => Prop::TextureRotation,
            other => {
                return Err(format!(
                    "'{other}' is not a track property (position, rotation, scale, color, alpha, shininess, emissive, lightcolor, lightrange, camerazoom, fogrange, texture, texturerotation)"
                ))
            }
        })
    }

    /// The command that sets the property and how many values it takes.
    fn command(self) -> (&'static str, usize) {
        match self {
            Prop::Position => ("PositionEntity", 3),
            Prop::Rotation => ("RotateEntity", 3),
            Prop::Scale => ("ScaleEntity", 3),
            Prop::Color => ("EntityColor", 3),
            Prop::Alpha => ("EntityAlpha", 1),
            Prop::Shininess => ("EntityShininess", 1),
            Prop::Emissive => ("EntityEmissive", 3),
            Prop::LightColor => ("LightColor", 3),
            Prop::LightRange => ("LightRange", 1),
            Prop::CameraZoom => ("CameraZoom", 1),
            Prop::FogRange => ("CameraFogRange", 2),
            Prop::Texture => ("PositionTexture", 2),
            Prop::TextureRotation => ("RotateTexture", 1),
        }
    }

    /// Texture properties act on the entity's first texture.
    fn on_texture(self) -> bool {
        matches!(self, Prop::Texture | Prop::TextureRotation)
    }
}

#[derive(Debug, Clone)]
pub struct Track {
    pub entity: i32,
    pub prop: Prop,
    /// (frame, values), sorted by frame.
    pub keys: Vec<(f32, [f32; 3])>,
    pub time: f32,
    pub speed: f32,
    pub mode: i32,
    pub dir: f32,
}

impl Track {
    pub fn length(&self) -> f32 {
        self.keys.last().map_or(0.0, |k| k.0)
    }

    /// Values at `frame` (holding the first and last keys outside them).
    pub fn value(&self, frame: f32) -> Option<[f32; 3]> {
        let first = self.keys.first()?;
        if frame <= first.0 {
            return Some(first.1);
        }
        for w in self.keys.windows(2) {
            let ((f0, a), (f1, b)) = (w[0], w[1]);
            if frame <= f1 {
                let t = if f1 > f0 { (frame - f0) / (f1 - f0) } else { 1.0 };
                return Some([0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t));
            }
        }
        self.keys.last().map(|k| k.1)
    }

    pub fn add_key(&mut self, frame: f32, v: [f32; 3]) {
        self.keys.retain(|k| k.0 != frame);
        let i = self.keys.partition_point(|k| k.0 < frame);
        self.keys.insert(i, (frame, v));
    }
}

/// Runs a runtime command by name, filling in its default arguments.
fn call(h: &mut Host3D, name: &str, mut args: Vec<Value>, ctx: &mut NativeCtx) -> Result<(), String> {
    let (sig, f) = crate::commands::table()
        .iter()
        .find(|(s, _)| parse_sig(s).name.eq_ignore_ascii_case(name))
        .ok_or_else(|| format!("command {name} missing"))?;
    for p in parse_sig(sig).params.iter().skip(args.len()) {
        args.push(match &p.default {
            Some(ConstVal::Int(i)) => Value::Int(*i),
            Some(ConstVal::Float(x)) => Value::Float(*x),
            Some(ConstVal::Str(s)) => Value::Str(s.to_string().into()),
            None => Value::Int(0),
        });
    }
    f(h, &args, ctx).map(|_| ())
}

impl Host3D<'_> {
    pub fn track_mut(&mut self, t: i32) -> Result<&mut Track, String> {
        usize::try_from(t - 1)
            .ok()
            .and_then(|i| self.world.tracks.get_mut(i))
            .and_then(Option::as_mut)
            .ok_or_else(|| format!("Track {t} does not exist"))
    }

    /// Sets the track's property to its value at `time`.
    pub fn apply_track(&mut self, t: i32, ctx: &mut NativeCtx) -> Result<(), String> {
        let tr = self.track_mut(t)?.clone();
        let Some(v) = tr.value(tr.time) else { return Ok(()) };
        if self.ent(tr.entity).is_err() {
            // the entity was freed: the track stops
            self.track_mut(t)?.mode = 0;
            return Ok(());
        }
        let (name, n) = tr.prop.command();
        let target = if tr.prop.on_texture() {
            let tex = self.ent(tr.entity)?.textures[0];
            if tex == 0 {
                return Ok(());
            }
            tex
        } else {
            tr.entity
        };
        let mut args = vec![Value::Int(target)];
        args.extend(v.iter().take(n).map(|x| Value::Float(*x)));
        call(self, name, args, ctx)
    }

    /// Advances and applies every playing track (called by `UpdateWorld`).
    pub fn update_tracks(&mut self, frames: f32, ctx: &mut NativeCtx) -> Result<(), String> {
        let playing: Vec<i32> = self.world.tracks.iter().enumerate().filter(|(_, t)| t.as_ref().is_some_and(|t| t.mode != 0)).map(|(i, _)| i as i32 + 1).collect();
        for t in playing {
            let tr = self.track_mut(t)?;
            let len = tr.length().max(1e-3);
            let (time, dir, stop) = crate::anim::advance(tr.time, tr.dir, tr.speed, frames, len, tr.mode);
            (tr.time, tr.dir) = (time, dir);
            if stop {
                tr.mode = 0;
            }
            self.apply_track(t, ctx)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_interpolate_and_stay_sorted() {
        let mut t = Track { entity: 1, prop: Prop::Color, keys: Vec::new(), time: 0.0, speed: 1.0, mode: 0, dir: 1.0 };
        t.add_key(10.0, [255.0, 0.0, 0.0]);
        t.add_key(0.0, [0.0, 0.0, 255.0]);
        t.add_key(10.0, [255.0, 255.0, 0.0]);
        assert_eq!(t.keys.len(), 2);
        assert_eq!(t.length(), 10.0);
        assert_eq!(t.value(5.0), Some([127.5, 127.5, 127.5]));
        assert_eq!(t.value(-3.0), Some([0.0, 0.0, 255.0]));
        assert_eq!(t.value(99.0), Some([255.0, 255.0, 0.0]));
        assert!(Prop::parse("LightColour").is_ok() && Prop::parse("wobble").is_err());
    }
}
