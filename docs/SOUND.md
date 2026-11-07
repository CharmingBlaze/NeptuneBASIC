# Sound

Sound in NeptuneBASIC works as in Blitz3D, with extensions for mixing and
effects. Every command is also in the
[command reference](COMMANDS.md#sound).

## Playing sounds

```basic
blip = LoadSound("blip.wav")          ; WAV, OGG, MP3 or FLAC
SoundVolume blip, 0.8                  ; 0 silent .. 1 full
SoundPitch blip, 22050                 ; play rate in Hz (0 = the file's own)
SoundPan blip, -0.5                    ; -1 left .. 1 right
channel = PlaySound(blip)
```

`PlaySound` returns a **channel**: one playing copy of the sound. Change it
while it plays with `ChannelVolume`, `ChannelPan`, `ChannelPitch`,
`PauseChannel`, `ResumeChannel` and `StopChannel`; `ChannelPlaying` says
whether it is still playing. `LoopSound` makes a sound repeat until its
channel is stopped.

Music is streamed from the file instead of being loaded whole:

```basic
music = PlayMusic("theme.ogg", True)  ; True loops it
```

`LoadSoundAsync` loads a sound in the background (see the command
reference).

## 3D sound

Sounds can come from places in the 3D world. The listener (the "ears") is
usually the camera:

```basic
CreateListener camera                  ; also: rolloff factor, distance
engine = Load3DSound("engine.wav")
LoopSound engine
EmitSound engine, car                  ; plays from the car and follows it
```

A sound emitted from an entity gets quieter with distance and moves
between the ears as the entity moves around the listener.

## Mixing with audio buses (ext)

A **bus** is a mixer channel that sounds play into. Bus 0 is the master
bus; everything ends up there. Make a bus for each kind of sound to set
their volumes apart, the way games have music, effects and voice volume
sliders:

```basic
music = CreateAudioBus("music")
sfx = CreateAudioBus("effects")
SoundBus theme, music                  ; channels of this sound play into music
SoundBus blip, sfx
BusVolume music, 0.5
BusVolume 0, 1                         ; the master volume
```

`SoundBus` applies to channels started afterwards; `ChannelBus` moves a
playing channel (for example the one `PlayMusic` returned). A bus can feed
another bus instead of the master: `CreateAudioBus("footsteps", sfx)`.

## Reverb (ext)

Reverb adds the echoes of a space. Give a bus a reverb and everything
playing into it sounds as if it were in that space:

```basic
BusReverb sfx, 0.6, 1, 9000            ; a small room
BusReverb sfx, 2.5, 0.8, 7000          ; a hall
BusReverb sfx, 6, 0.6, 4000            ; a cave
BusReverb sfx, 0                       ; no reverb
```

The values are how long the echoes last (seconds), how much of the
original sound is kept (`dry`, 0-1: lower makes the sound seem farther
away) and the highest frequency the echoes keep (`brightness`, Hz: lower
is duller, as in a room full of soft things).

A common setup: when the player walks into a cave, give the effects bus a
cave reverb, and take it away when they leave. Music usually stays on a
bus without reverb.

## Filters (ext)

A filter changes the tone of a bus:

| Kind | Effect | Use |
|---|---|---|
| `FILTER_LOWPASS` | keeps frequencies below `frequency` | underwater, behind a wall or door, far away |
| `FILTER_HIGHPASS` | keeps frequencies above `frequency` | radio, telephone, a small speaker |
| `FILTER_BANDPASS` | keeps a band around `frequency` | old radio, megaphone |
| `FILTER_LOWSHELF`, `FILTER_HIGHSHELF` | lower or raise the bass or treble by `gain` | tone controls |
| `FILTER_ALLPASS` | shifts phase only | special effects |
| `FILTER_NONE` | removes the filter | |

```basic
BusFilter 0, FILTER_LOWPASS, 500       ; underwater: everything muffled
BusFilter radio, FILTER_HIGHPASS, 1500, 1.2
BusFilter 0, FILTER_NONE               ; back to normal
```

`quality` sets how wide the change is around the frequency (default 0.7)
and `gain` scales the result (default 1). A bus has one filter, which
comes before its reverb.

Changing a bus's filter or reverb starts its effects afresh, so the echoes
of a reverb stop; changing its volume does not.

## Example

`examples/soundfx.nb` plays a hum on a music bus and blips on an effects
bus: keys 1-3 choose a room, hall or cave reverb, U goes underwater, R
turns the music into a radio, and Up/Down change the music volume.

## Not supported

* CD audio (`PlayCDTrack` returns no channel).
* Head-related (HRTF) 3D sound: Fyrox supports it, but it needs a measured
  head-response data file that NeptuneBASIC does not ship.
