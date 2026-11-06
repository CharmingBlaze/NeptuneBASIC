//! Sound (`LoadSound`, `PlaySound`, channels, `PlayMusic`) and 3D sound
//! (`CreateListener`, `Load3DSound`, `EmitSound`) on Fyrox's sound engine.
//!
//! A sound is a decoded buffer plus the defaults set with `SoundVolume`,
//! `SoundPan` and `SoundPitch`. Every play creates a Fyrox sound node: the
//! channel. Plain sounds sit at the scene root with no spatial effect; sounds
//! started with `EmitSound` are children of the emitting entity, so they
//! follow it. Channels of non-looping sounds remove themselves when done.
//! WAV, OGG, MP3 and FLAC files are supported.

use fyrox::{
    asset::untyped::ResourceKind,
    core::{pool::Handle, uuid::Uuid},
    graph::SceneGraph,
    scene::{
        base::BaseBuilder,
        node::Node,
        sound::{listener::ListenerBuilder, DataSource, Sound, SoundBuffer, SoundBufferResource, SoundBuilder, Status},
    },
};

use crate::world::Host3D;

#[derive(Clone, Debug)]
pub struct SoundInfo {
    pub buffer: SoundBufferResource,
    pub looping: bool,
    /// 0..1
    pub volume: f32,
    /// -1 left .. 1 right
    pub pan: f32,
    /// Playback rate as a multiple of the file's own rate.
    pub pitch: f64,
    /// Sample rate of the file in Hz (for `SoundPitch`/`ChannelPitch`).
    pub rate: f64,
    /// Loaded with `Load3DSound`.
    pub spatial: bool,
}

/// Decodes a sound file into a buffer. `stream` decodes while playing (music).
pub fn load(path: &str, stream: bool) -> Result<(SoundBufferResource, f64), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read '{path}': {e}"))?;
    let source = DataSource::from_memory(bytes);
    let data = if stream { SoundBuffer::raw_streaming(source) } else { SoundBuffer::raw_generic(source) }
        .map_err(|e| format!("cannot decode sound '{path}': {e:?}"))?;
    let buffer = SoundBufferResource::new_ok(Uuid::new_v4(), ResourceKind::Embedded, data);
    let rate = {
        let data = buffer.data_ref();
        match &*data {
            SoundBuffer::Generic(g) => g.sample_rate() as f64,
            SoundBuffer::Streaming(s) => s.sample_rate() as f64,
        }
    };
    Ok((buffer, rate.max(1.0)))
}

impl Host3D<'_> {
    pub fn sound(&self, s: i32) -> Result<&SoundInfo, String> {
        usize::try_from(s - 1)
            .ok()
            .and_then(|i| self.world.sounds.get(i))
            .and_then(Option::as_ref)
            .ok_or_else(|| format!("Sound {s} does not exist"))
    }

    pub fn sound_mut(&mut self, s: i32) -> Result<&mut SoundInfo, String> {
        usize::try_from(s - 1)
            .ok()
            .and_then(|i| self.world.sounds.get_mut(i))
            .and_then(Option::as_mut)
            .ok_or_else(|| format!("Sound {s} does not exist"))
    }

    pub fn add_sound(&mut self, info: SoundInfo) -> i32 {
        self.world.sounds.push(Some(info));
        self.world.sounds.len() as i32
    }

    /// Starts a channel playing `info`, optionally attached to an entity.
    pub fn play(&mut self, info: &SoundInfo, emitter: i32) -> Result<i32, String> {
        let emitter_node = if emitter != 0 { Some(self.ent(emitter)?.node) } else { None };
        let rolloff = self.world.listener_rolloff;
        let distance = self.world.listener_distance;
        let node = SoundBuilder::new(BaseBuilder::new().with_name("Channel"))
            .with_buffer(Some(info.buffer.clone()))
            .with_looping(info.looping)
            .with_play_once(!info.looping)
            .with_gain(info.volume)
            .with_panning(info.pan)
            .with_pitch(info.pitch)
            .with_spatial_blend_factor(if emitter_node.is_some() { 1.0 } else { 0.0 })
            .with_rolloff_factor(rolloff)
            .with_radius(distance)
            .with_status(Status::Playing)
            .build_node();
        let graph = self.graph()?;
        let h = graph.add_node(node);
        if let Some(parent) = emitter_node {
            graph.link_nodes(h, parent);
        }
        self.world.channels.push(h);
        Ok(self.world.channels.len() as i32)
    }

    /// The sound node of a channel, if it is still alive.
    pub fn channel(&mut self, c: i32) -> Result<Option<&mut Sound>, String> {
        let h = usize::try_from(c - 1).ok().and_then(|i| self.world.channels.get(i)).copied().unwrap_or(Handle::NONE);
        if h.is_none() {
            return Ok(None);
        }
        let graph = self.graph()?;
        Ok(graph.try_get_mut(h).ok().and_then(|n| n.cast_mut::<Sound>()))
    }

    pub fn stop_channel(&mut self, c: i32) -> Result<(), String> {
        if self.channel(c)?.is_some() {
            let h = self.world.channels[c as usize - 1];
            self.graph()?.remove_node(h);
        }
        Ok(())
    }

    /// `CreateListener`: the "ears" for 3D sound, attached to an entity.
    pub fn create_listener(&mut self, parent: i32, rolloff: f32, distance: f32) -> Result<Handle<Node>, String> {
        if let Some(old) = self.world.listener.take() {
            if self.graph()?.try_get(old).is_ok() {
                self.graph()?.remove_node(old);
            }
        }
        self.world.listener_rolloff = rolloff.max(0.0);
        self.world.listener_distance = distance.max(1e-3);
        let parent_node = self.ent(parent)?.node;
        let graph = self.graph()?;
        let l = graph.add_node(ListenerBuilder::new(BaseBuilder::new().with_name("Listener")).build_node());
        graph.link_nodes(l, parent_node);
        self.world.listener = Some(l);
        Ok(l)
    }

    /// Stops sound in an entity's subtree before the entity is freed.
    pub fn forget_channels(&mut self) {
        let channels = self.world.channels.clone();
        let alive: Vec<bool> = match self.scene_opt() {
            Some(scene) => channels.iter().map(|h| scene.graph.try_get(*h).is_ok()).collect(),
            None => return,
        };
        for (h, ok) in self.world.channels.iter_mut().zip(alive) {
            if !ok {
                *h = Handle::NONE;
            }
        }
    }
}

/// Blitz pitch in Hz to a rate multiplier.
pub fn pitch_from_hz(hz: f32, rate: f64) -> f64 {
    if hz <= 0.0 {
        1.0
    } else {
        hz as f64 / rate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny 8 kHz mono WAV file.
    fn wav() -> Vec<u8> {
        let samples: Vec<i16> = (0..800).map(|i| ((i as f32 * 0.3).sin() * 8000.0) as i16).collect();
        let data_len = samples.len() as u32 * 2;
        let mut v = Vec::new();
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&(36 + data_len).to_le_bytes());
        v.extend_from_slice(b"WAVEfmt ");
        v.extend_from_slice(&16u32.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes()); // PCM
        v.extend_from_slice(&1u16.to_le_bytes()); // mono
        v.extend_from_slice(&8000u32.to_le_bytes());
        v.extend_from_slice(&16000u32.to_le_bytes());
        v.extend_from_slice(&2u16.to_le_bytes());
        v.extend_from_slice(&16u16.to_le_bytes());
        v.extend_from_slice(b"data");
        v.extend_from_slice(&data_len.to_le_bytes());
        for s in samples {
            v.extend_from_slice(&s.to_le_bytes());
        }
        v
    }

    #[test]
    fn loads_wav_and_reads_rate() {
        let dir = std::env::temp_dir().join(format!("neptune-snd-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("beep.wav");
        std::fs::write(&path, wav()).unwrap();
        let (_, rate) = load(path.to_str().unwrap(), false).unwrap();
        assert_eq!(rate, 8000.0);
        assert!(load("missing.wav", false).is_err());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn pitch_conversion() {
        assert_eq!(pitch_from_hz(22050.0, 44100.0), 0.5);
        assert_eq!(pitch_from_hz(0.0, 44100.0), 1.0);
    }
}
