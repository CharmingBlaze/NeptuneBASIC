//! Audio buses and sound effects (ext): `CreateAudioBus`, `BusVolume`,
//! `BusReverb`, `BusFilter`, `SoundBus`, `ChannelBus`.
//!
//! Every channel plays into a bus. Bus 0 is the master bus that everything
//! ends up in; `CreateAudioBus` makes more (for example music, effects and
//! voices, each with its own volume), and a bus can feed another bus.
//! A bus can have a filter (low-pass muffles sound, as underwater or
//! behind a wall; high-pass thins it, as through a radio) and a reverb
//! (echoes of a room, a hall or a cave); the filter comes first.
//!
//! The settings live here and are copied into the sound engine of each
//! scene: at once into the current scene, and into another scene when a
//! sound first plays there after a change. Changing a bus's effects starts
//! them afresh (a reverb's echoes stop); changing its volume does not.

use std::collections::HashMap;

use fyrox::asset::untyped::ResourceKind;
use fyrox::scene::sound::{
    filter::{AllPassFilterEffect, BandPassFilterEffect, HighPassFilterEffect, HighShelfFilterEffect, LowPassFilterEffect, LowShelfFilterEffect},
    reverb::Reverb as FyroxReverb,
    AudioBus, AudioBusGraph, Effect, HrirSphere, HrirSphereResource, HrirSphereResourceExt, HrtfRenderer, Renderer, SAMPLE_RATE as ENGINE_RATE,
};

use crate::world::Host3D;

/// `BusFilter` kinds.
pub const FILTER_NONE: i32 = 0;
pub const FILTER_LOWPASS: i32 = 1;
pub const FILTER_HIGHPASS: i32 = 2;
pub const FILTER_BANDPASS: i32 = 3;
pub const FILTER_LOWSHELF: i32 = 4;
pub const FILTER_HIGHSHELF: i32 = 5;
pub const FILTER_ALLPASS: i32 = 6;

/// Sample rate of the sound engine (Hz).
const SAMPLE_RATE: f32 = 44100.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Reverb {
    pub decay: f32,
    pub dry: f32,
    pub brightness: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Filter {
    pub kind: i32,
    pub frequency: f32,
    pub quality: f32,
    pub gain: f32,
}

#[derive(Clone, Debug)]
pub struct Bus {
    pub name: String,
    pub parent: i32,
    pub volume: f32,
    pub filter: Option<Filter>,
    pub reverb: Option<Reverb>,
    /// Changed whenever the effects change.
    pub fx_gen: u32,
}

/// All buses; index 0 is the master bus.
#[derive(Debug)]
pub struct Buses {
    pub list: Vec<Option<Bus>>,
    /// (scene, bus) → effects generation copied into that scene
    applied: HashMap<(usize, i32), u32>,
    /// Changed whenever anything changes; per scene, what was copied.
    version: u32,
    synced: HashMap<usize, u32>,
    /// `SetHRTF`: head measurements for 3D sound, if any.
    hrtf: Option<HrirSphereResource>,
    /// Changed by `SetHRTF`; per scene, what was copied.
    hrtf_gen: u32,
    hrtf_applied: HashMap<usize, u32>,
}

impl Default for Buses {
    fn default() -> Self {
        let master = Bus { name: AudioBusGraph::PRIMARY_BUS.to_string(), parent: -1, volume: 1.0, filter: None, reverb: None, fx_gen: 0 };
        Buses { list: vec![Some(master)], applied: HashMap::new(), version: 0, synced: HashMap::new(), hrtf: None, hrtf_gen: 0, hrtf_applied: HashMap::new() }
    }
}

impl Buses {
    pub fn get(&self, b: i32) -> Result<&Bus, String> {
        usize::try_from(b).ok().and_then(|i| self.list.get(i)).and_then(Option::as_ref).ok_or_else(|| format!("Audio bus {b} does not exist"))
    }

    fn get_mut(&mut self, b: i32) -> Result<&mut Bus, String> {
        self.version += 1;
        usize::try_from(b).ok().and_then(|i| self.list.get_mut(i)).and_then(Option::as_mut).ok_or_else(|| format!("Audio bus {b} does not exist"))
    }
}

/// The Fyrox effect for a filter setting.
fn filter_effect(f: &Filter) -> Option<Effect> {
    macro_rules! make {
        ($t:ty, $v:ident) => {{
            let mut e = <$t>::default();
            e.set_cutoff_frequency_hz(f.frequency.clamp(10.0, SAMPLE_RATE * 0.49));
            e.set_quality(f.quality.max(0.01));
            e.set_gain(f.gain.max(0.0));
            Some(Effect::$v(e))
        }};
    }
    match f.kind {
        FILTER_LOWPASS => make!(LowPassFilterEffect, LowPassFilter),
        FILTER_HIGHPASS => make!(HighPassFilterEffect, HighPassFilter),
        FILTER_BANDPASS => make!(BandPassFilterEffect, BandPassFilter),
        FILTER_LOWSHELF => make!(LowShelfFilterEffect, LowShelfFilter),
        FILTER_HIGHSHELF => make!(HighShelfFilterEffect, HighShelfFilter),
        FILTER_ALLPASS => make!(AllPassFilterEffect, AllPassFilter),
        _ => None,
    }
}

fn reverb_effect(r: &Reverb) -> Effect {
    let mut e = FyroxReverb::new();
    e.set_decay_time(r.decay.max(0.01));
    e.set_dry(r.dry.clamp(0.0, 1.0));
    e.set_fc((r.brightness / SAMPLE_RATE).clamp(0.001, 0.49));
    Effect::Reverb(e)
}

impl Host3D<'_> {
    /// `CreateAudioBus`: a new bus feeding `parent` (0 = the master bus).
    pub fn create_audio_bus(&mut self, name: &str, parent: i32) -> Result<i32, String> {
        self.world.audio.get(parent)?;
        let id = self.world.audio.list.len() as i32;
        let name = if name.is_empty() { format!("Bus {id}") } else { name.to_string() };
        if self.world.audio.list.iter().flatten().any(|b| b.name == name) {
            return Err(format!("CreateAudioBus: there is already a bus named '{name}'"));
        }
        self.world.audio.list.push(Some(Bus { name, parent, volume: 1.0, filter: None, reverb: None, fx_gen: 0 }));
        self.world.audio.version += 1;
        self.sync_buses()?;
        Ok(id)
    }

    pub fn bus_volume(&mut self, b: i32, volume: f32) -> Result<(), String> {
        self.world.audio.get_mut(b)?.volume = volume.max(0.0);
        self.sync_buses()
    }

    /// `BusReverb`: decay 0 removes the reverb.
    pub fn bus_reverb(&mut self, b: i32, decay: f32, dry: f32, brightness: f32) -> Result<(), String> {
        let bus = self.world.audio.get_mut(b)?;
        bus.reverb = (decay > 0.0).then_some(Reverb { decay, dry, brightness });
        bus.fx_gen += 1;
        self.sync_buses()
    }

    /// `BusFilter`: kind `FILTER_NONE` removes the filter.
    pub fn bus_filter(&mut self, b: i32, kind: i32, frequency: f32, quality: f32, gain: f32) -> Result<(), String> {
        if !(FILTER_NONE..=FILTER_ALLPASS).contains(&kind) {
            return Err(format!("BusFilter: kind must be 0 to 6, not {kind}"));
        }
        let bus = self.world.audio.get_mut(b)?;
        bus.filter = (kind != FILTER_NONE).then_some(Filter { kind, frequency, quality, gain });
        bus.fx_gen += 1;
        self.sync_buses()
    }

    /// Name of a bus (for the sound nodes playing into it).
    pub fn bus_name(&self, b: i32) -> Result<String, String> {
        Ok(self.world.audio.get(b)?.name.clone())
    }

    /// `SetHRTF`: 3D sound through head measurements from a file, or back to
    /// plain panning with no file. 1 if it worked, 0 if the file is unusable
    /// (which leaves the sound as it was).
    pub fn set_hrtf(&mut self, file: &str) -> Result<i32, String> {
        let hrtf = if file.is_empty() {
            None
        } else {
            match HrirSphere::from_file(file, ENGINE_RATE) {
                Ok(sphere) => Some(HrirSphereResource::from_hrir_sphere(sphere, ResourceKind::Embedded)),
                Err(_) => return Ok(0),
            }
        };
        self.world.audio.hrtf = hrtf;
        self.world.audio.hrtf_gen += 1;
        self.world.audio.version += 1;
        self.sync_buses()?;
        Ok(1)
    }

    /// Copies the bus settings into the current scene's sound engine if
    /// anything changed since it was last done for this scene.
    pub fn sync_buses(&mut self) -> Result<(), String> {
        let scene_id = self.world.current_scene;
        let version = self.world.audio.version;
        if self.world.audio.synced.get(&scene_id) == Some(&version) {
            return Ok(());
        }
        let buses: Vec<(i32, Bus)> = self.world.audio.list.iter().enumerate().filter_map(|(i, b)| b.clone().map(|b| (i as i32, b))).collect();
        let done: HashMap<i32, u32> = buses.iter().filter_map(|(id, _)| self.world.audio.applied.get(&(scene_id, *id)).map(|g| (*id, *g))).collect();
        let mut applied = Vec::new();
        let hrtf_gen = self.world.audio.hrtf_gen;
        let new_renderer = (self.world.audio.hrtf_applied.get(&scene_id).copied().unwrap_or(0) != hrtf_gen).then(|| self.world.audio.hrtf.clone().map_or(Renderer::Default, |r| Renderer::HrtfRenderer(HrtfRenderer::new(r))));
        {
            let scene = self.scene()?;
            let mut state = scene.graph.sound_context.state();
            if let Some(renderer) = new_renderer {
                state.set_renderer(renderer);
            }
            let graph = state.bus_graph_mut();
            let mut handles = HashMap::new();
            for (id, bus) in &buses {
                let found = graph.buses_pair_iter().find(|(_, b)| b.name() == bus.name).map(|(h, _)| h);
                let handle = match found {
                    Some(h) => h,
                    None => {
                        let parent = handles.get(&bus.parent).copied().unwrap_or_else(|| graph.primary_bus_handle());
                        graph.add_bus(AudioBus::new(bus.name.clone()), parent)
                    }
                };
                handles.insert(*id, handle);
                let Ok(fy) = graph.try_get_bus_mut(handle) else { continue };
                fy.set_gain(bus.volume);
                if done.get(id) != Some(&bus.fx_gen) {
                    while fy.effect(0).is_some() {
                        fy.remove_effect(0);
                    }
                    if let Some(e) = bus.filter.as_ref().and_then(filter_effect) {
                        fy.add_effect(e);
                    }
                    if let Some(r) = &bus.reverb {
                        fy.add_effect(reverb_effect(r));
                    }
                    applied.push((*id, bus.fx_gen));
                }
            }
        }
        for (id, gen) in applied {
            self.world.audio.applied.insert((scene_id, id), gen);
        }
        self.world.audio.synced.insert(scene_id, version);
        self.world.audio.hrtf_applied.insert(scene_id, hrtf_gen);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_map_to_effects() {
        let f = |kind| Filter { kind, frequency: 800.0, quality: 0.7, gain: 1.0 };
        assert!(filter_effect(&f(FILTER_NONE)).is_none());
        assert!(matches!(filter_effect(&f(FILTER_LOWPASS)), Some(Effect::LowPassFilter(_))));
        assert!(matches!(filter_effect(&f(FILTER_HIGHPASS)), Some(Effect::HighPassFilter(_))));
        assert!(matches!(filter_effect(&f(FILTER_ALLPASS)), Some(Effect::AllPassFilter(_))));
        // an absurd frequency is clamped below the Nyquist limit
        if let Some(Effect::LowPassFilter(e)) = filter_effect(&Filter { kind: FILTER_LOWPASS, frequency: 1e9, quality: 0.7, gain: 1.0 }) {
            assert!(e.cutoff_frequency_hz() < SAMPLE_RATE * 0.5);
        }
    }

    #[test]
    fn reverb_settings() {
        let Effect::Reverb(r) = reverb_effect(&Reverb { decay: 3.0, dry: 2.0, brightness: 6000.0 }) else { panic!("not a reverb") };
        assert_eq!(r.decay_time(), 3.0);
        assert_eq!(r.get_dry(), 1.0);
        assert!((r.fc() - 6000.0 / SAMPLE_RATE).abs() < 1e-6);
    }

    #[test]
    fn master_bus_exists() {
        let b = Buses::default();
        assert_eq!(b.get(0).unwrap().name, AudioBusGraph::PRIMARY_BUS);
        assert!(b.get(1).is_err());
    }
}
