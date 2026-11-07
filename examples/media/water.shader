// Water for NeptuneBASIC (Fyrox shader format: GLSL inside a RON
// description). Used by examples/beach.nb and examples/island.nb:
//   water = LoadShader("examples/media/water.shader")
//   EntityShader sea, water
//   ShaderFloat sea, "time", MilliSecs() / 1000.0     ; every frame
//
// The water is see-through: it reads the depth of the ground behind it
// (Fyrox's scene depth), works out how far the light travels through the
// water and absorbs red first, then green, so the real sea bed shows
// through turquoise in the shallows and fades into deep blue. Over that go
// the sky's reflection, sun glints, caustics on the bed, light through the
// wave crests, foam lace at the shore and breaking lines rolling in.
// The ground must be drawn in the deferred pass (standard or PBR materials,
// or builtin:terrain) for the water to see it.
//
// Every vertex carries VertexData slot 0 = (depth of the water there in
// metres, 0, 0, 0), read at location 8: the swell dies out and the surge
// runs up the beach where the water is shallow.
// Colours are sRGB 0-1 (ShaderVector); directions are Fyrox world space
// (X mirrored from Blitz space).
(
    name: "NeptuneWater",

    resources: [
        (name: "fyrox_sceneDepth", kind: Texture(kind: Sampler2D, fallback: White), binding: 0),
        (
            name: "properties",
            kind: PropertyGroup([
                (name: "time", kind: Float(value: 0.0)),
                // light scattered back by shallow and by deep water
                (name: "shallowColor", kind: Vector4(value: (0.16, 0.74, 0.72, 1.0))),
                (name: "deepColor", kind: Vector4(value: (0.03, 0.18, 0.40, 1.0))),
                // reflected sky: at the horizon and overhead
                (name: "skyColor", kind: Vector4(value: (0.74, 0.85, 0.95, 1.0))),
                (name: "skyTopColor", kind: Vector4(value: (0.38, 0.62, 0.88, 1.0))),
                // the colour of the sea bed (sand), for the tint and caustics
                (name: "bedColor", kind: Vector4(value: (0.86, 0.80, 0.62, 1.0))),
                (name: "sunColor", kind: Vector4(value: (1.0, 0.96, 0.88, 1.0))),
                // direction towards the sun
                (name: "sunDir", kind: Vector4(value: (0.4, 0.6, 0.5, 0.0))),
                (name: "fogColor", kind: Vector4(value: (0.77, 0.85, 0.92, 1.0))),
                // fog start and end distances, 0 0 for none
                (name: "fogRange", kind: Vector4(value: (0.0, 0.0, 0.0, 0.0))),
                // depth (metres) where the water looks fully deep, foam width
                // (metres of depth), wave strength, swell height
                (name: "shape", kind: Vector4(value: (6.0, 1.2, 1.0, 0.15))),
                // how far you can see through the water (metres), surge height,
                // seconds between surges, ripple size
                (name: "clarity", kind: Vector4(value: (5.0, 0.18, 7.0, 1.0))),
            ]),
            binding: 0
        ),
        (name: "fyrox_instanceData", kind: PropertyGroup([]), binding: 1),
        (name: "fyrox_cameraData", kind: PropertyGroup([]), binding: 2),
    ],

    disabled_passes: ["GBuffer", "DirectionalShadow", "PointShadow", "SpotShadow"],

    passes: [
        (
            name: "Forward",
            draw_parameters: DrawParameters(
                cull_face: None,
                color_write: ColorMask(red: true, green: true, blue: true, alpha: true),
                depth_write: true,
                stencil_test: None,
                depth_test: Some(Less),
                // premultiplied: the shader adds its light and says how much
                // of the ground behind still shows
                blend: Some(BlendParameters(
                    func: BlendFunc(
                        sfactor: One,
                        dfactor: OneMinusSrcAlpha,
                        alpha_sfactor: One,
                        alpha_dfactor: OneMinusSrcAlpha,
                    ),
                    equation: BlendEquation(rgb: Add, alpha: Add)
                )),
                stencil_op: StencilOp(fail: Keep, zfail: Keep, zpass: Keep, write_mask: 0xFFFF_FFFF),
                scissor_box: None
            ),
            vertex_shader: r#"
                layout(location = 0) in vec3 vertexPosition;
                layout(location = 8) in vec4 waterData;

                out vec3 worldPos;
                out float rise;

                void main() {
                    vec4 world = fyrox_instanceData.worldMatrix * vec4(vertexPosition, 1.0);
                    float t = properties.time;
                    float d = waterData.x;
                    // a slow swell from three directions, dying out in the shallows
                    float swell = sin(dot(world.xz, vec2(0.040, 0.012)) + t * 0.85) * 0.55
                                + sin(dot(world.xz, vec2(-0.018, 0.052)) - t * 0.70) * 0.35
                                + sin(dot(world.xz, vec2(0.070, -0.050)) + t * 1.30) * 0.10;
                    // the surge: near the shore the water runs up the beach and
                    // draws back, a little out of step along the coast
                    float phase = t * 6.2831853 / max(properties.clarity.z, 0.1)
                                + sin(world.x * 0.031) * 1.7 + sin(world.z * 0.027) * 1.7;
                    float run = 0.5 + 0.5 * sin(phase);
                    run = run * run * (3.0 - 2.0 * run);
                    float shore = 1.0 - smoothstep(0.5, 4.0, d);
                    rise = run * properties.clarity.y * shore;
                    vec3 p = vertexPosition;
                    p.y += swell * properties.shape.w * clamp(d / 3.0, 0.0, 1.0) + rise;
                    worldPos = (fyrox_instanceData.worldMatrix * vec4(p, 1.0)).xyz;
                    gl_Position = fyrox_instanceData.worldViewProjection * vec4(p, 1.0);
                }
            "#,
            fragment_shader: r#"
                in vec3 worldPos;
                in float rise;
                out vec4 FragColor;

                float hash(vec2 p) {
                    vec3 p3 = fract(vec3(p.xyx) * 0.1031);
                    p3 += dot(p3, p3.yzx + 33.33);
                    return fract((p3.x + p3.y) * p3.z);
                }

                float noise(vec2 p) {
                    vec2 i = floor(p);
                    vec2 f = fract(p);
                    f = f * f * (3.0 - 2.0 * f);
                    return mix(mix(hash(i), hash(i + vec2(1.0, 0.0)), f.x),
                               mix(hash(i + vec2(0.0, 1.0)), hash(i + vec2(1.0, 1.0)), f.x), f.y);
                }

                // the surface's height at p: wave trains from four directions and
                // drifting noise; the small waves fade out far away
                float waves(vec2 p, float t, float near) {
                    float h = sin(dot(p, vec2(0.23, 0.07)) + t * 1.25) * 0.28;
                    h += sin(dot(p, vec2(-0.10, 0.27)) + t * 1.60) * 0.20;
                    h += sin(dot(p, vec2(0.41, -0.33)) + t * 2.30) * 0.09 * (0.4 + 0.6 * near);
                    h += sin(dot(p, vec2(-0.62, -0.21)) + t * 2.90) * 0.05 * near;
                    h += (noise(p * 0.7 + vec2(t * 0.45, t * 0.30)) - 0.5) * 0.34;
                    h += (noise(p * 1.9 - vec2(t * 0.70, -t * 0.20)) - 0.5) * 0.16 * (0.3 + 0.7 * near);
                    h += (noise(p * 4.3 + vec2(-t * 1.10, t * 0.90)) - 0.5) * 0.07 * near;
                    return h;
                }

                float linearDepth(float d) {
                    float zn = fyrox_cameraData.zNear;
                    float zf = fyrox_cameraData.zFar;
                    return (2.0 * zn * zf) / (zf + zn - (d * 2.0 - 1.0) * (zf - zn));
                }

                vec3 lin(vec4 c) {
                    return S_SRGBToLinear(c).rgb;
                }

                void main() {
                    float t = properties.time;
                    vec3 toEye = fyrox_cameraData.position - worldPos;
                    float dist = length(toEye);
                    vec3 v = toEye / max(dist, 1e-4);
                    float near = 1.0 - smoothstep(25.0, 240.0, dist);

                    // the surface normal from the waves, calmer far away
                    vec2 p = worldPos.xz / max(properties.clarity.w, 0.01);
                    float e = 0.1;
                    float h0 = waves(p, t, near);
                    float k = properties.shape.z / (1.0 + dist * 0.012);
                    vec3 n = normalize(vec3(-(waves(p + vec2(e, 0.0), t, near) - h0) / e * k, 1.0,
                                            -(waves(p + vec2(0.0, e), t, near) - h0) / e * k));

                    // how far the view travels through the water to the ground
                    // behind it, and how deep that ground is
                    vec2 suv = gl_FragCoord.xy / vec2(textureSize(fyrox_sceneDepth, 0));
                    float raw = texture(fyrox_sceneDepth, suv).r;
                    float here = linearDepth(gl_FragCoord.z);
                    float along = raw >= 1.0 ? 1.0e4 : max(linearDepth(raw) - here, 0.0) * dist / max(here, 1e-3);
                    float bedDepth = along * max(v.y, 0.0);

                    vec3 shallow = lin(properties.shallowColor);
                    vec3 deep = lin(properties.deepColor);
                    vec3 sun = lin(properties.sunColor);
                    vec3 bed = lin(properties.bedColor);
                    vec3 l = normalize(properties.sunDir.xyz);

                    // light absorbed on the way: red first, then green, then blue
                    vec3 absorb = vec3(2.4, 0.62, 0.42) / max(properties.clarity.x, 0.05);
                    vec3 T = exp(-absorb * along);
                    vec3 scatter = mix(shallow, deep, smoothstep(0.0, properties.shape.x, bedDepth));
                    // caustics: light focused by the waves dancing on the bed
                    vec3 bedPos = worldPos - v * min(along, 50.0);
                    vec2 cp = bedPos.xz * 0.8 + n.xz * 0.8;
                    float c1 = noise(cp + vec2(t * 0.35, t * 0.20));
                    float c2 = noise(cp * 1.7 - vec2(t * 0.30, -t * 0.25));
                    float caustic = pow(1.0 - abs(c1 - c2), 9.0) * (1.0 - smoothstep(0.5, 5.0, bedDepth)) * near;
                    // what is added on top of the ground, and how much ground shows
                    // (assuming the ground is about bedColor where it tints)
                    vec3 color = scatter * (1.0 - T) + bed * (T - vec3(T.r)) + bed * sun * caustic * 0.55 * T;
                    float through = T.r;

                    // the sky reflected, more at low angles
                    vec3 r = reflect(-v, n);
                    r.y = abs(r.y);
                    vec3 sky = mix(lin(properties.skyColor), lin(properties.skyTopColor), smoothstep(0.0, 0.55, r.y));
                    float fresnel = 0.02 + 0.98 * pow(1.0 - max(dot(n, v), 0.0), 5.0);
                    color = color * (1.0 - fresnel) + sky * fresnel;
                    through *= 1.0 - fresnel;

                    // sun glints and a softer sheen around them
                    float sr = max(dot(r, l), 0.0);
                    color += sun * (pow(sr, 700.0) * 7.0 + pow(sr, 60.0) * 0.18);
                    // light shining through the wave crests towards the viewer
                    float back = pow(max(dot(-v, l) * 0.5 + 0.5, 0.0), 3.0);
                    color += shallow * sun * back * max(h0, 0.0) * 0.35 * (1.0 - through);

                    // foam: lace near the shore, lines of breaking waves rolling in,
                    // and the edge of the surge as it runs up the sand
                    float fw = max(properties.shape.y, 1e-3);
                    float edge = 1.0 - smoothstep(0.0, fw, bedDepth);
                    float l1 = noise(p * 0.9 + vec2(t * 0.25, 0.0));
                    float l2 = noise(p * 1.6 - vec2(0.0, t * 0.2));
                    float lace = smoothstep(0.55, 0.9, pow(1.0 - abs(l1 - l2), 4.0) + noise(p * 3.1) * 0.35);
                    float rolls = fract(bedDepth * 0.9 + t * 0.22 + noise(p * 0.12) * 1.3);
                    float lines = smoothstep(0.82, 0.96, rolls) * (1.0 - smoothstep(0.0, fw * 2.5, bedDepth));
                    float runEdge = smoothstep(0.02, 0.12, rise) * (1.0 - smoothstep(0.0, fw * 0.35, bedDepth));
                    float foam = edge * edge * 0.45 + edge * lace * 0.8 + lines * (0.45 + 0.55 * lace) + runEdge * lace;
                    foam = clamp(foam, 0.0, 1.0) * step(1e-3, properties.shape.y);
                    vec3 foamColor = sun * (0.82 + 0.18 * max(dot(n, l), 0.0)) + sky * 0.15;
                    color = color * (1.0 - foam) + foamColor * foam;
                    through *= 1.0 - foam;

                    if (properties.fogRange.y > properties.fogRange.x) {
                        float f = clamp((dist - properties.fogRange.x) / (properties.fogRange.y - properties.fogRange.x), 0.0, 1.0);
                        color = color * (1.0 - f) + lin(properties.fogColor) * f;
                        through *= 1.0 - f;
                    }
                    FragColor = vec4(color, 1.0 - through);
                }
            "#,
        )
    ],
)
