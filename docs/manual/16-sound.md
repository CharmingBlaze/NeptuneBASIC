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
- [Try it yourself](#try-it-yourself)

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

## Try it yourself

These use the two small sounds in the repository, `examples/media/blip.wav`
and `examples/media/hum.wav`; run them from the folder that holds `examples`.

1. **Blip piano.** Make the keys A, S, D and F each play the blip at a different
   pitch (a low note, and notes that are higher), and show the keys on the
   screen as four coloured boxes. Escape quits.
   *Hint: `PlaySound` returns the channel, and `ChannelPitch` changes the pitch of that one playing copy; the blip is recorded at 22050.*
2. **Engine.** Loop the hum as an engine. Holding the up arrow makes the
   engine speed up (higher pitch and louder), and letting go slows it down
   again. Draw a bar showing the engine speed.
   *Hint: keep the speed in a variable from 0 to 1, and set `ChannelPitch` and `ChannelVolume` from it each frame.*
3. **Cave echo.** Put the blip on its own audio bus called `effects`. Space plays
   the blip, `C` switches echo on and off, and the up and down arrows change
   the volume of the bus. Show the state on the screen.
   *Hint: `BusReverb` with 0 seconds removes the echo.*

### Answers

1.

```basic
Graphics 800, 600
blip = LoadSound("examples/media/blip.wav")
If blip = 0 Then RuntimeError "Could not load blip.wav"

Dim keys(3)
keys(0) = KEY_A : keys(1) = KEY_S : keys(2) = KEY_D : keys(3) = KEY_F
Dim pitch(3)
pitch(0) = 22050        ; the sound as recorded
pitch(1) = 27775        ; a bit higher
pitch(2) = 33075        ; higher still
pitch(3) = 44100        ; double: one octave up
Dim names$(3)
names(0) = "A" : names(1) = "S" : names(2) = "D" : names(3) = "F"

While Not KeyHit(KEY_ESCAPE)
    For i = 0 To 3
        If KeyHit(keys(i)) Then ChannelPitch PlaySound(blip), pitch(i)
    Next

    Cls
    For i = 0 To 3
        If KeyDown(keys(i)) Then Color 255, 220, 60 Else Color 70, 70, 110
        Rect 180 + i * 110, 240, 90, 120
        Color 255, 255, 255
        Text 225 + i * 110, 295, names(i), True, True
    Next
    Text 10, 10, "Play with A S D F. Escape quits."
    Flip
Wend
```

The four keys and four pitches are kept in arrays, so one loop handles all of
them. `KeyHit` fires once per press, which starts one copy of the sound, and
`ChannelPitch` changes the speed of that copy only. A playback rate of
44100 is twice 22050, which is one octave higher. The second loop draws each
box brighter while its key is held down.

2.

```basic
Graphics 800, 600
engine = LoadSound("examples/media/hum.wav")
If engine = 0 Then RuntimeError "Could not load hum.wav"
LoopSound engine
channel = PlaySound(engine)

speed# = 0
While Not KeyHit(KEY_ESCAPE)
    If KeyDown(KEY_UP)
        speed = speed + 0.01
    Else
        speed = speed - 0.01
    EndIf
    speed = Clamp(speed, 0, 1)

    ChannelPitch channel, 22050 + speed * 22050
    ChannelVolume channel, 0.3 + speed * 0.7

    Cls
    Color 255, 255, 255
    Text 10, 10, "Hold the up arrow to rev the engine. Escape quits."
    Rect 100, 280, 600, 40, False
    Color 255, 120, 40
    Rect 100, 280, 600 * speed, 40
    Flip
Wend
StopChannel channel
```

The engine is one looping sound whose channel is changed every frame. At
speed 0 it plays at its normal rate and at 30% volume; at speed 1 it plays
twice as fast and at full volume. Adding or taking away a little each frame
makes the speed rise and fall smoothly, and `Clamp` keeps it between 0 and 1.
`StopChannel` after the loop silences it when the game ends.

3.

```basic
Graphics 800, 600
blip = LoadSound("examples/media/blip.wav")
If blip = 0 Then RuntimeError "Could not load blip.wav"

effects = CreateAudioBus("effects")
SoundBus blip, effects

volume# = 0.8
echo = False
BusVolume effects, volume

While Not KeyHit(KEY_ESCAPE)
    If KeyHit(KEY_SPACE) Then PlaySound blip
    If KeyHit(KEY_C)
        echo = Not echo
        BusReverb effects, IIf(echo, 2.5, 0), 0.7
    EndIf
    If KeyDown(KEY_UP) Then volume = volume + 0.01
    If KeyDown(KEY_DOWN) Then volume = volume - 0.01
    volume = Clamp(volume, 0, 1)
    BusVolume effects, volume

    Cls
    Color 255, 255, 255
    Text 10, 10, "Space: blip   C: echo on/off   Up/Down: volume   Escape: quit"
    Text 10, 40, "Echo: " + IIf(echo, "ON (a big cave)", "off")
    Text 10, 60, "Volume: " + Int(volume * 100) + "%"
    Flip
Wend
```

`SoundBus` sends the blip into the `effects` bus, so the bus settings change
how it sounds, whatever else is playing. `Not echo` flips the switch each
time `C` is pressed, and `IIf` chooses 2.5 seconds of echo or 0 seconds, which
turns it off. The volume is kept between 0 and 1 with `Clamp`.

[Contents](../MANUAL.md) · Previous: [15. Input](15-input.md) · Next: [17. 3D basics](17-3d-basics.md)
