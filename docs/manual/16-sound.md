# 16. Sound and movies

[Contents](../MANUAL.md) · Previous: [15. Input](15-input.md) · Next: [17. 3D basics](17-3d-basics.md)

Sound makes a game feel alive. This chapter covers sound effects, music,
sounds placed in a 3D world, mixing them, and playing movies. The
[sound guide](../SOUND.md) goes further into mixing, reverb and filters.

- [Sound effects](#sound-effects)
- [Channels](#channels)
- [Music](#music)
- [Sound in 3D](#sound-in-3d)
- [Mixing: buses, reverb and filters](#mixing-buses-reverb-and-filters)
- [Movies](#movies)

## Sound effects

```basic
Graphics 800, 600
jump = LoadSound("examples/media/blip.wav")
If jump = 0 Then RuntimeError "Could not load blip.wav"
While Not KeyHit(KEY_ESCAPE)
    If KeyHit(KEY_SPACE) Then PlaySound jump
    Cls
    Text 10, 10, "Press Space"
    Flip
Wend
```

`LoadSound(file$)` loads WAV, OGG, MP3 and FLAC files; load sounds once,
at the start, and play them as often as you like. Settings made on the
sound apply each time it is played:

| Command | Does |
|---|---|
| `PlaySound(sound)` | plays it, and returns the **channel** it plays on |
| `SoundVolume sound, volume` | 0 silent to 1 full |
| `SoundPan sound, pan` | -1 left, 0 centre, 1 right |
| `SoundPitch sound, hz` | its playback rate in samples per second: higher is higher and faster |
| `LoopSound sound` | makes it repeat until stopped |
| `FreeSound sound` | frees it |

## Channels

Each time a sound plays, it plays on a new **channel**. The channel
controls that one playing copy: stop it, pause it, or change its volume
while it plays.

```basic
engine = LoadSound("examples/media/hum.wav")
LoopSound engine
channel = PlaySound(engine)
; later, as the car speeds up:
speed# = 0.5
ChannelPitch channel, 22050 + speed * 22050
ChannelVolume channel, 0.5 + speed * 0.5
; when the car stops:
StopChannel channel
```

| Command | Does |
|---|---|
| `StopChannel ch`, `PauseChannel ch`, `ResumeChannel ch` | stops, pauses, resumes |
| `ChannelVolume ch, v`, `ChannelPan ch, p`, `ChannelPitch ch, hz` | changes it while it plays |
| `ChannelPlaying(ch)` | 1 while it is still playing |

A little variety stops repeated sounds getting tiring:
`ChannelPitch PlaySound(step), Rand(40000, 48000)`.

## Music

`PlayMusic(file$ [, mode])` streams a music file from disk (it is not
loaded into memory all at once) and returns its channel. Mode 1 loops it.

```basic
music = PlayMusic("music/theme.ogg", 1)
ChannelVolume music, 0.6
```

## Sound in 3D

In a 3D game, sounds can come from a place: they get quieter with
distance and move between the speakers as the camera turns.

```basic
Graphics3D 1280, 720
camera = CreateCamera()
CreateListener camera                     ; the ears are on the camera

waterfall = CreateCube()
PositionEntity waterfall, 10, 0, 20
splash = Load3DSound("examples/media/hum.wav")
LoopSound splash
EmitSound splash, waterfall               ; plays from the waterfall's position

While Not KeyHit(KEY_ESCAPE)
    If KeyDown(KEY_LEFT) Then TurnEntity camera, 0, 2, 0
    If KeyDown(KEY_RIGHT) Then TurnEntity camera, 0, -2, 0
    RenderWorld
    Flip
Wend
```

- `CreateListener(parent [, rolloff, doppler, distance_scale])` puts the
  listener (the player's ears) on an entity, usually the camera.
- `Load3DSound(file$)` loads a sound for 3D use.
- `EmitSound(sound, entity)` plays it from the entity's position, and
  returns its channel.

## Mixing: buses, reverb and filters

An **audio bus** is a mixer channel that sounds play into. Put music,
sound effects and voices on their own buses, and each can have its own
volume slider in the options menu, its own echo, or a muffling filter.

```basic
music = CreateAudioBus("music")
effects = CreateAudioBus("effects")
BusVolume music, 0.5                    ; the music slider
BusVolume effects, 0.9                  ; the effects slider

boom = LoadSound("explosion.wav")
SoundBus boom, effects                  ; this sound always plays into effects

; the player walks into a cave
BusReverb effects, 2.5, 0.7             ; echoes lasting 2.5 seconds

; the player dives under water
BusFilter effects, FILTER_LOWPASS, 600  ; everything sounds muffled
```

| Command | Does |
|---|---|
| `CreateAudioBus([name$, parent])` | a new bus, feeding the master bus (0) or another bus |
| `BusVolume bus, volume` | its volume |
| `BusReverb bus, seconds [, dry, brightness]` | echoes, as in a room or cave; 0 seconds removes them |
| `BusFilter bus, kind [, frequency, quality, gain]` | `FILTER_LOWPASS` muffles, `FILTER_HIGHPASS` thins (a radio), and others; `FILTER_NONE` removes it |
| `SoundBus sound, bus` | where the sound plays from now on |
| `ChannelBus channel, bus` | moves a playing channel |

The [sound guide](../SOUND.md) explains reverb and filter settings in
detail.

## Movies

`OpenMovie(file$)` opens a movie: an animated GIF, a Motion-JPEG AVI file,
or a folder of numbered images. `DrawMovie` draws its current frame each
frame, and returns 0 once it has ended:

```basic
Graphics 1280, 720
intro = OpenMovie("intro.avi")
While DrawMovie(intro, 0, 0, GraphicsWidth(), GraphicsHeight()) And Not KeyHit(KEY_SPACE)
    Flip
Wend
CloseMovie intro
```

`MovieWidth`, `MovieHeight` and `MoviePlaying` tell you about it; a
width and height of -1 draw it at its own size.

[Contents](../MANUAL.md) · Previous: [15. Input](15-input.md) · Next: [17. 3D basics](17-3d-basics.md)
