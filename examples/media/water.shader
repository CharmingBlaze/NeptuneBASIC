// Water for NeptuneBASIC (Fyrox shader format: GLSL inside a RON
// description). Used by examples/beach.nb and examples/island.nb:
//   water = LoadShader("examples/media/water.shader")
//   EntityShader sea, water
//   ShaderFloat sea, "time", MilliSecs() / 1000.0     ; every frame
// Every vertex carries VertexData slot 0 = (depth of the water there in
// metres, 0, 0, 0), read at location 8: shallow water shows the sea bed
// (bedColor) through clear turquoise with caustics, deep water is dark
// blue, and foam rolls in where it is shallowest. The water is opaque and
// draws the bed itself, so it looks the same whatever is drawn first.
// Colours are sRGB 0-1 (ShaderVector); directions are Fyrox world space
// (X mirrored from Blitz space).
(
    name: "NeptuneWater",

    resources: [
        (
            name: "properties",
            kind: PropertyGroup([
                (name: "time", kind: Float(value: 0.0)),
                (name: "shallowColor", kind: Vector4(value: (0.25, 0.85, 0.82, 1.0))),
                (name: "deepColor", kind: Vector4(value: (0.06, 0.28, 0.55, 1.0))),
                (name: "skyColor", kind: Vector4(value: (0.72, 0.84, 0.95, 1.0))),
                (name: "bedColor", kind: Vector4(value: (0.86, 0.80, 0.62, 1.0))),
                (name: "sunColor", kind: Vector4(value: (1.0, 0.96, 0.88, 1.0))),
                // direction towards the sun
                (name: "sunDir", kind: Vector4(value: (0.4, 0.6, 0.5, 0.0))),
                (name: "fogColor", kind: Vector4(value: (0.77, 0.85, 0.92, 1.0))),
                // fog start and end distances, 0 0 for none
                (name: "fogRange", kind: Vector4(value: (300.0, 1400.0, 0.0, 0.0))),
                // depth where the water is fully deep, foam width (metres),
                // wave strength, swell height
                (name: "shape", kind: Vector4(value: (6.0, 1.2, 1.0, 0.12))),
                // depth (metres) down to which the bed can be seen
                (name: "clarity", kind: Vector4(value: (4.0, 0.0, 0.0, 0.0))),
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
                blend: None,
                stencil_op: StencilOp(fail: Keep, zfail: Keep, zpass: Keep, write_mask: 0xFFFF_FFFF),
                scissor_box: None
            ),
            vertex_shader: r#"
                layout(location = 0) in vec3 vertexPosition;
                layout(location = 8) in vec4 waterData;

                out vec3 worldPos;
                out float depth;

                void main() {
                    vec4 world = fyrox_instanceData.worldMatrix * vec4(vertexPosition, 1.0);
                    // a slow swell, flattening out where the water is shallow
                    float t = properties.time;
                    float swell = sin(world.x * 0.045 + t * 0.9) * 0.6 + sin(world.z * 0.06 - t * 0.7) * 0.4;
                    vec3 p = vertexPosition;
                    p.y += swell * properties.shape.w * clamp(waterData.x / 3.0, 0.0, 1.0);
                    worldPos = (fyrox_instanceData.worldMatrix * vec4(p, 1.0)).xyz;
                    depth = waterData.x;
                    gl_Position = fyrox_instanceData.worldViewProjection * vec4(p, 1.0);
                }
            "#,
            fragment_shader: r#"
                in vec3 worldPos;
                in float depth;
                out vec4 FragColor;

                float hash(vec2 p) {
                    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
                }

                float noise(vec2 p) {
                    vec2 i = floor(p);
                    vec2 f = fract(p);
                    f = f * f * (3.0 - 2.0 * f);
                    return mix(mix(hash(i), hash(i + vec2(1.0, 0.0)), f.x),
                               mix(hash(i + vec2(0.0, 1.0)), hash(i + vec2(1.0, 1.0)), f.x), f.y);
                }

                // the ripples' height at p
                float ripples(vec2 p, float t) {
                    float h = sin(dot(p, vec2(0.21, 0.08)) + t * 1.3) * 0.30;
                    h += sin(dot(p, vec2(-0.11, 0.26)) + t * 1.7) * 0.22;
                    h += sin(dot(p, vec2(0.55, -0.37)) + t * 2.6) * 0.07;
                    h += (noise(p * 0.9 + vec2(t * 0.5, t * 0.35)) - 0.5) * 0.30;
                    h += (noise(p * 2.3 - vec2(t * 0.8, t * 0.25)) - 0.5) * 0.14;
                    return h;
                }

                void main() {
                    float t = properties.time;
                    vec2 p = worldPos.xz;
                    vec3 toEye = fyrox_cameraData.position - worldPos;
                    float dist = length(toEye);
                    vec3 v = toEye / max(dist, 1e-4);

                    // normal from the ripples, calmer far away (less shimmer)
                    float e = 0.12;
                    float h0 = ripples(p, t);
                    float k = properties.shape.z / (1.0 + dist * 0.02);
                    vec3 n = normalize(vec3(-(ripples(p + vec2(e, 0.0), t) - h0) / e * k, 1.0,
                                            -(ripples(p + vec2(0.0, e), t) - h0) / e * k));

                    float d = max(depth, 0.0);
                    float deepness = smoothstep(0.0, properties.shape.x, d);
                    vec3 shallow = S_SRGBToLinear(properties.shallowColor).rgb;
                    vec3 deep = S_SRGBToLinear(properties.deepColor).rgb;
                    vec3 sun = S_SRGBToLinear(properties.sunColor).rgb;
                    vec3 bed = S_SRGBToLinear(properties.bedColor).rgb;

                    // the bed, seen through the water (shifted a little by the ripples)
                    vec2 cp = p * 0.9 + n.xz * 0.6;
                    float c1 = noise(cp + vec2(t * 0.35, t * 0.2));
                    float c2 = noise(cp * 1.6 - vec2(t * 0.3, -t * 0.25));
                    float caustic = pow(1.0 - abs(c1 - c2), 10.0);
                    float see = 1.0 - smoothstep(0.0, properties.clarity.x, d);
                    vec3 under = bed * mix(vec3(1.0), shallow * 1.25, smoothstep(0.0, 1.5, d)) + sun * caustic * 0.5;
                    vec3 water = mix(mix(shallow, deep, deepness), under, see * see);

                    // the sky reflected, more at low angles
                    float fresnel = 0.03 + 0.97 * pow(1.0 - max(dot(n, v), 0.0), 5.0);
                    vec3 sky = S_SRGBToLinear(properties.skyColor).rgb;
                    vec3 color = mix(water, sky, fresnel * 0.8);

                    // sun glints
                    vec3 l = normalize(properties.sunDir.xyz);
                    float spec = pow(max(dot(reflect(-l, n), v), 0.0), 400.0);
                    color += sun * spec * 5.0;

                    // foam rolling in where the water is shallowest
                    float edge = 1.0 - smoothstep(0.0, properties.shape.y, d);
                    float rolls = 0.5 + 0.5 * sin(d * 7.0 - t * 2.2 + noise(p * 0.5) * 5.0);
                    float bubbles = noise(p * 4.0 + vec2(t * 0.4, 0.0));
                    float foam = edge * smoothstep(0.55, 0.9, rolls * 0.55 + bubbles * 0.55) + pow(edge, 3.0) * 0.5;
                    // right at the waterline the water thins out to wet sand
                    color = mix(color, vec3(1.0), clamp(foam, 0.0, 1.0));
                    color = mix(bed * 0.85, color, smoothstep(0.0, 0.08, d));

                    if (properties.fogRange.y > properties.fogRange.x) {
                        float f = clamp((dist - properties.fogRange.x) / (properties.fogRange.y - properties.fogRange.x), 0.0, 1.0);
                        color = mix(color, S_SRGBToLinear(properties.fogColor).rgb, f);
                    }
                    FragColor = vec4(color, 1.0);
                }
            "#,
        )
    ],
)
