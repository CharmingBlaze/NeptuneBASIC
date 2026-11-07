# 1. Introduction

[Contents](../SHADERS.md) · Next: [2. First steps](02-first-steps.md)

This chapter explains the ideas behind the shader system without any code
you need to type. If a word later in the manual is unfamiliar, come back
here: the [glossary](#glossary) at the end explains them all.

## How a 3D picture is made

Every frame, NeptuneBASIC hands the scene to the graphics card (the
**GPU**). The GPU draws each object in two steps:

1. **Vertices.** An object is made of triangles, and the corners of the
   triangles are **vertices**. The GPU works out where each vertex lands
   on the screen.
2. **Pixels.** The GPU fills in each triangle, pixel by pixel, and works
   out the colour of every pixel: what colour the surface is, how much
   light reaches it, whether it is in shadow.

A **shader** is a small program the GPU runs for those steps. A **vertex
shader** runs once per vertex (step 1); a **fragment shader** runs once
per pixel (step 2). "Fragment" is the graphics word for "a pixel of a
triangle that is about to be drawn".

Every object you have ever drawn in NeptuneBASIC already used shaders: the
engine has its own. The shader system lets you choose different ones, set
them up, or write your own.

## Materials

A **material** is "a shader plus its settings". The same shader can make a
red plastic ball and a gold ring: only the settings differ.

```basic
gold = CreatePBRMaterial()      ; a material...
SetAlbedo gold, 255, 200, 90    ; ...with its colour set to gold
SetMetallic gold, 1             ; ...and made of metal
```

You draw an entity with a material using `EntityMaterial`. Many entities
can share one material; change the material and they all change.

The settings of a material are called its **parameters**. Inside a shader
they are called **uniforms**, because they are the same ("uniform") for
every vertex and pixel of a draw.

## Textures

A **texture** is a picture wrapped onto a surface. Materials use textures
for more than colour:

| Texture | What it stores |
|---|---|
| albedo (colour) map | the colour of each point |
| normal map | tiny bumps and dents, as directions (looks purple-blue) |
| roughness map | which parts are shiny and which are matte |
| metallic map | which parts are metal |
| ambient occlusion (AO) map | creases and corners that light cannot reach |
| height map | how high each point is (white is high) |
| emissive map | which parts glow |
| opacity map | which parts are see-through |

A **cube map** is six square pictures making a box around the scene, used
for reflections and skies.

## Post-processing

A **post-processing effect** (or screen effect) works on the finished
picture, after the scene is drawn: it can blur it, add glow around bright
lights (bloom), make it look like an old TV, add rain, and so on. Effects
run in a list called a **chain**: each one takes the picture the one
before it made.

## Shadows

Lights can cast real-time **shadows**: objects block light from reaching
the objects behind them. The engine works them out every frame. Chapter 7
explains how, and how to set them up.

## Three ways to get a look

You can get most looks in three ways, from easiest to most flexible:

| Way | How | Good for |
|---|---|---|
| **Ready-made materials** | `CreatePBRMaterial`, `CreateToonMaterial`, `CreateWaterMaterial`, `CreateFoliageMaterial`, `CreateProceduralShader`, then `Set...` commands | most things: realistic surfaces, cartoon looks, water, grass, patterns |
| **Built-in shaders** | `LoadShader("builtin:hologram")`, then `ShaderSet...` commands | special effects: holograms, force fields, fire, dissolving, glass |
| **Your own shaders** | write GLSL in a `.glsl` file and `LoadShader` it | anything else |

Start with the first. Move down the list only when you need to.

Screen effects work the same way: 30 built-in effects
(`LoadPostProcess("builtin:bloom")`), or your own.

## What the engine can do

NeptuneBASIC draws with the Fyrox engine, through OpenGL 3.3 on Windows,
Linux and macOS. That decides a few things:

| | |
|---|---|
| Shader language the GPU runs | GLSL 3.30 |
| Languages you can write in | GLSL (with NeptuneBASIC's additions), WGSL (translated to GLSL for you), and Fyrox's own `.shader` files |
| Shader stages | vertex and fragment |
| Lighting | the realistic **PBR** model with shadows for most objects, plus simpler models (Lambert, Phong, toon, unlit) for custom shaders |
| Shadows | from directional, spot and point lights, hard or soft-edged |
| Screen effects | HDR with tone mapping, bloom, ambient occlusion and anti-aliasing built in, and any number of extra effects |

Two kinds of shader that some engines have are not available, because
OpenGL 3.3 in Fyrox has no place for them:

- **Geometry shaders** (making new triangles on the GPU). Build the
  triangles with the mesh commands (`CreateMesh`, `AddVertex`,
  `AddTriangle`) instead, or move existing vertices in a vertex shader.
- **Compute shaders** (general calculations on the GPU). Use a
  post-processing effect that draws into a texture
  (`PostProcessOutput`), or calculate on the CPU, as
  `BakeProceduralTexture` does.

The commands `ShaderGeometrySource` and `ShaderComputeSource` exist so
that programs written for other engines get a clear error message instead
of silently doing nothing.

## Glossary

| Word | Meaning |
|---|---|
| **albedo** | the base colour of a surface, without any lighting |
| **alpha** | how opaque a colour is: 1 (or 255) solid, 0 invisible |
| **ambient occlusion (AO)** | darkening in creases and corners where light from around the scene cannot reach |
| **blend mode** | how a see-through surface mixes with what is behind it: alpha (mix), add (glow), multiply (darken) |
| **bloom** | a glow that spreads from very bright parts of the picture |
| **cascade** | one of the slices a directional light's shadow is split into, so near shadows stay sharp |
| **cube map** | six pictures making a box, for reflections and skies |
| **deferred rendering** | drawing all surfaces into buffers first and lighting them afterwards; how PBR materials get shadows from every light |
| **directive** | a line starting with `//!` in a NeptuneBASIC shader that sets an option, like `//! blend alpha` |
| **emission** | light a surface gives off by itself; it is not affected by lights and can make bloom |
| **fragment** | a pixel of a triangle being drawn; a fragment shader colours it |
| **fresnel** | surfaces reflect more at glancing angles (look across a lake and it is a mirror; look down and you see into it) |
| **forward rendering** | drawing and lighting each surface in one go; needed for see-through surfaces and custom lighting |
| **G-buffer** | the set of buffers deferred rendering draws surfaces into |
| **GLSL** | the OpenGL Shading Language, the language NeptuneBASIC shaders are written in |
| **GPU** | the graphics card, which runs shaders |
| **HDR** | high dynamic range: the scene is drawn with brightness above "white", then **tone mapped** down to what a screen can show |
| **hot reload** | recompiling a shader automatically when you save its file |
| **keyword** | a switch (`ShaderSetKeyword`) that turns parts of a shader on or off |
| **material** | a shader together with its parameter values |
| **metallic** | whether a surface is metal (1) or not (0); metals tint their reflections with their own colour |
| **noise** | smooth random patterns, used for clouds, marble, rock, fire |
| **normal** | the direction a surface faces, at each point |
| **normal map** | a texture that bends the normals, to fake small bumps without extra triangles |
| **parallax mapping** | shifting a texture by a height map so flat surfaces look deep |
| **parameter** | a setting of a shader or material, like its colour or speed |
| **pass** | one drawing of an object for one purpose: into the G-buffer, with forward lighting, into a shadow map |
| **PBR** | physically based rendering: lighting that follows how real light behaves, controlled mainly by colour, metallic and roughness |
| **post-processing** | effects applied to the finished picture |
| **procedural** | made by a formula instead of a picture, so it never runs out of detail and has no seams |
| **roughness** | how rough a surface is: 0 mirror-smooth with sharp reflections, 1 matte |
| **sampler** | a texture parameter in a shader (`sampler2D`, `samplerCube`) |
| **shader** | a program the GPU runs to draw vertices or pixels |
| **shadow acne** | speckled or striped false shadows on lit surfaces, fixed with shadow bias |
| **shadow map** | a picture of the scene from the light's point of view, used to work out shadows |
| **sRGB** | the colour scale of pictures and screens; shaders convert to linear colour to do lighting maths |
| **stage** | one of the programs in a shader: vertex or fragment |
| **tone mapping** | turning HDR brightness into colours a screen can show |
| **toon shading** | cartoon lighting in a few flat bands instead of smooth shading |
| **uniform** | a shader parameter, the same for every vertex and pixel of a draw |
| **UV** | the 2D position on a texture that a point on a surface uses (u across, v down) |
| **vertex** | a corner of a triangle |
| **WGSL** | the WebGPU Shading Language, which NeptuneBASIC translates to GLSL |

[Contents](../SHADERS.md) · Next: [2. First steps](02-first-steps.md)
