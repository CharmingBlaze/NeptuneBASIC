// A custom NeptuneBASIC GLSL shader (examples/shaders.nb loads it with
// hot reloading on: edit and save this file while the program runs).
//
// Directives choose the pass and lighting model; uniforms become material
// parameters (ShaderSetFloat shader, "bands", 4). With no vertex stage the
// standard vertex program is used, and nb_surface() describes the surface;
// the toon lighting model lights it with every visible light.
//! lighting toon
//! shadows on
#include <neptune/noise.glsl>
uniform vec4 baseColor = vec4(0.95, 0.45, 0.75, 1.0);
uniform float bands = 3.0;
uniform vec3 rimColor = vec3(1.0, 0.9, 0.4);
uniform float rimPower = 2.5;
//! fragment
void nb_surface(inout NbSurface s) {
    // a little noise so the bands are not perfectly smooth
    float n = nb_valueNoise(nb_uv * 40.0) * 0.06;
    s.albedo = baseColor.rgb + n;
    s.toonSteps = bands;
    s.specularColor = vec3(1.0);
    s.shininess = 64.0;
    s.emission = nb_linear(rimColor) * smoothstep(0.55, 0.6, nb_fresnel(s.normal, rimPower)) * 0.8;
}
