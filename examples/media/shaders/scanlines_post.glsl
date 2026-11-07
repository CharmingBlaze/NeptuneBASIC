// A custom post-processing shader (LoadPostProcess makes it a post pass).
// nb_post() gets the screen position and returns the new colour;
// nb_screenTexture holds the image so far.
uniform float strength = 0.25;
uniform float speed = 40.0;
//! fragment
vec4 nb_post(vec2 uv) {
    vec3 c = texture(nb_screenTexture, uv).rgb;
    float line = 0.5 + 0.5 * sin(uv.y * nb_resolution.y * 1.5 + nb_time * speed);
    c *= 1.0 - strength * line;
    // a green phosphor tint
    c *= vec3(0.85, 1.05, 0.85);
    return vec4(c, 1.0);
}
